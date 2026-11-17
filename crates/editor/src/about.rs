//! The About Celesta window: the app's name, version, license, and
//! repository, under a small timeline strip.
//!
//! One About window exists at a time; the menu item brings an open one to
//! the front. It is the app's only window that is not an editor, so it
//! closes with the last editor window (see [`close_when_left_alone`]).
//!
//! Under the version are the system, the GPU, and Node.js: what a bug report
//! needs. The version copies itself and those rows when pressed. Pressed [`PRESSES_TO_PLAY`] times
//! in a row, it plays the strip: a playhead crosses it and the clips light as
//! it passes. With reduced motion the clips light without moving.

use crate::actions::{AboutCelesta, CloseWindow};
use crate::view::EditorView;
use celesta_editor_theme::{self as theme, metrics};
use celesta_project::TrackKind;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::link::Link;
use gpui_kit::component::{
    ActiveTheme as _, IconName, Placement, Root, Sizable as _, TitleBar, h_flex, v_flex,
};
use gpui_kit::prelude::*;
use gpui_kit::{
    App, Bounds, ClipboardItem, Context, FocusHandle, FontWeight, Global, SharedString, Task,
    Window, WindowBounds, WindowHandle, WindowOptions, div, px, relative, size,
};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const VERSION: &str = env!("CARGO_PKG_VERSION");
const LICENSE: &str = env!("CARGO_PKG_LICENSE");
const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

/// Presses of the version, each within [`PRESS_GAP`] of the one before, that
/// play the strip.
const PRESSES_TO_PLAY: u32 = 7;
const PRESS_GAP: Duration = Duration::from_millis(600);
/// How long the version shows a check after it was copied.
const COPIED_NOTE: Duration = Duration::from_millis(1500);

/// One crossing of the strip by the playhead, and how many it makes.
const SWEEP: Duration = Duration::from_millis(2400);
const SWEEPS: u32 = 2;

/// How far behind the playhead a clip is still fading, as a share of the
/// strip's width.
const GLOW_TAIL: f32 = 0.3;
/// How much of a clip's start lifts it as the playhead enters, as a share of
/// the strip's width.
const LIFT_SPAN: f32 = 0.1;
/// A resting clip's opacity; a lit one is opaque.
const REST_OPACITY: f32 = 0.4;

/// A clip of the strip: its lane's kind, and its start and length as shares
/// of the strip's width.
struct Clip {
    kind: TrackKind,
    start: f32,
    length: f32,
}

const fn clip(kind: TrackKind, start: f32, length: f32) -> Clip {
    Clip {
        kind,
        start,
        length,
    }
}

/// The strip's lanes, top first.
const LANES: [&[Clip]; 4] = [
    &[
        clip(TrackKind::Overlay, 0.10, 0.20),
        clip(TrackKind::Overlay, 0.48, 0.28),
    ],
    &[
        clip(TrackKind::Dialogue, 0.14, 0.18),
        clip(TrackKind::Dialogue, 0.40, 0.14),
        clip(TrackKind::Dialogue, 0.70, 0.20),
    ],
    &[
        clip(TrackKind::Video, 0.02, 0.34),
        clip(TrackKind::Video, 0.37, 0.29),
        clip(TrackKind::Video, 0.67, 0.31),
    ],
    &[
        clip(TrackKind::Audio, 0.02, 0.60),
        clip(TrackKind::Audio, 0.64, 0.34),
    ],
];

/// Counts presses that follow one another closely.
#[derive(Default)]
struct PressStreak {
    count: u32,
    last: Option<Instant>,
}

impl PressStreak {
    /// Counts a press at `now` and returns the streak's length: 1 for a
    /// press that starts a streak.
    fn press(&mut self, now: Instant) -> u32 {
        let continues = self
            .last
            .is_some_and(|last| now.saturating_duration_since(last) <= PRESS_GAP);
        self.count = if continues { self.count + 1 } else { 1 };
        self.last = Some(now);
        self.count
    }

    fn reset(&mut self) {
        *self = Self::default();
    }
}

