//! CLI-only progress presentation. Exporters and their event contract stay independent of the TUI.

use std::collections::VecDeque;
use std::io::{self, IsTerminal};
use std::path::Path;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, Ordering},
    mpsc,
};
use std::thread;
use std::time::{Duration, Instant};

use celesta_exporter::{
    ExportError, ExportProgress, RenderQuality, StillFormat, VideoEncoding, VideoFormat,
};
use ratatui::{
    Frame, Terminal, TerminalOptions, Viewport,
    backend::CrosstermBackend,
    layout::{Constraint, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Gauge, Paragraph, Sparkline, Wrap},
};

const TICK: Duration = Duration::from_millis(100);
const ACCENT: Color = Color::Rgb(170, 153, 255);
const BACKGROUND: Color = Color::Rgb(17, 20, 29);
const MUTED: Color = Color::Rgb(126, 137, 158);
const BORDER: Color = Color::Rgb(51, 61, 80);
const CYAN: Color = Color::Rgb(104, 219, 213);
const SAMPLE_INTERVAL: Duration = Duration::from_millis(500);

pub struct Settings<'a> {
    pub source: &'a Path,
    pub output: &'a Path,
    pub react: bool,
    /// `Some` for a still export, in this format.
    pub still: Option<StillFormat>,
    /// The JPEG or WebP quality a still export uses.
    pub still_quality: u8,
    pub format: VideoFormat,
    pub video: VideoEncoding,
    pub render_quality: RenderQuality,
    /// The property file in effect: `props.json (auto)` or the `--props-file` name.
    pub props: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Stage {
    Preparing,
    Rendering,
    Mixing,
    Muxing,
    Complete,
    Failed,
}

impl Stage {
    fn index(self) -> Option<usize> {
        match self {
            Self::Preparing => Some(0),
            Self::Rendering => Some(1),
            Self::Mixing => Some(2),
            Self::Muxing => Some(3),
            Self::Complete | Self::Failed => None,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Preparing => "Preparing composition",
            Self::Rendering => "Rendering frames",
            Self::Mixing => "Mixing audio",
            Self::Muxing => "Muxing",
            Self::Complete => "Export complete",
            Self::Failed => "Export failed",
        }
    }
}

enum Diagnostic {
    Warning(String),
    /// Says what it is (`note: …`).
    Note(String),
}

impl Diagnostic {
    fn line(&self) -> String {
        match self {
            Self::Warning(warning) => format!("warning: {warning}"),
            Self::Note(note) => note.clone(),
        }
    }
}

pub(crate) struct State {
    stage: Stage,
    active_stage: Stage,
    stage_started: Instant,
    stage_durations: [Option<Duration>; 4],
    speed_samples: VecDeque<u64>,
    sample_started: Instant,
    sampled_frames: u64,
    current_fps: Option<f64>,
    started: Instant,
    rendering_started: Option<Instant>,
    rendering_finished: Option<Instant>,
    finished: Option<Instant>,
    frame: u64,
    total: u64,
    /// Warnings and notes in the order they arrived. The panels list only
    /// the warnings; all of them are printed after the display closes.
    diagnostics: Vec<Diagnostic>,
    /// How many of `diagnostics` are already printed.
    printed: usize,
    error: Option<String>,
}

impl State {
    pub(crate) fn new() -> Self {
        let now = Instant::now();
        Self {
            stage: Stage::Preparing,
            active_stage: Stage::Preparing,
            stage_started: now,
            stage_durations: [None; 4],
            speed_samples: VecDeque::with_capacity(60),
            sample_started: now,
            sampled_frames: 0,
            current_fps: None,
            started: now,
            rendering_started: None,
            rendering_finished: None,
            finished: None,
            frame: 0,
            total: 0,
            diagnostics: Vec::new(),
            printed: 0,
            error: None,
        }
    }

