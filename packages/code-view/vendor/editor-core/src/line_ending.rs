//! Line ending helpers.
//!
//! `editor-core` stores text internally using LF (`'\n'`) newlines.
//! When opening a file that uses CRLF (`"\r\n"`), the content is normalized on load, but the
//! preferred line ending can be tracked for saving.

/// The preferred newline sequence used when saving a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    /// Unix-style LF (`'\n'`).
    Lf,
    /// Windows-style CRLF (`"\r\n"`).
    Crlf,
}

impl LineEnding {
    /// Detect the dominant line ending from a source text.
    ///
    /// Policy: if the input contains any CRLF (`"\r\n"`), returns [`LineEnding::Crlf`],
    /// otherwise [`LineEnding::Lf`].
    pub fn detect_in_text(text: &str) -> Self {
        if text.contains("\r\n") {
            Self::Crlf
        } else {
            Self::Lf
        }
    }

    /// Convert an LF-normalized text to this line ending for saving.
    pub fn apply_to_text(self, text: &str) -> String {
        match self {
            Self::Lf => text.to_string(),
            Self::Crlf => text.replace('\n', "\r\n"),
        }
    }
}
