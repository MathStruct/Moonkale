//! Shared name ownership, ported from Sophia 773d9898 (MIT). No file I/O.
use crate::{Budget, ContentIdError, Result, Syntax};
use std::collections::{HashMap, HashSet};

/// Naming metadata, independent of canonical encoding and content identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Definition {
    pub language: String,
    pub name: String,
    pub kind: String,
    pub start: usize,
    pub end: usize,
    pub name_span: Option<(usize, usize)>,
    pub qualified: bool,
    /// Absolute Julia module path; Rust/Python path relative to package.
    pub module: Vec<String>,
    pub extends: Option<Vec<String>>,
    /// Unknown package root: the path was used as a fallback namespace.
    pub path_derived: bool,
    /// Why this definition cannot bind a store name.
    pub diagnostic: Option<String>,
}
fn text<'a>(n: Syntax, src: &'a str) -> &'a str {
    src.get(n.start_byte()..n.end_byte()).unwrap_or("")
}
fn valid_segment(s: &str) -> bool {
    !s.is_empty()
        && !s
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '`')
}
fn segment(s: &str) -> String {
    if s.chars()
        .next()
        .is_some_and(|c| c.is_alphabetic() || c == '_')
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '!')
    {
        s.to_string()
    } else {
        format!("`{s}`")
    }
}
/// The `module … end` names around a node, within its file.
pub fn modules_around(mut n: Syntax, src: &str) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(p) = n.parent() {
        if p.kind() == "module_definition" {
            if let Some(x) = p.child_by_field_name("name").or_else(|| p.named_child(0)) {
                out.push(text(x, src).to_string());
            }
        }
        n = p;
    }
    out.reverse();
    out
}

/// Julia's module structure, as far as deciding **which generic function a
/// method belongs to** needs it. A generic function is identified by the
/// module that owns its binding and its name, never by its name alone
/// ([[Julia Name Resolution]]): `A.f` and `B.f` are different functions.
#[derive(Debug, Clone, Default)]
pub struct JuliaScopes {
    /// Every module declared (`module … end`), as a path from the root.
    pub modules: HashSet<Vec<String>>,
    /// Conflicting imports have no owner, independent of file read order.
    pub ambiguous_imports: HashSet<(Vec<String>, String)>,
    /// (module, name) → written module path (leading dots, then segments).
    /// Only `import M: name` / `import M.name` extends the imported function.
    pub imports: HashMap<(Vec<String>, String), (usize, Vec<String>)>,
}

impl JuliaScopes {
    fn import(&mut self, key: (Vec<String>, String), path: (usize, Vec<String>)) {
        if self.ambiguous_imports.contains(&key) {
            return;
        }
        if self.imports.get(&key).is_some_and(|old| old != &path) {
            self.imports.remove(&key);
            self.ambiguous_imports.insert(key);
        } else {
            self.imports.insert(key, path);
        }
    }
}

/// A module path as written: `Base.Broadcast` → (0, [Base, Broadcast]);
/// `..B` → (2, [B]).
fn written_path(t: &str) -> (usize, Vec<String>) {
    let dots = t.len() - t.trim_start_matches('.').len();
    let rest = t.trim_start_matches('.');
    (
        dots,
        if rest.is_empty() {
            Vec::new()
        } else {
            rest.split('.').map(|s| s.trim().to_string()).collect()
        },
    )
}

/// Collects the modules and the function imports of one file included at
/// `base`. Only `import` makes a later method extend another module's
/// function; `using M: f` followed by a method for `f` is an error in Julia,
/// and `import M` / `import M as N` imports a module, not a function.
pub fn julia_scopes(root: Syntax, src: &str, base: &[String], out: &mut JuliaScopes) {
    let mut stack = vec![root];
    while let Some(n) = stack.pop() {
        match n.kind() {
            "module_definition" => {
                let mut m = base.to_vec();
                m.extend(modules_around(n, src));
                if let Some(x) = n.child_by_field_name("name").or_else(|| n.named_child(0)) {
                    m.push(text(x, src).to_string());
                }
                out.modules.insert(m);
            }
            "import_statement" => {
                let mut m = base.to_vec();
                m.extend(modules_around(n, src));
                let mut c = n.walk();
                for item in n.named_children(&mut c) {
                    match item.kind() {
                        // `import M: a, b, +`
                        "selected_import" => {
                            let mut ic = item.walk();
                            let parts: Vec<Syntax> = item.named_children(&mut ic).collect();
                            let Some((from, names)) = parts.split_first() else {
                                continue;
                            };
                            let path = written_path(text(*from, src));
                            for x in names {
                                if matches!(x.kind(), "identifier" | "operator") {
                                    out.import(
                                        (m.clone(), text(*x, src).to_string()),
                                        path.clone(),
                                    );
                                }
                            }
                        }
                        // `import M.f` (a single segment imports a module).
                        "import_path" => {
                            let (dots, mut segs) = written_path(text(item, src));
                            if segs.len() >= 2 {
                                let f = segs.pop().unwrap();
                                out.import((m.clone(), f), (dots, segs));
                            }
                        }
                        _ => {}
                    }
                }
                continue;
            }
            _ => {}
        }
        let mut c = n.walk();
        let children: Vec<Syntax> = n.named_children(&mut c).collect();
        stack.extend(children.into_iter().rev());
    }
}