    pub(crate) fn update(&mut self, event: ExportProgress) {
        match event {
            ExportProgress::Rendering { frame, total } => {
                if self.stage != Stage::Rendering {
                    self.transition(Stage::Rendering);
                    self.sample_started = Instant::now();
                }
                self.rendering_started.get_or_insert_with(Instant::now);
                self.frame = frame;
                self.total = total;
            }
            ExportProgress::MixingAudio => self.end_rendering(Stage::Mixing),
            ExportProgress::Muxing => self.end_rendering(Stage::Muxing),
            ExportProgress::Warning(warning) => {
                self.diagnostics.push(Diagnostic::Warning(warning));
            }
            ExportProgress::Note(note) => self.diagnostics.push(Diagnostic::Note(note)),
            ExportProgress::Composition(_)
            | ExportProgress::ReactCompositions { .. }
            | ExportProgress::Wrote(_) => {}
        }
    }

    /// The warnings, oldest first.
    fn warnings(&self) -> impl DoubleEndedIterator<Item = &str> {
        self.diagnostics
            .iter()
            .filter_map(|diagnostic| match diagnostic {
                Diagnostic::Warning(warning) => Some(warning.as_str()),
                Diagnostic::Note(_) => None,
            })
    }

    /// The warnings and notes not printed yet, in arrival order, as the
    /// plain output prints them; they count as printed from now on.
    fn take_scrollback(&mut self) -> Vec<String> {
        let lines = self.diagnostics[self.printed..]
            .iter()
            .map(Diagnostic::line)
            .collect();
        self.printed = self.diagnostics.len();
        lines
    }

    fn end_rendering(&mut self, stage: Stage) {
        self.rendering_finished.get_or_insert_with(Instant::now);
        self.transition(stage);
    }

    fn transition(&mut self, stage: Stage) {
        if let Some(index) = self.active_stage.index() {
            self.stage_durations[index] = Some(self.stage_started.elapsed());
        }
        if stage.index().is_some() {
            self.active_stage = stage;
            self.stage_started = Instant::now();
        }
        self.stage = stage;
    }

    fn sample_speed(&mut self, now: Instant) {
        if self.stage != Stage::Rendering {
            return;
        }
        let elapsed = now.duration_since(self.sample_started);
        if elapsed < SAMPLE_INTERVAL {
            return;
        }
        let done = self.done();
        let fps = done.saturating_sub(self.sampled_frames) as f64 / elapsed.as_secs_f64();
        self.current_fps = Some(fps);
        if self.speed_samples.len() == 60 {
            self.speed_samples.pop_front();
        }
        // Tenths of a frame/sec retain low-rate variation in the sparkline.
        self.speed_samples.push_back((fps * 10.0).round() as u64);
        self.sampled_frames = done;
        self.sample_started = now;
    }

    pub(crate) fn finish(&mut self, result: &Result<(), String>) {
        self.finished = Some(Instant::now());
        if let Err(error) = result {
            self.error = Some(error.clone());
            self.transition(Stage::Failed);
        } else {
            self.end_rendering(Stage::Complete);
        }
    }

    fn done(&self) -> u64 {
        // Rendering events mark frame starts, including the last frame.
        if self.rendering_finished.is_some() {
            self.total
        } else {
            self.frame.saturating_sub(1).min(self.total)
        }
    }

    pub(crate) fn metrics(&self) -> (Option<f64>, Option<Duration>) {
        let Some(started) = self.rendering_started else {
            return (None, None);
        };
        let end = self
            .rendering_finished
            .or(self.finished)
            .unwrap_or_else(Instant::now);
        let seconds = end.duration_since(started).as_secs_f64();
        if self.done() == 0 || seconds <= 0.0 {
            return (None, None);
        }
        let fps = self.done() as f64 / seconds;
        let eta = (self.stage == Stage::Rendering)
            .then(|| {
                Duration::try_from_secs_f64(self.total.saturating_sub(self.done()) as f64 / fps)
                    .ok()
            })
            .flatten();
        (Some(fps), eta)
    }

    pub(crate) fn elapsed(&self) -> Duration {
        self.finished
            .unwrap_or_else(Instant::now)
            .duration_since(self.started)
    }
}

/// An inline viewport preserves scrollback and needs neither raw mode nor an alternate screen.
/// Keeping the cursor visible also leaves Ctrl-C with the terminal's usual behavior.
/// Runs `export` with progress lines on stderr and never the dashboard, for
/// an export that writes only subtitles: its frames are evaluated, not
/// rendered.
pub fn run_plain(
    export: impl FnOnce(&mut dyn FnMut(ExportProgress)) -> Result<(), ExportError>,
) -> Result<(), ExportError> {
    let mut plain = PlainProgress::new("evaluating");
    export(&mut |event| plain.update(event))
}

