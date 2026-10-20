//! Module-level Julia resolution (normalizer v5; SPEC-008 in Sophia's
//! vault, SR-116). Julia resolves a name in a module's global scope, and a
//! package is one module spread over `include`d files, so references are
//! decided per module: a name refers to a definition only when exactly one
//! definition binds it there and nothing else could. The crate does no IO:
//! the caller reads the files [`includes`] names, in include order.
//!
//! Which identifiers are locals is the normal form's business (v4 scoping
//! wins over a reference), so the resolver proposes a reference for every
//! identifier spelling a unique module binding and the encoding decides.

use crate::{
    definition_and_uses, identities, is_definition, Budget, ContentIdError, Identity, Part, Result,
    Syntax, Tree,
};
use std::collections::{BTreeSet, HashMap, HashSet};

/// At most this many files in one module.
pub const MAX_MODULE_FILES: usize = 1024;
/// At most this many source bytes in one module.
pub const MAX_MODULE_BYTES: usize = 32 * 1024 * 1024;
/// Work allowed for a module: this base plus [`MODULE_STEPS_PER_BYTE`] per
/// source byte. The limits are part of the normal form: over them, no ids.
pub const MODULE_STEPS_BASE: usize = 1_000_000;
pub const MODULE_STEPS_PER_BYTE: usize = 64;

/// A definition: its file (index into the module's files) and the byte
/// span of its syntax node.
pub type DefKey = (u32, usize, usize);

/// One `include(…)` call in a file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Include {
    /// The call's byte span.
    pub span: (usize, usize),
    /// The path as written, when it is a single string literal; `None` is a
    /// dynamic include, which is never followed.
    pub path: Option<String>,
    /// The `module … end` blocks around the call, within its file.
    pub module: Vec<String>,
}

/// One file of a module, as the caller read it.
pub struct ModuleFile<'a> {
    pub text: &'a str,
    pub tree: &'a Tree,
    /// The module path the file was included at (empty for the root).
    pub module: Vec<String>,
}

/// One unit: a definition directly in a module's scope.
#[derive(Clone, Debug)]
pub struct JuliaDefinition {
    pub key: DefKey,
    /// Its name, if it binds one (`Base.show(…) = …` binds nothing here).
    pub name: Option<String>,
    pub module: Vec<String>,
    pub identity: std::result::Result<Identity, ContentIdError>,
    /// The references its encoding uses: span → definition.
    pub references: Vec<((usize, usize), DefKey)>,
    /// The files its id depends on, transitively.
    pub depends_on: BTreeSet<u32>,
}

/// A module's definitions and its calls to generic functions.
#[derive(Clone, Debug, Default)]
pub struct JuliaModule {
    pub definitions: Vec<JuliaDefinition>,
    /// Uses of a function with several methods here, as the encoding reads
    /// them (a local of that name wins): (file, span, name).
    /// Drawn as edges; never part of an id.
    pub calls_generic: Vec<(u32, (usize, usize), String)>,
}

fn text<'a>(node: Syntax, src: &'a str) -> &'a str {
    &src[node.byte_range()]
}

/// The `module … end` names around a node, within its file.
fn module_path(mut node: Syntax, src: &str) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(parent) = node.parent() {
        if parent.kind() == "module_definition" {
            if let Some(name) = parent
                .child_by_field_name("name")
                .or_else(|| parent.named_child(0))
            {
                out.push(text(name, src).to_string());
            }
        }
        node = parent;
    }
    out.reverse();
    out
}

/// The `include` calls of a file, in source order.
pub fn includes(text_: &str, tree: &Tree) -> Vec<Include> {
    let mut out = Vec::new();
    let mut stack = vec![tree.root_node()];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression"
            && node
                .named_child(0)
                .is_some_and(|f| f.kind() == "identifier" && text(f, text_) == "include")
        {
            let arguments = node.named_child(1);
            let single = arguments.filter(|a| a.named_child_count() == 1);
            let path = single
                .and_then(|a| a.named_child(0))
                .filter(|a| a.kind() == "string_literal" && a.named_child_count() == 1)
                .and_then(|a| a.named_child(0))
                .filter(|c| c.kind() == "content")
                .map(|c| text(c, text_).to_string());
            out.push(Include {
                span: (node.start_byte(), node.end_byte()),
                path,
                module: module_path(node, text_),
            });
            continue;
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    out
}

/// Units: definitions whose ancestors are only modules. A module itself is
/// a namespace, not a unit. Nothing inside a call is a unit: there a
/// definition-like form is a keyword argument (`@deprecate f(; check::Bool
/// = true) g`), or a closure in a `do` block (v7).
fn units<'t>(root: Syntax<'t>) -> Vec<Syntax<'t>> {
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() != "module_definition" && is_definition("julia", node) {
            out.push(node);
            continue;
        }
        if node.kind() == "call_expression" {
            continue;
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
    out
}

