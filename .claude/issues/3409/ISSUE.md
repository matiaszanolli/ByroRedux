# SKY-2026-08-27b-D3-02: the pre-baked FaceGen head is spawned with no displacement mask, so hair renders through every helmet — 587 hair-slot ARMOs, 1,208 of 5,118 NPCs

- **Severity**: HIGH
- **Dimension**: 3 (NPC equip + FaceGen)
- **Location**: `byroredux/src/npc_spawn/resumable.rs:1087-1097` (the `pre_spawn = None` argument at `:1096`), against the working machinery at `byroredux/src/npc_spawn/resumable.rs:1111-1118` (the armour phase) and `byroredux/src/npc_spawn.rs:965` (the only `hidden_biped_mask` producer)
- **Confidence**: CONFIRMED — code path read end-to-end; both the partition data and the displacing ARMO population measured on shipped bytes.

## Description

The Skyrim+ equip chain hides displaced skin by handing `load_nif_bytes_with_skeleton` a `pre_spawn` hook that calls `ImportedMesh::hide_skin_partitions`:

```rust
// byroredux/src/npc_spawn/resumable.rs:1111-1118  (PrebakedPhase::Armor)
let hidden_biped_mask = armor.hidden_biped_mask;
let mut hide_displaced_skin = |scene: &mut byroredux_nif::import::ImportedScene| {
    hide_skin_partitions(scene, hidden_biped_mask);
};
let pre_spawn: Option<&mut dyn FnMut(&mut byroredux_nif::import::ImportedScene)> =
    (hidden_biped_mask != 0).then_some(&mut hide_displaced_skin);
```

The FaceGen phase, immediately above it, passes `None`:

```rust
// byroredux/src/npc_spawn/resumable.rs:1087-1097  (PrebakedPhase::Facegen)
let (_, root, _) = load_nif_bytes_with_skeleton(
    world, ctx, &data, facegen_path, tex_provider, mat_provider,
    Some(&state.skel_map),
    tint_path,
    None,                      // <- no pre_spawn hook
);
```

And `hidden_biped_mask` is only ever set for the *race skin*'s entries (`byroredux/src/npc_spawn.rs:965`), which resolve to torso/hands/feet ARMAs — the FaceGen head is never enrolled in `EquipmentSlots` and never receives a mask. The head NIF is nonetheless a multi-region mesh source in exactly the way the race skin is.

## Evidence

The shipped pre-baked heads carry per-triangle dismember partitions for the head *family*, not just the head. Sweeping 400 of the 3,158 `meshes\actors\character\facegendata\facegeom\…\*.nif` in `Skyrim - Meshes0.bsa` through `import_nif_scene`:

```
files=400 meshes=2603 no_skin=0 no_bp=1230
triangle body-part histogram:
  [(130, 651867), (131, 354146), (141, 140340), (230, 66408), (143, 25084),
   (30, 13068), (1, 12612), (0, 2060), (132, 1008), (41, 602), (31, 594)]
meshes by dominant body-part: [("bp130", 646), ("bp131", 370), ("bp141", 340), …]
```

`131` / `141` / `143` are hair, long hair and ears; `dismember_body_part_to_biped_bit` (`crates/nif/src/import/types.rs:1102-1109`) maps them to biped bits 1, 11 and 13, i.e. exactly the bits Skyrim helmets and hoods claim. On real `Skyrim.esm`:

```
ARMO total=7264  head-family (bits 0/1/11/12/13)=702  hair-bit(1)=587
NPC_ total=5118  equipping head-family armour (1 LVLI hop)=1208
```

`hide_skin_partitions` itself is verified working on this data — hiding the Body bit on the vanilla character-asset corpus removes triangles on 27 of 524 meshes and is a correct no-op on the rest, and the skin meshes carry clean single-region partitions (`femalebody_1.nif` → `{32: 688}` and `{32: 2212, 34: 40, 38: 160}`; `femalehands_1.nif` → `{33: 1448}`; `malefeet_1.nif` → `{37: 316}`).

## Impact

Roughly one Skyrim NPC in four renders their hair, long hair and ears intersecting the helmet, hood or circlet they are wearing — the classic "hair through the helmet" artifact, on guards, bandits, soldiers and Draugr alike. It is a pure wiring gap: the mask, the biped→partition mapping and the pre-spawn hook all exist and all work.

## Suggested Fix

Give the FaceGen head an inventory entry + `EquipmentSlots` claim over the head-family bits its own partitions cover, so the existing occupancy filter and `displaced_mask` fold treat it exactly like the race skin, then pass the resulting `hidden_biped_mask` through the same `pre_spawn` hook the armour phase uses. Guard with a real-data test on a helmeted vanilla NPC (any `EncDraugr*` or Whiterun guard).

## Related

#2094 (the displacement-mask mechanism), #2093 (the race-skin layer), #3357 (which fixed the mask reaching *every* skin mesh but not this one). Distinct from the concurrently-filed `slot_role.rs` slot-2 FaceGen finding, which is about the head's *texture roles*, not its geometry.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix

---

*Filed from `docs/audits/AUDIT_SKYRIM_2026-08-27b.md` (`/audit-skyrim`).*