pub fn run(
    settings: Settings<'_>,
    no_ui: bool,
    export: impl FnOnce(&mut dyn FnMut(ExportProgress)) -> Result<(), ExportError>,
) -> Result<(), ExportError> {
    let mut plain = PlainProgress::new("rendering");
    if no_ui
        || !io::stderr().is_terminal()
        || std::env::var_os("TERM").is_some_and(|term| term == "dumb")
    {
        return export(&mut |event| plain.update(event));
    }
    let terminal = Terminal::with_options(
        CrosstermBackend::new(io::stderr()),
        TerminalOptions {
            viewport: Viewport::Inline(26),
        },
    );
    let Ok(mut terminal) = terminal else {
        return export(&mut |event| plain.update(event));
    };
    let state = Mutex::new(State::new());
    let active = AtomicBool::new(true);
    let result = thread::scope(|scope| {
        // The sender drops before the scope joins if export unwinds.
        let (stop, stopped) = mpsc::channel::<()>();
        let state = &state;
        let active = &active;
        scope.spawn(move || {
            let mut finished = false;
            loop {
                let draw = terminal.draw(|frame| {
                    let mut state = state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.sample_speed(Instant::now());
                    render(frame, &settings, &state);
                });
                if draw.is_err() {
                    active.store(false, Ordering::Release);
                    break;
                }
                if finished {
                    break;
                }
                finished = !matches!(
                    stopped.recv_timeout(TICK),
                    Err(mpsc::RecvTimeoutError::Timeout)
                );
            }
            let _ = terminal.show_cursor();
            eprintln!();
        });
        let result = export(&mut |event| {
            if active.load(Ordering::Acquire) {
                state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .update(event);
            } else {
                // The dashboard stopped drawing: the notices it kept print
                // first, and later ones go through the same list, so every
                // notice prints once, in arrival order.
                let diagnostic =
                    matches!(event, ExportProgress::Warning(_) | ExportProgress::Note(_));
                let mut state = state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if diagnostic {
                    state.update(event.clone());
                }
                for line in state.take_scrollback() {
                    plain.print_diagnostic(&line);
                }
                drop(state);
                if !diagnostic {
                    plain.update(event);
                }
            }
        });
        state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .finish(&result.as_ref().map(|_| ()).map_err(ToString::to_string));
        let _ = stop.send(());
        result
    });
    // Leave all diagnostics in scrollback, including ones displaced by newer warnings.
    let mut state = state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    for line in state.take_scrollback() {
        eprintln!("{line}");
    }
    result
}

fn panel(title: impl Into<Line<'static>>) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(BORDER))
        .title(title)
}

