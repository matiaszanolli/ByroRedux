# Oblivion (TES4) Compatibility Audit — 2026-08-16

**Scope**: all 7 dimensions of `/audit-oblivion` — NIF v20.0.0.5 retail body +
the v10.x NetImmerse tail, BSA v103, the live ESM path, the Oblivion render /
shader path, NIFAL canonical material translation, real-data validation, and
the exterior blocker chain. Run as part of the `comprehensive` audit-suite
sweep. **No sub-agents** — every claim below was verified directly against the
live tree, live `cargo test` runs, and live data from
`/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data/`.

**Priority assignment**: quantify the Oblivion blast radius of
`FNV-2026-08-16-D1-01` (BSXFlags bit 5 drops whole NIFs). That is reported
below as a **blast radius**, not as a re-filed finding.

---

## Executive Summary

Oblivion's own subsystems are in the best shape this audit has ever measured.
Every regression guard the skill nominates still holds; the BSA v103 sweep is
perfect; the ESM path passes every real-data parity test; and the NIF parser is
at **8,031 / 8,032 clean with zero unknown blocks across all 82 authored block
types**. The 2026-08-07 sweep left twelve Oblivion findings open and nothing
Oblivion-touching has landed since, so this cycle produced only **two new
findings**, both small.

The material news is not an Oblivion-owned defect at all — it is the Oblivion
blast radius of the cross-game `BSXFlags` bit-5 bug filed by `/audit-fnv`.
Measured on real data, it is substantially larger on Oblivion than on FNV:

> **70 vanilla Oblivion meshes carrying real geometry or collision are dropped
> outright, and those meshes are placed 5,112 times across 536 interior and
> 505 exterior cells in `Oblivion.esm` alone.** Every large torch flame in the
> game (562 placements), 914 invisible collision volumes, the swinging-mace and
> dart traps, the Oblivion-realm root havok props, the waterfall foam and
> water-plane pieces, the will-o'-the-wisp creature rig, and `obgatemini01.nif`
> — the very mesh the `#1509` regression guard is named after.

Additionally, a regression test in `byroredux/src/cell_loader/` is **named for
Oblivion and actively asserts the wrong behaviour**, so it will block the fix
(OBL-2026-08-16-BR-01 below).

Per-dimension status (every dimension enumerated, including clean ones):

| Dim | Area | New findings |
|-----|------|--------------|
| 1 | NIF version handling — v20.0.0.5 + v10.x NetImmerse tail | **0** |
| 2 | BSA v103 archive | **0** |
| 3 | ESM record coverage (live path) | **0** |
| 4 | Rendering path for Oblivion shaders | **0** |
| 5 | NIFAL canonical material translation | **0** |
| 6 | Real-data validation | **1** (LOW) |
| 7 | Exterior blocker chain & game-specific quirks | **0** |
| — | Blast-radius work (assigned) | **1** (MEDIUM) + quantification |

Totals: **2 findings — 0 CRITICAL, 0 HIGH, 1 MEDIUM, 1 LOW.**

---

## Priority Blast Radius — `FNV-2026-08-16-D1-01` on Oblivion

*Already filed against `/audit-fnv`. Reported here as measured blast radius, not
as a new finding.*

### The premise, re-verified against the authoritative spec

`/mnt/data/src/reference/nifxml/nif.xml:4298-4314` defines `BSXFlags`:

```
Bit 5 : EditorMarkers present, bEditorMarker(Skyrim)
```

"EditorMarkers **present**" — the file *contains* an editor-marker node. It is
not a statement that the file *is* a marker. The per-node name filter
`is_editor_marker` (`crates/nif/src/import/walk/mod.rs:1786-1797`) already
removes the marker nodes themselves; the file-level drop is therefore both
redundant and destructive.

### The two live drop sites

Both carry the same predicate, duplicated rather than shared:

