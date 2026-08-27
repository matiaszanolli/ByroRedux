# Skyrim SE Compatibility Audit — 2026-08-27

**HEAD**: `30537bf3` · **Branch**: `main` (clean tree) · **Skill**: `/audit-skyrim` (all 7 dimensions)
**Reference data**: full vanilla install — `Skyrim.esm` + Dawnguard / HearthFires / Dragonborn,
`_ResourcePack.esl`, `Meshes0/1.bsa`, `Textures0..8.bsa` and 13 further shipped archives.
Every dimension had real-data validation available; none ran code-read-only.

---

## Executive Summary

Skyrim SE is the engine's renderer **control bench** — Whiterun BanneredMare loads and
renders, and 6 named NPCs equip through the M41 OTFT/LVLI chain. This audit was therefore
scoped as regression coverage plus the Skyrim-specific geometry / shader / equip risk
surface, not as readiness scoping.

The parse-level gates are genuinely healthy, and they were re-verified rather than assumed:
the Meshes0 + Meshes1 sweep holds at **32,709 NIFs, 100% clean, 0 truncated, 0 recovered,
0 realignment WARNs**; all 23 shipped BSAs extract **172,918 / 172,918 files with zero
failures**; every one of the 20 `BSLightingShaderType` arms matches `nif.xml` field-for-field
with **0 bytes of stream drift** across 73,475 `BSLightingShaderProperty` blocks; and raw-vs-parsed
ESM record parity is exact on the big tables (REFR + ACHR **703,837 / 703,837**, CELL 17,568 / 17,568).

**The findings are concentrated one layer above that.** All five of the CRITICAL/HIGH defects
are in code that consumes correctly-parsed data and then uses it wrongly — an index space read
in the wrong frame of reference, an array read as a scalar, a list read as a single element, a
uniform scale applied on two axes out of three, and a keyword rule whose true-positive set was
never checked. Not one of them can fail a parse-rate gate, which is exactly why they survived:
**every existing Skyrim gate is green while all five are live.**

| Severity | Count | Findings |
|---|---:|---|
| CRITICAL | 1 | D1-01 |
| HIGH | 4 | D3-01, D3-02, D6-01, D7-01 |
| MEDIUM | 4 | D1-02, D3-03, D4-01, D6-02 |
| LOW | 8 | D2-01, D3-04, D4-02, D5-01, D5-02, D6-03, D7-02, D7-03 |
| **Total** | **17** | |

### The five that matter

1. **D1-01 (CRITICAL)** — on SSE, `NiSkinPartition.triangles` already hold *shape-global*
   vertex indices, but the reconstructor remaps them through `vertex_map` as if they were
   partition-local. **14.69% of all skinned triangle indices are `>= vertex_map.len()`**, which
   is impossible for local indices; those triangles are dropped outright, damaging **10,194 of
   26,708** SSE skin-partition blocks (38.2%) — NPC bodies, creatures and facegen heads.
2. **D3-01 (HIGH)** — `parse_otft` reads one `u32` per `INAM` sub-record, but every Skyrim
   outfit ships **exactly one** `INAM` whose payload is an *array*. **765 of 1,246 outfit items
   (61.4%) are discarded**; 387 of 481 outfits equip one piece instead of all of them.
3. **D3-02 (HIGH)** — `resolve_armor_mesh` returns a single `&str` while a Skyrim ARMO holds an
   armature *list*. The race default skin `SkinNaked` carries **25** ARMAs; one is returned.
4. **D6-01 (HIGH)** — every `.btr` distant-terrain quad carries a uniform authored scale equal to
   its quad level, and `btr_local_to_world` applies it to X and Z but not Y. All prebaked distant
   terrain renders at **1/4 … 1/32 of true elevation**, a different wrong elevation per band.
5. **D7-01 (HIGH)** — the `ice` classifier arm is *inverted*: Bethesda's concatenated-compound
   naming (`icewraithbody`, `icelakesurface`) never satisfies the #2009 word-boundary rule, while
   Imperial-fort masonry (`impwall05ice` — `ice` after a digit) does. Real ice shades matte;
   stone walls shade mirror-smooth.

D1-02 is sequencing-critical rather than severe: `triangle_body_parts` makes the *same* inverted
remap, so the two currently cancel. **Fixing D1-01 alone silently breaks SSE dismemberment and
equip-hiding** — they must land in one commit.

### Verification posture

Every CRITICAL and HIGH finding was **re-measured by the orchestrator with an independently
written probe** before being accepted, because a dimension agent's own measurement is not
evidence on its own. Three of the four HIGHs and the CRITICAL were reproduced against shipped
bytes; the reproductions are recorded inline under each finding's dimension report. The nifly
citation underpinning D1-01 was checked at source rather than taken on trust.

### Corrections to the audit's own inputs

- **The bench-of-record figure in the skill's Game Context is stale, and so was my first
  reading of it.** I initially seeded the dimension agents with 335.0 FPS / 3237 ent from
  `ROADMAP.md:152`; that table is headed *"Superseded bench-of-record"* (`ROADMAP.md:140`) and
  states *"These numbers are not reproducible at HEAD"*. The live figure is the stepped-camera
  refresh at `ROADMAP.md:162-174` — **Whiterun BanneredMare, 5183 ent, 89.9 FPS / 11.12 ms**
  (TAA native). The skill's `R6a-stale-14` reference is stale in the same way. Caught by the
  Dimension 4 agent.
- **The `.btr` / `.bto` LOD meshes live in `Meshes1.bsa`, not `Textures8.bsa`** — the skill's
  Dimension 5 checklist asserts the latter. Caught by the Dimension 5 agent.
- **The control-bench FPS/entity guard could not be run this cycle.** Running the engine was
  ruled out because the user may have a live instance; the bench is reported unverified rather
  than fabricated.

---

## Findings

Ordered by severity, then dimension. Full evidence, including each orchestrator
re-measurement, is inline below; the orchestrator re-measurements are collected in Appendix A.

### SKY-2026-08-27-D1-01: SSE `SkinPartition.Triangles` already hold global indices — remapping them through `vertex_map` drops 17.6% of skinned triangles and mis-indexes 35.6% more

- **Severity**: CRITICAL
- **Confidence**: CONFIRMED (read the code + nifly + measured against Skyrim SE archives)
- **Location**: `crates/nif/src/import/mesh/sse_recon.rs:111-136` (the remap loop; the lookup
  itself at `sse_recon.rs:120`)
- **Description**:
  `try_reconstruct_sse_geometry` treats each `NiSkinPartition.partitions[i].triangles` entry as
  a **partition-local** index and translates it through `part.vertex_map` to reach the global
  packed-buffer vertex space:

  ```rust
  for (i, &local) in tri.iter().enumerate() {
      match part.vertex_map.get(local as usize) {
          Some(g) => globals[i] = g,
          None => { ok = false; break; }
      }
  }
  ```

  On Skyrim SE that translation is a no-op at best and a corruption at worst, because the
  `Triangles` field of an SSE `SkinPartition` is **already** expressed in the shape's global
  vertex space. nifly states this explicitly and unconditionally for `Stream() == 100`
  (BSVER 100 = Skyrim SE), in `NiSkinPartition::Sync`
  (`/mnt/data/src/reference/nifly/src/Skin.cpp:82-85`):

  ```cpp
  if (stream.GetVersion().User() >= 12 && stream.GetVersion().Stream() == 100) {
      if (stream.GetMode() == NiStreamReversible::Mode::Reading)
          bMappedIndices = false;
  ```

  and documents the meaning of that flag in `include/Skin.hpp:105-109`:

  ```cpp
  // bMappedIndices is not in the file; it is calculated from
  // the file version.  If true, the vertex indices in triangles
  // and strips are indices into vertexMap, not the shape's vertices.
  // trueTriangles always uses indices into the shape's vertex list.
  bool bMappedIndices = true;
  ```

  With `bMappedIndices == false`, nifly's `PrepareTrueTriangles` takes the branch
  `p.trueTriangles = p.triangles;` (`Skin.cpp:422-434`) — i.e. the `Triangles` field *is* the
  true/global list on SSE, and the trailing `Triangles Copy` (`nif.xml:2168`) is a duplicate of
  it, not a differently-indexed sibling.

  Two consequences in HEAD:
  1. A raw index `>= vertex_map.len()` is treated as malformed content and the whole triangle
     is **dropped** (the `#725 / NIF-D4-04` policy). It is not malformed; it is a perfectly
     valid global index that simply exceeds this partition's *vertex count*.
  2. Every raw index `< vertex_map.len()` is silently **replaced** by `vertex_map[index]`,
     which points at a different, unrelated vertex.

  The parser side is not at fault — `NiSkinPartition::parse`
  (`crates/nif/src/blocks/skin.rs:299-352`) reads `Triangles` at the correct wire position and
  skips the trailing `Triangles Copy`; only the *interpretation* downstream is inverted.

- **Evidence**: measured over both vanilla SSE mesh archives
  (`/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data/Skyrim - Meshes0.bsa`
  and `Meshes1.bsa`) with a throwaway `#[ignore]` probe in `crates/nif/tests/` (since deleted).

  Proof that the indices are global, not local — every single triangle index is a **member of
  its own partition's `vertex_map` values**, which is the definition of a global index
  belonging to that partition:

  ```
  partitions_with_tris=40599 subset_of_map_VALUES_ok=40599 bad=0
  lookups=56259423 indices_that_are_map_VALUES=56259423 indices_within_map_LENGTH=48042230
  ```

  56,259,423 of 56,259,423 indices are vertex_map *values*; only 48,042,230 are within
  vertex_map's *length*. Under the local-index reading, 8,217,193 lookups (14.6%) are
  out-of-range garbage; under the global reading, zero are:

  ```
  tri_index_lookups=56259423 raw_in_range_global=56259423 raw_oob_global=0
  remap_changes_index=21306465 remap_drops=8217193
  ```

  A concrete shipped example (a partition whose `vertex_map` is a scattered global set):

  ```
  EXAMPLE meshes\actors\character\facegendata\facegeom\skyrim.esm\00045c59.nif
      part nverts_buf=996 p.num_vertices=30 map_len=30
      map[0..8]=[244, 235, 243, 245, 252, 253, 254, 258]
      tri0=Some([244, 235, 243])
  ```

  `triangles[0] == [244, 235, 243]` is literally `vertex_map[0..3]`. Under the local reading
  those indices are 8× past the end of a 30-entry map and the triangle is dropped; under the
  global reading they address vertices 244/235/243 of a 996-vertex buffer, which is exactly
  what the map says this partition covers.

  Damage on the shipped corpus (per `NiSkinPartition` block carrying a global vertex buffer):

  ```
  blocks=26913 clean_blocks=16412 damaged_blocks=10501 total_loss_blocks=0
  blk_tris=18753141 blk_dropped=3297664 blk_corrupted=6681098
  ```

  and split by the shape type that consumes it:

  ```
  BSDynamicTriShape: recon_path_shapes=21139 damaged=8637 tris=12199358 dropped=2521012
  BSTriShape:        recon_path_shapes=5801  damaged=1864 tris=6559972  dropped=776652
  ```

  The 61% that come out clean are the single-partition shapes whose `vertex_map` happens to be
  the identity permutation (`identity_maps=16551` of `40599` partitions) — for those the wrong
  remap is accidentally a no-op. The 10,203 multi-partition shapes are where it bites.

  For context, every one of these shapes reaches the reconstructor:
  `extract_bs_tri_shape` (`crates/nif/src/import/mesh/bs_tri_shape.rs:27-31`) calls
  `try_reconstruct_sse_geometry` whenever `shape.triangles.is_empty()`, and a census of all
  81,226 SSE `BSTriShape` blocks found 26,978 with empty inline triangles against 26,913
  partition blocks with a global vertex buffer — i.e. essentially all of them.

  None of this is visible to the existing gates. `cargo test -q -p byroredux-nif` is green, and
  the real-archive gate reports a perfect score:

  ```
  [Skyrim SE] parsed 32709/32709 NIFs: clean 100.00% (32709 clean / 0 truncated / 0 failed)
  ```

  The synthetic unit tests actively lock the bug in:
  `crates/nif/src/import/mesh/sse_skin_geometry_reconstruction_tests.rs:363-444`,
  `partition_vertex_map_remaps_local_indices_to_global`, fabricates `vertex_map = [2, 0, 1]`
  with triangle `[0, 1, 2]` and asserts the output is `[2, 0, 1]`; and
  `partition_triangle_with_out_of_range_vertex_map_index_is_dropped` (line 694) asserts the
  drop policy. Both are hand-built fixtures, not extracted from shipped bytes, so they
  never contradicted the archives.

- **Impact**: 10,501 of 26,940 SSE skinned shapes (39.0%) import with mangled index buffers.
  3,297,664 of 18,753,141 triangles (17.6%) are silently discarded and 6,681,098 more (35.6%)
  reference the wrong vertices — only 46.8% survive intact. The population is exactly Skyrim's
  character content: 8,637 damaged facegen head meshes (`BSDynamicTriShape`, 20.7% of their
  triangles dropped) and 1,864 damaged skinned bodies/creatures (`BSTriShape`, 11.8% dropped).
  Visually this is holed and spike-shot NPC faces and bodies, worst on the multi-partition
  meshes. No shape is lost outright (`total_loss_blocks=0`), which is why it reads as
  corruption rather than as missing content. It also poisons everything derived from the index
  list downstream: `build_triangles_for_synth` → `synthesize_tangents_yup`
  (`import/mesh/bs_tri_shape.rs:175-225`) synthesizes the tangent basis from the corrupted
  triangles, and `extract_local_bound` / BLAS construction inherit the same list.

