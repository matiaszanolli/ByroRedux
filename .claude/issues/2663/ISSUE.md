# #2663: SCR-D7-NEW11-02: World-placement base-record family (DOOR/FURN/FLOR/MSTT/TACT/LIGH/STAT/IDLM/ADDN/BNDS) and TERM decode VMAD as a presence-only flag, so base_record_script_instance can never resolve them

**Severity**: MEDIUM
**Dimension**: Engine Attach & Trigger Wiring (root cause in `crates/plugin`)
**Untrusted-Input**: No
**Location**: `crates/plugin/src/esm/cell/support.rs:74` (`b"VMAD" => has_script = true,`); `crates/plugin/src/esm/records/dispatch_world_placement.rs:25-27`; `crates/plugin/src/esm/records/misc/world.rs:383` (`parse_term` discards `CommonNamedFields::script_instance`); `crates/plugin/src/esm/records/index.rs:605-629` (`base_record_script_instance`)
**Status**: NEW

## Description

This is the exact sibling of the closed #2189. `CommonItemFields` was taught to decode `VMAD` and an `items` arm was added to `base_record_script_instance` -- but the *other* two record populations that reach cell load through a different parser were not.

1. The MODL-only world-placement family (STAT/MSTT/FURN/DOOR/LIGH/FLOR/IDLM/BNDS/ADDN/TACT) is parsed by `parse_modl_group` -> `build_static_object_from_subs`, whose `VMAD` arm sets a boolean and **drops the payload**. `StaticObject` has nowhere to store a `ScriptInstanceData`, and `EsmIndex` has no typed map for `base_record_script_instance` to consult.
2. `TERM` *is* parsed through `CommonNamedFields` (which decodes `VMAD` fully), but `parse_term` copies only `editor_id` / `full_name` / `model_path` / `script_form_id` out of it and throws the decoded `script_instance` away -- and `base_record_script_instance` has no `terminals` arm either.

`parse_term`'s own justifying comment (`world.rs`, near `:385`: *"TERM is FO3/FNV-only, so the helper's VMAD arm never fires here"*) is **factually wrong** -- FO4 ships 207 VMAD-bearing `TERM` records.

## Evidence

Corpus census (temporary instrument, run then deleted):

- `Skyrim.esm`: `FURN` **34/400**, `DOOR` **5/244**, `FLOR` **3/86** -> **42** unreachable base records (samples: `GenPullChainAnim01NoPlayer`, `CartFurniturePassenger`, `TrapTriggerHinge`, `RiftenRWDoorJail01PRISONER`, `dunSleepingTreeCampSpigot`).
- `Fallout4.esm`: `FURN` **157/598**, `TERM` **207/778**, `MSTT` **36/961**, `FLOR` **18/53**, `DOOR` **17/371**, `LIGH` **3/801**, `TACT` **3/43**, `STAT` **1/19368** -> **442** unreachable (samples: `WorkshopBar03Counter`, `VRWorkshopShared_VRTerminalMusicSubMenu`, `DN136_KlaxonLight01NBDest`, `GreenHsPlanter01Mutfruit`, `LoadElevatorDoorHiTech_MinUse`).

`crates/plugin/src/esm/cell/support.rs:74` is a one-line `has_script = true` with no `ScriptInstanceData::parse` call -- mirroring the pre-#2189 shape of `CommonItemFields::from_subs`.

## Impact

Silent decline (no corrupted state) for a measured **42 Skyrim / 442 FO4** scripted base records -- a **larger** population than the item family #2189 was filed for.

A scripted crafting station, planter, workshop bar, jail door, elevator door, or FO4 terminal attaches nothing and contributes nothing to the `M47.2 scripts:` counter.

Partial mitigation that masks it: a REFR carrying its *own* `VMAD` still attaches through `refr_script_instance`, so only base-record-level scripts on these types are lost.

## Related

#2189 (closed; the item-family half of the same omission); SCR-D7-NEW4-01 in `docs/audits/AUDIT_SCRIPTING_2026-07-25.md`; SCR-D7-NEW11-01

## Suggested Fix

Add `script_instance: Option<ScriptInstanceData>` to `StaticObject`, populate it in `build_static_object_from_subs`'s `VMAD` arm the way `CommonNamedFields` does, and add a `self.cells.statics.get(base_form_id).and_then(|s| s.script_instance.as_ref())` arm at the **end** of `base_record_script_instance` so the typed maps keep priority.

Separately add `script_instance` to `TermRecord`, wire it from `common.script_instance`, add a `terminals` arm, and delete the incorrect "TERM is FO3/FNV-only" comment.

Pin with a test mirroring `base_record_script_instance_resolves_an_item_records_vmad`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other primitives, other parsers, other spawn paths)
- [ ] **TESTS**: A regression test pins this specific fix

---
*Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-12.md` (eleventh scripting-domain pass, 7 dimension agents).*
