//! The animated GIF video encoder `builtin/gif` and its muxer
//! `builtin/gifFile`.
//!
//! The encoder takes straight `rgba8` frames at the composition's rate and
//! keeps those its frame rate asks for, scaled, quantized to at most 256
//! colours (one palette for the whole GIF or one per frame, NeuQuant when a
//! frame has more colours than fit, Floyd–Steinberg dithering), with only
//! the rectangle that changed written and an identical frame folded into
//! the one before. Its track config's `description` is the file's header
//! (the logical screen, the global palette, and the loop extension); each
//! packet is one frame's graphic control extension and image. The muxer
//! writes the header, the packets in order, and the trailer.
//!
//! GIF times frames in hundredths of a second, and browsers play a delay
//! under 2 hundredths at 10, so no frame is kept less than 2 hundredths
//! after the one before, and delays are rounded on the frames' times so the
//! whole GIF keeps the composition's length.

use crate::encoders::{field, invalid_setting, option, transparent};
use celesta_composition::Rational;
use celesta_media::settings::validate;
use celesta_media::{
    Encoded, InvalidSetting, MediaError, MuxerRequest, Packet, PixelFormat, SettingField,
    SettingKind, SettingValue, Settings, Track, TrackConfig, TrackPacket, VideoEncoderRequest,
};
use celesta_plugin_host::PluginKillSwitch;
use std::borrow::Cow;
use std::collections::{BTreeSet, HashMap};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};

/// The encoder's id, and the codec (and family) it writes.
pub const GIF: &str = "gif";

/// The muxer's id: not `gif`, so `builtin/gif` names one component.
pub const GIF_FILE: &str = "gifFile";

/// The highest frame rate a GIF plays at: a frame lasts at least 2
/// hundredths of a second.
pub const MAX_FRAME_RATE: u32 = 50;

/// The shortest delay kept, in hundredths of a second.
const MIN_DELAY: u64 = 2;

/// The frame rates the `frameRate` select offers besides `source`.
const FRAME_RATES: [&str; 9] = ["50", "30", "25", "24", "20", "15", "12", "10", "5"];

/// A pixel with at least this alpha is opaque in a transparent GIF.
const ALPHA_THRESHOLD: u8 = 128;

/// The most pixels a global palette is learned from; frames past it are
/// sampled evenly.
const PALETTE_SAMPLE: usize = 1 << 20;

/// How finely NeuQuant samples what it learns from (1 is every pixel).
const NEUQUANT_SAMPLING: i32 = 3;

/// The GIF trailer.
const TRAILER: u8 = 0x3B;

/// The encoder's settings schema.
pub(crate) fn settings() -> Vec<SettingField> {
    let number = |min: f64, max: f64, unit: Option<&str>| SettingKind::Number {
        min: Some(min),
        max: Some(max),
        step: Some(1.0),
        integer: true,
        unit: unit.map(str::to_owned),
    };
    let mut rates = vec![option("source", "The composition's (at most 50)")];
    rates.extend(
        FRAME_RATES
            .iter()
            .map(|rate| option(rate, &format!("{rate} fps"))),
    );
    vec![
        field(
            "frameRate",
            "Frame rate",
            SettingValue::String("source".to_owned()),
            SettingKind::Select { options: rates },
            Some(
                "Frames are dropped to this rate. The composition's keeps every frame up to \
                 50 fps and every second (or third, …) frame above it.",
            ),
        ),
        field(
            "scale",
            "Scale",
            SettingValue::Number(100.0),
            number(1.0, 100.0, Some("%")),
            Some("The GIF's size, as a share of the composition's."),
        ),
        field(
            "palette",
            "Palette",
            SettingValue::String("global".to_owned()),
            SettingKind::Select {
                options: vec![
                    option("global", "One for the whole GIF"),
                    option("frame", "One for each frame"),
                ],
            },
            Some(
                "One palette keeps colours steady and files small, but holds every frame in \
                 memory until the end; one per frame suits scenes whose colours change.",
            ),
        ),
        field(
            "colors",
            "Colours",
            SettingValue::Number(256.0),
            number(2.0, 256.0, None),
            Some("The most colours in a palette; one is kept for transparency."),
        ),
        field(
            "dither",
            "Dither",
            SettingValue::Boolean(true),
            SettingKind::Boolean,
            Some("Mixes palette colours so gradients do not band."),
        ),
        field(
            "plays",
            "Plays",
            SettingValue::Number(0.0),
            number(0.0, 65_536.0, None),
            Some("How many times the GIF plays; 0 plays it forever."),
        ),
        transparent(),
    ]
}

/// Which frames the encoder keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Rate {
    /// Every `n`th frame.
    Step(u64),
    /// The first frame of each 1/`fps` second.
    Fixed(u32),
}

/// The encoder's validated settings.
#[derive(Clone, Copy, Debug)]
struct Options {
    rate: Rate,
    scale: u32,
    global: bool,
    colors: usize,
    dither: bool,
    plays: u32,
    transparent: bool,
}

fn number(settings: &Settings, key: &str) -> f64 {
    match settings.get(key) {
        Some(SettingValue::Number(value)) => *value,
        _ => 0.0,
    }
}

fn text<'a>(settings: &'a Settings, key: &str) -> &'a str {
    match settings.get(key) {
        Some(SettingValue::String(text)) => text,
        _ => "",
    }
}

/// `settings`, validated and filled in from the schema's defaults.
fn options(settings: &Settings, frame_rate: Rational) -> Result<Options, MediaError> {
    let settings = validate(&self::settings(), settings)
        .map_err(|InvalidSetting { key, message }| MediaError::InvalidSettings { key, message })?;
    let rate = match text(&settings, "frameRate") {
        "source" => Rate::Step(source_step(frame_rate)),
        rate => Rate::Fixed(rate.parse().unwrap_or(MAX_FRAME_RATE)),
    };
    Ok(Options {
        rate,
        scale: number(&settings, "scale") as u32,
        global: text(&settings, "palette") == "global",
        colors: number(&settings, "colors") as usize,
        dither: settings.get("dither") == Some(&SettingValue::Boolean(true)),
        plays: number(&settings, "plays") as u32,
        transparent: settings.get("transparent") == Some(&SettingValue::Boolean(true)),
    })
}

/// The step that keeps `frame_rate` at or under [`MAX_FRAME_RATE`]: 1 up
/// to 50 fps, 2 up to 100, and so on.
fn source_step(frame_rate: Rational) -> u64 {
    let limit = u64::from(MAX_FRAME_RATE) * u64::from(frame_rate.denominator.max(1));
    u64::from(frame_rate.numerator).div_ceil(limit).max(1)
}

/// A side scaled to `scale` percent, rounded, at least 1.
fn scaled(side: u32, scale: u32) -> u32 {
    ((u64::from(side) * u64::from(scale) + 50) / 100).max(1) as u32
}

