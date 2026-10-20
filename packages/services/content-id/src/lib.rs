//! Versioned, conservative syntactic content identities (WP-66) for Rust,
//! Julia and Python definitions. No compiler or code execution. A failed
//! request never publishes a partial identity. See Code Graph Schema §4.
//!
//! The crate has the parts that make an id: the per-language normal form
//! ([`definition`]) and the hashing of a definition graph by recursive group
//! ([`identities`]). Which identifier refers to which definition is the
//! caller's input: Moonkale's index resolves it with its symbol extractors.
//! The same text gets the same id only when references resolve the same way;
//! `golden/` has fixtures with Moonkale's ids for a cross-check (SR-116).
//! Keys are opaque to the crate: any `Copy + Eq + Hash` value per definition.

mod component;
pub mod julia;
mod normalize;

pub use arborium_tree_sitter::{Node as Syntax, Tree};
pub use component::identities;
pub use normalize::definition;

/// Changes whenever canonical encoding, language normalization or the
/// reference resolution that feeds it changes. 2: Julia/Python parameter
/// types and defaults are uses, not local names (2026-10-20). 3: a name
/// wrapper with the name's span is the name (Julia `struct`); a Rust struct
/// expression's type is a use (2026-10-20). 4: Julia local scopes (loops,
/// closures, `do`, `let`, `try`, comprehensions), varargs, keywords,
/// destructuring and anonymous `::T` parameters (2026-10-20). 5: Julia
/// references resolve per module, not per file (SPEC-008, 2026-10-20).
pub const NORMALIZER_VERSION: u32 = 5;
/// Maximum bytes of a single source file.
pub const MAX_FILE_BYTES: usize = 512 * 1024;
/// Maximum reachable definitions in one request.
pub const MAX_DEFINITIONS: usize = 4096;
/// Maximum canonicalization work per request (syntax nodes and graph visits).
pub const MAX_STEPS: usize = 1_000_000;
/// Maximum aggregate canonical syntax bytes retained by a request.
pub const MAX_INPUT_BYTES: usize = 32 * 1024 * 1024;

/// A definition's identity: digests only; the caller names it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    /// Full BLAKE3 digest of the member.
    pub digest: [u8; 32],
    pub normalizer_version: u32,
    /// Digest of the definition's recursive group.
    pub component_digest: [u8; 32],
    pub component_index: u32,
    pub component_size: u32,
}

/// A conservative failure: no approximate identity is substituted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContentIdError {
    NotDefinition,
    UnsupportedSyntax,
    MalformedSyntax,
    LimitExceeded,
    StaleInput,
    SourceUnavailable,
    WorkerFailed,
    /// A definition it refers to has no id, so it has none either.
    DependencyRefused,
}

impl std::fmt::Display for ContentIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "content identity: {self:?}")
    }
}
impl std::error::Error for ContentIdError {}

pub type Result<T> = std::result::Result<T, ContentIdError>;

/// One definition's canonical encoding: syntax bytes and references, in
/// syntax order.
#[derive(Clone, Debug)]
pub enum Part<K> {
    Bytes(Vec<u8>),
    Reference(K),
}

/// The work bound of one request; share it across all its calls.
pub struct Budget {
    steps: usize,
    bytes: usize,
    limit: usize,
}
impl Default for Budget {
    fn default() -> Self {
        Self::with_steps(MAX_STEPS)
    }
}
impl Budget {
    pub fn new() -> Self {
        Self::default()
    }
    /// A budget of `limit` steps (a module's: see [`julia::module`]).
    pub fn with_steps(limit: usize) -> Self {
        Self {
            steps: 0,
            bytes: 0,
            limit,
        }
    }
    /// Counts one unit of work; fails once the request is over its bounds.
    pub fn step(&mut self) -> Result<()> {
        self.steps += 1;
        if self.steps > self.limit || self.bytes > MAX_INPUT_BYTES {
            Err(ContentIdError::LimitExceeded)
        } else {
            Ok(())
        }
    }
}

/// Unambiguous length-delimited field. All integers in the encoding are LE.
fn field(out: &mut Vec<u8>, tag: &[u8], bytes: &[u8]) {
    out.extend_from_slice(&(tag.len() as u32).to_le_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(bytes);
}

/// The grammar for `rust`, `python` or `julia`.
pub fn language(lang: &str) -> Result<arborium_tree_sitter::Language> {
    Ok(match lang {
        "rust" => arborium_rust::language(),
        "python" => arborium_python::language(),
        "julia" => arborium_julia::language(),
        _ => return Err(ContentIdError::UnsupportedSyntax),
    }
    .into())
}

/// Parses a file with bounded parser work, and fails when it has more than
/// [`MAX_DEFINITIONS`] definitions or nests deeper than the walkers allow.
pub fn parse(lang: &str, text: &str, budget: &mut Budget) -> Result<Tree> {
    if text.len() > MAX_FILE_BYTES {
        return Err(ContentIdError::LimitExceeded);
    }
    let mut parser = arborium_tree_sitter::Parser::new();
    parser
        .set_language(&language(lang)?)
        .map_err(|_| ContentIdError::UnsupportedSyntax)?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut checks = 0usize;
    let mut progress = |_state: &arborium_tree_sitter::ParseState| {
        checks += 1;
        if checks > MAX_STEPS || std::time::Instant::now() >= deadline {
            std::ops::ControlFlow::Break(())
        } else {
            std::ops::ControlFlow::Continue(())
        }
    };
    let options = arborium_tree_sitter::ParseOptions::new().progress_callback(&mut progress);
    let mut input = |offset: usize, _| &text.as_bytes()[offset.min(text.len())..];
    let tree = parser
        .parse_with_options(&mut input, None, Some(options))
        .ok_or(ContentIdError::LimitExceeded)?;
    // Protect recursive walkers and bound resolver input.
    let mut stack = vec![(tree.root_node(), 0)];
    let mut definition_count = 0;
    while let Some((node, depth)) = stack.pop() {
        budget.step()?;
        if depth > 128 {
            return Err(ContentIdError::LimitExceeded);
        }
        if is_definition(lang, node) {
            definition_count += 1;
            if definition_count > MAX_DEFINITIONS {
                return Err(ContentIdError::LimitExceeded);
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor).map(|child| (child, depth + 1)));
    }
    Ok(tree)
}

/// Whether a syntax node is a definition the normal form covers.
pub fn is_definition(lang: &str, node: Syntax) -> bool {
    matches!(
        node.kind(),
        "function_item"
            | "function_signature_item"
            | "function_definition"
            | "struct_item"
            | "enum_item"
            | "trait_item"
            | "mod_item"
            | "const_item"
            | "static_item"
            | "type_item"
            | "union_item"
            | "macro_definition"
            | "class_definition"
            | "struct_definition"
            | "module_definition"
            | "abstract_definition"
            | "primitive_definition"
            | "const_statement"
    ) || (lang == "julia" && short_function(node))
}

/// Julia's `f(x) = …` form.
fn short_function(node: Syntax) -> bool {
    node.kind() == "assignment"
        && node.named_child(0).is_some_and(|n| {
            matches!(
                n.kind(),
                "call_expression" | "where_expression" | "typed_expression"
            )
        })
}