/// Where the playhead is `elapsed` after the strip started playing, as a
/// share of the strip's width, or `None` once it has finished.
fn playhead_at(elapsed: Duration) -> Option<f32> {
    if elapsed >= SWEEP * SWEEPS {
        return None;
    }
    Some((elapsed.as_secs_f32() / SWEEP.as_secs_f32()).fract())
}

/// How lit a clip is with the playhead at `playhead`: 1 under it, fading to
/// 0 over [`GLOW_TAIL`] behind it, 0 ahead of it.
fn glow(clip: &Clip, playhead: f32) -> f32 {
    let end = clip.start + clip.length;
    if playhead < clip.start {
        0.0
    } else if playhead <= end {
        1.0
    } else {
        (1.0 - (playhead - end) / GLOW_TAIL).clamp(0.0, 1.0)
    }
}

/// How far a clip is lifted with the playhead at `playhead`, 0 to 1: it
/// rises and settles as the playhead crosses its first [`LIFT_SPAN`].
fn lift(clip: &Clip, playhead: f32) -> f32 {
    let entered = (playhead - clip.start) / LIFT_SPAN;
    if (0.0..=1.0).contains(&entered) {
        (entered * std::f32::consts::PI).sin()
    } else {
        0.0
    }
}

/// What the machine says about itself, read off the main thread.
#[derive(Clone, Debug, PartialEq, Eq)]
struct System {
    /// The system's name and version, such as "macOS 15.5".
    os: String,
    /// The version of the Node.js that runs React compositions, or `None`
    /// when it did not answer.
    node: Option<String>,
}

impl System {
    /// Asks the system and `node`. Both run a program, so this blocks.
    fn read(node: &Path) -> Self {
        Self {
            os: os_name(),
            node: answer(Command::new(node).arg("--version"))
                .map(|version| version.trim_start_matches('v').to_owned()),
        }
    }
}

/// The first line a program prints, or `None` when it fails or prints none.
fn answer(command: &mut Command) -> Option<String> {
    let output = celesta_plugin_host::without_console_window(command)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())?;
    let text = String::from_utf8_lossy(&output.stdout);
    let line = text.lines().map(str::trim).find(|line| !line.is_empty())?;
    Some(line.to_owned())
}

/// The system's name with its version, or the name alone when the system
/// does not tell.
fn os_name() -> String {
    #[cfg(target_os = "macos")]
    let name = answer(Command::new("sw_vers").arg("-productVersion"))
        .map_or_else(|| "macOS".to_owned(), |version| format!("macOS {version}"));
    // `ver` prints "Microsoft Windows [Version 10.0.26100.4349]".
    #[cfg(target_os = "windows")]
    let name = answer(Command::new("cmd").args(["/C", "ver"])).map_or_else(
        || "Windows".to_owned(),
        |line| line.replace("Microsoft ", "").replace(['[', ']'], ""),
    );
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let name = std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|release| pretty_name(&release))
        .unwrap_or_else(|| "Linux".to_owned());
    name
}

/// `PRETTY_NAME` of an `os-release` file, without its quotes.
#[cfg_attr(any(target_os = "macos", target_os = "windows"), cfg(test))]
fn pretty_name(release: &str) -> Option<String> {
    let name = release
        .lines()
        .find_map(|line| line.strip_prefix("PRETTY_NAME="))?
        .trim()
        .trim_matches('"');
    (!name.is_empty()).then(|| name.to_owned())
}

/// The rows under the version, as label and value: the system, the GPU the
/// preview renders with, and Node.js. A row still being read shows `…`.
fn detail_rows(system: Option<&System>, gpu: Option<&str>) -> Vec<(&'static str, String)> {
    let mut rows = vec![(
        "OS",
        system.map_or_else(
            || "…".to_owned(),
            |system| format!("{} ({})", system.os, std::env::consts::ARCH),
        ),
    )];
    if let Some(gpu) = gpu {
        rows.push(("GPU", gpu.to_owned()));
    }
    rows.push((
        "Node.js",
        match system {
            None => "…".to_owned(),
            Some(system) => system
                .node
                .clone()
                .unwrap_or_else(|| "not found".to_owned()),
        },
    ));
    rows
}