/// What a GIF export will hold: its size, how many frames it keeps at most,
/// and how many bytes of frames a global palette holds until the end.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GifEstimate {
    pub width: u32,
    pub height: u32,
    pub frames: u64,
    /// The kept frames' rgba8 bytes when `palette` is `global`, else 0.
    pub held_bytes: u64,
}

/// What `builtin/gif` with `settings` makes of `frames` `width`x`height`
/// frames at `frame_rate`: an upper bound, as identical frames are folded.
/// `None` when the settings do not validate.
pub fn estimate(
    settings: &Settings,
    width: u32,
    height: u32,
    frame_rate: Rational,
    frames: u64,
) -> Option<GifEstimate> {
    let options = options(settings, frame_rate).ok()?;
    let (width, height) = (scaled(width, options.scale), scaled(height, options.scale));
    let kept = match options.rate {
        Rate::Step(step) => frames.div_ceil(step),
        Rate::Fixed(fps) => {
            // At most one frame per 1/fps second, and never more than given.
            let slots = u128::from(frames) * u128::from(frame_rate.denominator) * u128::from(fps);
            let slots = slots.div_ceil(u128::from(frame_rate.numerator.max(1)));
            frames.min(u64::try_from(slots).unwrap_or(u64::MAX))
        }
    };
    let frame_bytes = u64::from(width) * u64::from(height) * 4;
    Some(GifEstimate {
        width,
        height,
        frames: kept,
        held_bytes: if options.global {
            frame_bytes.saturating_mul(kept)
        } else {
            0
        },
    })
}

/// Pixel-frames (width × height × frames) past which a GIF is called large.
const LARGE_PIXEL_FRAMES: u64 = 100_000_000;

/// Seconds past which a GIF is called long.
const LONG_SECONDS: u64 = 30;

/// Bytes of held frames past which a global palette warns.
const HELD_WARNING_BYTES: u64 = 1 << 30;

/// The warnings a GIF export of `frames` frames at `frame_rate` gives:
/// long, large, or a global palette holding too much in memory.
pub fn warnings(
    settings: &Settings,
    width: u32,
    height: u32,
    frame_rate: Rational,
    frames: u64,
) -> Vec<String> {
    let Some(estimate) = self::estimate(settings, width, height, frame_rate, frames) else {
        return Vec::new();
    };
    let mut warnings = Vec::new();
    let seconds = frames as f64 * f64::from(frame_rate.denominator.max(1))
        / f64::from(frame_rate.numerator.max(1));
    let pixel_frames = u64::from(estimate.width) * u64::from(estimate.height) * estimate.frames;
    if seconds > LONG_SECONDS as f64 || pixel_frames > LARGE_PIXEL_FRAMES {
        warnings.push(format!(
            "a {:.1} s GIF of {} {}x{} frames will be large and slow to write; GIFs suit short \
             loops: lower scale or frameRate (--video-option scale=50), or export a span with \
             --from/--to",
            seconds, estimate.frames, estimate.width, estimate.height
        ));
    }
    if estimate.held_bytes > HELD_WARNING_BYTES {
        warnings.push(format!(
            "palette=global holds every frame in memory until the end, about {:.1} GB here; \
             use palette=frame or a lower scale",
            estimate.held_bytes as f64 / 1e9
        ));
    }
    warnings
}

/// A frame the encoder kept: its pts, its time in hundredths, and its
/// pixels (scaled, with alpha 0 or 255).
struct Kept {
    pts: i64,
    centis: u64,
    pixels: Vec<u8>,
}

/// An open GIF encoder.
pub(crate) struct EncoderSession {
    options: Options,
    /// The frames' size as given.
    source: (u32, u32),
    /// The GIF's size.
    size: (u16, u16),
    timescale: u32,
    /// A frame's duration in the timescale.
    frame_duration: i64,
    /// The slot of the last frame kept, for [`Rate::Fixed`].
    last_slot: Option<u64>,
    /// The end of the last frame given, in the timescale.
    end: i64,
    /// The last frame kept, waiting for the next to know its delay.
    pending: Option<Kept>,
    /// The pixels of the frame before `pending`, which it is drawn over.
    shown: Option<Vec<u8>>,
    /// With a global palette, every frame kept before `pending`, with its
    /// delay.
    held: Vec<(Kept, u16)>,
    /// With a palette per frame, the GIF's header and its encoder, made at
    /// open; the header goes with the first packet.
    writer: Option<gif::Encoder<Vec<u8>>>,
    header: Option<Vec<u8>>,
    /// Stops the final encode of a global palette once the export is
    /// cancelled.
    kill: PluginKillSwitch,
}

fn failed(error: impl std::fmt::Display) -> MediaError {
    MediaError::Codec(format!("could not encode GIF: {error}"))
}

/// The GIF's header and an encoder writing after it, for a `size` screen
/// with `palette` as its global palette (none when empty).
fn header(
    size: (u16, u16),
    palette: &[u8],
    plays: u32,
) -> Result<(gif::Encoder<Vec<u8>>, Vec<u8>), MediaError> {
    let mut encoder = gif::Encoder::new(Vec::new(), size.0, size.1, palette).map_err(failed)?;
    match plays {
        0 => encoder.set_repeat(gif::Repeat::Infinite).map_err(failed)?,
        // Played once: no loop extension.
        1 => {}
        plays => {
            let repeats = u16::try_from(plays - 1).unwrap_or(u16::MAX);
            encoder
                .set_repeat(gif::Repeat::Finite(repeats))
                .map_err(failed)?;
        }
    }
    let header = std::mem::take(encoder.get_mut());
    Ok((encoder, header))
}

impl EncoderSession {
    /// Opens the encoder `request` asks for: whether it keeps alpha.
    pub(crate) fn open(
        request: &VideoEncoderRequest<'_>,
        kill: PluginKillSwitch,
    ) -> Result<(Self, bool), MediaError> {
        let options = options(request.settings, request.frame_rate)?;
        if !request.offered.contains(&PixelFormat::Rgba8) {
            return Err(invalid_setting(
                "input",
                "GIF needs rgba8 input, which this export does not offer",
            ));
        }
        if request.frame_rate.numerator == 0 || request.frame_rate.denominator == 0 {
            return Err(MediaError::Codec(format!(
                "the frame rate {}/{} is not valid",
                request.frame_rate.numerator, request.frame_rate.denominator
            )));
        }
        let (width, height) = (
            scaled(request.width, options.scale),
            scaled(request.height, options.scale),
        );
        let (Ok(gif_width), Ok(gif_height)) = (u16::try_from(width), u16::try_from(height)) else {
            return Err(invalid_setting(
                "scale",
                format!(
                    "a {width}x{height} GIF is too large: a GIF holds at most 65535 pixels on \
                     each side (lower scale)"
                ),
            ));
        };
        let size = (gif_width, gif_height);
        let (writer, header) = if options.global {
            (None, None)
        } else {
            let (writer, header) = self::header(size, &[], options.plays)?;
            (Some(writer), Some(header))
        };
        let session = Self {
            options,
            source: (request.width, request.height),
            size,
            timescale: request.timescale,
            frame_duration: i64::from(request.frame_rate.denominator),
            last_slot: None,
            end: 0,
            pending: None,
            shown: None,
            held: Vec::new(),
            writer,
            header,
            kill,
        };
        Ok((session, options.transparent))
    }

