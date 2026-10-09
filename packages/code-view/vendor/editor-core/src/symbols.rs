//! First-class symbol / outline data model.
//!
//! This module provides UI-agnostic types for:
//! - document outline (document symbols, typically hierarchical)
//! - workspace symbol search results (usually flat, cross-file)
//!
//! The goal is to give hosts a stable schema to build:
//! - outline trees
//! - fuzzy search over symbols
//! - navigation/jump lists

/// A half-open character-offset range (`start..end`) in the document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SymbolRange {
    /// Range start offset (inclusive), in Unicode scalar values (`char`) from the start of the document.
    pub start: usize,
    /// Range end offset (exclusive), in Unicode scalar values (`char`) from the start of the document.
    pub end: usize,
}

impl SymbolRange {
    /// Create a new symbol range.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

/// A UTF-16 coordinate used by protocols like LSP.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utf16Position {
    /// Zero-based line index.
    pub line: u32,
    /// Zero-based UTF-16 code unit offset within the line.
    pub character: u32,
}

impl Utf16Position {
    /// Create a new UTF-16 position.
    pub fn new(line: u32, character: u32) -> Self {
        Self { line, character }
    }
}

/// A UTF-16 range (`start..end`) in `(line, character)` coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Utf16Range {
    /// Range start position (inclusive).
    pub start: Utf16Position,
    /// Range end position (exclusive).
    pub end: Utf16Position,
}

impl Utf16Range {
    /// Create a new UTF-16 range.
    pub fn new(start: Utf16Position, end: Utf16Position) -> Self {
        Self { start, end }
    }
}

/// A cross-file symbol location (URI + UTF-16 range).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolLocation {
    /// Document URI (LSP-style).
    pub uri: String,
    /// Location range (UTF-16).
    pub range: Utf16Range,
}

/// A coarse symbol kind tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SymbolKind {
    /// A file-level symbol.
    File,
    /// A module symbol.
    Module,
    /// A namespace symbol.
    Namespace,
    /// A package symbol.
    Package,
    /// A class symbol.
    Class,
    /// A method symbol.
    Method,
    /// A property symbol.
    Property,
    /// A field symbol.
    Field,
    /// A constructor symbol.
    Constructor,
    /// An enum symbol.
    Enum,
    /// An interface symbol.
    Interface,
    /// A function symbol.
    Function,
    /// A variable symbol.
    Variable,
    /// A constant symbol.
    Constant,
    /// A string literal / string-like symbol.
    String,
    /// A numeric symbol.
    Number,
    /// A boolean symbol.
    Boolean,
    /// An array symbol.
    Array,
    /// An object symbol.
    Object,
    /// A key symbol.
    Key,
    /// A null symbol.
    Null,
    /// An enum member symbol.
    EnumMember,
    /// A struct symbol.
    Struct,
    /// An event symbol.
    Event,
    /// An operator symbol.
    Operator,
    /// A type parameter symbol.
    TypeParameter,
    /// An integration-defined kind value.
    Custom(u32),
}

impl SymbolKind {
    /// Convert an LSP `SymbolKind` numeric value into a [`SymbolKind`].
    pub fn from_lsp_kind(kind: u32) -> Self {
        match kind {
            1 => Self::File,
            2 => Self::Module,
            3 => Self::Namespace,
            4 => Self::Package,
            5 => Self::Class,
            6 => Self::Method,
            7 => Self::Property,
            8 => Self::Field,
            9 => Self::Constructor,
            10 => Self::Enum,
            11 => Self::Interface,
            12 => Self::Function,
            13 => Self::Variable,
            14 => Self::Constant,
            15 => Self::String,
            16 => Self::Number,
            17 => Self::Boolean,
            18 => Self::Array,
            19 => Self::Object,
            20 => Self::Key,
            21 => Self::Null,
            22 => Self::EnumMember,
            23 => Self::Struct,
            24 => Self::Event,
            25 => Self::Operator,
            26 => Self::TypeParameter,
            other => Self::Custom(other),
        }
    }
}

/// A single document symbol node (hierarchical).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSymbol {
    /// Symbol name (e.g. function name).
    pub name: String,
    /// Optional detail string (e.g. type signature).
    pub detail: Option<String>,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// Full symbol span (character offsets).
    pub range: SymbolRange,
    /// Selection span (character offsets).
    pub selection_range: SymbolRange,
    /// Child symbols.
    pub children: Vec<DocumentSymbol>,
    /// Optional raw integration payload, encoded as JSON text.
    pub data_json: Option<String>,
}

impl DocumentSymbol {
    /// Collect this node and all descendants in pre-order.
    pub fn flatten_preorder<'a>(&'a self, out: &mut Vec<&'a DocumentSymbol>) {
        out.push(self);
        for child in &self.children {
            child.flatten_preorder(out);
        }
    }

    /// Find all symbols with the given name (pre-order).
    pub fn find_by_name<'a>(&'a self, name: &str, out: &mut Vec<&'a DocumentSymbol>) {
        if self.name == name {
            out.push(self);
        }
        for child in &self.children {
            child.find_by_name(name, out);
        }
    }
}

/// A document outline (top-level symbol list).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DocumentOutline {
    /// Top-level symbols.
    pub symbols: Vec<DocumentSymbol>,
}

impl DocumentOutline {
    /// Create a new outline.
    pub fn new(symbols: Vec<DocumentSymbol>) -> Self {
        Self { symbols }
    }

    /// Returns true if there are no symbols.
    pub fn is_empty(&self) -> bool {
        self.symbols.is_empty()
    }

    /// Return the top-level symbol count.
    pub fn top_level_count(&self) -> usize {
        self.symbols.len()
    }

    /// Flatten all symbols in pre-order.
    pub fn flatten_preorder(&self) -> Vec<&DocumentSymbol> {
        let mut out = Vec::new();
        for sym in &self.symbols {
            sym.flatten_preorder(&mut out);
        }
        out
    }

    /// Find all symbols with the given name (pre-order).
    pub fn find_by_name(&self, name: &str) -> Vec<&DocumentSymbol> {
        let mut out = Vec::new();
        for sym in &self.symbols {
            sym.find_by_name(name, &mut out);
        }
        out
    }
}

/// A workspace symbol (cross-file, usually flat).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSymbol {
    /// Symbol name.
    pub name: String,
    /// Optional detail string.
    pub detail: Option<String>,
    /// Symbol kind.
    pub kind: SymbolKind,
    /// Symbol location.
    pub location: SymbolLocation,
    /// Optional container name (e.g. namespace/module).
    pub container_name: Option<String>,
    /// Optional raw integration payload, encoded as JSON text.
    pub data_json: Option<String>,
}
