//! Reading one key from a real terminal, for `read_key`.
//!
//! This is the one part of `miru` that cannot be written portably, and it is
//! the reason issue #12 was closed before issue #32 was opened for work: until
//! 1.6 nothing here had ever been compiled on Windows or macOS by CI.
//!
//! # What raw mode means here
//!
//! A terminal hands a program a line at a time because the terminal is
//! buffering it. Two flags are turned off to stop that, and no more than two
//! are turned off that are not needed:
//!
//! - **`ICANON`**, the line buffering itself. This is the point.
//! - **`ECHO`**, so a key does not appear on screen twice.
//! - **`ISIG`**, so Ctrl-C reaches the program as a key rather than as a
//!   signal. See below, because this one has a consequence.
//!
//! **`OPOST` is left alone**, which most raw-mode code turns off as well. With
//! it off, a newline written by `print` moves down but not back to the left
//! margin, and every program's output comes out as a staircase. Leaving it on
//! costs nothing here: it changes what happens on the way out, and this module
//! only cares about the way in.
//!
//! # Ctrl-C
//!
//! With `ISIG` off, the terminal does not raise SIGINT, so **Ctrl-C stops
//! stopping the program**. `read_key` gives `"ctrl+c"` and the program decides
//! what that means.
//!
//! That is a deliberate trade and the alternative is worse. If the signal were
//! left on, it would end the process without unwinding, so [`RawMode`]'s
//! `Drop` would not run and the terminal would be left with no echo and no line
//! buffering: the person is then typing blind into a shell that appears to have
//! frozen. Taking the signal off means the restore always happens.
//!
//! The cost is that a program which ignores `"ctrl+c"` cannot be stopped from
//! the keyboard. Section 8.11 of the specification says so, and so does the
//! wiki.

/// The terminal settings from before raw mode, put back when this is dropped.
///
/// `Drop` rather than a `restore` method, because the three ways a program can
/// end are a normal return, an error, and `exit`, and only one of those is a
/// place a call could be written. Rust unwinds through all three.
pub struct RawMode {
    #[cfg(unix)]
    saved: nix::sys::termios::Termios,
    #[cfg(windows)]
    saved: u32,
}

/// The keyboard `miru` gives a program.
///
/// Raw mode is entered on the first `read_key` and not before, so a program
/// that never calls it leaves the terminal exactly as it found it. That also
/// means `miru run` on a program with no `read_key` in it behaves as it always
/// has, including when standard input is a pipe.
pub struct RealKeyboard {
    raw: Option<RawMode>,
    /// Bytes read from the terminal but not yet used.
    ///
    /// An escape sequence is read as a group, and a sequence this module does
    /// not recognise leaves its tail here rather than being thrown away, so the
    /// next call sees those bytes rather than a fresh read.
    pending: Vec<u8>,
}

impl RealKeyboard {
    pub fn new() -> RealKeyboard {
        RealKeyboard {
            raw: None,
            pending: Vec::new(),
        }
    }

    /// The next byte, from what is left over or from the terminal.
    fn next_byte(&mut self) -> Result<Option<u8>, String> {
        if !self.pending.is_empty() {
            return Ok(Some(self.pending.remove(0)));
        }
        read_one_byte()
    }
}

impl miruscriptx::value::Keyboard for RealKeyboard {
    fn read_key(&mut self) -> Result<Option<String>, String> {
        if self.raw.is_none() {
            self.raw = Some(RawMode::enter()?);
        }
        let Some(first) = self.next_byte()? else {
            return Ok(None);
        };
        decode(first, &mut self.pending, |pending| {
            // Only consulted after an escape, to tell `Escape` pressed alone
            // from the start of a sequence. A key sends its whole sequence at
            // once, so anything that has not arrived within the wait was not
            // part of one.
            match read_byte_soon() {
                Ok(Some(byte)) => {
                    pending.push(byte);
                    true
                }
                _ => false,
            }
        })
    }
}

