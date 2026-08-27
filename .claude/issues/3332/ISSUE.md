# FNV-2026-08-26-D9-02

**Issue**: #3332
**Filed**: 2026-08-26 from `docs/audits/AUDIT_FNV_2026-08-26.md`

---

**Severity**: MEDIUM
**Dimension**: 9 — AI Packages & Procedures
**Status**: NEW
**Source**: `docs/audits/AUDIT_FNV_2026-08-26.md` (audit HEAD `d6e16c90`)


**File**: `byroredux/src/systems/escort.rs:82` (`ESCORT_COLLECT_DISTANCE`), `escort.rs:274-280` (the comparison) · `byroredux/src/npc_spawn/ai_package.rs:66-72,136-143` (`AmbientBehavior::Escort` carries no target-distance field) · `crates/plugin/src/esm/records/misc/pack.rs:920-940` (`PTDT` arm; no `PKE2` arm)

**Premise verified**: `AmbientBehavior::from_package` computes
`target_distance` from `PackTarget::count_or_distance`
(`ai_package.rs:111-115`) and hands it to **Follow only**
(`ai_package.rs:129-134`). The `is_escort()` branch
(`ai_package.rs:136-143`) constructs `Escort { target_form_id,
destination_form_id, destination_radius, actor_form_id }` — the parsed
distance is dropped on the floor. `escort_system` then compares against
the module constant `ESCORT_COLLECT_DISTANCE: f32 = 128.0`, self-described
as *"Engine default, same scale as `follow.rs::FOLLOW_DEFAULT_DISTANCE`"*.
`parse_pack` has arms for `PKDT`/`PKCU`/`PSDT`/`PLDT`/`PTDT`/`CTDA` only —
`PKE2` is not matched anywhere in the crate.

**Evidence** — all 12 FNV Escort (`procedure_type == 2`) packages, byte-dumped:
```
   form_id  EDID                                      PKE2  PTDT(type,fid,count/dist)  PLDT(type,fid,radius)
  0x1682af VFSOrrisIgnoredPackage                      600  (0, 0x14=PlayerRef, 256)   (0, 0x118, 0)
  0x1384b3 vNiptonLegionCaravanTravel02                200  (0, 0x134CB9,       0)     (0, 0x137, 0)
  0x1384b2 vNiptonLegionCaravanTravel                  200  (0, 0x164BD5,       0)     (0, 0x137, 0)
  0x1384a9 vNiptonLegionCaravanTravel04                200  (0, 0x134CB9,       0)     (0, 0x137, 0)
  0x1384a8 vNiptonLegionCaravanTravel03                200  (0, 0x134CB9,       0)     (0, 0x137, 0)
  0x11bee1 VFSOrrisLeadPlayerPackage                   600  (0, 0x14,         256)     (0, 0x11C, 0)
  0x118ab7 VFSOrrisDetourPackage                       500  (0, 0x14,         256)     (0, 0x118, 0)
  0x118ab2 VFSOrrisDetour03Package                     600  (0, 0x14,         256)     (0, 0x118, 0)
  0x1064aa NellisRaquelIntroEscortAIPackage            200  (0, 0x14,           0)     (1, 0x101, 0)
   0xc2f11 GomezEscortToAtrium                         300  (0, 0x14,         800)     (3, 0,   100)
   0x25c74 GomezEscortToEntrance                       300  (0, 0x14,           0)     (3, 0,   800)
   0x2ecc4 CG01DadLeaveRoom2                           200  (0, 0x14,           0)     (3, 0,     0)
```
Two independent authored signals are being discarded:
1. **`PTDT.count_or_distance`** — already parsed into
   `PackTarget::count_or_distance`, already consumed by Follow as a
   stand-off distance, non-zero on **5 of 12** Escort packages (256 ×4,
   800 ×1) and never read by Escort.
2. **`PKE2`** — a 4-byte sub-record present on **12 of 12** Escort
   packages and on **exactly zero** non-Escort packages anywhere in
   `FalloutNV.esm` (4163 records swept), always non-zero, values
   {200 ×6, 300 ×2, 500 ×1, 600 ×3}. Its Escort-exclusivity and
   distance-scale values are strong evidence it is the authored escort
   distance, but the *semantic name* is an inference from the corpus, not
   a spec read — confirm against xEdit's `wbDefinitions FalloutNV.pas`
   before wiring it, per the no-guessing policy.

**Impact**: with `BYRO_ESCORT=1`, every vanilla FNV escort collects at
128 units — 1.6× to 6.25× tighter than authored. Gomez's
`GomezEscortToAtrium` should consider the player collected at 800 units
(PTDT) and only begins leading at 128; the Nipton Legion caravan chain
(200) and the VFSOrris companion chain (500–600) are all similarly
over-tight. Concretely: the escorting NPC walks all the way into the
player's personal space before the lead phase starts, on 100 % of the
FNV Escort corpus. Escort is opt-in and the corpus is only 12 packages,
which caps the severity at MEDIUM.

**Fix sketch**: carry `target_distance` into `AmbientBehavior::Escort` and
`EscortBehavior` the same way Follow already does, and use
`.unwrap_or(ESCORT_COLLECT_DISTANCE)`; separately, add a `b"PKE2"` arm to
`parse_pack` once the field's meaning is confirmed against xEdit, and
prefer it over PTDT when both are present.

---

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
