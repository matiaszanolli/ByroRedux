# CONC-D4-2026-08-30-03: `player_controller_system`'s doc points its access declaration at `main.rs`, which has held none since #1858/#1670

**Issue**: #3654
**Labels**: documentation, low, concurrency, doc-rot
**Filed**: 2026-08-30
**Source report**: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md`

---

Source: `docs/audits/AUDIT_CONCURRENCY_2026-08-30.md` — CONC-D4-2026-08-30-03 (LOW, D4 · Scheduler Access Declarations — doc rot).

**Location**: `byroredux/src/systems/character.rs:76-78`.

## Description

The M27-Phase-3 merge comment on `player_controller_system` — **the system whose whole reason for existing is that its declaration is the union of `fly_camera_system` + `character_controller_system`** — sends a maintainer to the wrong file to find or amend that union.

## Evidence

```rust
// byroredux/src/systems/character.rs:76-78
/// Access (declared at registration in `byroredux/src/main.rs`) is the
/// union of the two inner systems' accesses. The `PlayerMode` read
/// here is itself part of that union.
```

Verification (grep): `byroredux/src/main.rs` contains **no** `Access::new()`. `build_scheduler` — all 19 declarations — lives in `boot.rs:706-1523`; `main.rs:472/505` only calls `boot::build_scheduler()` and `boot::install_runtime_registries`.

## Impact

A maintainer widening `character_controller_system`'s or `fly_camera_system`'s access surface is pointed at a file with no declarations in it. The most likely outcome is a **silently incomplete union on the engine's only Early parallel pair** — which is precisely what makes `known_conflict_count() == 0` unsound (the #2676 / #2389 failure mode).

## Related

#1858 / #1670 (`main.rs` -> `boot.rs` split); #2676; #2389.

## Suggested Fix

Change the path to `byroredux/src/boot.rs` (`build_scheduler`).

## Completeness Checks
- [ ] **SIBLING**: Every other doc comment in `byroredux/src/systems/` that cites `main.rs` for registration/access re-pointed at `boot.rs` — the #1858/#1670 split is 20+ months of accumulated citations
- [ ] **TESTS**: Note that `_audit-validate.sh` passes here because `main.rs` still exists — a *wrong but live* path is invisible to the gate (cf. #3439)
