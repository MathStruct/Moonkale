use std::borrow::Cow;

pub(crate) fn normalize_crlf_to_lf(text: &str) -> Cow<'_, str> {
    if text.contains('\r') {
        // Normalize:
        // - CRLF (`\r\n`) -> LF (`\n`)
        // - Lone CR (`\r`) -> LF (`\n`)
        //
        // This keeps the internal model consistent with line indexing/layout (which are `\n`-based).
        let tmp = text.replace("\r\n", "\n");
        Cow::Owned(tmp.replace('\r', "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

pub(crate) fn normalize_crlf_to_lf_string(text: String) -> String {
    if text.contains('\r') {
        let tmp = text.replace("\r\n", "\n");
        tmp.replace('\r', "\n")
    } else {
        text
    }
}

pub(crate) fn split_lines_preserve_trailing(text: &str) -> Vec<String> {
    // `str::split('\n')` preserves trailing empty segments, which matches typical editor
    // line semantics (N newlines => N+1 lines), and keeps behavior consistent with Rope.
    text.split('\n').map(|line| line.to_string()).collect()
}
