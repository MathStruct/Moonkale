//! Lossless Rust snapshots and pure, guarded patch plans (WP-67 foundation).
//!
//! Declaration rename changes only its name token, **not references**. Moves
//! reorder top-level functions within one file. Neither operation writes a
//! document or source; a future consumer must use Workspace's guarded edit path.

use arborium_tree_sitter::{Node, Parser, Tree};
use std::{ops::Range, sync::Arc};

/// Upper bound for one explicitly requested syntax snapshot/output.
pub const MAX_SOURCE_BYTES: usize = 512 * 1024;
const MAX_NODES: usize = 200_000;
const MAX_DEPTH: usize = 128;

/// Failure leaves the input unchanged and never produces a partial plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyntaxEditError {
    LimitExceeded,
    MalformedSyntax,
    InvalidName,
    InvalidFunction,
    InvalidRange,
    OverlappingEdits,
    StaleInput,
    InvalidPlacement,
}
impl std::fmt::Display for SyntaxEditError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Rust structural edit: {self:?}")
    }
}
impl std::error::Error for SyntaxEditError {}
type Result<T> = std::result::Result<T, SyntaxEditError>;

/// A terminal syntax token and the exact trivia immediately preceding it.
/// Comments are trivia; the gap may include multiple comments and blank lines.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxToken {
    pub kind: String,
    pub range: Range<usize>,
    pub leading_trivia: Range<usize>,
}

/// A top-level function. All offsets address the snapshot's UTF-8 bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustFunction {
    pub name: String,
    pub name_range: Range<usize>,
    pub syntax_range: Range<usize>,
    /// Leading trivia and trailing same-line comment/terminator travel with it.
    /// File BOM, shebang and inner attributes never belong to a function.
    pub owned_range: Range<usize>,
}

/// One local replacement against a plan's original snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustTextEdit {
    pub range: Range<usize>,
    pub removed_text: String,
    pub inserted_text: String,
}

/// Immutable edits against exactly one input hash. Consumers may inspect edits
/// to build an engine-neutral revisioned EditBatch; this type performs no I/O.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RustEditPlan {
    input_hash: [u8; 32],
    edits: Vec<RustTextEdit>,
}
impl RustEditPlan {
    pub fn input_hash(&self) -> [u8; 32] {
        self.input_hash
    }
    pub fn edits(&self) -> &[RustTextEdit] {
        &self.edits
    }

    /// Validate every edit, then apply right-to-left without changing the caller's text.
    pub fn apply_to(&self, current: &str) -> Result<String> {
        if current.len() > MAX_SOURCE_BYTES {
            return Err(SyntaxEditError::LimitExceeded);
        }
        if blake3::hash(current.as_bytes()).as_bytes() != &self.input_hash {
            return Err(SyntaxEditError::StaleInput);
        }
        let mut previous_end = 0;
        let mut removed = 0usize;
        let mut inserted = 0usize;
        for (i, edit) in self.edits.iter().enumerate() {
            if edit.range.start > edit.range.end
                || current.get(edit.range.clone()) != Some(edit.removed_text.as_str())
            {
                return Err(SyntaxEditError::InvalidRange);
            }
            if i > 0 && edit.range.start < previous_end {
                return Err(SyntaxEditError::OverlappingEdits);
            }
            previous_end = edit.range.end;
            removed = removed
                .checked_add(edit.removed_text.len())
                .ok_or(SyntaxEditError::LimitExceeded)?;
            inserted = inserted
                .checked_add(edit.inserted_text.len())
                .ok_or(SyntaxEditError::LimitExceeded)?;
        }
        current
            .len()
            .checked_sub(removed)
            .and_then(|n| n.checked_add(inserted))
            .filter(|n| *n <= MAX_SOURCE_BYTES)
            .ok_or(SyntaxEditError::LimitExceeded)?;
        let mut result = current.to_string();
        for edit in self.edits.iter().rev() {
            result.replace_range(edit.range.clone(), &edit.inserted_text);
        }
        Ok(result)
    }
}

