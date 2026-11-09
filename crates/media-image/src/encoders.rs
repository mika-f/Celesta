//! The PNG, JPEG, and WebP video encoders: each frame becomes a whole image
//! file, one key packet. They follow their format's colour convention, not
//! the session's [`celesta_media::Color`]: PNG and WebP keep sRGB RGB(A),
//! JPEG is JFIF (full-range BT.601, as the `image` crate writes it).

use crate::{ImageComponents, JPEG, PNG, WEBP};
use celesta_media::settings::validate;
use celesta_media::{
    Encoded, InvalidSetting, MediaError, Packet, PixelFormat, SelectOption, SessionId,
    SettingField, SettingKind, SettingValue, Settings, TrackConfig, VideoEncoder,
    VideoEncoderOpened, VideoEncoderRequest,
};
use image::codecs::jpeg::JpegEncoder;
use image::{ExtendedColorType, ImageEncoder};
use std::collections::BTreeMap;

/// WebP stores each side in 14 bits.
const MAX_WEBP_SIDE: u32 = 16_383;

/// The quality JPEG and WebP use by default.
const DEFAULT_QUALITY: f64 = 90.0;

/// The quality `webp::Encoder::encode_lossless` passes, which sets how hard
/// lossless WebP compresses.
const LOSSLESS_EFFORT: f32 = 75.0;

/// Every format `encoder` can take under any settings, best first.
pub(crate) fn input_formats(encoder: &str) -> &'static [PixelFormat] {
    match encoder {
        PNG => &[PixelFormat::Rgba16, PixelFormat::Rgba8],
        _ => &[PixelFormat::Rgba8],
    }
}

fn option(value: &str, label: &str) -> SelectOption {
    SelectOption {
        value: value.to_owned(),
        label: label.to_owned(),
    }
}

fn field(
    key: &str,
    label: &str,
    default: SettingValue,
    kind: SettingKind,
    description: Option<&str>,
) -> SettingField {
    SettingField {
        key: key.to_owned(),
        label: label.to_owned(),
        default,
        kind,
        description: description.map(str::to_owned),
        group: None,
        advanced: false,
        visible_when: BTreeMap::new(),
    }
}

fn quality() -> SettingField {
    field(
        "quality",
        "Quality",
        SettingValue::Number(DEFAULT_QUALITY),
        SettingKind::Number {
            min: Some(1.0),
            max: Some(100.0),
            step: Some(1.0),
            integer: true,
            unit: None,
        },
        Some("Higher is sharper and larger."),
    )
}

fn transparent() -> SettingField {
    field(
        "transparent",
        "Transparent background",
        SettingValue::Boolean(false),
        SettingKind::Boolean,
        Some("Leaves the background transparent instead of filling it."),
    )
}

/// `encoder`'s settings schema; none for an id that is not one.
pub(crate) fn settings(encoder: &str) -> Vec<SettingField> {
    match encoder {
        PNG => vec![
            field(
                "compression",
                "Compression",
                SettingValue::String("balanced".to_owned()),
                SettingKind::Select {
                    options: vec![
                        option("none", "None"),
                        option("fastest", "Fastest"),
                        option("fast", "Fast"),
                        option("balanced", "Balanced"),
                        option("high", "High"),
                    ],
                },
                Some("Every level is lossless; smaller files take longer to write."),
            ),
            field(
                "depth",
                "Bit depth",
                SettingValue::String("8".to_owned()),
                SettingKind::Select {
                    options: vec![
                        option("8", "8 bits per channel"),
                        option("16", "16 bits per channel"),
                    ],
                },
                None,
            ),
            transparent(),
        ],
        JPEG => vec![quality()],
        WEBP => {
            let mut quality = quality();
            quality.visible_when =
                [("lossless".to_owned(), vec![SettingValue::Boolean(false)])].into();
            vec![
                field(
                    "lossless",
                    "Lossless",
                    SettingValue::Boolean(false),
                    SettingKind::Boolean,
                    None,
                ),
                quality,
                transparent(),
            ]
        }
        _ => Vec::new(),
    }
}