/// Names a `using`/`import` mentions, by module path: a local definition
/// of such a name may extend or clash with the imported binding.
fn imported(root: Syntax, src: &str, base: &[String], out: &mut HashSet<(Vec<String>, String)>) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if matches!(node.kind(), "using_statement" | "import_statement") {
            let mut module = base.to_vec();
            module.extend(module_path(node, src));
            let mut inner = vec![node];
            while let Some(n) = inner.pop() {
                if matches!(n.kind(), "identifier" | "operator") {
                    out.insert((module.clone(), text(n, src).to_string()));
                }
                let mut cursor = n.walk();
                inner.extend(n.children(&mut cursor));
            }
            continue;
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
}

/// Whether an identifier is a use that could refer to a module binding:
/// not a field (`x.name`), a field declaration in a struct, a keyword or
/// named-tuple label, or inside a string, symbol, quote or `using`/
/// `import`/`export` statement.
fn is_use(node: Syntax) -> bool {
    let Some(parent) = node.parent() else {
        return true;
    };
    let first = |p: Syntax| p.named_child(0).is_some_and(|c| c.id() == node.id());
    if parent.kind() == "field_expression" && !first(parent) {
        return false;
    }
    if matches!(parent.kind(), "assignment" | "named_argument")
        && first(parent)
        && parent
            .parent()
            .is_some_and(|g| matches!(g.kind(), "argument_list" | "tuple_expression"))
    {
        return false;
    }
    // `struct S; a; b::T; c::T = 1; end`: field names.
    let mut field = parent;
    let mut child = node;
    while matches!(field.kind(), "typed_expression" | "assignment") && first(field) {
        child = field;
        field = match field.parent() {
            Some(p) => p,
            None => break,
        };
    }
    if field.kind() == "block"
        && field
            .parent()
            .is_some_and(|s| s.kind() == "struct_definition")
        && (child.id() == node.id() || child.named_child(0).is_some())
    {
        return false;
    }
    let mut ancestor = Some(parent);
    while let Some(a) = ancestor {
        if matches!(
            a.kind(),
            "string_literal"
                | "command_literal"
                | "prefixed_string_literal"
                | "quote_expression"
                | "quote_statement"
                | "using_statement"
                | "import_statement"
                | "export_statement"
        ) {
            return false;
        }
        ancestor = a.parent();
    }
    true
}