/// Owned working snapshot, not a global cache. Parsing is explicit; callers
/// must schedule it outside the UI input path. Its source and tree are bounded.
pub struct RustSyntaxSnapshot {
    source: Arc<str>,
    tree: Tree,
    input_hash: [u8; 32],
    tokens: Vec<SyntaxToken>,
    eof_trivia: Range<usize>,
    functions: Vec<RustFunction>,
    tail_start: usize,
}
impl RustSyntaxSnapshot {
    pub fn parse(source: &str) -> Result<Self> {
        let tree = parse_tree(source)?;
        let root = tree.root_node();
        let mut pending = vec![(root, 0)];
        let mut tokens = Vec::new();
        let mut previous = 0;
        let mut count = 0;
        while let Some((node, depth)) = pending.pop() {
            count += 1;
            if count > MAX_NODES || depth > MAX_DEPTH {
                return Err(SyntaxEditError::LimitExceeded);
            }
            if node.kind().contains("comment")
                || (node.id() == root.id() && node.child_count() == 0)
            {
                continue;
            }
            if node.child_count() == 0 {
                let range = node.byte_range();
                if range.is_empty() {
                    continue;
                }
                if range.start < previous || source.get(range.clone()).is_none() {
                    return Err(SyntaxEditError::InvalidRange);
                }
                tokens.push(SyntaxToken {
                    kind: node.kind().into(),
                    leading_trivia: previous..range.start,
                    range: range.clone(),
                });
                previous = range.end;
            } else {
                let mut cursor = node.walk();
                pending.extend(
                    node.children(&mut cursor)
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .map(|n| (n, depth + 1)),
                );
            }
        }
        let mut functions = Vec::new();
        let mut cursor = root.walk();
        let children: Vec<_> = root.named_children(&mut cursor).collect();
        drop(cursor);
        let mut boundary = if source.starts_with('\u{feff}') { 3 } else { 0 };
        for node in &children {
            if node.kind().contains("comment") {
                continue;
            }
            let mut end = trailing_comment_end(source, *node, &children);
            if matches!(node.kind(), "shebang" | "inner_attribute_item") {
                if source[end..].starts_with("\r\n") {
                    end += 2;
                } else if source[end..].starts_with('\n') {
                    end += 1;
                }
            }
            if node.kind() == "function_item" {
                let name = node
                    .child_by_field_name("name")
                    .ok_or(SyntaxEditError::MalformedSyntax)?;
                functions.push(RustFunction {
                    name: source[name.byte_range()].into(),
                    name_range: name.byte_range(),
                    syntax_range: node.byte_range(),
                    owned_range: boundary..end,
                });
            }
            // Non-function items, including shebang/inner attributes, protect
            // everything up to their end from a subsequent function move.
            boundary = end;
        }
        Ok(Self {
            input_hash: *blake3::hash(source.as_bytes()).as_bytes(),
            source: Arc::from(source),
            tree,
            tokens,
            eof_trivia: previous..source.len(),
            functions,
            tail_start: boundary,
        })
    }
    pub fn input_hash(&self) -> [u8; 32] {
        self.input_hash
    }
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn tokens(&self) -> &[SyntaxToken] {
        &self.tokens
    }
    pub fn eof_trivia(&self) -> Range<usize> {
        self.eof_trivia.clone()
    }
    pub fn functions(&self) -> &[RustFunction] {
        &self.functions
    }
    /// Keep the syntax tree with the exact bytes; no lossy AST reconstruction.
    pub fn tree(&self) -> &Tree {
        &self.tree
    }
    /// Reconstruct from token/trivia ranges, including the final trivia gap.
    pub fn print_original(&self) -> String {
        let mut out = String::with_capacity(self.source.len());
        for token in &self.tokens {
            out.push_str(&self.source[token.leading_trivia.clone()]);
            out.push_str(&self.source[token.range.clone()]);
        }
        out.push_str(&self.source[self.eof_trivia.clone()]);
        out
    }

    /// Change only a declaration token. References, imports and other files are
    /// intentionally untouched; this must not be advertised as semantic rename.
    pub fn plan_declaration_rename(&self, function: usize, name: &str) -> Result<RustEditPlan> {
        let item = self
            .functions
            .get(function)
            .ok_or(SyntaxEditError::InvalidFunction)?;
        if name.is_empty()
            || name.len() > 256
            || forbidden_name(name)
            || matches!(name, "r#self" | "r#Self" | "r#crate" | "r#super")
        {
            return Err(SyntaxEditError::InvalidName);
        }
        let probe = format!("fn {name}() {{}}");
        let tree = parse_tree(&probe).map_err(|_| SyntaxEditError::InvalidName)?;
        let function = tree
            .root_node()
            .named_child(0)
            .ok_or(SyntaxEditError::InvalidName)?;
        if function.kind() != "function_item"
            || function
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(probe.as_bytes()).ok())
                != Some(name)
        {
            return Err(SyntaxEditError::InvalidName);
        }
        let edits = if name == item.name {
            Vec::new()
        } else {
            vec![RustTextEdit {
                range: item.name_range.clone(),
                removed_text: item.name.clone(),
                inserted_text: name.into(),
            }]
        };
        self.plan(edits, false)
    }

    /// Move one top-level function before another, or before file-owned EOF
    /// trivia when `before` is None. No imports or references are rewritten.
    pub fn plan_move_function(
        &self,
        function: usize,
        before: Option<usize>,
    ) -> Result<RustEditPlan> {
        let item = self
            .functions
            .get(function)
            .ok_or(SyntaxEditError::InvalidFunction)?;
        let destination = match before {
            Some(index) => {
                self.functions
                    .get(index)
                    .ok_or(SyntaxEditError::InvalidFunction)?
                    .owned_range
                    .start
            }
            None => self.tail_start,
        };
        if destination == item.owned_range.start || destination == item.owned_range.end {
            return self.plan(Vec::new(), true);
        }
        if item.owned_range.contains(&destination) {
            return Err(SyntaxEditError::InvalidPlacement);
        }
        self.plan(
            vec![
                RustTextEdit {
                    range: item.owned_range.clone(),
                    removed_text: self.source[item.owned_range.clone()].into(),
                    inserted_text: String::new(),
                },
                RustTextEdit {
                    range: destination..destination,
                    removed_text: String::new(),
                    inserted_text: self.source[item.owned_range.clone()].into(),
                },
            ],
            true,
        )
    }
    fn token_inventory(&self) -> std::collections::BTreeMap<(&str, &str), usize> {
        let mut counts = std::collections::BTreeMap::new();
        for token in &self.tokens {
            *counts
                .entry((token.kind.as_str(), &self.source[token.range.clone()]))
                .or_default() += 1;
        }
        counts
    }
    fn plan(&self, mut edits: Vec<RustTextEdit>, preserve_tokens: bool) -> Result<RustEditPlan> {
        edits.sort_by_key(|e| e.range.start);
        let plan = RustEditPlan {
            input_hash: self.input_hash,
            edits,
        };
        let result = plan.apply_to(&self.source)?;
        let candidate = Self::parse(&result).map_err(|e| match e {
            SyntaxEditError::LimitExceeded => e,
            _ => SyntaxEditError::InvalidPlacement,
        })?;
        if candidate.functions.len() != self.functions.len()
            || (preserve_tokens && self.token_inventory() != candidate.token_inventory())
        {
            return Err(SyntaxEditError::InvalidPlacement);
        }
        Ok(plan)
    }
}

