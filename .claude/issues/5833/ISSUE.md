# TD3-2026-10-15-09: ROADMAP "Growth discipline — Tripwire today" is off by 2.5x on two rows, names a deleted file, and omits four files over the 3 500-line line

Labels: low,tech-debt,documentation,doc-rot

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-10-15.md` (finding `TD3-2026-10-15-09`).

- **Severity**: LOW
- **Dimension**: 3 - Stale Documentation & Comments
- **Location**: `ROADMAP.md:443-450`
- **Status**: NEW
- **Age**: measured 2026-10-10; `c140cb48e` (#5584) deleted `byroredux/src/env_translate.rs` (now `env_translate/{mod,climate,fallback,lod,sky_weather,water}.rs`); the M47.2 native-call arms grew `translate/effects.rs`.
- **Effort**: trivial
- **Description**: at HEAD (inline tests included, dedicated test files excluded, the doc's own rule): `crates/scripting/src/translate/effects.rs` 10 117 (doc: 4 022), `crates/physics/src/world/mod.rs` 5 209 (doc: 4 149), `byroredux/src/inventory.rs` 4 202 (4 075). Missing: `crates/scripting/src/fragment/vm.rs` 6 215, `crates/physics/src/water.rs` 3 963, `crates/scripting/src/condition.rs` 3 691, `crates/plugin/src/esm/records/misc/world.rs` 3 508. `env_translate.rs` no longer exists.
- **Evidence**: `find crates byroredux tools -name '*.rs' -not -path 'tools/nifskope/*' -not -path '*/tests/*' -not -name '*_tests.rs' -not -name 'tests.rs' -exec wc -l {} + | awk '$1>3500'`.
- **Impact**: ROADMAP:443 is the only place the 3 500 "investigate before adding" threshold is operationalised, and its list is the list `/audit-tech-debt` Dim 1 is expected to reconcile. `ROADMAP.md` is at 798 lines against the 800 cap (#5111), so the fix must be net-zero lines.
- **Related**: #5111, #5584, Dim 1 report (effects.rs / vm.rs).
- **Suggested Fix**: rewrite the list to the nine files above with 2026-10-15 numbers (or remove the numbers and keep the command).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files/docs