/// How a session writes its frames.
#[derive(Clone, Copy, Debug)]
enum Format {
    /// `None` keeps the `png` crate's default (`Balanced`) without calling
    /// `set_compression`, as the still export always wrote.
    Png {
        compression: Option<png::Compression>,
        sixteen: bool,
    },
    Jpeg {
        quality: u8,
    },
    Webp {
        lossless: bool,
        quality: f32,
    },
}

/// An open encoder.
pub(crate) struct Session {
    format: Format,
    width: u32,
    height: u32,
    input: PixelFormat,
    /// A frame's duration in the session's timescale.
    duration: i64,
    /// Whether the track's config has not been answered yet.
    config_due: bool,
}

fn invalid_setting(key: &str, message: impl Into<String>) -> MediaError {
    MediaError::InvalidSettings {
        key: key.to_owned(),
        message: message.into(),
    }
}

fn text<'a>(settings: &'a Settings, key: &str) -> &'a str {
    match settings.get(key) {
        Some(SettingValue::String(text)) => text,
        _ => "",
    }
}

fn boolean(settings: &Settings, key: &str) -> bool {
    settings.get(key) == Some(&SettingValue::Boolean(true))
}

/// The validated `quality`, 1 to 100.
fn quality_of(settings: &Settings) -> u8 {
    match settings.get("quality") {
        Some(SettingValue::Number(quality)) => *quality as u8,
        _ => DEFAULT_QUALITY as u8,
    }
}

/// Refuses an encoder for 8-bit input when `rgba8` is not offered.
fn needs_rgba8(name: &str, offered: &[PixelFormat]) -> Result<PixelFormat, MediaError> {
    if offered.contains(&PixelFormat::Rgba8) {
        Ok(PixelFormat::Rgba8)
    } else {
        Err(invalid_setting(
            "input",
            format!("{name} needs rgba8 input, which this export does not offer"),
        ))
    }
}

/// The format, input, and alpha `request` opens, or why it cannot.
fn open(request: &VideoEncoderRequest<'_>) -> Result<(Format, PixelFormat, bool), MediaError> {
    let component = request.component;
    if ![PNG, JPEG, WEBP].contains(&component) {
        return Err(MediaError::Codec(format!(
            "no video encoder {component:?} in builtin"
        )));
    }
    let settings = validate(&settings(component), request.settings)
        .map_err(|InvalidSetting { key, message }| MediaError::InvalidSettings { key, message })?;
    let offered = request.offered;
    match component {
        PNG => {
            let compression = match text(&settings, "compression") {
                "none" => Some(png::Compression::NoCompression),
                "fastest" => Some(png::Compression::Fastest),
                "fast" => Some(png::Compression::Fast),
                "high" => Some(png::Compression::High),
                _ => None,
            };
            let sixteen = text(&settings, "depth") == "16";
            let input = if sixteen {
                if !offered.contains(&PixelFormat::Rgba16) {
                    return Err(invalid_setting(
                        "depth",
                        "16-bit PNG needs rgba16 input, which this export does not offer",
                    ));
                }
                PixelFormat::Rgba16
            } else {
                needs_rgba8("8-bit PNG", offered)?
            };
            let format = Format::Png {
                compression,
                sixteen,
            };
            Ok((format, input, boolean(&settings, "transparent")))
        }
        JPEG => {
            let input = needs_rgba8("JPEG", offered)?;
            let quality = quality_of(&settings);
            Ok((Format::Jpeg { quality }, input, false))
        }
        _ => {
            // WEBP, the only other.
            let input = needs_rgba8("WebP", offered)?;
            let (width, height) = (request.width, request.height);
            if width > MAX_WEBP_SIDE || height > MAX_WEBP_SIDE {
                return Err(invalid_setting(
                    "size",
                    format!(
                        "a {width}x{height} image is too large for WebP, which holds at most \
                         {MAX_WEBP_SIDE} pixels on each side (write PNG or JPEG, or make the \
                         contact sheet smaller)"
                    ),
                ));
            }
            let lossless = boolean(&settings, "lossless");
            let quality = f32::from(quality_of(&settings));
            let format = Format::Webp { lossless, quality };
            Ok((format, input, boolean(&settings, "transparent")))
        }
    }
}

