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

pub use moonkale_ext_api::media::image::{
    dimensions, is_image, mime_of, IMAGE_EXTENSIONS, MAX_BYTES, MAX_PIXELS,
};
