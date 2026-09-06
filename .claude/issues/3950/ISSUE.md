# #3950 — SCR-D6-2026-09-06-03: `OnEquipEvent` was deleted but four ground-truth doc lines still describe it as defined/shipped

- **Finding ID**: SCR-D6-2026-09-06-03
- **Labels**: low,scripting,documentation,doc-rot
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3950

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Scripting Runtime Systems · **Untrusted-Input**: No · **Location**: `docs/engine/scripting.md:145`; `docs/engine/m47-0-design.md:103, 162`; `docs/engine/m47-2-design.md:312, 371` · **Status**: NEW
- **Description**: `events.rs:189-210` replaced it with `EquipmentChange` + `EquipmentEventBatch` (wearer-keyed batch, `get_mut`-then-`extend`); the docs the skill names as ground truth still list `OnEquipEvent { wearer }` as shipped.

- **Suggested Fix**: replace the four `OnEquipEvent` lines with `EquipmentEventBatch` / `EquipmentChange` (wearer-keyed batch, `get_mut`-then-`extend`) and name the two emit sites (`crates/scripting/src/equipment.rs:51`, `byroredux/src/inventory.rs:519`).

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
