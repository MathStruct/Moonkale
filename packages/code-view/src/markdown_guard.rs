//! Untrusted Markdown must retain source when presentation parsing fails.
//!
//! NUL, vertical tab and form feed bypass pulldown-cmark before construction.
//! Version 0.13.4's empty tight-paragraph offset bug (#1129, P-212) also affects
//! panic=abort builds
//! (including the wasm client), where catch_unwind cannot recover. Do not
//! sanitize or replace bytes: offsets must always refer to the original text.
//! Catch the complete operation on unwind builds, discarding partial output.
//! Report rejected controls once per process to bound diagnostics.
//! Our diagnostics include neither source nor panic payload. The process's
//! existing panic hook remains installed (and runs before catch_unwind).
pub(crate) fn parse<T>(source: &str, operation: impl FnOnce() -> T) -> Option<T> {
    if source.contains(['\0', '\u{b}', '\u{c}']) {
        static REPORTED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if !REPORTED.swap(true, std::sync::atomic::Ordering::Relaxed) {
            eprintln!("Markdown presentation skipped: unsupported control character (P-212)");
        }
        return None;
    }
    #[cfg(panic = "unwind")]
    {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)) {
            Ok(value) => Some(value),
            Err(_) => {
                eprintln!("Markdown presentation skipped: parser panic (P-212)");
                None
            }
        }
    }
    #[cfg(not(panic = "unwind"))]
    Some(operation())
}

#[cfg(test)]
pub(crate) const CRASH_INPUTS: [&str; 4] = [
    include_str!("../../../fuzz/corpus/math_delimiters/p-212-crash.md"),
    include_str!("../../../fuzz/corpus/math_delimiters/p-212-minimal.md"),
    include_str!("../../../fuzz/corpus/math_delimiters/p-212-vertical-tab.md"),
    include_str!("../../../fuzz/corpus/math_delimiters/p-212-form-feed.md"),
];

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn unsafe_controls_never_enter_parser() {
        for source in ["- [\0]:[\n\u{b}", "- [a]:[\n\u{b}", "- [a]:[\n\u{c}"] {
            assert_eq!(parse(source, || panic!("must not parse")), None::<()>);
        }
    }

    #[cfg(feature = "markdown-preview")]
    /// The vendored pulldown-cmark carries upstream's fix (#1129), so the
    /// offset iterator itself no longer panics, without any guard: the fuzz
    /// finds and the printable-only reproductions from the upstream issue.
    #[test]
    fn patched_parser_survives_known_crash_inputs() {
        use pulldown_cmark::{Options, Parser};
        // Fuzz bytes, not all UTF-8; documents reach the parser as text.
        let tab = String::from_utf8_lossy(include_bytes!(
            "../../../fuzz/corpus/math_delimiters/p-212-tab.md"
        ));
        let issue = [
            "- [o]:u\n    \t",
            "> - [o]:u\n>     \t",
            "> - [o]:u\n> \u{c}",
            "+\t[\0]:d\n\u{b}",
            "- [a]:[\n\u{b}",
            "- [a]:[\n\u{c}",
        ];
        for source in super::CRASH_INPUTS.into_iter().chain([&*tab]).chain(issue) {
            for options in [Options::empty(), Options::all()] {
                let events = Parser::new_ext(source, options).into_offset_iter().count();
                assert!(events > 0, "{source:?}");
            }
        }
        // Upstream's expected events for the first case: a closed, empty item
        // (unpatched, the stream ended after `Start(Item)`).
        let tags: Vec<_> = Parser::new("- [o]:u\n    \t").collect();
        assert!(
            matches!(tags.last(), Some(pulldown_cmark::Event::End(_))),
            "{tags:?}"
        );
    }

    #[test]
    fn successful_parse_keeps_result() {
        assert_eq!(parse("safe", || 42), Some(42));
    }

    #[cfg(panic = "unwind")]
    #[test]
    fn panic_discards_partial_result() {
        assert_eq!(
            parse("safe", || { panic!("parser failed") }),
            None::<Vec<u8>>
        );
    }
}