fn parse_tree(source: &str) -> Result<Tree> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(SyntaxEditError::LimitExceeded);
    }
    let mut parser = Parser::new();
    parser
        .set_language(&arborium_rust::language().into())
        .map_err(|_| SyntaxEditError::MalformedSyntax)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut progress = |_state: &arborium_tree_sitter::ParseState| {
        if std::time::Instant::now() > deadline {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    };
    let options = arborium_tree_sitter::ParseOptions::new().progress_callback(&mut progress);
    let mut input = |offset: usize, _| &source.as_bytes()[offset.min(source.len())..];
    let tree = parser
        .parse_with_options(&mut input, None, Some(options))
        .ok_or(SyntaxEditError::LimitExceeded)?;
    if tree.root_node().has_error() {
        return Err(SyntaxEditError::MalformedSyntax);
    }
    Ok(tree)
}

fn trailing_comment_end(source: &str, node: Node, children: &[Node]) -> usize {
    let mut end = node.end_byte();
    for next in children
        .iter()
        .filter(|n| n.start_byte() >= node.end_byte())
    {
        if !next.kind().contains("comment") {
            break;
        }
        let gap = &source[end..next.start_byte()];
        if gap.contains(['\r', '\n']) || !gap.chars().all(char::is_whitespace) {
            break;
        }
        end = next.end_byte();
        if source[next.byte_range()].starts_with("//") {
            break;
        }
    }
    if end > node.end_byte() {
        let mut terminator = end;
        while matches!(source.as_bytes().get(terminator), Some(b' ' | b'\t')) {
            terminator += 1;
        }
        if source[terminator..].starts_with("\r\n") {
            end = terminator + 2;
        } else if source[terminator..].starts_with('\n') {
            end = terminator + 1;
        }
    }
    end
}

// Conservatively reject strict/reserved words from every edition. Contextual
// words (e.g. union) remain available; raw identifiers bypass reserved words.
fn forbidden_name(name: &str) -> bool {
    matches!(
        name,
        "_" | "r#_"
            | "as"
            | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "abstract"
            | "become"
            | "box"
            | "do"
            | "final"
            | "gen"
            | "macro"
            | "override"
            | "priv"
            | "try"
            | "typeof"
            | "unsized"
            | "virtual"
            | "yield"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_utf8_and_overlapping_edits_are_rejected_before_application() {
        let source = "é abc";
        let mut plan = RustEditPlan {
            input_hash: *blake3::hash(source.as_bytes()).as_bytes(),
            edits: vec![RustTextEdit {
                range: 1..2,
                removed_text: String::new(),
                inserted_text: "x".into(),
            }],
        };
        assert_eq!(plan.apply_to(source), Err(SyntaxEditError::InvalidRange));
        plan.edits = vec![
            RustTextEdit {
                range: 0..2,
                removed_text: "é".into(),
                inserted_text: "x".into(),
            },
            RustTextEdit {
                range: 0..3,
                removed_text: "é ".into(),
                inserted_text: String::new(),
            },
        ];
        assert_eq!(
            plan.apply_to(source),
            Err(SyntaxEditError::OverlappingEdits)
        );
        assert_eq!(source, "é abc");
    }
}