fn render(frame: &mut Frame, settings: &Settings<'_>, state: &State) {
    let area = frame.area();
    // The details row grows by a line when a props file is named.
    let details_height = 6 + u16::from(settings.props.is_some());
    if area.width < 82 || area.height < 17 + details_height {
        render_compact(frame, settings, state);
        return;
    }
    frame.set_cursor_position((area.x, area.bottom().saturating_sub(1)));
    frame.render_widget(
        Block::new().style(Style::new().bg(BACKGROUND).fg(Color::White)),
        area,
    );
    let rows = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(7),
        Constraint::Length(3),
        Constraint::Length(details_height),
        Constraint::Min(4),
        Constraint::Length(1),
    ])
    .split(area);
    let status_color = match state.stage {
        Stage::Complete => Color::Green,
        Stage::Failed => Color::Red,
        _ => ACCENT,
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::styled("  C E L E S T A", Style::new().fg(ACCENT).bold()),
                Span::styled("   /   EXPORT STUDIO", Style::new().fg(MUTED)),
            ]),
            Line::from(vec![
                Span::styled(
                    format!("  {}  ", state.stage.label()),
                    Style::new().fg(status_color),
                ),
                Span::styled(
                    match settings.still {
                        Some(StillFormat::Png) => "PNG / LOSSLESS".to_owned(),
                        Some(StillFormat::Jpeg) => {
                            format!("JPEG / QUALITY {}", settings.still_quality)
                        }
                        Some(StillFormat::Webp) => {
                            format!("WEBP / QUALITY {}", settings.still_quality)
                        }
                        None => match settings.format {
                            VideoFormat::Mp4 => "MP4 / H.264 + AAC",
                            VideoFormat::Mov => "MOV / ProRes 4444 + PCM",
                            VideoFormat::Webm => "WEBM / VP9 alpha + Opus",
                        }
                        .to_owned(),
                    },
                    Style::new().fg(MUTED),
                ),
            ]),
        ]),
        rows[0],
    );
    let body = Layout::horizontal([Constraint::Length(29), Constraint::Min(1)]).split(rows[1]);
    render_pipeline(frame, body[0], settings, state);
    let right = Layout::vertical([Constraint::Length(3), Constraint::Min(4)]).split(body[1]);
    let ratio = if state.total == 0 {
        0.0
    } else {
        state.done() as f64 / state.total as f64
    };
    frame.render_widget(
        Gauge::default()
            .block(panel(" RENDER PROGRESS "))
            .gauge_style(Style::new().fg(status_color).bg(BORDER))
            .ratio(ratio.clamp(0.0, 1.0))
            .label(format!(
                "{:.0}%  /  {} of {} frames",
                (ratio * 100.0).floor(),
                state.done(),
                state.total
            )),
        right[0],
    );
    let speed_title = state.current_fps.map_or_else(
        || " THROUGHPUT ".into(),
        |fps| format!(" THROUGHPUT / {fps:.1} fps "),
    );
    if state.speed_samples.is_empty() {
        frame.render_widget(
            Paragraph::new(" Waiting for rendering samples…")
                .style(Style::new().fg(MUTED))
                .block(panel(speed_title)),
            right[1],
        );
    } else {
        // Stretch the available history across the plot, including short exports.
        let columns = right[1].width.saturating_sub(2).max(1) as usize;
        let samples = (0..columns)
            .map(|column| state.speed_samples[column * state.speed_samples.len() / columns])
            .collect::<Vec<_>>();
        frame.render_widget(
            Sparkline::default()
                .block(panel(speed_title))
                .data(&samples)
                .style(Style::new().fg(CYAN)),
            right[1],
        );
    }
    let (fps, eta) = state.metrics();
    let stats = Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Fill(1),
        Constraint::Fill(1),
    ])
    .split(rows[2]);
    for (area, title, value) in [
        (stats[0], " ELAPSED ", clock(state.elapsed())),
        (
            stats[1],
            " AVERAGE SPEED ",
            fps.map_or_else(|| "—".into(), |fps| format!("{fps:.1} fps")),
        ),
        (
            stats[2],
            " RENDER ETA ",
            eta.map_or_else(|| "—".into(), clock),
        ),
    ] {
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                value,
                Style::new().fg(CYAN).bold(),
            )))
            .centered()
            .block(panel(title)),
            area,
        );
    }
    let details = Layout::horizontal([Constraint::Fill(3), Constraint::Fill(2)]).split(rows[3]);
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(Span::styled("SOURCE", Style::new().fg(MUTED))),
            Line::from(settings.source.display().to_string()),
            Line::from(Span::styled("DESTINATION", Style::new().fg(MUTED))),
            Line::from(settings.output.display().to_string()),
        ])
        .block(panel(" FILES ")),
        details[0],
    );
    let encoding = match settings.still {
        Some(StillFormat::Png) => "Lossless RGBA".into(),
        Some(StillFormat::Jpeg) => format!("JPEG quality {}", settings.still_quality),
        Some(StillFormat::Webp) => format!("WebP quality {}", settings.still_quality),
        None => match settings.format {
            VideoFormat::Mp4 => format!("{} / CRF {}", settings.video.preset, settings.video.crf),
            VideoFormat::Mov => "ProRes 4444".into(),
            VideoFormat::Webm => "VP9 CRF 20".into(),
        },
    };
    let mut profile = vec![
        Line::from(format!(
            " Input    {}",
            if settings.react {
                "React / TSX"
            } else {
                "JSON project"
            }
        )),
        Line::from(format!(" Quality  {}", settings.render_quality)),
        Line::from(format!(" Encode   {encoding}")),
        Line::from(if settings.still.is_some() {
            // Stills are drawn over the opaque export background.
            " Alpha    opaque background".into()
        } else if settings.format.has_alpha() {
            " Alpha    straight, BT.709".into()
        } else {
            format!(
                " Color    {}, {}-bit",
                settings.video.color_conversion, settings.video.bit_depth
            )
        }),
    ];
    if let Some(props) = &settings.props {
        profile.push(Line::from(format!(" Props    {props}")));
    }
    frame.render_widget(
        Paragraph::new(profile).block(panel(" EXPORT PROFILE ")),
        details[1],
    );
    let warnings = state.warnings().count();
    let diagnostics = match &state.error {
        Some(error) => Paragraph::new(error.as_str())
            .style(Style::new().fg(Color::Red))
            .block(panel(" EXPORT ERROR ")),
        None if warnings == 0 => Paragraph::new(" ✓  No warnings reported")
            .style(Style::new().fg(MUTED))
            .block(panel(" DIAGNOSTICS ")),
        None => Paragraph::new(
            state
                .warnings()
                .rev()
                .take(3)
                .map(|warning| Line::from(format!(" !  {warning}")))
                .collect::<Vec<_>>(),
        )
        .style(Style::new().fg(Color::Yellow))
        .block(panel(format!(" WARNINGS / {warnings} "))),
    };
    frame.render_widget(diagnostics.wrap(Wrap { trim: false }), rows[4]);
    frame.render_widget(
        Paragraph::new(
            "  Ctrl+C  interrupt    •    throughput: 500 ms samples    •    --no-ui  text output",
        )
        .style(Style::new().fg(MUTED)),
        rows[5],
    );
}