/// Base's submodules that every module sees through the implicit
/// `using Base` (Julia 1.13 `names(Base)`; `Core` and `Main` are top-level
/// already). `Broadcast.broadcasted(…) = …` extends `Base.Broadcast`.
const BASE_SUBMODULES: &[&str] = &[
    "Broadcast",
    "Checked",
    "Docs",
    "Filesystem",
    "GC",
    "IR",
    "Iterators",
    "Libc",
    "MathConstants",
    "Meta",
    "Order",
    "ScopedValues",
    "Sort",
    "StackTraces",
    "Sys",
    "Threads",
];

/// Where a module path written in module `m` points. Relative paths
/// (`.X`, `..X`) go up from `m`. A plain path's first segment is looked up
/// as Julia does, innermost module outwards: a declared submodule, or the
/// enclosing module's own name; then Base's exported submodules; otherwise
/// it is a top-level module (`Base`, `Core`, another package). Bindings made by `using` are not
/// followed (an approximation: [[Julia Name Resolution]]).
pub fn locate(m: &[String], dots: usize, q: &[String], scopes: &JuliaScopes) -> Vec<String> {
    if dots > 0 {
        let up = m.len().saturating_sub(dots - 1);
        return m[..up].iter().chain(q).cloned().collect();
    }
    let Some(first) = q.first() else {
        return m.to_vec();
    };
    for k in (0..=m.len()).rev() {
        let mut cand = m[..k].to_vec();
        cand.push(first.clone());
        if scopes.modules.contains(&cand) {
            return m[..k].iter().chain(q).cloned().collect();
        }
        if k > 0 && m[k - 1] == *first {
            return m[..k].iter().chain(&q[1..]).cloned().collect();
        }
    }
    if BASE_SUBMODULES.contains(&first.as_str()) {
        return std::iter::once("Base".to_string())
            .chain(q.iter().cloned())
            .collect();
    }
    q.to_vec()
}

/// Sets `d.extends` for a Julia method that extends another module's
/// function: a qualified head (`Base.show`, `Base.:+`, `BLAS.gemm!`) or a
/// name its module imported with `import`.
pub fn julia_extends(d: &mut Definition, scopes: &JuliaScopes) {
    if !matches!(d.kind.as_str(), "function_definition" | "assignment") || d.name.is_empty() {
        return;
    }
    if scopes
        .ambiguous_imports
        .contains(&(d.module.clone(), d.name.clone()))
    {
        d.qualified = true; // No deterministic owner for conflicting imports.
        return;
    }
    if d.qualified {
        // `Base.:+` / `Base.:(==)` / `LinearAlgebra.BLAS.gemm!`
        let (q, f) = match d.name.find(".:") {
            Some(i) => (&d.name[..i], &d.name[i + 2..]),
            None => match d.name.rsplit_once('.') {
                Some(x) => x,
                None => return,
            },
        };
        let f = f.trim_start_matches('(').trim_end_matches(')').trim();
        // `Base.$f(…)` in an `@eval` loop: the function is not known.
        if f.contains('$') || q.contains('$') {
            return;
        }
        let (dots, segs) = written_path(q);
        let mut path = locate(&d.module, dots, &segs, scopes);
        path.push(f.to_string());
        d.extends = Some(path);
    } else if let Some((dots, segs)) = scopes.imports.get(&(d.module.clone(), d.name.clone())) {
        let mut path = locate(&d.module, *dots, segs, scopes);
        path.push(d.name.clone());
        d.extends = Some(path);
    }
}

/// What a definition binds, as Sophia stores it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Binding {
    /// A method of the generic function (module path, name).
    Method(Vec<String>),
    /// A directly named definition (type, constant, macro, module item).
    Named(Vec<String>),
}