- **Suggested Fix**: gate the remap on whether the partition is an SSE global-buffer partition.
  Inside `try_reconstruct_sse_geometry` the answer is unconditionally "yes" — the function only
  runs when `partition.global_vertex_data` is `Some`, which `NiSkinPartition::parse` populates
  only for `bsver` in `SKYRIM_SE..FALLOUT4` (`blocks/skin.rs:231-245`). So the loop should
  consume `part.triangles` directly as `u32` global indices, with a bounds check against the
  decoded buffer's vertex count (`decoded.positions.len()`) replacing the `vertex_map` lookup;
  keep the "drop the whole triangle" policy for anything genuinely out of range, which the
  corpus says is zero (`raw_oob_global=0`). Do **not** remove the `vertex_map` reads in
  `remap_bs_tri_shape_bone_indices` (`import/mesh/skin.rs:480-488`) — that one uses `vertex_map`
  correctly, as a global→partition inverse. Replace/retarget the two synthetic tests named
  above with fixtures that model the global-index layout, ideally seeded from a real
  `facegeom` partition. Fix in the same change as SKY-2026-08-27-D1-02, which currently masks
  itself against this bug.

- **Related**: checked the 300-issue dedup baseline (84 open, fetched 2026-08-27) — no open or closed issue covers SSE partition
  triangle index semantics. Not #3221/#3219/#3103/#3071 (open Skyrim), not #3176/#3177
  (tangent guards), not #3187 (slot swap). The code comment at `sse_recon.rs:104-110` cites
  #725 / NIF-D4-04 as the origin of the drop policy; that fix was applied on the inverted
  premise and its claim that "vanilla Bethesda BSAs always supply complete vertex_maps" is
  falsified by the 8.2M out-of-range lookups measured above.

---

### SKY-2026-08-27-D3-01: `parse_otft` reads only the first FormID of the `INAM` array — 765 of 1,246 Skyrim outfit items (61%) are silently dropped

- **Severity**: HIGH
- **Confidence**: CONFIRMED (code read + raw-byte walk of `Skyrim.esm` + live equip-chain trace)
- **Location**: `crates/plugin/src/esm/records/outfit.rs:73`
- **Description**:
  Skyrim+ `OTFT` carries its item list as **one `INAM` sub-record containing an array
  of 4-byte FormIDs** (xEdit `wbDefinitionsTES5.pas`: `wbArray(INAM, 'Items',
  wbFormIDCk('Item', [ARMO, LVLI]))`). The parser instead treats each `INAM`
  sub-record as a *single* FormID:

  ```rust
  b"INAM" if sub.data.len() >= 4 => {
      if let Ok(id) = SubReader::new(&sub.data).u32() {
          out.items.push(remap_fid(id, remap));
      }
  }
  ```

  `SubReader::u32()` consumes the first 4 bytes and the arm returns; bytes 4..N of the
  array are never read. Every outfit therefore yields exactly one item regardless of
  how many it authors. The length guard (`>= 4`) makes the truncation invisible — a
  20-byte 5-item `INAM` passes it and contributes one item.

- **Evidence**:
  Raw walk of the `OTFT` top-level GRUP in `Skyrim.esm` (record header 24 B,
  sub-record header 6 B, zlib-decompressing flagged records), counting `INAM`
  sub-record payload sizes:

  ```
  Skyrim.esm     OTFT=481 total item FormIDs=1246 parsed=481 DROPPED=765
                 INAM byte-size histogram {4: 94, 8: 144, 12: 131, 16: 91, 20: 19, 24: 2}
  Dawnguard.esm  OTFT=69  total item FormIDs=191  parsed=69  DROPPED=122
  HearthFires.esm OTFT=3  total item FormIDs=8    parsed=3   DROPPED=5
  Dragonborn.esm OTFT=57  total item FormIDs=161  parsed=57  DROPPED=104
  Fallout4.esm   OTFT=388 total item FormIDs=720  parsed=388 DROPPED=332
  ```

  Every one of the 481 Skyrim outfits has exactly **one** `INAM` sub-record; 387 of
  them are longer than 4 bytes. The five outfits worn by the six Bannered Mare NPCs,
  raw (full array vs. what `parse_otft` keeps — only the first entry):

  ```
  0002D75E FarmClothesOutfit02      ['000209A5', '000209A6']
  0005FB81 BarkeepClothes01         ['0005B6A1', '0005B6A0']
  00028B61 BeggarWithHatOutfit      ['00013105', '00013106', '00013104']
  000B1FAE ArmorBandedIronAllOutfit ['00013948', '00012E46', '00012E4B', '00012E4D']
  000E40DD FineClothesOutfit02      ['000CEE82', '000CEE80']
  ```

  Driving the production `build_npc_equip_state` over real `Skyrim.esm` confirms the
  loss reaches the equip state — `FarmClothesOutfit02`'s second entry (`000209A6`,
  the farm-clothes torso) is gone, so Hulda and Mikael end up with the race skin
  occupying the torso slot instead of clothing:

  ```
  Mikael:  b0=00000D64/SkinNaked  b2=00000D64/SkinNaked  b3=00000D64/SkinNaked
           b5=000877DC  b6=0001CF2B  b7=000209A5/ClothesFarmBoots02
  Hulda:   b0=00000D64/SkinNaked  b2=00000D64/SkinNaked  b3=00000D64/SkinNaked
           b5=00087832  b6=000877A7  b7=000209A5/ClothesFarmBoots02
  Sinmir:  b0=00000D64/SkinNaked  b2=00013948/ArmorIronBandedCuirass
           b3=00000D64/SkinNaked  b7=00000D64/SkinNaked      (no boots/gauntlets/helmet)
  ```

  The one fixture that should have caught this models a wire shape the game never
  emits — four separate 4-byte `INAM` sub-records, under a comment asserting it is
  real (`crates/plugin/src/esm/records/outfit.rs:105-123`):

  ```rust
  // Models a real OTFT shape: `WhiterunGuardOutfit` with helmet,
  // cuirass, gauntlets, boots referenced by INAM.
  let subs = vec![ edid("WhiterunGuardOutfit"),
      inam(0x0001_3937), inam(0x0001_3938), inam(0x0001_3939), inam(0x0001_393A) ];
  ```

  There is no `0x0008F09E` and no `WhiterunGuardOutfit` in `Skyrim.esm`. The four real
  Whiterun guard outfits each carry a single `INAM`:

  ```
  000D33C6 GuardWhiterunOutfit                 [('EDID',20),('INAM',4)]  ['000D33C7']
  00104F3F GuardWhiterunOutfitNoHelmetNoShield [('EDID',36),('INAM',8)]  ['0002150D','000A6D7F']
  0010B2EF GuardWhiterunOutfitNormalHelmet     [('EDID',32),('INAM',4)]  ['0010B2F0']
  000DD052 GuardWhiterunOutfitNoHelmet         [('EDID',28),('INAM',4)]  ['000E962D']
  ```

- **Impact**:
  Every Skyrim/FO4 NPC whose outfit authors more than one item spawns partially
  clothed. 387 of 481 Skyrim outfits (80%) are truncated; on the reference
  `WhiterunBanneredMare` bench cell that means Hulda and Mikael lose their torso
  clothing entirely and Sinmir loses boots, gauntlets and helmet from a 4-piece armour
  set. Compounds with SKY-…-D3-02 below: the lost torso item is exactly what would
  have displaced the (wrongly-resolved) race skin. Also drops 231 items across the
  three Skyrim DLC masters and 332 across `Fallout4.esm`.

- **Suggested Fix**:
  Iterate the `INAM` payload in 4-byte strides instead of reading one `u32`:
  ```rust
  b"INAM" => {
      for chunk in sub.data.chunks_exact(4) {
          let id = u32::from_le_bytes(chunk.try_into().expect("chunks_exact(4)"));
          out.items.push(remap_fid(id, remap));
      }
  }
  ```
  `chunks_exact` preserves the existing "short/trailing bytes are dropped" contract
  that `malformed_inam_short_payload_is_dropped` pins. Replace the fabricated
  `parses_outfit_with_multiple_items` fixture with the real shape — a single 16-byte
  `INAM` — and add a real-data assertion that `ArmorBandedIronAllOutfit` (`000B1FAE`)
  parses to 4 items.

- **Related**: no OPEN issue covers OTFT/`INAM`. #2079/#1996 touched this function
  (FormID remap) without noticing the array shape. Distinct from #3217 (LVLI flags).

---

### SKY-2026-08-27-D3-02: `resolve_armor_mesh` returns one ARMA mesh for a multi-part ARMO — the race skin resolves to a feet NIF, so 2,068 of 5,118 Skyrim NPCs render with no torso or hands

- **Severity**: HIGH
- **Confidence**: CONFIRMED (code read + real `Skyrim.esm` + archive presence check)
- **Location**: `crates/plugin/src/equip.rs:122` (signature), `:165` (pass 1 `return`), `:179` (pass 2 `return`); consumers `byroredux/src/npc_spawn.rs:797` (race skin) and `:911` (OTFT/CNTO items)
- **Description**:
  On Skyrim+ an `ARMO` links N `ARMA` armour addons; the ARMO's `BOD2` mask is the
  **union** of the regions those addons cover, and each addon supplies its own NIF.
  `resolve_armor_mesh` returns `Option<&'a str>` and `return`s from inside its
  race-match loop on the first hit:

  ```rust
  for &arma_fid in armatures {
      ...
      if race_match {
          if let Some(path) = pick_path(arma) { return Some(path); }   // :157
      }
  }
  ```

  For a single-region item (a cuirass, a ring) that is right. For the race default
  skin — the layer #2093 added precisely to cover the regions gear leaves bare — it is
  wrong: `SkinNaked` (`0x00000D64`, `BOD2 = 0x0000008D` = Head|Body|Hands|Feet) carries
  **25 ARMAs**, and for any given race three of them match (`NakedTorso*`,
  `NakedHands*`, `NakedFeet*`). Only the first in list order is returned.

- **Evidence**:
  `SkinNaked` armature table (excerpt) — the generic human family, matched via
  `additional_races` for Breton `0x13741`, Nord `0x13746`, Redguard `0x13748`, …:

  ```
  SKIN 00000D64 edid=SkinNaked bod2=0x0000008D armatures=25
    ...
    ARMA 00000D6E NakedFeet   race=00000019 addl=[13741,13744,13746,13747,13748,...]
       M='Actors\Character\Character Assets\MaleFeet_1.nif'
    ARMA 00000D6C NakedHands  race=00000019 addl=[ same ]
       M='Actors\Character\Character Assets\MaleHands_1.nif'
    ARMA 00000D67 NakedTorso  race=00000019 addl=[ same ]
       M='Actors\Character\Character Assets\MaleBody_1.NIF'
  ```

  `NakedFeet` precedes `NakedHands` and `NakedTorso` in the armature list, so the loop
  returns the feet mesh. Production `build_npc_equip_state` over real `Skyrim.esm`,
  all six Bannered Mare NPCs — every skin row resolves to a *Feet* NIF:

  ```
  == Saadia   fid=00013BA2 race=00013748(RedguardRace) skin=00000D64 gender=Female
     MESH fid=00000D64 hidden=0x00000004 path=Actors\Character\Character Assets\FemaleFeet_1.nif
  == Brenuin  fid=00013BA7 race=00013748  MESH 00000D64 hidden=0x00000004 ...\MaleFeet_1.nif
  == Mikael   fid=0001A670 race=00013746  MESH 00000D64 hidden=0x00000080 ...\MaleFeet_1.nif
  == Sinmir   fid=000813B5 race=00013746  MESH 00000D64 hidden=0x00000004 ...\MaleFeet_1.nif
  == AmaundMotierreEnd fid=0004E64F race=00013741 MESH 00000D64 hidden=0x00000080 ...\MaleFeet_1.nif
  == Hulda    fid=00013BA3 race=00013746  MESH 00000D64 hidden=0x00000080 ...\FemaleFeet_1.nif
  ```

  Population sweep over all 5,118 `Skyrim.esm` NPCs, using each NPC's own
  `RACE.WNAM` skin and the production equip builder:

  ```
  NPCs whose race skin still owns biped bit 2 (torso): 3445
    ...of which the queued skin mesh is a FEET/HANDS nif (or none): 2068
    {"...\FemaleFeet_1.nif": 617, "...\MaleFeet_1.nif": 1451}
  ```

  The `hidden_biped_mask` half of #2094 then misfires on the wrong NIF: Mikael, Hulda
  and Amaund get `hidden = 0x80` (feet displaced by boots) applied to a **feet** mesh,
  so `hide_skin_partitions` strips the whole thing and the skin contributes nothing at
  all; Saadia, Brenuin and Sinmir get `hidden = 0x04` (torso) applied to a feet mesh,
  which is a no-op, so their feet render but their hands never do.

  The correct meshes are present in the archives, so this is a resolver defect, not a
  missing asset:

  ```
  FOUND    meshes\actors\character\character assets\malebody_1.nif
  FOUND    meshes\actors\character\character assets\femalebody_1.nif
  FOUND    meshes\actors\character\character assets\malehands_1.nif
  FOUND    meshes\actors\character\character assets\malefeet_1.nif
  ```

  `166` of 2,762 Skyrim ARMO records have more than one ARMA serving the same race, so
  the single-return shape is wrong beyond the skin as well. No other consumer reads
  `ItemKind::Armor::armatures` — `resolve_armor_mesh` is the only path from ARMO to a
  worn mesh (`grep -rn "armatures"` over the tree).