fn render_pipeline(frame: &mut Frame, area: Rect, settings: &Settings<'_>, state: &State) {
    let stages = [
        (Stage::Preparing, "Prepare"),
        (Stage::Rendering, "Render"),
        (Stage::Mixing, "Audio mix"),
        (Stage::Muxing, "Finalize"),
    ];
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let mut lines = Vec::new();
    for (index, (stage, label)) in stages.iter().enumerate() {
        if settings.still.is_some() && index > 1 {
            continue;
        }
        let active = *stage == state.active_stage;
        let (icon, color, duration) = if active && state.stage == Stage::Failed {
            (
                "!",
                Color::Red,
                state.stage_durations[index].map_or_else(|| "—".into(), clock),
            )
        } else if let Some(duration) = state.stage_durations[index] {
            ("✓", Color::Green, clock(duration))
        } else if active && state.finished.is_none() {
            (
                spinner[(state.elapsed().as_millis() / 100 % 10) as usize],
                ACCENT,
                clock(state.stage_started.elapsed()),
            )
        } else if state.stage == Stage::Complete
            || state
                .active_stage
                .index()
                .is_some_and(|active| active > index)
        {
            ("–", MUTED, "skipped".into())
        } else {
            ("○", MUTED, "pending".into())
        };
        lines.push(Line::from(vec![
            Span::styled(format!(" {icon}  {label:<9}"), Style::new().fg(color)),
            Span::styled(format!(" {duration:>8}"), Style::new().fg(MUTED)),
        ]));
    }
    frame.render_widget(Paragraph::new(lines).block(panel(" PIPELINE ")), area);
}

