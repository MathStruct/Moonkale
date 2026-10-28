//! Admitted import syntax. Unsupported forms deliberately produce no binding.
use arborium_tree_sitter::Node;

#[derive(Clone)]
pub(super) struct Import {
    pub local: String,
    pub target: String,
    pub statement: String,
    pub module: bool,
    pub scope: (usize, usize),
}

fn path(s: &str, separator: &str) -> bool {
    !s.is_empty()
        && s.split(separator)
            .all(|part| !part.is_empty() && part.chars().all(|c| c.is_alphanumeric() || c == '_'))
}

pub(super) fn collect(root: Node, text: &str, lang: &str) -> Vec<Import> {
    let mut out = Vec::new();
    let mut pending = vec![root];
    while let Some(n) = pending.pop() {
        let mut cursor = n.walk();
        pending.extend(n.named_children(&mut cursor));
        if !matches!(
            n.kind(),
            "use_declaration" | "import_statement" | "import_from_statement" | "using_statement"
        ) || n.has_error()
        {
            continue;
        }
        let Ok(statement) = n.utf8_text(text.as_bytes()) else {
            continue;
        };
        let mut scope = (root.start_byte(), root.end_byte());
        let mut straight = super::unattributed(n);
        let mut parent = n.parent();
        while let Some(p) = parent {
            if matches!(p.kind(), "module_definition" | "mod_item") {
                scope = (p.start_byte(), p.end_byte());
                break;
            }
            if !matches!(
                p.kind(),
                "source_file" | "module" | "block" | "declaration_list"
            ) {
                straight = false;
            }
            parent = p.parent();
        }
        let s = statement.trim().trim_end_matches(';');
        let binding = if let Some(rest) = s.strip_prefix("use ") {
            let rest = rest.trim();
            path(rest, "::").then(|| Import {
                local: rest.rsplit("::").next().unwrap().into(),
                target: rest.into(),
                statement: statement.into(),
                module: false,
                scope: (0, 0),
            })
        } else if let Some(rest) = s.strip_prefix("from ") {
            rest.split_once(" import ").and_then(|(module, name)| {
                (path(module, ".") && path(name, ".") && !name.contains('.')).then(|| Import {
                    local: name.into(),
                    target: format!("{module}.{name}"),
                    statement: statement.into(),
                    module: false,
                    scope: (0, 0),
                })
            })
        } else if let Some(rest) = s
            .strip_prefix("using ")
            .or_else(|| s.strip_prefix("import "))
        {
            if let Some((module, name)) = rest.split_once(':') {
                let name = name.trim();
                (path(module.trim(), ".") && path(name, ".") && !name.contains('.')).then(|| {
                    Import {
                        local: name.into(),
                        target: format!("{}.{name}", module.trim()),
                        statement: statement.into(),
                        module: false,
                        scope: (0, 0),
                    }
                })
            } else {
                (path(rest, ".") && (lang != "python" || !rest.contains('.'))).then(|| Import {
                    local: rest.rsplit('.').next().unwrap().into(),
                    target: rest.into(),
                    statement: statement.into(),
                    module: !rest.contains('.'),
                    scope: (0, 0),
                })
            }
        } else {
            None
        };
        if let Some(mut binding) = binding.filter(|_| straight) {
            binding.scope = scope;
            out.push(binding);
        } else {
            // A wildcard, alias, list or unsupported form can rebind names.
            // Refuse this file's import environment rather than overlooking it.
            out.push(Import {
                local: String::new(),
                target: String::new(),
                statement: statement.into(),
                module: false,
                scope,
            });
        }
    }
    out
}

pub(super) fn target(imports: &[&Import], name: &str, lang: &str) -> Option<(String, String)> {
    if imports.iter().any(|i| i.local.is_empty()) {
        return None;
    }
    let sep = if lang == "rust" { "::" } else { "." };
    let (head, tail) = name
        .split_once(sep)
        .map_or((name, None), |(a, b)| (a, Some(b)));
    let bindings: Vec<_> = imports.iter().filter(|i| i.local == head).collect();
    if bindings.len() != 1 {
        return None;
    }
    let i = bindings[0];
    match (i.module, tail) {
        (false, None) => Some((i.target.clone(), i.statement.clone())),
        (true, Some(tail)) if path(tail, sep) && !tail.contains(sep) => {
            Some((format!("{}{sep}{tail}", i.target), i.statement.clone()))
        }
        _ => None,
    }
}
