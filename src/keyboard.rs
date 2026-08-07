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

use std::io::Read;

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
/// Whether the terminal has been put into raw mode, and whether there is one.
enum Raw {
    /// No key has been read yet.
    Untried,
    /// Raw mode is on, and dropping this puts the terminal back.
    ///
    /// Nothing reads the value. Holding it *is* the mechanism, which is what
    /// the lint attribute records: if a future change starts reading it, the
    /// attribute fires and this comment gets revisited.
    On(
        #[expect(
            dead_code,
            reason = "held only for its Drop, which restores the terminal"
        )]
        RawMode,
    ),
    /// Standard input is not a terminal, so there is nothing to configure.
    ///
    /// A pipe still has bytes to read, and reading them needs no raw mode:
    /// nothing is buffering a line, because nothing is a terminal. Refusing
    /// here would make `read_key` fail in a shell pipeline for no reason the
    /// program could act on, and would leave the builtin with no end-to-end
    /// test at all.
    NotATerminal,
}

pub struct RealKeyboard {
    raw: Raw,
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
            raw: Raw::Untried,
            pending: Vec::new(),
        }
    }

    /// The next byte, from what is left over or from the terminal.
    fn next_byte(&mut self) -> Result<Option<u8>, String> {
        if self.pending.is_empty() {
            self.pending = read_bytes()?;
        }
        if self.pending.is_empty() {
            return Ok(None);
        }
        Ok(Some(self.pending.remove(0)))
    }
}

impl miruscriptx::value::Keyboard for RealKeyboard {
    fn read_key(&mut self) -> Result<Option<String>, String> {
        if matches!(self.raw, Raw::Untried) {
            self.raw = match RawMode::enter()? {
                Some(mode) => Raw::On(mode),
                None => Raw::NotATerminal,
            };
        }
        let Some(first) = self.next_byte()? else {
            return Ok(None);
        };
        decode(first, &mut self.pending, |pending| {
            // Only consulted after an escape, to tell `Escape` pressed alone
            // from the start of a sequence. A key sends its whole sequence at
            // once, so anything that has not arrived within the wait was not
            // part of one.
            if !pending.is_empty() {
                return true;
            }
            match read_bytes_soon() {
                Ok(more) if !more.is_empty() => {
                    pending.extend(more);
                    true
                }
                _ => false,
            }
        })
    }

