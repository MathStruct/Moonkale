# moonkale-content-id

WP-66 syntactic content ids for Rust, Julia and Python definitions: the per-language normal form and the hashing of a definition graph by recursive group. It is a small crate, depending only on arborium (tree-sitter) and blake3, so that Sophia can compute the same ids as Moonkale (SR-116 in Sophia's vault). The rules are in [Code Graph Schema](../../../markdown/architecture/Code%20Graph%20Schema.md), "WP-66 normalizer".

## API

- `parse(lang, text, &mut Budget) -> Tree`: bounded parse, with the per-file limits (512 KiB, 4,096 definitions, depth 128, two-second parser deadline).
- `definition(lang, syntax, text, &references, &mut Budget) -> Vec<Part<K>>`: one definition's canonical encoding. `references` maps the byte span `(start, end)` of every identifier that refers to another definition to that definition's key `K` (any `Copy + Eq + Hash`).
- `identities(&units, &mut Budget) -> HashMap<K, Identity>`: the ids of a closed set of definitions. Each referenced key must be in `units`. Recursive groups hash together, and the result doesn't depend on the map's order.
- `Identity { digest, normalizer_version, component_digest, component_index, component_size }`. Moonkale's node id is `NodeId::from_content(&digest)`.

One `Budget` covers a whole request; any error means no identity at all.

## Julia modules (v5)

For Julia, the crate also resolves references, per **module** (SPEC-008 in Sophia's vault). A package is one module over its `include`d files, so `dist` in `ops.jl` depends on `Point` in `types.jl`.

- `julia::includes(text, tree) -> Vec<Include>`: a file's `include` calls, with the literal path, or `None` for a dynamic include, and the `module` blocks around each call. The caller reads the files, depth first in source order. Package discovery is the caller's job too: the nearest `Project.toml` with `name = "X"` and `src/X.jl`.
- `julia::module(&[ModuleFile], dynamic) -> JuliaModule`: for every unit, its identity or error, the references its encoding uses, and the files its id depends on, keyed by `(file, start_byte, end_byte)`. It also returns `calls_generic`, the uses of names with several methods that the encoding reads as module names (a local wins). They are drawn as edges and never part of an id.

Rules:
- **Units** are definitions whose only ancestors are `module` blocks; a module itself is a namespace. A macro call's arguments count (`@inline f(x) = …`, `@static if … end`), but nothing inside a call does: there an `x::T = v` is a keyword argument, as in `@deprecate f(A; check::Bool = true) g(A; check)`.
- **Bindings:** a unit binds its name (`julia_name`: the callee of its head call, or its type head) in its module path, unless the name is qualified (`Base.show(…) = …`), the head isn't a name (`(::Type{T})(x)`, `$S(x)`), or it is a macro. Submodules are separate scopes.
- **What resolves:** an identifier resolves only to the **unique** binding of its spelling in its module, and only if no `using`/`import` there mentions that name. In a module with a dynamic include, functions stay unresolved (an unseen file could add methods); types and constants still resolve.
- **Not references:** fields (`x.name`), struct field names, keyword and named-tuple labels, and identifiers inside strings, symbols, quotes and `using`/`import`/`export`. Locals are left to the normal form: v4 scoping wins over a proposed reference.
- **Refusal propagates:** a unit whose encoding refers to a refused unit is refused too (`DependencyRefused`).
- **Limits** (normative, since over them there are no ids): 1024 files, 32 MiB, and 1,000,000 + 64 steps per source byte for the whole module.

On LinearAlgebra (Julia 1.13, v7), 3416 of 3552 units get ids. That is 27 unsupported, 6 malformed and 103 refused through a dependency. There are 2469 references (948 across files) and 2237 generic uses, computed in 1.3 s (release build). Measure a package with `MOONKALE_JULIA_PACKAGE=<src/Pkg.jl> cargo test --release -p moonkale-content-id --test corpus -- --nocapture`.

## What the crate doesn't do

**Reference resolution for Rust and Python.** The crate doesn't decide which identifier refers to which definition there. Moonkale's index decides that with its symbol extractors (`packages/services/index/src/extract/references.rs`): same-file, conservative, by spelling and scope, with possible shadows left unresolved. Two implementations get the same ids only if they resolve references the same way.

## Golden fixtures

`golden/` has fixtures in all three languages, with recursion, a struct used as a parameter type and a constructor, and a renamed copy of a method. `scopes.jl` covers the v4 Julia forms:

- `ids.tsv`: Moonkale's digest, component index and component size for every definition;
- `references.tsv`: the reference spans Moonkale resolved, for Rust and Python;
- `module.tsv`: Julia in module mode, for `module/Geo.jl`, `module/Dyn.jl` (a dynamic include), `module/Callable.jl` (v6: type parameters, callable and interpolated heads, a local named like a generic function), `module/Macros.jl` (v7: definitions in macro calls, keyword arguments in calls) and the single-file modules `geometry.jl` and `scopes.jl`. It lists ids, depends_on, references by span and generic uses. `module/` is Sophia's fixture from SPEC-008, at Sophia eb549c6.

So another implementation can check its normal form, by feeding Moonkale's spans, separately from its resolver. The test that keeps them current is `golden_fixtures_keep_their_ids` in `packages/services/index/tests/content_ids.rs`. `MOONKALE_BLESS=1` rewrites both files, and a change to either needs a `NORMALIZER_VERSION` bump.

## Versions

- 1 (2026-10-16): first version.
- 7 (2026-10-20), found by Sophia's check against Julia's method table: nothing inside a Julia call is a module-level unit. In v6 a keyword default in a top-level call, such as `check::Bool = true` in `@deprecate cholesky!(A, ::Val{false}; check::Bool = true) …`, looked like a short-form definition `check::Bool = …` and got an id. On LinearAlgebra 19 such units go away (3416 of 3552 get ids); references and generic uses are unchanged.
- 2 (2026-10-20): Julia and Python parameter types and defaults are uses, not local names. In v1, `f(p::Point)` made `Point` look like a local, so the method didn't depend on the struct.
- 3 (2026-10-20): a node with the declaration name's span is the name. Julia's `type_head` had put a self-reference into every `struct`'s id. A Rust struct expression's type (`Point { x }`) is a use. Both were found by Sophia's cross-check.
- 6 (2026-10-20), from Sophia's use of `calls_generic`:
  - Julia type parameters (`where {T, S <: X}`, `struct Box{T}`) are locals, so renaming one keeps the id and it never resolves to a module name.
  - A definition's name is the callee of its head call (`julia_name`), so a callable-type method `(::Type{T})(x) = …` or an interpolated `$S(x) = …` names nothing.
  - `calls_generic` lists only the uses the encoding reads as module names (`definition_and_uses`): a parameter called like a generic function is a local.

  On LinearAlgebra: 3435 of 3571 units get ids, with 2469 references (948 across files) and 2237 generic uses.
- 5 (2026-10-20): Julia references resolve per module (above). Moonkale's index finds the package and reads every file of the module, and its cache key covers them all.
- 4 (2026-10-20): Julia local scopes: `for`, `while`, closures (`x -> …`), `do`, `let`, `try`/`catch`/`finally`, comprehensions and generators; varargs (`A...`), keyword parameters (their names are observable), destructuring (`m, n = …`, and `(; a, b) = …` by name) and anonymous `::T` parameters. Each scope has its own frame. What a scope reads from outside (a `while` condition, a `for` iterable, a `let` right-hand side) is encoded before the scope's names are bound. An assignment to a name that is local in an enclosing frame updates that variable, as Julia's soft scope does; `local` declares a new one. On LinearAlgebra (Julia 1.13) 3538 of 3571 definitions get ids (Sophia measured about 69% under v3); the rest are macro definitions, nested named functions and some macro calls. `tests/corpus.rs` measures a tree: `MOONKALE_JULIA_CORPUS=<dir>`.
