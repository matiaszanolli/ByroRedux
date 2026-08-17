# REG-2026-08-16-D5-03: #2567's Oblivion creature-asset corpus guard is not #[ignore]d

**Issue**: #3084
**Severity**: LOW
**Labels**: `low,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_REGRESSION_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_REGRESSION_2026-08-16.md` (Dimension 5 — Green-by-construction guard sweep).

**Location**: `byroredux/src/npc_spawn/tests.rs`:773-876 (`installed_oblivion_creature_assets_resolve_from_their_records`)

## Description

#2567's Oblivion creature-asset corpus guard is **the only data-dependent corpus test in the tree that is not `#[ignore]`d**.

It reads `Oblivion.esm` and every `*meshes.bsa` under the game directory, then asserts ≥90% skeleton and NIFZ-part resolution. **Its assertions are real and correct** — the problem is the skip path.

## Evidence

Every other data-dependent corpus sweep in the workspace is `#[ignore]`d:
`parse_real_nifs.rs`, `per_block_baselines.rs`, `block_coverage_baselines.rs`, `skinning_e2e.rs`, `crates/audio/src/tests.rs`, `crates/scripting/tests/pex_recognize_e2e.rs`.

This one is not, so on any machine without Oblivion installed it prints a skip line to stderr and is **counted as `ok` by `cargo test`**. Its own docstring calls this "self-skips".

Re-verified 2026-08-17 at `byroredux/src/npc_spawn/tests.rs`:764-780.

## Impact

A default `cargo test` on a machine without Oblivion reports this guard green when it did not run. That is indistinguishable from the guard passing — the same skip-reads-as-pass shape as #3003 (smoke gates) and #3014 (`crates/hkx`).

The guard itself is good; only its skip semantics are wrong.

## Suggested Fix

Add `#[ignore]` to match the house pattern for data-dependent corpus tests, so a skipped run is visibly skipped rather than counted as a pass. Document the `--ignored` invocation alongside the others.

## Related

- #3003 (RT-04), #3014 (SCR-D8-04) — the same skip-reads-as-pass shape
- #2567 (CLOSED — the fix this guards)

## Completeness Checks
- [ ] **HOUSE-PATTERN**: `#[ignore]` matches the six sibling corpus tests
- [ ] **SKIP≠PASS**: A machine without Oblivion no longer reports this green
- [ ] **INVOCATION**: The `--ignored` run is documented as for the siblings
- [ ] **STILL-PASSES**: The guard still passes when run with Oblivion data present

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3084 --json state` when live state is needed.*