- `byroredux/src/cell_loader/references/import.rs:84-98` — the synchronous
  REFR cell-load path.
  ```rust
  let bsx_editor_marker = bsx & 0x20 != 0 && bsver < byroredux_nif::version::bsver::FALLOUT4;
  if bsx_editor_marker { … return None; }
  ```
- `byroredux/src/cell_loader/partial.rs:58-81` — the asynchronous exterior
  streaming drain, which additionally writes a **negative cache entry**, so the
  mesh stays dropped for the rest of the session.

Oblivion NIFs carry `bsver` 6–11, all below `FALLOUT4`, so the gate always
fires.

### Measurement 1 — how many meshes are dropped

`crates/nif/examples/_tmp_obl_bsx.rs` (written for this audit) parses every NIF
in an archive, keeps those with bit 5 set and `bsver < 130`, runs the real
`import_nif_with_collision` (so the per-node name filter has already been
applied), and separates "pure marker" (0 meshes, 0 colliders) from "real
geometry".

| Archive set | NIFs | bit 5 set | pure markers | **real geometry — DROPPED** |
|---|---|---|---|---|
| `Oblivion - Meshes.bsa` | 8,032 | 73 | 29 | **44** |
| SI + Knights + 6 DLC BSAs | 1,580 | 54 | 28 | **26** |
| **Total** | 9,612 | 127 | 57 | **70** |

So 55% of the bit-5 population is real content, not markers.

### Measurement 2 — how often those meshes are actually placed

`crates/plugin/examples/_tmp_obl_bsxrefr.rs` (written for this audit) parses
vanilla `Oblivion.esm`, maps the 44 base-game dropped models onto the 78 base
records that reference them, and counts `PlacedRef` entries across every
interior and exterior cell:

```
TOTAL dropped-model REFR placements: 5112
  distinct interior cells affected: 536
  distinct exterior cells affected: 505
```

Top of the distribution (placements → mesh):

```
   914  dungeons\misc\collisionbox01.nif          (0 meshes, 1 collider — pure blocking volume)
   562  fire\firetorchlarge.nif                   (the large-torch flame)
   527  dungeons\misc\fx\fxcloudsmall01.nif
   415  dungeons\misc\roothavok05.nif
   406  dungeons\misc\roothavok01.nif
   314  dungeons\misc\roothavok06.nif
   251  dungeons\misc\roothavok02.nif
   187  dungeons\fortruins\traps\rftrapdarts01.nif
   182  landscape\miscbutterfly02.nif
   180  dungeons\misc\roothavok07.nif
   177  landscape\miscbutterfly01.nif
   157  dungeons\ayleidruins\interior\traps\artrapgasemitter01.nif
   139  dungeons\caves\traps\ctrapswingmaceshort01.nif
   120  fire\firetorchlargesmoke.nif
   119  dungeons\misc\fx\fxmistgroundeffect01.nif
   133  landscape\landscapewaterhoriz01/02/03.nif  (combined)
    62  landscape\miscmoth01.nif
    38  landscape\landscapewaterfallfoam01.nif
     3  creatures\willothewisp\skeleton.nif        (18 meshes, 17 colliders — a creature rig)
     1  oblivion\gate\obgatemini01.nif             (the #1509 morph-gate test mesh)
```

This is a floor, not a ceiling: it counts `Oblivion.esm` only. The 26 dropped
Shivering Isles / Knights meshes (`dementiastatue.nif` at 6,044 tris,
`explodingrootpod.nif`, the whole `seflamesofagnon*` family, `ndbarrier.nif`)
are placed by their own ESPs and are not in the 5,112.

### Why this is worse than the FNV number

Two of the top three entries are not cosmetic:

- `collisionbox01.nif` (914 placements) has **zero renderable meshes and one
  collider**. Dropping it removes an invisible blocking volume — a collision
  regression, not a visual one. `collisionboxstatic.nif` is the SI sibling.
- `firetorchlarge.nif` / `firetorchlargesmoke.nif` (682 combined) are the flame
  geometry on the standard wall torch, i.e. every lit dungeon corridor in the
  game renders an unlit torch bracket.

