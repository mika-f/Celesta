//! Loudness of a whole mix (ITU-R BS.1770-4 / EBU R128) and the gain that
//! brings it to a target, keeping true peaks under a ceiling.
//!
//! Everything here is plain arithmetic on the buffer in a fixed order, so the
//! same mix always comes out the same.

use crate::error::AudioMixError;
use crate::types::AudioBuffer;
use ebur128::{EbuR128, Mode};
use rayon::prelude::*;

/// The integrated loudness and true peak of a buffer.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Loudness {
    /// Gated integrated loudness, LUFS. `None` when no 400 ms gating block
    /// is above the -70 LUFS absolute gate: a silent buffer, or one shorter
    /// than 400 ms.
    pub integrated_lufs: Option<f64>,
    /// The highest true peak of any channel (8x oversampled), dBTP. `None`
    /// for digital silence.
    pub true_peak_dbtp: Option<f64>,
}

/// What an exported mix's loudness is brought to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LoudnessTarget {
    /// Integrated loudness, LUFS.
    pub integrated_lufs: f64,
    /// The highest true peak the mix may reach, dBTP.
    pub true_peak_dbtp: f64,
    /// What gives when the gain would take a peak over `true_peak_dbtp`.
    pub peak_handling: PeakHandling,
}

impl LoudnessTarget {
    /// Lowest and highest targets accepted (FFmpeg's `loudnorm` range).
    pub const INTEGRATED_RANGE: (f64, f64) = (-70.0, -5.0);
    /// Lowest and highest true-peak ceilings accepted.
    pub const TRUE_PEAK_RANGE: (f64, f64) = (-9.0, 0.0);
    /// Streaming services (YouTube, Spotify) play at about -14 LUFS.
    pub const STREAMING: Self = Self::new(-14.0);
    /// EBU R128 broadcast loudness.
    pub const BROADCAST: Self = Self::new(-23.0);

    /// `integrated_lufs` with a -1 dBTP ceiling held by the limiter.
    pub const fn new(integrated_lufs: f64) -> Self {
        Self {
            integrated_lufs,
            true_peak_dbtp: -1.0,
            peak_handling: PeakHandling::Limit,
        }
    }
}

/// What a loudness target gives up when its gain would take a true peak over
/// the ceiling.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    strum::Display,
    strum::EnumString,
    strum::IntoStaticStr,
    strum::VariantArray,
)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[strum(serialize_all = "lowercase")]
pub enum PeakHandling {
    /// A true-peak limiter holds the peaks at the ceiling, so the target
    /// is reached. The limiter only touches the moments that would go over.
    #[default]
    Limit,
    /// The gain stops where the highest peak meets the ceiling, so the mix
    /// stays quieter than the target but is otherwise unchanged.
    Lower,
}

impl PeakHandling {
    pub fn as_str(self) -> &'static str {
        self.into()
    }
}

/// Measures `buffer`'s integrated loudness and true peak. Channels are
/// weighted as BS.1770 weights left, right and centre (1.0 each).
pub fn measure_loudness(buffer: &AudioBuffer) -> Loudness {
    let integrated_lufs = meter(buffer, Mode::I).and_then(|mut meter| {
        meter.add_frames_f32(&buffer.samples).ok()?;
        meter.loudness_global().ok()
    });
    Loudness {
        integrated_lufs: integrated_lufs.filter(|lufs| lufs.is_finite()),
        true_peak_dbtp: (buffer.channels > 0)
            .then(|| amplitude_db(f64::from(highest_true_peak(buffer))))
            .filter(|dbtp| dbtp.is_finite()),
    }
}

