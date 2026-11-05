//! Starting a helper process from the editor without a console window.

use std::process::Command;

/// `CREATE_NO_WINDOW`: the process gets a console, but no window for it.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Has `command` start its process without a console window of its own.
///
/// On Windows, a console program started by a GUI program, such as the
/// release editor, gets a new console window unless it is started with
/// `CREATE_NO_WINDOW`, and the window stays for as long as the process runs.
/// Every helper the editor starts goes through here: plugin processes,
/// Node.js, and the external code editor. Elsewhere this does nothing.
pub fn without_console_window(command: &mut Command) -> &mut Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}