- **Impact**:
  2,068 of 5,118 vanilla Skyrim NPCs (40%) render with bare feet where their torso
  should be and no hands. On the `WhiterunBanneredMare` bench cell all six named NPCs
  are hit; Hulda, Mikael and Amaund Motierre reduce to a FaceGen head plus jewellery
  plus boots with no body between them. #2093's stated purpose — "an NPC whose
  OTFT/CNTO doesn't cover a biped region has zero mesh source there" — is not achieved
  for the two regions (torso, hands) it most matters for.

- **Suggested Fix**:
  Change the signature to yield every matching addon rather than the first:
  `pub fn resolve_armor_meshes<'a>(...) -> Vec<&'a str>` — pass 1 collects **all**
  race-matching ARMAs with a non-empty gender mesh (dedup by path; several ARMAs can
  share one NIF), pass 2 keeps today's single "first non-empty" fallback for the
  no-race-match case. `build_npc_equip_state` pushes one `ResolvedArmor` per returned
  path, all sharing the same `inv_idx` so the #2094 `retain` and the
  `hidden_biped_mask` assignment (which already iterates `armor_to_spawn` with `find`
  — switch to `filter`) keep working unchanged. Keep a thin
  `resolve_armor_mesh` wrapper returning `.first()` if any caller wants one path.

- **Related**: #2093, #2094 (both CLOSED — the mechanisms they added are present and
  correct at HEAD; this is the layer below them). No OPEN issue covers it.

---

### SKY-2026-08-27-D6-01: `.btr` distant terrain drops the authored uniform scale on the height axis — every baked terrain quad renders at 1/`level` of its true elevation

- **Severity**: HIGH
- **Confidence**: CONFIRMED (read the code + verified against shipped Skyrim SE data)
- **Location**: `byroredux/src/cell_loader/terrain_lod_btr.rs:124-129` (`btr_local_to_world`),
  premise stated at `byroredux/src/cell_loader/terrain_lod_btr.rs:48-62` and
  `byroredux/src/cell_loader/terrain_lod_btr.rs:188-223`, enshrined by the unit test at
  `byroredux/src/cell_loader/terrain_lod_btr.rs:427-446`
- **Description**:
  The module premise is that a `.btr` is *"a normalized quad-local mesh … at the origin
  with **identity transform** — only the heights differ"*, and that heights are
  *"absolute world heights and are not scaled"*. Both halves of that premise are false
  on real data. Every shipped `.btr` geometry block carries a **uniform**
  `NiAVObject.transform.scale == level` (the quad edge in cells). The loader
  reproduces that scale by hand on X and Z only:

  ```rust
  fn btr_local_to_world(local: [f32; 3], level: i32, qx: i32, qy: i32) -> [f32; 3] {
      let lvl = level as f32;
      let ox = qx as f32 * EXTERIOR_CELL_UNITS;
      let oz = qy as f32 * EXTERIOR_CELL_UNITS;
      [ox + local[0] * lvl, local[1], local[2] * lvl - oz]   // <-- local[1] unscaled
  }
  ```

  and explicitly discards the authored transform
  (`// The mesh's own translation/rotation/scale are identity for `.btr` and deliberately ignored.`).
  The horizontal footprint therefore comes out right by accident — hand-multiplying by
  `level` happens to equal applying the authored scale — while the height axis is left
  `level`× too small. The downstream anisotropic normal/tangent fix-ups
  (`normal ∝ (nx/level, ny, nz/level)`, `tangent ∝ (tx·level, ty, tz·level)`,
  lines 205-217) are consistent with that same wrong anisotropic mapping; under a
  correct uniform scale they are no-ops and must be removed too (a uniform scale
  preserves normal directions).
- **Evidence**:
  1. **Raw wire scale** — dumping `NiAVObject.transform` off the shipped blocks:
     ```
     === meshes\terrain\tamriel\tamriel.4.-72.32.btr
       [0] BSMultiBoundNode name=Some("chunk") trans=(0,0,0) scale=1
       [1] BSTriShape        name=Some("land")  trans=(0,0,0) scale=4
       [4] BSMultiBoundNode name=Some("WATER") trans=(0,0,0) scale=1
       [5] BSSubIndexTriShape name=None        trans=(0,0,0) scale=4
     === meshes\terrain\tamriel\tamriel.32.-96.32.btr
       [1] BSTriShape        name=Some("land")  trans=(0,0,0) scale=32
       [5] BSTriShape        name=None          trans=(0,0,0) scale=32
     ```
     Scale is uniform and equals the quad level; translation is zero; the parent
     `BSMultiBoundNode`s are identity.
  2. **Per-level height ranges over all 3060 Tamriel `.btr`** — the local Y range
     halves exactly as the level doubles, the signature of heights pre-divided by the
     authored scale:
     ```
     level   4: files= 2304 scales=[4.0]  local x[0.0,4096.0] y[-9726.0,9848.0] z[-4096.0,-0.0]
     level   8: files=  576 scales=[8.0]  local x[0.0,4096.0] y[-4965.0,4924.0] z[-4096.0,-0.0]
     level  16: files=  144 scales=[16.0] local x[0.0,4096.0] y[-2540.0,2462.0] z[-4096.0,-0.0]
     level  32: files=   36 scales=[32.0] local x[-4.0,4108.0] y[-1303.5,1227.0] z[-4096.0,0.0]
     ```
     `9848·4 = 4924·8 = 2462·16 = 39392`; `1227·32 = 39264`. All four bands converge on
     the same world height range **only** when the authored scale is applied to Y.
  3. **Shared-corner cross-check** — the SW corner cell (-72, 32) is covered by both a
     level-4 and a level-8 quad. Their local heights at that corner differ by exactly
     the level ratio, and agree after scaling:
     ```
     tamriel.4.-72.32.btr  level=4  SW-corner local y = [-5934.0, -6184.0, -3500.0]   × level = [-23736.0, -24736.0, -14000.0]
     tamriel.8.-72.32.btr  level=8  SW-corner local y = [-2967.0, -3217.0, -1750.0]   × level = [-23736.0, -25736.0, -14000.0]
     ```
  4. **Independent absolute check** — the `WATER` sub-mesh vertex heights, once
     multiplied by `level`, land on authored round water heights across all 1937
     water-bearing Tamriel quads; the smallest is exactly Tamriel's WRLD `DNAM`
     default water height, which this repo already documents at
     `byroredux/src/env_translate.rs:159` (*"Tamriel -14000"*):
     ```
     distinct WATER vertex heights × level (79): [-14000.0, -13670.0, -13400.0, -13300.0,
       -13000.0, -12750.0, -12450.0, -12350.0, -12150.0, -12000.0, -11801.0, -11800.0, …]
     ```
     Unscaled these would be -3500.0, -3417.5, -3350.0 … — no relationship to any
     authored height.
  5. **The sibling loader disagrees** — `.bto` object LOD reads the same authored
     transform and applies it uniformly:
     `byroredux/src/cell_loader/object_lod.rs:320-336` does
     `let scale = mesh.scale; … Transform::new(pos, rot, scale)`. Dumping a `.bto`
     confirms the same convention (`mesh 'Obj' t=[-65536,0,-65536] s=4`, parents
     identity, translation = the quad's SW world corner). So `.bto` is correct and
     `.btr` is not, for identical authored data.
- **Impact**: Every prebaked distant-terrain quad on Skyrim SE renders at 1/4 (finest
  baked band) to 1/32 (coarsest) of its true elevation — mountains flatten toward the
  worldspace floor and the horizon collapses into a near-planar sheet. Because the
  error scales with the band, adjacent bands sit at *different* wrong elevations, so
  every band boundary is a vertical discontinuity, and the `.btr` ring meets the
  full-detail LAND terrain at a cliff at the streaming boundary. This is the whole
  M35 `.btr` feature on Tamriel: 2304 level-4 + 576 level-8 + 144 level-16 + 36
  level-32 quads for Tamriel alone, plus every DLC worldspace. The same loader is
  gated `Skyrim | Fallout4` (`combined_lod_supported`), so FO4 is on the same code
  path (not measured here — FO4 `.btr` live in a BA2 and were out of scope for this
  probe). Also flattens the mesh normals at coarse bands: dividing `nx`/`nz` by 32
  drives every level-32 normal to near `(0,1,0)`, so distant terrain shades as a flat
  plane on top of being one.
- **Suggested Fix**: Apply the authored uniform scale on all three axes —
  `world_y = local[1] * level` in `btr_local_to_world` — and delete the anisotropic
  normal (`/lvl`) and tangent (`*lvl`) corrections at
  `terrain_lod_btr.rs:205-217`, since a uniform scale leaves both unchanged. Better
  still, stop hand-rolling the mapping and consume `mesh.scale` / `mesh.translation`
  the way `object_lod.rs` already does, so the two baked-LOD loaders share one
  convention. Update the module doc (lines 48-62), which asserts identity transform
  and unscaled heights, and the unit test at lines 427-446, whose
  `assert_eq!(sw, [0.0, 10.0, 4.0 * cell])` currently pins the bug. Add a regression
  assertion on the cross-band agreement (the level-4 vs level-8 shared-corner check
  above) so the two mappings cannot drift apart again.
- **Related**: Not covered by any open issue. Checked #3336 (btr has no `Material` —
  different defect, same file), #3306 (FO4 LAND height crack — full-detail terrain,
  not LOD), #3307 / #1731 (VWD culling, explicitly out of scope). the 300-issue dedup baseline (84 open, fetched 2026-08-27)
  has no `.btr`/height/terrain-LOD-placement entry.

---

### SKY-2026-08-27-D7-01: the `ice` classifier arm is exactly inverted on Skyrim — 0 real ice surfaces reach it, 269 Imperial-fort stone walls do

- **Severity**: HIGH
- **Confidence**: CONFIRMED (code read + full-archive census of vanilla Skyrim SE meshes)
- **Location**: `crates/core/src/ecs/components/material.rs:772` (classifier ice/gem arm),
  `crates/core/src/ecs/components/material.rs:716` (`is_glass_keyword_path`),
  `crates/core/src/ecs/components/material.rs:1136` (`contains_any_ci_word`)
- **Description**:
  `classify_pbr_keyword`'s glass arm matches `ice`/`gem` through
  `contains_any_ci_word` (word-boundary), not the plain substring matcher used
  for `glass`/`crystal`. #2009 introduced that boundary to stop FO3/FNV English
  collisions (`office`, `notice`, `justice`, …). Its own in-source comment
  already notes the tension — *"Bethesda's own concatenated-compound naming
  convention (`brokenglasssheet*`) relies on the mid-word match still firing"* —
  and then applies the boundary to `ice` anyway.

  Skyrim names ice assets exactly that concatenated way: `icefrozen01`,
  `icecavewall01`, `icerock01`, `icecavesnowtrim01`, `icelakesurface`,
  `icewall01`, `icefloes`, `icevine01`, `iceberglargelod`. In every one the
  character after `ice` is alphabetic, so `after_ok` is false and the arm is
  skipped. They then fall through to the `cave`/`rock` stone arm or to the
  default-matte arm, both of which resolve **roughness 0.85, metalness 0.0**.

  In the other direction, `contains_any_ci_word` treats a *digit* as a word
  boundary (`before_ok = i == 0 || !hs[i-1].is_ascii_alphabetic()`,
  material.rs:1147). Skyrim's Imperial-fort snow-variant textures are named
  `impextwall01ice.dds` / `impwall05ice.dds` / `impextrubble01ice.dds` /
  `impextdecals01ice.dds` — `ice` preceded by a digit and followed by `.` — so
  they *do* match, and rough masonry resolves to **roughness 0.10** (glass-smooth),
  which also makes `Material::path_indicates_glass` / `is_glass_keyword_path`
  true for them.
- **Evidence**: throwaway probe over `Skyrim - Meshes0.bsa` + `Skyrim - Meshes1.bsa`,
  running the real `import_nif` path and printing each material's
  `roughness_override` (the value `translate_material` seeds and `resolve_pbr`
  clamps through unchanged):

  ```
  materials=76934  'ice' substring: word-bounded(reaches glass arm)=269 in 4 paths;
                                    NOT word-bounded(misses)=1928 in 67 paths

  -- word-bounded (glass arm fires) --
       56  textures\dungeons\imperial\impextdecals01ice.dds  rgh=Some(0.1) met=Some(0.0) kind=0
       25  textures\dungeons\imperial\impextrubble01ice.dds  rgh=Some(0.1) met=Some(0.0) kind=0
       83  textures\dungeons\imperial\impextwall01ice.dds    rgh=Some(0.1) met=Some(0.0) kind=0
      105  textures\dungeons\imperial\impwall05ice.dds       rgh=Some(0.1) met=Some(0.0) kind=0

  -- NOT word-bounded (glass arm suppressed), genuine-ice excerpt --
      283  textures\dungeons\caves\icefrozen01.dds          rgh=Some(0.85) met=Some(0.0)
      170  textures\dungeons\caves\icefrozen02.dds          rgh=Some(0.85) met=Some(0.0)
      241  textures\dungeons\caves\icecavesnowtrim01.dds    rgh=Some(0.85) met=Some(0.0)
      187  textures\dungeons\caves\icecavewall01.dds        rgh=Some(0.85) met=Some(0.0)
      100  textures\dungeons\caves\icecavewall04.dds        rgh=Some(0.85) met=Some(0.0)
      178  textures\dungeons\caves\icerock01.dds            rgh=Some(0.85) met=Some(0.0)
       88  textures\dlc01\landscape\icewall01.dds           rgh=Some(0.85) met=Some(0.0)
       77  textures\dlc01\landscape\icelakesurface.dds      rgh=Some(0.85)/0.55 (env arm)
       14  textures\dungeons\caves\icecaverocks01.dds       rgh=Some(0.85) met=Some(0.0)
       11  textures\landscape\frozenmarshice01.dds          rgh=Some(0.85) met=Some(0.0)
        8  textures\dlc01\landscape\icelakesnowcracks.dds   rgh=Some(0.85) met=Some(0.0)
        4  textures\dlc01\lod\dlc01icewalllod.dds           rgh=Some(0.85) met=Some(0.0)
        3  textures\lod\iceberglargelod.dds                 rgh=Some(0.85) met=Some(0.0)
  ```
  The 67 suppressed paths do contain genuine false positives the boundary
  correctly rejects (`riftenlattice01`, `wrwoodlattice01`, `mageapprentice\*`,
  `blacksmithnovice*`, `practicedummy01`, `sanspicedwine`, `dlc01chalice`,
  `birthsignapprentice01`, `sbitsandpices` — 145 instances). Netting those out
  leaves **1,783** instances of real ice/frozen surface, i.e. **~92% of the
  suppressed set is genuine ice, and 100% of the matched set is not ice.**