/// Brings `buffer` to `target`: one gain for the whole mix, plus true-peak
/// limiting with [`PeakHandling::Limit`]. Returns the gain in dB (0 when the
/// buffer has no measurable loudness, which leaves it unchanged) and the
/// loudness the buffer has afterwards.
///
/// `buffer` may run past ±1 (a mix before clipping); afterwards its true
/// peaks are at most the ceiling. With the limiter the gain is found in a
/// few passes, each measuring the limited mix, so the result lands within
/// [`TARGET_TOLERANCE_LU`] of the target unless the limiter would have to
/// flatten the mix to get there.
pub fn normalize_loudness(
    buffer: &mut AudioBuffer,
    target: &LoudnessTarget,
    mut is_cancelled: impl FnMut() -> bool,
) -> Result<(f64, Loudness), AudioMixError> {
    let input = measure_loudness(buffer);
    let Some(integrated) = input.integrated_lufs else {
        return Ok((0.0, input));
    };
    let mut gain_db = target.integrated_lufs - integrated;
    match target.peak_handling {
        PeakHandling::Lower => {
            if let Some(peak) = input.true_peak_dbtp {
                gain_db = gain_db.min(target.true_peak_dbtp - peak);
            }
            scale(buffer, gain_db);
        }
        PeakHandling::Limit => {
            let limiter = Limiter::new(buffer, db_amplitude(target.true_peak_dbtp));
            // Loudness rises with the gain, but by less than the gain once
            // the limiter works. The first gain, which ignores the limiter,
            // falls short; later ones are found between the loudest gain that
            // fell short and the quietest that went over (Illinois false
            // position), each as (gain, LU short of the target).
            let mut short: Option<(f64, f64)> = None;
            let mut over: Option<(f64, f64)> = None;
            let mut last_short = None;
            for _ in 0..MAX_LIMITER_PASSES {
                if is_cancelled() {
                    return Err(AudioMixError::Cancelled);
                }
                let Some(reached) = limiter.loudness(buffer, gain_db) else {
                    break;
                };
                let error = target.integrated_lufs - reached;
                if error.abs() <= TARGET_TOLERANCE_LU / 2.0 {
                    break;
                }
                let previous = short;
                let is_short = error > 0.0;
                // An end kept twice in a row counts half as far off, so the
                // next gain moves away from it.
                let kept = if is_short { &mut over } else { &mut short };
                if last_short == Some(is_short)
                    && let Some((_, kept_error)) = kept
                {
                    *kept_error /= 2.0;
                }
                last_short = Some(is_short);
                *(if is_short { &mut short } else { &mut over }) = Some((gain_db, error));
                gain_db = match (short, over) {
                    (Some((low, low_error)), Some((high, high_error))) => {
                        low + low_error / (low_error - high_error) * (high - low)
                    }
                    // Still short: continue the line through the last two
                    // gains, by at most twice what is left, since the line
                    // is shallowest where the limiter takes the most.
                    _ => {
                        let through = previous.filter(|(gain, _)| gain_db - gain > 1e-6).map_or(
                            1.0,
                            |(gain, previous_error)| {
                                ((previous_error - error) / (gain_db - gain)).clamp(0.5, 1.0)
                            },
                        );
                        gain_db + error / through
                    }
                };
            }
            limiter.apply(buffer, gain_db);
        }
    }
    if is_cancelled() {
        return Err(AudioMixError::Cancelled);
    }
    let mut output = measure_loudness(buffer);
    // The limiter holds the peaks it found around each frame, while the
    // gain moves under them; whatever still reaches over the ceiling is
    // taken off the whole mix.
    if let Some(peak) = output.true_peak_dbtp
        && peak > target.true_peak_dbtp
    {
        let excess = peak - target.true_peak_dbtp;
        scale(buffer, -excess);
        gain_db -= excess;
        output = measure_loudness(buffer);
    }
    Ok((gain_db, output))
}

/// How far from its target [`normalize_loudness`] leaves a mix the limiter
/// can bring there, LU.
pub const TARGET_TOLERANCE_LU: f64 = 0.1;

/// Passes of gain, limiting and measuring before the gain is taken as is.
/// Most mixes need two or three.
const MAX_LIMITER_PASSES: usize = 8;

/// How far ahead the limiter starts to turn a peak down: the gain ramps over
/// twice this, so a peak never meets a sudden step.
const LOOKAHEAD_SECONDS: f64 = 0.005;

/// How quickly the limiter lets the gain back up after a peak (time
/// constant).
const RELEASE_SECONDS: f64 = 0.1;

/// Frames per `rayon` task when finding peaks.
const PEAK_CHUNK_FRAMES: usize = 1 << 16;

/// A true-peak limiter for one buffer. It first finds, for every frame, the
/// highest oversampled peak within the lookahead on either side; the gain it
/// applies at a frame is then never more than what keeps each of those
/// peaks at the ceiling, smoothed so it ramps down ahead of a peak and
/// recovers after it.
struct Limiter {
    ceiling: f64,
    /// Highest true peak (all channels) within `lookahead` frames of each frame.
    peaks: Vec<f32>,
    lookahead: usize,
    release: f64,
}