/// Turn one byte, and whatever follows it, into a key name.
///
/// Separated from the reading so it can be tested without a terminal. `more`
/// asks for another byte and says whether one arrived, appending it to
/// `pending`.
fn decode(
    first: u8,
    pending: &mut Vec<u8>,
    mut more: impl FnMut(&mut Vec<u8>) -> bool,
) -> Result<Option<String>, String> {
    let named = |name: &str| Ok(Some(name.to_string()));
    match first {
        0x1B => {
            // Escape, alone or starting a sequence.
            if !more(pending) {
                return named("escape");
            }
            let second = pending.remove(0);
            if second != b'[' && second != b'O' {
                // Not a sequence this module knows. Give the escape, and let
                // the byte that followed it be read next.
                pending.insert(0, second);
                return named("escape");
            }
            let mut body = Vec::new();
            loop {
                if !more(pending) {
                    break;
                }
                let byte = pending.remove(0);
                body.push(byte);
                // A sequence ends at a letter or a tilde.
                if byte.is_ascii_alphabetic() || byte == b'~' {
                    break;
                }
            }
            Ok(Some(sequence_name(&body)))
        }
        b'\r' | b'\n' => named("enter"),
        b'\t' => named("tab"),
        0x7F | 0x08 => named("backspace"),
        // A control code. Tab, Enter, and Backspace are named above rather than
        // as ctrl+i, ctrl+m, and ctrl+h, because a program checking for Tab
        // must not have to know that a terminal sends the same byte for both.
        0x01..=0x1A => Ok(Some(format!("ctrl+{}", (b'a' + first - 1) as char))),
        0x00 => named("ctrl+space"),
        // A printable character, or the start of a multi-byte one. UTF-8 says
        // how many bytes follow from the first.
        _ => {
            let extra = match first {
                0x00..=0x7F => 0,
                0xC0..=0xDF => 1,
                0xE0..=0xEF => 2,
                0xF0..=0xF7 => 3,
                // A continuation byte with nothing before it, which means the
                // stream is not valid UTF-8 from here.
                _ => return Err("the keyboard sent a character that is not UTF-8".to_string()),
            };
            let mut bytes = vec![first];
            for _ in 0..extra {
                if !more(pending) {
                    break;
                }
                bytes.push(pending.remove(0));
            }
            match String::from_utf8(bytes) {
                Ok(text) => Ok(Some(text)),
                Err(_) => Err("the keyboard sent a character that is not UTF-8".to_string()),
            }
        }
    }
}

/// The name for the body of an escape sequence, after the `[` or `O`.
///
/// An unrecognised sequence gives `"unknown"` rather than an error. A terminal
/// has more keys than this list, and a program in a loop should be able to
/// ignore one it does not know rather than stop.
fn sequence_name(body: &[u8]) -> String {
    let name = match body {
        b"A" => "up",
        b"B" => "down",
        b"C" => "right",
        b"D" => "left",
        b"H" | b"1~" | b"7~" => "home",
        b"F" | b"4~" | b"8~" => "end",
        b"2~" => "insert",
        b"3~" => "delete",
        b"5~" => "pageup",
        b"6~" => "pagedown",
        b"P" | b"11~" => "f1",
        b"Q" | b"12~" => "f2",
        b"R" | b"13~" => "f3",
        b"S" | b"14~" => "f4",
        b"15~" => "f5",
        b"17~" => "f6",
        b"18~" => "f7",
        b"19~" => "f8",
        b"20~" => "f9",
        b"21~" => "f10",
        b"23~" => "f11",
        b"24~" => "f12",
        _ => "unknown",
    };
    name.to_string()
}

// The two platform halves. Each supplies `RawMode::enter`, the `Drop` that puts
// the terminal back, and the two reads.

#[cfg(unix)]
mod platform {
    use super::RawMode;
    use nix::sys::termios::{self, LocalFlags, SetArg, SpecialCharacterIndices};
    use std::io::Read;

    impl RawMode {
        pub fn enter() -> Result<RawMode, String> {
            let stdin = std::io::stdin();
            let saved = termios::tcgetattr(&stdin)
                .map_err(|err| format!("cannot read the terminal settings: {err}"))?;
            let mut raw = saved.clone();
            raw.local_flags
                .remove(LocalFlags::ICANON | LocalFlags::ECHO | LocalFlags::ISIG);
            // One byte is enough to return, and there is no timeout: a read
            // waits for a key rather than spinning.
            raw.control_chars[SpecialCharacterIndices::VMIN as usize] = 1;
            raw.control_chars[SpecialCharacterIndices::VTIME as usize] = 0;
            termios::tcsetattr(&stdin, SetArg::TCSANOW, &raw)
                .map_err(|err| format!("cannot put the terminal into raw mode: {err}"))?;
            Ok(RawMode { saved })
        }
    }

