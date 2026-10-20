# moonkale-content-id

WP-66 syntactic content ids for Rust, Julia and Python definitions: the per-language normal form and the hashing of a definition graph by recursive group. It is a small crate, depending only on arborium (tree-sitter) and blake3, so that Sophia can compute the same ids as Moonkale (SR-116 in Sophia's vault). The rules are in [Code Graph Schema](../../../markdown/architecture/Code%20Graph%20Schema.md), "WP-66 normalizer".

## API

- `parse(lang, text, &mut Budget) -> Tree`: bounded parse, with the per-file limits (512 KiB, 4,096 definitions, depth 128, two-second parser deadline).
- `definition(lang, syntax, text, &references, &mut Budget) -> Vec<Part<K>>`: one definition's canonical encoding. `references` maps the byte span `(start, end)` of every identifier that refers to another definition to that definition's key `K` (any `Copy + Eq + Hash`).
- `identities(&units, &mut Budget) -> HashMap<K, Identity>`: the ids of a closed set of definitions. Each referenced key must be in `units`. Recursive groups hash together, and the result doesn't depend on the map's order.
- `Identity { digest, normalizer_version, component_digest, component_index, component_size }`. Moonkale's node id is `NodeId::from_content(&digest)`.

One `Budget` covers a whole request; any error means no identity at all.

## What the crate doesn't do

**Reference resolution.** The crate doesn't decide which identifier refers to which definition. Moonkale's index decides that with its symbol extractors (`packages/services/index/src/extract/references.rs`): same-file, conservative, by spelling and scope, with possible shadows left unresolved. Two implementations get the same ids only if they resolve references the same way.

## Golden fixtures

`golden/` has fixtures in all three languages, with recursion, a struct used as a parameter type and a constructor, and a renamed copy of a method. `scopes.jl` covers the v4 Julia forms:

- `ids.tsv`: Moonkale's digest, component index and component size for every definition;
- `references.tsv`: the reference spans Moonkale resolved.

So another implementation can check its normal form, by feeding Moonkale's spans, separately from its resolver. The test that keeps them current is `golden_fixtures_keep_their_ids` in `packages/services/index/tests/content_ids.rs`. `MOONKALE_BLESS=1` rewrites both files, and a change to either needs a `NORMALIZER_VERSION` bump.

## Versions

- 1 (2026-10-16): first version.
- 2 (2026-10-20): Julia and Python parameter types and defaults are uses, not local names. In v1, `f(p::Point)` made `Point` look like a local, so the method didn't depend on the struct.
- 3 (2026-10-20): a node with the declaration name's span is the name. Julia's `type_head` had put a self-reference into every `struct`'s id. A Rust struct expression's type (`Point { x }`) is a use. Both were found by Sophia's cross-check.
- 4 (2026-10-20): Julia local scopes: `for`, `while`, closures (`x -> …`), `do`, `let`, `try`/`catch`/`finally`, comprehensions and generators; varargs (`A...`), keyword parameters (their names are observable), destructuring (`m, n = …`, and `(; a, b) = …` by name) and anonymous `::T` parameters. Each scope has its own frame. What a scope reads from outside (a `while` condition, a `for` iterable, a `let` right-hand side) is encoded before the scope's names are bound. An assignment to a name that is local in an enclosing frame updates that variable, as Julia's soft scope does; `local` declares a new one. On LinearAlgebra (Julia 1.13) 3538 of 3571 definitions get ids (Sophia measured about 69% under v3); the rest are macro definitions, nested named functions and some macro calls. `tests/corpus.rs` measures a tree: `MOONKALE_JULIA_CORPUS=<dir>`.
