# FNV-2026-10-18-01: `saved_type_shape_changes_require_format_major_bump` is red on `main`

- **Issue**: #6050 (https://github.com/matiaszanolli/ByroRedux/issues/6050)
- **Labels**: high, save-load, bug, game:fnv, legacy-compat
- **Source**: docs/audits/AUDIT_FNV_2026-10-18.md (snapshot as filed; GitHub is authoritative for state)

Found by the FNV compatibility audit of 2026-10-18 (`docs/audits/AUDIT_FNV_2026-10-18.md`, HEAD `af5063597`).

- **Severity**: HIGH per `_audit-severity.md` ("Save: serialized-shape change without a `FORMAT_MAJOR` bump / baseline refresh"). Dimension 1 rated the same defect MEDIUM because it is not a save-compat break; I kept the table's rating and record the dispute here.
- **Origin**: D1-01 + D5-03 (merged). **Dimension**: save state around the cell round trip. **Owner**: `/audit-save`.
- **Location**: `byroredux/src/save_io/serde_default_guard_tests.rs:1112` / `:1230` (`BASELINE_SHAPE_FINGERPRINT = 0x244d_dcf0_0b26_0495`, `BASELINE_MAJOR = 111`); trigger `317320437` and `6f44874ac` (`byroredux/src/save_io/registry.rs`: `byroredux_scripting::challenges::register_saved`, `ChallengeState` at `crates/scripting/src/challenges.rs:340`, plus the obscript-ledger column).
- **Status**: NEW. Related but different: #5459 (bump ledger in `snapshot.rs` stops at v32).
- **Description**: The baseline fingerprint was last refreshed in `4b99e2393`. The two later commits registered new saved columns without touching the guard or `snapshot.rs`.
- **Evidence**: `cargo test --release -p byroredux --bin byroredux`: `saved serialized type shape changed without updating FORMAT_MAJOR/baseline; actual=0x6195e31318a87d92`.
- **Impact**: `main` is red, and the guard is blind to any later shape drift until someone refreshes the constant. Mitigating: `restore_resources` leaves a live resource untouched when its column is absent from an older snapshot (`crates/save/src/driver.rs` ~200), so an old save should load; real-save impact was not measured. FNV-visible because the challenge system is New Vegas-only.
- **Suggested Fix**: Decide bump-or-refresh per the guard file's ledger (an additive new resource may only need a deliberate fingerprint refresh, #4612), and make that change in the commit that adds the type.

**Related**: #5459 (bump ledger in `snapshot.rs` stops at v32). Owner audit: `/audit-save`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the other FO3 / Oblivion / Skyrim paths that share this code
- [ ] **TESTS**: A regression test (or a repaired gate) pins this specific fix