fn failed(format: &str, error: impl std::fmt::Display) -> MediaError {
    MediaError::Codec(format!("could not encode {format}: {error}"))
}

impl Session {
    /// `frame`, in the session's input format, as a whole image file.
    fn encode(&self, mut frame: Vec<u8>) -> Result<Vec<u8>, MediaError> {
        let (width, height) = (self.width, self.height);
        let expected = self.input.frame_bytes(width, height);
        if frame.len() != expected {
            return Err(MediaError::Codec(format!(
                "a {width}x{height} {} frame has {expected} bytes, not {}",
                self.input,
                frame.len()
            )));
        }
        match self.format {
            Format::Png {
                compression,
                sixteen,
            } => {
                let mut file = Vec::new();
                let mut encoder = png::Encoder::new(&mut file, width, height);
                encoder.set_color(png::ColorType::Rgba);
                if sixteen {
                    encoder.set_depth(png::BitDepth::Sixteen);
                    // PNG stores samples big-endian; rgba16 is little-endian.
                    for sample in frame.as_chunks_mut::<2>().0 {
                        sample.swap(0, 1);
                    }
                } else {
                    encoder.set_depth(png::BitDepth::Eight);
                }
                if let Some(compression) = compression {
                    encoder.set_compression(compression);
                }
                let mut writer = encoder
                    .write_header()
                    .map_err(|error| failed("PNG", error))?;
                writer
                    .write_image_data(&frame)
                    .map_err(|error| failed("PNG", error))?;
                writer.finish().map_err(|error| failed("PNG", error))?;
                Ok(file)
            }
            Format::Jpeg { quality } => {
                // An encoder answering no alpha gets opaque frames, so
                // dropping alpha is what flattening them would give.
                let rgb: Vec<u8> = frame
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .flat_map(|&[red, green, blue, _]| [red, green, blue])
                    .collect();
                let mut file = Vec::new();
                JpegEncoder::new_with_quality(&mut file, quality)
                    .write_image(&rgb, width, height, ExtendedColorType::Rgb8)
                    .map_err(|error| failed("JPEG", error))?;
                Ok(file)
            }
            Format::Webp { lossless, quality } => {
                let encoder = webp::Encoder::from_rgba(&frame, width, height);
                // `encode_lossless` is `encode_simple(true, 75.0)`, unwrapped.
                let encoded = if lossless {
                    encoder.encode_simple(true, LOSSLESS_EFFORT)
                } else {
                    encoder.encode_simple(false, quality)
                };
                let encoded = encoded.map_err(|error| failed("WebP", format!("{error:?}")))?;
                Ok(encoded.to_vec())
            }
        }
    }

    fn codec(&self) -> &'static str {
        match self.format {
            Format::Png { .. } => PNG,
            Format::Jpeg { .. } => JPEG,
            Format::Webp { .. } => WEBP,
        }
    }
}

fn no_session(session: SessionId) -> MediaError {
    MediaError::Codec(format!("no open video encoder session {}", session.0))
}

impl VideoEncoder for ImageComponents {
    fn open_video(
        &mut self,
        request: &VideoEncoderRequest<'_>,
    ) -> Result<VideoEncoderOpened, MediaError> {
        let (format, input, alpha) = open(request)?;
        let session = self.next_session();
        self.encoders.insert(
            session,
            Session {
                format,
                width: request.width,
                height: request.height,
                input,
                duration: i64::from(request.frame_rate.denominator),
                config_due: true,
            },
        );
        Ok(VideoEncoderOpened {
            session,
            input,
            alpha,
        })
    }

    fn encode_video(
        &mut self,
        session: SessionId,
        pts: i64,
        frame: Vec<u8>,
    ) -> Result<Encoded, MediaError> {
        let open = self
            .encoders
            .get_mut(&session)
            .ok_or_else(|| no_session(session))?;
        let bytes = open.encode(frame)?;
        let config = std::mem::take(&mut open.config_due).then(|| TrackConfig {
            codec: open.codec().to_owned(),
            description: None,
        });
        Ok(Encoded {
            packets: vec![Packet {
                pts,
                dts: pts,
                duration: open.duration,
                key: true,
                bytes,
                additional: Vec::new(),
            }],
            config,
        })
    }

