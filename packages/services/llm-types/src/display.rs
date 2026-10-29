//! Typed, bounded agent display data. No executable markup or arbitrary styles.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_DISPLAY_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpenViews {
    #[default]
    Ask,
    WhenIdle,
    Always,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Canvas {
    Markdown {
        text: String,
    },
    Table {
        columns: Vec<String>,
        rows: Vec<Vec<String>>,
    },
    Graph {
        nodes: Vec<GraphNode>,
        edges: Vec<GraphEdge>,
    },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub style: ThemeStyle,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeStyle {
    #[default]
    Default,
    Accent,
    Success,
    Warning,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
}

impl Canvas {
    pub fn validate(&self) -> Result<(), String> {
        if !within_bounds(self) {
            return Err("canvas exceeds 64 KiB".into());
        }
        let bounded = |s: &str| s.len() <= 4096 && !s.contains('\0');
        match self {
            Self::Markdown { text } => {
                validate_markdown(text)?;
            }
            Self::Table { columns, rows } => {
                if columns.is_empty()
                    || columns.len() > 32
                    || rows.len() > 200
                    || !columns.iter().all(|c| bounded(c))
                    || !rows
                        .iter()
                        .all(|r| r.len() == columns.len() && r.iter().all(|c| bounded(c)))
                {
                    return Err("invalid or oversized table".into());
                }
            }
            Self::Graph { nodes, edges } => {
                let ids: BTreeSet<_> = nodes.iter().map(|n| &n.id).collect();
                if nodes.is_empty()
                    || nodes.len() > 200
                    || edges.len() > 1000
                    || ids.len() != nodes.len()
                    || !nodes
                        .iter()
                        .all(|n| !n.id.is_empty() && n.id.len() <= 128 && bounded(&n.label))
                    || !edges
                        .iter()
                        .all(|e| ids.contains(&e.from) && ids.contains(&e.to))
                {
                    return Err("invalid or oversized graph".into());
                }
            }
        }
        Ok(())
    }
}

// The same parser/options as the safe message renderer. HTML inside a fenced
// code block is inert code; comparisons, blockquotes and math are valid Markdown.
fn validate_markdown(text: &str) -> Result<(), String> {
    use pulldown_cmark::{Event, Options, Parser};
    // P-212: these controls must never reach the math-enabled parser.
    if text.contains(['\0', '\u{b}', '\u{c}']) {
        return Err("unsupported Markdown control character".into());
    }
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_MATH;
    let mut depth = 0usize;
    for (count, event) in Parser::new_ext(text, options).enumerate() {
        if count >= 8192 {
            return Err("Markdown event limit (8192)".into());
        }
        match event {
            Event::Html(_) | Event::InlineHtml(_) => {
                return Err("HTML is not accepted in a canvas".into())
            }
            Event::Start(_) => {
                depth += 1;
                if depth >= 64 {
                    return Err("Markdown nesting limit (64)".into());
                }
            }
            Event::End(_) => depth = depth.saturating_sub(1),
            _ => {}
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reveal {
    pub source: String,
    pub node: String,
    #[serde(default)]
    pub line: u32,
    #[serde(default)]
    pub column: u32,
    #[serde(default)]
    pub end_line: Option<u32>,
    #[serde(default)]
    pub end_column: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    pub nodes: Vec<String>,
}

/// Validated actions shared by UI and server hosts.
#[derive(Clone, Debug, PartialEq)]
pub enum DisplayAction {
    Canvas(Canvas),
    Reveal(Reveal),
    Select(Selection),
}

/// Bound serialization before allocating or decoding another copy of an input.
pub fn within_bounds(value: &impl Serialize) -> bool {
    struct Limit(usize);
    impl std::io::Write for Limit {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            if bytes.len() > self.0 {
                return Err(std::io::Error::other("display input too large"));
            }
            self.0 -= bytes.len();
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Limit(MAX_DISPLAY_BYTES), value).is_ok()
}

pub fn parse_action(name: &str, input: serde_json::Value) -> Result<DisplayAction, String> {
    if !within_bounds(&input) {
        return Err("display input exceeds 64 KiB".into());
    }
    match name {
        "ui.reveal" | "editor.open" => {
            let mut range: Reveal = serde_json::from_value(input).map_err(|_| "invalid reveal")?;
            if name == "editor.open" {
                range.line = range.line.saturating_sub(1);
                range.end_line = range.end_line.map(|line| line.saturating_sub(1));
            }
            if range.source.len() > 1024
                || range.node.len() > 128
                || range.source.is_empty()
                || range.end_line.is_some_and(|end| end < range.line)
                || range.end_line == Some(range.line)
                    && range.end_column.is_some_and(|end| end < range.column)
            {
                return Err("invalid reveal range".into());
            }
            Ok(DisplayAction::Reveal(range))
        }
        "ui.select" => {
            let selection: Selection =
                serde_json::from_value(input).map_err(|_| "invalid selection")?;
            if selection.nodes.len() > 200 || selection.nodes.iter().any(|id| id.len() > 128) {
                return Err("selection exceeds bounds".into());
            }
            Ok(DisplayAction::Select(selection))
        }
        "ui.canvas" | "ui.show_result" => {
            #[derive(Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Content {
                content: Canvas,
            }
            let content: Content =
                serde_json::from_value(input).map_err(|_| "invalid typed canvas content")?;
            content.content.validate()?;
            if name == "ui.show_result" && matches!(content.content, Canvas::Markdown { .. }) {
                return Err("a result must be a table or graph".into());
            }
            Ok(DisplayAction::Canvas(content.content))
        }
        _ => Err("unknown display tool".into()),
    }
}

/// Per-conversation budget; charge attempts before validation/queueing. The
/// caller supplies monotonic milliseconds so this works on every platform.
#[derive(Clone, Debug, Default)]
pub struct DisplayBudget {
    calls: u32,
    last: Option<u64>,
}
impl DisplayBudget {
    pub fn charge(&mut self, now: u64) -> Result<(), String> {
        self.calls = self.calls.saturating_add(1);
        if self.calls > 100 {
            return Err("agent display budget exhausted (100 calls)".into());
        }
        if self
            .last
            .is_some_and(|last| now.saturating_sub(last) < 2000)
        {
            return Err("agent display rate limit (one call per two seconds)".into());
        }
        self.last = Some(now);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn markdown_code_math_and_quotes_are_data_but_raw_html_is_refused() {
        for text in [
            "```rust\nfn f<T>() -> Vec<T> { vec![] }\n```",
            "> A quote",
            "$x < y$",
            "`<script>inert code</script>`",
            "```html\n<script>inert code</script>\n```",
        ] {
            assert!(
                Canvas::Markdown { text: text.into() }.validate().is_ok(),
                "{text}"
            );
        }
        for text in [
            "<script>alert(1)</script>",
            "> <img src='https://example.invalid/x'>",
            "a\0b",
            "a\u{b}b",
            "a\u{c}b",
        ] {
            assert!(Canvas::Markdown { text: text.into() }.validate().is_err());
        }
        assert!(Canvas::Markdown {
            text: format!("{}deep", "> ".repeat(1000))
        }
        .validate()
        .is_err());
    }

    #[test]
    fn compatibility_open_keeps_one_based_lines_and_reveal_keeps_zero_based_ranges() {
        let input = serde_json::json!({"source":"folder", "node":"node", "line":7});
        let DisplayAction::Reveal(legacy) = parse_action("editor.open", input.clone()).unwrap()
        else {
            panic!("wrong action")
        };
        let DisplayAction::Reveal(reveal) = parse_action("ui.reveal", input).unwrap() else {
            panic!("wrong action")
        };
        assert_eq!(legacy.line, 6);
        assert_eq!(reveal.line, 7);
    }

    #[test]
    fn canvas_accepts_only_bounded_typed_content() {
        for json in [
            r#"{"kind":"html","text":"x"}"#,
            r#"{"kind":"markdown","text":"ok","css":"body{}"}"#,
            r#"{"kind":"graph","nodes":[{"id":"a","label":"a","style":"red"}],"edges":[]}"#,
        ] {
            assert!(serde_json::from_str::<Canvas>(json).is_err());
        }
        assert!(Canvas::Markdown {
            text: "<script>alert(1)</script>".into()
        }
        .validate()
        .is_err());
        assert!(Canvas::Markdown {
            text: "x".repeat(MAX_DISPLAY_BYTES)
        }
        .validate()
        .is_err());
        assert!(Canvas::Table {
            columns: vec!["a".into()],
            rows: vec![vec![]]
        }
        .validate()
        .is_err());
        assert!(Canvas::Markdown {
            text: "# A safe **result**".into()
        }
        .validate()
        .is_ok());
    }
    #[test]
    fn rate_limit_and_budget_are_independent_of_wall_clock() {
        let mut budget = DisplayBudget::default();
        assert!(budget.charge(1000).is_ok());
        assert!(budget.charge(2999).is_err());
        assert!(budget.charge(3000).is_ok());
        assert!(budget.charge(0).is_err());
        for i in 0..96 {
            assert!(budget.charge(5000 + i * 2000).is_ok());
        }
        assert!(budget.charge(999999).unwrap_err().contains("budget"));
    }
}