The trap family (`rftrapdarts01`, `ctrapswingmace*`, `ctrapswingloglong01`,
`artrapgasemitter01`, `argastrapgrate01`, `artrapchannelspikes01`,
`necroclawtrap01`) accounts for 578 placements of interactive geometry, all of
which carry Havok colliders.

None of the 70 dropped meshes is a `_far.nif`, so distant-object LOD is
unaffected.

### One correction for whoever fixes it

`import.rs:72-78` justifies the `bsver < FALLOUT4` carve-out by claiming
Bethesda "re-purposed" bit 5 to `MultiBoundNode` on Skyrim+/FO4. **nif.xml does
not say that** — it names the bit `bEditorMarker(Skyrim)` in the same line that
defines it as "EditorMarkers present". Under the spec reading, the era gate is
not the fix; deleting the file-level drop in both sites is, and the per-node
name filter (verified below) already does the real work.

### OBL-2026-08-16-BR-01: A regression test named for Oblivion asserts the wrong semantics and will block the fix

- **Severity**: MEDIUM
- **Dimension**: Blast-radius follow-up (Dim 7 / cell loader)
- **Location**: `byroredux/src/cell_loader/finish_partial_tests.rs:179-196`
- **Status**: NEW
- **Description**: `finish_partial_import_oblivion_bsx_bit5_is_still_editor_marker`
  asserts that a pre-FO4 NIF with `BSXFlags = 0x20` **must** produce a negative
  cache entry, with the message *"Oblivion-era BSXFlags bit 5 is a genuine
  editor marker and must still be skipped"*. That is the exact behaviour
  `FNV-2026-08-16-D1-01` identifies as wrong, and the exact behaviour the
  measurements above empirically refute (44 of 73 bit-5 Oblivion files carry
  real geometry or collision). The sibling test
  `finish_partial_import_fo4_bsx_bit5_is_not_editor_marker` (`:158-177`) pins
  the FO4 half correctly, so the pair encodes the era gate as an invariant.
- **Evidence**: `finish_partial_tests.rs:183` —
  ```rust
  fn finish_partial_import_oblivion_bsx_bit5_is_still_editor_marker() {
      let partial = dummy_partial_with(0x20, byroredux_nif::version::bsver::OBLIVION);
      finish_partial_import(&mut world, None, None, "xmarkerheading.nif", partial);
      assert!(entry.is_none(), "Oblivion-era BSXFlags bit 5 is a genuine editor marker …");
  }
  ```
  Contradicted by `nif.xml:4305` ("EditorMarkers **present**") and by the 70-mesh
  / 5,112-placement measurement above.
- **Impact**: Anyone fixing `FNV-2026-08-16-D1-01` hits a red test whose name
  and message both claim Oblivion is the case the gate "was never wrong about".
  The likely outcomes are that the fix is narrowed to exclude Oblivion (leaving
  the largest blast radius in place) or that the fixer assumes their change is
  wrong. `/audit-fnv`'s report names both drop sites but not this test.
- **Related**: `FNV-2026-08-16-D1-01`; `#2046` / `TD2-103` (the era gate this
  test was added for); the fixture `dummy_partial_with` at the same file.
- **Suggested Fix**: Retire the Oblivion half and replace it with the inverse
  guard: a pre-FO4 partial with bit 5 set **and non-empty geometry** must yield a
  positive cache entry, while a genuine marker scene (no meshes, no colliders)
  may be dropped on the zero-contribution path that already exists at
  `references/import.rs:150-162`. Land it in the same commit as the code fix.

---

## Dimension Findings

### Dimension 1 — NIF Version Handling (v20.0.0.5 + the v10.x NetImmerse tail) · **0 new findings**

Every regression guard the skill nominates was re-read in the live tree and
holds:

- `user_version` threshold — `crates/nif/src/header.rs:114`:
  `if version >= NifVersion::V10_0_1_8`. ✓