    fn finish_video(&mut self, session: SessionId) -> Result<Encoded, MediaError> {
        self.encoders
            .remove(&session)
            .map(|_| Encoded::default())
            .ok_or_else(|| no_session(session))
    }

    fn abort_video(&mut self, session: SessionId) {
        self.encoders.remove(&session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use celesta_composition::Rational;
    use celesta_media::Color;
    use std::io::{self, Cursor, Write};

    const WIDTH: u32 = 7;
    const HEIGHT: u32 = 5;
    const BOTH: [PixelFormat; 2] = [PixelFormat::Rgba16, PixelFormat::Rgba8];

    /// A `width`x`height` RGBA8 frame with varied colours and alpha 255.
    fn opaque(width: u32, height: u32) -> Vec<u8> {
        (0..width * height)
            .flat_map(|index| {
                let index = index as u8;
                [
                    index.wrapping_mul(37),
                    index.wrapping_mul(11).wrapping_add(90),
                    255 - index.wrapping_mul(5),
                    255,
                ]
            })
            .collect()
    }

    fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect()
    }

    fn text(value: &str) -> SettingValue {
        SettingValue::String(value.to_owned())
    }

    fn open_sized(
        provider: &mut ImageComponents,
        component: &str,
        settings: &Settings,
        (width, height): (u32, u32),
        offered: &[PixelFormat],
    ) -> Result<VideoEncoderOpened, MediaError> {
        provider.open_video(&VideoEncoderRequest {
            component,
            settings,
            width,
            height,
            frame_rate: Rational::new(30_000, 1_001),
            timescale: 30_000,
            offered,
            color: &Color::bt709_limited(),
        })
    }

    /// `frame` through `component` with `settings`, as one file.
    fn encode(
        component: &str,
        settings: &Settings,
        offered: &[PixelFormat],
        frame: Vec<u8>,
    ) -> (VideoEncoderOpened, Vec<u8>) {
        let mut provider = ImageComponents::default();
        let opened =
            open_sized(&mut provider, component, settings, (WIDTH, HEIGHT), offered).unwrap();
        let encoded = provider.encode_video(opened.session, 0, frame).unwrap();
        let [packet] = &encoded.packets[..] else {
            panic!("{encoded:?}");
        };
        (opened, packet.bytes.clone())
    }

    /// A PNG's info and pixels, as the file stores them.
    fn decode_png(bytes: &[u8]) -> (png::OutputInfo, Vec<u8>) {
        let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        pixels.truncate(info.buffer_size());
        (info, pixels)
    }

    /// A WebP's pixels as RGBA8, an opaque one given alpha 255.
    fn decode_webp(bytes: &[u8]) -> Vec<u8> {
        let image = webp::Decoder::new(bytes).decode().unwrap();
        if image.is_alpha() {
            image.to_vec()
        } else {
            image
                .as_chunks::<3>()
                .0
                .iter()
                .flat_map(|&[red, green, blue]| [red, green, blue, 255])
                .collect()
        }
    }

    #[test]
    fn default_png_is_todays_png() {
        let frame = opaque(WIDTH, HEIGHT);
        let mut provider = ImageComponents::default();
        let opened = open_sized(
            &mut provider,
            "png",
            &Settings::new(),
            (WIDTH, HEIGHT),
            &BOTH,
        )
        .unwrap();
        assert_eq!(opened.input, PixelFormat::Rgba8);
        assert!(!opened.alpha);
        let first = provider
            .encode_video(opened.session, 0, frame.clone())
            .unwrap();
        let second = provider
            .encode_video(opened.session, 1_001, frame.clone())
            .unwrap();

        // Today's `stills.rs` (`Encoder::encode`, PNG).
        let mut today = Vec::new();
        let mut encoder = png::Encoder::new(&mut today, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&frame).unwrap();
        writer.finish().unwrap();

        assert_eq!(
            first,
            Encoded {
                packets: vec![Packet {
                    pts: 0,
                    dts: 0,
                    duration: 1_001,
                    key: true,
                    bytes: today.clone(),
                    additional: Vec::new(),
                }],
                config: Some(TrackConfig {
                    codec: "png".to_owned(),
                    description: None,
                }),
            }
        );
        assert_eq!(second.config, None);
        assert_eq!(second.packets[0].pts, 1_001);
        assert_eq!(second.packets[0].dts, 1_001);
        assert_eq!(second.packets[0].bytes, today);
        assert_eq!(
            provider.finish_video(opened.session).unwrap(),
            Encoded::default()
        );
        assert!(provider.encode_video(opened.session, 0, frame).is_err());
    }

    #[test]
    fn default_jpeg_and_webp_are_todays() {
        let frame = opaque(WIDTH, HEIGHT);
        let rgb: Vec<u8> = frame
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|&[red, green, blue, _]| [red, green, blue])
            .collect();

        // Today's `stills.rs` (`Encoder::encode`, JPEG and WebP).
        let mut jpeg = Vec::new();
        {
            let mut writer = io::BufWriter::new(&mut jpeg);
            JpegEncoder::new_with_quality(&mut writer, 90)
                .write_image(&rgb, WIDTH, HEIGHT, ExtendedColorType::Rgb8)
                .unwrap();
            writer.flush().unwrap();
        }
        let webp = webp::Encoder::from_rgba(&frame, WIDTH, HEIGHT)
            .encode_simple(false, 90.0)
            .unwrap()
            .to_vec();

        let (opened, bytes) = encode("jpeg", &Settings::new(), &BOTH, frame.clone());
        assert_eq!(opened.input, PixelFormat::Rgba8);
        assert_eq!(bytes, jpeg);
        let quality_90 = settings(&[("quality", SettingValue::Number(90.0))]);
        assert_eq!(encode("jpeg", &quality_90, &BOTH, frame.clone()).1, jpeg);

        let (opened, bytes) = encode("webp", &Settings::new(), &BOTH, frame.clone());
        assert_eq!(opened.input, PixelFormat::Rgba8);
        assert_eq!(bytes, webp);
        assert_eq!(encode("webp", &quality_90, &BOTH, frame.clone()).1, webp);

        let quality_40 = settings(&[("quality", SettingValue::Number(40.0))]);
        assert_ne!(encode("jpeg", &quality_40, &BOTH, frame.clone()).1, jpeg);
        assert_ne!(encode("webp", &quality_40, &BOTH, frame).1, webp);
    }

