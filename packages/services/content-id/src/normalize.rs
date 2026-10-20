use super::{field, Budget, ContentIdError, Part, Result, Syntax, NORMALIZER_VERSION};
use std::collections::{HashMap, HashSet};
use std::hash::Hash;

#[derive(Default)]
struct Frame {
    names: HashMap<String, u32>,
    next: u32,
}
struct Normalizer<'a, 'b, K> {
    lang: &'a str,
    text: &'a str,
    budget: &'b mut Budget,
    parts: Vec<Part<K>>,
    frames: Vec<Frame>,
    declarations: HashMap<usize, (u32, bool)>,
    references: &'a HashMap<(usize, usize), K>,
    /// The declaration's own name, by span: a wrapper with the same span
    /// (Julia's `type_head`) is the name too.
    own: Option<(usize, usize)>,
    member_names: HashSet<usize>,
    /// Julia soft scope: an assignment to a name local to an enclosing
    /// frame updates that variable instead of declaring a new one.
    soft: bool,
}

/// The canonical encoding of one definition. `syntax` is the definition's
/// node in `text`'s tree; `references` maps the byte span `(start, end)` of
/// each identifier that refers to another definition to that definition's
/// key. Spans not in the map are locals or unresolved names.
pub fn definition<K: Copy + Eq + Hash>(
    lang: &str,
    mut syntax: Syntax,
    text: &str,
    references: &HashMap<(usize, usize), K>,
    budget: &mut Budget,
) -> Result<Vec<Part<K>>> {
    if syntax.has_error() {
        return Err(ContentIdError::MalformedSyntax);
    }
    let own = if lang == "julia" {
        let mut head = syntax.named_child(0);
        let mut found = None;
        while let Some(node) = head {
            if matches!(
                node.kind(),
                "identifier" | "operator" | "macro_identifier" | "field_expression"
            ) {
                found = Some((node.start_byte(), node.end_byte()));
                break;
            }
            head = node.named_child(0);
        }
        found
    } else {
        syntax
            .child_by_field_name("name")
            .map(|n| (n.start_byte(), n.end_byte()))
    };
    // Decorators are part of Python meaning, outside the extractor's occurrence.
    if lang == "python"
        && syntax
            .parent()
            .is_some_and(|p| p.kind() == "decorated_definition")
    {
        syntax = syntax.parent().ok_or(ContentIdError::MalformedSyntax)?;
    }
    let mut n = Normalizer {
        lang,
        text,
        budget,
        parts: Vec::new(),
        frames: Vec::new(),
        declarations: HashMap::new(),
        references,
        own,
        member_names: HashSet::new(),
        soft: false,
    };
    n.bytes(b"language", lang.as_bytes());
    n.bytes(b"normalizer", &NORMALIZER_VERSION.to_le_bytes());
    if lang == "rust" {
        // Attributes are siblings in tree-sitter-rust, not children of the item.
        let mut attributes = Vec::new();
        let mut previous = syntax.prev_named_sibling();
        while let Some(node) = previous {
            if node.kind() == "attribute_item" {
                attributes.push(node);
            } else if !node.kind().contains("comment") {
                break;
            }
            previous = node.prev_named_sibling();
        }
        for attribute in attributes.into_iter().rev() {
            n.walk(attribute, 0)?;
        }
    }
    n.walk(syntax, 0)?;
    Ok(n.parts)
}

