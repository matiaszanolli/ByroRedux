# TD8-2026-10-15-06: `import_nif`'s doc says it is "used by cell loader"; the flat-import family has zero production callers

Labels: low,nif-parser,tech-debt,bug

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD8-2026-10-15-06`).

- **Severity**: LOW
- **Dimension**: 8 - Dead Code & Backwards-Compat Cruft
- **Location**: `crates/nif/src/import/mod.rs:409-416` (`import_nif`, doc "Backward-compatible flat import (used by cell loader …)"), `:420` (`import_nif_with_resolver`), `:584` (`import_nif_with_collision`, no resolver), `import_nif_impl` `:428`
- **Status**: NEW
- **Age**: `import_nif` predates the scene-graph importer; verified callers are `byroredux/examples/{dump_nif,msn_basis_probe}.rs`, `crates/nif/tests/translation_completeness.rs`, `byroredux/src/fog.rs` (test module, `:2450+`) and `import/tests/units.rs`, `cell_loader/finish_partial_tests.rs`
- **Effort**: trivial
- **Description**: production code (cell loader, streaming) calls `import_nif_scene*` and `import_nif_with_collision_and_resolver` (`cell_loader/references/import.rs:88`, `streaming/pre_parse.rs:341`); `import_nif`, `import_nif_with_resolver` and `import_nif_with_collision` are reached only from examples, corpus tests and `#[cfg(test)]` code. The doc claim "used by cell loader where hierarchy is unnecessary" is false, and `import_nif_texture_effects` (`:551`) is documented as a deliberately dead extractor (`docs/engine/nifal.md:839`) so it is correctly NOT flagged.
- **Evidence**: `git grep -nw import_nif -- '*.rs'` filtered to non-test, non-example paths -> nothing.
- **Impact**: misleads a reader into believing the flat path is a live production route (it feeds none of the NIFAL-translated data the scene importer does).
- **Related**: TD8-2026-10-12-03 (same "public API reachable only by tests" class).
- **Suggested Fix**: fix the doc to "harness/example entry point; production uses `import_nif_scene*`"; fold `import_nif_with_collision` into the `_and_resolver` form with `None`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
- [ ] **TESTS**: A regression test or guard pins this specific fix (or the existing guard is named)
