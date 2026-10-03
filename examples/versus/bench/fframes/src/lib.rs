//! NEBULA benchmark scene, fframes version. See ../../SCENE.md for the spec that the
//! Celesta and Remotion versions implement with the same math.
use fframes::{AudioMap, Color, Duration, FFramesContext, Frame, Svgr, Video, include_media_dir};
use std::f64::consts::TAU;

// Fonts are shared with the Celesta and Remotion versions.
include_media_dir!(pub struct NebulaFframesMedia, "../assets/fonts");

pub const WIDTH: usize = 1920;
pub const HEIGHT: usize = 1080;
const FPS: usize = 60;
const FRAMES: usize = 600;

const BLOBS: [&str; 6] = ["#FF3D7F", "#3DA5FF", "#7B5CFF", "#00E0B8", "#FFB13D", "#FF6B3D"];
const STARS: [&str; 4] = ["#FFFFFF", "#9EC9FF", "#FFC2E0", "#B8FFF0"];

fn rand(i: usize, k: usize) -> f64 {
    let v = (i as f64 * 12.9898 + k as f64 * 78.233).sin() * 43758.5453;
    v - v.floor()
}

fn mix(a: &str, b: &str, k: f64) -> String {
    let ch = |s: &str, o: usize| u8::from_str_radix(&s[o..o + 2], 16).unwrap() as f64;
    let c = |o: usize| (ch(a, o) + (ch(b, o) - ch(a, o)) * k).round() as u8;
    format!("#{:02X}{:02X}{:02X}", c(1), c(3), c(5))
}

pub struct NebulaFframesVideo<'a> {
    pub media: &'a NebulaFframesMedia,
}

impl<'a> NebulaFframesVideo<'a> {
    pub fn new(media: &'a NebulaFframesMedia) -> Self {
        Self { media }
    }
}

fn blobs<'a>(t: f64) -> Vec<Svgr<'a>> {
    BLOBS
        .iter()
        .enumerate()
        .map(|(j, color)| {
            let j = j as f64;
            fframes::svgr!(<circle cx={960. + (t * 0.35 + j * 1.047).cos() * 520.}
                cy={540. + (t * 0.5 + j * 1.3).sin() * 260.} r={260. + 60. * (t * 0.8 + j).sin()}
                fill={*color} opacity="0.55" filter="url(#blur)" style="mix-blend-mode:screen" />)
        })
        .collect()
}

fn rings<'a>(t: f64) -> Vec<Svgr<'a>> {
    (0..24)
        .map(|k| {
            let rx = 180. + k as f64 * 30.;
            let deg = k as f64 * 7.5 + t * if k % 2 == 1 { -12. } else { 12. };
            fframes::svgr!(<ellipse rx={rx} ry={rx * 0.38} fill="none" stroke="#8FB8FF" stroke-width="1.5"
                transform={format!("translate(960 540) rotate({deg})")}
                opacity={0.18 + 0.22 * (0.5 + 0.5 * (t * 2. + k as f64 * 0.4).sin())} />)
        })
        .collect()
}

fn particles<'a>(t: f64) -> Vec<Svgr<'a>> {
    (0..1500)
        .map(|i| {
            let r = 60. + rand(i, 1) * 1000.;
            let dir = if rand(i, 4) < 0.5 { -1. } else { 1. };
            let a = rand(i, 2) * TAU + t * (0.1 + rand(i, 3) * 0.4) * dir;
            let size = 2. + rand(i, 3) * 6.;
            let twinkle = 0.3 + 0.7 * (0.5 + 0.5 * (t * (2. + rand(i, 5) * 4.) + i as f64).sin());
            fframes::svgr!(<circle cx={960. + a.cos() * r} cy={540. + a.sin() * r * 0.45} r={size / 2.}
                fill={STARS[i % 4]} opacity={twinkle} />)
        })
        .collect()
}

