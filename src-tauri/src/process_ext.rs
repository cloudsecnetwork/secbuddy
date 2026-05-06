//! Cross-platform helpers for spawning child processes without flashing a console
//! window on Windows. In a windowed (non-console) Tauri build, every spawned
//! console child (e.g. `where`, `taskkill`, `npx`, security CLIs) briefly pops a
//! conhost window unless we set `CREATE_NO_WINDOW`.

#[cfg(windows)]
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Apply `CREATE_NO_WINDOW` to a `std::process::Command` on Windows. No-op elsewhere.
pub fn no_window_std(cmd: &mut std::process::Command) -> &mut std::process::Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Apply `CREATE_NO_WINDOW` to a `tokio::process::Command` on Windows. No-op elsewhere.
pub fn no_window_tokio(cmd: &mut tokio::process::Command) -> &mut tokio::process::Command {
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}