fn render_compact(frame: &mut Frame, settings: &Settings<'_>, state: &State) {
    let area = frame.area();
    frame.set_cursor_position((area.x, area.bottom().saturating_sub(1)));
    let color = match state.stage {
        Stage::Failed => Color::Red,
        Stage::Complete => Color::Green,
        _ => ACCENT,
    };
    if area.height < 10 || area.width < 36 {
        frame.render_widget(
            Paragraph::new(format!(
                "CELESTA  {}\n{} / {} frames\nelapsed {}",
                state.stage.label(),
                state.done(),
                state.total,
                clock(state.elapsed())
            ))
            .style(Style::new().fg(color)),
            area,
        );
        return;
    }
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .title(Line::from(vec![
            Span::styled(" CELESTA ", Style::new().fg(ACCENT).bold()),
            Span::raw("/ EXPORT "),
        ]))
        .border_style(Style::new().fg(Color::DarkGray));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(3),
        Constraint::Length(2),
        Constraint::Length(4),
        Constraint::Min(1),
    ])
    .split(inner);
    let spinner = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    let icon = match state.stage {
        Stage::Complete => "✓",
        Stage::Failed => "!",
        _ => spinner[(state.elapsed().as_millis() / 100 % spinner.len() as u128) as usize],
    };
    frame.render_widget(
        Paragraph::new(format!(" {icon}  {}", state.stage.label()))
            .style(Style::new().fg(color).bold()),
        rows[0],
    );
    let ratio = if state.total == 0 {
        0.0
    } else {
        state.done() as f64 / state.total as f64
    };
    frame.render_widget(
        Gauge::default()
            .block(Block::new().title(" Rendered frames "))
            .gauge_style(Style::new().fg(color))
            .ratio(ratio.clamp(0.0, 1.0))
            .label(format!(
                "{:.0}%   {} / {}",
                (ratio * 100.0).floor(),
                state.done(),
                state.total
            )),
        rows[1],
    );
    let (fps, eta) = state.metrics();
    frame.render_widget(
        Paragraph::new(format!(
            " Elapsed {}   •   {} fps   •   Render ETA {}",
            clock(state.elapsed()),
            fps.map_or_else(|| "—".into(), |fps| format!("{fps:.1}")),
            eta.map_or_else(|| "—".into(), clock)
        )),
        rows[2],
    );
    let format = if let Some(still) = settings.still {
        match still {
            StillFormat::Png => "PNG stills".to_owned(),
            StillFormat::Jpeg => format!("JPEG stills  •  quality {}", settings.still_quality),
            StillFormat::Webp => format!("WebP stills  •  quality {}", settings.still_quality),
        }
    } else {
        match settings.format {
            VideoFormat::Mp4 => format!(
                "H.264 / AAC  •  {}  •  CRF {}  •  color {}  •  {}-bit",
                settings.video.preset,
                settings.video.crf,
                settings.video.color_conversion,
                settings.video.bit_depth
            ),
            VideoFormat::Mov => "ProRes 4444 / PCM  •  straight alpha, BT.709".to_owned(),
            VideoFormat::Webm => "VP9 alpha / Opus  •  straight alpha, BT.709".to_owned(),
        }
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(format!(" Source  {}", settings.source.display())),
            Line::from(format!(" Output  {}", settings.output.display())),
            Line::from(format!(" Format  {format}")),
            Line::from(format!(
                " Input   {}  •  quality {}",
                if settings.react {
                    "React"
                } else {
                    "JSON project"
                },
                settings.render_quality
            )),
        ]),
        rows[3],
    );
    let warnings = state.warnings().count();
    let diagnostics = if let Some(error) = &state.error {
        Paragraph::new(error.as_str())
            .style(Style::new().fg(Color::Red))
            .wrap(Wrap { trim: false })
    } else if warnings == 0 {
        Paragraph::new(" No warnings").style(Style::new().fg(Color::DarkGray))
    } else {
        Paragraph::new(
            state
                .warnings()
                .rev()
                .take(3)
                .map(|warning| Line::from(format!(" ! {warning}")))
                .collect::<Vec<_>>(),
        )
        .block(Block::new().title(format!(" Warnings ({warnings}) ")))
        .style(Style::new().fg(Color::Yellow))
        .wrap(Wrap { trim: false })
    };
    frame.render_widget(diagnostics, rows[4]);
}

struct PlainProgress {
    state: State,
    last_line: Option<Instant>,
    terminal: bool,
    /// What is done to each frame, as progress lines name it.
    verb: &'static str,
}

impl PlainProgress {
    fn new(verb: &'static str) -> Self {
        Self {
            state: State::new(),
            last_line: None,
            terminal: io::stderr().is_terminal(),
            verb,
        }
    }