fn spectrum<'a>(t: f64) -> Vec<Svgr<'a>> {
    (0..80)
        .map(|k| {
            let k = k as f64;
            let h = 20. + 180. * ((t * 2.1 + k * 0.27).sin() * (t * 1.3 + k * 0.11).cos()).abs();
            fframes::svgr!(<rect x={163. + k * 20.} y={1040. - h} width="14" height={h} rx="4" fill="url(#bar)" />)
        })
        .collect()
}

fn hud<'a>(t: f64, frame: usize) -> Vec<Svgr<'a>> {
    let mut lines: Vec<Svgr<'a>> = (0..48)
        .map(|j| {
            let v = (t * (1. + j as f64 * 0.13) + j as f64).sin() * 100.;
            let sign = if v < 0. { '-' } else { '+' };
            let label = format!("CH{j:02} {sign}{:.3}", v.abs());
            // Celesta places the top of the glyphs at y; the baseline sits a cap height below.
            fframes::svgr!(<text x={if j < 24 { 40 } else { 1720 }} y={73 + (j % 24) * 30}
                font-family="IBM Plex Mono" font-size="18" fill="#9EC9FF">{label}</text>)
        })
        .collect();
    lines.push(fframes::svgr!(<text x="1880" y="1060" text-anchor="end" font-family="IBM Plex Mono"
        font-size="18" fill="#9EC9FF">{format!("FRAME {frame:04} / {FRAMES}")}</text>));
    lines
}

impl Video for NebulaFframesVideo<'_> {
    const FPS: usize = FPS;
    const WIDTH: usize = WIDTH;
    const HEIGHT: usize = HEIGHT;
    const BACKGROUND_COLOR: Color = Color::BLACK;

    fn duration(&self) -> Duration<'_> {
        Duration::Frames(FRAMES)
    }

    fn audio(&self) -> AudioMap<'_> {
        AudioMap::none()
    }

    fn render_frame<'a>(&'a self, frame: Frame, _ctx: &FFramesContext<'a, '_>) -> Svgr<'a> {
        let t = frame.index as f64 / FPS as f64;
        let k = 0.5 + 0.5 * (t * 0.6).sin();
        let scale = 1. + 0.03 * (t * 2.).sin();

        fframes::svgr!(
            <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1920 1080" width={WIDTH} height={HEIGHT}>
                <defs>
                    <linearGradient id="bg" x1="0" y1="0" x2="0" y2="1">
                        <stop offset="0" stop-color={mix("#0B1026", "#1A0B2E", k)} />
                        <stop offset="1" stop-color={mix("#04050C", "#0B1A2E", k)} />
                    </linearGradient>
                    <linearGradient id="bar" x1="0" y1="1" x2="0" y2="0">
                        <stop offset="0" stop-color="#3DA5FF" />
                        <stop offset="1" stop-color="#FF3D7F" />
                    </linearGradient>
                    <filter id="blur" x="-50%" y="-50%" width="200%" height="200%">
                        <feGaussianBlur stdDeviation="48" />
                    </filter>
                    <filter id="glow" x="-50%" y="-50%" width="200%" height="200%">
                        <feDropShadow dx="0" dy="0" stdDeviation="24" flood-color="#7B5CFF" />
                    </filter>
                </defs>
                <rect width="1920" height="1080" fill="url(#bg)" />
                {blobs(t)}
                {rings(t)}
                {particles(t)}
                {spectrum(t)}
                <g transform={format!("translate(960 470) scale({scale})")} filter="url(#glow)">
                    <text y="77" text-anchor="middle" font-family="Bebas Neue" font-size="220"
                        letter-spacing={20. + 10. * t.sin()} fill="#FFFFFF">"NEBULA"</text>
                    <text y="158" text-anchor="middle" font-family="IBM Plex Mono" font-size="24"
                        fill="#C8D6FF">"1500 PARTICLES / 24 RINGS / 80 BARS / 6 BLURS"</text>
                </g>
                <g opacity="0.8">{hud(t, frame.index)}</g>
            </svg>
        )
    }
}
