//! Translating egui key events into the byte sequences a terminal application expects
//! (xterm conventions).

use egui::{Key, Modifiers};

/// Terminal modes that affect how keys are encoded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KeyModes {
    /// DECCKM: cursor keys send `ESC O x` instead of `ESC [ x`.
    pub app_cursor: bool,
    /// DECKPAM: keypad application mode (affects Home/End in some apps).
    pub app_keypad: bool,
}

/// xterm modifier parameter: 1 + shift + 2*alt + 4*ctrl (0 means "no modifiers").
fn modifier_param(m: Modifiers) -> u8 {
    let mut p = 0;
    if m.shift {
        p |= 1;
    }
    if m.alt {
        p |= 2;
    }
    if m.ctrl {
        p |= 4;
    }
    if p == 0 { 0 } else { p + 1 }
}

/// Encode a key press that is not plain text. Returns `None` when the key should be handled
/// as text (via `Event::Text`) or is not a terminal key at all.
///
/// `mods.alt` is treated as Meta (ESC prefix) for letters, digits and editing keys, the way
/// iTerm2's "Esc+" option and most Linux terminals behave.
pub fn encode_key(key: Key, mods: Modifiers, modes: KeyModes) -> Option<Vec<u8>> {
    // Cmd (macOS) combinations belong to the host UI (copy, paste, close tab...).
    if mods.mac_cmd || (cfg!(target_os = "macos") && mods.command && !mods.ctrl) {
        return None;
    }
    let mp = modifier_param(mods);

    // Cursor keys: CSI/SS3 with an optional modifier parameter.
    let cursor = |c: char| -> Vec<u8> {
        if mp != 0 {
            format!("\x1b[1;{mp}{c}").into_bytes()
        } else if modes.app_cursor {
            format!("\x1bO{c}").into_bytes()
        } else {
            format!("\x1b[{c}").into_bytes()
        }
    };
    // `CSI n ~` keys.
    let tilde = |n: u8| -> Vec<u8> {
        if mp != 0 { format!("\x1b[{n};{mp}~").into_bytes() } else { format!("\x1b[{n}~").into_bytes() }
    };
    // F1-F4 use SS3 P..S, or CSI 1;m P..S with modifiers.
    let ss3 = |c: char| -> Vec<u8> {
        if mp != 0 { format!("\x1b[1;{mp}{c}").into_bytes() } else { format!("\x1bO{c}").into_bytes() }
    };
    let meta = |bytes: &[u8]| -> Vec<u8> {
        let mut v = Vec::with_capacity(bytes.len() + 1);
        if mods.alt {
            v.push(0x1b);
        }
        v.extend_from_slice(bytes);
        v
    };

    let bytes = match key {
        Key::ArrowUp => cursor('A'),
        Key::ArrowDown => cursor('B'),
        Key::ArrowRight => cursor('C'),
        Key::ArrowLeft => cursor('D'),
        Key::Home => cursor('H'),
        Key::End => cursor('F'),
        Key::PageUp => tilde(5),
        Key::PageDown => tilde(6),
        Key::Insert => tilde(2),
        Key::Delete => tilde(3),
        Key::F1 => ss3('P'),
        Key::F2 => ss3('Q'),
        Key::F3 => ss3('R'),
        Key::F4 => ss3('S'),
        Key::F5 => tilde(15),
        Key::F6 => tilde(17),
        Key::F7 => tilde(18),
        Key::F8 => tilde(19),
        Key::F9 => tilde(20),
        Key::F10 => tilde(21),
        Key::F11 => tilde(23),
        Key::F12 => tilde(24),
        Key::Enter => meta(b"\r"),
        Key::Tab if mods.shift => b"\x1b[Z".to_vec(),
        Key::Tab => meta(b"\t"),
        Key::Backspace if mods.ctrl => meta(b"\x08"),
        Key::Backspace => meta(b"\x7f"),
        Key::Escape => b"\x1b".to_vec(),
        Key::Space if mods.ctrl => meta(b"\x00"),
        _ => {
            if mods.ctrl {
                return ctrl_key(key).map(|b| meta(&[b]));
            }
            if mods.alt {
                // Alt+letter/digit as Meta: ESC followed by the plain character.
                return plain_char(key, mods.shift).map(|c| {
                    let mut v = vec![0x1b];
                    let mut buf = [0u8; 4];
                    v.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
                    v
                });
            }
            return None;
        }
    };
    Some(bytes)
}

/// The control code for Ctrl+`key`, if any.
fn ctrl_key(key: Key) -> Option<u8> {
    if let Some(c) = letter(key) {
        return Some(c as u8 - b'a' + 1);
    }
    Some(match key {
        Key::OpenBracket => 0x1b,
        Key::Backslash => 0x1c,
        Key::CloseBracket => 0x1d,
        Key::Num6 => 0x1e,
        Key::Minus => 0x1f,
        Key::Num2 => 0x00,
        Key::Slash => 0x1f,
        Key::Num3 => 0x1b,
        Key::Num4 => 0x1c,
        Key::Num5 => 0x1d,
        Key::Num7 => 0x1f,
        Key::Num8 => 0x7f,
        _ => return None,
    })
}

fn letter(key: Key) -> Option<char> {
    let name = key.name();
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => Some(c.to_ascii_lowercase()),
        _ => None,
    }
}