    fn update(&mut self, event: ExportProgress) {
        match &event {
            ExportProgress::Rendering { frame, total } => {
                let (frame, total) = (*frame, *total);
                self.state.update(event);
                let interval = if self.terminal {
                    TICK
                } else {
                    Duration::from_secs(1)
                };
                if frame != total && self.last_line.is_some_and(|last| last.elapsed() < interval) {
                    return;
                }
                self.last_line = Some(Instant::now());
                let (fps, eta) = self.state.metrics();
                let line = format!(
                    "{} frame {frame}/{total}  {} fps  elapsed {}  eta {}",
                    self.verb,
                    fps.map_or_else(|| "—".into(), |fps| format!("{fps:.1}")),
                    clock(self.state.elapsed()),
                    eta.map_or_else(|| "—".into(), clock)
                );
                if self.terminal {
                    eprint!("\r{line}   ");
                    if frame == total {
                        eprintln!();
                    }
                } else {
                    eprintln!("{line}");
                }
            }
            ExportProgress::MixingAudio => {
                self.state.update(event);
                eprintln!("mixing audio");
            }
            ExportProgress::Muxing => {
                self.state.update(event);
                eprintln!("muxing");
            }
            ExportProgress::Composition(_)
            | ExportProgress::ReactCompositions { .. }
            | ExportProgress::Wrote(_) => {}
            ExportProgress::Warning(warning) => {
                self.print_diagnostic(&Diagnostic::Warning(warning.clone()).line());
            }
            ExportProgress::Note(note) => self.print_diagnostic(note),
        }
    }

    /// A warning or note line, over the progress line on a terminal.
    fn print_diagnostic(&self, line: &str) {
        if self.terminal {
            eprintln!("\r{line}");
        } else {
            eprintln!("{line}");
        }
    }
}