    /// `pts` in hundredths of a second, rounded.
    fn centis(&self, pts: i64) -> u64 {
        let pts = u128::try_from(pts.max(0)).unwrap_or(0);
        let timescale = u128::from(self.timescale.max(1));
        u64::try_from((pts * 200 + timescale) / (timescale * 2)).unwrap_or(u64::MAX)
    }

    /// Whether the frame at `pts` is one the frame rate keeps.
    fn keeps(&mut self, pts: i64) -> bool {
        let index = u64::try_from(pts / self.frame_duration.max(1)).unwrap_or(0);
        match self.options.rate {
            Rate::Step(step) => index.is_multiple_of(step),
            Rate::Fixed(fps) => {
                // The 1/fps second the frame starts in.
                let slot = u128::from(index)
                    * u128::try_from(self.frame_duration).unwrap_or(1)
                    * u128::from(fps)
                    / u128::from(self.timescale.max(1));
                let slot = u64::try_from(slot).unwrap_or(u64::MAX);
                if self.last_slot == Some(slot) {
                    return false;
                }
                self.last_slot = Some(slot);
                true
            }
        }
    }

    /// `frame` scaled to the GIF's size, with alpha made 0 or 255.
    fn prepare(&self, frame: Vec<u8>) -> Vec<u8> {
        let (width, height) = self.source;
        let (to_width, to_height) = (u32::from(self.size.0), u32::from(self.size.1));
        let transparent = self.options.transparent;
        let scaled = (to_width, to_height) != (width, height);
        let mut pixels = if scaled {
            let mut frame = frame;
            if transparent {
                // Straight alpha is premultiplied to scale, so transparent
                // pixels' colour does not darken the edges they blend into.
                for pixel in frame.as_chunks_mut::<4>().0 {
                    let alpha = u16::from(pixel[3]);
                    for channel in &mut pixel[..3] {
                        *channel = ((u16::from(*channel) * alpha + 127) / 255) as u8;
                    }
                }
            }
            let image = image::RgbaImage::from_raw(width, height, frame)
                .expect("the frame's length was checked");
            image::imageops::resize(
                &image,
                to_width,
                to_height,
                image::imageops::FilterType::Triangle,
            )
            .into_raw()
        } else {
            frame
        };
        for pixel in pixels.as_chunks_mut::<4>().0 {
            if transparent && pixel[3] < ALPHA_THRESHOLD {
                *pixel = [0; 4];
                continue;
            }
            if transparent && scaled && pixel[3] < 255 {
                let alpha = u16::from(pixel[3]);
                for channel in &mut pixel[..3] {
                    *channel = ((u16::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
                }
            }
            pixel[3] = 255;
        }
        pixels
    }

    /// Takes the frame shown at `pts`: the packets ready, which are the
    /// frame before the last one kept, with a palette per frame.
    pub(crate) fn encode(&mut self, pts: i64, frame: Vec<u8>) -> Result<Encoded, MediaError> {
        let (width, height) = self.source;
        let expected = PixelFormat::Rgba8.frame_bytes(width, height);
        if frame.len() != expected {
            return Err(MediaError::Codec(format!(
                "a {width}x{height} rgba8 frame has {expected} bytes, not {}",
                frame.len()
            )));
        }
        self.end = self.end.max(pts.saturating_add(self.frame_duration));
        if !self.keeps(pts) {
            return Ok(Encoded::default());
        }
        let centis = self.centis(pts);
        if let Some(pending) = &self.pending
            && centis < pending.centis + MIN_DELAY
        {
            return Ok(Encoded::default());
        }
        let pixels = self.prepare(frame);
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| pending.pixels == pixels)
        {
            // Unchanged: the frame before lasts longer.
            return Ok(Encoded::default());
        }
        let next = Kept {
            pts,
            centis,
            pixels,
        };
        match self.pending.replace(next) {
            Some(done) => {
                let delay = centis - done.centis;
                self.settle(done, delay)
            }
            None => Ok(Encoded::default()),
        }
    }

    /// `done`, shown for `delay` hundredths: written now with a palette per
    /// frame, held with a global one.
    fn settle(&mut self, done: Kept, delay: u64) -> Result<Encoded, MediaError> {
        let delay = u16::try_from(delay).unwrap_or(u16::MAX);
        if self.options.global {
            self.held.push((done, delay));
            return Ok(Encoded::default());
        }
        let palette = Palette::learn(
            std::iter::once(done.pixels.as_slice()),
            self.options.colors - 1,
        );
        let packet = self.write(&done, delay, &palette, true)?;
        let config = self.header.take().map(|header| TrackConfig {
            codec: GIF.to_owned(),
            description: Some(header),
        });
        Ok(Encoded {
            packets: vec![packet],
            config,
        })
    }

    /// Writes `frame` over the one shown before it, as one packet.
    fn write(
        &mut self,
        frame: &Kept,
        delay: u16,
        palette: &Palette,
        local: bool,
    ) -> Result<Packet, MediaError> {
        let (width, height) = (usize::from(self.size.0), usize::from(self.size.1));
        let transparent = self.options.transparent;
        // A transparent GIF clears each frame and draws it whole, and so does
        // one with a palette per frame, whose colours for an unchanged pixel
        // differ from the palette before; an opaque GIF with a global palette
        // draws only what changed, over what is shown.
        let shown = if transparent || local {
            None
        } else {
            self.shown.as_deref()
        };
        let (left, top, right, bottom) = match shown {
            Some(shown) => changed(shown, &frame.pixels, width, height),
            None => (0, 0, width, height),
        };
        let whole = shown.is_none();
        let clear = palette.colors.len() as u8;
        let mut lookup = Lookup::new(palette);
        let (rect_width, rect_height) = (right - left, bottom - top);
        let mut indices = vec![0; rect_width * rect_height];
        let mut errors = Diffusion::new(rect_width, self.options.dither && !palette.exact);
        for y in top..bottom {
            errors.next_row();
            for x in left..right {
                let at = (y * width + x) * 4;
                let pixel = &frame.pixels[at..at + 4];
                let unchanged = shown.is_some_and(|shown| shown[at..at + 4] == *pixel);
                let index = if unchanged || pixel[3] == 0 {
                    errors.skip(x - left);
                    clear
                } else {
                    let wanted = errors.wanted(x - left, [pixel[0], pixel[1], pixel[2]]);
                    let index = lookup.nearest(wanted);
                    errors.spread(x - left, wanted, palette.colors[usize::from(index)]);
                    index
                };
                indices[(y - top) * rect_width + (x - left)] = index;
            }
        }
        let mut colors: Vec<u8> = palette.colors.iter().flatten().copied().collect();
        // The transparent entry.
        colors.extend([0, 0, 0]);
        let gif_frame = gif::Frame {
            delay,
            dispose: if transparent {
                gif::DisposalMethod::Background
            } else {
                gif::DisposalMethod::Keep
            },
            transparent: Some(clear),
            left: left as u16,
            top: top as u16,
            width: rect_width as u16,
            height: rect_height as u16,
            palette: local.then_some(colors),
            buffer: Cow::Owned(indices),
            ..gif::Frame::default()
        };
        let writer = self
            .writer
            .as_mut()
            .ok_or_else(|| failed("its header is not written"))?;
        writer.write_frame(&gif_frame).map_err(failed)?;
        let bytes = std::mem::take(writer.get_mut());
        self.shown = Some(frame.pixels.clone());
        let next = self
            .pending
            .as_ref()
            .map_or(self.end, |pending| pending.pts);
        Ok(Packet {
            pts: frame.pts,
            dts: frame.pts,
            duration: (next - frame.pts).max(1),
            key: whole,
            bytes,
            additional: Vec::new(),
        })
    }

    /// Fails once the export was cancelled, which kills the plugins.
    fn check_killed(&self) -> Result<(), MediaError> {
        if self.kill.is_killed() {
            Err(MediaError::CodecStopped(
                "the export was cancelled while the GIF was written".to_owned(),
            ))
        } else {
            Ok(())
        }
    }

    /// The last frames: the one kept last, and with a global palette every
    /// frame, after the header.
    pub(crate) fn finish(&mut self) -> Result<Encoded, MediaError> {
        let Some(last) = self.pending.take() else {
            return Ok(Encoded::default());
        };
        let end = self.centis(self.end);
        let delay = end.saturating_sub(last.centis).max(MIN_DELAY);
        if !self.options.global {
            // `pending` is gone, so the packet lasts until the end.
            return self.settle(last, delay);
        }
        let delay = u16::try_from(delay).unwrap_or(u16::MAX);
        self.held.push((last, delay));
        let held = std::mem::take(&mut self.held);
        self.check_killed()?;
        let palette = Palette::learn(
            held.iter().map(|(frame, _)| frame.pixels.as_slice()),
            self.options.colors - 1,
        );
        let mut colors: Vec<u8> = palette.colors.iter().flatten().copied().collect();
        colors.extend([0, 0, 0]);
        let (writer, header) = self::header(self.size, &colors, self.options.plays)?;
        self.writer = Some(writer);
        let mut packets = Vec::with_capacity(held.len());
        let mut frames = held.into_iter().peekable();
        while let Some((frame, delay)) = frames.next() {
            self.check_killed()?;
            // Each packet lasts until the next frame.
            self.pending = frames.peek().map(|(next, _)| Kept {
                pts: next.pts,
                centis: next.centis,
                pixels: Vec::new(),
            });
            packets.push(self.write(&frame, delay, &palette, false)?);
        }
        self.pending = None;
        Ok(Encoded {
            packets,
            config: Some(TrackConfig {
                codec: GIF.to_owned(),
                description: Some(header),
            }),
        })
    }
}

/// The smallest rectangle (left, top, right, bottom; right and bottom
/// exclusive) holding every pixel that differs between `shown` and `next`;
/// the whole frame when none does.
fn changed(shown: &[u8], next: &[u8], width: usize, height: usize) -> (usize, usize, usize, usize) {
    let (mut left, mut top, mut right, mut bottom) = (width, height, 0, 0);
    for y in 0..height {
        let row = y * width * 4..(y + 1) * width * 4;
        let (before, after) = (&shown[row.clone()], &next[row]);
        if before == after {
            continue;
        }
        let first = (0..width)
            .find(|x| before[x * 4..x * 4 + 4] != after[x * 4..x * 4 + 4])
            .unwrap_or(0);
        let last = (0..width)
            .rev()
            .find(|x| before[x * 4..x * 4 + 4] != after[x * 4..x * 4 + 4])
            .unwrap_or(width - 1);
        left = left.min(first);
        right = right.max(last + 1);
        top = top.min(y);
        bottom = y + 1;
    }
    if right == 0 {
        (0, 0, width, height)
    } else {
        (left, top, right, bottom)
    }
}

/// A palette: at most 255 colours, the transparent entry after them.
struct Palette {
    colors: Vec<[u8; 3]>,
    /// Whether every opaque colour of the frames is in it exactly.
    exact: bool,
}

impl Palette {
    /// The palette of at most `limit` colours for the opaque pixels of
    /// `frames`: their colours, when there are no more; else what NeuQuant
    /// learns from them, sampled evenly to [`PALETTE_SAMPLE`] pixels.
    fn learn<'a>(frames: impl Iterator<Item = &'a [u8]> + Clone, limit: usize) -> Self {
        let limit = limit.max(1);
        let opaque = |frame: &'a [u8]| {
            frame
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|pixel| pixel[3] != 0)
        };
        let mut unique = BTreeSet::new();
        let mut count = 0usize;
        for frame in frames.clone() {
            for pixel in opaque(frame) {
                count += 1;
                if unique.len() <= limit {
                    unique.insert([pixel[0], pixel[1], pixel[2]]);
                }
            }
        }
        if unique.len() <= limit {
            let mut colors: Vec<[u8; 3]> = unique.into_iter().collect();
            if colors.is_empty() {
                colors.push([0, 0, 0]);
            }
            return Self {
                colors,
                exact: true,
            };
        }
        let stride = count.div_ceil(PALETTE_SAMPLE).max(1);
        let sample: Vec<u8> = frames
            .flat_map(opaque)
            .step_by(stride)
            .flatten()
            .copied()
            .collect();
        let quantizer = color_quant::NeuQuant::new(NEUQUANT_SAMPLING, limit, &sample);
        let colors = quantizer.color_map_rgb().as_chunks::<3>().0.to_vec();
        Self {
            colors,
            exact: false,
        }
    }
}