/// The character an unmodified (or shifted) letter/digit/punctuation key types on a US layout.
fn plain_char(key: Key, shift: bool) -> Option<char> {
    if let Some(c) = letter(key) {
        return Some(if shift { c.to_ascii_uppercase() } else { c });
    }
    let (normal, shifted) = match key {
        Key::Num0 => ('0', ')'),
        Key::Num1 => ('1', '!'),
        Key::Num2 => ('2', '@'),
        Key::Num3 => ('3', '#'),
        Key::Num4 => ('4', '$'),
        Key::Num5 => ('5', '%'),
        Key::Num6 => ('6', '^'),
        Key::Num7 => ('7', '&'),
        Key::Num8 => ('8', '*'),
        Key::Num9 => ('9', '('),
        Key::Period => ('.', '>'),
        Key::Comma => (',', '<'),
        Key::Minus => ('-', '_'),
        Key::Slash => ('/', '?'),
        Key::Semicolon => (';', ':'),
        Key::OpenBracket => ('[', '{'),
        Key::CloseBracket => (']', '}'),
        Key::Backslash => ('\\', '|'),
        Key::Quote => ('\'', '"'),
        Key::Backtick => ('`', '~'),
        Key::Equals => ('=', '+'),
        _ => return None,
    };
    Some(if shift { shifted } else { normal })
}

/// Wrap pasted text in bracketed-paste markers when the application asked for it, and
/// normalize newlines to carriage returns as terminals do.
pub fn encode_paste(text: &str, bracketed: bool) -> Vec<u8> {
    // Strip any embedded paste terminators so pasted text cannot escape bracketed mode.
    let cleaned = text.replace("\x1b[201~", "").replace("\r\n", "\r").replace('\n', "\r");
    if bracketed {
        let mut v = b"\x1b[200~".to_vec();
        v.extend_from_slice(cleaned.as_bytes());
        v.extend_from_slice(b"\x1b[201~");
        v
    } else {
        cleaned.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Modifiers = Modifiers::NONE;

    fn m(shift: bool, alt: bool, ctrl: bool) -> Modifiers {
        Modifiers { alt, ctrl, shift, mac_cmd: false, command: false }
    }

    fn enc(key: Key, mods: Modifiers) -> Option<String> {
        encode_key(key, mods, KeyModes::default()).map(|b| String::from_utf8(b).unwrap())
    }

    #[test]
    fn arrows_normal_and_app_cursor() {
        assert_eq!(enc(Key::ArrowUp, NONE).unwrap(), "\x1b[A");
        let app = KeyModes { app_cursor: true, app_keypad: false };
        assert_eq!(encode_key(Key::ArrowLeft, NONE, app).unwrap(), b"\x1bOD");
        // Modifiers override app-cursor mode and use the CSI 1;m form.
        assert_eq!(encode_key(Key::ArrowRight, m(false, false, true), app).unwrap(), b"\x1b[1;5C");
        assert_eq!(enc(Key::ArrowDown, m(true, true, false)).unwrap(), "\x1b[1;4B");
    }

    #[test]
    fn control_letters() {
        assert_eq!(enc(Key::C, m(false, false, true)).unwrap(), "\x03");
        assert_eq!(enc(Key::A, m(false, false, true)).unwrap(), "\x01");
        assert_eq!(enc(Key::Z, m(false, false, true)).unwrap(), "\x1a");
        assert_eq!(enc(Key::OpenBracket, m(false, false, true)).unwrap(), "\x1b");
        assert_eq!(enc(Key::Space, m(false, false, true)).unwrap(), "\x00");
    }

    #[test]
    fn alt_is_meta() {
        assert_eq!(enc(Key::B, m(false, true, false)).unwrap(), "\x1bb");
        assert_eq!(enc(Key::F, m(true, true, false)).unwrap(), "\x1bF");
        assert_eq!(enc(Key::Backspace, m(false, true, false)).unwrap(), "\x1b\x7f");
        assert_eq!(enc(Key::Num1, m(false, true, false)).unwrap(), "\x1b1");
    }

    #[test]
    fn editing_and_function_keys() {
        assert_eq!(enc(Key::Enter, NONE).unwrap(), "\r");
        assert_eq!(enc(Key::Backspace, NONE).unwrap(), "\x7f");
        assert_eq!(enc(Key::Tab, NONE).unwrap(), "\t");
        assert_eq!(enc(Key::Tab, m(true, false, false)).unwrap(), "\x1b[Z");
        assert_eq!(enc(Key::Delete, NONE).unwrap(), "\x1b[3~");
        assert_eq!(enc(Key::PageUp, m(true, false, false)).unwrap(), "\x1b[5;2~");
        assert_eq!(enc(Key::F1, NONE).unwrap(), "\x1bOP");
        assert_eq!(enc(Key::F5, NONE).unwrap(), "\x1b[15~");
        assert_eq!(enc(Key::F12, NONE).unwrap(), "\x1b[24~");
        assert_eq!(enc(Key::Home, NONE).unwrap(), "\x1b[H");
        assert_eq!(enc(Key::Escape, NONE).unwrap(), "\x1b");
    }

    #[test]
    fn plain_letters_are_text_not_keys() {
        assert_eq!(enc(Key::A, NONE), None);
        assert_eq!(enc(Key::A, m(true, false, false)), None);
    }

    #[test]
    fn cmd_combinations_are_left_to_the_host() {
        let cmd = Modifiers { mac_cmd: true, command: true, ..Default::default() };
        assert_eq!(enc(Key::C, cmd), None);
        assert_eq!(enc(Key::ArrowUp, cmd), None);
    }

    #[test]
    fn paste_brackets_and_sanitizes() {
        assert_eq!(encode_paste("a\nb", false), b"a\rb");
        assert_eq!(encode_paste("x\x1b[201~y", true), b"\x1b[200~xy\x1b[201~");
    }
}