- BSStreamHeader dual-band (`#170`) — `header.rs:137-143` reproduces the
  documented band exactly (`V10_0_1_2` OR `user_version >= 3` AND
  (`V20_2_0_7` | `V20_0_0_5` | `V10_1_0_0 ..= V20_0_0_4` with
  `user_version <= 11`)). The `#170` regression test
  `bs_stream_header_not_read_for_off_spec_version` (`header.rs:643`) is present
  and asserts a non-Bethesda v20.1.0.0 / `user_version=4` file does **not** read
  the header. ✓
- v10.x band constants all present in `crates/nif/src/version.rs`: `V10_0_1_2`
  (71), `V10_0_1_8` (77), `V10_1_0_0` (79), `V10_1_0_114` (113), `V10_2_0_0`
  (116), `V20_0_0_4` (130), `V20_0_0_5` (132). ✓
- `#1509` morph gate — `crates/nif/src/blocks/controller/morph.rs:107-110`
  reads the trailing field only for `V10_2_0_0 <= version <= V20_0_0_5 && bsver
  >= MORPH_LEGACY_CUTOFF` (10, i.e. the `> 9` spelling `#2423` normalised). ✓
- `NiTexturingProperty` reads a raw `u32` count with no leading bool —
  `crates/nif/src/blocks/properties.rs:212`. ✓
- `havok_motion_type` (`#1652`) still maps the full nif.xml enum —
  `crates/nif/src/import/collision/mod.rs:222-231` (1–5|8 → Dynamic, 6 →
  Keyframed, 7 → Static, 9 → CharacterKinematic), with
  `havok_motion_type_maps_full_enum` guarding it. ✓
- Shape dispatch↔resolve parity holds: `bhkMultiSphereShape` /
  `bhkConvexListShape` dispatch at `crates/nif/src/blocks/mod.rs:1248` / `:1256`
  and resolve at `crates/nif/src/import/collision/shape.rs:110` / `:235`. ✓
- The `#1506` / `#1507` / `#1508` stride-drift family shows no regression: the
  live truncation count is **1**, below the 6-file baseline (see Dim 6).

**Open from prior sweeps, not re-filed**: `#2562` (`NiKeyframeController.Data`,
the sole remaining truncation), `#2563`, `#2564`, `#2565`, `#2566`, `#2345`.

### Dimension 2 — BSA v103 Archive · **0 findings**

Regression guard `#699` intact end-to-end.

- `crates/bsa/src/archive/open.rs:40` rejects anything outside {103, 104, 105}. ✓
- `open.rs:100` — `let folder_record_size: usize = if version == BSA_V_SKYRIM_SE { 24 } else { 16 };`
  i.e. v103 **and** v104 are 16 bytes, only v105 is 24. ✓
- `open.rs:75` — `embed_file_names` gates on `version >= BSA_V_FO3_SKYRIM`, so
  the "Xbox archive" bit several vanilla v103 archives set is correctly ignored. ✓
- Live full-archive sweep (`crates/bsa/examples/obl_sweep.rs`) over all
  **17** vanilla Oblivion BSAs:
  ```
  TOTAL files=147629 ok=147629 fail=0 (100.0000%) | nif=9612 nif_ok=9612 nif_fail=0 (100.0000%)
  ```
  Every archive reports `ver=103`. No regression.

### Dimension 3 — ESM Record Coverage (live path) · **0 new findings**

`cargo test --release -p byroredux-plugin`: **693 passed / 0 failed**.

`-- --ignored` real-data runs against vanilla `Oblivion.esm`, all green:

```
clas_oblivion_knight_against_vanilla                       ok   [OBL/CLAS] classes=111 | Knight ok | with_primaries=111
race_oblivion_data_and_subs_against_vanilla                ok   [OBL/RACE] races=15 | sane_heights=15 voices=5 hairs=12 attrs=15
parse_rate_oblivion_esm                                    ok   [OBL] total=55317 | weathers=37 climates=19 trees=142
parse_real_oblivion_esm_walker_survives                    ok
parse_real_oblivion_esm_surfaces_tamriel_worldspace        ok
oblivion_cells_populate_xcll_lighting                      ok
```