fn home(d: &Definition, pkg: &str) -> Vec<String> {
    if (d.language == "julia" || d.path_derived) && !d.module.is_empty() {
        d.module.clone()
    } else {
        std::iter::once(pkg.to_string())
            .chain(d.module.iter().cloned())
            .collect()
    }
}

/// The one grouping rule ([[Julia Name Resolution]]): a function is a
/// method of `extends` if set, else of (its module, its name); other
/// definitions are named (module, name). A qualified method that could not
/// be placed (`Base.$f`), and anything unnamed, binds nothing.
pub fn binding(d: &Definition, pkg: &str) -> Option<Binding> {
    if d.name.is_empty() {
        return None;
    }
    let function = matches!(
        d.kind.as_str(),
        "function_definition" | "assignment" | "function_item" | "function_signature_item"
    );
    let proposed = if function {
        if d.qualified && d.extends.is_none() {
            return None;
        }
        Some(Binding::Method(d.extends.clone().unwrap_or_else(|| {
            let mut k = home(d, pkg);
            k.push(d.name.clone());
            k
        })))
    } else if d.qualified {
        None
    } else {
        let mut k = home(d, pkg);
        k.push(d.name.clone());
        Some(Binding::Named(k))
    };
    proposed.filter(|b| {
        let key = match b {
            Binding::Method(k) | Binding::Named(k) => k,
        };
        !key.is_empty() && key.iter().all(|s| valid_segment(s))
    })
}

/// `jl` + `[LinearAlgebra, BLAS, gemm!]` → `jl.LinearAlgebra.BLAS.gemm!`.
pub fn entity_path(prefix: &str, key: &[String]) -> String {
    let mut s = prefix.to_string();
    for k in key {
        s.push('.');
        s.push_str(&segment(k));
    }
    s
}

/// Caller-supplied root context. Discover Cargo.toml / __init__.py / pyproject.toml
/// outside this crate. `module` is the module-tree path relative to that root
/// (including `#[path]` overrides); None derives it from `path` by convention.
#[derive(Debug, Clone, Copy)]
pub struct NamingContext<'a> {
    pub package: Option<&'a str>,
    pub path: &'a str,
    pub module: Option<&'a [String]>,
}

fn file_module(lang: &str, path: &str, known_root: bool) -> Vec<String> {
    let mut parts: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    if let Some(last) = parts.pop() {
        let stem = last.rsplit_once('.').map_or(last.as_str(), |(s, _)| s);
        if !known_root
            || !matches!(
                (lang, stem),
                ("rust", "lib" | "main" | "mod") | ("python", "__init__")
            )
        {
            parts.push(stem.to_string());
        }
    }
    if known_root && lang == "rust" && parts.first().is_some_and(|s| s == "src") {
        parts.remove(0);
    }
    parts
}

fn rust_path(mut n: Syntax, src: &str) -> Vec<String> {
    // Type arguments select methods of the same nominal type/trait.
    if n.kind() == "generic_type" {
        n = n.child_by_field_name("type").unwrap_or(n);
    }
    text(n, src)
        .split("::")
        .map(|s| s.trim().strip_prefix("r#").unwrap_or(s.trim()).to_string())
        .collect()
}