    fn key_ready(&mut self) -> Result<bool, String> {
        // Bytes already read and not yet used mean the next `read_key` needs no
        // terminal at all. Checking this first is not an optimisation: the tail
        // of an unrecognised escape sequence lives here, and asking the platform
        // whether *more* has arrived would say no while a whole key sat in
        // this buffer waiting to be decoded.
        if !self.pending.is_empty() {
            return Ok(true);
        }
        // Raw mode is entered on the first read, and a program that asks
        // whether a key is ready is about to read one. Entering here as well
        // keeps the two calls agreeing about what standard input is; without
        // it, the first `key_ready` of a run would poll a terminal that is
        // still line-buffered and answer about a line rather than a key.
        if matches!(self.raw, Raw::Untried) {
            self.raw = match RawMode::enter()? {
                Some(mode) => Raw::On(mode),
                None => Raw::NotATerminal,
            };
        }
        bytes_waiting()
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

    impl RawMode {
        pub fn enter() -> Result<Option<RawMode>, String> {
            let stdin = std::io::stdin();
            let Ok(saved) = termios::tcgetattr(&stdin) else {
                // Not a terminal, which is what a pipe or a file gives: there
                // is nothing to put into raw mode and nothing to put back.
                //
                // Any failure means that, rather than one specific errno.
                // Matching only ENOTTY passed on Linux and failed on macOS,
                // which reports a different one for the same situation, and
                // the distinction was never useful: if the settings cannot be
                // read, they cannot be changed either.
                return Ok(None);
            };
            let mut raw = saved.clone();
            raw.local_flags
                .remove(LocalFlags::ICANON | LocalFlags::ECHO | LocalFlags::ISIG);
            // One byte is enough to return, and there is no timeout: a read
            // waits for a key rather than spinning.
            raw.control_chars[SpecialCharacterIndices::VMIN as usize] = 1;
            raw.control_chars[SpecialCharacterIndices::VTIME as usize] = 0;
            termios::tcsetattr(&stdin, SetArg::TCSANOW, &raw)
                .map_err(|err| format!("cannot put the terminal into raw mode: {err}"))?;
            Ok(Some(RawMode { saved }))
        }
    }

    impl Drop for RawMode {
        fn drop(&mut self) {
            // Nothing to do about a failure here. The program is on its way
            // out, and there is no one left to tell.
            let _ = termios::tcsetattr(std::io::stdin(), SetArg::TCSANOW, &self.saved);
        }
    }

    pub fn read_bytes() -> Result<Vec<u8>, String> {
        super::read_burst(&mut std::io::stdin())
    }

    /// A byte if one is already there or arrives very soon, and `None`
    /// otherwise.
    ///
    /// This is what tells Escape pressed alone from the start of an escape
    /// sequence. A key sends its whole sequence in one burst, so a wait of a
    /// few milliseconds separates the two without being long enough for a
    /// person to notice.
    pub fn read_bytes_soon() -> Result<Vec<u8>, String> {
        use nix::poll::{PollFd, PollFlags, PollTimeout};
        use std::os::fd::AsFd;

        let stdin = std::io::stdin();
        let mut fds = [PollFd::new(stdin.as_fd(), PollFlags::POLLIN)];
        let ready = nix::poll::poll(&mut fds, PollTimeout::from(25u8))
            .map_err(|err| format!("cannot wait for the keyboard: {err}"))?;
        if ready == 0 {
            return Ok(Vec::new());
        }
        read_bytes()
    }

    /// Whether a read of standard input would return without waiting.
    ///
    /// `poll` with no timeout at all answers exactly that question, and answers
    /// it correctly for a terminal, a pipe, and a file alike. **A closed stream
    /// polls as readable**, which is the behaviour `Keyboard::key_ready`
    /// requires: the read that follows gives nothing and the caller's loop ends,
    /// rather than waiting for a key that will never come.
    pub fn bytes_waiting() -> Result<bool, String> {
        use nix::poll::{PollFd, PollFlags, PollTimeout};
        use std::os::fd::AsFd;

        let stdin = std::io::stdin();
        let mut fds = [PollFd::new(stdin.as_fd(), PollFlags::POLLIN)];
        let ready = nix::poll::poll(&mut fds, PollTimeout::ZERO)
            .map_err(|err| format!("cannot check the keyboard: {err}"))?;
        Ok(ready != 0)
    }
}

#[cfg(windows)]
mod platform {
    use super::RawMode;
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
        pub fn enter() -> Result<Option<RawMode>, String> {
            let handle = stdin_handle()?;
            let mut saved: u32 = 0;
            // SAFETY: `handle` came from `GetStdHandle` and was checked, and
            // `saved` is a live `u32` for the duration of the call.
            //
            // A failure here means the handle is not a console, which is what
            // a pipe or a redirected file gives. Same answer as ENOTTY on
            // unix: there is nothing to configure, and the bytes are readable
            // anyway.
            if unsafe { GetConsoleMode(handle, &mut saved) } == 0 {
                return Ok(None);
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
            Ok(Some(RawMode { saved }))
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

    pub fn read_bytes() -> Result<Vec<u8>, String> {
        super::read_burst(&mut std::io::stdin())
    }

    /// Whatever else has already arrived, or nothing.
    ///
    /// **This used to answer "nothing" always, and that was a defect rather
    /// than a limitation.** The reasoning was that windows has no `poll` over
    /// standard input, and that one read of [`READ_SIZE`] bytes makes a split
    /// sequence very unlikely anyway: a console delivers a key's whole
    /// sequence at once, and a pipe delivers far more than one key. The second
    /// half is where it went wrong. A pipe does deliver far more than one key
    /// — and stops at sixteen bytes, which is five whole arrow keys and the
    /// escape of a sixth. The tail was stranded, the escape read as Escape
    /// pressed alone, and a game that quits on Escape quit in the middle of
    /// its input, on windows and nowhere else.
    ///
    /// There is no `poll` here, but there has been an answer to this exact
    /// question since 1.8: [`stdin_is_ready`] asks whether a read would block,
    /// dispatching on the handle type because a console, a pipe, and a file
    /// each answer differently. `key_ready` was built on it. Asking it here
    /// costs nothing new and keeps both cases right — a pipe holding the rest
    /// of a sequence says yes, and Escape pressed alone at a console with
    /// nothing behind it still says no, so it still reads as Escape.
    pub fn read_bytes_soon() -> Result<Vec<u8>, String> {
        if stdin_is_ready()? {
            read_bytes()
        } else {
            Ok(Vec::new())
        }
    }

    /// Whether a read of standard input would return without waiting.
    ///
    /// **Windows has no one call that answers this for every kind of handle**,
    /// which is the whole difficulty: `read_bytes_soon` above says the same
    /// thing about its own problem. `poll` on unix answers for a console, a
    /// pipe, and a file alike; here each has to be asked in its own way, so the
    /// first thing to establish is which one this is.
    ///
    /// An earlier version answered `true` for everything that was not a
    /// console, on the reasoning that a pipe or a file reads promptly. **That
    /// was wrong, and it made every game freeze under a pipe.** A pipe whose
    /// writer is still open and has written nothing does not read promptly: it
    /// blocks until something arrives. A game polls, is told it can read, calls
    /// `read_key`, and stops dead — which is exactly the state this builtin
    /// exists to prevent. `pong_keeps_moving_while_nothing_is_pressed` holds a
    /// pipe open and silent and is what caught it.
    fn stdin_is_ready() -> Result<bool, String> {
        use windows_sys::Win32::Storage::FileSystem::{
            GetFileType, FILE_TYPE_CHAR, FILE_TYPE_PIPE,
        };

        let handle = stdin_handle()?;
        // SAFETY: `handle` is a valid standard-input handle from `stdin_handle`.
        match unsafe { GetFileType(handle) } {
            FILE_TYPE_CHAR => {
                // A console, or a character device such as `NUL`. Only the
                // first has an input buffer to peek at; `GetConsoleMode`
                // separates them, which is the same question `RawMode::enter`
                // asks to decide whether there is a terminal at all.
                let mut mode = 0u32;
                // SAFETY: `handle` is valid and `mode` is live for the call.
                if unsafe { GetConsoleMode(handle, &mut mode) } == 0 {
                    // `NUL` and its like read zero bytes at once, which is end
                    // of input, which is ready.
                    return Ok(true);
                }
                console_has_a_key(handle)
            }
            FILE_TYPE_PIPE => pipe_has_bytes(handle),
            // A disk file, or a type this does not know. A file read returns
            // at once with bytes or with end of input, so it is always ready.
            _ => Ok(true),
        }
    }

    /// Whether the console's input buffer holds a record that will yield a byte.
    ///
    /// **Counting would be a defect here rather than an approximation.**
    /// `GetNumberOfConsoleInputEvents` counts every input record: mouse
    /// movement, a window resize, a focus change, and a key being *released* as
    /// well as pressed. None of those produce a byte to read. A count above zero
    /// would say "ready", the caller would call `read_key`, and the program
    /// would stop dead inside a blocking read because the mouse moved.
    ///
    /// `PeekConsoleInputW` leaves the records in the buffer, so the read that
    /// follows still sees them.
    fn console_has_a_key(handle: HANDLE) -> Result<bool, String> {
        use windows_sys::Win32::System::Console::{PeekConsoleInputW, INPUT_RECORD, KEY_EVENT};

        // Sixteen is enough that a burst of held keys does not hide a real one
        // behind a run of mouse records, and small enough to sit on the stack.
        let mut records: [INPUT_RECORD; 16] = unsafe { std::mem::zeroed() };
        let mut read = 0u32;
        // SAFETY: `records` is a live array of `records.len()` elements and
        // `read` is a live u32. `PeekConsoleInputW` writes at most that many.
        let ok = unsafe {
            PeekConsoleInputW(
                handle,
                records.as_mut_ptr(),
                records.len() as u32,
                &mut read,
            )
        };
        if ok == 0 {
            return Err("cannot check the console".to_string());
        }

        for record in records.iter().take(read as usize) {
            if record.EventType != KEY_EVENT as u16 {
                continue;
            }
            // SAFETY: the union is read as the variant `EventType` names.
            let key = unsafe { record.Event.KeyEvent };
            // A release produces no byte, and neither does a bare modifier,
            // which arrives as a key-down whose character is zero.
            if key.bKeyDown != 0 && unsafe { key.uChar.UnicodeChar } != 0 {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Whether a pipe has bytes waiting, without taking them.
    ///
    /// **A closed pipe is ready**, and that is the case worth stating. When the
    /// writer has gone, `PeekNamedPipe` fails with `ERROR_BROKEN_PIPE`; the
    /// read that follows returns nothing at once, which is end of input. Saying
    /// `false` there would be the ruinous answer: `read_key` would never be
    /// called, so the program would never learn that no key is coming, and a
    /// loop guarded by `key_ready` would spin forever. `poll` on unix reports a
    /// closed stream as readable for the same reason, and section 8.11 of the
    /// specification states the rule for both.
    fn pipe_has_bytes(handle: HANDLE) -> Result<bool, String> {
        use windows_sys::Win32::Foundation::{GetLastError, ERROR_BROKEN_PIPE};
        use windows_sys::Win32::System::Pipes::PeekNamedPipe;

        let mut available = 0u32;
        // SAFETY: `handle` is a valid pipe handle and `available` is live. The
        // null arguments are the optional buffer, byte count, and message
        // length, none of which are wanted: this asks only how much is there.
        let ok = unsafe {
            PeekNamedPipe(
                handle,
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            // SAFETY: read immediately after the failing call, as documented.
            let error = unsafe { GetLastError() };
            if error == ERROR_BROKEN_PIPE {
                return Ok(true);
            }
            return Err(format!("cannot check the keyboard: error {error}"));
        }
        Ok(available > 0)
    }

    pub fn bytes_waiting() -> Result<bool, String> {
        stdin_is_ready()
    }
}

use platform::{bytes_waiting, read_bytes, read_bytes_soon};

/// How many bytes one read asks for.
///
/// **Not one, because Windows refuses it.** Rust's standard input there rejects
/// a buffer that cannot hold one whole character when the handle is a console,
/// so a one-byte read fails on the exact platform this module exists for.
///
/// **Not four either, and this is the size that matters.** A key sends its
/// whole escape sequence in one burst, but a *pipe* delivers several keys at
/// once, and a read that stops in the middle of the second sequence leaves the
/// rest to a follow-up read that Windows cannot make. That produced a real
/// fault: `ESC [ C ESC` filled a four-byte buffer, the trailing escape was
/// read as Escape pressed alone, and the arrow-key example quit early on
/// Windows and nowhere else. Sixteen holds any sequence this decoder knows
/// several times over, so the follow-up read is needed only at a terminal,
/// where it works.
const READ_SIZE: usize = 16;

/// Read at least one byte and at most [`READ_SIZE`], blocking until one
/// arrives.
fn read_burst(source: &mut impl Read) -> Result<Vec<u8>, String> {
    let mut buffer = [0u8; READ_SIZE];
    match source.read(&mut buffer) {
        Ok(0) => Ok(Vec::new()),
        Ok(read) => Ok(buffer[..read].to_vec()),
        Err(err) => Err(format!("cannot read from the keyboard: {err}")),
    }
}

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

    /// Several keys arriving in one read, decoded with **no follow-up read
    /// available**. This is the Windows condition exactly: there is no `poll`
    /// there, so `read_bytes_soon` always says nothing more arrived, and the
    /// decoder has to work from the buffer alone.
    ///
    /// It is a regression test for a real fault. With a four-byte read,
    /// `ESC [ C ESC` filled the buffer, the trailing escape had no follow-up
    /// read to complete it, and it decoded as Escape pressed alone. The
    /// arrow-key example quit at the second key on Windows and nowhere else,
    /// and only the platform matrix caught it.
    #[test]
    fn a_burst_of_several_keys_decodes_without_a_follow_up_read() {
        // The same loop `RealKeyboard::read_key` runs, over bytes rather than
        // a terminal, with `more` refusing to fetch anything: reading from the
        // buffer alone is all Windows can do.
        let mut source: &[u8] = b"\x1b[C\x1b[C\x1b[Dq";
        let mut pending: Vec<u8> = Vec::new();
        let mut keys = Vec::new();
        loop {
            if pending.is_empty() {
                pending = read_burst(&mut source).expect("the read works");
            }
            if pending.is_empty() {
                break;
            }
            let first = pending.remove(0);
            let key = decode(first, &mut pending, |left| !left.is_empty())
                .expect("the bytes decode")
                .expect("a key");
            keys.push(key);
        }
        assert_eq!(keys, vec!["right", "right", "left", "q"]);
    }

    /// Drive the same loop `RealKeyboard::read_key` runs, over bytes, with
    /// `more` deciding whether a follow-up read is available.
    fn keys_from_burst(mut source: &[u8], follow_up: bool) -> Vec<String> {
        let mut pending: Vec<u8> = Vec::new();
        let mut keys = Vec::new();
        loop {
            if pending.is_empty() {
                pending = read_burst(&mut source).expect("the read works");
            }
            if pending.is_empty() {
                break;
            }
            let first = pending.remove(0);
            let key = decode(first, &mut pending, |left| {
                if !left.is_empty() {
                    return true;
                }
                if !follow_up {
                    return false;
                }
                match read_burst(&mut source) {
                    Ok(more) if !more.is_empty() => {
                        left.extend(more);
                        true
                    }
                    _ => false,
                }
            })
            .expect("the bytes decode")
            .expect("a key");
            keys.push(key);
        }
        keys
    }

    /// **More keys than one read holds, and the boundary lands inside an
    /// escape sequence.**
    ///
    /// [`READ_SIZE`] is 16 and an arrow is three bytes, so six of them fill one
    /// read with five whole sequences and a lone escape. Whether that sixth key
    /// survives is decided entirely by whether a follow-up read is available,
    /// which is the one thing unix has and windows does not.
    ///
    /// Both halves are asserted here because the difference between them is the
    /// defect. `a_burst_of_several_keys_decodes_without_a_follow_up_read` above
    /// covers the short case and cannot see this: ten bytes never reach the
    /// boundary.
    #[test]
    fn a_sequence_split_across_two_reads_needs_a_follow_up_read() {
        let six_arrows = b"\x1b[C".repeat(6);
        assert!(
            six_arrows.len() > READ_SIZE,
            "the input must span two reads"
        );

        // What unix does, and what windows must do once `read_bytes_soon`
        // consults `stdin_is_ready` rather than always saying no.
        assert_eq!(keys_from_burst(&six_arrows, true), vec!["right"; 6]);

        // What windows did before that: the escape at the end of the first
        // read has nothing to complete it, so it reads as Escape pressed
        // alone and the two bytes behind it decode on their own. A game that
        // quits on Escape quits here, in the middle of its input.
        let stranded = keys_from_burst(&six_arrows, false);
        assert_eq!(&stranded[..5], &["right"; 5]);
        assert_eq!(stranded[5], "escape");
        assert_ne!(
            stranded.len(),
            6,
            "the tail of the split sequence has to go somewhere"
        );
    }

    /// A byte that cannot start a character is refused rather than guessed at.
    #[test]
    fn a_byte_that_is_not_utf8_is_an_error() {
        assert!(key_from(&[0x80]).is_err());
    }
}