- **Impact**: Every ice cave, glacier wall, frozen lake surface, ice floe and
  Forgotten Vale / Soul Cairn ice asset in Skyrim + Dawnguard + Dragonborn
  shades as fully matte dielectric (roughness 0.85, well above `triangle.frag:2549`'s
  `roughness < 0.6` RT-reflection gate), so it receives no environment
  reflection and never reaches the glass/IOR path — ice reads as grey plaster.
  Conversely 269 Imperial-fort exterior wall/rubble/decal draws are shaded as
  mirror-smooth (0.10) and are additionally flagged as glass-keyword paths by
  `is_glass_keyword_path`, which is the alpha-gated promotion input to
  `classify_glass_into_material`. This is a wrong `Material` out of the NIFAL
  boundary, which the severity table pins at HIGH minimum.
- **Suggested Fix**: split the two matchers. `ice` needs a Skyrim-aware rule, not
  a symmetric word boundary — e.g. accept `ice` when it is a *path-component
  prefix* (`\ice…`, `dlc1ice…` filename-initial after the digits) or when it is
  followed by a known ice noun (`cave`, `wall`, `rock`, `frozen`, `lake`, `berg`,
  `floe`, `vine`, `snow`), while keeping the current boundary for the trailing
  case so `lattice` / `apprentice` / `novice` / `practice` stay rejected. Tighten
  the leading side so a digit no longer opens the boundary
  (`impwall05ice` must not match). Pin both directions with the exact vanilla
  paths above as regression cases — the current test suite has no Skyrim ice case
  at all.
- **Related**: #2009 (CLOSED, introduced the boundary), #3315 (CLOSED, the same
  class of collision on the skin arm), #3335 (OPEN — *unbounded* collisions in
  the same classifier; this is the opposite direction and is not covered by it).
  No OPEN issue mentions the ice arm.

---

### SKY-2026-08-27-D1-02: `triangle_body_parts` applies the same inverted `vertex_map` remap — currently self-cancelling, and it silently breaks SSE dismember/equip hiding the moment D1-01 is fixed

- **Severity**: MEDIUM
- **Confidence**: CONFIRMED (code-read; same wire semantics proven in D1-01)
- **Location**: `crates/nif/src/import/mesh/skin.rs:35-44` (the `vertex_map` remap inside `triangle_body_parts`, `skin.rs:16-67`)
- **Description**:
  `triangle_body_parts` builds a `canonical_triangle -> body_part` map by walking the same
  `NiSkinPartition.partitions[i].triangles` and pushing each index through `part.vertex_map`:

  ```rust
  for (dst, &local) in global.iter_mut().zip(triangle) {
      if part.vertex_map.is_empty() {
          *dst = local as u32;
      } else if let Some(&mapped) = part.vertex_map.get(local as usize) {
          *dst = mapped as u32;
      } else { valid = false; break; }
  }
  ```

  For SSE partitions this is the same inverted interpretation proven in D1-01. Today it is
  *masked*: the `final_indices` it looks its keys up against are produced by
  `try_reconstruct_sse_geometry`, which applies the identical wrong remap and skips the
  identical set of triangles, so the two sides agree and body parts land on the (mis-indexed)
  surviving triangles. The moment D1-01 is fixed in `sse_recon.rs` alone, `final_indices`
  becomes global while these keys stay remapped, no key matches, every entry falls to
  `UNASSIGNED_BODY_PART`, and the function's own trailing guard
  (`if mapped.iter().any(|&part| part != UNASSIGNED_BODY_PART)`) returns `Vec::new()`.

  The remap is still **correct** for the legacy path: `bMappedIndices` defaults to `true` and
  only flips to `false` for `Stream() == 100` (`nifly Skin.cpp:82-85`), so Oblivion/FO3/FNV
  `NiSkinPartition` triangles genuinely are vertex_map-local, and `extract_skin_ni_tri_shape`
  (`import/mesh/skin.rs:135`) routes those through the same function. The fix therefore has to
  be a version gate, not a deletion.

- **Evidence**: the code above at `skin.rs:35-44`; its two call sites at `skin.rs:135`
  (`extract_skin_ni_tri_shape`, legacy) and `skin.rs:162` (`extract_skin_bs_tri_shape`, SSE+);
  the consumer `ImportedMesh::hide_skin_partitions` at `crates/nif/src/import/types.rs:1118-1135`,
  which is a no-op unless `skin.triangle_body_parts.len() == old_triangle_count`. The SSE wire
  semantics are the same ones proven in D1-01 (nifly `Skin.cpp:82-85`, `Skin.hpp:105-109`, and
  the 56,259,423/56,259,423 subset measurement).

- **Impact**: latent today. On fixing D1-01 in isolation, `triangle_body_parts` returns empty
  for every Skyrim SE skinned mesh, so `hide_skin_partitions` stops hiding anything and the M41
  outfit-equip path renders bare body skin through every piece of armour on every NPC —
  a regression that the parse-rate gate and `cargo test -p byroredux-nif` would both pass.

- **Suggested Fix**: gate on the partition itself rather than on the game: use the raw
  `part.triangles` as global indices when `partition.global_vertex_data.is_some()` (the exact
  SSE marker `NiSkinPartition::parse` already computes), and keep the existing `vertex_map`
  remap for the `None` case (Oblivion/FO3/FNV). Land it in the same commit as D1-01 so neither
  half is ever live alone.

- **Related**: same root cause as SKY-2026-08-27-D1-01. Nothing in the 300-issue dedup baseline (84 open, fetched 2026-08-27)
  covers it. Adjacent but distinct from #3187 (slot swap).

---

### SKY-2026-08-27-D3-03: no guard pins the Whiterun Bannered Mare NPCs, and no test anywhere walks the Skyrim equip chain on real ESM data

- **Severity**: MEDIUM
- **Confidence**: CONFIRMED (exhaustive grep + full `--ignored` inventory)
- **Location**: `byroredux/src/npc_spawn/tests.rs` (whole file), `crates/plugin/tests/parse_real_esm.rs`
- **Description**:
  Checklist item 1 asks for the guard that pins the six named Bannered Mare NPCs to
  Inventory + EquipmentSlots + OTFT/LVLI-dispatched equip. **It does not exist.**
  Nothing in the tree names `saadia`, `brenuin`, `mikael`, `sinmir`, `hulda`, or
  `amaundmotierre` outside a path-formatting assertion, and no test drives
  `build_npc_equip_state` against a real ESM on any game.
- **Evidence**:
  ```
  $ grep -rn "saadia\|brenuin\|mikael\|sinmir\|hulda\|amaundmotierre" --include="*.rs" .
  byroredux/src/npc_spawn/tests.rs:169:  // Vanilla SSE Whiterun Mikael (FormID 0x00013BBE in
  byroredux/src/npc_spawn/tests.rs:172:  prebaked_facegen_nif_path("Skyrim.esm", 0x00013BBE),
  byroredux/src/npc_spawn/tests.rs:187:  prebaked_facegen_tint_path("Skyrim.esm", 0x00013BBE),
  ```
  That is `prebaked_facegen_nif_path_matches_vanilla_layout` — a pure string-format
  test on a hard-coded FormID; it never opens an ESM and asserts nothing about equip.
  (It also cites `0x00013BBE` as "Mikael" while `Skyrim.esm`'s Mikael is `0x0001A670`;
  `0x00013BBE` is some other record. Cosmetic, but the comment is wrong.)

  All eleven `build_npc_equip_state` call sites in `npc_spawn/tests.rs` construct
  synthetic `EsmIndex` fixtures (`:505 :550 :592 :782 :864 :964 :1037` …). The three
  that cover #2093/#2094 —
  `prebaked_equip_state_falls_back_to_race_skin_for_uncovered_slots`,
  `prebaked_equip_state_marks_only_partially_displaced_skin_slots`,
  `prebaked_equip_state_drops_skin_mesh_fully_displaced_by_gear` — each give the skin
  ARMO exactly **one** ARMA, which is why SKY-…-D3-02 is invisible to them. The only
  real-data NPC test in the crate is
  `npc_spawn/ai_package.rs::real_skyrim_esm_ambient_packages_now_resolve_for_previously_blind_npcs`,
  which covers PKID, not equip. The 21 `#[ignore]` tests in
  `crates/plugin/tests/parse_real_esm.rs` cover parse rates, GLOB/AVIF/CLAS/RACE/WATR
  and one FNV LVLI pin — none touch OTFT, ARMA, or the equip chain.

- **Impact**:
  Two HIGH-severity defects that a ~2 s real-data test makes obvious (the throwaway
  used for this audit parses `Skyrim.esm` in 1.8 s) shipped and survived a prior D3
  audit pass. Both are on the bench-of-record cell.
- **Suggested Fix**:
  Add one `#[ignore]` real-data test beside the existing `real_skyrim_esm_ambient_*`
  guard, using the same `BYROREDUX_SKYRIM_DATA`-with-default + self-skip convention:
  resolve the six NPCs by `editor_id`, assert all six are found, and for each assert
  (a) `Inventory` is non-empty, (b) `EquipmentSlots` occupies the expected biped bits,
  (c) the queued mesh set covers biped bit 2 with a torso NIF and bit 3 with a hands
  NIF, and (d) `ArmorBandedIronAllOutfit` contributes 4 items to Sinmir. (a) and (b)
  pass today; (c) and (d) are the ones that fail and would have caught both defects.
- **Related**: none open.

---

### SKY-2026-08-27-D4-01: a Deleted-REFR tombstone authored under a different CELL than the base placement never removes it — 5 vanilla Skyrim SE placements still spawn

- **Severity**: MEDIUM
- **Confidence**: CONFIRMED (code read + reproduced against the five shipped SSE masters)
- **Location**: `crates/plugin/src/esm/cell/mod.rs:1142` (`merge_placed_references`), fed from `crates/plugin/src/esm/cell/walkers.rs:691` and `crates/plugin/src/esm/cell/walkers.rs:160` / `crates/plugin/src/esm/cell/wrld.rs:314`
- **Description**: `parse_refr_group` correctly detects `RECORD_FLAG_DELETED` (0x20) and records the tombstoned FormID, but it pushes it into the **enclosing `CellData`'s** `deleted_refs`, and `merge_cell_references` only ever applies `over.deleted_refs` against **that same cell's** base references:

  ```rust
  fn merge_cell_references(base: &CellData, over: &mut CellData) {
      merge_placed_references(&base.references, &mut over.references, &over.deleted_refs);
  }
  ```

  In the real Creation Engine a REFR is a globally-unique FormID and deleting it removes the object wherever it lives; the cell GRUP the deletion happens to be authored under is not part of the identity. Bethesda's own DLCs routinely author the tombstone under a *different* CELL from the one the base master placed the REFR in (a REFR that was moved across a cell boundary before being deleted, or a home interior that HearthFires re-authored). When that happens the tombstone lands on `CellData` A while the live placement sits in `CellData` B, the `retain` in `merge_placed_references` matches nothing, and the object the DLC deleted still renders.