impl<K: Copy + Eq + Hash> Normalizer<'_, '_, K> {
    fn bytes(&mut self, tag: &[u8], value: &[u8]) {
        let mut bytes = Vec::new();
        field(&mut bytes, tag, value);
        self.budget.bytes += bytes.len();
        self.parts.push(Part::Bytes(bytes));
    }
    fn raw<'n>(&self, node: Syntax<'n>) -> Result<&str> {
        node.utf8_text(self.text.as_bytes())
            .map_err(|_| ContentIdError::MalformedSyntax)
    }
    fn bind(&mut self, node: Syntax, preserve: bool) -> Result<()> {
        self.budget.step()?;
        match node.kind() {
            "identifier" | "self" => {
                let name = self.raw(node)?.to_string();
                let outer = self.frames.len().saturating_sub(1);
                if self.soft
                    && self.frames[..outer]
                        .iter()
                        .any(|f| f.names.contains_key(&name))
                {
                    return Ok(());
                }
                let frame = self
                    .frames
                    .last_mut()
                    .ok_or(ContentIdError::UnsupportedSyntax)?;
                let slot = if let Some(slot) = frame.names.get(&name) {
                    *slot
                } else {
                    let slot = frame.next;
                    frame.next += 1;
                    frame.names.insert(name, slot);
                    slot
                };
                self.declarations.insert(node.id(), (slot, preserve));
            }
            "tuple_pattern"
            | "tuple"
            | "pattern_list"
            | "list_pattern"
            | "mut_pattern"
            | "ref_pattern"
            | "reference_pattern"
            | "typed_expression"
            | "typed_parameter"
            | "default_parameter"
            | "typed_default_parameter"
            | "list_splat_pattern"
            | "dictionary_splat_pattern"
            | "slurp_expression"
            | "splat_expression"
            | "open_tuple"
            | "tuple_expression" => {
                if matches!(
                    node.kind(),
                    "tuple_pattern"
                        | "tuple"
                        | "pattern_list"
                        | "list_pattern"
                        | "open_tuple"
                        | "tuple_expression"
                ) {
                    // Julia `(; a, b) = x` reads properties by name.
                    let by_name = node.kind() == "tuple_expression"
                        && node.child(1).is_some_and(|c| c.kind() == ";");
                    let mut cursor = node.walk();
                    for child in node.named_children(&mut cursor) {
                        self.bind(child, preserve || by_name)?;
                    }
                } else {
                    let pattern = node
                        .child_by_field_name("name")
                        .or_else(|| node.named_child(0))
                        .ok_or(ContentIdError::UnsupportedSyntax)?;
                    self.bind(
                        pattern,
                        preserve
                            && !matches!(
                                node.kind(),
                                "list_splat_pattern"
                                    | "dictionary_splat_pattern"
                                    | "splat_expression"
                            ),
                    )?;
                }
            }
            // Julia's anonymous `::T` parameter binds nothing.
            "_" | "unary_typed_expression" => {}
            _ => return Err(ContentIdError::UnsupportedSyntax),
        }
        Ok(())
    }
    fn parameters(&mut self, node: Syntax) -> Result<()> {
        let mut cursor = node.walk();
        let children: Vec<_> = node.named_children(&mut cursor).collect();
        let positional_end = children
            .iter()
            .position(|n| n.kind() == "positional_separator");
        for (i, child) in children.into_iter().enumerate() {
            match child.kind() {
                "parameter" => {
                    let pattern = child
                        .child_by_field_name("pattern")
                        .ok_or(ContentIdError::UnsupportedSyntax)?;
                    self.bind(pattern, false)?;
                }
                "self_parameter" => {
                    let mut c = child.walk();
                    let name = child
                        .named_children(&mut c)
                        .find(|n| n.kind() == "self")
                        .ok_or(ContentIdError::UnsupportedSyntax)?;
                    self.bind(name, false)?;
                }
                "keyword_separator" | "positional_separator" => {}
                _ => self.bind(
                    child,
                    self.lang == "python" && positional_end.is_none_or(|end| i > end),
                )?,
            }
        }
        Ok(())
    }
    /// Julia parameters: an argument list, a tuple or one parameter.
    /// Keyword names (after `;`) are observable.
    fn julia_parameters(&mut self, node: Syntax) -> Result<()> {
        if !matches!(node.kind(), "argument_list" | "tuple_expression") {
            return self.julia_parameter(node, false);
        }
        let mut keywords = false;
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            if child.kind() == ";" {
                keywords = true;
            } else if child.is_named() {
                self.julia_parameter(child, keywords)?;
            }
        }
        Ok(())
    }
    fn julia_parameter(&mut self, node: Syntax, keyword: bool) -> Result<()> {
        match node.kind() {
            "named_argument" => Err(ContentIdError::UnsupportedSyntax),
            // A default: `k = 1`, `x::Int = 1`.
            "assignment" => self.bind(
                node.named_child(0)
                    .ok_or(ContentIdError::UnsupportedSyntax)?,
                keyword,
            ),
            _ => self.bind(node, keyword),
        }
    }
    /// A Julia local scope inside a definition. What the scope evaluates
    /// outside itself (a `while` condition, a `for` iterable, a `let`
    /// right-hand side) is walked before its own names are bound, so a name
    /// read there never encodes as the scope's local.
    fn julia_scope(&mut self, node: Syntax, depth: usize) -> Result<()> {
        let kind = node.kind();
        self.bytes(b"open", kind.as_bytes());
        let mut cursor = node.walk();
        let mut children: Vec<_> = node.named_children(&mut cursor).collect();
        if kind == "while_statement" && !children.is_empty() {
            let condition = children.remove(0);
            self.walk(condition, depth + 1)?;
        }
        self.frames.push(Frame::default());
        let result = self.julia_scope_body(kind, children, depth);
        self.frames.pop();
        result?;
        self.bytes(b"close", kind.as_bytes());
        Ok(())
    }
    fn julia_scope_body(&mut self, kind: &str, children: Vec<Syntax>, depth: usize) -> Result<()> {
        // Comprehension and generator clauses bind before the body is read.
        let (clauses, mut rest): (Vec<_>, Vec<_>) = children
            .into_iter()
            .partition(|c| matches!(c.kind(), "for_clause" | "for_binding"));
        for clause in clauses {
            let bindings = if clause.kind() == "for_clause" {
                let mut cursor = clause.walk();
                clause.named_children(&mut cursor).collect()
            } else {
                vec![clause]
            };
            self.bytes(b"open", clause.kind().as_bytes());
            for binding in bindings {
                if binding.kind() != "for_binding" {
                    return Err(ContentIdError::UnsupportedSyntax);
                }
                self.let_binding(binding, depth + 1)?;
            }
            self.bytes(b"close", clause.kind().as_bytes());
        }
        let parameters = match kind {
            "arrow_function_expression" if rest.len() > 1 => 1,
            "do_clause" | "catch_clause" | "let_statement" => {
                rest.iter().filter(|c| c.kind() != "block").count()
            }
            _ => 0,
        };
        let body = rest.split_off(parameters);
        for parameter in rest {
            match kind {
                "let_statement" if parameter.kind() == "assignment" => {
                    self.let_binding(parameter, depth + 1)?
                }
                _ => {
                    self.julia_parameters(parameter)?;
                    self.walk(parameter, depth + 1)?;
                }
            }
        }
        for part in &body {
            if !julia_scope(*part) {
                self.collect_locals(*part)?;
            }
        }
        for part in body {
            self.walk(part, depth + 1)?;
        }
        Ok(())
    }
    /// `left in iterable` or `left = value`: the right side first, then the
    /// new binding.
    fn let_binding(&mut self, node: Syntax, depth: usize) -> Result<()> {
        let mut cursor = node.walk();
        let parts: Vec<_> = node.named_children(&mut cursor).collect();
        let [left, operator, right] = parts[..] else {
            return Err(ContentIdError::UnsupportedSyntax);
        };
        self.bytes(b"open", node.kind().as_bytes());
        self.walk(right, depth + 1)?;
        self.walk(operator, depth + 1)?;
        self.bind(left, false)?;
        self.walk(left, depth + 1)?;
        self.bytes(b"close", node.kind().as_bytes());
        Ok(())
    }
    fn collect_locals(&mut self, root: Syntax) -> Result<()> {
        self.soft = self.lang == "julia";
        let result = self.collect_locals_in(root);
        self.soft = false;
        result
    }
    fn collect_locals_in(&mut self, root: Syntax) -> Result<()> {
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            self.budget.step()?;
            if node.id() != root.id()
                && matches!(
                    node.kind(),
                    "function_definition" | "lambda" | "class_definition"
                )
            {
                return Err(ContentIdError::UnsupportedSyntax);
            }
            // A nested Julia scope collects its own names when walked.
            if self.lang == "julia" && node.id() != root.id() && julia_scope(node) {
                continue;
            }
            // `local x` declares in this scope even if an outer `x` exists.
            if node.kind() == "local_statement" {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    let name = if child.kind() == "assignment" {
                        child.named_child(0)
                    } else {
                        Some(child)
                    };
                    if let Some(name) = name {
                        self.soft = false;
                        self.bind(name, false)?;
                        self.soft = true;
                    }
                }
            }
            if node.kind() == "assignment" {
                if let Some(left) = node
                    .child_by_field_name("left")
                    .or_else(|| node.named_child(0))
                {
                    // Mutating fields/subscripts does not introduce a local name.
                    if !matches!(
                        left.kind(),
                        "attribute" | "field_expression" | "subscript" | "index_expression"
                    ) {
                        self.bind(left, false)?;
                    }
                }
            }
            let mut cursor = node.walk();
            stack.extend(
                node.named_children(&mut cursor)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev(),
            );
        }
        Ok(())
    }
    fn local(&self, name: &str) -> Option<(u32, u32)> {
        self.frames
            .iter()
            .rev()
            .enumerate()
            .find_map(|(depth, frame)| frame.names.get(name).map(|slot| (depth as u32, *slot)))
    }
    fn walk(&mut self, node: Syntax, depth: usize) -> Result<()> {
        self.budget.step()?;
        if depth > 128 {
            return Err(ContentIdError::LimitExceeded);
        }
        let kind = node.kind();
        if node.is_error() || node.is_missing() {
            return Err(ContentIdError::MalformedSyntax);
        }
        if kind.contains("comment") {
            return Ok(());
        }
        // Optional attribute containers must not make a doc-only edit visible.
        if kind == "attributes" {
            let mut cursor = node.walk();
            for child in node.named_children(&mut cursor) {
                self.walk(child, depth + 1)?;
            }
            return Ok(());
        }
        if matches!(kind, "attribute_item" | "attribute") {
            let compact: String = self.raw(node)?.split_whitespace().collect();
            if compact.starts_with("#[doc=")
                || compact.starts_with("#[doc(")
                || compact.starts_with("doc=")
                || compact.starts_with("doc(")
            {
                return Ok(());
            }
        }
        if self.lang == "julia" && julia_scope(node) {
            return self.julia_scope(node, depth);
        }
        if matches!(
            kind,
            "match_expression"
                | "closure_expression"
                | "lambda"
                | "lambda_expression"
                | "list_comprehension"
                | "set_comprehension"
                | "dictionary_comprehension"
                | "generator_expression"
                | "global_statement"
                | "nonlocal_statement"
                | "for_statement"
                | "for_expression"
                | "let_statement"
                | "catch_clause"
                | "except_clause"
                | "with_statement"
                | "let_condition"
                | "while_let_expression"
                | "macro_definition"
                | "arrow_function_expression"
                | "do_clause"
                | "comprehension_expression"
                | "for_clause"
                | "for_binding"
                | "match_statement"
                | "delete_statement"
                | "augmented_assignment"
                | "named_expression"
        ) {
            return Err(ContentIdError::UnsupportedSyntax);
        }
        if (self.lang == "rust"
            && kind == "let_declaration"
            && node.child_by_field_name("alternative").is_some())
            || (!self.frames.is_empty()
                && matches!(
                    kind,
                    "import_statement" | "import_from_statement" | "use_declaration"
                ))
        {
            return Err(ContentIdError::UnsupportedSyntax);
        }
        if self.own == Some((node.start_byte(), node.end_byte())) {
            self.bytes(b"declaration-name", &[]);
            return Ok(());
        }
        let mut scoped = false;
        let function = matches!(
            kind,
            "function_item" | "function_signature_item" | "function_definition"
        ) || (self.lang == "julia"
            && kind == "assignment"
            && node.named_child(0).is_some_and(|n| {
                matches!(
                    n.kind(),
                    "call_expression" | "where_expression" | "typed_expression"
                )
            }));
        if function {
            if self.lang == "julia" {
                let mut head = node.named_child(0);
                while let Some(n) = head {
                    if matches!(n.kind(), "identifier" | "operator" | "macro_identifier") {
                        self.member_names.insert(n.id());
                        break;
                    }
                    head = n.named_child(0);
                }
            }
            self.frames.push(Frame {
                names: HashMap::new(),
                next: 0,
            });
            scoped = true;
            if self.lang == "julia" {
                let mut head = node.named_child(0);
                while let Some(n) = head {
                    if n.kind() == "call_expression" {
                        let mut c = n.walk();
                        for arg in n.named_children(&mut c).skip(1) {
                            self.julia_parameters(arg)?;
                        }
                        break;
                    }
                    head = n.named_child(0);
                }
            } else if let Some(parameters) = node.child_by_field_name("parameters") {
                self.parameters(parameters)?;
            }
            if self.lang != "rust" {
                if self.lang == "julia" && kind == "assignment" {
                    // `f(x) = body`: the body is after the `=` operator.
                    if let Some(body) = node.named_child(2) {
                        self.collect_locals(body)?;
                    }
                } else if let Some(body) = node.child_by_field_name("body") {
                    self.collect_locals(body)?;
                } else if self.lang == "julia" {
                    let mut c = node.walk();
                    for child in node.named_children(&mut c).skip(1) {
                        self.collect_locals(child)?;
                    }
                }
            }
        } else if self.lang == "rust" && kind == "block" {
            self.frames.push(Frame {
                names: HashMap::new(),
                next: 0,
            });
            scoped = true;
        }
        // Julia attached docstrings are outside occurrences. Python docstrings
        // are the first string expression in function/class bodies only.
        if self.lang == "python"
            && matches!(
                kind,
                "expression_statement" | "string" | "concatenated_string"
            )
            && first_noncomment(node)
            && node.parent().is_some_and(|p| {
                p.kind() == "block"
                    && p.parent().is_some_and(|p| {
                        matches!(p.kind(), "function_definition" | "class_definition")
                    })
            })
            && (matches!(kind, "string" | "concatenated_string")
                || node
                    .named_child(0)
                    .is_some_and(|n| matches!(n.kind(), "string" | "concatenated_string")))
            && {
                let literal = if matches!(kind, "string" | "concatenated_string") {
                    Some(node)
                } else {
                    node.named_child(0)
                };
                literal
                    .and_then(|n| n.utf8_text(self.text.as_bytes()).ok())
                    .is_some_and(|raw| !raw.starts_with(['f', 'F', 'b', 'B']))
            }
        {
            return Ok(());
        }
        if self.lang == "rust" && kind == "let_declaration" {
            self.bytes(b"open", kind.as_bytes());
            // Initializer sees the previous binding, not the one being declared.
            if let Some(value) = node.child_by_field_name("value") {
                self.walk(value, depth + 1)?;
            }
            if let Some(pattern) = node.child_by_field_name("pattern") {
                // Rust `let x = ...; let x = ...` introduces a fresh slot.
                let mut names = Vec::new();
                let mut pending = vec![pattern];
                while let Some(p) = pending.pop() {
                    if p.kind() == "identifier" {
                        names.push(self.raw(p)?.to_string());
                    }
                    let mut c = p.walk();
                    pending.extend(p.named_children(&mut c));
                }
                if let Some(frame) = self.frames.last_mut() {
                    for name in names {
                        frame.names.remove(&name);
                    }
                }
                self.bind(pattern, false)?;
                self.walk(pattern, depth + 1)?;
            }
            let pattern = node.child_by_field_name("pattern").map(|n| n.id());
            let value = node.child_by_field_name("value").map(|n| n.id());
            let mut c = node.walk();
            for child in node.children(&mut c) {
                if Some(child.id()) != pattern && Some(child.id()) != value {
                    self.walk(child, depth + 1)?;
                }
            }
            self.bytes(b"close", kind.as_bytes());
        } else {
            self.encode(node, depth)?;
        }
        if scoped {
            self.frames.pop();
        }
        Ok(())
    }
    fn encode(&mut self, node: Syntax, depth: usize) -> Result<()> {
        let kind = node.kind();
        let raw = self.raw(node)?.to_string();
        if let Some((slot, preserve)) = self.declarations.get(&node.id()).copied() {
            self.bytes(b"binding", &slot.to_le_bytes());
            if preserve {
                self.bytes(b"observable", raw.as_bytes());
            }
            return Ok(());
        }
        let identifier = matches!(kind, "identifier" | "self");
        let observable = self.member_names.contains(&node.id())
            || node.parent().is_some_and(|p| {
                if matches!(
                    p.kind(),
                    "function_item"
                        | "function_signature_item"
                        | "function_definition"
                        | "class_definition"
                        | "struct_item"
                        | "enum_item"
                        | "type_item"
                ) && p
                    .child_by_field_name("name")
                    .is_some_and(|n| n.id() == node.id())
                {
                    return true;
                }
                if matches!(p.kind(), "keyword_argument" | "named_argument") {
                    p.child_by_field_name("name")
                        .or_else(|| p.named_child(0))
                        .is_some_and(|n| n.id() == node.id())
                } else {
                    matches!(p.kind(), "attribute" | "field_expression")
                        && p.named_child(0)
                            .is_some_and(|first| first.id() != node.id())
                }
            });
        if identifier && !observable {
            if let Some((scope, slot)) = self.local(&raw) {
                let mut bytes = scope.to_le_bytes().to_vec();
                bytes.extend_from_slice(&slot.to_le_bytes());
                self.bytes(b"local", &bytes);
                return Ok(());
            }
        }
        if let Some(target) = self
            .references
            .get(&(node.start_byte(), node.end_byte()))
            .copied()
        {
            if !observable {
                self.parts.push(Part::Reference(target));
                return Ok(());
            }
        }
        // Literals and macro token trees observe their interior spelling.
        if node.child_count() == 0
            || matches!(
                kind,
                "lifetime"
                    | "string_literal"
                    | "raw_string_literal"
                    | "string"
                    | "character_literal"
                    | "char_literal"
                    | "token_tree"
                    | "quote_expression"
            )
        {
            self.bytes(b"token-kind", kind.as_bytes());
            self.bytes(
                if identifier && !observable {
                    b"unresolved"
                } else {
                    b"token"
                },
                raw.as_bytes(),
            );
            return Ok(());
        }
        self.bytes(b"open", kind.as_bytes());
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let outer_expression = self.lang == "python"
                && ((matches!(
                    kind,
                    "default_parameter" | "typed_parameter" | "typed_default_parameter"
                ) && ["value", "type"].into_iter().any(|field| {
                    node.child_by_field_name(field)
                        .is_some_and(|n| n.id() == child.id())
                })) || (kind == "function_definition"
                    && node
                        .child_by_field_name("return_type")
                        .is_some_and(|n| n.id() == child.id())));
            if outer_expression {
                let frame = self.frames.pop();
                self.walk(child, depth + 1)?;
                if let Some(frame) = frame {
                    self.frames.push(frame);
                }
            } else {
                self.walk(child, depth + 1)?;
            }
        }
        self.bytes(b"close", kind.as_bytes());
        Ok(())
    }
}

/// Julia's local scopes below a definition: loops, closures, `do`, `let`,
/// `try`/`catch`/`finally` and comprehensions.
fn julia_scope(node: Syntax) -> bool {
    matches!(
        node.kind(),
        "for_statement"
            | "while_statement"
            | "arrow_function_expression"
            | "do_clause"
            | "let_statement"
            | "catch_clause"
            | "finally_clause"
            | "comprehension_expression"
            | "generator"
    ) || (node.kind() == "block" && node.parent().is_some_and(|p| p.kind() == "try_statement"))
}

fn first_noncomment(node: Syntax) -> bool {
    let mut previous = node.prev_named_sibling();
    while let Some(p) = previous {
        if !p.kind().contains("comment") {
            return false;
        }
        previous = p.prev_named_sibling();
    }
    true
}