impl Limiter {
    fn new(buffer: &AudioBuffer, ceiling: f64) -> Self {
        let rate = f64::from(buffer.sample_rate);
        let lookahead = ((LOOKAHEAD_SECONDS * rate).round() as usize).max(1);
        Self {
            ceiling,
            peaks: windowed_peaks(buffer, lookahead),
            lookahead,
            release: 1.0 - (-1.0 / (RELEASE_SECONDS * rate)).exp(),
        }
    }

    /// The loudness of `buffer` with `gain_db` and limiting, without
    /// changing it.
    fn loudness(&self, buffer: &AudioBuffer, gain_db: f64) -> Option<f64> {
        let channels = usize::from(buffer.channels);
        let mut meter = meter(buffer, Mode::I)?;
        let mut block = Vec::with_capacity(PEAK_CHUNK_FRAMES * channels);
        let mut gains = self.gains(gain_db);
        for frames in buffer.samples.chunks(PEAK_CHUNK_FRAMES * channels) {
            block.clear();
            for frame in frames.chunks_exact(channels) {
                let gain = gains.next().unwrap_or(0.0) as f32;
                block.extend(frame.iter().map(|sample| sample * gain));
            }
            meter.add_frames_f32(&block).ok()?;
        }
        meter.loudness_global().ok().filter(|lufs| lufs.is_finite())
    }

    fn apply(&self, buffer: &mut AudioBuffer, gain_db: f64) {
        let channels = usize::from(buffer.channels);
        for (frame, gain) in buffer
            .samples
            .chunks_exact_mut(channels)
            .zip(self.gains(gain_db))
        {
            for sample in frame {
                *sample *= gain as f32;
            }
        }
    }

    /// The linear gain of each frame: `gain_db`, turned down wherever that
    /// would take a peak over the ceiling.
    ///
    /// `held(n)` is the most gain frame `n` can take: `gain_db`, or less to
    /// keep every peak within the lookahead of it at the ceiling. Averaging
    /// `held` over the lookahead on either side keeps that guarantee, since
    /// every frame averaged covers frame `n`'s own peak, and ramps the gain
    /// down ahead of a peak; the release then only ever lowers it further.
    fn gains(&self, gain_db: f64) -> impl Iterator<Item = f64> + '_ {
        let gain = db_amplitude(gain_db);
        let frames = self.peaks.len();
        let held = move |frame: usize| {
            let peak = f64::from(self.peaks[frame]) * gain;
            if peak > self.ceiling {
                self.ceiling / peak
            } else {
                1.0
            }
        };
        let lookahead = self.lookahead;
        let mut sum: f64 = (0..lookahead.min(frames)).map(held).sum();
        let mut released = 1.0_f64;
        (0..frames).map(move |frame| {
            if frame + lookahead < frames {
                sum += held(frame + lookahead);
            }
            let first = frame.saturating_sub(lookahead);
            let last = (frame + lookahead).min(frames - 1);
            let average = (sum / (last - first + 1) as f64).min(1.0);
            if frame >= lookahead {
                sum -= held(frame - lookahead);
            }
            released = average.min(released + (1.0 - released) * self.release);
            gain * released
        })
    }
}

/// For each frame of `buffer`, the highest [`true_peak`] of any frame within
/// `lookahead` frames of it.
fn windowed_peaks(buffer: &AudioBuffer, lookahead: usize) -> Vec<f32> {
    let frames = buffer.frame_count();
    let mut windowed = vec![0.0_f32; frames];
    let taps = interpolation_taps();
    windowed
        .par_chunks_mut(PEAK_CHUNK_FRAMES)
        .enumerate()
        .for_each(|(index, chunk)| {
            let start = index * PEAK_CHUNK_FRAMES;
            let from = start.saturating_sub(lookahead);
            let to = (start + chunk.len() + lookahead).min(frames);
            let peaks: Vec<f32> = (from..to)
                .map(|frame| true_peak(buffer, frame, &taps))
                .collect();
            // A running maximum over each frame's window: `window` holds
            // frames whose peaks decrease, so its front is the maximum.
            let mut window = std::collections::VecDeque::new();
            let mut next = from;
            for (offset, out) in chunk.iter_mut().enumerate() {
                let frame = start + offset;
                let last = (frame + lookahead).min(frames - 1);
                while next <= last {
                    let peak = peaks[next - from];
                    while window
                        .back()
                        .is_some_and(|&back| peaks[back - from] <= peak)
                    {
                        window.pop_back();
                    }
                    window.push_back(next);
                    next += 1;
                }
                while window
                    .front()
                    .is_some_and(|&front| front + lookahead < frame)
                {
                    window.pop_front();
                }
                *out = window.front().map_or(0.0, |&front| peaks[front - from]);
            }
        });
    windowed
}

