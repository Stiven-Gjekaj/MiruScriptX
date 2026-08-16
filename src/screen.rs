//! Drawing on a real terminal, for `clear`, `move_to`, and the rest.
//!
//! The companion of [`crate::keyboard`]: that module is about the way in, this
//! one about the way out. Both are `miru`'s implementation of a capability the
//! language crate only declares, so a host without a terminal, such as the
//! browser playground, gets `NoScreen` and every call refuses.
//!
//! # Escapes go to standard output, not through `Output`
//!
//! `Output::write` is the program's *result*. A caller may capture it, pipe it,
//! or compare it against an expected string, and `run_capture` does exactly
//! that. Escape sequences are not a result; they are instructions to a device.
//! Writing them through `Output` would mean every golden test of a program that
//! cleared its screen asserted on control codes. So this writes to stdout
//! directly, and the two stay separable.
//!
//! # Nothing happens when standard output is not a terminal
//!
//! Redirect a game to a file and the escapes would land in the file, where they
//! are noise: `cat -v` shows `^[[2J` between the frames and nothing has been
//! cleared, because a file has no screen to clear.
//!
//! So every operation here checks first, and does nothing when there is no
//! terminal. **This is not a silent failure.** It is the same shape as
//! `RawMode::enter` returning `None`: the honest answer to "clear the screen"
//! when the output is a file is that there is no screen, and there is nothing
//! to report because the program asked for something that simply does not
//! apply. It is also what keeps a game's output comparable in a test.
//!
//! `size` is the exception and refuses, because there is no honest number to
//! give. See [`RealScreen::size`].

use std::io::Write;

/// The terminal `miru` gives a program.
///
/// Holds whether the cursor was hidden, so [`Drop`] can put it back.
pub struct RealScreen {
    hidden: bool,
}

impl RealScreen {
    pub fn new() -> RealScreen {
        RealScreen { hidden: false }
    }

    /// Send an escape sequence, or do nothing where there is no terminal.
    ///
    /// Flushed immediately. A frame that sat in a buffer until the next newline
    /// would arrive after the thing it was supposed to precede, and a cursor
    /// moved after the text it was meant to position is a scrambled screen.
    fn escape(&mut self, sequence: &str) -> Result<(), String> {
        if !platform::stdout_is_terminal() {
            return Ok(());
        }
        let mut out = std::io::stdout();
        out.write_all(sequence.as_bytes())
            .and_then(|()| out.flush())
            .map_err(|err| format!("cannot write to the terminal: {err}"))
    }
}

impl Default for RealScreen {
    fn default() -> RealScreen {
        RealScreen::new()
    }
}

impl miruscriptx::value::Screen for RealScreen {
    fn clear(&mut self) -> Result<(), String> {
        // Erase everything, then home the cursor. Two sequences rather than
        // one, because clearing does not move the cursor and a program that
        // cleared without homing would draw its next frame wherever the last
        // one left off.
        self.escape("\x1b[2J\x1b[H")
    }

    fn move_to(&mut self, column: i64, row: i64) -> Result<(), String> {
        // The language counts from zero and this sequence counts from one.
        // Converting here rather than asking every program to add one keeps
        // `move_to(0, 0)` meaning the top left, which is where a program that
        // indexes a grid expects it to be.
        if column < 0 || row < 0 {
            return Err(format!(
                "move_to expects a column and row that are not negative but got {column} and {row}"
            ));
        }
        self.escape(&format!("\x1b[{};{}H", row + 1, column + 1))
    }

    fn hide_cursor(&mut self) -> Result<(), String> {
        self.escape("\x1b[?25l")?;
        self.hidden = true;
        Ok(())
    }

    fn show_cursor(&mut self) -> Result<(), String> {
        self.escape("\x1b[?25h")?;
        self.hidden = false;
        Ok(())
    }

    /// The terminal's size, or a refusal where there is no terminal.
    ///
    /// **This is the one operation here that refuses rather than doing
    /// nothing**, and the reason is the one `NoClock` gives: 80 by 24 is a
    /// wrong answer, not an absent one, and a program handed it would go on to
    /// draw a board that does not fit. There is nothing to do quietly, because
    /// the caller wanted a number back.
    ///
    /// A game that must run under a pipe should therefore not call this. The
    /// bundled ones fix their own size for exactly that reason.
    fn size(&mut self) -> Result<(i64, i64), String> {
        platform::terminal_size()
    }
}

impl Drop for RealScreen {
    /// Put the cursor back, however the program ended.
    ///
    /// The same reasoning as `RawMode`'s `Drop`, and it matters more. A
    /// terminal left in raw mode is confusing; a terminal left with no cursor
    /// looks broken, keeps looking broken in the shell afterwards, and gives
    /// the person no clue what did it. A program can end normally, with an
    /// error, or through `exit`, and only the first is somewhere a call could
    /// have been written.
    fn drop(&mut self) {
        if self.hidden {
            // Nothing to do about a failure. The program is on its way out and
            // there is nobody left to tell.
            let mut out = std::io::stdout();
            let _ = out.write_all(b"\x1b[?25h");
            let _ = out.flush();
        }
    }
}