fn clock(duration: Duration) -> String {
    let seconds = duration.as_secs();
    format!(
        "{:02}:{:02}:{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;

    fn settings() -> Settings<'static> {
        Settings {
            source: Path::new("film.tsx"),
            output: Path::new("film.mp4"),
            react: true,
            still: None,
            still_quality: 90,
            format: VideoFormat::Mp4,
            video: VideoEncoding::default(),
            render_quality: RenderQuality::default(),
            props: Some("props.json (auto)".to_owned()),
        }
    }

    fn screen(state: &State, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal
            .draw(|frame| render(frame, &settings(), state))
            .unwrap();
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn alpha_formats_name_their_codecs_and_alpha() {
        for (format, header, encode) in [
            (VideoFormat::Mov, "MOV / ProRes 4444 + PCM", "ProRes 4444"),
            (VideoFormat::Webm, "WEBM / VP9 alpha + Opus", "VP9 CRF 20"),
        ] {
            let settings = Settings {
                format,
                ..settings()
            };
            let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
            terminal
                .draw(|frame| render(frame, &settings, &State::new()))
                .unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(text.contains(header), "{text}");
            assert!(text.contains(encode), "{text}");
            assert!(text.contains("Alpha    straight, BT.709"), "{text}");
            assert!(!text.contains("H.264"), "{text}");
        }
    }

    #[test]
    fn still_formats_name_their_encoding_and_quality() {
        for (still, header, encode) in [
            (StillFormat::Png, "PNG / LOSSLESS", "Lossless RGBA"),
            (StillFormat::Jpeg, "JPEG / QUALITY 75", "JPEG quality 75"),
            (StillFormat::Webp, "WEBP / QUALITY 75", "WebP quality 75"),
        ] {
            let settings = Settings {
                output: Path::new("film.png"),
                still: Some(still),
                still_quality: 75,
                ..settings()
            };
            let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
            terminal
                .draw(|frame| render(frame, &settings, &State::new()))
                .unwrap();
            let text: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(text.contains(header), "{text}");
            assert!(text.contains(encode), "{text}");
            assert!(text.contains("Alpha    opaque background"), "{text}");
            assert!(!text.contains("Audio mix"), "{text}");
        }
    }

    #[test]
    fn last_frame_start_does_not_claim_rendering_is_complete() {
        let mut state = State::new();
        state.update(ExportProgress::Rendering {
            frame: 10,
            total: 10,
        });
        assert_eq!(state.done(), 9);
    }

    #[test]
    fn rendering_eta_is_not_shown_during_audio_mixing() {
        let mut state = State::new();
        state.update(ExportProgress::Rendering {
            frame: 10,
            total: 10,
        });
        state.update(ExportProgress::MixingAudio);
        assert_eq!(state.metrics().1, None);
    }

    #[test]
    fn failure_keeps_the_last_frame_incomplete() {
        let mut state = State::new();
        state.update(ExportProgress::Rendering {
            frame: 10,
            total: 10,
        });
        state.finish(&Err("encoder failed".into()));
        assert_eq!(state.done(), 9);
    }

    #[test]
    fn dashboard_displays_failure_and_preserves_output_path() {
        let mut state = State::new();
        state.finish(&Err("encoder failed".into()));
        let text = screen(&state, 110, 26);
        assert!(
            text.contains("Export failed")
                && text.contains("encoder failed")
                && text.contains("film.mp4"),
            "{text}"
        );
    }

    #[test]
    fn warning_does_not_replace_the_active_stage() {
        let mut state = State::new();
        state.update(ExportProgress::MixingAudio);
        state.update(ExportProgress::Warning("Missing font".into()));
        let text = screen(&state, 110, 26);
        assert!(
            text.contains("Mixing audio") && text.contains("Missing font"),
            "{text}"
        );
    }

    #[test]
    fn notes_are_kept_apart_from_warnings() {
        let mut state = State::new();
        state.update(ExportProgress::Note("note: terms of use".into()));
        assert_eq!(state.warnings().count(), 0);
        let text = screen(&state, 110, 26);
        assert!(text.contains("No warnings reported"), "{text}");
    }

    #[test]
    fn scrollback_keeps_warnings_and_notes_in_arrival_order() {
        let mut state = State::new();
        state.update(ExportProgress::Warning("fallback font".into()));
        state.update(ExportProgress::Note("note: terms of use".into()));
        state.update(ExportProgress::Rendering { frame: 1, total: 2 });
        state.update(ExportProgress::Warning("missing glyph".into()));
        assert_eq!(
            state.take_scrollback(),
            [
                "warning: fallback font",
                "note: terms of use",
                "warning: missing glyph"
            ]
        );
        assert_eq!(
            state.warnings().collect::<Vec<_>>(),
            ["fallback font", "missing glyph"]
        );
    }

    #[test]
    fn scrollback_prints_each_notice_once() {
        let mut state = State::new();
        state.update(ExportProgress::Warning("fallback font".into()));
        assert_eq!(state.take_scrollback(), ["warning: fallback font"]);
        assert!(state.take_scrollback().is_empty());
        state.update(ExportProgress::Note("note: terms of use".into()));
        assert_eq!(state.take_scrollback(), ["note: terms of use"]);
        assert_eq!(state.warnings().count(), 1);
    }

    #[test]
    fn small_and_resized_terminals_render_without_panicking() {
        let mut terminal = Terminal::new(TestBackend::new(100, 18)).unwrap();
        let state = State::new();
        for (width, height) in [(110, 26), (100, 24), (100, 18), (40, 12), (24, 6), (1, 1)] {
            terminal.backend_mut().resize(width, height);
            terminal
                .draw(|frame| render(frame, &settings(), &state))
                .unwrap();
        }
    }

    #[test]
    fn throughput_samples_show_a_stall_as_zero_speed() {
        let mut state = State::new();
        state.update(ExportProgress::Rendering {
            frame: 10,
            total: 20,
        });
        let now = state.sample_started;
        state.sample_speed(now + SAMPLE_INTERVAL);
        state.sample_speed(now + SAMPLE_INTERVAL * 2);
        assert_eq!(
            state.speed_samples.iter().copied().collect::<Vec<_>>(),
            vec![180, 0]
        );
    }

    #[test]
    fn percentage_stays_below_100_until_the_last_frame_finishes() {
        let mut state = State::new();
        state.update(ExportProgress::Rendering {
            frame: 720,
            total: 720,
        });
        assert!(screen(&state, 110, 26).contains("99%"));
    }

    #[test]
    fn failed_mux_keeps_rendered_frames_complete() {
        let mut state = State::new();
        state.update(ExportProgress::Rendering {
            frame: 10,
            total: 10,
        });
        state.update(ExportProgress::Muxing);
        state.finish(&Err("mux failed".into()));
        assert_eq!(state.done(), 10);
    }

    #[test]
    fn pipeline_marks_unneeded_audio_as_skipped() {
        let mut state = State::new();
        state.update(ExportProgress::Rendering {
            frame: 10,
            total: 10,
        });
        state.update(ExportProgress::Muxing);
        let text = screen(&state, 110, 26);
        assert!(text.contains("Audio mix  skipped"), "{text}");
    }
}
