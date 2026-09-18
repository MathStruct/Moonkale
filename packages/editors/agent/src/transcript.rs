//! A conversation as a markdown page: readable, indexable, linkable.

use moonkale_llm::{AuditLog, Content, Message, Role};

pub fn render(
    title: &str,
    provider: &str,
    messages: &[Message],
    audit: &AuditLog,
    cited: &[String],
) -> String {
    let mut out = format!("# {title}\n\nprovider: {provider}\n\n");
    for m in messages {
        match m.role {
            Role::User => {
                let text = m.text();
                if !text.is_empty() {
                    out.push_str("## You\n\n");
                    out.push_str(&text);
                    out.push_str("\n\n");
                }
                for c in &m.content {
                    if let Content::ToolResult {
                        output, is_error, ..
                    } = c
                    {
                        let head: String = output.lines().take(12).collect::<Vec<_>>().join("\n");
                        out.push_str(if *is_error {
                            "> tool error\n"
                        } else {
                            "> tool result\n"
                        });
                        out.push_str("```\n");
                        out.push_str(&head);
                        if output.lines().count() > 12 {
                            out.push_str("\n…");
                        }
                        out.push_str("\n```\n\n");
                    }
                }
            }
            Role::Assistant => {
                out.push_str("## Agent\n\n");
                for c in &m.content {
                    match c {
                        Content::Text { text } => {
                            out.push_str(text);
                            out.push_str("\n\n");
                        }
                        Content::ToolUse { name, input, .. } => {
                            out.push_str(&format!("*calls* `{name}` `{input}`\n\n"));
                        }
                        Content::ToolResult { .. } => {}
                    }
                }
            }
        }
    }
    if !cited.is_empty() {
        out.push_str("## Cited\n\n");
        for c in cited {
            let link = c.strip_suffix(".md").unwrap_or(c);
            out.push_str(&format!("- [[{link}]]\n"));
        }
        out.push('\n');
    }
    if !audit.entries.is_empty() {
        out.push_str("## Tool calls\n\n| # | tool | class | decision | ok | ms |\n|---|---|---|---|---|---|\n");
        for e in &audit.entries {
            out.push_str(&format!(
                "| {} | {} | {:?} | {:?} | {} | {} |\n",
                e.seq, e.tool, e.class, e.decision, e.ok, e.millis
            ));
        }
    }
    out
}