/// What pressing the version copies: the version and the rows, one a line,
/// to paste into a bug report.
fn report(rows: &[(&'static str, String)]) -> String {
    std::iter::once(format!("Celesta: {VERSION}"))
        .chain(
            rows.iter()
                .map(|(label, value)| format!("{label}: {value}")),
        )
        .collect::<Vec<_>>()
        .join("\n")
}

/// The GPU an editor window renders its preview with, as its status bar
/// names it.
fn editor_gpu(cx: &App) -> Option<SharedString> {
    cx.windows()
        .into_iter()
        .filter_map(|window| window.downcast::<Root>())
        .find_map(|root| {
            let editor = root
                .read(cx)
                .ok()?
                .view()
                .clone()
                .downcast::<EditorView>()
                .ok()?;
            Some(editor.read(cx).gpu_name.clone())
        })
}

/// The open About window, if any.
#[derive(Default)]
struct AboutWindow(Option<WindowHandle<Root>>);

impl Global for AboutWindow {}

/// Handles About Celesta wherever the focus is.
pub(crate) fn init(cx: &mut App) {
    // Deferred: the action arrives through the focused editor window, which
    // cannot be read for its GPU until the dispatch is over.
    cx.on_action(|_: &AboutCelesta, cx| cx.defer(show));
}

/// Opens the About window, or brings the open one to the front.
pub(crate) fn show(cx: &mut App) {
    let open = cx.default_global::<AboutWindow>().0;
    if let Some(handle) = open
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let gpu = editor_gpu(cx);
    // physical: platform window sizes.
    let bounds = Bounds::centered(None, size(px(400.0), px(420.0)), cx);
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                is_resizable: false,
                is_minimizable: false,
                ..TitleBar::window_options()
            },
            |window, cx| {
                let view = cx.new(|cx| AboutView::new(gpu, window, cx));
                cx.new(|cx| Root::new(view, window, cx))
            },
        )
        .expect("could not open the About window");
    cx.set_global(AboutWindow(Some(handle)));
}

/// Closes the About window when it is the only window left, so the app
/// quits with its last editor window.
pub(crate) fn close_when_left_alone(cx: &mut App) {
    let Some(handle) = cx.default_global::<AboutWindow>().0 else {
        return;
    };
    let windows = cx.windows();
    if windows.len() == 1 && windows[0] == handle.into() {
        // The handle of a window already closed fails; there is nothing to do.
        let _ = handle.update(cx, |_, window, _| window.remove_window());
    }
}

struct AboutView {
    focus_handle: FocusHandle,
    streak: PressStreak,
    /// When the strip started playing.
    playing_since: Option<Instant>,
    /// Runs while the version shows its check, and ends that.
    copied_note: Option<Task<()>>,
    gpu: Option<SharedString>,
    /// `None` until the system and Node.js have answered.
    system: Option<System>,
}

impl AboutView {
    fn new(gpu: Option<SharedString>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let (node, _) = celesta_react_bridge::runtime_paths();
        let read = cx
            .background_executor()
            .spawn(async move { System::read(&node) });
        cx.spawn(async move |this, cx| {
            let system = read.await;
            this.update(cx, |this, cx| {
                this.system = Some(system);
                cx.notify();
            })
            .ok();
        })
        .detach();
        Self {
            focus_handle,
            streak: PressStreak::default(),
            playing_since: None,
            copied_note: None,
            gpu,
            system: None,
        }
    }

    fn detail_rows(&self) -> Vec<(&'static str, String)> {
        detail_rows(self.system.as_ref(), self.gpu.as_deref())
    }

    fn close_window_action(&mut self, _: &CloseWindow, window: &mut Window, _: &mut Context<Self>) {
        window.remove_window();
    }