/// The nearest palette entry to a colour: exact colours by table, others
/// by the nearest to the centre of their 6-bit cell, cached.
struct Lookup<'a> {
    palette: &'a Palette,
    exact: HashMap<[u8; 3], u8>,
    cells: Vec<u16>,
}

impl<'a> Lookup<'a> {
    const EMPTY: u16 = u16::MAX;

    fn new(palette: &'a Palette) -> Self {
        let exact = palette
            .colors
            .iter()
            .enumerate()
            .map(|(index, color)| (*color, index as u8))
            .collect();
        Self {
            palette,
            exact,
            cells: vec![Self::EMPTY; 1 << 18],
        }
    }

    fn nearest(&mut self, color: [u8; 3]) -> u8 {
        if let Some(index) = self.exact.get(&color) {
            return *index;
        }
        let [red, green, blue] = color.map(|channel| usize::from(channel >> 2));
        let cell = (red << 12) | (green << 6) | blue;
        if self.cells[cell] == Self::EMPTY {
            let centre = color.map(|channel| i32::from(channel & !3) + 2);
            let index = self
                .palette
                .colors
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| {
                    entry
                        .iter()
                        .zip(centre)
                        .map(|(&entry, centre)| (i32::from(entry) - centre).pow(2))
                        .sum::<i32>()
                })
                .map_or(0, |(index, _)| index);
            self.cells[cell] = index as u16;
        }
        self.cells[cell] as u8
    }
}

