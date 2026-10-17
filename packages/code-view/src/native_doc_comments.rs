//! Syntax-recognized documentation; presentation never evaluates or rewrites code.
use crate::native_decorations::{
    Batch, CodeFragment, Decoration, InlineFragment, InlineStyle, Kind, Provider,
};
use arborium_tree_sitter::{InputEdit, Node, Parser, Point, Tree};
use moonkale_ext_api::editor::DocumentRevision;
use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser as Markdown, Tag, TagEnd,
};
use std::{collections::HashMap, ops::Range, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Language {
    Rust,
    Python,
    Julia,
}
impl Language {
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "rust" => Some(Self::Rust),
            "python" => Some(Self::Python),
            "julia" => Some(Self::Julia),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Block {
    Text {
        heading: u8,
        quote: usize,
        marker: Option<(usize, String)>,
        fragments: Range<usize>,
    },
    Code {
        language: String,
        text: String,
        cached: Rc<std::cell::RefCell<Option<Vec<CodeFragment>>>>,
    },
    Table {
        alignments: Vec<String>,
        rows: Vec<Vec<Range<usize>>>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Document {
    pub blocks: Vec<Block>,
    pub fragments: Vec<InlineFragment>,
    pub label: String,
}

#[derive(Default)]
pub(crate) struct Index {
    language: Option<Language>,
    parser: Option<Parser>,
    tree: Option<Tree>,
    source: String,
    line_starts: Vec<usize>,
    dirty: bool,
    pending: Option<DocumentRevision>,
    unchanged_documentation: bool,
    documentation_end: usize,
    values: Vec<Decoration>,
    rows: Vec<(Range<usize>, Range<usize>)>,
    revision: Option<DocumentRevision>,
    validated: Option<Rc<Vec<crate::native_presentation::Replacement>>>,
    bodies: HashMap<String, Rc<Document>>,
    #[cfg(test)]
    pub parses: usize,
    #[cfg(test)]
    pub rendered: usize,
}
impl Index {
    pub fn rebuild(&mut self) {
        self.tree = None;
        self.values.clear();
        self.rows.clear();
        self.source.clear();
        self.line_starts.clear();
        self.revision = None;
        self.validated = None;
        self.dirty = false;
        self.pending = None;
        self.unchanged_documentation = false;
        self.documentation_end = 0;
    }
    pub fn apply_delta(
        &mut self,
        revision: DocumentRevision,
        target: DocumentRevision,
        range: Range<usize>,
        inserted: &str,
    ) {
        if self.revision.or(self.pending) != Some(revision)
            || range.end > self.source.len()
            || range.start > range.end
            || !self.source.is_char_boundary(range.start)
            || !self.source.is_char_boundary(range.end)
        {
            self.rebuild();
            return;
        }
        let position = |byte| {
            let row = self
                .line_starts
                .partition_point(|start| *start <= byte)
                .saturating_sub(1);
            Point {
                row,
                column: byte - self.line_starts.get(row).copied().unwrap_or(0),
            }
        };
        let start = position(range.start);
        let old_end = if range.end == range.start {
            start
        } else {
            position(range.end)
        };
        let lines = inserted.split('\n').count() - 1;
        let new_end = Point {
            row: start.row + lines,
            column: if lines == 0 {
                start.column + inserted.len()
            } else {
                inserted.len() - inserted.rfind('\n').unwrap() - 1
            },
        };
        if let Some(tree) = &mut self.tree {
            tree.edit(&InputEdit {
                start_byte: range.start,
                old_end_byte: range.end,
                new_end_byte: range.start + inserted.len(),
                start_position: start,
                old_end_position: old_end,
                new_end_position: new_end,
            });
        }
        // Rust doc-comment ownership depends on lexical tokens, not the
        // declaration following them. An edit strictly after the final owner
        // (including its newline), with no doc tokens in the entire tail, cannot
        // alter an existing owner or introduce another. Still update/parse the
        // syntax tree, but retain the source mappings and rendered payloads.
        let unchanged = self.language == Some(Language::Rust)
            && self.tree.is_some()
            && range.start > self.documentation_end
            && (!self.dirty || self.unchanged_documentation)
            && !rust_doc_token(&self.source[self.documentation_end..]);
        let first = self
            .line_starts
            .partition_point(|start| *start <= range.start);
        let after = self
            .line_starts
            .partition_point(|start| *start <= range.end);
        for start in &mut self.line_starts[after..] {
            *start = *start - (range.end - range.start) + inserted.len();
        }
        self.line_starts.splice(
            first..after,
            inserted
                .match_indices('\n')
                .map(|(byte, _)| range.start + byte + 1),
        );
        self.source.replace_range(range, inserted);
        self.unchanged_documentation =
            unchanged && !rust_doc_token(&self.source[self.documentation_end..]);
        self.dirty = true;
        self.pending = Some(target);
        self.revision = None;
        if !self.unchanged_documentation {
            self.validated = None;
        }
    }
    pub fn updated_source(&self, revision: DocumentRevision) -> Option<String> {
        (self.pending == Some(revision)).then(|| self.source.clone())
    }
    pub fn cached(
        &self,
        revision: DocumentRevision,
    ) -> Option<Rc<Vec<crate::native_presentation::Replacement>>> {
        (self.revision == Some(revision))
            .then(|| self.validated.clone())
            .flatten()
    }
    pub fn replacements(
        &mut self,
        language: Language,
        revision: DocumentRevision,
        source: &str,
    ) -> Rc<Vec<crate::native_presentation::Replacement>> {
        self.batch(language, revision, source);
        if let Some(values) = &self.validated {
            return values.clone();
        }
        let values = self
            .values
            .iter()
            .filter_map(|value| {
                if let Kind::Replace {
                    range,
                    widget,
                    style,
                } = &value.kind
                {
                    Some(crate::native_presentation::Replacement {
                        start: range.start,
                        end: range.end,
                        widget: widget.clone(),
                        style: style.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();
        let values = Rc::new(crate::native_presentation::valid_replacements(
            source, values,
        ));
        self.validated = Some(values.clone());
        values
    }
    pub fn unwrapped_owner_row(&self, start: usize) -> Option<usize> {
        self.rows
            .get(
                self.rows
                    .partition_point(|(source, _)| source.start < start),
            )
            .filter(|(source, _)| source.start == start)
            .map(|(_, rows)| rows.start - 1)
    }
    pub fn unwrapped_rows(&self, state: crate::native_layout::PresentationState) -> Vec<usize> {
        self.rows
            .iter()
            .filter(|(source, _)| state.previews(source.start, source.end))
            .flat_map(|(_, rows)| rows.clone())
            .collect()
    }
    pub fn batch(&mut self, language: Language, revision: DocumentRevision, source: &str) -> Batch {
        if self.language != Some(language) {
            *self = Self::default();
            self.language = Some(language);
            let grammar = match language {
                Language::Rust => arborium_rust::language(),
                Language::Python => arborium_python::language(),
                Language::Julia => arborium_julia::language(),
            };
            let mut parser = Parser::new();
            if parser.set_language(&grammar.into()).is_ok() {
                self.parser = Some(parser);
            }
        }
        if self.tree.is_none() || self.source != source || self.dirty {
            if self.source != source || self.line_starts.is_empty() {
                if let Some(tree) = &mut self.tree {
                    tree.edit(&difference(&self.source, source));
                }
                self.line_starts = std::iter::once(0)
                    .chain(source.match_indices('\n').map(|(byte, _)| byte + 1))
                    .collect();
            }
            self.tree = self
                .parser
                .as_mut()
                .and_then(|parser| parser.parse(source, self.tree.as_ref()));
            #[cfg(test)]
            {
                self.parses += 1;
                self.rendered = 0;
            }
            if self.unchanged_documentation && self.source == source {
                self.dirty = false;
                self.unchanged_documentation = false;
                self.pending = None;
                self.revision = Some(revision);
                return Batch {
                    revision,
                    values: self.values.clone(),
                };
            }
            let candidates = self
                .tree
                .as_ref()
                .map(|tree| candidates(tree.root_node(), source, language))
                .unwrap_or_default();
            self.documentation_end = candidates.last().map_or(0, |(range, _)| range.end);
            self.unchanged_documentation = false;
            let mut bodies = HashMap::new();
            let mut values = Vec::new();
            let (mut byte, mut scalar, mut line) = (0, 0, 0);
            let mut rows = Vec::new();
            for (range, body) in candidates {
                let first_line = line + source[byte..range.start].match_indices('\n').count();
                line = first_line + source[range.clone()].match_indices('\n').count();
                scalar += source[byte..range.start].chars().count();
                let start = scalar;
                scalar += source[range.clone()].chars().count();
                byte = range.end;
                if body.trim().is_empty() {
                    continue;
                }
                rows.push((start..scalar, first_line + 1..line + 1));
                let document = self
                    .bodies
                    .get(&body)
                    .or_else(|| bodies.get(&body))
                    .cloned()
                    .unwrap_or_else(|| {
                        #[cfg(test)]
                        {
                            self.rendered += 1;
                        }
                        Rc::new(render(&body))
                    });
                values.push(Decoration {
                    provider: Provider::Documentation,
                    id: start,
                    kind: Kind::Replace {
                        range: start..scalar,
                        widget: Some(document.label.clone()),
                        style: Some(InlineStyle::Documentation(document.clone())),
                    },
                });
                bodies.insert(body, document);
            }
            self.validated = None;
            self.values = values;
            self.rows = rows;
            self.bodies = bodies;
            if self.source != source {
                self.source = source.to_string();
            }
            self.dirty = false;
        }
        self.pending = None;
        self.revision = Some(revision);
        Batch {
            revision,
            values: self.values.clone(),
        }
    }
}

fn point(text: &str, byte: usize) -> Point {
    Point {
        row: text[..byte].split('\n').count() - 1,
        column: byte - text[..byte].rfind('\n').map_or(0, |byte| byte + 1),
    }
}
fn difference(old: &str, new: &str) -> InputEdit {
    let mut start = old
        .bytes()
        .zip(new.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(start) || !new.is_char_boundary(start) {
        start -= 1;
    }
    let mut suffix = old[start..]
        .bytes()
        .rev()
        .zip(new[start..].bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    while !old.is_char_boundary(old.len() - suffix) || !new.is_char_boundary(new.len() - suffix) {
        suffix -= 1;
    }
    InputEdit {
        start_byte: start,
        old_end_byte: old.len() - suffix,
        new_end_byte: new.len() - suffix,
        start_position: point(old, start),
        old_end_position: point(old, old.len() - suffix),
        new_end_position: point(new, new.len() - suffix),
    }
}
fn children(node: Node<'_>) -> Vec<Node<'_>> {
    // Indexed child lookup repeatedly walks preceding siblings in tree-sitter.
    // A cursor visits wide documentation-heavy trees in linear time.
    let mut result = Vec::new();
    let mut cursor = node.walk();
    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() {
                result.push(child);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }
    result
}
fn has(node: Node<'_>, kinds: &[&str]) -> bool {
    kinds.contains(&node.kind()) || children(node).into_iter().any(|child| has(child, kinds))
}
fn standalone(source: &str, start: usize, end: usize) -> Option<Range<usize>> {
    let end = source[..end].trim_end_matches(['\n', '\r']).len();
    let line = source[..start].rfind('\n').map_or(0, |byte| byte + 1);
    let tail = source[end..]
        .find('\n')
        .map_or(source.len(), |byte| end + byte);
    (source[line..start]
        .chars()
        .all(|ch| matches!(ch, ' ' | '\t'))
        && source[end..tail].trim().is_empty())
    .then_some(line..tail)
}
fn literal(node: Node<'_>, source: &str, language: Language) -> Option<String> {
    if node.has_error()
        || node.is_missing()
        || has(
            node,
            &["interpolation", "string_interpolation", "escape_sequence"],
        )
    {
        return None;
    }
    let text = source.get(node.byte_range())?;
    let quote = text.find(['\'', '"'])?;
    let prefix = &text[..quote];
    if match language {
        Language::Python => !matches!(prefix.to_ascii_lowercase().as_str(), "" | "r" | "u"),
        Language::Julia => !matches!(prefix, "" | "raw"),
        Language::Rust => true,
    } {
        return None;
    }
    let marker = &text[quote..quote + 1];
    let delimiter = if text[quote..].starts_with(&marker.repeat(3)) {
        marker.repeat(3)
    } else {
        marker.into()
    };
    let inner = text[quote..]
        .strip_prefix(&delimiter)?
        .strip_suffix(&delimiter)?;
    Some(dedent(inner))
}
fn dedent(text: &str) -> String {
    let lines: Vec<_> = text.lines().collect();
    let margin = lines
        .iter()
        .skip(1)
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            line.chars()
                .take_while(|ch| matches!(ch, ' ' | '\t'))
                .count()
        })
        .min()
        .unwrap_or(0);
    let mut result = lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            if i == 0 {
                line.trim_start().to_string()
            } else {
                line.chars().skip(margin).collect()
            }
        })
        .collect::<Vec<String>>()
        .join("\n");
    result = result.trim_matches('\n').to_string();
    result
}
fn rust_doc_token(text: &str) -> bool {
    ["///", "//!", "/**", "/*!"]
        .iter()
        .any(|marker| text.contains(marker))
}
fn candidates(root: Node<'_>, source: &str, language: Language) -> Vec<(Range<usize>, String)> {
    let mut result = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let kind = node.kind();
        // The grammar still decides ownership; this lexical prefilter only
        // skips subtrees that cannot contain any Rust documentation token.
        if language == Language::Rust {
            let text = &source[node.byte_range()];
            if !["///", "//!", "/**", "/*!"]
                .iter()
                .any(|marker| text.contains(marker))
            {
                continue;
            }
        }
        if !node.has_error() && !node.is_missing() {
            match language {
                Language::Rust
                    if matches!(
                        kind,
                        "line_comment"
                            | "block_comment"
                            | "line_outer_doc_comment"
                            | "block_outer_doc_comment"
                    ) && node.child_by_field_name("doc").is_some() =>
                {
                    let raw = &source[node.byte_range()];
                    if let Some(range) = standalone(source, node.start_byte(), node.end_byte()) {
                        let body = if raw.starts_with("///") || raw.starts_with("//!") {
                            raw[3..]
                                .trim_end_matches(['\r', '\n'])
                                .strip_prefix(' ')
                                .unwrap_or(raw[3..].trim_end_matches(['\r', '\n']))
                                .to_string()
                        } else if raw.starts_with("/**") || raw.starts_with("/*!") {
                            let inner = raw[3..].strip_suffix("*/").unwrap_or("");
                            let star_margin = inner.contains('\n')
                                && inner
                                    .lines()
                                    .skip(1)
                                    .filter(|line| !line.trim().is_empty())
                                    .all(|line| {
                                        let text = line.trim_start();
                                        text == "*"
                                            || text.starts_with("* ")
                                            || text.starts_with("*\t")
                                    });
                            let lines = inner
                                .lines()
                                .enumerate()
                                .map(|(index, line)| {
                                    let trimmed = line.trim_start();
                                    if let Some(rest) = (star_margin && index > 0)
                                        .then(|| trimmed.strip_prefix('*'))
                                        .flatten()
                                    {
                                        rest.strip_prefix(' ').unwrap_or(rest)
                                    } else {
                                        line
                                    }
                                })
                                .collect::<Vec<_>>()
                                .join("\n");
                            dedent(&lines)
                        } else {
                            continue;
                        };
                        result.push((range, body));
                    }
                }
                Language::Python if kind == "string" => {
                    let statement = node
                        .parent()
                        .filter(|parent| {
                            parent.kind() == "expression_statement"
                                && parent.named_child_count() == 1
                        })
                        .unwrap_or(node);
                    {
                        if let Some(parent) = statement.parent() {
                            let first = children(parent)
                                .into_iter()
                                .find(|child| child.kind() != "comment");
                            let scope = parent.kind() == "module"
                                || parent.kind() == "block"
                                    && parent.parent().is_some_and(|owner| {
                                        matches!(
                                            owner.kind(),
                                            "function_definition" | "class_definition"
                                        )
                                    });
                            if scope && first.is_some_and(|first| first.id() == statement.id()) {
                                if let (Some(range), Some(body)) = (
                                    standalone(source, node.start_byte(), node.end_byte()),
                                    literal(node, source, language),
                                ) {
                                    result.push((range, body));
                                }
                            }
                        }
                    }
                }
                Language::Julia if matches!(kind, "string_literal" | "prefixed_string_literal") => {
                    let next = node.next_named_sibling();
                    let implicit = next.is_some_and(|next| {
                        let gap = &source[node.end_byte()..next.start_byte()];
                        gap.chars().all(char::is_whitespace)
                            && gap.bytes().filter(|byte| *byte == b'\n').count() == 1
                            && matches!(
                                next.kind(),
                                "assignment"
                                    | "function_definition"
                                    | "macro_definition"
                                    | "module_definition"
                                    | "struct_definition"
                                    | "abstract_definition"
                                    | "primitive_definition"
                                    | "const_statement"
                            )
                    }) && node.parent().is_some_and(|parent| {
                        matches!(
                            parent.kind(),
                            "source_file"
                                | "module_definition"
                                | "function_definition"
                                | "compound_statement"
                        )
                    });
                    let explicit = node
                        .parent()
                        .filter(|parent| {
                            parent.kind() == "macro_argument_list"
                                && parent
                                    .named_child(0)
                                    .is_some_and(|first| first.id() == node.id())
                        })
                        .and_then(|parent| parent.parent())
                        .filter(|parent| parent.kind() == "macrocall_expression")
                        .filter(|parent| {
                            node.parent()
                                .is_some_and(|arguments| arguments.named_child_count() >= 2)
                                || parent.next_named_sibling().is_some_and(|target| {
                                    let gap = &source[parent.end_byte()..target.start_byte()];
                                    !target.has_error()
                                        && gap.chars().all(char::is_whitespace)
                                        && gap.bytes().filter(|byte| *byte == b'\n').count() == 1
                                })
                        })
                        .filter(|parent| {
                            source[parent.start_byte()..node.start_byte()].trim() == "@doc"
                        });
                    let start = explicit.map_or(node.start_byte(), |parent| parent.start_byte());
                    if implicit || explicit.is_some() {
                        if let (Some(range), Some(body)) = (
                            standalone(source, start, node.end_byte()),
                            literal(node, source, language),
                        ) {
                            result.push((range, body));
                        }
                    }
                }
                _ => {}
            }
        }
        if node.named_child_count() > 0
            && !(language == Language::Rust && matches!(kind, "line_comment" | "block_comment"))
        {
            stack.extend(children(node).into_iter().rev());
        }
    }
    result.sort_by_key(|(range, _)| range.start);
    if language == Language::Rust {
        let mut grouped: Vec<(Range<usize>, String)> = Vec::new();
        for (range, body) in result {
            if let Some((previous, text)) = grouped.last_mut() {
                let prior = source[previous.clone()].trim_start();
                let current = source[range.clone()].trim_start();
                if ((prior.starts_with("///") && current.starts_with("///"))
                    || (prior.starts_with("//!") && current.starts_with("//!")))
                    && source[previous.end..range.start].replace('\r', "") == "\n"
                {
                    previous.end = range.end;
                    text.push('\n');
                    text.push_str(&body);
                    continue;
                }
            }
            grouped.push((range, body));
        }
        grouped
    } else {
        result
    }
}

fn fragment(
    text: String,
    emphasis: bool,
    strong: bool,
    code: bool,
    link: Option<crate::native_link_parser::Link>,
) -> InlineFragment {
    InlineFragment {
        text,
        emphasis,
        strong,
        code,
        link,
        math: None,
    }
}
fn heading(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}
fn render(source: &str) -> Document {
    crate::markdown_guard::parse(source, || render_parsed(source)).unwrap_or_else(|| Document {
        blocks: vec![Block::Text {
            heading: 0,
            quote: 0,
            marker: None,
            fragments: 0..1,
        }],
        fragments: vec![InlineFragment {
            text: source.into(),
            math: None,
            emphasis: false,
            strong: false,
            code: false,
            link: None,
        }],
        label: source.into(),
    })
}

fn render_parsed(source: &str) -> Document {
    let mut blocks = Vec::new();
    let mut fragments = Vec::new();
    let (mut emphasis, mut strong, mut quote) = (0usize, 0usize, 0usize);
    let mut link = None;
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut marker = None;
    let (mut start, mut level) = (0usize, 0u8);
    let mut active = false;
    let mut code: Option<(String, String)> = None;
    type TableRows = (Vec<String>, Vec<Vec<Range<usize>>>);
    let mut table: Option<TableRows> = None;
    let mut cell = 0;
    let mut image = 0usize;
    let mut literal_link = false;
    for (event, range) in
        Markdown::new_ext(source, Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS)
            .into_offset_iter()
    {
        if literal_link {
            if matches!(event, Event::End(TagEnd::Link)) {
                literal_link = false;
            }
            continue;
        }
        if image > 0 {
            match event {
                Event::Start(Tag::Image { .. }) => image += 1,
                Event::End(TagEnd::Image) => image -= 1,
                _ => {}
            }
            continue;
        }
        match event {
            Event::Start(Tag::Image { .. }) => {
                fragments.push(fragment(
                    source[range].into(),
                    emphasis > 0,
                    strong > 0,
                    false,
                    None,
                ));
                image = 1;
            }
            Event::Start(Tag::Paragraph) => {
                start = fragments.len();
                level = 0;
                active = true;
            }
            Event::Start(Tag::Heading { level: value, .. }) => {
                start = fragments.len();
                level = heading(value);
                active = true;
            }
            Event::End(TagEnd::Paragraph | TagEnd::Heading(_)) => {
                blocks.push(Block::Text {
                    heading: level,
                    quote,
                    marker: marker.clone(),
                    fragments: start..fragments.len(),
                });
                active = false;
            }
            Event::Start(Tag::BlockQuote(_)) => quote += 1,
            Event::End(TagEnd::BlockQuote(_)) => quote = quote.saturating_sub(1),
            Event::Start(Tag::List(number)) => {
                if active && start < fragments.len() {
                    blocks.push(Block::Text {
                        heading: level,
                        quote,
                        marker: marker.clone(),
                        fragments: start..fragments.len(),
                    });
                    active = false;
                }
                lists.push(number);
            }
            Event::End(TagEnd::List(_)) => {
                lists.pop();
                marker = None;
            }
            Event::Start(Tag::Item) => {
                marker = Some((
                    lists.len(),
                    lists
                        .last()
                        .copied()
                        .flatten()
                        .map_or_else(|| "•".into(), |n| format!("{n}.")),
                ));
                start = fragments.len();
                level = 0;
                active = true;
            }
            Event::End(TagEnd::Item) => {
                if active && start < fragments.len() {
                    blocks.push(Block::Text {
                        heading: 0,
                        quote,
                        marker: marker.clone(),
                        fragments: start..fragments.len(),
                    });
                    active = false;
                }
                if let Some(Some(n)) = lists.last_mut() {
                    *n += 1;
                }
            }
            Event::Start(Tag::Emphasis) => emphasis += 1,
            Event::End(TagEnd::Emphasis) => emphasis = emphasis.saturating_sub(1),
            Event::Start(Tag::Strong) => strong += 1,
            Event::End(TagEnd::Strong) => strong = strong.saturating_sub(1),
            Event::Start(Tag::Link {
                dest_url, title, ..
            }) => {
                link = crate::native_links::Target::markdown(&dest_url)
                    .filter(|target| match target {
                        crate::native_links::Target::External(_) => true,
                        crate::native_links::Target::File { path, .. } => {
                            path.contains('/') || path.contains('.')
                        }
                        _ => false,
                    })
                    .map(|target| crate::native_link_parser::Link {
                        range: 0..0,
                        target,
                        title: title.to_string(),
                    });
                if link.is_none() {
                    fragments.push(fragment(
                        source[range].into(),
                        emphasis > 0,
                        strong > 0,
                        false,
                        None,
                    ));
                    literal_link = true;
                }
            }
            Event::End(TagEnd::Link) => link = None,
            Event::Start(Tag::CodeBlock(kind)) => {
                code = Some((
                    match kind {
                        CodeBlockKind::Fenced(info) => info.split(',').next().unwrap_or("").into(),
                        _ => String::new(),
                    },
                    String::new(),
                ));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((info, text)) = code.take() {
                    let hint = info.split_whitespace().next().unwrap_or("");
                    let language = match hint {
                        "rs" => "rust",
                        "py" => "python",
                        "js" => "javascript",
                        "ts" => "typescript",
                        "sh" => "bash",
                        other => other,
                    }
                    .to_string();
                    blocks.push(Block::Code {
                        language,
                        text,
                        cached: Rc::new(Default::default()),
                    });
                }
            }
            Event::Start(Tag::Table(alignments)) => {
                table = Some((
                    alignments
                        .into_iter()
                        .map(|alignment| {
                            match alignment {
                                Alignment::Left => "left",
                                Alignment::Center => "center",
                                Alignment::Right => "right",
                                Alignment::None => "left",
                            }
                            .to_string()
                        })
                        .collect(),
                    Vec::new(),
                ))
            }
            Event::Start(Tag::TableHead | Tag::TableRow) => {
                if let Some((_, rows)) = &mut table {
                    rows.push(Vec::new());
                }
            }
            Event::Start(Tag::TableCell) => cell = fragments.len(),
            Event::End(TagEnd::TableCell) => {
                if let Some((_, rows)) = &mut table {
                    if let Some(row) = rows.last_mut() {
                        row.push(cell..fragments.len());
                    }
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some((alignments, rows)) = table.take() {
                    blocks.push(Block::Table { alignments, rows });
                }
            }
            Event::Text(text) => {
                if let Some((_, body)) = &mut code {
                    body.push_str(&text);
                } else {
                    fragments.push(fragment(
                        text.to_string(),
                        emphasis > 0,
                        strong > 0,
                        false,
                        link.clone(),
                    ));
                }
            }
            Event::Code(text) => fragments.push(fragment(
                text.to_string(),
                emphasis > 0,
                strong > 0,
                true,
                link.clone(),
            )),
            Event::SoftBreak | Event::HardBreak => fragments.push(fragment(
                "\n".into(),
                emphasis > 0,
                strong > 0,
                false,
                link.clone(),
            )),
            Event::TaskListMarker(checked) => {
                if let Some((_, label)) = &mut marker {
                    *label = if checked { "☑" } else { "☐" }.into();
                }
            }
            Event::Html(text) | Event::InlineHtml(text) => {
                let first = fragments.len();
                fragments.push(fragment(text.to_string(), false, false, false, None));
                if !active && table.is_none() {
                    blocks.push(Block::Text {
                        heading: 0,
                        quote,
                        marker: None,
                        fragments: first..fragments.len(),
                    });
                }
            }
            Event::Rule => {
                let first = fragments.len();
                fragments.push(fragment(source[range].into(), false, false, false, None));
                blocks.push(Block::Text {
                    heading: 0,
                    quote,
                    marker: None,
                    fragments: first..fragments.len(),
                });
            }
            _ => {}
        }
    }
    let mut label = String::new();
    // Label follows DOM order, including code blocks interleaved with prose.
    label.clear();
    for block in &blocks {
        match block {
            Block::Text {
                fragments: range, ..
            } => {
                for f in &fragments[range.clone()] {
                    label.push_str(&f.text);
                }
            }
            Block::Table { rows, .. } => {
                for row in rows {
                    for cell in row {
                        for f in &fragments[cell.clone()] {
                            label.push_str(&f.text);
                        }
                    }
                }
            }
            Block::Code { text, .. } => label.push_str(text),
        }
    }
    Document {
        blocks,
        fragments,
        label,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn docs(language: Language, source: &str) -> Vec<(String, Rc<Document>)> {
        let mut index = Index::default();
        let chars: Vec<_> = source.chars().collect();
        index
            .batch(language, DocumentRevision(1), source)
            .values
            .into_iter()
            .filter_map(|value| {
                if let Kind::Replace {
                    range,
                    style: Some(InlineStyle::Documentation(doc)),
                    ..
                } = value.kind
                {
                    Some((chars[range].iter().collect(), doc))
                } else {
                    None
                }
            })
            .collect()
    }
    #[test]
    fn rust_groups_only_real_standalone_documentation() {
        let source="//! Module **docs**\n//! Second line\n\n/// # Heading 猫\n///\n/// - first\n/// - second\nfn f() {}\n// ordinary\n//// ordinary too\nlet s = \"/// string\";\n/** Block docs */\nfn g() {}\nfn h(){ /** inline */ }\n";
        let values = docs(Language::Rust, source);
        assert_eq!(values.len(), 3, "{values:?}");
        assert!(values[0].0.starts_with("//!"));
        assert!(values[1].1.label.contains("Heading 猫"));
        assert!(values[2].1.label.contains("Block docs"));
    }
    #[test]
    fn block_comment_margins_do_not_consume_markdown_emphasis() {
        let values=docs(Language::Rust,"/** *Inline* docs */\nfn a() {}\n/**\n * *Decorated* docs\n */\nfn b() {}\n/**\n *Mixed* docs\n plain continuation\n*/\nfn c() {}\n");
        assert_eq!(values.len(), 3);
        for (_, doc) in values {
            assert!(doc.fragments.iter().any(|fragment| fragment.emphasis));
        }
        assert!(docs(Language::Julia, "@doc \"Orphan\"\n").is_empty());
    }

    #[test]
    fn python_docstrings_are_first_literal_statements() {
        let source="\"\"\"Module **docs**\"\"\"\nvalue = \"assigned\"\nclass C:\n    \"\"\"Class docs\"\"\"\n    def f(self):\n        r\"\"\"Function `docs`.\"\"\"\n        return 'not docs'\nasync def a():\n    \"async docs\"\n    pass\ndef b():\n    x=1\n    \"not first\"\ndef c():\n    f\"dynamic {x}\"\ndef d():\n    \"escape\\ntext\"\n";
        let values = docs(Language::Python, source);
        assert_eq!(values.len(), 4, "{values:?}");
    }
    #[test]
    fn julia_requires_immediate_attachment_and_literal_content() {
        let source="\"\"\"# Function docs\"\"\"\nf(x) = x\n\"Detached\"\n\ng(x) = x\n\"Not docs\"\n# intervening\nh(x) = x\n\"dynamic $x\"\nk(x)=x\nraw\"\"\"Type **docs**\"\"\"\nstruct T\nend\n@doc \"Explicit\"\nf\n";
        let values = docs(Language::Julia, source);

        assert_eq!(values.len(), 3, "{values:?}");
        assert!(values.iter().all(|(_, doc)| !doc.label.contains("Detached")
            && !doc.label.contains("Not docs")
            && !doc.label.contains("dynamic")));
    }
    #[test]
    fn incremental_index_reuses_bodies_and_unicode_offsets() {
        let mut index = Index::default();
        let source = "/// First **猫**\nfn a() {}\n/// Second\nfn b() {}\n";
        let initial = index.batch(Language::Rust, DocumentRevision(1), source);
        assert_eq!(index.parses, 1);
        let again = index.batch(Language::Rust, DocumentRevision(2), source);
        assert!(again.values == initial.values);
        assert_eq!(index.parses, 1);
        index.apply_delta(DocumentRevision(2), DocumentRevision(3), 4..9, "😀 Updated");
        assert!(index.cached(DocumentRevision(2)).is_none());
        let next = index.batch(
            Language::Rust,
            DocumentRevision(3),
            &source.replace("First", "😀 Updated"),
        );
        assert_eq!(index.parses, 2);
        assert_eq!(index.rendered, 1);
        assert_eq!(next.values.len(), 2);
        index.rebuild();
        let rebuilt = index.batch(
            Language::Rust,
            DocumentRevision(4),
            &source.replace("First", "😀 Updated"),
        );
        assert_eq!(index.rendered, 0);
        assert!(rebuilt.values == next.values);
        if let (
            Kind::Replace {
                style: Some(InlineStyle::Documentation(a)),
                ..
            },
            Kind::Replace {
                style: Some(InlineStyle::Documentation(b)),
                ..
            },
        ) = (&initial.values[1].kind, &next.values[1].kind)
        {
            assert!(Rc::ptr_eq(a, b));
        } else {
            panic!();
        }
    }
    #[test]
    fn pending_deltas_and_revealed_rows_follow_canonical_revisions() {
        let source = "/// # First\n///\n/// body\nfn one() {}\n\n/// Second\nfn two() {}\n";
        let mut index = Index::default();
        let values = index.replacements(Language::Rust, DocumentRevision(1), source);
        let preview = crate::native_layout::PresentationState {
            provider: 3,
            ..Default::default()
        };
        assert_eq!(index.unwrapped_rows(preview), vec![1, 2]);
        assert_eq!(index.unwrapped_owner_row(0), Some(0));
        assert_eq!(index.unwrapped_owner_row(values[1].start), Some(5));
        assert_eq!(index.unwrapped_owner_row(1), None);
        let revealed = crate::native_presentation::preview_state(&values, 0, None, 3);
        assert!(index.unwrapped_rows(revealed).is_empty());
        index.apply_delta(
            DocumentRevision(1),
            DocumentRevision(2),
            source.len()..source.len(),
            "x",
        );
        index.apply_delta(
            DocumentRevision(2),
            DocumentRevision(3),
            source.len() + 1..source.len() + 1,
            "y",
        );
        assert_eq!(
            index.updated_source(DocumentRevision(3)).as_deref(),
            Some(format!("{source}xy").as_str())
        );
        assert!(index.updated_source(DocumentRevision(2)).is_none());
        index.apply_delta(DocumentRevision(1), DocumentRevision(4), 0..0, "wrong");
        assert!(index.updated_source(DocumentRevision(4)).is_none());
        assert_eq!(
            index
                .replacements(Language::Rust, DocumentRevision(4), source)
                .len(),
            2
        );
    }

    #[test]
    fn rust_tail_edits_retain_owners_and_new_tokens_force_classification() {
        let source = "/// # Docs 猫\nfn one() {}\n";
        let mut index = Index::default();
        let original = index.replacements(Language::Rust, DocumentRevision(1), source);
        index.apply_delta(
            DocumentRevision(1),
            DocumentRevision(2),
            source.len()..source.len(),
            "fn tail() {}",
        );
        let updated = format!("{source}fn tail() {{}}");
        let current = index.replacements(Language::Rust, DocumentRevision(2), &updated);
        assert!(Rc::ptr_eq(&original, &current));
        assert_eq!(index.parses, 2); // syntax still updates incrementally
        assert_eq!(index.rendered, 0);
        index.apply_delta(
            DocumentRevision(2),
            DocumentRevision(3),
            updated.len()..updated.len(),
            "\n/// New\nfn two() {}\n",
        );
        let updated = format!("{updated}\n/// New\nfn two() {{}}\n");
        let current = index.replacements(Language::Rust, DocumentRevision(3), &updated);
        let fresh = Index::default().replacements(Language::Rust, DocumentRevision(3), &updated);
        assert!(*current == *fresh);
        assert_eq!(current.len(), 2);
        // Removing the newline immediately following a comment changes its
        // body; it must never retain that owner's rendered payload.
        let end = "/// # Docs 猫".len();
        index.apply_delta(DocumentRevision(3), DocumentRevision(4), end..end + 1, "");
        let updated = format!("{}{}", &updated[..end], &updated[end + 1..]);
        assert!(
            index.replacements(Language::Rust, DocumentRevision(4), &updated)
                == Index::default().replacements(Language::Rust, DocumentRevision(4), &updated)
        );
    }

    #[test]
    fn retained_heights_require_the_same_visible_owner() {
        let source = "/// Docs\nfn one() {}\n";
        let mut index = Index::default();
        let values = index.replacements(Language::Rust, DocumentRevision(1), source);
        let Some(InlineStyle::Documentation(body)) = &values[0].style else {
            panic!("documentation payload");
        };
        let preview =
            crate::native_presentation::preview_state(&values, source.chars().count(), None, 3);
        let entry = crate::native_layout::CachedRunHeight {
            revision: DocumentRevision(1),
            row: 0,
            start: values[0].start,
            viewport_width: 500.0,
            font_epoch: 1,
            height: 60.0,
            presentation: preview,
            documentation: Some((values[0].end, body.clone())),
        };
        assert!(entry.documentation_matches(&values, preview));
        assert!(!entry.documentation_matches(
            &values,
            crate::native_presentation::preview_state(&values, 0, None, 3)
        ));
        // Source edits before/inside an owner and independently parsed payloads
        // cannot borrow its geometry, even with identical rendered prose.
        for changed in [
            "\n/// Docs\nfn one() {}\n",
            "/// Longer docs\nfn one() {}\n",
            source,
        ] {
            let replacements =
                Index::default().replacements(Language::Rust, DocumentRevision(2), changed);
            assert!(!entry.documentation_matches(&replacements, preview));
        }
        index.apply_delta(
            DocumentRevision(1),
            DocumentRevision(2),
            source.len()..source.len(),
            "fn two() {}",
        );
        let values = index.replacements(
            Language::Rust,
            DocumentRevision(2),
            &format!("{source}fn two() {{}}"),
        );
        assert!(entry.documentation_matches(&values, preview));
    }

    #[test]
    fn syntax_points_remain_byte_based_after_unicode_and_multiline_splices() {
        let mut source = "/// Docs 猫\nfn one() {}\n\n".to_string();
        let mut index = Index::default();
        index.replacements(Language::Rust, DocumentRevision(1), &source);
        let mut revision = 1;
        for (start, removed, inserted) in [
            (0, 0, "// 😀\n"),
            (10, 0, "x\ny\n"),
            (10, 4, ""),
            (0, 10, ""),
        ] {
            index.apply_delta(
                DocumentRevision(revision),
                DocumentRevision(revision + 1),
                start..start + removed,
                inserted,
            );
            source.replace_range(start..start + removed, inserted);
            revision += 1;
            let expected: Vec<_> = std::iter::once(0)
                .chain(source.match_indices('\n').map(|(byte, _)| byte + 1))
                .collect();
            assert_eq!(index.line_starts, expected);
            let current = index.replacements(Language::Rust, DocumentRevision(revision), &source);
            let fresh =
                Index::default().replacements(Language::Rust, DocumentRevision(revision), &source);
            assert!(current == fresh);
        }
    }

    #[test]
    fn markdown_keeps_unsupported_content_literal_and_never_resolves_wiki() {
        let doc=render("# Title\n\n**Bold** [Web](https://example.org) [[Wiki]] $x$ <safe> ![alt](image.png) [Vec](Vec) [Unsafe](javascript:alert)\n\n```rust,no_run\nfn f() {}\n```\n\n| A | B |\n|---|---|\n|one|two|\n");
        assert!(doc.label.contains(
            "[[Wiki]] $x$ <safe> ![alt](image.png) [Vec](Vec) [Unsafe](javascript:alert)"
        ));
        assert!(doc
            .blocks
            .iter()
            .any(|block| matches!(block, Block::Table { .. })));
        assert!(doc
            .blocks
            .iter()
            .any(|block| matches!(block,Block::Code{language,..} if language=="rust")));
        assert_eq!(doc.fragments.iter().filter(|f| f.link.is_some()).count(), 1);
    }
}

#[cfg(all(test, feature = "markdown-preview"))]
mod p212_regression {
    #[test]
    fn crash_retains_unparsed_source() {
        for source in crate::markdown_guard::CRASH_INPUTS {
            let rust = source
                .lines()
                .map(|line| format!("/// {line}\n"))
                .collect::<String>()
                + "fn example() {}\n";
            let python = format!("\"\"\"{source}\"\"\"\n");
            let julia = format!("\"\"\"{source}\"\"\"\nf() = 1\n");
            for (language, code) in [
                (super::Language::Rust, rust),
                (super::Language::Python, python),
                (super::Language::Julia, julia),
            ] {
                let mut index = super::Index::default();
                let result = index.batch(
                    language,
                    moonkale_ext_api::editor::DocumentRevision(1),
                    &code,
                );
                assert_eq!(
                    result.revision,
                    moonkale_ext_api::editor::DocumentRevision(1)
                );
            }
            let document = super::render(source);
            assert_eq!(document.fragments.len(), 1);
            assert_eq!(document.fragments[0].text, source);
            assert_eq!(document.label, source);
            assert!(
                matches!(&document.blocks[..], [super::Block::Text { fragments, .. }] if fragments == &(0..1))
            );
        }
    }
}