    /// The version was pressed: the first press of a streak copies the
    /// report, and a full streak plays the strip.
    fn press_version(&mut self, cx: &mut Context<Self>) {
        let presses = self.streak.press(Instant::now());
        if presses == 1 {
            cx.write_to_clipboard(ClipboardItem::new_string(report(&self.detail_rows())));
            // A note already showing is replaced, so it lasts from this press.
            self.copied_note = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(COPIED_NOTE).await;
                this.update(cx, |this, cx| {
                    this.copied_note = None;
                    cx.notify();
                })
                .ok();
            }));
        }
        if presses >= PRESSES_TO_PLAY {
            self.streak.reset();
            // Pressing on while it plays leaves it playing.
            self.playing_since.get_or_insert_with(Instant::now);
        }
        cx.notify();
    }

    /// The strip of clips, and the playhead while it plays.
    fn strip(&self, playhead: Option<f32>, cx: &App) -> impl IntoElement {
        let still = cx.reduce_motion();
        let lanes = LANES.iter().map(|lane| {
            div()
                .relative()
                .w_full()
                .h(metrics::scaled(12.))
                .children(lane.iter().map(|clip| {
                    let (glow, lift) = match playhead {
                        Some(_) if still => (1.0, 0.0),
                        Some(playhead) => (glow(clip, playhead), lift(clip, playhead)),
                        None => (0.0, 0.0),
                    };
                    let opacity = REST_OPACITY + (1.0 - REST_OPACITY) * glow;
                    div()
                        .absolute()
                        .left(relative(clip.start))
                        .w(relative(clip.length))
                        .top(metrics::scaled_px(-3. * lift))
                        .h_full()
                        .rounded_sm()
                        .bg(theme::clip_label_fill(clip.kind).opacity(opacity))
                }))
        });
        div()
            .debug_selector(|| "about-strip".into())
            .relative()
            .w_full()
            .p_3()
            .bg(cx.theme().title_bar)
            .border_b_1()
            .border_color(cx.theme().border)
            .child(v_flex().relative().gap_1().children(lanes).when_some(
                playhead.filter(|_| !still),
                |strip, playhead| {
                    strip.child(
                        div()
                            .debug_selector(|| "about-playhead".into())
                            .absolute()
                            .left(relative(playhead))
                            .top_0()
                            .bottom_0()
                            .w(px(1.))
                            .bg(theme::accent()),
                    )
                },
            ))
    }
}