The Oblivion-specific decode branches (16-byte ACBS `#1650`, CONT 4-byte DATA
guard, CLMT three-entry WLST, MGEF-by-code map, XCLL lighting) are all exercised
by that set and are unchanged since the last sweep.

**Cross-reference, deliberately not re-filed** — `FO3-D4-01` in
`docs/audits/AUDIT_FO3_2026-08-16.md` (`PLAYER_BASE_FORM_ID = 0x0000_0014`,
`byroredux/src/inventory.rs:17`) was filed earlier in this same sweep against
FO3, FNV, Skyrim and FO4 masters. It reproduces on Oblivion too, and Oblivion is
the one title where the record has a name worth quoting. Probed with
`crates/plugin/examples/_tmp_obl_player.rs`:

```
Oblivion.esm  game=Oblivion  npcs=2482
   NPC_ 0x00000007: editor_id="Player" full="Bendu Olo" inv_entries=4 outfit=None
   NPC_ 0x00000014: ABSENT
```

Oblivion's four authored starting-inventory entries therefore never reach the
player either. Add Oblivion to `FO3-D4-01`'s evidence table when it is
published; no separate issue.

### Dimension 4 — Rendering Path for Oblivion Shaders · **0 new findings**

Live per-type histogram over `Oblivion - Meshes.bsa` (`nif_stats --tsv`,
8,032 NIFs, 82 distinct block types, **0 unknown blocks in any type**):

```
NiVertexColorProperty 4968   NiAlphaProperty 1314   NiStencilProperty 699
NiParticleSystem       547   NiBillboardNode  213   NiZBufferProperty 177
NiSpecularProperty     159   NiFogProperty     11   NiWireframeProperty  8
NiCamera                 2   NiDitherProperty   1
```

`NiTextureEffect` and `NiShadeProperty` are **not authored anywhere in vanilla
Oblivion**, so the "honored or dropped silently?" question is moot for those two
on this title.

Guards verified:

- `#869` wireframe — `crates/renderer/src/vulkan/pipeline.rs:115-133` documents
  and selects the `vk::PolygonMode::LINE` variant for `Opaque { wireframe: true }`;
  the LINE pipeline is rebuilt on resize (`context/resize.rs:204`, `:328`). Only
  8 Oblivion meshes author it. ✓
- `#869` flat shading — `INSTANCE_FLAG_FLAT_SHADING = 1 << 7`
  (`crates/renderer/src/vulkan/scene_buffer/constants.rs:254`) is pinned to the
  generated GLSL define by
  `flat_shading_bit_pinned_at_128_for_shader_constant` (`:488`). ✓
- `#1239` Oblivion `NiPSysEmitter` version gate — documented and in place at
  `crates/nif/src/blocks/particle.rs:81-89` (the pre-fix `bsver() >= 34` gate is
  named in the comment as the thing that excluded Oblivion). ✓
- `NiStencilProperty` two-sided state survives import —
  `crates/nif/src/import/material/mod.rs:770-771` plus
  `crates/nif/src/import/material/stencil_state_capture_tests.rs` (`#337`). ✓

**Verified closed since the last sweep**: `#2568` (OBL-D4-01, the legacy
pre-`NiPSys` particle stack) and `#2570`. **Still open**: `#2569` (OBL-D4-02,
the π factor between the clustered and no-cluster Lambert paths).

### Dimension 5 — NIFAL Canonical Material Translation for Oblivion · **0 new findings**

- `MAT_FLAG_PBR_BSDF` is `1 << 5` in
  `crates/renderer/src/shader_constants_data.rs:254` and mirrored as `32u` in
  the generated `crates/renderer/shaders/include/shader_constants.glsl:96`. ✓