#[cfg(unix)]
mod platform {
    /// Whether standard output is a terminal.
    ///
    /// `isatty` through `nix`, which is already a dependency for raw mode, so
    /// this costs no new crate.
    pub fn stdout_is_terminal() -> bool {
        use std::os::fd::AsFd;
        nix::unistd::isatty(std::io::stdout().as_fd()).unwrap_or(false)
    }

    pub fn terminal_size() -> Result<(i64, i64), String> {
        use std::os::fd::AsRawFd;

        if !stdout_is_terminal() {
            return Err("this program's output is not a terminal, so it has no size".to_string());
        }

        // TIOCGWINSZ, declared here rather than pulled from another crate.
        // `nix` exposes the ioctl macros but not this struct in the feature set
        // this project enables, and six lines is cheaper than a dependency.
        #[repr(C)]
        struct WinSize {
            rows: libc_ushort,
            columns: libc_ushort,
            x_pixels: libc_ushort,
            y_pixels: libc_ushort,
        }
        #[allow(non_camel_case_types)]
        type libc_ushort = std::ffi::c_ushort;

        // Every unix this builds for uses the same request number for
        // TIOCGWINSZ except that Linux and the BSDs encode it differently.
        #[cfg(any(target_os = "linux", target_os = "android"))]
        const TIOCGWINSZ: std::ffi::c_ulong = 0x5413;
        #[cfg(not(any(target_os = "linux", target_os = "android")))]
        const TIOCGWINSZ: std::ffi::c_ulong = 0x4008_7468;

        unsafe extern "C" {
            fn ioctl(fd: std::ffi::c_int, request: std::ffi::c_ulong, ...) -> std::ffi::c_int;
        }

        let mut size = WinSize {
            rows: 0,
            columns: 0,
            x_pixels: 0,
            y_pixels: 0,
        };
        // SAFETY: `size` is a live `WinSize` for the call, which is what
        // TIOCGWINSZ writes through the pointer it is given.
        let result = unsafe { ioctl(std::io::stdout().as_raw_fd(), TIOCGWINSZ, &mut size) };
        if result != 0 {
            return Err("cannot read the terminal's size".to_string());
        }
        // A terminal that reports zero is one that does not know, which is a
        // wrong answer rather than an absent one.
        if size.columns == 0 || size.rows == 0 {
            return Err("the terminal did not report a size".to_string());
        }
        Ok((i64::from(size.columns), i64::from(size.rows)))
    }
}

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetConsoleScreenBufferInfo, GetStdHandle, CONSOLE_SCREEN_BUFFER_INFO,
        STD_OUTPUT_HANDLE,
    };

    fn stdout_handle() -> Option<HANDLE> {
        // SAFETY: `GetStdHandle` takes a constant and returns a handle or
        // `INVALID_HANDLE_VALUE`, which is checked here.
        let handle = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return None;
        }
        Some(handle)
    }

    /// Whether standard output is a console.
    ///
    /// A console has a mode and a pipe does not, which is the same question
    /// `RawMode::enter` asks about standard input.
    pub fn stdout_is_terminal() -> bool {
        let Some(handle) = stdout_handle() else {
            return false;
        };
        let mut mode = 0u32;
        // SAFETY: `handle` is valid and `mode` is live for the call.
        unsafe { GetConsoleMode(handle, &mut mode) != 0 }
    }

    pub fn terminal_size() -> Result<(i64, i64), String> {
        if !stdout_is_terminal() {
            return Err("this program's output is not a terminal, so it has no size".to_string());
        }
        let handle = stdout_handle().ok_or("cannot reach the console")?;

        // SAFETY: zeroed is a valid starting state for this struct, and the
        // call fills it in.
        let mut info: CONSOLE_SCREEN_BUFFER_INFO = unsafe { std::mem::zeroed() };
        // SAFETY: `handle` is valid and `info` is live for the call.
        if unsafe { GetConsoleScreenBufferInfo(handle, &mut info) } == 0 {
            return Err("cannot read the console's size".to_string());
        }
        // The **window**, not the buffer. A console's buffer is usually taller
        // than the window and scrolls; a program drawing a frame wants the part
        // that is visible, or its bottom rows sit below the fold.
        let columns = i64::from(info.srWindow.Right - info.srWindow.Left + 1);
        let rows = i64::from(info.srWindow.Bottom - info.srWindow.Top + 1);
        if columns <= 0 || rows <= 0 {
            return Err("the console did not report a size".to_string());
        }
        Ok((columns, rows))
    }
}
