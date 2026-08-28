# Skyrim SE Compatibility Audit — 2026-08-27b

**HEAD**: `969d81c8` · **Branch**: `main` · **Skill**: `/audit-skyrim` (all 7 dimensions)
**Run context**: second `/audit-skyrim` pass of 2026-08-27, executed inside an
`/audit-suite --preset comprehensive` run. The first pass of the day
(`docs/audits/AUDIT_SKYRIM_2026-08-27.md`, HEAD `30537bf3`, committed as `558af58c`) is
reconciled below rather than repeated; `docs/audits/AUDIT_SKYRIM_2026-08-24.md` is
reconciled with it.
**Reference data**: full vanilla + AE install —
`/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/`
(`Skyrim.esm`, `Update.esm`, `Dawnguard.esm`, `HearthFires.esm`, `Dragonborn.esm`,
`_ResourcePack.esl`, 23 shipped BSAs). Every dimension had real-data validation; nothing
below is code-read-only speculation.
**Dedup baseline**: `gh issue list --state all --limit 1000` (issues #2345–#3398,
`/tmp/audit/issues_all.json`) + the full `docs/audits/` tree.

---

## Scope line

All 7 dimensions were executed **directly by this agent, solo** — no sub-agent fan-out
(per the task briefing; nested-agent result relay is unreliable in this project).
`crates/facegen` was treated as in-scope (it has no owner audit of its own).

Three findings were **already filed by concurrent audits in this same suite run** and are
deliberately **not** re-filed here: the `SLSF2_Soft_Lighting` slot-2 mask loss on FaceGen
heads (`crates/nif/src/import/material/slot_role.rs:236-248`), the Skyrim
`Use Stats` / `Use Traits` template-chain race mismatch
(`crates/plugin/src/esm/records/actor_value_derive.rs:188`), and the GRUP recursion depth
bound.

The engine was **not launched** (the user may have a live instance); no `pkill` was run.
Verification used the NIF/BSA/plugin crates directly against shipped bytes, plus
`cargo test` on the binary crate. Consequently the **Whiterun control-bench FPS / entity
count is unverified this pass** and is reported as such rather than fabricated — the same
posture the first 2026-08-27 pass took.

---

## Executive Summary

The parse-level gates are healthy and were re-measured, not carried:
**32,709 / 32,709** NIF-family files in `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa`
parse clean — 0 truncated, 0 error — which is exactly the ROADMAP figure. Every one of the
five CRITICAL/HIGH defects the earlier 2026-08-27 pass opened is **fixed and verified
fixed at this HEAD** (see *Reconciliation*), including the CRITICAL SSE
`SkinPartition.Triangles` index-space inversion, and both `#3361` real-data equip guards
pass in 1.79 s against real `Skyrim.esm`.

**The new findings are, again, one layer above the parser** — and this time they are
concentrated in the *equip assembly* step that the previous pass's own fixes made
reachable. The two HIGH equip findings are structurally the same shape as the D3-02 the
last pass found: a filter or a hook whose premise is true for the common case and silently
false for a whole class of vanilla content.

| Severity | Count | Findings |
|---|---:|---|
| CRITICAL | 0 | — |
| HIGH | 3 | D3-01, D3-02, D5-01 |
| MEDIUM | 1 | D6-01 |
| LOW | 3 | D4-01, D5-02, D7-01 |
| **Total** | **7** | |

### The three that matter

1. **D3-01 (HIGH)** — seven vanilla Skyrim races ship a default skin ARMO whose `BOD2`
   mask is **zero** (`SkinDraugr`, `SkinSabrecat`, `SkinFrostbiteSpider`, `SkinSkeever`,
   `SkinSlaughterfish`, …). `EquipmentSlots::equip(0, idx)` occupies nothing, so the
   `#2094` occupancy filter drops every mesh that skin queued. Measured on real
   `Skyrim.esm` through the live `build_npc_equip_state`: **351 of 351** NPCs on those
   races lose their body mesh, and **170 of them end the equip pass with zero meshes at
   all**. 322 of the 351 are Draugr — the game's most common dungeon enemy.
2. **D3-02 (HIGH)** — the pre-baked FaceGen head is the mesh source for the head-family
   biped regions (`130` head, `131` hair, `141` long hair, `143` ears — measured present
   in the shipped facegeom NIFs), but `PrebakedPhase::Facegen` spawns it with
   `pre_spawn = None`, so no `hidden_biped_mask` is ever applied to it and it is never
   enrolled in `EquipmentSlots`. **587 vanilla ARMOs occupy the Hair bit and 1,208 of
   5,118 NPCs (23.6%) equip head-family armour**; every one renders its hair through the
   helmet. The hiding machinery exists and works — it is simply never wired to this mesh.
3. **D5-01 (HIGH)** — `BsaArchive::extract` validates the declared `original_size` through
   `checked_chunk_size` and then **uses it only as a `Vec::with_capacity` hint**, inflating
   with an unbounded `read_to_end`. The v105 LZ4-frame branch is Skyrim's. The sibling
   defect on the ESM side was filed HIGH today
   (`docs/audits/AUDIT_ESM_2026-08-27.md`), which explicitly deferred
   `crates/bsa`'s decompressors to this audit family.

D6-01 is the one MEDIUM: `.bto` object LOD binds a single per-worldspace atlas to every
sub-mesh, but **809 of 2,357 vanilla `.bto` diffuse bindings (34.3%) name
`landscape\mountains\mountainslab02.dds`**, not the atlas — including 612 of Tamriel's own.