/// Points per frame the true peak is read at. BS.1770 asks for 4x, which
/// can miss a 20 kHz peak by 0.5 dB and reads mixes a few tenths lower than
/// a finer meter; at 8x the most a peak is missed by is 0.12 dB, so what the
/// limiter holds at the ceiling stays there on any meter.
const OVERSAMPLING: usize = 8;

/// Frames on either side of an interpolated point.
const HALF_TAPS: usize = 16;

/// The Kaiser window's shape: about 80 dB of stopband, as FFmpeg's
/// resampler uses (β 9) for the 192 kHz upsampling its `ebur128` reads.
const KAISER_BETA: f64 = 8.0;

/// Windowed-sinc taps for the points between one frame and the next:
/// `taps[phase][k]` weighs frame `n + k - (HALF_TAPS - 1)` for the point
/// `(phase + 1) / OVERSAMPLING` of the way from frame `n`.
type Taps = [[f32; 2 * HALF_TAPS]; OVERSAMPLING - 1];

fn interpolation_taps() -> Taps {
    let mut taps = [[0.0; 2 * HALF_TAPS]; OVERSAMPLING - 1];
    let edge = bessel_i0(KAISER_BETA);
    for (phase, taps) in taps.iter_mut().enumerate() {
        let offset = (phase + 1) as f64 / OVERSAMPLING as f64;
        for (k, tap) in taps.iter_mut().enumerate() {
            let distance = offset + (HALF_TAPS - 1) as f64 - k as f64;
            let x = std::f64::consts::PI * distance;
            let sinc = if distance == 0.0 { 1.0 } else { x.sin() / x };
            let position = distance / HALF_TAPS as f64;
            let window =
                bessel_i0(KAISER_BETA * (1.0 - position * position).max(0.0).sqrt()) / edge;
            *tap = (sinc * window) as f32;
        }
    }
    taps
}

/// The zeroth-order modified Bessel function of the first kind, for the
/// Kaiser window.
fn bessel_i0(x: f64) -> f64 {
    let (mut sum, mut term, mut k) = (1.0, 1.0, 1.0);
    while term > sum * 1e-12 {
        term *= (x / (2.0 * k)).powi(2);
        sum += term;
        k += 1.0;
    }
    sum
}

/// The highest absolute value of `frame` and the points interpolated between
/// it and the next frame, over every channel.
fn true_peak(buffer: &AudioBuffer, frame: usize, taps: &Taps) -> f32 {
    let channels = usize::from(buffer.channels);
    let frames = buffer.frame_count();
    let mut peak = 0.0_f32;
    for channel in 0..channels {
        // The frames the points are interpolated from; before the first and
        // after the last is silence.
        let mut around = [0.0_f32; 2 * HALF_TAPS];
        for (k, sample) in around.iter_mut().enumerate() {
            if let Some(at) = (frame + 1 + k)
                .checked_sub(HALF_TAPS)
                .filter(|&at| at < frames)
            {
                *sample = buffer.samples[at * channels + channel];
            }
        }
        peak = peak.max(around[HALF_TAPS - 1].abs());
        for taps in taps {
            let value: f32 = around
                .iter()
                .zip(taps)
                .map(|(sample, tap)| sample * tap)
                .sum();
            peak = peak.max(value.abs());
        }
    }
    peak
}

/// The highest [`true_peak`] of `buffer`.
fn highest_true_peak(buffer: &AudioBuffer) -> f32 {
    let taps = interpolation_taps();
    (0..buffer.frame_count())
        .into_par_iter()
        .with_min_len(PEAK_CHUNK_FRAMES)
        .map(|frame| true_peak(buffer, frame, &taps))
        .reduce(|| 0.0, f32::max)
}

fn meter(buffer: &AudioBuffer, mode: Mode) -> Option<EbuR128> {
    EbuR128::new(u32::from(buffer.channels), buffer.sample_rate, mode).ok()
}

fn scale(buffer: &mut AudioBuffer, gain_db: f64) {
    let gain = db_amplitude(gain_db) as f32;
    for sample in &mut buffer.samples {
        *sample *= gain;
    }
}

fn db_amplitude(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

fn amplitude_db(amplitude: f64) -> f64 {
    20.0 * amplitude.log10()
}