    #[test]
    fn png_takes_sixteen_bits_and_compression() {
        let sixteen = settings(&[("depth", text("16"))]);
        let values: Vec<u16> = (0..WIDTH * HEIGHT * 4)
            .map(|index| (index * 1_871 + 3) as u16)
            .collect();
        let frame: Vec<u8> = values
            .iter()
            .flat_map(|value| value.to_le_bytes())
            .collect();
        let (opened, bytes) = encode("png", &sixteen, &BOTH, frame);
        assert_eq!(opened.input, PixelFormat::Rgba16);
        let (info, pixels) = decode_png(&bytes);
        assert_eq!(info.bit_depth, png::BitDepth::Sixteen);
        assert_eq!(info.color_type, png::ColorType::Rgba);
        let decoded: Vec<u16> = pixels
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&sample| u16::from_be_bytes(sample))
            .collect();
        assert_eq!(decoded, values);

        let frame = opaque(WIDTH, HEIGHT);
        let level = |level: &str| {
            let level = settings(&[("compression", text(level))]);
            encode("png", &level, &BOTH, frame.clone()).1
        };
        let (none, high) = (level("none"), level("high"));
        assert_ne!(none, high);
        assert_eq!(decode_png(&high).1, frame);
        assert_eq!(decode_png(&none).1, frame);
        for other in ["fastest", "fast", "balanced"] {
            assert_eq!(decode_png(&level(other)).1, frame, "{other}");
        }