- The `#2570` fix landed and is the missing negative test the prior sweep asked
  for: `crates/nif/src/import/material/legacy_is_pbr_tests.rs` pins `!is_pbr` for
  legacy Oblivion-shaped material trees, so the "Disney lobe is unreachable on
  Oblivion" invariant now has a guard rather than only a measurement. ✓
- The `NiSpecularProperty { flags: 0 }` ordering comment and the
  `specular_enabled` zeroing at
  `crates/nif/src/import/material/walker.rs:159-181` are unchanged.

**Still open**: `#2571`, `#2572`, `#2573` (OBL-D5-01/02/03), `#2346`.

### Dimension 6 — Real-Data Validation · **1 new finding (LOW)**

`nif_stats` over `Oblivion - Meshes.bsa`:

```
total:       8032
clean:       8031  (99.99%)
truncated:      1  (8 blocks dropped)  — meshes\marker_map.nif
failures:       0
recovered:      0
82 distinct block types, 0 unknown blocks across 0 types with partial unknown
```

This **confirms** `/audit-nif`'s 1/8032 measurement in this sweep. `ROADMAP.md:543`
still says 99.93% (8,026/8,032) and
`crates/nif/tests/data/block_coverage_baselines/oblivion_truncations.tsv:1` still
says `truncating=6` — stale in the good direction, already `#2564`.

`cargo test --release -p byroredux-nif --test per_block_baselines -- --ignored
per_block_baseline_oblivion` **PASSES** ("82 types matched"). The per-block
baseline was regenerated by `c1dd2e07` on 2026-08-08, one day after the sweep
that filed `#2574` — so **`#2574`'s premise is resolved and the issue should be
closed**, not fixed.

The three representative-mesh trace and the block-type-histogram delta both came
back clean: no new block types appeared since the last sweep, and the live
histogram is byte-identical to the checked-in per-block baseline.

#### OBL-2026-08-16-D6-01: The Oblivion truncation gate is one-directional, so a *fixed* truncation rots the baseline undetectably

- **Severity**: LOW
- **Dimension**: Real-Data Validation (Dim 6)
- **Location**: `crates/nif/tests/block_coverage_baselines.rs:153-168`
- **Status**: NEW (distinct from `#2564` — see Related)
- **Description**: The Oblivion truncation gate computes only
  `new_truncations = live \ baseline` and panics when that set is non-empty. It
  never computes `baseline \ live`. A file that *stops* truncating therefore
  leaves a permanent phantom entry that no test run will ever surface. Its
  sibling gate in the same crate — `per_block_baselines.rs` — does check
  shrinkage ("any `parsed` shrinkage is a regression"), so the two baselines in
  the same test directory disagree on whether improvement is worth pinning.
- **Evidence**: `block_coverage_baselines.rs:153-168` —
  ```rust
  let new_truncations: Vec<&String> = truncating
      .keys()
      .filter(|p| !baseline.contains(*p))
      .collect();
  if !new_truncations.is_empty() { … panic!(…) }
  ```
  Live state: the baseline lists 6 files, `nif_stats` reports 1, and the gate
  passes. It has passed in this state since `#1543`/`#1544` reduced the count,
  across at least three audit cycles.
- **Impact**: The Oblivion parse-rate figure quoted in `ROADMAP.md` and in every
  audit skill is sourced from a baseline that cannot self-correct. This is the
  mechanism that produced `#2564` and will reproduce it after `#2564` is fixed by
  regeneration. Low severity because it only ever hides *good* news — no
  regression can slip past it.
- **Related**: `#2564` (the current stale instance). `#2564` explicitly reasons
  "the gate still catches regressions (it's a superset), so nothing is broken",
  and its suggested fix is to regenerate the file — which leaves the asymmetry
  in place. This finding is the asymmetry, not the instance.