    impl Drop for RawMode {
        fn drop(&mut self) {
            // Nothing to do about a failure here. The program is on its way
            // out, and there is no one left to tell.
            let _ = termios::tcsetattr(std::io::stdin(), SetArg::TCSANOW, &self.saved);
        }
    }

    pub fn read_one_byte() -> Result<Option<u8>, String> {
        let mut byte = [0u8; 1];
        match std::io::stdin().read(&mut byte) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(byte[0])),
            Err(err) => Err(format!("cannot read from the keyboard: {err}")),
        }
    }

    /// A byte if one is already there or arrives very soon, and `None`
    /// otherwise.
    ///
    /// This is what tells Escape pressed alone from the start of an escape
    /// sequence. A key sends its whole sequence in one burst, so a wait of a
    /// few milliseconds separates the two without being long enough for a
    /// person to notice.
    pub fn read_byte_soon() -> Result<Option<u8>, String> {
        use nix::poll::{PollFd, PollFlags, PollTimeout};
        use std::os::fd::AsFd;

        let stdin = std::io::stdin();
        let mut fds = [PollFd::new(stdin.as_fd(), PollFlags::POLLIN)];
        let ready = nix::poll::poll(&mut fds, PollTimeout::from(25u8))
            .map_err(|err| format!("cannot wait for the keyboard: {err}"))?;
        if ready == 0 {
            return Ok(None);
        }
        read_one_byte()
    }
}

#[cfg(windows)]
mod platform {
    use super::RawMode;
    use std::io::Read;
    use windows_sys::Win32::Foundation::{HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Console::{
        GetConsoleMode, GetStdHandle, SetConsoleMode, ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT,
        ENABLE_PROCESSED_INPUT, ENABLE_VIRTUAL_TERMINAL_INPUT, STD_INPUT_HANDLE,
    };

    fn stdin_handle() -> Result<HANDLE, String> {
        // SAFETY: `GetStdHandle` takes a constant and returns a handle or
        // `INVALID_HANDLE_VALUE`, which is checked here.
        let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            return Err("cannot reach the console".to_string());
        }
        Ok(handle)
    }

    impl RawMode {
        pub fn enter() -> Result<RawMode, String> {
            let handle = stdin_handle()?;
            let mut saved: u32 = 0;
            // SAFETY: `handle` came from `GetStdHandle` and was checked, and
            // `saved` is a live `u32` for the duration of the call.
            if unsafe { GetConsoleMode(handle, &mut saved) } == 0 {
                return Err("cannot read the console mode".to_string());
            }
            // ENABLE_VIRTUAL_TERMINAL_INPUT makes the console send the same
            // escape sequences a Unix terminal sends, so one decoder serves
            // both platforms rather than two that can disagree about what an
            // arrow key is.
            let raw = (saved & !(ENABLE_LINE_INPUT | ENABLE_ECHO_INPUT | ENABLE_PROCESSED_INPUT))
                | ENABLE_VIRTUAL_TERMINAL_INPUT;
            // SAFETY: as above.
            if unsafe { SetConsoleMode(handle, raw) } == 0 {
                return Err("cannot put the console into raw mode".to_string());
            }
            Ok(RawMode { saved })
        }
    }

    impl Drop for RawMode {
        fn drop(&mut self) {
            if let Ok(handle) = stdin_handle() {
                // SAFETY: as above. A failure here has nobody left to tell.
                unsafe {
                    SetConsoleMode(handle, self.saved);
                }
            }
        }
    }

    pub fn read_one_byte() -> Result<Option<u8>, String> {
        let mut byte = [0u8; 1];
        match std::io::stdin().read(&mut byte) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(byte[0])),
            Err(err) => Err(format!("cannot read from the keyboard: {err}")),
        }
    }

    /// The console has no `poll`, so this reads only what is already buffered.
    ///
    /// `PeekConsoleInput` would be the exact equivalent. It is not used here
    /// because the pending buffer already holds whatever a sequence delivered
    /// in one burst, and the console delivers one.
    pub fn read_byte_soon() -> Result<Option<u8>, String> {
        Ok(None)
    }
}