/// Floyd–Steinberg error diffusion over one rectangle's rows, or none.
struct Diffusion {
    on: bool,
    /// The error carried into this row and the next, per pixel and channel,
    /// with a pixel of margin on each side.
    this: Vec<[f32; 3]>,
    next: Vec<[f32; 3]>,
}

impl Diffusion {
    fn new(width: usize, on: bool) -> Self {
        let row = if on { width + 2 } else { 0 };
        Self {
            on,
            this: vec![[0.0; 3]; row],
            next: vec![[0.0; 3]; row],
        }
    }

    fn next_row(&mut self) {
        if self.on {
            std::mem::swap(&mut self.this, &mut self.next);
            self.next.fill([0.0; 3]);
        }
    }

    /// The colour wanted at `x`: the pixel plus the error carried to it.
    fn wanted(&self, x: usize, color: [u8; 3]) -> [u8; 3] {
        if !self.on {
            return color;
        }
        let error = self.this[x + 1];
        [0, 1, 2].map(|channel| {
            (f32::from(color[channel]) + error[channel])
                .round()
                .clamp(0.0, 255.0) as u8
        })
    }

    /// Spreads what drawing `drawn` for `wanted` at `x` missed by.
    fn spread(&mut self, x: usize, wanted: [u8; 3], drawn: [u8; 3]) {
        if !self.on {
            return;
        }
        let at = x + 1;
        for channel in 0..3 {
            let error = f32::from(wanted[channel]) - f32::from(drawn[channel]);
            self.this[at + 1][channel] += error * 7.0 / 16.0;
            self.next[at - 1][channel] += error * 3.0 / 16.0;
            self.next[at][channel] += error * 5.0 / 16.0;
            self.next[at + 1][channel] += error / 16.0;
        }
    }

    /// Drops the error carried to `x`, which is not drawn.
    fn skip(&mut self, x: usize) {
        if self.on {
            self.this[x + 1] = [0.0; 3];
        }
    }
}

/// An open GIF muxer: the file it writes.
pub(crate) struct MuxerSession {
    output: PathBuf,
    file: Option<BufWriter<File>>,
}

fn tracks_refused(message: impl Into<String>) -> MediaError {
    MediaError::InvalidSettings {
        key: "tracks".to_owned(),
        message: message.into(),
    }
}

fn write_failed(path: &Path, error: std::io::Error) -> MediaError {
    MediaError::Codec(format!("could not write {}: {error}", path.display()))
}