impl Render for AboutView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let playhead = self
            .playing_since
            .and_then(|since| playhead_at(since.elapsed()));
        if playhead.is_some() {
            window.request_animation_frame();
        } else {
            self.playing_since = None;
        }
        window.set_window_title("About Celesta");
        let muted = cx.theme().muted_foreground;
        v_flex()
            .key_context("CelestaAbout")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::close_window_action))
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(TitleBar::new())
            .child(self.strip(playhead, cx))
            .child(
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .gap_1()
                    .p_4()
                    .child(
                        div()
                            .text_2xl()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme::accent())
                            .child("Celesta"),
                    )
                    .child(
                        Button::new("about-version")
                            .debug_selector(|| "about-version".into())
                            .ghost()
                            .small()
                            // The check joins the label, which stays under the
                            // pointer for the next press.
                            .when(self.copied_note.is_some(), |button| {
                                button.icon(IconName::Check)
                            })
                            .label(format!("Version {VERSION}"))
                            // Beside the version: above it is the name, below
                            // it the rows the press copies.
                            .tooltip("Copy details")
                            .tooltip_placement(Placement::Right)
                            .on_click(cx.listener(|this, _, _, cx| this.press_version(cx))),
                    )
                    .child(
                        div()
                            .text_base()
                            .text_center()
                            .child("A code-first video editor."),
                    )
                    .child(
                        v_flex()
                            .debug_selector(|| "about-details".into())
                            .mt_2()
                            .gap_0p5()
                            .text_sm()
                            .children(self.detail_rows().into_iter().map(|(label, value)| {
                                h_flex()
                                    .gap_2()
                                    .child(
                                        div()
                                            .flex_none()
                                            .w(metrics::scaled(56.))
                                            .text_color(muted)
                                            .child(label),
                                    )
                                    .child(div().child(value))
                            })),
                    )
                    .child(
                        div()
                            .mt_2()
                            .text_sm()
                            .text_color(muted)
                            .child(format!("Licensed under {LICENSE}")),
                    )
                    .child(
                        Link::new("about-repository")
                            .href(REPOSITORY)
                            .text_sm()
                            .child(REPOSITORY.trim_start_matches("https://")),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};

    #[test]
    fn close_presses_make_a_streak_and_a_pause_starts_another() {
        let start = Instant::now();
        let mut streak = PressStreak::default();
        assert_eq!(streak.press(start), 1);
        assert_eq!(streak.press(start + PRESS_GAP), 2);
        assert_eq!(streak.press(start + PRESS_GAP * 2), 3);
        let late = start + PRESS_GAP * 3 + Duration::from_millis(1);
        assert_eq!(streak.press(late), 1);
    }

    #[test]
    fn the_playhead_crosses_the_strip_then_stops() {
        assert_eq!(playhead_at(Duration::ZERO), Some(0.0));
        assert_eq!(playhead_at(SWEEP / 2), Some(0.5));
        // The second crossing starts from the left again.
        assert_eq!(playhead_at(SWEEP + SWEEP / 4), Some(0.25));
        assert_eq!(playhead_at(SWEEP * SWEEPS), None);
    }

    #[test]
    fn a_clip_lights_under_the_playhead_and_fades_behind_it() {
        let clip = clip(TrackKind::Video, 0.2, 0.2);
        assert_eq!(glow(&clip, 0.1), 0.0);
        assert_eq!(glow(&clip, 0.3), 1.0);
        let fading = glow(&clip, 0.4 + GLOW_TAIL / 2.0);
        assert!((fading - 0.5).abs() < 1e-4, "{fading}");
        assert_eq!(glow(&clip, 0.4 + GLOW_TAIL), 0.0);
    }

    #[test]
    fn a_clip_lifts_only_as_the_playhead_enters_it() {
        let clip = clip(TrackKind::Video, 0.2, 0.4);
        assert_eq!(lift(&clip, 0.1), 0.0);
        let peak = lift(&clip, 0.2 + LIFT_SPAN / 2.0);
        assert!((peak - 1.0).abs() < 1e-4, "{peak}");
        assert_eq!(lift(&clip, 0.5), 0.0);
    }

    #[test]
    fn the_report_lists_the_version_and_each_row() {
        let system = System {
            os: "macOS 15.5".to_owned(),
            node: Some("24.11.0".to_owned()),
        };
        let rows = detail_rows(Some(&system), Some("Apple M4 (Metal)"));
        assert_eq!(
            report(&rows),
            format!(
                "Celesta: {VERSION}\nOS: macOS 15.5 ({})\nGPU: Apple M4 (Metal)\nNode.js: 24.11.0",
                std::env::consts::ARCH
            )
        );
    }

    #[test]
    fn rows_say_what_is_still_read_missing_or_unknown() {
        let labels = |rows: Vec<(&'static str, String)>| -> Vec<String> {
            rows.into_iter()
                .map(|(label, value)| format!("{label}: {value}"))
                .collect()
        };
        assert_eq!(labels(detail_rows(None, None)), ["OS: …", "Node.js: …"]);
        let system = System {
            os: "Linux".to_owned(),
            node: None,
        };
        assert_eq!(
            labels(detail_rows(Some(&system), None))[1],
            "Node.js: not found"
        );
    }

    #[test]
    fn a_program_that_does_not_run_gives_no_answer() {
        assert_eq!(answer(&mut Command::new("celesta-no-such-program")), None);
    }

    #[test]
    fn the_pretty_name_loses_its_quotes() {
        let release = "NAME=\"Ubuntu\"\nPRETTY_NAME=\"Ubuntu 24.04.2 LTS\"\nID=ubuntu\n";
        assert_eq!(pretty_name(release).as_deref(), Some("Ubuntu 24.04.2 LTS"));
        assert_eq!(pretty_name("NAME=Arch\n"), None);
    }

    #[test]
    fn every_clip_stays_inside_the_strip() {
        for clip in LANES.iter().flat_map(|lane| lane.iter()) {
            assert!(clip.start >= 0.0 && clip.start + clip.length <= 1.0);
        }
    }

    fn set_up(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_kit::init(cx);
            theme::init(cx);
            init(cx);
        });
    }

    fn about_window(cx: &mut TestAppContext) -> WindowHandle<Root> {
        cx.update(|cx| {
            show(cx);
            cx.global::<AboutWindow>().0.expect("the About window")
        })
    }

    #[gpui_kit::test]
    fn showing_it_twice_keeps_one_window(cx: &mut TestAppContext) {
        set_up(cx);
        cx.update(show);
        cx.update(show);
        assert_eq!(cx.windows().len(), 1);
    }

    #[gpui_kit::test]
    fn it_opens_again_after_it_was_closed(cx: &mut TestAppContext) {
        set_up(cx);
        let first = about_window(cx);
        first
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        cx.run_until_parked();
        assert!(cx.windows().is_empty());
        cx.update(show);
        assert_eq!(cx.windows().len(), 1);
    }

    #[gpui_kit::test]
    fn it_closes_when_it_is_the_only_window_left(cx: &mut TestAppContext) {
        set_up(cx);
        about_window(cx);
        cx.update(close_when_left_alone);
        cx.run_until_parked();
        assert!(cx.windows().is_empty());
    }

    #[gpui_kit::test]
    fn it_stays_open_beside_another_window(cx: &mut TestAppContext) {
        set_up(cx);
        cx.add_empty_window();
        about_window(cx);
        cx.update(close_when_left_alone);
        cx.run_until_parked();
        assert_eq!(cx.windows().len(), 2);
    }

    #[gpui_kit::test]
    fn close_window_closes_it(cx: &mut TestAppContext) {
        set_up(cx);
        let handle = about_window(cx);
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        cx.dispatch_action(CloseWindow);
        cx.run_until_parked();
        assert!(cx.windows().is_empty());
    }

    #[gpui_kit::test]
    fn pressing_the_version_copies_it(cx: &mut TestAppContext) {
        set_up(cx);
        let handle = about_window(cx);
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        let version = cx.debug_bounds("about-version").expect("the version");
        cx.simulate_click(version.center(), Modifiers::none());
        let copied_text = cx
            .read_from_clipboard()
            .and_then(|item| item.text())
            .expect("the report");
        let lines: Vec<&str> = copied_text.lines().collect();
        assert_eq!(lines[0], format!("Celesta: {VERSION}"));
        assert!(lines[1].starts_with("OS: "), "{copied_text}");
        // No editor window is open, so no GPU is named.
        assert!(lines[2].starts_with("Node.js: "), "{copied_text}");
        assert_eq!(lines.len(), 3);
        assert!(cx.debug_bounds("about-playhead").is_none());
        let copied = |cx: &mut VisualTestContext| {
            handle
                .update(cx, |root, _, cx| {
                    let view = root.view().clone().downcast::<AboutView>().unwrap();
                    view.read(cx).copied_note.is_some()
                })
                .unwrap()
        };
        assert!(copied(cx), "the version shows its check");
        cx.executor().advance_clock(COPIED_NOTE);
        cx.run_until_parked();
        assert!(!copied(cx), "and loses it again");
    }

    #[gpui_kit::test]
    fn pressing_the_version_seven_times_plays_the_strip(cx: &mut TestAppContext) {
        set_up(cx);
        let handle = about_window(cx);
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        let version = cx.debug_bounds("about-version").expect("the version");
        for _ in 0..PRESSES_TO_PLAY {
            cx.simulate_click(version.center(), Modifiers::none());
        }
        cx.run_until_parked();
        assert!(cx.debug_bounds("about-playhead").is_some());
    }

    #[gpui_kit::test]
    fn reduced_motion_lights_the_strip_without_a_playhead(cx: &mut TestAppContext) {
        set_up(cx);
        cx.update(|cx| cx.set_reduce_motion(true));
        let handle = about_window(cx);
        let cx = &mut VisualTestContext::from_window(handle.into(), cx);
        cx.run_until_parked();
        let version = cx.debug_bounds("about-version").expect("the version");
        for _ in 0..PRESSES_TO_PLAY {
            cx.simulate_click(version.center(), Modifiers::none());
        }
        cx.run_until_parked();
        assert!(cx.debug_bounds("about-strip").is_some());
        assert!(cx.debug_bounds("about-playhead").is_none());
    }
}