- **Evidence**: raw GRUP walk of the five masters (tombstone census), then the same five parsed through `parse_record_indexes_in_load_order` and every merged reference checked against the tombstone set. Deleted-flagged record census (raw walk, flag `0x20`):

  ```
  Skyrim.esm:        deleted-flagged total=0
  Update.esm:        total=97   [NAVM 3/244, REFR 93/9618, STAT 1/379]
  Dawnguard.esm:     total=154  [ACHR 10/908, IDLE 2/388, NAVM 59/1922, NPC_ 1/623,
                                 REFR 80/66095, SMQN 1/36, STAT 1/930]
  HearthFires.esm:   total=16   [NAVM 5/137, REFR 11/12463]
  Dragonborn.esm:    total=12   [EXPL 1, INFO 1, NAVM 1, REFR 8/78754, SPEL 1]
  _ResourcePack.esl: total=0
  ```

  Merged 5-plugin load (order `["skyrim.esm","update.esm","dawnguard.esm","hearthfires.esm","dragonborn.esm"]`):

  ```
  tombstoned REFR/ACHR FormIDs across the 5 masters = 202
  merged refs total=871810; tombstoned survivors=5
    ["SolitudeProudspireManor:0010C205", "POIReach24:00023C63",
     ":000DDD36", ":000CE60E", ":000CE60D"]
  ```

  Provenance of each survivor (raw walk, printing the enclosing children-GRUP label):

  ```
  Skyrim.esm:      REFR 0010C205 deleted=false in cell 00016A06 grup_type=9
  HearthFires.esm: REFR 0010C205 deleted=TRUE  in cell 00016778 grup_type=9
  Skyrim.esm:      REFR 000DDD36 deleted=false in cell 00007158 grup_type=9
  Update.esm:      REFR 000DDD36 deleted=TRUE  in cell 00007157 grup_type=9
  Skyrim.esm:      REFR 00023C63 deleted=false in cell 0000711B grup_type=9
  Dawnguard.esm:   REFR 00023C63 deleted=TRUE  in cell 0000713B grup_type=9
  Skyrim.esm:      REFR 000CE60D deleted=false in cell 00000D74 grup_type=8
  Skyrim.esm:      REFR 000CE60E deleted=false in cell 00000D74 grup_type=8
  Dawnguard.esm:   REFR 000CE60D deleted=TRUE  in cell 0000BC34 grup_type=9
  Dawnguard.esm:   REFR 000CE60E deleted=TRUE  in cell 0000BC34 grup_type=9
  ```

  Every one is a deletion authored under a *different* CELL than the base placement
  (and `000CE60D/E` additionally cross the persistent(8) → temporary/VWD(9) group
  boundary). What actually survives, with its base object resolved from the merged
  index:

  ```
  SURVIVOR 0010C205 in 'solitudeproudspiremanor' base=001010B3 CraftingCookingPotSm
                    [FURN] Furniture\SmallCookingPot.nif pos=[652.39,-970.58,0.00]
  SURVIVOR 000DDD36 in 'tamriel(-39, 2)'  base=0002BCA7 FXSplashSmallParticlesLong [MSTT]
  SURVIVOR 00023C63 in 'tamriel(-40, 4)'  base=00000032 COCMarkerHeading [STAT]
  SURVIVOR 000CE60E in 'tamriel<persistent>' base=000140BD pos=[135365.9,-75910.6,11338.2]
  SURVIVOR 000CE60D in 'tamriel<persistent>' base=000140BD pos=[135897.3,-76105.9,11338.2]
  ```

  197 of 202 tombstones are honoured correctly; these 5 are not.
- **Impact**: 5 vanilla-authored deletions are ignored on a full Skyrim SE + 3-DLC load.
  The user-visible one is `0010C205` — HearthFires deletes a small cooking pot from
  Proudspire Manor (Solitude player home) and it still spawns, so the reworked HF
  interior renders with a leftover/duplicate cooking pot at `[652, -971, 0]`. The other
  four are an exterior splash effect, a COC marker (invisible), and two persistent-cell
  placements. Blast radius grows with mods: any mod that relocates then deletes a
  vanilla REFR hits the same hole, and there is no workaround short of not loading the
  deleting plugin.
- **Suggested Fix**: promote the tombstone from per-`CellData` to per-plugin/global.
  Collect `deleted_refs` into an index-level `HashSet<u32>` on `EsmCellIndex` as each
  plugin merges (the FormID is already remapped by `read_record_header`), and have the
  merge — or a single post-merge sweep over `cells` / `exterior_cells` /
  `worldspace_persistent_cells` — drop any placement whose FormID is tombstoned by a
  *later-or-equal* plugin. Keep the existing per-cell path as the fast case; the
  un-delete semantics already pinned by
  `three_plugin_chain_composes_refr_merge_and_cross_plugin_delete` must still hold (a
  later plugin re-placing the FormID wins), so the global set has to be consulted in
  load order, not as a flat union. A regression test asserting these 5 FormIDs are
  absent from the merged 5-master index is the natural pin.
- **Related**: #1660 (the original tombstone skip — correct, but per-cell), #2370 EX-09/17
  item 7 (`0cef6fc0`, added `deleted_refs` + cross-plugin removal — same per-cell scope),
  #1546. No OPEN issue in the 300-issue dedup baseline (84 open, fetched 2026-08-27) covers this.

---

### SKY-2026-08-27-D6-02: the `.btr` `WATER` sub-mesh is welded into the opaque distant-terrain mesh instead of being excluded or routed to WATAL

- **Severity**: MEDIUM
- **Confidence**: CONFIRMED (read the code + verified against shipped Skyrim SE data)
- **Location**: `byroredux/src/cell_loader/terrain_lod_btr.rs:196-226`
- **Description**: `spawn_btr_block` iterates **every** sub-mesh the `.btr` imports and
  bakes them all into one vertex/index buffer uploaded as a single opaque
  `IsLodTerrain` draw sampling the per-quad land diffuse. Vanilla `.btr` are not a
  single surface: they ship a `chunk`/`land` sub-tree **and** a separate `WATER`
  `BSMultiBoundNode` carrying a flat water surface. Nothing in the loop filters it, so
  the distant water plate is rasterised as opaque ground geometry with the terrain
  diffuse bound and never reaches the water pass, while the engine *separately* draws
  a worldspace-wide LOD water frame over the same annulus
  (`byroredux/src/cell_loader/water.rs:735-868`, `spawn_lod_water_plane`, #2449).
- **Evidence**:
  ```
  === meshes\terrain\tamriel\tamriel.4.-72.32.btr   (imported)
    node[0] 'chunk' parent=None t=[0,0,-0] s=1
    node[1] 'WATER' parent=Some(0) t=[0,0,-0] s=1
    mesh[0] 'land' parent=Some(0) s=4 verts=1080
    mesh[1] None   parent=Some(1) s=4 verts=64      <-- welded into the same buffer
  ```
  Census over Tamriel: `btr files=3060 with WATER submesh=1937`
  (per level — `4: (2304 files, 1375 with water), 8: (576, 410), 16: (144, 118), 32: (36, 34)`).
  The loop that consumes them has no name/parent test:
  ```rust
  for mesh in &imported.meshes {
      if mesh.positions.is_empty() || mesh.indices.is_empty() { continue; }
      let base = vertices.len() as u32;
      ...
  }
  ```
- **Impact**: 63 % of Tamriel's baked terrain quads draw an extra opaque plate at the
  local water height, sampling the land atlas, covering the lake/sea bed underneath it
  and adding ~64 verts/quad of geometry that the renderer treats as ground. Once
  SKY-2026-08-27-D6-01 is fixed and those plates land at their true authored heights
  (-14000 for Tamriel's ocean, matching `spawn_lod_water_plane`'s `lod_height`), they
  become coplanar with the LOD water frame and will z-fight it across the whole
  distant seascape. It is also a WATAL boundary violation: a water surface that never
  reaches the water material path.
- **Suggested Fix**: Skip sub-meshes whose parent `ImportedNode` is named `WATER`
  (case-insensitive) when accumulating the land buffer — the discriminator is already
  in `ImportedScene.nodes` and needs no new parsing. Either drop them (the engine
  already owns distant water via `spawn_lod_water_plane`) or, better, hand them to
  WATAL as per-quad LOD water so lake surfaces above sea level are represented at all;
  the worldspace-wide frame is a single height and cannot express them.
- **Related**: #2449 / EXAL-01 (the LOD water frame this would collide with).
  No open issue covers the `.btr` sub-mesh split.

---

### SKY-2026-08-27-D2-01: `canonical_shader_type` translates `BSShaderType155` for the FO76 layout only, but the parser feeds Starfield the same 155 enum

- **Severity**: LOW
- **Confidence**: PLAUSIBLE (code-read only — no Starfield install on this machine to census)
- **Location**: `crates/nif/src/import/material/slot_role.rs:140-153` (`canonical_shader_type`),
  paired with `crates/nif/src/blocks/shader.rs:878-880` and
  `crates/nif/src/blocks/shader.rs:1582-1614` (`parse_shader_type_data_fo76`)
- **Description**: The parser dispatches `bsver >= FO76 (155)` — which *includes* Starfield
  (`bsver >= 172`) — to `parse_fo76_plus`, whose `shader_type` is a `BSShaderType155` value
  decoded by `parse_shader_type_data_fo76`. `parse_fo76_plus`'s own doc states this
  explicitly: *"Starfield (BSVER 172+) reuses the FO76 enum."* But the importer's
  enum-translation step only remaps when the layout is `Fallout76`:

  ```rust
  pub const fn canonical_shader_type(layout: TextureSlotLayout, raw: u32) -> u32 {
      if matches!(layout, TextureSlotLayout::Fallout76) {
          match raw {
              3 => bs_lighting::FACE_TINT,   // 4
              4 => bs_lighting::SKIN_TINT,   // 5
              5 => bs_lighting::HAIR_TINT,   // 6
              12 => bs_lighting::EYE_ENVMAP, // 16
              17 => 0,
              _ => raw,
          }
      } else { raw }
  }
  ```

  `TextureSlotLayout::from_bsver` returns `Starfield` (not `Fallout76`) for `bsver >= 172`
  (`slot_role.rs:105-113`), so Starfield raw types fall through untranslated and are then
  consumed as Skyrim `BSLightingShaderType` numbers by `slot_to_role` and by
  `info.material_kind`. `normalize_shader_type` (`dedicated_shader.rs:47-57`) masks two of
  the five divergences because the *payload* variants `Fo76SkinTint` (155 type 4) and
  `HairTint` (155 type 5) carry the tag; the three that parse to `ShaderTypeData::None`
  do not:

  | 155 raw | means | reaches slot table as | Skyrim meaning |
  |---|---|---|---|
  | 3 | Face Tint | 3 | Parallax |
  | 12 | Eye Envmap | 12 | Tree Anim |
  | 17 | Terrain | 17 | Cloud (FO76 arm degrades this to 0) |

  Consequence for a Starfield type-3 (FaceTint) property with a `BSShaderTextureSet`:
  `slot_to_role((Starfield, 3))` hits `(Skyrim | Starfield, 3) => match shader_type {
  FACE_TINT => Detail, _ => Height }` (`slot_role.rs:273-277`) and binds the head's detail
  map as a POM height field, while slot 2's `_sk` tint mask is dropped because
  `tint_family` is false — the exact failure #2694 fixed for Skyrim FaceTint.
- **Evidence**:
  - nif.xml enum split, `/mnt/data/src/reference/nifxml/nif.xml:1400` and `:1425`:
    `BSLightingShaderType` `4 = Face Tint / 5 = Skin Tint / 6 = Hair Tint / 16 = Eye Envmap`
    vs `BSShaderType155` `3 = Face Tint / 4 = Skin Tint / 5 = Hair Tint / 12 = Eye Envmap /
    17 = Terrain`.
  - `shader.rs` parse gate:
    ```rust
    let mut me = if bsver >= crate::version::bsver::FO76 {
        Self::parse_fo76_plus(stream, bsver)?
    ```
    with `FO76 = 155`, `STARFIELD = 172` (`crates/nif/src/version.rs:448,453`) — so Starfield
    takes the FO76 arm and `parse_shader_type_data_fo76`.
  - `slot_role.rs` layout gate: `if bsver >= STARFIELD { Self::Starfield } else if bsver >= FO76 { Self::Fallout76 }`.
  - Not verifiable against data here: the SteamLibrary root holds Skyrim SE, Oblivion,
    FO3 GOTY, FNV and FO4 — no Starfield install, so the instance count is **unsourced**.
- **Impact**: Starfield full-body `BSLightingShaderProperty` blocks (the ones with an empty
  `net.name`, which bypass the material-reference stub) with shader type 3/12/17 get the
  wrong canonical material kind and, if they bind a `BSShaderTextureSet`, the wrong texture
  roles. Zero impact on Skyrim, FO4, FO76 or the legacy games. The module doc at
  `slot_role.rs:17-23` asserts Starfield materials "deliberately do not enter this table"
  (their roles come from the CDB) — if that holds universally the slot half is inert, but
  the table nonetheless carries explicit `TextureSlotLayout::Starfield` arms and
  `info.shader_type` / `info.material_kind` are written regardless of whether a texture set
  exists.
- **Suggested Fix**: change the guard to
  `if matches!(layout, TextureSlotLayout::Fallout76 | TextureSlotLayout::Starfield)`, since
  the *parser* boundary (`bsver >= FO76`) is what decides which enum the integer came from —
  keep the translation keyed on the same boundary rather than on a narrower layout tag. Add
  a unit test pinning `canonical_shader_type(Starfield, 3) == FACE_TINT`, mirroring the
  existing FO76 pins in `shader_type_data_tests.rs:162-198`. Confirm against a Starfield
  corpus census before treating the routing half as user-visible.
- **Related**: #3085 (CLOSED — the FO76 sibling: slot-6 arm keyed on a Skyrim shader type
  FO76's enum cannot produce), #2579 (the FO76 remap that introduced
  `canonical_shader_type`), #2695 (single slot→role table), #2616 (established that
  Starfield reuses the FO76 wire layout). Checked the 300-issue dedup baseline (84 open, fetched 2026-08-27): no OPEN issue
  covers this.

---

### SKY-2026-08-27-D3-04: #3217's `multi_pick` narrowing has no Skyrim real-data pin, even though its entire justification is Skyrim-sourced

- **Severity**: LOW
- **Confidence**: CONFIRMED (measured on real `Skyrim.esm`; current behaviour is sane)
- **Location**: `crates/plugin/src/equip.rs:411`; tests at `:765`, `:793`, `:827`; the only real-data pin at `crates/plugin/tests/parse_real_esm.rs:2954`
- **Description**:
  Checklist item 3 asks for verification of #3217 on real Skyrim data. The narrowing
  from `flags & (0x02 | 0x04)` to `flags & 0x04` is correct at HEAD
  (`let multi_pick = lvli.flags & 0x04 != 0;`) and behaves sanely on real Skyrim data,
  but all three of its own tests are synthetic fixtures. The only real-data pin that
  exists is `fnv_leveled_item_multi_pick_semantics_are_pinned_on_the_shipped_master`
  — FNV, added by #3285 as a *side-effect* characterisation of a Skyrim-motivated
  change. The population #3217 actually names ("1,491 vanilla Skyrim NPCs") is pinned
  by nothing.