/// Extract naming metadata from a bounded parsed file. Julia scopes must first
/// be collected across all included files, using their absolute include bases.
/// `context.module` is also the Julia include base (module blocks are appended).
/// No references, encodings or identities are changed by this operation.
pub fn definitions(
    lang: &str,
    root: Syntax,
    src: &str,
    context: NamingContext<'_>,
    scopes: &JuliaScopes,
    budget: &mut Budget,
) -> Result<Vec<Definition>> {
    if src.len() > crate::MAX_FILE_BYTES
        || root.end_byte() > src.len()
        || !src.is_char_boundary(root.end_byte())
    {
        return Err(ContentIdError::LimitExceeded);
    }
    let units: HashSet<_> = if lang == "julia" {
        crate::julia::units(root).iter().map(|n| n.id()).collect()
    } else {
        HashSet::new()
    };
    let mut out = Vec::new();
    let mut stack = vec![(root, 0)];
    while let Some((n, depth)) = stack.pop() {
        budget.step()?;
        if depth > 128 {
            return Err(ContentIdError::LimitExceeded);
        }
        if crate::is_definition(lang, n) && (lang != "julia" || units.contains(&n.id())) {
            let (nn, qualified) = if lang == "julia" {
                crate::julia_name(n).map_or((None, true), |(x, q)| (Some(x), q))
            } else {
                (n.child_by_field_name("name"), false)
            };
            let mut module = context.module.map(<[String]>::to_vec).unwrap_or_else(|| {
                if lang == "julia" {
                    vec![]
                } else {
                    file_module(lang, context.path, context.package.is_some())
                }
            });
            if lang == "julia" {
                module.extend(modules_around(n, src));
            } else {
                let mut ancestors = Vec::new();
                let mut parent = n.parent();
                while let Some(p) = parent {
                    if (lang == "rust" && matches!(p.kind(), "mod_item" | "trait_item"))
                        || (lang == "python"
                            && matches!(p.kind(), "class_definition" | "function_definition"))
                    {
                        if let Some(name) = p.child_by_field_name("name") {
                            ancestors.push(if lang == "rust" {
                                rust_path(name, src)
                            } else {
                                vec![text(name, src).to_string()]
                            });
                        }
                    } else if lang == "rust" && p.kind() == "impl_item" {
                        let mut owner = Vec::new();
                        if let Some(ty) = p.child_by_field_name("type") {
                            owner.extend(rust_path(ty, src));
                        }
                        if let Some(tr) = p.child_by_field_name("trait") {
                            owner.push(rust_path(tr, src).join("::"));
                        }
                        ancestors.push(owner);
                    }
                    parent = p.parent();
                }
                for a in ancestors.into_iter().rev() {
                    module.extend(a);
                }
            }
            let mut d = Definition {
                language: lang.to_string(),
                name: nn
                    .map(|x| {
                        let name = text(x, src);
                        if lang == "rust" {
                            name.strip_prefix("r#").unwrap_or(name)
                        } else {
                            name
                        }
                        .to_string()
                    })
                    .unwrap_or_default(),
                kind: n.kind().to_string(),
                start: n.start_byte(),
                end: n.end_byte(),
                name_span: nn.map(|x| (x.start_byte(), x.end_byte())),
                qualified,
                module,
                extends: None,
                path_derived: context.package.is_none(),
                diagnostic: None,
            };
            if lang == "julia" {
                julia_extends(&mut d, scopes);
            }
            if binding(&d, context.package.unwrap_or("unknown")).is_none() {
                d.diagnostic = Some(
                    "unnamed, unresolved qualified name, or unspellable store path".to_string(),
                );
            }
            out.push(d);
            if out.len() > crate::MAX_DEFINITIONS {
                return Err(ContentIdError::LimitExceeded);
            }
        }
        let mut c = n.walk();
        let children: Vec<_> = n.named_children(&mut c).collect();
        stack.extend(children.into_iter().rev().map(|x| (x, depth + 1)));
    }
    Ok(out)
}

/// A single store name with all its method members (input indices).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingGroup {
    pub binding: Binding,
    pub members: Vec<usize>,
}

/// Group methods before writing names. Named collisions are refused explicitly;
/// Julia constructors coexist with the type under `<Type>.new`, as in Sophia.
/// Output paths are sorted, independent of file enumeration order. Member
/// indices refer to input order and are not persistent identities.
pub fn binding_groups(
    defs: &[Definition],
    package: &str,
) -> std::result::Result<Vec<BindingGroup>, Vec<String>> {
    use std::collections::BTreeMap;
    let mut groups: BTreeMap<Vec<String>, BindingGroup> = BTreeMap::new();
    let named: HashSet<Vec<String>> = defs
        .iter()
        .filter_map(|d| match binding(d, package) {
            Some(Binding::Named(k)) => Some(k),
            _ => None,
        })
        .collect();
    let mut conflicts = Vec::new();
    for (i, d) in defs.iter().enumerate() {
        let Some(mut b) = binding(d, package) else {
            continue;
        };
        if let Binding::Method(k) = &mut b {
            if d.language == "julia" && named.contains(k) {
                k.push("new".to_string());
            }
        }
        let key = match &b {
            Binding::Method(k) | Binding::Named(k) => k.clone(),
        };
        if let Some(g) = groups.get_mut(&key) {
            if matches!((&g.binding, &b), (Binding::Method(_), Binding::Method(_))) {
                g.members.push(i);
            } else {
                conflicts.push(entity_path("name", &key));
            }
        } else {
            groups.insert(
                key,
                BindingGroup {
                    binding: b,
                    members: vec![i],
                },
            );
        }
    }
    if conflicts.is_empty() {
        Ok(groups.into_values().collect())
    } else {
        conflicts.sort();
        conflicts.dedup();
        Err(conflicts)
    }
}
