# Investigation — #1337 Full v10.0.1.0 NIF format support

## Scope correction (vs. the filed issue)
The follow-up issue estimated the v10.0.1.0 gap spanned particles / morphs /
animation across ~63 Oblivion files. **That estimate was wrong** — it counted
*all* truncating Oblivion files, but NiPSysBoxEmitter (23), NiControllerSequence
(13), NiMorphData (8) etc. are **v20.0.0.5** Oblivion files with unrelated bugs,
not v10.0.x format.

Filtering to actual v10.0.x files (`version ∈ [10.0.0.0, 10.1.0.0)`): **64 files**
(41 at 10.0.1.0, 23 at 10.0.1.2), of which only **7 truncated** after #1329 —
across **4 block types**. Fixing those revealed a short downstream chain. Final:
**all 64 parse clean (0 truncated).**

## Approach — "through the NIFAL" (user directive)
Per the GameVariant doctrine (`format_abstraction.md` / NIFAL), version branching
is expressed as **named version-aware helpers on `NifVersion`** instead of scattered
raw `version < V10_1_0_0` literals. `NifVariant::detect` routes both v10.0.x and
v20.0.0.5 to the same `Oblivion` variant, so these live on `NifVersion` (the per-file
version), not the variant. #1329's raw checks + `starts_with("bhk")` heuristic were
retrofitted to the helpers.

New `NifVersion` helpers: `has_object_group_id`, `has_mopp_offset`,
`has_havok_strips_scale`, `has_skin_data_partition_ref`, `uses_old_rigid_body_layout`,
`has_keyframe_controller_data`. Plus the `is_havok_serializable(type_name)` block-class
predicate in `blocks/mod.rs`.

## The v10.0.1.x deltas fixed (each cross-checked vs nif.xml + openmw, byte-verified)
1. **groupID over-broad gate (regression from #1329)** — `bhkCollisionObject` &
   family (`*CollisionObject`) descend from `NiObject` (nifly `bhk.hpp`), NOT
   `bhkRefObject`, so they DO carry the v10.0.x groupID. `is_havok_serializable`
   now excludes `*CollisionObject`. (Fixed the 3 NiTriStrips meshes + a
   NiBinaryExtraData file — all cascade victims of the misaligned collision object.)
2. **bhkNiTriStripsShape.Scale** — `since="10.1.0.0"`, was read unconditionally.
3. **NiSkinData.Skin Partition** — a `Ref` carried inline `until="10.1.0.0"`, was
   not read (the 4-byte ref was misparsed as `has_vertex_weights`).
4. **NiSkinPartition `Has Bone Indices`** — nif.xml gives it NO version gate (unlike
   the `since=10.1.0.0` vertex-map/weights/faces flags). #174 wrongly gated it on
   `has_conditionals`, skipping the byte on v10.0.x. Now unconditional (matches openmw).
5. **BSKeyframeController** — fell through to the NiTimeController base-only stub
   (relied on block_size recovery, absent on sizeless Oblivion). New dedicated parser:
   base + interpolator (since 10.1.0.104) + `Data` ref (until 10.1.0.103) + `Data 2`.

(#1329's `has_mopp_offset` / `uses_old_rigid_body_layout` raw checks also retrofitted.)

## Verification
- `trace_block` byte-exact on every previously-truncating file: handscythe01 47/47,
  oar01 17/17, ungrdltraphingedoor 31/31, stonepedastellarge01 19/19, minotaurold 206/206.
- Oblivion Meshes sweep: clean **7969 → 7976 (+7)**, truncated **63 → 56** (exactly
  −7, **zero new**). FNV 99.82% / Skyrim 99.97% unaffected.
- All gates are `version`-keyed to the v10.0.x band → no other title can change;
  FO3+ also has block_size recovery as a safety net for the BSKeyframeController change.
- 749 existing + 3 new nif tests pass.

## Sources
nif.xml (`bhkNiTriStripsShape` 3173, `NiSkinData` 5067, `SkinPartition` 2143,
`NiKeyframeController`/`NiSingleInterpController`/`BSKeyframeController` 3646-4325),
openmw `components/nif/{physics,data}.cpp`, nifly `include/bhk.hpp` (collision-object
hierarchy) + `BasicTypes.hpp:972` (groupID).