- **Evidence**: measured on `Skyrim.esm` through the production
  `expand_leveled_form_id`, at each NPC's `effective_actor_level`:
  ```
  Skyrim LVLI total=3075
    flags histogram {0:553, 1:62, 2:239, 3:1855, 4:280, 8:5, 9:1, 10:39, 11:41}
    #3217-affected (0x02 set, 0x04 clear, multi-level) = 935;  Use-All(0x04) = 280
  OTFT expansion size histogram (NPCs): {0:28, 1:3221, 2:3, 3:96, 4:229, 9:1, 24:55}
    worst: 24 items on NPC 00038451
  ```
  No combinatorial blow-up: the worst case across all 5,118 NPCs is 24 items, from an
  authored `0x04` Use-All list. 935 Skyrim records sit in the affected set — 4.7×
  FNV's 200-record floor — with zero coverage.
  (Note the `1: 3221` bucket is inflated by SKY-…-D3-01, which truncates most outfits
  to one entry before expansion ever runs; re-measure this histogram after that fix.)
- **Impact**: a future change to `expand_leveled_inner` can regress the exact
  population #3217 was written for and only the FNV pin will notice — and FNV's LVLI
  shape differs (2,700+ lists, different flag mix), so it is not a proxy.
- **Suggested Fix**: mirror `fnv_leveled_item_multi_pick_semantics_are_pinned_on_the_shipped_master`
  for Skyrim: assert the 935-record affected set has not collapsed, and pin
  `dunIronbindBeemJa`'s outfit (the record named in #3217's own fixture doc at
  `equip.rs:785`) to exactly one item at a representative level.
- **Related**: #3217 (CLOSED — fix verified present and correct at HEAD), #3285,
  #3340/#3341 (OPEN, FNV-side LVLI issues; distinct).

---

### SKY-2026-08-27-D4-02: `plugin_for_form_id` indexes the load-order list by position, not by global slot — every unresolved-REFR diagnostic names the wrong plugin once an ESL is not last, and never names an ESL at all

- **Severity**: LOW (diagnostic output only — no rendering or parse effect)
- **Confidence**: CONFIRMED (code read + reproduced with `_ResourcePack.esl`)
- **Location**: `byroredux/src/cell_loader/load_order.rs:31-34`
- **Description**: global slots are allocated by `allocate_global_slot` from **two
  independent counters** — regular plugins take `0x00..=0xFD` from `next_regular`, ESLs
  take a 12-bit sub-index in the `0xFE` space from `next_light`
  (`byroredux/src/cell_loader/load_order.rs:293-328`). The remap itself is correct
  because `build_remap_for_plugin` looks a master up by *load-order position* and then
  reads `slots[pos]`, keeping the two in step. But the diagnostic helper

  ```rust
  pub(super) fn plugin_for_form_id(form_id: u32, load_order: &[String]) -> Option<&str> {
      let mod_index = (form_id >> 24) as usize;
      load_order.get(mod_index).map(|s| s.as_str())
  }
  ```

  treats the top byte as a **load-order position**. Those two only coincide when no ESL
  precedes a regular plugin. An ESL anywhere but last shifts every later regular
  plugin's position past its slot byte, and an ESL-owned form (top byte `0xFE` = index
  254) falls off the end of the list entirely.
- **Evidence**: real 5-plugin order with the ESL in the middle
  (`skyrim.esm, update.esm, hearthfires.esm, _resourcepack.esl, dragonborn.esm` —
  `_ResourcePack.esl` declares `["Skyrim.esm","Update.esm","HearthFires.esm"]` so this
  order is legal):

  ```
  order = ["skyrim.esm","update.esm","hearthfires.esm","_resourcepack.esl","dragonborn.esm"]
  statics with top byte 0x03: [03028434, 030384C1, 030185ED]
    03028434 editor_id="DLC2EnchStalhrimGreatswordTurn05" -> plugin_for_form_id says Some("_resourcepack.esl")
    030384C1 editor_id="DLC2DweFacadeBalconyCap01_LOD"    -> plugin_for_form_id says Some("_resourcepack.esl")
    030185ED editor_id="DLC2TreePineForestStump01Ash"     -> plugin_for_form_id says Some("_resourcepack.esl")
    ESL form FE0000E4 editor_id="RP_SWellFreeStanding01CoverStaticAlpha" -> says None
    ESL form FE00014D editor_id="RP_RoadCurveLong45R01DesertLumpy01Light" -> says None
  ```

  `DLC2*` editor IDs are unambiguously Dragonborn.esm content, reported as
  `_resourcepack.esl`. ESL-owned forms report `None`, which the callers render as
  `"???"` / `"Engine.esm"`.
- **Impact**: the #561 "name the missing master" completeness guarantee is false in
  exactly the configuration it is most needed — a mixed ESM/ESL load order. The
  unresolved-base-object breakdown in
  `byroredux/src/cell_loader/references/complete.rs:268` and `:289` and the synth-child
  provenance stamps in `byroredux/src/cell_loader/references/synth_child.rs:58` / `:625`
  will point the user at the wrong plugin, sending them to add a master they already
  have. No rendering impact — the remap that actually places geometry is a separate,
  correct code path.
- **Suggested Fix**: `parse_record_indexes_in_load_order` already computes
  `slots: Vec<GlobalSlot>` parallel to `load_order`; return it (or a prebuilt
  `HashMap<GlobalSlot, String>`) alongside the name list and have `plugin_for_form_id`
  decode the FormID into a `GlobalSlot` first — `Regular(top)` for `top <= 0xFD`,
  `Light((raw >> 12) & 0x0FFF)` for `top == 0xFE` — then look that slot up. That also
  makes ESL-owned forms nameable.
- **Related**: #561 (the "name the missing plugin" requirement), #1554 (the ESL slot
  split this helper was never updated for). Existing coverage
  (`plugin_for_form_id_resolves_top_byte_to_load_order_basename`,
  `byroredux/src/cell_loader/nif_light_spawn_gate_tests.rs:268`) only exercises
  all-regular orders, which is why it stayed green.

---

### SKY-2026-08-27-D5-01: the per-file "bit 31 = embed-name toggle" semantics is unsourced and diverges from the reference implementation

- **Severity**: LOW
- **Confidence**: PLAUSIBLE (code-read + reference-impl comparison; the *data* half —
  that the bit is never set on shipped content — is CONFIRMED)
- **Location**: `crates/bsa/src/archive/open.rs:250`, `crates/bsa/src/archive/extract.rs:60`,
  doc comment `crates/bsa/src/archive/mod.rs:64-73`