/// Resolves and identifies a whole module. `dynamic` lists the module
/// paths with a dynamic include: their functions stay open, since an
/// unseen file could add methods; types and constants still resolve.
///
/// A definition the normal form refuses has no id, and neither does any
/// definition whose encoding refers to it.
pub fn module(files: &[ModuleFile], dynamic: &[Vec<String>]) -> Result<JuliaModule> {
    if files.len() > MAX_MODULE_FILES {
        return Err(ContentIdError::LimitExceeded);
    }
    let bytes: usize = files.iter().map(|f| f.text.len()).sum();
    if bytes > MAX_MODULE_BYTES {
        return Err(ContentIdError::LimitExceeded);
    }
    let mut budget = Budget::with_steps(MODULE_STEPS_BASE + MODULE_STEPS_PER_BYTE * bytes);
    struct Unit<'t> {
        key: DefKey,
        node: Syntax<'t>,
        name: Option<(String, (usize, usize))>,
        module: Vec<String>,
        function: bool,
    }
    let mut all = Vec::new();
    let mut scope: HashMap<(Vec<String>, String), Vec<usize>> = HashMap::new();
    let mut blocked = HashSet::new();
    for (f, file) in files.iter().enumerate() {
        let root = file.tree.root_node();
        imported(root, file.text, &file.module, &mut blocked);
        for node in units(root) {
            budget.step()?;
            let mut module = file.module.clone();
            module.extend(module_path(node, file.text));
            let named = crate::julia_name(node);
            let name = named.filter(|(_, qualified)| !qualified).map(|(n, _)| {
                (
                    text(n, file.text).to_string(),
                    (n.start_byte(), n.end_byte()),
                )
            });
            let function = matches!(node.kind(), "function_definition" | "assignment");
            if let Some((name, _)) = &name {
                if node.kind() != "macro_definition" {
                    scope
                        .entry((module.clone(), name.clone()))
                        .or_default()
                        .push(all.len());
                }
            }
            all.push(Unit {
                key: (f as u32, node.start_byte(), node.end_byte()),
                node,
                name,
                module,
                function,
            });
        }
    }
    let dynamic: HashSet<&Vec<String>> = dynamic.iter().collect();
    let mut out = JuliaModule::default();
    let mut proposed: Vec<HashMap<(usize, usize), usize>> = Vec::new();
    let mut generic: Vec<HashMap<(usize, usize), String>> = Vec::new();
    for unit in &all {
        let mut uses = HashMap::new();
        let src = files[unit.key.0 as usize].text;
        let own = unit.name.as_ref().map(|(_, span)| *span);
        let mut references = HashMap::new();
        let mut stack = vec![unit.node];
        while let Some(node) = stack.pop() {
            budget.step()?;
            if matches!(node.kind(), "identifier" | "operator") && is_use(node) {
                let span = (node.start_byte(), node.end_byte());
                let key = (unit.module.clone(), text(node, src).to_string());
                if Some(span) != own && !blocked.contains(&key) {
                    match scope.get(&key).map(Vec::as_slice) {
                        Some(&[target])
                            if !(all[target].function && dynamic.contains(&unit.module)) =>
                        {
                            references.insert(span, target);
                        }
                        Some(targets)
                            if targets.len() > 1 && targets.iter().all(|&t| all[t].function) =>
                        {
                            uses.insert(span, key.1);
                        }
                        _ => {}
                    }
                }
            }
            let mut cursor = node.walk();
            stack.extend(node.children(&mut cursor));
        }
        proposed.push(references);
        generic.push(uses);
    }
    // Encode every unit; a refusal propagates to the units that refer to it.
    let mut parts: Vec<std::result::Result<Vec<Part<usize>>, ContentIdError>> = Vec::new();
    for (i, unit) in all.iter().enumerate() {
        let src = files[unit.key.0 as usize].text;
        // Generic uses count only where the encoding reads a module name.
        let probes = generic[i].keys().copied().collect();
        let encoded =
            definition_and_uses("julia", unit.node, src, &proposed[i], &probes, &mut budget);
        parts.push(encoded.map(|(parts, hits)| {
            for span in hits {
                out.calls_generic
                    .push((unit.key.0, span, generic[i][&span].clone()));
            }
            parts
        }));
        if matches!(parts[i], Err(ContentIdError::LimitExceeded)) {
            return Err(ContentIdError::LimitExceeded);
        }
    }
    let targets = |p: &[Part<usize>]| -> Vec<usize> {
        p.iter()
            .filter_map(|p| match p {
                Part::Reference(t) => Some(*t),
                _ => None,
            })
            .collect()
    };
    loop {
        budget.step()?;
        let refused: Vec<usize> = (0..all.len())
            .filter(|&i| {
                parts[i]
                    .as_ref()
                    .is_ok_and(|p| targets(p).iter().any(|&t| parts[t].is_err()))
            })
            .collect();
        if refused.is_empty() {
            break;
        }
        for i in refused {
            parts[i] = Err(ContentIdError::DependencyRefused);
        }
    }
    let units: HashMap<usize, Vec<Part<usize>>> = parts
        .iter()
        .enumerate()
        .filter_map(|(i, p)| p.as_ref().ok().map(|p| (i, p.clone())))
        .collect();
    let ids = identities(&units, &mut budget)?;
    for (i, unit) in all.iter().enumerate() {
        let mut depends_on = BTreeSet::new();
        let mut seen = HashSet::new();
        let mut stack = vec![i];
        while let Some(j) = stack.pop() {
            budget.step()?;
            if !seen.insert(j) {
                continue;
            }
            depends_on.insert(all[j].key.0);
            if let Some(p) = units.get(&j) {
                stack.extend(targets(p));
            }
        }
        let references = units
            .get(&i)
            .map(|_| {
                let mut used: Vec<_> = proposed[i]
                    .iter()
                    .filter(|(_, t)| units.get(&i).is_some_and(|p| targets(p).contains(t)))
                    .map(|(span, &t)| (*span, all[t].key))
                    .collect();
                used.sort();
                used
            })
            .unwrap_or_default();
        out.definitions.push(JuliaDefinition {
            key: unit.key,
            name: unit.name.as_ref().map(|(n, _)| n.clone()),
            module: unit.module.clone(),
            identity: match &parts[i] {
                Ok(_) => ids.get(&i).cloned().ok_or(ContentIdError::MalformedSyntax),
                Err(e) => Err(e.clone()),
            },
            references,
            depends_on,
        });
    }
    Ok(out)
}
