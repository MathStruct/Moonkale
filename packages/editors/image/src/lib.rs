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
    use super::*;
    use moonkale_core::{Node, NodeId, NodeKind, SourceId, Version};

    fn node(key: &str, kind: NodeKind) -> Node {
        let src = SourceId::new("folder:/x");
        Node {
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
