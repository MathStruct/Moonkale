//! Key events → bytes for the PTY (xterm-256color conventions): printable
//! text as typed, control keys as their escape sequences, Ctrl+letter as
//! C0 controls, Alt+key as ESC-prefixed.

/// `key` is the DOM key name (`"a"`, `"Enter"`, `"ArrowUp"`, …).
pub fn encode(key: &str, ctrl: bool, alt: bool, shift: bool, app_cursor: bool) -> Option<Vec<u8>> {
    let mut out: Vec<u8> = Vec::new();
    if alt && key.chars().count() == 1 {
        out.push(0x1b);
    }
    let cursor = |c: u8| -> Vec<u8> {
        if app_cursor {
            vec![0x1b, b'O', c]
        } else {
            vec![0x1b, b'[', c]
        }
    };
    let bytes: Vec<u8> = match key {
        "Enter" => vec![b'\r'],
        "Tab" => {
            if shift {
                b"\x1b[Z".to_vec()
            } else {
                vec![b'\t']
            }
        }
        "Backspace" => vec![0x7f],
        "Escape" => vec![0x1b],
        "Delete" => b"\x1b[3~".to_vec(),
        "Insert" => b"\x1b[2~".to_vec(),
        "ArrowUp" => cursor(b'A'),
        "ArrowDown" => cursor(b'B'),
        "ArrowRight" => cursor(b'C'),
        "ArrowLeft" => cursor(b'D'),
        "Home" => cursor(b'H'),
        "End" => cursor(b'F'),
        "PageUp" => b"\x1b[5~".to_vec(),
        "PageDown" => b"\x1b[6~".to_vec(),
        "F1" => b"\x1bOP".to_vec(),
        "F2" => b"\x1bOQ".to_vec(),
        "F3" => b"\x1bOR".to_vec(),
        "F4" => b"\x1bOS".to_vec(),
        "F5" => b"\x1b[15~".to_vec(),
        "F6" => b"\x1b[17~".to_vec(),
        "F7" => b"\x1b[18~".to_vec(),
        "F8" => b"\x1b[19~".to_vec(),
        "F9" => b"\x1b[20~".to_vec(),
        "F10" => b"\x1b[21~".to_vec(),
        "F11" => b"\x1b[23~".to_vec(),
        "F12" => b"\x1b[24~".to_vec(),
        // Modifier keys alone, dead keys, IME composition: nothing.
        "Shift" | "Control" | "Alt" | "Meta" | "CapsLock" | "Dead" | "Unidentified" | "Process"
        | "Compose" | "OS" | "AltGraph" | "NumLock" | "ScrollLock" => return None,
        k => {
            let mut chars = k.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                return None; // an unknown named key
            };
            if ctrl {
                let u = c.to_ascii_lowercase() as u32;
                match c {
                    'a'..='z' | 'A'..='Z' => vec![(u - 'a' as u32 + 1) as u8],
                    '[' | '3' => vec![0x1b],
                    '\\' | '4' => vec![0x1c],
                    ']' | '5' => vec![0x1d],
                    '^' | '6' => vec![0x1e],
                    '_' | '7' | '/' => vec![0x1f],
                    ' ' | '2' | '@' => vec![0x00],
                    '?' | '8' => vec![0x7f],
                    _ => return None,
                }
            } else {
                c.to_string().into_bytes()
            }
        }
    };
    out.extend(bytes);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::encode;

    #[test]
    fn keys_become_bytes() {
        assert_eq!(encode("a", false, false, false, false), Some(b"a".to_vec()));
        assert_eq!(
            encode("é", false, false, false, false),
            Some("é".as_bytes().to_vec())
        );
        assert_eq!(
            encode("Enter", false, false, false, false),
            Some(b"\r".to_vec())
        );
        assert_eq!(encode("c", true, false, false, false), Some(vec![3]));
        assert_eq!(encode("C", true, false, true, false), Some(vec![3]));
        assert_eq!(
            encode("ArrowUp", false, false, false, false),
            Some(b"\x1b[A".to_vec())
        );
        assert_eq!(
            encode("ArrowUp", false, false, false, true),
            Some(b"\x1bOA".to_vec())
        );
        assert_eq!(
            encode("x", false, true, false, false),
            Some(b"\x1bx".to_vec())
        );
        assert_eq!(
            encode("Tab", false, false, true, false),
            Some(b"\x1b[Z".to_vec())
        );
        assert_eq!(encode("Shift", false, false, true, false), None);
        assert_eq!(encode("Dead", false, false, false, false), None);
    }
}
