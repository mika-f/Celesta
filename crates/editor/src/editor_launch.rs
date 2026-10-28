//! Opens a layer's source position in the user's text editor, chosen by
//! `VISUAL` or `EDITOR`, or in the OS default application when neither is set.

use celesta_react_bridge::LayerSource;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::OnceLock;

/// Surrounds the values the login shell prints, so rc-file noise before or
/// after them is ignored.
const LOGIN_SHELL_MARKER: &str = "__CELESTA_EDITOR__";

/// Editors that run in a terminal and cannot be launched from the app.
const TERMINAL_EDITORS: &[&str] = &[
    "vi", "vim", "nvim", "nano", "pico", "micro", "hx", "helix", "kak", "ed",
];

/// A program and its arguments, ready to spawn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Invocation {
    pub(crate) program: String,
    pub(crate) args: Vec<String>,
}

/// Why a source position could not be opened. `Display` is the text the
/// panel shows.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum LaunchError {
    /// `VISUAL`/`EDITOR` names a terminal editor, carrying its name.
    #[error("VISUAL/EDITOR is a terminal editor ({0}); set VISUAL to a graphical editor")]
    TerminalEditor(String),
    /// `VISUAL`/`EDITOR` is not a valid command line, carrying its value.
    #[error("VISUAL/EDITOR could not be parsed: {0}")]
    Unparsable(String),
    /// The process failed to start, carrying the system's message.
    #[error("Could not open the editor: {0}")]
    Spawn(String),
}

/// `VISUAL`, then `EDITOR`, read through `env`. Empty values count as unset.
pub(crate) fn editor_value(env: impl Fn(&str) -> Option<String>) -> Option<String> {
    ["VISUAL", "EDITOR"]
        .into_iter()
        .filter_map(env)
        .find(|value| !value.trim().is_empty())
}

/// The text between the first two login shell markers, once both are present.
fn marked_values(output: &str) -> Option<&str> {
    let start = output.find(LOGIN_SHELL_MARKER)? + LOGIN_SHELL_MARKER.len();
    let len = output[start..].find(LOGIN_SHELL_MARKER)?;
    Some(&output[start..start + len])
}

/// The first non-empty of the two values printed between the login shell
/// markers (`VISUAL`, then `EDITOR`, separated by a NUL).
pub(crate) fn parse_login_shell_output(output: &str) -> Option<String> {
    marked_values(output)?
        .split('\0')
        .find(|value| !value.trim().is_empty())
        .map(str::to_string)
}

/// Reads until the closing marker has arrived, or `reader` ends. Stopping at
/// the marker matters because a daemon an rc file starts can keep the pipe
/// open long after the shell has printed its answer and exited.
#[cfg_attr(not(unix), allow(dead_code))]
pub(crate) fn read_until_closing_marker(mut reader: impl Read) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        match reader.read(&mut chunk) {
            Ok(0) | Err(_) => break,
            Ok(read) => bytes.extend_from_slice(&chunk[..read]),
        }
        if marked_values(&String::from_utf8_lossy(&bytes)).is_some() {
            break;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Asks the login shell for `VISUAL`/`EDITOR` once per session, caching the
/// answer including "none". A Finder- or Dock-launched app does not inherit
/// the variables the user sets in their shell profile. Blocks for up to 3
/// seconds on the first call.
pub(crate) fn login_shell_editor() -> Option<String> {
    static CACHE: OnceLock<Option<String>> = OnceLock::new();
    CACHE.get_or_init(read_login_shell_editor).clone()
}

#[cfg(unix)]
fn read_login_shell_editor() -> Option<String> {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let shell = std::env::var("SHELL")
        .ok()
        .filter(|shell| !shell.is_empty())?;
    let script = format!(
        "printf \"\\n{LOGIN_SHELL_MARKER}%s\\0%s{LOGIN_SHELL_MARKER}\\n\" \"$VISUAL\" \"$EDITOR\""
    );
    let mut child = Command::new(shell)
        .args(["-lic", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let _ = sender.send(read_until_closing_marker(stdout));
    });
    let output = receiver.recv_timeout(Duration::from_secs(3)).ok();
    // A shell that already exited is unaffected; one stuck in an rc file is
    // stopped here so the caller never waits beyond the timeout.
    let _ = child.kill();
    let _ = child.wait();
    parse_login_shell_output(&output?)
}

#[cfg(not(unix))]
fn read_login_shell_editor() -> Option<String> {
    None
}

/// Splits an editor command line into words with shell rules. On Windows a
/// backslash is a path separator, so it is escaped before splitting.
pub(crate) fn split_command(value: &str, windows: bool) -> Option<Vec<String>> {
    if windows {
        shlex::split(&value.replace('\\', "\\\\"))
    } else {
        shlex::split(value)
    }
}

/// The program's file name without directory or extension, as written.
fn program_name(program: &str) -> &str {
    let file = program.rsplit(['/', '\\']).next().unwrap_or(program);
    match file.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem,
        _ => file,
    }
}

