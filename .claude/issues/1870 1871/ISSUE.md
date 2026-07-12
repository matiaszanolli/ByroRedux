# #1870: OBL-D1-NOTE-01 — audit-oblivion SKILL.md #1509 checklist bullet has the doghead.nif bsver-9 gate backwards

**Severity**: low
**Location**: `.claude/commands/audit-oblivion/SKILL.md` lines 110-113 (Dimension 1, `#1509` checklist bullet); code is
correct at `crates/nif/src/blocks/controller/morph.rs:89-92`.

## Description
The skill checklist read: "`doghead.nif` is v10.2.0.0 bsver 9 and must
**keep** the field" — backwards. The `#1509` fix gates the trailing field
on `bsver > 9`; doghead is bsver 9, so `9 > 9` is false and the field is
correctly **skipped**. The field is *kept* for Oblivion's bsver-11 morph
rigs. The code, its inline comment, and the regression test all agree the
gate is correct; only the audit skill's checklist prose was wrong.

## Suggested Fix
Amend to: "`doghead.nif` is v10.2.0.0 bsver 9 and must **skip** the
trailing field; Oblivion's bsver-11 morph rigs must **keep** it — an
off-by-band gate either direction truncates/misaligns `NiMorphData`."

---

# #1871: LC0703-02 — terrain_lod.rs hole-mask has the same radius_load/radius_unload hysteresis gap as #1866

**Severity**: medium
**Location**: `byroredux/src/cell_loader/terrain_lod.rs::block_hole_mask` (~line 190) and its caller `stream_lod_blocks`.

## Description
`compute_streaming_deltas` unloads a full cell only past `radius_unload`
(`radius_load + 1`, a one-cell hysteresis band). `block_hole_mask` holed
out LOD terrain for a cell only when `chebyshev(cell, player) <=
full_radius_load`; a cell at exactly `radius_load + 1` was NOT holed out,
so the coarse LOD terrain rendered there too while the full-detail terrain
for that same cell could still be resident under hysteresis. Same overlap
mechanism as #1866 (object/placement LOD), applied to terrain. Both call
sites (`app_step.rs`, `scene/world_setup.rs`) passed `state.radius_load`
to `stream_lod_blocks`, not `state.radius_unload`.

## Suggested Fix
Same shape as the #1866 fix: rename the `full_radius_load` parameter to
make the "must be radius_unload" contract explicit, and change both call
sites to pass `state.radius_unload`. Add a regression test on
`block_hole_mask` (or an extracted pure helper) pinning that a cell at
exactly `radius_load + 1` is holed out when gated on `radius_unload`.
