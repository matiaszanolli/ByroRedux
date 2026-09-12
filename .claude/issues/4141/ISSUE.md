### SAVE-D2-2026-09-11-02: `save_type_sources()`'s discovery roots are a stale hardcoded 5-crate list, not the dynamic scan the sibling completeness guard was rewritten to use after proving the static-list approach misses real crates

- **Severity**: MEDIUM
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: Latent — a future `#[serde(default)]` or silent field-shape change on a save-participating type registered from outside the 5 scanned roots would bypass both SAVE-D2 shape/serde-default guards entirely, reproducing exactly the silent-corruption class those guards exist to prevent.
- **Location**: `byroredux/src/save_io/serde_default_guard_tests.rs:45-79` (`save_type_sources()`) vs. `byroredux/src/save_io/registry_completeness_tests.rs:51-84` (`discover_scan_roots()`)
- **Status**: NEW
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: `save_type_sources()` still uses five hardcoded roots (`byroredux/src`, `crates/core/src`, `crates/plugin/src`, `crates/scripting/src`, `crates/physics/src`) plus four explicit non-turbofish edges. This covers every currently-registered type today, but it is the exact shape of guard `registry_completeness_tests.rs` no longer trusts for itself: `#3497` replaced that file's own hardcoded `SCAN_ROOTS` with `discover_scan_roots()` — a live enumeration of every `crates/*/src` directory — specifically because the static list "has no defense against a root never being added," and doing so immediately surfaced four real types the old list had missed. `serde_default_guard_tests.rs`'s `save_type_sources()` was never given the same treatment. `crates/sdk` in particular (per this repo's `CLAUDE.md`: no owner audit skill) is a plausible future home for a save-participating type; if one lands there or in any other unscanned crate, both shape/serde-default guards will silently never scan it.

**Evidence**: Root-list comparison confirmed directly during publish — `save_type_sources()` hardcodes exactly the 5 roots named above at `serde_default_guard_tests.rs:50-56`; `registry_completeness_tests.rs:470` comment directly documenting the prior static-list miss.

**Impact**: No live corruption today (nothing currently registered lives outside the 5 roots), but the guard-coverage gap is structural and will not fail loudly when the next save-participating type lands in an unscanned crate.

**Related**: `#3497` (the sibling fix this guard was never given).

**Suggested Fix**: Extract `discover_scan_roots()` into a shared location both test modules call, so `save_type_sources()` scans every workspace crate the same way.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — e.g. a variant of `discover_scan_roots_finds_every_workspace_crate_and_byroredux` run against `save_type_sources()`'s output
- [ ] **SIBLING**: Confirm no third copy of the same hardcoded-roots pattern exists elsewhere in `save_io/*_tests.rs`