- **Description**: The reader treats bit 31 (`0x80000000`) of the file record's size
  word as a *per-file embed-name override* that XORs against the archive-level
  `0x100` flag, and the doc comment states that meaning as established fact
  ("Bit 31 (0x80000000) of the on-disk size word … Mixed-mode BSAs … need this
  toggle XOR'd against the archive-level `embed_file_names`"). No spec or reference
  implementation available on this machine assigns that meaning to bit 31. openmw —
  the only full third-party BSA reader in `/mnt/data/src/reference/` — declares exactly
  one size flag and deliberately leaves bit 31 inside the size value:

  ```cpp
  // reference/openmw/components/bsa/compressedbsafile.hpp:73-76
  enum FileSizeFlags { FileSizeFlag_Compression = 0x40000000, };
  // reference/openmw/components/bsa/compressedbsafile.cpp:267
  size_t size = fileRecord.mSize & (~FileSizeFlag_Compression);
  ```

  and it drives the name skip purely off the archive flag
  (`if ((mHeader.mFlags & ArchiveFlag_EmbeddedNames) != 0)`, `compressedbsafile.cpp:271`).
  The referenced internal issue (#616 / SK-D2-03) is an audit finding, not an
  external source, so the claim currently rests on nothing citable — a direct hit on
  the project's NO-GUESSING doctrine.
- **Evidence**: I re-parsed every file record in every BSA of four installed games
  and counted the bit independently of `BsaArchive`:

  ```
  Skyrim SE   (23 archives, 172,918 files):  bit31 set on 0 files
  Fallout NV  (21 archives):                 bit31 set on 0 files
  Fallout 3   (16 archives):                 bit31 set on 0 files
  Oblivion    (17 archives):                 bit31 set on 0 files
  ```

  Bit 30 by contrast *is* exercised on real data (Oblivion
  `DLCShiveringIsles - Meshes.bsa`: 3,014 files; `- Textures.bsa`: 1,869 files;
  `Knights.bsa`: 217), so the compression XOR has genuine on-disk coverage while the
  bit-31 path has none outside the three synthetic tests at
  `archive/tests.rs:385/419`.
- **Impact**: Zero on any vanilla or Creation Club content across all four games.
  Only reachable on a third-party/modded v105 archive whose packer sets bit 31. If
  that bit means something other than "flip embed-name", the extractor consumes a
  1+N-byte bstring prefix that isn't there and returns a body shifted by that many
  bytes — a silent, non-erroring corruption (a NIF would fail its magic check
  loudly, but a DDS or a raw asset would not).
- **Suggested Fix**: Either (a) cite a real source in the doc comment (UESP
  `Skyrim Mod:Archive File Format`, BSArch, or libbsarch), or (b) match openmw and
  ignore bit 31, downgrading it to a `log::debug!` "unknown size flag set on '<path>'"
  so a real-world instance surfaces instead of being silently acted on. Keep the
  `& 0x3FFFFFFF` mask either way — it is harmless (largest single decompressed file
  across all Skyrim BSAs is 67,308,868 bytes, `shadersfx\shaders011.fxp`).
- **Related**: checked the 300-issue dedup baseline (84 open, fetched 2026-08-27) — no open or closed issue mentions
  bit 31 / `0x80000000` / the embed-name toggle. #3348 (red `--ignored` doctests) is
  unrelated and out of scope per CONTEXT.

---

### SKY-2026-08-27-D5-02: the header's folder-records offset (bytes 8..12) is never read — the folder-table walk hardcodes an implicit 36

- **Severity**: LOW
- **Confidence**: CONFIRMED (code-read + verified the field's value on all 23 shipped archives)
- **Location**: `crates/bsa/src/archive/open.rs:28-62` (header slice reads), first folder
  record read at `open.rs:118-120`
- **Description**: `BsaArchive::open` reads a fixed 36-byte header and then reads folder
  records straight out of the same `BufReader` at whatever position that left it (36).
  Bytes `[8..12]` — the folder-records offset — are the only header word other than the
  trailing file-flags that is never sliced at all, so the parser has no way to notice or
  honour an archive whose folder table does not begin immediately after the header.
  openmw explicitly seeks to it before touching the folder table:

  ```cpp
  // reference/openmw/components/bsa/compressedbsafile.cpp:67
  input.seekg(mHeader.mFoldersOffset);
  if (input.fail())
      fail("Failed to read compressed BSA folder record offset: " + ...);
  ```
- **Evidence**: A throwaway raw re-parser that printed the field whenever it was not 36
  produced no output across all 23 Skyrim SE archives, i.e. every shipped archive has
  `folders_offset == 36`. The debug-build run over Meshes0 + Textures0 + Textures7 +
  Textures8 + Misc (46,692 files) also emitted zero `"BSA folder offset mismatch"`
  warnings from the existing `#[cfg(debug_assertions)]` check at `open.rs:185-193` —
  the *per-folder* offsets are self-consistent, which is a different field and does
  not cover this one. (The logger was self-tested: a deliberate `log::warn!` at startup
  printed, so the zero-warning result is real and not a dead sink.)
- **Impact**: None on any shipped Bethesda archive. A third-party archive with a padded
  or extended header fails loudly (the folder-name length byte reads as garbage →
  `read_exact` error or an `InvalidData` from `checked_entry_count`), so this is a
  robustness/diagnosability gap, not a corruption vector.
- **Suggested Fix**: Read `header[8..12]` and `reader.seek(SeekFrom::Start(offset))`
  before the folder loop (BufReader needs `Seek`, already imported under
  `cfg(debug_assertions)` — promote it), or at minimum validate `offset == 36` and
  return a clear `InvalidData` naming the field.
- **Related**: nothing in the 300-issue dedup baseline (84 open, fetched 2026-08-27) touches the BSA header layout.
  #586 / FO4-DIM2-01 hardened the *count* fields in this same header but did not add
  the offset.

---

### SKY-2026-08-27-D6-03: the Skyrim SE parse-rate gate omits 5 vanilla-shipped BSAs (715 NIFs), the same blind spot #3041 closed for FNV

- **Severity**: LOW
- **Confidence**: CONFIRMED (read the code + ran the sweep on the omitted archives)
- **Location**: `crates/nif/tests/common/mod.rs:184`
- **Description**: `Game::mesh_archives()` returns
  `Game::SkyrimSE => &["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa"]`. A stock Steam
  Skyrim SE (Anniversary) `Data/` also ships `_ResourcePack.bsa` and four Creation Club
  archives that carry NIFs, none of which the gate opens. This is structurally the same
  hole #3041 closed for FNV (*"the gate that certifies FNV NIF parse rate 100 % clean
  was measuring a fraction of the content it claimed"*) — the fix widened the FNV list
  but Skyrim's was left at two entries.
- **Evidence**: sweeping the omitted archives with the same parse path the gate uses:
  ```
  _ResourcePack.bsa              total=149 clean=149 truncated=0 recovered=0 failed=0   (BSTreeNode=16)
  ccBGSSSE001-Fish.bsa           total=231 clean=231 truncated=0 recovered=0 failed=0
  ccBGSSSE025-AdvDSGS.bsa        total=266 clean=266 truncated=0 recovered=0 failed=0
  ccBGSSSE037-Curios.bsa         total= 65 clean= 65 truncated=0 recovered=0 failed=0
  ccQDRSSE001-SurvivalMode.bsa   total=  4 clean=  4 truncated=0 recovered=0 failed=0
  ```
  715 NIFs, all clean today — so there is no live defect hiding behind the gap, only an
  unguarded surface (including 16 `BSTreeNode` SpeedTree roots that exist nowhere in the
  gated set at that density).
- **Impact**: No current user-visible impact. A parser regression that touched only
  Creation Club / Anniversary content would not turn the Skyrim gate red, and the
  ROADMAP compat-matrix "Skyrim SE 100 % clean" figure describes 32,709 of 33,424
  shipped NIFs.
- **Suggested Fix**: Extend the `Game::SkyrimSE` arm of `mesh_archives()` to
  `["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa", "_ResourcePack.bsa",
  "ccBGSSSE001-Fish.bsa", "ccBGSSSE025-AdvDSGS.bsa", "ccBGSSSE037-Curios.bsa",
  "ccQDRSSE001-SurvivalMode.bsa"]`. `open_all_mesh_archives` already skips absent
  archives, so non-AE installs are unaffected.
- **Related**: #3041 (the FNV instance of this same gap).

---

### SKY-2026-08-27-D7-02: `material_translate.rs`'s Phase-2 module doc claims Skyrim roughness ships from Phase 2, which the Phase-2 function's own rule makes impossible

- **Severity**: LOW
- **Confidence**: CONFIRMED (code read; the two statements are mutually exclusive)
- **Location**: `byroredux/src/material_translate.rs:50-55` (claim) vs
  `byroredux/src/material_translate.rs:719-777` (`normal_alpha_spec_roughness`,
  the `if normal_has_alpha { None }` arm at :770-771)
- **Description**: The module header states:

  > *"This matters for Skyrim in particular: it has no dedicated gloss map and
  > its spec mask lives in the normal-map alpha, so for most Skyrim architecture
  > the shipped roughness comes from Phase 2, not from Phase 1's literal. Anyone
  > adding per-game material logic needs to know both write sites exist — a
  > Phase-1-only change will not stick for a field that Phase 2 also writes."*

  `resolve_normal_alpha_spec_roughness` is the only Phase-2 writer of
  `Material::roughness`, and it delegates to `normal_alpha_spec_roughness`,
  whose first branch is:

  ```rust
  if normal_has_alpha {
      None
  } else if metalness < 0.3 && env_map_scale <= 0.3 && specular_strength > 1.2 {
  ```

  with its own doc saying *"An alpha-bearing normal is deliberately a no-op here:
  its alpha is the per-pixel specular-intensity mask consumed in the shader,
  never a smoothness signal."* The header's own premise — Skyrim's spec mask
  lives in the normal-map alpha — is precisely the condition under which
  Phase 2 returns `None` and Phase 1's value ships. What Phase 2 does for that
  population is the *gloss-slot binding* (`normal_alpha_spec_binding_applies`,
  render-side), not a roughness write.
- **Evidence**: quoted above; the two doc blocks are 720 lines apart in the same
  file and assert opposite ownership of `Material::roughness` for the same
  population.
- **Impact**: The one paragraph in the codebase that tells a future contributor
  *where to change Skyrim roughness* points at the wrong write site. A
  Phase-1-only change to Skyrim architecture roughness in fact does stick; a
  Phase-2 change to it is a no-op. No runtime effect.
- **Suggested Fix**: correct the paragraph — for alpha-bearing Skyrim normals
  Phase 1 owns the scalar and Phase 2 owns only the per-draw gloss-slot binding;
  Phase 2's roughness write is the alpha-*less*, high-`specular_strength`
  fallback.
- **Related**: #1480 (the resolve-once relocation this doc describes), #2330
  (two-phase boundary). Not covered by #3188/#3236 (both CLOSED, and both about
  `nifal.md`, not this module header).

---

### SKY-2026-08-27-D7-03: `EmissiveSource::None`'s doc asserts the exact behaviour #2591 removed, and the BGEM merge is a fourth, ungated writer the helper's doc claims to cover

- **Severity**: LOW
- **Confidence**: CONFIRMED (code read + `git log` for the fix that made the doc stale)
- **Location**: `crates/core/src/ecs/components/material.rs:591-601` (stale doc),
  `crates/core/src/ecs/components/material.rs:620-640`
  (`emissive_contribution_is_authored`, claims "all three set-sites"),
  `byroredux/src/asset_provider/material.rs:1716-1718` (the ungated writer)
- **Description**: Two related contradictions on the emissive discriminator.

  (a) `EmissiveSource::None`'s doc says:

  > *"All three writers (`dedicated_shader.rs`, `legacy_properties.rs`,
  > `asset_provider/material.rs`) set their variant unconditionally once their
  > property class is bound — there is no non-zero-emissive gate, so e.g. a
  > `BSLightingShaderProperty` with `emissive_multiple == 0.0` still reports
  > `Lighting`, not `None` (#2641)."*

  Commit `aedde151` (*Fix #2589 #2590 #2591*) added exactly that gate. The three
  NIF-side sites (`dedicated_shader.rs:315` Lighting, `dedicated_shader.rs:446`
  Effect, `legacy_properties.rs:155` Material) now all guard on
  `emissive_contribution_is_authored`, and the helper's own doc 20 lines below
  says so. The `None` doc is the pre-#2591 text, still describing the behaviour
  the fix removed.

  (b) `emissive_contribution_is_authored`'s doc says it is *"Shared by all three
  `EmissiveSource::{Material,Lighting,Effect}` set-sites (#2591 / SKY-D7-03)"*.
  There are four writers, and `asset_provider/material.rs`'s BGEM merge is not
  one of them:

  ```rust
  material.emissive_color = bgem.base_color;
  material.emissive_mult = bgem.base_color_scale;
  material.emissive_source =
      byroredux_core::ecs::components::material::EmissiveSource::Effect;
  ```

  A BGEM with `base_color == [0,0,0]` or `base_color_scale == 0.0` is still
  tagged `Effect`, which is the exact degeneration ("has an effect shader"
  rather than "authored an emissive") that #2591 fixed on the other three.
- **Evidence**: `git log -S emissive_contribution_is_authored` →
  `aedde151 Fix #2589 #2590 #2591: … unconditional EmissiveSource::Lighting tag`;
  the four set-sites listed above; the two doc blocks quoted.
- **Impact**: Zero on Skyrim (no BGEM/BGEM merge on that title, and my census
  shows Skyrim's discriminator behaves per the post-#2591 rule). The behavioural
  half reaches FO4+ only, and nothing in the render path reads `emissive_source`
  today, so the practical cost is that two adjacent doc blocks on the canonical
  enum state opposite rules, and the discriminator is not uniformly meaningful
  across producers.
- **Suggested Fix**: delete/replace the stale `None` paragraph (its #2641 citation
  no longer describes HEAD); route the BGEM merge's tag through
  `emissive_contribution_is_authored` like the other three, or amend the helper
  doc to say three of four writers use it and why the fourth does not.
- **Related**: #2591 (CLOSED, the fix that made this stale), #2641 (cited by the
  stale text), #3337 (OPEN, a different claim in `nifal.md` §4).

---

## Shader-Type Coverage Matrix


Skyrim LE/SE path (`parse_shader_type_data`, `crates/nif/src/blocks/shader.rs:1389-1471`).
"nif.xml fields" is the `cond="Shader Type == N"` field set at `nif.xml:6619-6636`.
"SSE count" is the observed instance count over 22,196 vanilla SSE mesh NIFs.

| # | nif.xml name | nif.xml trailing fields | `ShaderTypeData` variant | bytes read | SSE count | parse | import | render |
|---|---|---|---|---|---|---|---|---|
| 0 | Default | — | `None` | 0 | 45,732 | OK | OK | OK |
| 1 | Environment Map | Environment Map Scale (float) | `EnvironmentMap` | 4 | 6,798 | OK | `MaterialInfo.env_map_scale` | CPU-side only (roughness classifier); no GPU `envMapScale` uniform |
| 2 | Glow Shader | — | `None` | 0 | 1,396 | OK | OK | glow via SLSF2 bit 6 + slot 2 |
| 3 | Parallax | — | `None` | 0 | 11 | OK | OK | slot 3 → `Height` |
| 4 | Face Tint | — | `None` | 0 | 3,158 | OK | tint family via `slot_to_role` (#2694) | OK |
| 5 | Skin Tint | Skin Tint Color (Color3) | `SkinTint{alpha:None}` | 12 | 1,631 | OK | `skin_tint_color` | OK |
| 6 | Hair Tint | Hair Tint Color (Color3) | `HairTint` | 12 | 10,817 | OK | `hair_tint_color` | packs into `GpuInstance` vec4 |
| 7 | Parallax Occ | Max Passes, Scale (2×float) | `ParallaxOcc` | 8 | 0 | OK (wire test `shader_tests/skyrim.rs:117`) | `parallax_max_passes` / `_height_scale` | OK |
| 8 | Multitexture Landscape | — | `None` | 0 | 0 | OK | OK | n/a |
| 9 | LOD Landscape | — | `None` | 0 | 0 | OK | OK | n/a |
| 10 | Snow | — | `None` | 0 | 0 | OK | OK | n/a |
| 11 | MultiLayer Parallax | Thickness, Refraction Scale, Inner Layer TexCoord, Envmap Strength | `MultiLayerParallax` | 20 | 662 | OK | 4 fields | `MATERIAL_KIND_MULTI_LAYER_PARALLAX 11u` |
| 12 | Tree Anim | — | `None` | 0 | 0 | OK | OK | n/a |
| 13 | LOD Objects | — | `None` | 0 | 0 | OK | OK | n/a |
| 14 | Sparkle Snow | Sparkle Parameters (Vector4) | `SparkleSnow` | 16 | 19 | OK | `sparkle_parameters` | rides on `Material`, no GPU branch yet |
| 15 | LOD Objects HD | — | `None` | 0 | 0 | OK | OK | n/a |
| 16 | Eye Envmap | Eye Cubemap Scale, Left/Right Reflection Center (Vector3 ×2) | `EyeEnvmap` | 28 | 3,251 | OK | 3 fields | OK |
| 17 | Cloud | — | `None` | 0 | 0 | OK | OK | n/a |
| 18 | LOD Landscape Noise | — | `None` | 0 | 0 | OK | OK | n/a |
| 19 | Multitexture Landscape LOD Blend | — | `None` | 0 | 0 | OK | OK | n/a |
| 20 | FO4 Dismemberment | — | `None` | 0 | 0 | OK | OK | n/a |

FO4 delta (`parse_shader_type_data_fo4`, `shader.rs:1473-1580`) — matches
`nif.xml:6619-6621` / `:6625`: type 1 appends `Use SSR` + `Wetness Control: Use SSR`
(2 bools, `#BS_FO4_2#` = BSVER 130..=139) and gates `Environment Map Scale` on
`#NI_BS_LTE_FO4#` (< 140); type 5 appends `Skin Tint Alpha` (float, `#BS_FO4_2#`).
Both bounds use `FALLOUT4..FO4_DLC_UPPER`, exactly the spec range.

FO76/Starfield delta (`parse_shader_type_data_fo76`, `shader.rs:1582-1614`): type 4 →
`Fo76SkinTint` (Color4, 16 B), type 5 → `HairTint` (Color3, 12 B), all others `None` —
matching `nif.xml:6622-6623` (`vercond="#BS_F76#"`) and the closed `BSShaderType155`
value set `{0,2,3,4,5,12,17}`.

---

## Cell-Load Regression Status

TES5 cells parse through the unified `esm/cell/` walker with **exact raw-vs-parsed parity** on
every large table. 44,153 compressed records decompress cleanly. `parse_real_skyrim_esm` is
green and finds `SolitudeWinkingSkeever` with 981 refs.

### Record parse counts on real Skyrim.esm

Method: an independent raw GRUP walk of `Skyrim.esm` (24-byte record/group headers,
`GRUP.total_size` includes its header) counting record signatures and FormIDs,
diffed against `esm::records::parse_esm`'s populated tables. "Raw" and "uniq" are
identical for every type listed — `Skyrim.esm` ships no duplicate FormIDs.

| Record | Raw in file | Parsed / indexed | Δ | Notes |
|---|---:|---:|---:|---|
| CELL (interior, top `CELL` GRUP) | 590 | 590 (`cells.cells`) | 0 | `SolitudeWinkingSkeever` present, 981 refs |
| CELL (exterior, under `WRLD`) | 16,978 | 16,942 + 36 persistent | 0 | 36 worldspace persistent cells; 37 WRLD, one has none |
| REFR + ACHR | 703,837 | 404,901 int + 280,093 ext + 18,843 persistent = **703,837** | 0 | exact |
| WRLD | 37 | 37 (`cells.worldspaces`) | 0 | |
| LAND | 15,564 | 15,564 (exterior cells with `landscape`) | 0 | VHGT ×8 delta decode, `walkers.rs:1062` |
| STAT | 9,720 | 9,712 (`cells.statics`) | −8 | the 8 are model-less engine markers — see below |
| LIGH | 435 | 435 | 0 | |
| ADDN | 89 | 89 | 0 | |
| MSTT | 691 | 691 | 0 | |
| FURN | 400 | 400 | 0 | |
| DOOR | 244 | 243 | −1 | `0010FCD9 dunTwilightSepulcherDoor` ships no `MODL` |
| TREE / FLOR / CONT / TACT | 154 / 86 / 436 / 25 | same | 0 | |
| WEAP | 2,484 | 2,484 (`items`) | 0 | 2,466 also carry a static model |
| ARMO | 2,762 | 2,762 (`items`) | 0 | |
| BOOK / ALCH / MISC / KEYM / AMMO / INGR | 821 / 363 / 371 / 334 / 35 / 94 | same | 0 | |
| LTEX | 68 | 67 (`landscape_textures`) | −1 | `00000C1E LScrub01` has `MNAM`(grass) but no `TNAM` |
| TXST | 572 | 572 | 0 | |
| NAVM | 15,966 | 15,966 | 0 | |
| HDPT | 766 | 766 | 0 | |
| NPC_ | 5,118 | 5,118 | 0 | |
| **`EsmIndex::total()`** | — | **96,322** | — | |
| compressed records (flag `0x00040000`) | 44,153 | all parsed | 0 | zlib path, `reader.rs:630` |
| deleted-flagged records (flag `0x20`) | 0 | — | — | vanilla `Skyrim.esm` ships none |

The 8 STAT records that never reach `cells.statics` are all model-less engine markers
carrying the `Is Marker` header flag and only `EDID`/`OBND`/`DNAM`:
`00000015 MultiBoundMarker`, `00000017 PlaneMarker`, `0000001F RoomMarker`,
`00000020 PortalMarker`, `00000021 CollisionMarker`, `00000067 FurnitureMarker04`,
`00000068 FurnitureMarker05`, `000000C4 WaterCurrentZoneMarker`.
`build_static_object_from_subs` (`crates/plugin/src/esm/cell/support.rs:319`) returns
`None` when there is no `MODL`, no `LightData` and no `AddonData` — correct for these.
`LScrub01` is a grass-only LTEX with no texture set to resolve. Neither is a defect.

Full 5-master merged load (`Skyrim + Update + Dawnguard + HearthFires + Dragonborn`),
for scale: 759 interior cells, 49 worldspaces, 23,322 statics, 9,947 items, 6,362 NPCs,
871,810 placed references, parsed in **3.07 s**.

---

**Control bench — NOT verified this cycle.** The Whiterun BanneredMare entity-count + FPS guard
requires launching the engine, which was ruled out for this audit (a live user instance may be
running). Static review found no change on the cell-load path that would move entity count.
The live baseline to measure against when it is next run is `ROADMAP.md:162-174`:
**5183 ent, 89.9 FPS / 11.12 ms** TAA native — *not* the superseded 3237 ent / 335.0 FPS table.

---

## Regression Guards Re-Verified Intact

These are the guards the skill flags as "any proposal to change this is a regression". All hold
at HEAD:

| Guard | Status | Evidence |
|---|---|---|
| #838 — `BSLODTriShape` routes to `NiLodTriShape`, `BSMeshLODTriShape` to `BsTriShape::parse_meshlod` | INTACT | dispatch read + checked against nif.xml inheritance (`#SKY##SSE#` vs `#FO4#`) |
| #837 — `BsLagBoneController` + `BsProceduralLightningController` dedicated parsers | INTACT | both present, no stream drift; no WARN burst on the Meshes0 sweep |
| `Material::classify_pbr` deleted; no render-time PBR classification | INTACT | single translation site, no render-time classifier |
| `MAT_FLAG_PBR_BSDF` unreachable for vanilla Skyrim | INTACT (corpus-proven) | `is_pbr` / `from_bgsm` / `material_path` are 0 across the whole SSE material universe |
| #1201 / #1202 — alpha cascade gated on `alpha_property_consumed`, two gate sites, none in `walker.rs` | INTACT | gates located; `alpha_flag_tests.rs` genuinely pins what it claims |
| #1781 — deleted-REFR doc comment in `cell/mod.rs` | NO ROT | still in sync |
| BSA compressed-file flag XOR (`compressed_by_default != compression_toggle`) | CORRECT | byte-identical to OpenMW's predicate |

---

## Pre-existing Open Issues Re-Confirmed (not re-filed)

| Issue | Status at HEAD | Note |
|---|---|---|
| #3336 | Still live | `terrain_lod_btr.rs` spawns a drawn entity with no canonical `Material`. **Worse on Skyrim than on the FNV title it was filed from** — `.btr` is Skyrim's primary distant-terrain path and is unreachable on FNV. Interacts with D6-01/D6-02. |
| #3073 | Still live | `parallax_height_scale` / `parallax_max_passes` bypass the canonical `Material` |
| #3072 | Still live | `finish_partial_import` hardcodes `furniture: None` |
| #3335 | Still live | Unbounded-substring collisions in the *same* classifier as D7-01. D7-01 is the **opposite** failure mode on the word-bounded matcher — file together. |
| #3348 | Still live | `cargo test -p byroredux-bsa -- --ignored` red from two pseudo-code doctests |
| #1731 / #3307 | Forward scope, not a gap | VWD full-model culling deliberately unwired; no z-fight possible by construction today |


---

## Appendix A — Orchestrator Independent Verification

Each CRITICAL and HIGH finding was re-measured by the orchestrator with a probe
written independently of the dimension agent's, so the confirmation does not
inherit the agent's method. Where a finding cited an external reference
implementation, that citation was checked at source. All probes were deleted
afterwards; the working tree carries none of them.

### Dimension 1

The CRITICAL finding was re-measured by the orchestrator with a separately-written
probe over `Skyrim - Meshes0.bsa` alone (18,862 NIFs), to avoid inheriting the
dimension agent's method:

```
nifs=18862  sse_partition_blocks=26708  damaged_shapes=10194  partitions=40314
tri_indices=55778313
  < vertex_map.len()   (LOCAL hypothesis)  = 47586881  (85.31%)
  member of vertex_map (GLOBAL hypothesis) = 55778313  (100.00%)
  < global vertex count                    = 55778313  (100.00%)
```

The decisive discriminator is the first row: **14.69% of triangle indices are
`>= vertex_map.len()`**, which is impossible if they were partition-local. Every
index is simultaneously in range of the global packed buffer. 10,194 of 26,708
SSE skin-partition blocks (38.2%) contain at least one such index, and those
triangles are dropped outright by the `None` arm today.

The nifly citation was also checked at source rather than taken on trust:
`/mnt/data/src/reference/nifly/src/Skin.cpp:83-85` sets `bMappedIndices = false`
for `User() >= 12 && Stream() == 100`, and `include/Skin.hpp:105-109` documents
that flag as meaning triangles index the shape's vertices rather than `vertexMap`.
`Skin.cpp:432-433` then assigns `trueTriangles = triangles` directly in that case.

Both remap sites confirmed present at HEAD: `sse_recon.rs:118-127` and
`skin.rs:35-44` (`triangle_body_parts`).

### Dimension 3

Both HIGH findings were re-measured by the orchestrator directly against
`Skyrim.esm`'s raw bytes (independent Python GRUP walker, zlib-decompressing
records with header flag `0x00040000`), not through the project's parser — so
the measurement cannot inherit the parser's own defect.

**OTFT `INAM` — CONFIRMED.**
```
OTFT records: 481
INAM payload length histogram (bytes -> count):
  {4: 94, 8: 144, 12: 131, 16: 91, 20: 19, 24: 2}
total FormIDs actually present:            1246
FormIDs the parser keeps (1 per INAM sub): 481
dropped:                                   765  (61.4%)
outfits losing >= 1 item:                  387 of 481  (80.5%)
```
Every OTFT carries **exactly one** `INAM` sub-record (481 subs for 481 records)
whose payload is an array of 1-6 FormIDs. `parse_otft`
(`crates/plugin/src/esm/records/outfit.rs:71-75`) reads `SubReader::u32()` once
per sub-record, so it keeps the first FormID and discards the rest of the array.

This also confirms the agent's point about the fixture: the existing test
`parses_outfit_with_multiple_items` builds **four separate** 4-byte `INAM`
sub-records — a wire shape `Skyrim.esm` never emits (the histogram has no
plausible way to produce 481 records with 481 single-entry subs if that shape
were real). The test passes precisely because it models the wrong wire format.

**Multi-ARMA armor — CONFIRMED.**
```
ARMO records: 2762
armature-count histogram: {1: 2060, 2: 74, 3: 453, 4: 171, 5: 1, 6: 1, 11: 1, 25: 1}
multi-ARMA armors: 702 of 2762  (25.4%)
SkinNaked = 25 armatures, SkinNakedBeast = 11
```
`resolve_armor_mesh` (`crates/plugin/src/equip.rs:122-128`) returns
`Option<&'a str>` — one path — and returns on the first race-matching ARMA,
while `ItemKind::Armor { armatures: Vec<u32> }` holds the whole list. For the
race default skin that is 1 of 25 addons. The structural defect is conclusive
from the signature alone; the specific "returns MaleFeet_1.nif / 2,068 NPCs"
consequence is the dimension agent's measurement and is reported as such.

### Dimension 6

D6-01 was re-measured with a separately-written probe over all 3,060 `tamriel.*.btr`
quads in `Skyrim - Meshes1.bsa`, reading the authored `NiAVObject.transform.scale`
off each `BSTriShape` and the raw local Z-up height range:

```
level  4: 2304 quads  distinct authored scales=[4.0]   local y=[-9726.0, 9848.0]  x level=[-38904.0, 39392.0]
level  8:  576 quads  distinct authored scales=[8.0]   local y=[-4965.0, 4924.0]  x level=[-39720.0, 39392.0]
level 16:  144 quads  distinct authored scales=[16.0]  local y=[-2540.0, 2462.0]  x level=[-40640.0, 39392.0]
level 32:   36 quads  distinct authored scales=[32.0]  local y=[-1303.5, 1227.0]  x level=[-41712.0, 39264.0]
```

Three independent confirmations fall out of this one table:

1. **The authored scale is exactly the quad level, with no exceptions** — one
   distinct value per band across all 3,060 quads.
2. **Raw heights halve as the level doubles** (9848 -> 4924 -> 2462 -> 1227), the
   signature of heights authored pre-divided by the level.
3. **Multiplying by the level makes the bands agree to the byte**:
   `9848 x 4 = 4924 x 8 = 2462 x 16 = 39392`. Identical across three bands is not
   a coincidence; it is the same Tamriel summit expressed at three resolutions.

`btr_local_to_world` (`byroredux/src/cell_loader/terrain_lod_btr.rs:124-129`) is:

```rust
[ox + local[0] * lvl, local[1], local[2] * lvl - oz]
```

X and Z are multiplied by `lvl` — i.e. the code already applies the authored scale
on two axes, and simply omits it on Y. That internal inconsistency is itself
evidence the omission is a bug rather than a deliberate convention.

CONFIRMED at HIGH.

### Dimension 7

The `ice` inversion was re-checked against shipped texture filenames (all
`Skyrim - Textures0..8.bsa`, 31,833 `.dds`), re-implementing
`contains_any_ci_word`'s boundary rule independently:

```
dds files scanned: 31833
genuinely-icy paths MISSED by the word rule: 48
   textures\actors\icewraith\icewraithbody.dds
   textures\actors\icewraith\icewraith_m.dds
   textures\actors\dlc01\dragon\icelakedragonlod.dds   (+ 45 more)
non-icy paths WRONGLY matched by the word rule: 5
   textures\dungeons\imperial\impextwall01ice.dds
   textures\dungeons\imperial\impwall05ice.dds
   textures\dungeons\imperial\impextrubble01ice.dds
   textures\dungeons\imperial\impextdecals01ice.dds
   textures\effects\gradients\gradsteamthin_ice.dds
```

The mechanism is confirmed in both directions. `contains_any_ci_word`
(`crates/core/src/ecs/components/material.rs:1136-1153`) requires a
non-alphabetic character on **both** sides, so:

* `icefrozen01`, `icewraithbody`, `icelakesurface` — `ice` is followed by a
  letter, so `after_ok` is false and the arm never fires. Bethesda's naming
  convention is concatenated compounds, which is exactly the shape this rule
  rejects.
* `impwall05ice`, `impextwall01ice` — `ice` follows a **digit** (non-alphabetic,
  so `before_ok` is true) and ends the stem, so the arm fires and Imperial-fort
  masonry shades at roughness 0.10.

Note the file counts here (48 / 5) are unique `.dds` files and are NOT the same
metric as the dimension agent's 1,783 / 269, which counts material instances
across NIFs; the two agree in direction and mechanism, and the instance counts
are the ones that matter for what is drawn.

The #2009 comment block immediately above the arm justifies the word-boundary
rule solely by English-word collisions (`office`, `notice`, `justice`, ...). That
reasoning is sound for avoiding false positives but was never checked against the
true-positive set, which is where it fails.

CONFIRMED at HIGH.

---

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_SKYRIM_2026-08-27.md
```

Label every finding `game:skyrim` + `legacy-compat`, plus its domain label
(`nif-parser`, `esm-plugin`, `import-pipeline`, `renderer`, `terrain-exterior`, `test-gap`).

**File D1-01 and D1-02 as one issue, or as two issues that explicitly block each other** — they
share a root cause and fixing either alone leaves Skyrim worse than fixing both. D7-01 should
cross-reference #3335, which is the same classifier failing in the opposite direction.