use platform::{read_byte_soon, read_one_byte};

#[cfg(test)]
mod tests {
    use super::*;

    /// Decode a key from bytes, with no terminal anywhere near it.
    fn key_from(bytes: &[u8]) -> Result<Option<String>, String> {
        let mut pending: Vec<u8> = bytes[1..].to_vec();
        decode(bytes[0], &mut pending, |left| !left.is_empty())
    }

    #[test]
    fn a_printable_key_is_itself() {
        assert_eq!(key_from(b"a").unwrap().unwrap(), "a");
        assert_eq!(key_from(b"A").unwrap().unwrap(), "A");
        assert_eq!(key_from(b" ").unwrap().unwrap(), " ");
        assert_eq!(key_from(b"7").unwrap().unwrap(), "7");
    }

    /// A key outside ASCII arrives as several bytes and comes back as one
    /// character, because the language counts characters everywhere else.
    #[test]
    fn a_multi_byte_character_is_one_key() {
        assert_eq!(key_from("é".as_bytes()).unwrap().unwrap(), "é");
        assert_eq!(key_from("😀".as_bytes()).unwrap().unwrap(), "😀");
    }

    #[test]
    fn the_arrows_are_named() {
        assert_eq!(key_from(b"\x1b[A").unwrap().unwrap(), "up");
        assert_eq!(key_from(b"\x1b[B").unwrap().unwrap(), "down");
        assert_eq!(key_from(b"\x1b[C").unwrap().unwrap(), "right");
        assert_eq!(key_from(b"\x1b[D").unwrap().unwrap(), "left");
    }

    #[test]
    fn the_editing_keys_are_named() {
        assert_eq!(key_from(b"\x1b[3~").unwrap().unwrap(), "delete");
        assert_eq!(key_from(b"\x1b[5~").unwrap().unwrap(), "pageup");
        assert_eq!(key_from(b"\x1b[6~").unwrap().unwrap(), "pagedown");
        assert_eq!(key_from(b"\x1b[H").unwrap().unwrap(), "home");
        assert_eq!(key_from(b"\x1b[F").unwrap().unwrap(), "end");
        assert_eq!(key_from(b"\x1bOP").unwrap().unwrap(), "f1");
        assert_eq!(key_from(b"\x1b[15~").unwrap().unwrap(), "f5");
    }

    /// Escape with nothing after it is Escape. This is the case the short wait
    /// in `read_byte_soon` exists for, and the one a decoder gets wrong by
    /// blocking forever on a key that sends one byte.
    #[test]
    fn escape_alone_is_escape() {
        assert_eq!(key_from(b"\x1b").unwrap().unwrap(), "escape");
    }

    /// Tab, Enter, and Backspace are named rather than given as the control
    /// letters that send the same bytes. A program checking for Tab must not
    /// have to know that Tab and Ctrl-I are one byte.
    #[test]
    fn the_keys_that_are_also_control_codes_keep_their_names() {
        assert_eq!(key_from(b"\t").unwrap().unwrap(), "tab");
        assert_eq!(key_from(b"\r").unwrap().unwrap(), "enter");
        assert_eq!(key_from(b"\n").unwrap().unwrap(), "enter");
        assert_eq!(key_from(b"\x7f").unwrap().unwrap(), "backspace");
        assert_eq!(key_from(b"\x08").unwrap().unwrap(), "backspace");
    }

    #[test]
    fn a_control_combination_is_named_for_its_letter() {
        assert_eq!(key_from(b"\x03").unwrap().unwrap(), "ctrl+c");
        assert_eq!(key_from(b"\x04").unwrap().unwrap(), "ctrl+d");
        assert_eq!(key_from(b"\x1a").unwrap().unwrap(), "ctrl+z");
    }

    /// A sequence this module does not know gives a name rather than an error,
    /// so a program in a loop can ignore a key it was not expecting.
    #[test]
    fn an_unknown_sequence_is_named_unknown() {
        assert_eq!(key_from(b"\x1b[99~").unwrap().unwrap(), "unknown");
    }

    /// A byte that cannot start a character is refused rather than guessed at.
    #[test]
    fn a_byte_that_is_not_utf8_is_an_error() {
        assert!(key_from(&[0x80]).is_err());
    }
}
