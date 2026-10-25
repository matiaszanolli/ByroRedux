# TD7-2026-10-25-01: `VatsModel::ap_regen_per_second` hard-codes the 60 Hz tick that `POOL_REGEN_DT` already names

Labels: low, character, game:fnv, game:fo3, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6389
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Severity**: LOW · **Dimension**: 7 · **Effort**: trivial · **Age**: 3dc16208e 2026-10-25
- **Location**: `crates/core/src/character/vats.rs:292` `self.ap.restore_rate * 60.0`; the constant is `crates/core/src/character/regen.rs:45` `pub const POOL_REGEN_DT: f32 = 1.0 / 60.0;` (documented "Fixed regen tick — 60 Hz"). The FNV struct doc (vats.rs:211) also restates "0.06 x 60 Hz = 3.6/s".
- **What**: The per-frame -> per-second conversion is the inverse of the tick the regen system consumes the result at. If the fixed tick is ever retuned, ActionPoints regen silently desyncs from Fatigue regen (same `PoolRegenConfig`, same `POOL_REGEN_DT`).
- **Fix**: `self.ap.restore_rate / POOL_REGEN_DT` (import from `super::regen`).

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD7-2026-10-25-01`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