impl MuxerSession {
    /// Creates `request.output` and writes the header of its one GIF track.
    pub(crate) fn open(request: &MuxerRequest<'_>) -> Result<Self, MediaError> {
        validate(&[], request.settings).map_err(|InvalidSetting { key, message }| {
            MediaError::InvalidSettings { key, message }
        })?;
        let [Track::Video { config, .. }] = request.tracks else {
            return Err(tracks_refused(
                "builtin/gifFile takes one video track and no audio",
            ));
        };
        if celesta_plugin_host::codec_family(&config.codec) != GIF {
            return Err(tracks_refused(format!(
                "builtin/gifFile takes gif video, not {}",
                config.codec
            )));
        }
        let Some(header) = &config.description else {
            return Err(tracks_refused("the gif track has no header"));
        };
        let output = request.output.to_owned();
        let file = File::create(&output).map_err(|error| write_failed(&output, error))?;
        let mut session = Self {
            output,
            file: Some(BufWriter::new(file)),
        };
        session.write(header)?;
        Ok(session)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<(), MediaError> {
        let output = &self.output;
        self.file
            .as_mut()
            .ok_or_else(|| MediaError::Codec("builtin/gifFile is finished".to_owned()))?
            .write_all(bytes)
            .map_err(|error| write_failed(output, error))
    }

    pub(crate) fn write_packets(&mut self, packets: &[TrackPacket]) -> Result<(), MediaError> {
        for TrackPacket { track, packet } in packets {
            if *track != 0 {
                return Err(MediaError::Codec(format!(
                    "a packet for track {track}, which the muxer does not have"
                )));
            }
            self.write(&packet.bytes)?;
        }
        Ok(())
    }

    /// Writes the trailer and closes the file: the file written.
    pub(crate) fn finish(mut self) -> Result<Vec<PathBuf>, MediaError> {
        self.write(&[TRAILER])?;
        let file = self.file.take().expect("written above");
        file.into_inner()
            .map_err(|error| write_failed(&self.output, error.into_error()))?;
        Ok(vec![self.output])
    }

    /// Closes the file: the file left, for Celesta to remove.
    pub(crate) fn abort(mut self) -> Vec<PathBuf> {
        self.file.take();
        vec![self.output]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ImageComponents;
    use celesta_media::{Color, MediaProvider, Muxer, SessionId, VideoEncoder};

    /// A frame of `width`x`height` pixels, each `color(x, y)`.
    fn frame(width: u32, height: u32, color: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        (0..height)
            .flat_map(|y| (0..width).map(move |x| (x, y)))
            .flat_map(|(x, y)| color(x, y))
            .collect()
    }

    fn flat(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        frame(width, height, |_, _| color)
    }

    fn settings(pairs: &[(&str, SettingValue)]) -> Settings {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), value.clone()))
            .collect()
    }

    fn text(value: &str) -> SettingValue {
        SettingValue::String(value.to_owned())
    }

    /// Encodes `frames` (`width`x`height`, at `fps`/1) with `settings` and
    /// muxes them: the GIF's bytes, and whether the encoder kept alpha.
    fn gif_of(
        settings: &Settings,
        (width, height): (u32, u32),
        frame_rate: Rational,
        frames: &[Vec<u8>],
    ) -> (Vec<u8>, bool, Vec<Packet>) {
        let mut provider = ImageComponents::default();
        let color = Color::bt709_limited();
        let opened = provider
            .open_video(&VideoEncoderRequest {
                component: GIF,
                settings,
                width,
                height,
                frame_rate,
                timescale: frame_rate.numerator,
                offered: &[
                    PixelFormat::Yuv420p,
                    PixelFormat::Rgba16,
                    PixelFormat::Rgba8,
                ],
                color: &color,
            })
            .unwrap();
        assert_eq!(opened.input, PixelFormat::Rgba8);
        let mut config = None;
        let mut packets = Vec::new();
        let mut take = |encoded: Encoded| {
            if config.is_none() {
                config = encoded.config;
            }
            packets.extend(encoded.packets);
        };
        for (index, frame) in frames.iter().enumerate() {
            let pts = index as i64 * i64::from(frame_rate.denominator);
            take(
                provider
                    .encode_video(opened.session, pts, frame.clone())
                    .unwrap(),
            );
        }
        take(provider.finish_video(opened.session).unwrap());
        let config = config.expect("the encoder gave its header");
        assert_eq!(config.codec, "gif");
        let folder = tempfile::tempdir().unwrap();
        let output = folder.path().join("out.gif");
        let track = Track::Video {
            config,
            timescale: frame_rate.numerator,
            width,
            height,
            frame_rate,
            color,
            alpha: opened.alpha,
        };
        let session = provider
            .open_muxer(&MuxerRequest {
                component: GIF_FILE,
                settings: &Settings::new(),
                output: &output,
                tracks: &[track],
                leading_audio: 0,
            })
            .unwrap();
        let written: Vec<TrackPacket> = packets
            .iter()
            .map(|packet| TrackPacket {
                track: 0,
                packet: packet.clone(),
            })
            .collect();
        provider.write_packets(session, &written).unwrap();
        assert_eq!(
            provider.finish_muxer(session).unwrap(),
            std::slice::from_ref(&output)
        );
        (std::fs::read(&output).unwrap(), opened.alpha, packets)
    }

    /// One decoded frame: its delay, its rectangle, and whether it had a
    /// palette of its own, with the canvas after drawing it.
    #[derive(Debug)]
    struct Shown {
        delay: u16,
        rect: (u16, u16, u16, u16),
        local: bool,
        canvas: Vec<u8>,
    }

    /// The GIF's size, its repeat, and each frame drawn as a browser draws
    /// it (straight RGBA).
    fn decode(bytes: &[u8]) -> ((u16, u16), gif::Repeat, Vec<Shown>) {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let mut decoder = options.read_info(bytes).unwrap();
        let (width, height) = (decoder.width(), decoder.height());
        let mut canvas = vec![0u8; usize::from(width) * usize::from(height) * 4];
        let mut shown = Vec::new();
        while let Some(frame) = decoder.read_next_frame().unwrap() {
            for y in 0..usize::from(frame.height) {
                for x in 0..usize::from(frame.width) {
                    let from = (y * usize::from(frame.width) + x) * 4;
                    let pixel = &frame.buffer[from..from + 4];
                    if pixel[3] != 0 {
                        let to = ((usize::from(frame.top) + y) * usize::from(width)
                            + usize::from(frame.left)
                            + x)
                            * 4;
                        canvas[to..to + 4].copy_from_slice(pixel);
                    }
                }
            }
            shown.push(Shown {
                delay: frame.delay,
                rect: (frame.left, frame.top, frame.width, frame.height),
                local: frame.palette.is_some(),
                canvas: canvas.clone(),
            });
            if frame.dispose == gif::DisposalMethod::Background {
                for y in 0..usize::from(frame.height) {
                    let row = ((usize::from(frame.top) + y) * usize::from(width)
                        + usize::from(frame.left))
                        * 4;
                    canvas[row..row + usize::from(frame.width) * 4].fill(0);
                }
            }
        }
        ((width, height), decoder.repeat(), shown)
    }

    const RED: [u8; 4] = [220, 30, 40, 255];
    const BLUE: [u8; 4] = [20, 60, 200, 255];
    const WHITE: [u8; 4] = [255, 255, 255, 255];

    #[test]
    fn writes_a_gif_that_loops_with_each_frames_delay() {
        let (width, height) = (8, 6);
        let dot = frame(
            width,
            height,
            |x, y| if (x, y) == (3, 2) { RED } else { WHITE },
        );
        let frames = [
            flat(width, height, WHITE),
            dot.clone(),
            dot,
            flat(width, height, BLUE),
        ];
        let (bytes, alpha, packets) = gif_of(
            &Settings::new(),
            (width, height),
            Rational::new(10, 1),
            &frames,
        );
        assert!(!alpha);
        assert!(bytes.starts_with(b"GIF89a"));
        assert_eq!(bytes.last(), Some(&TRAILER));
        let (size, repeat, shown) = decode(&bytes);
        assert_eq!(size, (8, 6));
        assert_eq!(repeat, gif::Repeat::Infinite);
        // The repeated frame lasts twice as long, and the 4 frames at 10
        // fps last 40 hundredths in all.
        let delays: Vec<u16> = shown.iter().map(|shown| shown.delay).collect();
        assert_eq!(delays, [10, 20, 10]);
        // Packets last until the next one, in the timescale.
        let times: Vec<(i64, i64)> = packets.iter().map(|p| (p.pts, p.duration)).collect();
        assert_eq!(times, [(0, 1), (1, 2), (3, 1)]);
        // Only the changed pixel is drawn over the white frame, with the
        // one global palette; every colour is exact.
        assert_eq!(shown[1].rect, (3, 2, 1, 1));
        assert!(shown.iter().all(|shown| !shown.local));
        for (shown, expected) in shown.iter().zip([&frames[0], &frames[1], &frames[3]]) {
            assert_eq!(&shown.canvas, expected);
        }
    }

    #[test]
    fn a_palette_per_frame_writes_each_frame_as_it_comes() {
        let (width, height) = (4, 4);
        let frames = [flat(width, height, RED), flat(width, height, BLUE)];
        let options = settings(&[("palette", text("frame"))]);
        let mut provider = ImageComponents::default();
        let color = Color::bt709_limited();
        let opened = provider
            .open_video(&VideoEncoderRequest {
                component: GIF,
                settings: &options,
                width,
                height,
                frame_rate: Rational::new(25, 1),
                timescale: 25,
                offered: &[PixelFormat::Rgba8],
                color: &color,
            })
            .unwrap();
        let first = provider.encode_video(opened.session, 0, frames[0].clone());
        assert_eq!(first.unwrap(), Encoded::default());
        // The first frame is written once the second gives its delay, with
        // the header.
        let second = provider
            .encode_video(opened.session, 1, frames[1].clone())
            .unwrap();
        assert_eq!(second.packets.len(), 1);
        assert!(
            second
                .config
                .unwrap()
                .description
                .unwrap()
                .starts_with(b"GIF89a")
        );
        let (bytes, _, _) = gif_of(&options, (width, height), Rational::new(25, 1), &frames);
        let (_, _, shown) = decode(&bytes);
        assert!(shown.iter().all(|shown| shown.local));
        assert_eq!(shown[0].canvas, frames[0]);
        assert_eq!(shown[1].canvas, frames[1]);
        assert_eq!(
            shown.iter().map(|shown| shown.delay).collect::<Vec<_>>(),
            [4, 4]
        );
    }

    #[test]
    fn a_palette_per_frame_redraws_pixels_the_palette_before_missed() {
        // One colour per frame: the first frame's palette cannot hold red,
        // and the all-red frame after it must still show red everywhere.
        let (width, height) = (4, 2);
        let mixed = frame(width, height, |x, _| if x < 3 { BLUE } else { RED });
        let red = flat(width, height, RED);
        let options = settings(&[
            ("palette", text("frame")),
            ("colors", SettingValue::Number(2.0)),
        ]);
        let (bytes, _, _) = gif_of(
            &options,
            (width, height),
            Rational::new(10, 1),
            &[mixed, red.clone()],
        );
        let (_, _, shown) = decode(&bytes);
        assert_eq!(shown[1].canvas, red);
        assert_eq!(shown[1].rect, (0, 0, 4, 2));
    }

    /// The delays of `count` distinct frames at `frame_rate` with
    /// `options`.
    fn delays(options: &Settings, frame_rate: Rational, count: u32) -> Vec<u16> {
        let frames: Vec<Vec<u8>> = (0..count)
            .map(|index| flat(2, 2, [index as u8, 0, 255 - index as u8, 255]))
            .collect();
        let (bytes, _, _) = gif_of(options, (2, 2), frame_rate, &frames);
        decode(&bytes).2.iter().map(|shown| shown.delay).collect()
    }

    #[test]
    fn the_frame_rate_drops_frames_and_keeps_the_length() {
        // 60 fps keeps every second frame: 30 fps, in 3 and 4 hundredths.
        let sixty = delays(&Settings::new(), Rational::new(60, 1), 12);
        assert_eq!(sixty, [3, 4, 3, 3, 4, 3]);
        assert_eq!(sixty.iter().map(|&d| u32::from(d)).sum::<u32>(), 20);
        // 30 fps is kept whole.
        assert_eq!(delays(&Settings::new(), Rational::new(30, 1), 3), [3, 4, 3]);
        // NTSC 29.97 too.
        assert_eq!(
            delays(&Settings::new(), Rational::new(30_000, 1_001), 3).len(),
            3
        );
        // 10 fps from 30: every third frame.
        let ten = settings(&[("frameRate", text("10"))]);
        assert_eq!(delays(&ten, Rational::new(30, 1), 9), [10, 10, 10]);
        // 50 fps from 60 drops what would last under 2 hundredths.
        let fifty = settings(&[("frameRate", text("50"))]);
        let kept = delays(&fifty, Rational::new(60, 1), 60);
        assert!(kept.iter().all(|&delay| delay >= 2), "{kept:?}");
        assert_eq!(kept.iter().map(|&d| u32::from(d)).sum::<u32>(), 100);
    }

    #[test]
    fn source_step_keeps_at_most_50_fps() {
        assert_eq!(source_step(Rational::new(24, 1)), 1);
        assert_eq!(source_step(Rational::new(50, 1)), 1);
        assert_eq!(source_step(Rational::new(60, 1)), 2);
        assert_eq!(source_step(Rational::new(60_000, 1_001)), 2);
        assert_eq!(source_step(Rational::new(120, 1)), 3);
    }

    #[test]
    fn plays_sets_the_loop_extension() {
        let frames = [flat(2, 2, RED), flat(2, 2, BLUE)];
        let repeat = |plays: f64| {
            let options = settings(&[("plays", SettingValue::Number(plays))]);
            let (bytes, _, _) = gif_of(&options, (2, 2), Rational::new(10, 1), &frames);
            (
                decode(&bytes).1,
                bytes.windows(11).any(|w| w == b"NETSCAPE2.0"),
            )
        };
        assert_eq!(repeat(0.0), (gif::Repeat::Infinite, true));
        assert_eq!(repeat(1.0), (gif::Repeat::Finite(0), false));
        assert_eq!(repeat(3.0), (gif::Repeat::Finite(2), true));
    }

    #[test]
    fn scale_makes_the_gif_smaller() {
        let options = settings(&[("scale", SettingValue::Number(50.0))]);
        let frames = [flat(9, 6, RED), flat(9, 6, BLUE)];
        let (bytes, _, _) = gif_of(&options, (9, 6), Rational::new(10, 1), &frames);
        let ((width, height), _, shown) = decode(&bytes);
        assert_eq!((width, height), (5, 3));
        assert_eq!(shown[1].canvas, flat(5, 3, BLUE));
    }

    #[test]
    fn transparent_keeps_one_bit_alpha_and_clears_each_frame() {
        let (width, height) = (4, 2);
        let left = frame(
            width,
            height,
            |x, _| if x < 2 { RED } else { [9, 9, 9, 40] },
        );
        let right = frame(width, height, |x, _| if x >= 2 { BLUE } else { [0; 4] });
        let options = settings(&[("transparent", SettingValue::Boolean(true))]);
        let (bytes, alpha, _) = gif_of(
            &options,
            (width, height),
            Rational::new(10, 1),
            &[left, right],
        );
        assert!(alpha);
        let (_, _, shown) = decode(&bytes);
        let clear = [0u8; 4];
        assert_eq!(
            shown[0].canvas,
            frame(width, height, |x, _| if x < 2 { RED } else { clear })
        );
        // The red half is cleared before the second frame.
        assert_eq!(
            shown[1].canvas,
            frame(width, height, |x, _| if x >= 2 { BLUE } else { clear })
        );
        assert!(shown.iter().all(|shown| shown.rect == (0, 0, 4, 2)));
    }

    #[test]
    fn a_scaled_transparent_edge_keeps_its_colour() {
        // Transparent black beside red: the edge it blends into stays red.
        let (width, height) = (8, 2);
        let half = frame(width, height, |x, _| if x < 4 { RED } else { [0; 4] });
        let options = settings(&[
            ("transparent", SettingValue::Boolean(true)),
            ("scale", SettingValue::Number(50.0)),
        ]);
        let (bytes, _, _) = gif_of(
            &options,
            (width, height),
            Rational::new(10, 1),
            std::slice::from_ref(&half),
        );
        let (_, _, shown) = decode(&bytes);
        let pixels = shown[0].canvas.as_chunks::<4>().0;
        let kept: Vec<_> = pixels.iter().filter(|pixel| pixel[3] != 0).collect();
        // A pixel of red alone and one blended from red and transparent,
        // red within 8-bit rounding (unpremultiplied it would be about 190);
        // the next, mostly transparent, is dropped.
        assert_eq!(kept.len(), 2, "{pixels:?}");
        assert!(
            kept.iter()
                .all(|pixel| pixel.iter().zip(RED).all(|(&a, b)| a.abs_diff(b) <= 2)),
            "{pixels:?}"
        );
    }

    #[test]
    fn cancel_stops_the_final_encode() {
        let kill = PluginKillSwitch::new();
        let mut provider = ImageComponents::new(kill.clone());
        let color = Color::bt709_limited();
        let opened = provider
            .open_video(&VideoEncoderRequest {
                component: GIF,
                settings: &Settings::new(),
                width: 2,
                height: 2,
                frame_rate: Rational::new(10, 1),
                timescale: 10,
                offered: &[PixelFormat::Rgba8],
                color: &color,
            })
            .unwrap();
        for (pts, color) in [RED, BLUE, RED].into_iter().enumerate() {
            provider
                .encode_video(opened.session, pts as i64, flat(2, 2, color))
                .unwrap();
        }
        kill.kill();
        assert!(matches!(
            provider.finish_video(opened.session),
            Err(MediaError::CodecStopped(_))
        ));
    }

    #[test]
    fn many_colours_are_learned_and_dithered() {
        let (width, height) = (32, 32);
        let gradient = frame(width, height, |x, y| {
            [(x * 8) as u8, (y * 8) as u8, ((x + y) * 4) as u8, 255]
        });
        for dither in [true, false] {
            let options = settings(&[
                ("colors", SettingValue::Number(16.0)),
                ("dither", SettingValue::Boolean(dither)),
            ]);
            let (bytes, _, _) = gif_of(
                &options,
                (width, height),
                Rational::new(10, 1),
                std::slice::from_ref(&gradient),
            );
            let mut options = gif::DecodeOptions::new();
            options.set_color_output(gif::ColorOutput::Indexed);
            let decoder = options.read_info(bytes.as_slice()).unwrap();
            // 15 colours and the transparent entry, padded to 16.
            assert_eq!(decoder.global_palette().unwrap().len(), 16 * 3);
            let (_, _, shown) = decode(&bytes);
            let error: f64 = shown[0]
                .canvas
                .iter()
                .zip(&gradient)
                .map(|(&a, &b)| f64::from(a.abs_diff(b)))
                .sum::<f64>()
                / gradient.len() as f64;
            assert!(error < 24.0, "dither {dither}: mean error {error}");
        }
    }

    #[test]
    fn estimates_and_warns_of_large_gifs() {
        let thirty = Rational::new(30, 1);
        let estimate = estimate(&Settings::new(), 640, 360, thirty, 90).unwrap();
        assert_eq!(
            estimate,
            GifEstimate {
                width: 640,
                height: 360,
                frames: 90,
                held_bytes: 640 * 360 * 4 * 90,
            }
        );
        let ten = settings(&[
            ("frameRate", text("10")),
            ("scale", SettingValue::Number(50.0)),
            ("palette", text("frame")),
        ]);
        let estimate = self::estimate(&ten, 640, 360, thirty, 90).unwrap();
        assert_eq!(
            (estimate.width, estimate.height, estimate.frames),
            (320, 180, 30)
        );
        assert_eq!(estimate.held_bytes, 0);
        assert!(warnings(&Settings::new(), 640, 360, thirty, 90).is_empty());
        let long = warnings(&Settings::new(), 1920, 1080, thirty, 1800);
        assert_eq!(long.len(), 2, "{long:?}");
        assert!(
            long[0].starts_with("a 60.0 s GIF of 1800 1920x1080 frames"),
            "{long:?}"
        );
        assert!(
            long[1].starts_with("palette=global holds every frame"),
            "{long:?}"
        );
        assert!(
            warnings(
                &settings(&[("palette", text("frame"))]),
                1920,
                1080,
                thirty,
                1800
            )
            .iter()
            .all(|warning| !warning.contains("palette=global"))
        );
    }

    #[test]
    fn refuses_what_it_cannot_write() {
        let mut provider = ImageComponents::default();
        let color = Color::bt709_limited();
        let open = |provider: &mut ImageComponents, width, settings: &Settings, offered| {
            provider.open_video(&VideoEncoderRequest {
                component: GIF,
                settings,
                width,
                height: 4,
                frame_rate: Rational::new(30, 1),
                timescale: 30,
                offered,
                color: &color,
            })
        };
        fn key<T: std::fmt::Debug>(result: Result<T, MediaError>) -> String {
            match result {
                Err(MediaError::InvalidSettings { key, .. }) => key,
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(
            key(open(
                &mut provider,
                4,
                &Settings::new(),
                &[PixelFormat::Rgba16]
            )),
            "input"
        );
        assert_eq!(
            key(open(
                &mut provider,
                70_000,
                &Settings::new(),
                &[PixelFormat::Rgba8]
            )),
            "scale"
        );
        let wrong = settings(&[("palette", text("none"))]);
        assert_eq!(
            key(open(&mut provider, 4, &wrong, &[PixelFormat::Rgba8])),
            "palette"
        );
        let session = open(&mut provider, 4, &Settings::new(), &[PixelFormat::Rgba8])
            .unwrap()
            .session;
        assert!(matches!(
            provider.encode_video(session, 0, vec![0; 3]),
            Err(MediaError::Codec(_))
        ));

        let folder = tempfile::tempdir().unwrap();
        let video = |codec: &str, description: Option<Vec<u8>>| Track::Video {
            config: TrackConfig {
                codec: codec.to_owned(),
                description,
            },
            timescale: 30,
            width: 4,
            height: 4,
            frame_rate: Rational::new(30, 1),
            color: color.clone(),
            alpha: false,
        };
        let header = Some(b"GIF89a".to_vec());
        let mut mux = |name: &str, tracks: &[Track]| {
            provider.open_muxer(&MuxerRequest {
                component: GIF_FILE,
                settings: &Settings::new(),
                output: &folder.path().join(name),
                tracks,
                leading_audio: 0,
            })
        };
        assert_eq!(key(mux("a.gif", &[video("png", header.clone())])), "tracks");
        assert_eq!(key(mux("a.gif", &[video("gif", None)])), "tracks");
        let audio = Track::Audio {
            config: TrackConfig {
                codec: "opus".to_owned(),
                description: None,
            },
            timescale: 48_000,
            sample_rate: 48_000,
            channels: 2,
            delay: 0,
        };
        assert_eq!(
            key(mux("a.gif", &[video("gif", header.clone()), audio])),
            "tracks"
        );
        // A muxer named for another extension still writes a GIF there:
        // Celesta warns of the mismatch.
        let named: SessionId = mux("a.mp4", &[video("gif", header.clone())]).unwrap();
        // An aborted GIF names its file for Celesta to remove.
        let session: SessionId = mux("b.gif", &[video("gif", header)]).unwrap();
        assert_eq!(provider.abort_muxer(named), [folder.path().join("a.mp4")]);
        assert_eq!(provider.abort_muxer(session), [folder.path().join("b.gif")]);
        assert!(provider.describe().unwrap().iter().any(|c| c.id == GIF));
    }
}
