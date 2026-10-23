# Changelog

## Unreleased — 2026-10-23

- Add optional bounded `egraph::EGraph` snapshot projection and browser
  `set_egraph`, `collapse_eclass`, `eclass_state`, `rewrite_state` methods.
- `InGroup` gains optional JSON `kind` and `representative` fields. Existing JSON
  remains compatible; Rust struct literals must supply these new fields.
