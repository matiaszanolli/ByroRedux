# #1320 -- TD-D6: Test hygiene -- dump_prospector, translation_completeness, golden

_Snapshot as filed 2026-05-29 from /audit-publish (AUDIT_TECH_DEBT_2026-05-28)._

**Severity**: LOW | **Dim 6** — Test Hygiene
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-05-28.md` (TH6-NEW-01, TH6-NEW-02, TH6-NEW-03 bundled)
**Domain**: nif-parser | **Effort**: small

**TH6-NEW-01** — `dump_prospector_saloon_refrs` (`byroredux/tests/`) is `#[ignore]`d with a "requires FNV BSA" guard but contains **zero assertions** — it only prints and returns Ok. This test passes vacuously and provides no regression value. Either add assertions (e.g. minimum entity count, no parse errors) or delete it.

**TH6-NEW-02** — `cross_game_translation_completeness` (`crates/nif/tests/translation_completeness.rs`) is `#[ignore]`d for game data but also defers all fill-rate floor assertions — the closures that should assert `>= N%` material fill are currently commented out / set to 0. The test infrastructure is wired; the thresholds are empty. Add per-game fill-rate floors matching the ROADMAP compat matrix (e.g. FNV ≥ 80% metalness fill on BGSM content).

**TH6-NEW-03** — `cube_demo_60f.png` golden baseline in `byroredux/tests/golden_frames.rs` was captured against a shader state 76 commits ago (pre Disney-BSDF / pre water-caustics). The golden comparison will either fail or is skipped. Refresh the baseline against HEAD or retire the golden test and replace with a pixel-statistics check (no golden file dependency).

## Completeness Checks
- [ ] **SIBLING**: check other `#[ignore]`d tests in `byroredux/tests/` for zero-assertion pattern
- [ ] **TESTS**: these ARE test fixes; verify with `cargo test --ignored`
- [ ] **CANONICAL-BOUNDARY**: TH6-NEW-02 touches translation_completeness.rs — the fill-rate floors should reference material_translate's output, not re-derive
- [ ] **UNSAFE**: no unsafe