- **Suggested Fix**: Add the mirror check — collect `baseline \ live` and fail
  with a "regenerate: N file(s) no longer truncate" message, matching the
  shrinkage semantics `per_block_baselines.rs` already uses.

### Dimension 7 — Exterior Blocker Chain & Game-Specific Quirks · **0 new findings**

- The dead framings were **not** regenerated: BSA v103 decompression works
  (Dim 2, 147,629/147,629), and TES4 worldspace + LAND wiring is implemented and
  game-agnostic since `#1556`.
- `--bsa` end-to-end: the Dim 2 sweep opens, lists and extracts every Oblivion
  archive; the Dim 6 `nif_stats` run round-trips all 8,032 NIFs through that
  path.
- **`_far.nif` distant-object LOD (`#1726`/`#1745`) verified on real data.**
  `distantlod\` entries live in `Oblivion - Meshes.bsa` (not `Misc.bsa`):
  `distantlod\tamriel_30_0.lod`, `distantlod\tamriel_-48_28.lod`,
  `distantlod\toddland_2_3.lod`, `distantlod\anvilworld_*`. The builder at
  `byroredux/src/cell_loader/placement_lod.rs:183` produces exactly
  `distantlod\{w}_{cx}_{cy}.lod`, which matches. None of the 70 BSX-dropped
  meshes is a `_far.nif`, so the bit-5 bug does not touch LOD.
- **Pre-v3.3.0.13 fallback log level**: `crates/nif/src/lib.rs:380` logs the
  inline-block-type-name fallback at `log::debug!`, not `warn` — no spam risk on
  a full-archive sweep. The `warn` at `:404`/`:417` is reserved for an actual
  inline-type read failure. ✓
- Animation blocks that parse but can't play: unchanged from the known
  cell-loader gap documented at
  `byroredux/src/cell_loader/references/import.rs:123-141` (`#261` — embedded
  clips are captured on the cache entry but the cell-loader spawn path attaches
  no `Name`/subtree root for the `AnimationStack` to anchor to). Pre-existing,
  cross-game, not re-filed.

**Still open**: `#2348` (README exterior framing), `#2575` (ROADMAP entity/FPS
figure).

---

## Blocker Chain — "an Oblivion exterior cell renders"

Interiors already render end-to-end (Anvil Heinrich Oaken Halls). TES4
worldspace + LAND wiring is implemented and game-agnostic (`#1556`), and
Tamriel `(0,0)` radius 1 has been recorded at 4,886 entities / 150.6 FPS. The
remaining chain is therefore short and does not include archive or wiring work:

1. **On-device exterior render bench** on the current build (tracked by `#2377`
   / `#2368`) — the same shape FO3 was pre-bench.
2. **Fix `FNV-2026-08-16-D1-01` before the bench is taken as a baseline.** 505
   Oblivion *exterior* cells contain at least one dropped-mesh placement; a bench
   run now measures a world missing its waterfall foam, water planes,
   butterflies/moths and torch flames, and any later fix will move the numbers.
   Landing the fix first means the bench measures the real content.
3. Whatever placement / LOD gaps the bench surfaces.

---

## Regression Guard List — verified still holding this sweep