/// The command that opens `source` at its line and column in the editor
/// named by `value` (`VISUAL`/`EDITOR`, which may carry arguments).
pub(crate) fn editor_invocation(
    value: &str,
    source: &LayerSource,
) -> Result<Invocation, LaunchError> {
    let words = split_command(value, cfg!(windows))
        .filter(|words| !words.is_empty())
        .ok_or_else(|| LaunchError::Unparsable(value.to_string()))?;
    let (program, user_args) = words.split_first().expect("words is not empty");
    let name = program_name(program);
    let lower = name.to_ascii_lowercase();

    let terminal_flag = matches!(lower.as_str(), "emacs" | "emacsclient")
        && user_args
            .iter()
            .any(|arg| matches!(arg.as_str(), "-nw" | "-t" | "--tty"));
    if terminal_flag || TERMINAL_EDITORS.contains(&lower.as_str()) {
        return Err(LaunchError::TerminalEditor(name.to_string()));
    }

    let file = source.file.to_string_lossy().into_owned();
    let (line, column) = (source.line, source.column);
    let mut args = user_args.to_vec();
    match lower.as_str() {
        "code" | "code-insiders" | "codium" | "cursor" | "windsurf" => {
            args.extend(["--goto".to_string(), format!("{file}:{line}:{column}")]);
        }
        "zed" | "subl" | "sublime_text" => args.push(format!("{file}:{line}:{column}")),
        "idea" | "webstorm" | "phpstorm" | "pycharm" | "goland" | "clion" | "rider"
        | "rustrover" | "fleet" => args.extend([
            "--line".to_string(),
            line.to_string(),
            "--column".to_string(),
            column.to_string(),
            file,
        ]),
        "mate" => args.extend(["--line".to_string(), line.to_string(), file]),
        _ => args.extend([format!("+{line}"), file]),
    }
    Ok(Invocation {
        program: program.clone(),
        args,
    })
}

/// Opens `file`, without a position, in the OS default application.
pub(crate) fn default_opener(file: &Path) -> Invocation {
    let file = file.to_string_lossy().into_owned();
    let (program, args) = if cfg!(target_os = "macos") {
        ("open", vec![file])
    } else if cfg!(windows) {
        (
            "cmd",
            vec!["/C".to_string(), "start".to_string(), String::new(), file],
        )
    } else {
        ("xdg-open", vec![file])
    };
    Invocation {
        program: program.to_string(),
        args,
    }
}

/// Opens `source` in the editor from `VISUAL`/`EDITOR` (the login shell's
/// when the process has neither), or in the OS default application when
/// there is none. Blocks while looking up the login shell, so call it on a
/// background executor. The process is not waited on beyond reaping it.
pub(crate) fn open_source(source: &LayerSource, cwd: &Path) -> Result<(), LaunchError> {
    let invocation = match editor_value(|name| std::env::var(name).ok()).or_else(login_shell_editor)
    {
        Some(value) => editor_invocation(&value, source)?,
        None => default_opener(&source.file),
    };
    spawn_detached(&invocation, cwd)
}

fn spawn_detached(invocation: &Invocation, cwd: &Path) -> Result<(), LaunchError> {
    let mut command = Command::new(&invocation.program);
    command
        .args(&invocation.args)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = command
        .spawn()
        .map_err(|error| LaunchError::Spawn(error.to_string()))?;
    // Reaps the process when the editor exits, so it does not linger as a zombie.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