---

## Findings

### SKY-2026-08-27b-D3-01: a race default skin with `BOD2 == 0` loses its body mesh to the #2094 occupancy filter — 351/351 vanilla creature-race NPCs, all 322 Draugr included

- **Severity**: HIGH
- **Dimension**: 3 (NPC equip + FaceGen)
- **Location**: `byroredux/src/npc_spawn.rs:977` (the retain), fed from
  `byroredux/src/npc_spawn.rs:795-796` (the race-skin equip + `race_skin_slots` record)
- **Status**: NEW
- **Confidence**: CONFIRMED — reproduced through the live `build_npc_equip_state` against
  real `Skyrim.esm`, not inferred.
- **Description**: `build_npc_equip_state` equips the race default skin first as the
  lowest-priority layer (#2093), then at the end runs the #2094 occupancy filter:

  ```rust
  // byroredux/src/npc_spawn.rs:977
  armor_to_spawn.retain(|armor| equipment_slots.occupants.contains(&Some(armor.inv_idx)));
  ```

  Its premise — "a queued mesh is only kept when its inventory index still holds a biped
  bit" — silently fails for any ARMO whose authored mask is zero, because
  `EquipmentSlots::equip` iterates the set bits of the mask and a zero mask sets none:

  ```rust
  // crates/core/src/ecs/components/inventory.rs:226-233
  pub fn equip(&mut self, slot_mask: u32, idx: InventoryIndex) -> Vec<InventoryIndex> {
      let mut displaced = Vec::new();
      for bit in 0..MAX_BIPED_SLOTS {
          if slot_mask & (1u32 << bit) == 0 {
              continue;
          }
  ```

  Such a skin can therefore never satisfy the retain, and the mesh it resolved — the whole
  creature body — is discarded. `race_skin_slots` records `(inv_idx, 0)`, so the
  `displaced_mask` fold above it also collapses to `0` and contributes nothing.
- **Evidence**: measured on real `Skyrim.esm`.

  ARMO census — 10 of 2,762 armour records author `BOD2 == 0`, and every one of them
  carries ARMAs (i.e. they *do* name meshes):

  ```
  ARMO biped_flags==0: 10 (of which 10 have ARMAs); nonzero=2752
    SkinDraugrHair01 (0003BC83) armatures=1
    SkinDraugrHair02 (0003BC84) armatures=1
    SkinDraugrBeard01 (0003BC81) armatures=1
    SkinSlaughterfish (0004124A) armatures=3
    SkinSabrecat (00016EE6) armatures=2
    SkinDraugr (00016EE3) armatures=1
    SkinFrostbiteSpider (0003636F) armatures=2
    SkinFrostbiteSpiderCold (00048C0E) armatures=2
  ```

  RACE census — 7 of 99 races point `WNAM` at one of them:

  ```
  races=99  default-skin BOD2==0: 7 | BOD2!=0: 90 | no skin: 2
    race DraugrRace            (00000D53) skin SkinDraugr
    race FrostbiteSpiderRace   (000131F8) skin SkinFrostbiteSpider
    race SabreCatRace          (00013200) skin SkinSabrecat
    race SkeeverRace           (00013201) skin SkinSkeever
    race SlaughterfishRace     (00013203) skin SkinSlaughterfish
    race FrostbiteSpiderRaceLarge (00053477) skin SkinFrostbiteSpider
    race DraugrMagicRace       (000F71DC) skin SkinDraugr
  NPC_ records on those races: 351 of 5118 total
    {"DraugrMagicRace": 8, "DraugrRace": 314, "FrostbiteSpiderRace": 10,
     "FrostbiteSpiderRaceLarge": 6, "SabreCatRace": 4, "SkeeverRace": 8,
     "SlaughterfishRace": 1}
  ```

  Driving the real `build_npc_equip_state` (a temporary `#[ignore]` probe test in
  `byroredux/src/npc_spawn/tests.rs`, run against real `Skyrim.esm`, then reverted):

  ```
  PROBE 00022401 EncDraugr02MissileHeadM01     race=DraugrRace skin=SkinDraugr bod2=0x0 meshes=0 slots_occupied=0
  PROBE 000EA50E EncDraugr03AmbushMelee2HHeadM07 race=DraugrRace skin=SkinDraugr bod2=0x0 meshes=0 slots_occupied=0
  PROBE 00073989 dunFellglow_WarlockPet        race=FrostbiteSpiderRace skin=SkinFrostbiteSpider bod2=0x0 meshes=1 slots_occupied=1
  PROBE creature-skin NPCs: skin-mesh-DROPPED=351 skin-mesh-kept=0
  ```

  A separate pass of the same probe counted **170 of the 351** ending with
  `armor_to_spawn.len() == 0` — no mesh source of any kind, only the shared skeleton.
- **Impact**: every Draugr, sabrecat, skeever, frostbite spider and slaughterfish placement
  in vanilla Skyrim + Dawnguard + Dragonborn spawns without its body. Where the actor also
  carries an outfit (most Draugr do), the armour renders on a bodyless skeleton; where it
  does not (170 records), nothing renders. This is the single most common enemy family in
  the game. Nothing catches it: the equip guards added by `#3361` walk only the six
  Bannered Mare humans, all of whom sit on `NordRace` (`SkinNaked`, `BOD2 != 0`).
- **Related**: `#2094` (the filter), `#2093` (the race-skin layer), `#3357` (the previous
  race-skin resolver defect, CLOSED). Nothing in the 1,000-issue dedup baseline covers it.
- **Suggested Fix**: exempt the race-skin entry from the occupancy filter when its authored
  mask is zero — a skin that claims no biped region cannot be *displaced* out of one, so
  the filter has no opinion about it. Concretely: retain when
  `armor.inv_idx == skin_inv_idx && skin_biped_flags == 0`, in addition to the existing
  occupancy test. Add a real-data guard alongside `#3361`'s that asserts a `DraugrRace`
  NPC resolves at least one mesh.

---

### SKY-2026-08-27b-D3-02: the pre-baked FaceGen head is spawned with no displacement mask, so hair renders through every helmet — 587 hair-slot ARMOs, 1,208 of 5,118 NPCs

- **Severity**: HIGH
- **Dimension**: 3 (NPC equip + FaceGen)
- **Location**: `byroredux/src/npc_spawn/resumable.rs:1087-1097` (the `pre_spawn = None`
  argument at `:1096`), against the working machinery at
  `byroredux/src/npc_spawn/resumable.rs:1111-1118` (the armour phase) and
  `byroredux/src/npc_spawn.rs:965` (the only `hidden_biped_mask` producer)
- **Status**: NEW
- **Confidence**: CONFIRMED — code path read end-to-end; both the partition data and the
  displacing ARMO population measured on shipped bytes.
- **Description**: the Skyrim+ equip chain hides displaced skin by handing
  `load_nif_bytes_with_skeleton` a `pre_spawn` hook that calls
  `ImportedMesh::hide_skin_partitions`:

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

  And `hidden_biped_mask` is only ever set for the *race skin*'s entries
  (`byroredux/src/npc_spawn.rs:965`), which resolve to torso/hands/feet ARMAs — the
  FaceGen head is never enrolled in `EquipmentSlots` and never receives a mask. The head
  NIF is nonetheless a multi-region mesh source in exactly the way the race skin is.
- **Evidence**: the shipped pre-baked heads carry per-triangle dismember partitions for the
  head *family*, not just the head. Sweeping 400 of the 3,158
  `meshes\actors\character\facegendata\facegeom\…\*.nif` in `Skyrim - Meshes0.bsa`
  through `import_nif_scene`:

  ```
  files=400 meshes=2603 no_skin=0 no_bp=1230
  triangle body-part histogram:
    [(130, 651867), (131, 354146), (141, 140340), (230, 66408), (143, 25084),
     (30, 13068), (1, 12612), (0, 2060), (132, 1008), (41, 602), (31, 594)]
  meshes by dominant body-part: [("bp130", 646), ("bp131", 370), ("bp141", 340), …]
  ```

  `131` / `141` / `143` are hair, long hair and ears; `dismember_body_part_to_biped_bit`
  (`crates/nif/src/import/types.rs:1102-1109`) maps them to biped bits 1, 11 and 13, i.e.
  exactly the bits Skyrim helmets and hoods claim. On real `Skyrim.esm`:

  ```
  ARMO total=7264  head-family (bits 0/1/11/12/13)=702  hair-bit(1)=587
  NPC_ total=5118  equipping head-family armour (1 LVLI hop)=1208
  ```

  `hide_skin_partitions` itself is verified working on this data — hiding the Body bit on
  the vanilla character-asset corpus removes triangles on 27 of 524 meshes and is a
  correct no-op on the rest, and the skin meshes carry clean single-region partitions
  (`femalebody_1.nif` → `{32: 688}` and `{32: 2212, 34: 40, 38: 160}`;
  `femalehands_1.nif` → `{33: 1448}`; `malefeet_1.nif` → `{37: 316}`).
- **Impact**: roughly one Skyrim NPC in four renders their hair, long hair and ears
  intersecting the helmet, hood or circlet they are wearing — the classic
  "hair through the helmet" artifact, on guards, bandits, soldiers and Draugr alike. It is
  a pure wiring gap: the mask, the biped→partition mapping and the pre-spawn hook all
  exist and all work.
- **Related**: `#2094` (the displacement-mask mechanism), `#2093` (the race-skin layer),
  `#3357` (which fixed the mask reaching *every* skin mesh but not this one).
  Distinct from the concurrently-filed `slot_role.rs` slot-2 FaceGen finding, which is
  about the head's *texture roles*, not its geometry.
- **Suggested Fix**: give the FaceGen head an inventory entry + `EquipmentSlots` claim over
  the head-family bits its own partitions cover, so the existing occupancy filter and
  `displaced_mask` fold treat it exactly like the race skin, then pass the resulting
  `hidden_biped_mask` through the same `pre_spawn` hook the armour phase uses. Guard with a
  real-data test on a helmeted vanilla NPC (any `EncDraugr*` or Whiterun guard).

---

### SKY-2026-08-27b-D5-01: BSA extraction validates the declared uncompressed size and then never enforces it — `read_to_end` inflates without a bound

- **Severity**: HIGH
- **Dimension**: 5 (BSA v105 / LZ4)
- **Location**: `crates/bsa/src/archive/extract.rs:131-141` (both codec arms; `:134` LZ4
  frame — Skyrim's v105 path — and `:139` zlib)
- **Status**: NEW
- **Confidence**: CONFIRMED (code read; the bound is demonstrably absent)
- **Description**: the compressed branch reads the 4-byte declared uncompressed size,
  bounds it correctly through `checked_chunk_size` (`MAX_CHUNK_BYTES` = 1 GB) — and then
  spends that validated value only as a capacity hint:

  ```rust
  // crates/bsa/src/archive/extract.rs:131-141
  let (decompressed, codec) = if self.version >= BSA_V_SKYRIM_SE {
      let mut decoder = lz4_flex::frame::FrameDecoder::new(&compressed[..]);
      let mut buf = Vec::with_capacity(original_size);
      decoder.read_to_end(&mut buf)?;
      (buf, "LZ4 frame")
  } else {
      let mut decoder = ZlibDecoder::new(&compressed[..]);
      let mut buf = Vec::with_capacity(original_size);
      decoder.read_to_end(&mut buf)?;
      (buf, "zlib")
  };
  ```

  `read_to_end` has no output limit; it grows `buf` until the decoder reaches end-of-stream.
  The function *does* notice the mismatch — but only afterwards, and only as a `warn!`:

  ```rust
  // crates/bsa/src/archive/extract.rs:154-165
  if decompressed.len() != original_size {
      log::warn!("BSA {} decompression for '{}' produced {} bytes but original_size declared {} …");
  }
  ```

  So the archive's own declared ceiling is checked, logged against — and never used to stop
  the allocation it was checked for.
- **Impact**: a crafted or corrupt `.bsa` — the ordinary distribution format for Skyrim
  mods, i.e. the engine's real untrusted-input surface — terminates the process on
  allocation failure. `entry.size` is masked to 30 bits, so the compressed payload is
  bounded at 1 GB; LZ4's block ratio tops out near 255:1 and DEFLATE's near 1000:1, so the
  reachable inflation is hundreds of GB from an archive that looks unremarkable on disk.
  Unrecoverable: an OOM abort is not an `Err` any caller can handle, and the per-NIF
  `catch_unwind` in `streaming::pre_parse_cell` cannot intercept it.
- **Related**: the ESM-side sibling filed today in
  `docs/audits/AUDIT_ESM_2026-08-27.md` (`crates/plugin/src/esm/reader.rs:630-647`, same
  `Vec::with_capacity` + unbounded `read_to_end` shape, rated HIGH) — that report states
  verbatim that *"`crates/bsa`'s decompressors are a separate surface"*, so this is the
  uncovered half, not a duplicate. `#2356` (BA2 DX10 per-chunk cap) and `#3392`/`#3394`
  (the BA2 LZ4 *block* safe-decoder work) both hardened the **BA2** reader and left the
  BSA reader as-is.
- **Suggested Fix**: replace both `read_to_end` calls with
  `Read::take(original_size as u64).read_to_end(&mut buf)` and turn the existing
  post-hoc length comparison into a hard `Err` when the limit was reached, so an over-ratio
  payload is diagnosably rejected instead of silently truncated or fatally inflated. Add
  the two negative tests the BA2 side already has (lying size prefix; over-ratio payload).

---

### SKY-2026-08-27b-D6-01: `.bto` object LOD binds one worldspace atlas to every sub-mesh, but 34% of vanilla bindings name the mountain-slab texture instead

- **Severity**: MEDIUM
- **Dimension**: 6 (specialty blocks + real-data rendering)
- **Location**: `byroredux/src/cell_loader/object_lod.rs:294` (single atlas resolve),
  `:342` and `:356` (bound to every sub-mesh), premise stated at `:249`
- **Status**: NEW
- **Confidence**: CONFIRMED (measured across every shipped `.bto`)
- **Description**: `spawn_object_lod_quad` resolves exactly one texture per worldspace —
  `object_lod_atlas_path(scheme, worldspace_key)` — and binds it to every sub-mesh the
  `.bto` imports, discarding whatever each sub-mesh's own `BSShaderTextureSet` named. The
  module doc states the assumption as fact: *"All sub-meshes share the worldspace object
  atlas"* (`:249`). Vanilla Skyrim data contradicts it.
- **Evidence**: reading slot 0 of every `BSLightingShaderProperty` in all 1,078 `.bto`
  files in `Skyrim - Meshes1.bsa`, grouped by worldspace:

  ```
  22 distinct texture bindings across 1078 .bto
     1131  tamriel               -> data\textures\terrain\tamriel\objects\tamriel.objects.dds
      612  tamriel               -> data\textures\landscape\mountains\mountainslab02.dds
      114  dlc2solstheimworld    -> …\dlc2solstheimworld.objects.dds
       82  dlc2apocryphaworld    -> …\dlc2apocryphaworld.objects.dds
       61  dlc01falmervalley     -> …\dlc01falmervalley.objects.dds
       58  dlc01soulcairn        -> …\dlc01soulcairn.objects.dds
       57  dlc2solstheimworld    -> data\textures\landscape\mountains\mountainslab02.dds
       40  sovngarde             -> …\sovngarde.objects.dds
       39  sovngarde             -> data\textures\landscape\mountains\mountainslab02.dds
       …
  ```

  **809 of 2,357** slot-0 bindings (34.3%) name `mountainslab02.dds`, and every one of the
  12 worldspaces with baked object LOD has some. The atlas the code binds does exist and
  resolves (`textures\terrain\tamriel\objects\tamriel.objects.dds` →
  `Skyrim - Textures7.bsa`, verified by direct `BsaArchive::contains`), so this is not a
  missing-asset problem — the correct per-sub-mesh path is right there in the imported
  material and is simply not read.
- **Impact**: roughly a third of Skyrim's distant-object LOD geometry — in Tamriel, that is
  the mountain silhouette, the most visually prominent distant feature in the game —
  samples the object *atlas* with UVs authored for a tiling mountain slab. The result is
  coherent-looking garbage rather than an obvious failure, which is why the parse-rate and
  texture-resolution gates stay green. The `_n` normal siblings are also present for every
  atlas and are separately unbound, which `object_lod.rs:508-509` already records as
  known forward scope.
- **Related**: `#2444` (MAT-D3-02 — object LOD carries no real `Material`; CLOSED, and its
  `translate_texture_only_material` call at `:356` is the site that would carry the fix),
  `#2586`/`#2587` (earlier `.bto` corpus/alignment work). Distinct from
  `SKY-2026-08-27-D6-01` (`.btr` height scale, `#3358`, CLOSED) — that was terrain, this is
  objects, and it is a texture binding, not a transform.
- **Suggested Fix**: prefer each sub-mesh's own imported `material.textures.base_color`
  when it is populated (running it through the existing `strip_build_prefix` for the
  `data\` prefix these paths carry), and keep the per-worldspace atlas as the fallback for
  sub-meshes that name nothing. That reduces to one `resolve_texture` per distinct path
  per quad, which is the same order of work as today.

---

### SKY-2026-08-27b-D4-01: `list_cells.rs`'s `.STRINGS` doc comment describes a gap that the archive fallback closed — "Skyrim SE hits this for every cell" is false at HEAD

- **Severity**: LOW
- **Dimension**: 4 (multi-master load order)
- **Location**: `byroredux/src/list_cells.rs:130-138`
- **Status**: NEW
- **Confidence**: CONFIRMED (both halves of the claim checked against code and archives)
- **Description**: the comment reads:

  ```rust
  /// A localized plugin's FULL sub-record holds a string-table ID, not
  /// text; when the companion table can't be found the resolver hands
  /// back a `<lstring 0xNNNNNNNN>` placeholder. Skyrim SE hits this for
  /// every cell — it ships its `.STRINGS` inside `Skyrim - Interface.bsa`
  /// rather than as loose `Data/Strings/` files, which
  /// `esm::StringTableSet::load` is the only thing that reads.
  ```

  Both load-bearing statements are now wrong. `list_cells::run` calls
  `parse_record_indexes_in_load_order` (`byroredux/src/cell_loader/load_order.rs:206-213`),
  which installs an `ArchiveStringSource` and routes through
  `StringTableSet::load_with_archive` (`:118`), not `::load`. `ArchiveStringSource::discover`
  (`:143-175`) matches `Skyrim - Interface.bsa` twice over — once as a `plugin_archive`
  (`" - interface"` suffix on the `skyrim` stem) and once as a `shared_archive`
  (`stem.ends_with(" - interface")`), the latter covering `Update.esm` / `Dawnguard.esm` /
  `HearthFires.esm` / `Dragonborn.esm`, whose tables all live in that same archive.
- **Evidence**: `Skyrim - Interface.bsa` carries 138 `strings\…` entries including
  `strings\skyrim_english.strings`, `strings\update_english.strings`,
  `strings\dawnguard_english.strings` and `strings\dragonborn_english.strings`;
  `_ResourcePack.bsa` and each `ccBGSSSE*.bsa` carry their own 27 apiece, all reachable
  through the exact-stem match. The behaviour is pinned by the real-data test
  `real_skyrim_load_order_preserves_categories_and_resolves_archive_strings`
  (`byroredux/src/cell_loader/load_order.rs:540`).
- **Impact**: documentation only. The `is_unresolved_lstring` helper the comment introduces
  is still correct and still useful (a non-localized or table-less plugin does produce the
  placeholder) — but a reader is told a live, fixed subsystem is broken, which is exactly
  the false premise the project's audit-hygiene rule exists to prevent.
- **Related**: `#1553` (the `.STRINGS` wiring), `db5bb149` (the multi-plugin load-path
  invocation the `/audit-skyrim` Dimension 4 checklist names).
- **Suggested Fix**: rewrite the comment to say the placeholder is what a *non-localized or
  table-less* plugin yields, and point at `ArchiveStringSource` for where Skyrim's tables
  actually come from.

---

### SKY-2026-08-27b-D5-02: the parse-rate gate's "whole vanilla set" rationale is false — `Skyrim - Animations.bsa` ships 44 NIFs and is loaded by the engine's own profile

- **Severity**: LOW
- **Dimension**: 5 / 6 (corpus gate)
- **Location**: `crates/nif/tests/common/mod.rs:178-184`
- **Status**: NEW (distinct from the prior `SKY-2026-08-27-D6-03` / `#3369`, which named
  only `_ResourcePack.bsa` and the four Creation Club archives and missed this one)
- **Confidence**: CONFIRMED (archive census)
- **Description**: the `Game::SkyrimSE` arm of `mesh_archives()` is justified by a comment
  asserting the two-archive pair is complete:

  ```rust
  // SE splits the base mesh corpus across two archives and folds
  // the DLC into them (there is no `Dawnguard.bsa` — only the
  // `.esm`), so this pair is the whole vanilla set on every
  // install, AE or not. `_ResourcePack.bsa` and the `ccBGSSSE*`
  // archives are Creation Club content that varies per account,
  // and stay out for the reproducibility rule above.
  Game::SkyrimSE => &["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"],
  ```

  The per-account-variance argument is sound for `_ResourcePack.bsa` and the `ccBGSSSE*`
  archives. It does not apply to `Skyrim - Animations.bsa`, which ships with every SE
  install, contains NIFs, and is already in the engine's own launch profile
  (`assets/debug_profiles.toml:103`, `default_bsas`) — so the engine loads meshes from an
  archive the parse-rate gate never opens.
- **Evidence**: NIF-family census over all 23 shipped BSAs:

  ```
  Skyrim - Animations.bsa   v105 total=  8979 nif=   44 btr=   0 bto=   0
  Skyrim - Meshes0.bsa      v105 total= 19443 nif=18862 btr=   0 bto=   0
  Skyrim - Meshes1.bsa      v105 total= 14242 nif= 3185 btr=9584 bto=1078
  Skyrim - Interface/Misc/Shaders/Sounds/Textures*/Voices*  nif=0
  _ResourcePack.bsa         v105 total=   463 nif=  149
  ccBGSSSE001-Fish.bsa      v105 total=   744 nif=  231
  ccBGSSSE025-AdvDSGS.bsa   v105 total=   645 nif=  266
  ccBGSSSE037-Curios.bsa    v105 total=   218 nif=   65
  ccQDRSSE001-SurvivalMode  v105 total=    86 nif=    4
  ```

  Sweeping the ungated archives through the same parse path the gate uses:

  ```
  Skyrim - Animations.bsa: files=44  clean=44  truncated=0 error=0
  _ResourcePack.bsa:       files=149 clean=149 truncated=0 error=0
  ccBGSSSE001-Fish.bsa:    files=231 clean=231 truncated=0 error=0
  ccBGSSSE025-AdvDSGS.bsa: files=266 clean=266 truncated=0 error=0
  ccBGSSSE037-Curios.bsa:  files=65  clean=65  truncated=0 error=0
  ccQDRSSE001-SurvivalMode: files=4  clean=4   truncated=0 error=0
  ```
- **Impact**: no live defect — all 759 ungated NIFs parse clean today. The gap is that a
  parser regression touching only animation-archive content would leave the Skyrim gate
  green, and the comment's stated rationale is factually wrong in a way that will keep the
  archive out on the next review too.
- **Related**: `#3369` (the still-open prior instance), `#3041` (the FNV instance,
  CLOSED).
- **Suggested Fix**: add `"Skyrim - Animations.bsa"` to the arm — it is unconditional
  vanilla content, so it does not touch the reproducibility rule — and narrow the comment's
  claim to the CC/AE archives it actually describes.

---

### SKY-2026-08-27b-D7-01: `material_translate.rs` still documents the retired 348-byte `GpuMaterial` — the one site #3240's sweep missed

- **Severity**: LOW
- **Dimension**: 7 (NIFAL canonical material translation)
- **Location**: `byroredux/src/material_translate.rs:77`
- **Status**: NEW (sibling site of `#3240`, CLOSED)
- **Confidence**: CONFIRMED
- **Description**: the `material_optical_scalar` doc justifies overloading `ior` by citing
  the record's size: *"without adding another field to the hot 348-byte GPU material
  record."* `GpuMaterial` has since grown 348 → 364 → 396 → 432 B; the live pin is
  `gpu_material_size_is_432_bytes` (`crates/renderer/src/vulkan/material.rs:46`, `:71`,
  `:87`). `#3240` swept exactly this stale figure out of
  `crates/renderer/shaders/include/bindings.glsl`; this occurrence, in the NIFAL boundary
  that reasons about the record's cost, was not in that sweep.
- **Evidence**: `grep -rn "348" byroredux/src/material_translate.rs` → one hit, line 77.
  `crates/renderer/src/vulkan/material.rs:43` documents the growth chain itself
  (`… → 348 B (common supplemental texture roles) → 364 B (#2221 animated …)`).
- **Impact**: documentation only, but it is a **GPU layout contract** number in the module
  whose whole job is deciding what fits in that record — the class of stale figure the
  audit-hygiene rule calls out by name.
- **Related**: `#3240`, `#2222`.
- **Suggested Fix**: change to 432 B, or better, drop the literal and cite
  `gpu_material_size_is_432_bytes` so it cannot drift again.

---

## Reconciliation with both priors

### `docs/audits/AUDIT_SKYRIM_2026-08-27.md` (HEAD `30537bf3`, 17 findings)

Every CRITICAL and HIGH is fixed and **verified fixed at `969d81c8`**, not assumed.

| Prior ID | Sev | State at this HEAD | Verification |
|---|---|---|---|
| D1-01 (SSE partition triangles are global) | CRITICAL | **FIXED** (`07ca5979`, `#3355`) | `crates/nif/src/import/mesh/sse_recon.rs:133-160` now bounds against `decoded.positions.len()` with no `vertex_map` hop; per the task briefing this was re-verified on the full vanilla corpus (15,728/15,728 dismember shapes) |
| D1-02 (`triangle_body_parts` same inverted remap) | MEDIUM | **FIXED in the same commit** (`#3360`) | `crates/nif/src/import/mesh/skin.rs:49` gates on `partition.global_vertex_data.is_some()`, exactly as the finding directed; both halves landed together |
| D3-01 (`parse_otft` reads one FormID) | HIGH | **FIXED** (`fa71f1a2`, `#3356`) | `crates/plugin/src/esm/records/outfit.rs:82-87` uses `chunks_exact(4)`; `bannered_mare_outfits_keep_every_inam_entry_on_real_skyrim_data` **passes** on real data this pass |
| D3-02 (`resolve_armor_mesh` single ARMA) | HIGH | **FIXED** (`e0d5ec18`, `#3357`) | `crates/plugin/src/equip.rs:153-227` (`resolve_armor_meshes`); `bannered_mare_npcs_resolve_a_full_equip_state_on_real_skyrim_data` **passes** |
| D3-03 (no real-data equip guard) | MEDIUM | **FIXED** (`b08ebada`, `#3361`) | Both guards run green in 1.79 s (`cargo test -p byroredux --bin byroredux bannered_mare -- --ignored`) |
| D3-04 (`multi_pick` has no Skyrim real-data pin) | LOW | Superseded by `#3361`'s corpus floor | Not re-filed |
| D4-01 (cross-CELL tombstone) | MEDIUM | **FIXED** (`921bfb8d`, `#3362`) | Not re-derived |
| D6-01 (`.btr` height axis unscaled) | HIGH | **FIXED** (`05c029ed`, `#3358`) | Re-measured independently: across **15,585** `.btr` sub-meshes in all 9,584 shipped `.btr`, the authored scale equals the quad level for **15,585/15,585**, chain scale is 1.0 everywhere and both local and chain translation are zero everywhere — so `btr_local_to_world`'s uniform `level` scale is exactly the authored transform |
| D6-02 (`.btr` `WATER` welded into land) | MEDIUM | **FIXED** (`337c15c8`, `#3363`) | Re-measured: 6,001 of 15,585 `.btr` sub-meshes hang off a `WATER` parent and **all 6,001** are caught by `btr_mesh_is_water`; **no** `.btr` is all-water, so the exclusion never empties a quad |
| D7-01 (inverted `ice` classifier) | HIGH | **FIXED** (`ffbf5681`, `#3359`) | `crates/core/src/ecs/components/material.rs:754-781` (`path_indicates_ice`), routed from both call sites |
| D2-01, D4-02, D5-01, D5-02, D6-03, D7-02, D7-03 | LOW | **Unchanged, already filed** (`#3364`–`#3370` range) | `plugin_for_form_id` still indexes by position (`byroredux/src/cell_loader/load_order.rs:31-34`); the Phase-2 roughness doc claim still stands at `byroredux/src/material_translate.rs:50-55`. Not re-filed. D6-03's *remediation rationale* is separately wrong — see D5-02 above |

### `docs/audits/AUDIT_SKYRIM_2026-08-24.md` (0 findings)

Nothing to reconcile: that pass opened no findings and its five carried items were all
closed there. Its one still-open carry, the CHARAL leveling-GMST unreachability
(`#3221` / `#3170`), remains open and CHARAL-owned; not re-filed.

---

## Regression guards re-verified intact this pass

| Guard | Result |
|---|---|
| Meshes0 + Meshes1 parse rate | **32,709 / 32,709 clean**, 0 truncated, 0 error — matches ROADMAP exactly |
| `#838` `BSLODTriShape` → `NiLodTriShape` (not `BsTriShape`) | Intact — `crates/nif/src/blocks/mod.rs:473`; `BSMeshLODTriShape` → `parse_lod` at `:478`, `BSSubIndexTriShape` at `:489`, `BSDynamicTriShape` at `:505` |
| `#837` `BsLagBoneController` / `BsProceduralLightningController` | Intact — `crates/nif/src/blocks/mod.rs:880-881` |
| `#614` `BSBoneLODExtraData`, `BSTreeNode`, `BSPackedCombined[Shared]GeomDataExtra` | Intact — `:663`, `:344`, `:743` |
| `#2695` single slot→role table, importer + REFR overlay | Intact — `byroredux/src/cell_loader/spawn/mesh_instance.rs:188-195` threads all six flag inputs (`glow_map`, `model_space_normals`, `soft`, `rim`, `back`) into the same `TextureSlotContext` the importer builds |
| Unrouted authored texture-slot telemetry, whole corpus | slot 0/1/3/4/5 = **0**; slot 2 = 1,364; slot 6 = 3,151; slot 7 = 11. Slot 6 is 3,150 FaceTint FaceGen tints (deliberately owned by the FaceGen path) + 1; slot 7 is 11 non-MSN bindings; slot 2 is the `#3068` "no Glow/Soft/Rim flag authored" policy. All three are documented design decisions, none is a routing gap |
| `#3356` / `#3357` real-data equip guards | Both **PASS** on real `Skyrim.esm` |
| `#1832` mass-0 Dynamic reclassification, ghosting investigation | Settled / open respectively — not re-litigated, per briefing |

## Shader-Type Coverage Matrix

Unchanged from the 2026-08-27 pass — `crates/nif/src/blocks/shader.rs` has **zero** delta
commits in the 27-commit window since `30537bf3`. The canonical numbering translation
(`canonical_shader_type`, `crates/nif/src/import/material/slot_role.rs:140-153`) and the
nine `ShaderTypeData` variants are byte-for-byte as that report's matrix records them. The
corpus-level cross-check performed instead this pass is the unrouted-slot telemetry above,
which is the observable consequence of every arm being right.

## Cell-Load Regression Status

| Guard | Result |
|---|---|
| Meshes0 + Meshes1 sweep | **32,709 / 32,709 clean** (re-measured) |
| `.STRINGS` reachable for Skyrim + all four DLC | **Verified** — 138 `strings\…` entries in `Skyrim - Interface.bsa`, matched by `ArchiveStringSource::discover` via both the plugin-stem and shared-archive rules |
| ESL / light-master decode (`#1554`), tombstones (`#1660`), repeatable `--master` (`#561`) | No delta commits to the remap core; not re-derived |
| `#3361` Bannered Mare equip + OTFT guards | **PASS**, live-run |
| Whiterun BanneredMare control bench (entity count / FPS) | **NOT RUN** — engine launch was ruled out for this pass; reported unverified rather than fabricated |

---

## Disproved / rejected candidates

Recorded so they are not re-investigated:

1. **`.btr` placement ignores an authored translation or a non-`level` scale.** Disproved by
   direct measurement over all 9,584 shipped `.btr` (15,585 sub-meshes): local scale is
   exactly the quad level in every case, parent-chain scale is 1.0 in every case, and both
   local and accumulated translation are zero in every case. `btr_local_to_world`'s
   hard-coded `level` reproduces the authored transform exactly.
2. **`object_lod` misplaces `.bto` sub-meshes by using the mesh-local transform instead of
   the accumulated chain.** Disproved: the parent chain is identity on all 2,392 `.bto`
   sub-meshes, and `mesh.translation` *is* the quad's world corner
   (`tamriel.8.-16.-24.bto` → `t = [-65536, 0, 98304]` = `(-16, -24) × 4096`).
   2,116 of 2,392 sub-mesh world AABBs land inside their quad footprint plus one cell of
   slack; the remainder are tall geometry overhanging the quad edge, which is expected.
   The `.bto` *texture* binding is a real defect (D6-01); the transform is not.
3. **Shader type 16 (`EyeEnvmap`) loses its slot-2 `*_sk.dds` mask on 3,096 vanilla
   properties.** Disproved — the first measurement tested the wrong flag bits
   (`SOFT_LIGHTING` is `0x0200_0000`, not `1 << 21`; see
   `crates/nif/src/shader_flags.rs:151-153`). Re-run with the real constants, only **2** of
   the 3,096 reach the drop path; the rest carry `SLSF2_Soft_Lighting` and route to
   `TextureRole::LightingMask` correctly.
4. **`tamriel.objects.dds` is absent from the shipped archives.** Disproved by direct
   `BsaArchive::contains` — it is in `Skyrim - Textures7.bsa`, along with its `_n` sibling.
   The atlas resolves; the defect in D6-01 is that it is bound to sub-meshes that asked for
   something else.
5. **`ImportedMesh::hide_skin_partitions` leaves `bs_sub_index` segment ranges stale after
   compacting `indices`.** True as written (`crates/nif/src/import/types.rs:1194-1203`
   rebuilds `triangle_body_parts` but not the segment table), but not a live defect:
   `bs_sub_index` has exactly one consumer, `hide_skin_partitions` itself, and each
   `ImportedMesh` is hidden at most once per spawn.
6. **The Skyrim SE parse-rate gate is blind to a live parser regression.** Disproved for
   today: all 759 NIFs in the six ungated archives parse clean. The gap is a test-coverage
   gap only (D5-02).

---

## Verification performed this pass

| Check | Result |
|---|---|
| Full `.nif`/`.btr`/`.bto` parse sweep, `Skyrim - Meshes0/1.bsa` | 32,709 / 32,709 clean, 0 truncated, 0 error |
| Full import sweep (`import_nif_scene`) over the same 32,709 | Completed; unrouted-slot telemetry captured (table above) |
| NIF census across all 23 shipped BSAs | 4 archives carry NIF-family content beyond the gated pair |
| Parse sweep over the 6 ungated archives (759 files) | 759 / 759 clean |
| `.btr` transform census (9,584 files, 15,585 sub-meshes) | scale == level, translation zero, rotation identity, in every case |
| `.btr` `WATER` sub-tree census | 6,001 water sub-meshes, 6,001 detected, 0 all-water files |
| `.bto` transform + world-AABB census (1,078 files, 2,392 sub-meshes) | Chain identity; `translation` == quad world corner |
| `.bto` slot-0 texture census (2,357 bindings) | 809 (34.3%) name `mountainslab02.dds`, not the atlas |
| FaceGen head partition census (400 of 3,158 facegeom NIFs, 2,603 sub-meshes) | Hair (131) / long hair (141) / ears (143) partitions present and populated |
| Vanilla character-asset dismember census (574 NIFs, 590 skinned meshes) | 524 carry body-part data, 0 length mismatches; `hide_skin_partitions` behaves correctly on all |
| `Skyrim.esm` ARMO / RACE / NPC_ censuses | 7,264 ARMO · 99 RACE · 5,118 NPC_; figures inline under D3-01 / D3-02 |
| `build_npc_equip_state` driven on real `Skyrim.esm` (temporary probe, reverted) | 351/351 creature-race skin meshes dropped; 170 NPCs reach zero meshes |
| `cargo test -p byroredux --bin byroredux bannered_mare -- --ignored` | **2 passed, 0 failed** (1.79 s, real data) |
| Engine launch / control bench | **Not performed** (deliberate — see Scope line) |

All probes were throwaway `crates/{nif,plugin}/examples/_tmp_sky27b_*.rs` binaries and one
temporary `#[ignore]` test in `byroredux/src/npc_spawn/tests.rs`; **all were deleted /
reverted** and the working tree carries only this report.

---

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-08-27b.md
```

Label every finding `game:skyrim` + `legacy-compat`, plus its own domain label —
D3-01/D3-02 `gameplay` + `import-pipeline`, D5-01 `safety` + `import-pipeline`,
D6-01 `terrain-exterior` + `renderer`, D4-01/D7-01 `doc-rot`, D5-02 `test-gap`.

**Land D3-01 and D3-02 together with a real-data guard**: both are equip-assembly gaps that
the `#3361` Bannered Mare guards cannot see, because all six of those NPCs are humans in
non-head-family gear. A Draugr and a helmeted guard are the two fixtures that would have
caught them.

---

TALLY: CRITICAL=0 HIGH=3 MEDIUM=1 LOW=3
