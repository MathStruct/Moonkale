//! # moonkale-editor-image
//!
//! One panel per opened image node (spec 008): the bytes come through
//! `Source::fetch_bytes` (folder natively, `RemoteSource` on the web) and
//! are shown as a `data:` URL in an `<img>` — the webview decodes; nothing
//! else is loaded. SVG goes through `<img>` too, so scripts inside it never
//! run. Zoom (fit / 100 % / ±, wheel), pan by dragging, size and pixel
//! dimensions in the toolbar. Images above [`MAX_BYTES`] are refused with a
//! message rather than turned into a 30 MB data URL.

pub mod extension;
pub mod panel;

/// This crate's strings (spec 030): English, German, Chinese.
pub(crate) static L: moonkale_ext_api::i18n::Locales = &[
    ("en", include_str!("../locales/en.ftl")),
    ("de", include_str!("../locales/de.ftl")),
    ("zh-CN", include_str!("../locales/zh-CN.ftl")),
];

pub use extension::ImageExtension;

/// Largest image the viewer will inline (data URL), in bytes.
pub const MAX_BYTES: u64 = 24 * 1024 * 1024;

/// Extensions the viewer claims (lower-case).
pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "svg", "bmp", "ico", "avif",
];

/// Is this node an image file the viewer should open?
pub fn is_image(node: &moonkale_core::Node) -> bool {
    node.kind == moonkale_core::NodeKind::File
        && node
            .native_key
            .rsplit('.')
            .next()
            .map(|e| IMAGE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false)
}

/// The most pixels the viewer decodes (#19): a small file can declare
/// 50000 × 50000 and freeze the tab while the browser decodes it.
pub const MAX_PIXELS: u64 = 100_000_000;

/// Width and height from the header of a PNG, GIF, JPEG, WebP or BMP, or
/// `None` for other formats (SVG is vector: drawn at the size shown).
pub fn dimensions(b: &[u8]) -> Option<(u32, u32)> {
    let be32 = |i: usize| {
        b.get(i..i + 4)
            .map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    };
    let le16 = |i: usize| {
        b.get(i..i + 2)
            .map(|s| u16::from_le_bytes([s[0], s[1]]) as u32)
    };
    let le24 = |i: usize| {
        b.get(i..i + 3)
            .map(|s| s[0] as u32 | (s[1] as u32) << 8 | (s[2] as u32) << 16)
    };
    let le32 = |i: usize| {
        b.get(i..i + 4)
            .map(|s| i32::from_le_bytes([s[0], s[1], s[2], s[3]]).unsigned_abs())
    };
    if b.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some((be32(16)?, be32(20)?));
    }
    if b.starts_with(b"GIF8") {
        return Some((le16(6)?, le16(8)?));
    }
    if b.starts_with(b"BM") {
        return Some((le32(18)?, le32(22)?));
    }
    if b.len() > 30 && &b[0..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        return match &b[12..16] {
            b"VP8X" => Some((le24(24)? + 1, le24(27)? + 1)),
            b"VP8L" => {
                let v = le32(21)?;
                Some(((v & 0x3fff) + 1, ((v >> 14) & 0x3fff) + 1))
            }
            b"VP8 " => Some((le16(26)? & 0x3fff, le16(28)? & 0x3fff)),
            _ => None,
        };
    }
    if b.starts_with(&[0xff, 0xd8]) {
        // Walk the segments to a start-of-frame marker.
        let mut i = 2;
        while i + 9 < b.len() {
            if b[i] != 0xff {
                return None;
            }
            let marker = b[i + 1];
            let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
            if matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
                let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
                let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
                return Some((w, h));
            }
            i += 2 + len;
        }
    }
    None
}

pub fn mime_of(key: &str) -> &'static str {
    match key
        .rsplit('.')
        .next()
        .map(|e| e.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("bmp") => "image/bmp",
        Some("ico") => "image/x-icon",
        Some("avif") => "image/avif",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn header_dimensions() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(50_000u32.to_be_bytes());
        png.extend(40_000u32.to_be_bytes());
        assert_eq!(super::dimensions(&png), Some((50_000, 40_000)));
        let gif = b"GIF89a\x10\x00\x20\x00rest".to_vec();
        assert_eq!(super::dimensions(&gif), Some((16, 32)));
        let jpeg = [
            0xff, 0xd8, 0xff, 0xe0, 0, 4, 0, 0, 0xff, 0xc0, 0, 11, 8, 0, 30, 0, 40, 3, 0, 0,
        ];
        assert_eq!(super::dimensions(&jpeg), Some((40, 30)));
        assert_eq!(super::dimensions(b"<svg/>"), None);
        const { assert!(50_000u64 * 40_000 > super::MAX_PIXELS) };
    }

    use super::*;
    use moonkale_core::{Node, NodeId, NodeKind, SourceId, Version};

    fn node(key: &str, kind: NodeKind) -> Node {
        let src = SourceId::new("folder:/x");
        Node {
            props: Default::default(),
            id: NodeId::derive(&src, key),
            source: src,
            kind,
            label: key.into(),
            native_key: key.into(),
            content: None,
            version: Version::default(),
        }
    }

    #[test]
    fn claims_images_by_extension() {
        assert!(is_image(&node("a/b.PNG", NodeKind::File)));
        assert!(is_image(&node("logo.svg", NodeKind::File)));
        assert!(!is_image(&node("notes.md", NodeKind::File)));
        assert!(!is_image(&node("pics", NodeKind::Directory)));
        assert_eq!(mime_of("x.jpeg"), "image/jpeg");
    }
}