| Guard | Where | Status |
|---|---|---|
| v10.x stride-drift family `#1506`/`#1507`/`#1508` | live truncation count 1/8032, below the 6-file baseline | ✓ |
| `#1509` `NiGeomMorpherController` `bsver >= 10` gate | `blocks/controller/morph.rs:107-110` | ✓ |
| `NiTexturingProperty` raw `u32` count, no bool gate | `blocks/properties.rs:212` | ✓ |
| BSStreamHeader dual-band `#170` | `header.rs:137-143` + test `header.rs:643` | ✓ |
| `user_version` threshold `V10_0_1_8` | `header.rs:114` | ✓ |
| BSA v103 extraction `#699` | 147,629/147,629 across 17 archives | ✓ |
| `#1652` `havok_motion_type` full enum | `import/collision/mod.rs:222-231` | ✓ |
| bhk shape dispatch↔resolve parity | `blocks/mod.rs:1248/1256` ↔ `import/collision/shape.rs:110/235` | ✓ |
| Disney/PBR gate stays 0 on Oblivion | now pinned by `import/material/legacy_is_pbr_tests.rs` (`#2570`) | ✓ |
| `#869` wireframe LINE pipeline + flat-shading bit | `vulkan/pipeline.rs:115-133`, `scene_buffer/constants.rs:488` | ✓ |
| `#1239` Oblivion `NiPSysEmitter` gate | `blocks/particle.rs:81-89` | ✓ |
| `#337` `NiStencilProperty` state capture | `import/material/stencil_state_capture_tests.rs` | ✓ |
| Pre-Gamebryo inline-type fallback logs at `debug` | `crates/nif/src/lib.rs:380` | ✓ |
| Oblivion 16-byte ACBS `#1650`, CLMT WLST, XCLL, MGEF-by-code | 693/693 plugin tests + 6 real-data tests | ✓ |

---

## Candidates Investigated and Disproved

Recorded so future sweeps do not re-derive them.

1. **"The per-node `is_editor_marker` name filter also eats real Oblivion
   geometry."** Plausible given Oblivion's heavy `marker_*` naming, and it would
   have widened the BSX blast radius. **Disproved by measurement**
   (`crates/nif/examples/_tmp_obl_markername.rs`): across all 8,032
   `Oblivion - Meshes.bsa` NIFs the filter matches **95 geometry blocks under 12
   distinct names**, and every one is a genuine marker —
   `EditorMarker:0` (84 hits, all inside `frostfiregastrap.nif`-class scenes),
   plus one shape each in `marker_travel` / `marker_horse` / `marker_prison` /
   `marker_arrow` / `marker_light` / `marker_north` / `marker_sound` /
   `marker_error`. No false positives. The name filter is safe to keep as the
   sole marker suppression once the file-level BSX drop is removed.

2. **"`#2574` (per-block baseline stale, gate FAILS) is still live."** The
   opt-in gate now **passes** — regenerated by `c1dd2e07` on 2026-08-08. Not
   re-filed; recommend closing the issue instead.

3. **"The `PLAYER_BASE_FORM_ID` bug is an Oblivion-specific finding."** It is
   cross-game and was already filed as `FO3-D4-01` earlier in this same sweep.
   Reported as a cross-reference with added Oblivion evidence, not re-filed.

4. **"`NiTextureEffect` / `NiShadeProperty` are silently dropped on Oblivion."**
   Neither block type appears anywhere in the 82-type vanilla Oblivion histogram,
   so there is nothing to drop.

5. **"WATR `wind_speed` / Oblivion fog-colour offsets."** Known and unfixed per
   the standing memory note; this sweep produced no new specific evidence, so it
   is deliberately not reported.

---

## Scratch Artifacts

Per-dimension notes: `/tmp/audit/oblivion/dim_1.md` … `dim_7.md`, plus
`dim_blast.md`. Raw measurement output: `bsx_obl.txt`, `bsx_dlc.txt`,
`bsa_sweep.txt`, `nif_stats.txt`, `hist.tsv`, `dropped_list.txt`.

Four throwaway probes were written for this audit and left in the tree for
reproduction (same convention as the existing `_tmp_obl_d4_*` / `_tmp_obl_d5_*`
examples):

- `crates/nif/examples/_tmp_obl_bsx.rs` — BSX bit-5 drop census
- `crates/nif/examples/_tmp_obl_markername.rs` — name-filter false-positive census
- `crates/plugin/examples/_tmp_obl_player.rs` — player base-record FormID probe
- `crates/plugin/examples/_tmp_obl_bsxrefr.rs` — dropped-mesh REFR placement count

---

Report ready. Suggested next step:

```
/audit-publish docs/audits/AUDIT_OBLIVION_2026-08-16.md
```
