//! Keys made of parts, encoded so that byte order is the order of the parts:
//! a scan over `Key::new().str(folder)` returns that folder's records only,
//! and `(folder, seq)` keys come back sorted by `seq`.
//!
//! - a string: its UTF-8 bytes, a `0x00` inside written as `0x00 0xFF`, then
//!   the terminator `0x00 0x01` — so `"a"` < `"a\0"` < `"ab"`, and no string
//!   is a prefix of another's encoding;
//! - a `u64`: 8 bytes big-endian.

/// A key under construction ([`Key::new`]) or being read back
/// ([`Key::reader`]).
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct Key(Vec<u8>);

impl Key {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Append a string part.
    pub fn str(mut self, s: &str) -> Self {
        for &b in s.as_bytes() {
            if b == 0 {
                self.0.extend_from_slice(&[0x00, 0xFF]);
            } else {
                self.0.push(b);
            }
        }
        self.0.extend_from_slice(&[0x00, 0x01]);
        self
    }

    /// Append a number part (sorts numerically).
    pub fn u64(mut self, n: u64) -> Self {
        self.0.extend_from_slice(&n.to_be_bytes());
        self
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// Read the parts of an encoded key back, in the order they were written.
    pub fn reader(bytes: &[u8]) -> KeyReader<'_> {
        KeyReader { rest: bytes }
    }
}

impl From<Key> for Vec<u8> {
    fn from(k: Key) -> Self {
        k.0
    }
}

/// Reads the parts of an encoded key; `None` when the next part is not of the
/// asked type (or the key is used up).
pub struct KeyReader<'a> {
    rest: &'a [u8],
}

impl KeyReader<'_> {
    pub fn str(&mut self) -> Option<String> {
        let mut out = Vec::new();
        let mut i = 0;
        loop {
            let b = *self.rest.get(i)?;
            if b == 0 {
                match self.rest.get(i + 1)? {
                    0x01 => {
                        self.rest = &self.rest[i + 2..];
                        return String::from_utf8(out).ok();
                    }
                    0xFF => {
                        out.push(0);
                        i += 2;
                    }
                    _ => return None,
                }
            } else {
                out.push(b);
                i += 1;
            }
        }
    }

    pub fn u64(&mut self) -> Option<u64> {
        let (head, rest) = self.rest.split_first_chunk::<8>()?;
        self.rest = rest;
        Some(u64::from_be_bytes(*head))
    }

    pub fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_order_is_part_order() {
        let mut keys = [
            Key::new().str("ab"),
            Key::new().str("a\0"),
            Key::new().str("a"),
            Key::new().str("a").u64(10),
            Key::new().str("a").u64(2),
            Key::new().str(""),
        ];
        keys.sort();
        let read: Vec<(String, Option<u64>)> = keys
            .iter()
            .map(|k| {
                let mut r = Key::reader(k.as_bytes());
                (r.str().unwrap(), r.u64())
            })
            .collect();
        assert_eq!(
            read,
            [
                ("".into(), None),
                ("a".into(), None),
                ("a".into(), Some(2)),
                ("a".into(), Some(10)),
                ("a\0".into(), None),
                ("ab".into(), None),
            ]
        );
    }

    #[test]
    fn a_string_part_is_a_clean_prefix() {
        let folder = Key::new().str("folder:a");
        let record = Key::new().str("folder:a").u64(7);
        let other = Key::new().str("folder:ab").u64(7);
        assert!(record.as_bytes().starts_with(folder.as_bytes()));
        assert!(!other.as_bytes().starts_with(folder.as_bytes()));
    }
}
