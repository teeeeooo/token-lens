//! Platform policy for Token Lens-owned background helper processes.
//!
//! The desktop application is a GUI process. On Windows, child console
//! executables otherwise create a visible console window on every spawn, which
//! is especially disruptive during periodic quota refresh.

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub(crate) fn configure_tokio(command: &mut tokio::process::Command) {
    #[cfg(target_os = "windows")]
    command.creation_flags(CREATE_NO_WINDOW);
    #[cfg(not(target_os = "windows"))]
    let _ = command;
}

pub(crate) fn configure_std(command: &mut std::process::Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    let _ = command;
}