        let mut provider = ImageComponents::default();
        let only = |provider: &mut ImageComponents, settings: &Settings, offered| {
            open_sized(provider, "png", settings, (WIDTH, HEIGHT), offered).unwrap_err()
        };
        let error = only(&mut provider, &sixteen, &[PixelFormat::Rgba8]);
        assert!(
            matches!(&error, MediaError::InvalidSettings { key, message }
                if key == "depth"
                && message == "16-bit PNG needs rgba16 input, which this export does not offer"),
            "{error:?}"
        );
        let error = only(&mut provider, &Settings::new(), &[PixelFormat::Rgba16]);
        assert!(
            matches!(&error, MediaError::InvalidSettings { key, .. } if key == "input"),
            "{error:?}"
        );
    }

    #[test]
    fn transparent_keeps_alpha() {
        let mut frame = opaque(WIDTH, HEIGHT);
        frame[3] = 0;
        frame[7] = 128;
        let transparent = settings(&[("transparent", SettingValue::Boolean(true))]);

        let (opened, bytes) = encode("png", &transparent, &BOTH, frame.clone());
        assert!(opened.alpha);
        assert_eq!(opened.input, PixelFormat::Rgba8);
        let pixels = decode_png(&bytes).1;
        assert_eq!((pixels[3], pixels[7]), (0, 128));

        let lossless = settings(&[
            ("transparent", SettingValue::Boolean(true)),
            ("lossless", SettingValue::Boolean(true)),
        ]);
        let (opened, bytes) = encode("webp", &lossless, &BOTH, frame.clone());
        assert!(opened.alpha);
        let pixels = decode_webp(&bytes);
        assert_eq!((pixels[3], pixels[7]), (0, 128));
        assert!(encode("webp", &transparent, &BOTH, frame.clone()).0.alpha);

        for component in ["png", "webp"] {
            assert!(
                !encode(component, &Settings::new(), &BOTH, frame.clone())
                    .0
                    .alpha
            );
        }
        assert!(!encode("jpeg", &Settings::new(), &BOTH, frame).0.alpha);
    }

    #[test]
    fn webp_lossless_is_vp8l() {
        let has = |bytes: &[u8], chunk: &[u8; 4]| bytes.windows(4).any(|window| window == chunk);
        let frame = opaque(WIDTH, HEIGHT);
        let lossless = settings(&[("lossless", SettingValue::Boolean(true))]);
        let (_, bytes) = encode("webp", &lossless, &BOTH, frame.clone());
        assert!(has(&bytes, b"VP8L") && !has(&bytes, b"VP8 "));
        assert_eq!(decode_webp(&bytes), frame);
        let (_, bytes) = encode("webp", &Settings::new(), &BOTH, frame);
        assert!(has(&bytes, b"VP8 ") && !has(&bytes, b"VP8L"));
    }

    #[test]
    fn webp_refuses_images_too_large() {
        let mut provider = ImageComponents::default();
        let mut open = |size| open_sized(&mut provider, "webp", &Settings::new(), size, &BOTH);
        assert!(open((16_383, 16_383)).is_ok());
        let error = open((16_384, 9)).unwrap_err();
        assert!(
            matches!(&error, MediaError::InvalidSettings { key, message }
                if key == "size"
                && message == "a 16384x9 image is too large for WebP, which holds at most \
                    16383 pixels on each side (write PNG or JPEG, or make the contact sheet smaller)"),
            "{error:?}"
        );
        assert!(open((9, 16_384)).is_err());
    }

    #[test]
    fn sessions_share_one_counter_and_refuse_strangers() {
        let mut provider = ImageComponents::default();
        let a = open_sized(&mut provider, "png", &Settings::new(), (1, 1), &BOTH).unwrap();
        let b = open_sized(&mut provider, "jpeg", &Settings::new(), (1, 1), &BOTH).unwrap();
        assert_eq!((a.session, b.session), (SessionId(1), SessionId(2)));
        assert!(matches!(
            provider.encode_video(a.session, 0, vec![0; 3]),
            Err(MediaError::Codec(_))
        ));
        provider.abort_video(a.session);
        assert!(provider.finish_video(a.session).is_err());
        assert!(matches!(
            open_sized(&mut provider, "gif", &Settings::new(), (1, 1), &BOTH),
            Err(MediaError::Codec(_))
        ));
        let unknown = settings(&[("speed", SettingValue::Number(1.0))]);
        assert!(matches!(
            open_sized(&mut provider, "png", &unknown, (1, 1), &BOTH),
            Err(MediaError::InvalidSettings { key, .. }) if key == "speed"
        ));
    }
}
