# FO3-D2-02: nif_stats per-block histogram keys by parsed Rust type, not header-advertised type — doc claims the opposite

Filed from: `docs/audits/AUDIT_FO3_2026-08-03.md`
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2323

**Severity**: MEDIUM
**Location**: `crates/nif/examples/nif_stats.rs:33-41` (doc claim) vs `:176-184` (impl); consumed by `crates/nif/tests/per_block_baselines.rs` + `crates/nif/tests/data/per_block_baselines/fallout_3.tsv`
**Status**: NEW

### Description
The doc says blocks attribute to "header-advertised type name, not parsed Rust type"; the success path actually keys by `block.block_type_name()` (the Rust struct's name, set at the `impl_ni_object!` macro invocation site — a per-Rust-struct constant string, not the on-disk header string). Every FO3 block family whose dispatch arm returns a shared struct collapses into one histogram row: `BSSegmentedTriShape`/`NiTriStrips` both fold into `NiTriShape`; `BSFadeNode` folds into `NiNode`; five distinct `NiPSysEmitter` subtypes fold into one row; ~24 particle modifier/controller/collider types fold into opaque `NiPSysBlock`; `Lighting30ShaderProperty` folds into `BSShaderPPLightingProperty`.

Confirmed against current code: `nif_stats.rs:33-41` doc claims header-advertised attribution; the impl (`:176-184`) does `self.block_histogram.entry(block.block_type_name().to_string())` for the success path (only the `NiUnknown` recovery-path branch uses `unknown.type_name`, the genuinely header-advertised name). `block_type_name()` is defined per-struct by the `impl_ni_object!` macro (`crates/nif/src/blocks/mod.rs:111-126`) using either `stringify!(TypeName)` or an explicit literal — either way a fixed per-Rust-type string, confirming the doc/impl mismatch.

### Impact
A composition shift inside a collapsed family is invisible to the FO3 baseline regression gate — e.g. a change that silently reroutes `BSSegmentedTriShape` to the plain `NiTriShape` arm (dropping its trailer) keeps the aggregate row byte-identical and passes `per_block_baselines`. Bounded blind spot: total dispatch loss still surfaces as an `unknown` row — this only masks "still parses, parses *differently*", which is precisely the FO3-vs-FNV divergence class this audit was asked to hunt.

### Suggested Fix
Attribute by the header-advertised type name already in scope at the dispatch site (or minimally emit `original_type` for the `NiPSysBlock`/`NiPSysEmitter` families); regenerate all seven baseline TSVs in the same commit; correct the module doc.

### Related
#2216 (different harness)

## Completeness Checks
- [ ] **SIBLING**: Same collapsed-family blind spot applies to all other per-game baseline TSVs (FNV, Oblivion, Skyrim, FO4, FO76, Starfield) — regenerate all seven together, not just FO3's
- [ ] **TESTS**: `per_block_baselines.rs` extended (or a new assertion) confirming header-advertised attribution distinguishes `BSSegmentedTriShape` from plain `NiTriShape`, etc.
