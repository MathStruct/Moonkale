//! Julia's local scopes, varargs, keywords and destructuring (normalizer v4).
//! A rename of a local keeps the id; anything that changes which variable a
//! name reads changes it.
use moonkale_content_id::{definition, identities, is_definition, parse, Budget, ContentIdError};
use std::collections::HashMap;

/// The id of the file's first definition, which references nothing else.
fn id(text: &str) -> Result<[u8; 32], ContentIdError> {
    let mut budget = Budget::new();
    let tree = parse("julia", text, &mut budget)?;
    let mut cursor = tree.root_node().walk();
    let node = tree
        .root_node()
        .named_children(&mut cursor)
        .find(|n| is_definition("julia", *n))
        .expect("a definition");
    let parts = definition("julia", node, text, &HashMap::<_, u8>::new(), &mut budget)?;
    Ok(identities(&HashMap::from([(0u8, parts)]), &mut budget)?[&0].digest)
}

fn same(a: &str, b: &str) {
    assert_eq!(id(a).unwrap(), id(b).unwrap(), "\n{a}\n---\n{b}");
}
fn differ(a: &str, b: &str) {
    assert_ne!(id(a).unwrap(), id(b).unwrap(), "\n{a}\n---\n{b}");
}

#[test]
fn the_forms_real_julia_uses_get_ids() {
    for text in [
        "function f(xs)\n    s = 0\n    for (i, x) in enumerate(xs), j in 1:i\n        s += x * j\n    end\n    s\nend",
        "function f(n)\n    while n > 0\n        n -= 1\n    end\n    n\nend",
        "f(A...) = g(A...)",
        "_f(ok, A, B...) = ok",
        "function f(A)\n    m, n = size(A)\n    (; band, index) = A\n    m + n + band + index\nend",
        "f(A, ::Type{T}) where {T} = T",
        "f(xs) = map(x -> x + 1, xs)",
        "f(xs) = map((a, b) -> a + b, xs)",
        "function f(xs)\n    map(xs) do x\n        y = x\n        y\n    end\nend",
        "f(xs) = [x * y for x in xs, y in xs if x > 0]",
        "f(xs) = sum(x for x in xs)",
        "function f(x)\n    let y = x, z\n        y\n    end\nend",
        "function f(x)\n    try\n        x\n    catch e\n        e\n    finally\n        x\n    end\nend",
        "f(x; k=1, kw...) = x + k",
        "function f(x)\n    local y = x\n    y\nend",
    ] {
        id(text).unwrap_or_else(|e| panic!("{text}: {e}"));
    }
}

#[test]
fn renaming_locals_keeps_the_id() {
    same(
        "function f(xs)\n    s = 0\n    for x in xs\n        s += x\n    end\n    s\nend",
        "function f(ys)\n    t = 0\n    for y in ys\n        t += y\n    end\n    t\nend",
    );
    same("f(xs) = map(x -> x + 1, xs)", "f(v) = map(e -> e + 1, v)");
    same("f(A...) = g(A...)", "f(B...) = g(B...)");
    same(
        "function f(A)\n    m, n = size(A)\n    m\nend",
        "function f(B)\n    p, q = size(B)\n    p\nend",
    );
    same("f(xs) = [x for x in xs]", "f(v) = [e for e in v]");
    same(
        "function f(x)\n    try\n        x\n    catch e\n        e\n    end\nend",
        "function f(x)\n    try\n        x\n    catch err\n        err\n    end\nend",
    );
}

#[test]
fn observable_names_are_part_of_the_id() {
    // Keyword names are the API; property destructuring reads by name.
    differ("f(x; k=1) = x + k", "f(x; j=1) = x + j");
    differ(
        "function f(p)\n    (; a) = p\n    a\nend",
        "function f(p)\n    (; b) = p\n    b\nend",
    );
    // Positional and by-name destructuring differ.
    differ(
        "function f(p)\n    (a, b) = p\n    a\nend",
        "function f(p)\n    (; a, b) = p\n    a\nend",
    );
}

#[test]
fn what_a_scope_reads_outside_itself_is_not_its_local() {
    // `x` and `y` are globals in the condition: different programs.
    differ(
        "function f()\n    while x < 3\n        x = 1\n    end\nend",
        "function f()\n    while y < 3\n        y = 1\n    end\nend",
    );
    // `let x = x` reads the outer `x`.
    differ(
        "function f()\n    let x = x\n        x\n    end\nend",
        "function f()\n    let y = y\n        y\n    end\nend",
    );
    // The iterable is evaluated before the loop variable exists.
    differ(
        "function f()\n    for x in x\n        x\n    end\nend",
        "function f()\n    for y in y\n        y\n    end\nend",
    );
}

#[test]
fn assignments_in_a_scope_update_outer_locals() {
    // The closure assigns the function's `x`, not a fresh local.
    differ(
        "function f()\n    x = 1\n    g = () -> (x = 2)\n    g()\n    x\nend",
        "function f()\n    x = 1\n    g = () -> (z = 2)\n    g()\n    x\nend",
    );
    // A loop accumulates into the function's `s`; `local` declares a new one.
    differ(
        "function f(xs)\n    s = 0\n    for x in xs\n        s = x\n    end\n    s\nend",
        "function f(xs)\n    s = 0\n    for x in xs\n        local s = x\n    end\n    s\nend",
    );
}

#[test]
fn type_parameters_are_locals() {
    same("f(x::T) where T = x", "f(y::U) where U = y");
    same(
        "f(x::T, y::S) where {T, S<:Real} = x",
        "f(a::A, b::B) where {A, B<:Real} = a",
    );
    same(
        "struct Box{T}\n    value::T\nend",
        "struct Box{U}\n    value::U\nend",
    );
    // The bound is a use: a different bound is a different method.
    differ(
        "f(x::T) where {T<:Real} = x",
        "f(x::T) where {T<:Integer} = x",
    );
}
