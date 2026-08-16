# Starfield Compatibility Audit — 2026-08-16

*Run as part of the `comprehensive` audit-suite sweep. All 9 dimensions
covered. Real game data available at
`/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` (129 archives,
89,276 vanilla NIFs, the 1,389 MB `Starfield.esm`); every dimension ran
against it.*

## Executive Summary

Starfield remains a first-class `GameKind`: BA2 v2/v3 (zlib + LZ4 block, GNRL +
DX10) extracts at 100%, all 89,276 vanilla NIFs parse with zero truncation and
six known `BSWeakReferenceNode` recoveries (#2105, unchanged), the CDB presence
gate + DLC/Creation discovery work, and the Cydonia resolve rate holds at
**91.2% (25,437/27,898)** — byte-identical to the 2026-08-13 `/audit-esm`
baseline. **Every regression guard this audit was asked to check passed.**

### The priority lead, answered

The sweep's lead was that the 2026-08-14 texture-role unification (#2695)
collapsed the per-game slot tables into one Skyrim-measured
`slot_to_role(shader_type, slot, model_space_normals)`, with no `bsver` gate,
and that Starfield's CDB `.mat` — as "the second-densest role producer" — would
therefore be mis-routed.

**On Starfield the table is not mismapped. It is unreachable, and the CDB is a
zero-role producer.** All three candidate producers are structurally dead on
vanilla content, measured:

| Producer | Starfield status | Evidence |
|---|---|---|
| NIF material walker | dead | **0** `BSShaderTextureSet` blocks in 89,276 NIFs; **0 of 2,538** full-body `BSLightingShaderProperty` carries a non-NULL `texture_set_ref`; 403,562 of 406,100 (99.4%) are material-reference stubs that return before the slot loop |
| REFR texture overlay | dead | `Starfield.esm`: **XATO = 0, XTXR = 0, XMSP = 0** across **1,971,135 REFRs** in 11,985 interior cells; only 21 TXST records exist at all |
| CDB `.mat` | dead | the `.mat` arm of `merge_external_material` sets `is_pbr` and returns `PresenceOnly` **without touching `material.textures`** |

Derived from 390,766 imported meshes: `ImportedMaterial.shader_type == 0` on
**100%**, `model_space_normals == false` on **100%**. So even a hypothetical
override would resolve against the Skyrim "Default" column. Two plausible
Starfield mismappings were chased and **disproved on data** (Dimension 8).

### What this pass did find

**Six new findings: 0 CRITICAL, 1 HIGH, 3 MEDIUM, 2 LOW.**

The one that matters is SF-D9-01. Vanilla Starfield NIFs reference **234
distinct `.bgsm`/`.bgem` material paths across 1,639 shader properties** — on
hangar exteriors, New Atlantis lobby walls, ship interiors, mine caves and
weapon receivers — and **not one of those files exists in any of the 129
Starfield archives**. Because the CDB PBR gate is keyed on the `.mat` suffix,
those shapes are excluded from the Disney-BSDF routing their `.mat` neighbours
on the same mesh receive, fall to legacy Lambert on parser-placeholder
constants, and produce no log line at all on the way.

Behind it: the process-global CDB byte cache holds up to **233 MB** for the
process lifetime to answer a boolean (SF-D3-01); `ComponentDatabaseFile::parse`
peaks at **9.19 GB RSS** on the vanilla 105 MB CDB with no ceiling, and is the
exact entry point #2359's Phase 2 is documented to call (SF-D3-02); and
Starfield `ARMO` `MODL` is a fixed-width 4-byte payload the parser mislabels as
"corrupt", 1,480 WARN lines per ESM parse (SF-D5-01).

---

## Dimension Findings

| Dimension | New findings |
|---|---|
| 1 — BA2 v2/v3 LZ4 block decompression | 0 |
| 2 — BSGeometry mesh extraction | 0 |
| 3 — CDB material database correctness | **2** (MEDIUM ×2) |
| 4 — Starfield ESM resolve-rate baseline | 0 |
| 5 — ESM + cell bring-up regression surface | **2** (MEDIUM, LOW) |
| 6 — NIF shader blocks, BSVER 155+ | 0 |
| 7 — Real-data validation | 0 |
| 8 — NIFAL canonical material translation | **1** (LOW) |
| 9 — BGSM/BGEM external material flow | **1** (HIGH) |

---

### Dimension 1 — BA2 v2 / v3 LZ4 block decompression — 0 findings

Re-verified against the current tree: the version match in
`crates/bsa/src/ba2.rs:222-260` is exhaustive over `{1,2,3,7,8}`; v2 reads an
8-byte extension and v3 reads 8 + a 4-byte `compression_method`; an unsupported
method is a hard `InvalidData` return at `:245-254`, not a fall-through; and
per-chunk raw-vs-compressed selection is `packed_size == 0` in **both**
`extract_general` and `extract_dx10`, so v3's mixed raw/LZ4 mip chunks resolve
per chunk. The only touch since the last audit (`b3a53e56`) is `cargo fmt`
reflow inside `mod tests`.

Sweep over all 129 Starfield archives (`--per 120` stratified sample):
**12,674 extractions, 0 failures, 0 open failures.** v3 appears on exactly the
13 texture archives (`Textures01..11`, `TexturesPatch01/02`) plus
`LODTextures01/02`; everything else is v2.

Open, not re-filed: #2360, #2097, #2584, #2585.

### Dimension 2 — BSGeometry mesh extraction — 0 findings

**100% of vanilla Starfield geometry is the external `.mesh` path**: 358,068
external blocks, **0 internal**, 0 no-slots. At block level, **66,224 / 66,224**
external blocks resolve — 0 resolver misses, 0 parse errors, 0 sentinel bodies —
so #1292 (`geometries\` head untouched), #1209 and #1828/#1829 (LOD-slot
iteration with sentinel skip) all hold. Drop one archive from the chain and 978
misses appear, confirming every observed miss is cross-archive rather than a
path-convention failure. Survivor attribute coverage is 341,877/341,877 for
normals, UVs, **tangents** (#1232) and colours, with 5,774 skinned (#1203).

LOD is fine despite `Starfield - LODMeshes.ba2` shipping zero `.mesh` files —
its NIFs resolve 42,318/42,538 (99.5%) against the shared `geometries\` hash
tree in the mesh archives.

Two candidates were disproved — see *Disproved Candidates* below.

Open, not re-filed: #2361, #2362. Note the 100%-external measurement makes
#2362 a *total* import failure at those four call sites for Starfield, not a
partial one; that is worth recording on the issue.

### Dimension 3 — CDB material database correctness — 2 findings

Already-fixed and verified: #2705 (`sf_cdb_cache` at module scope so a provider
rebuild is a refcount bump), #2706 (the `sf_cdbs` doc claim removed from
`byroredux/src/app_step.rs`), #1571 (`discover_starfield_cdbs` still scans for
every `materials\...materialsbeta.cdb`), and the `peek_magic` → `probe_header`
ordering in `register_starfield_cdb`.

Measured across all 129 archives: **13 distinct CDBs, 233,395,272 bytes
inflated** — base `materials\materialsbeta.cdb` at 105,037,616 B, a nearly-equal
`materials\creations\sfbgs007\materialsbeta.cdb` at 104,868,172 B, ShatteredSpace
at 10,615,315 B, and ten creations at ~1.0–2.4 MB each.

#### SF-D3-01: `sf_cdb_cache` is an uncapped, never-evicted process-lifetime hold of up to 233 MB
- **Severity**: MEDIUM
- **Dimension**: 3 — CDB material database correctness
- **Location**: `byroredux/src/asset_provider/material.rs:135-210`
- **Status**: NEW
- **Description**: #2705 fixed a real cost (re-inflating a 105 MB blob on every
  cell transition / save-load / debug-load) by promoting the cache to module
  scope. But the cache stores the **fully inflated CDB bytes**, keyed
  `"<archive source>|<in-archive path>"`, and the only consumer of those bytes
  is `register_starfield_cdb`, which reads a 4-byte magic and an 8-byte header.
  There is no eviction, no cap, and no `clear` call site anywhere in the tree
  (`grep -rn sf_cdb_cache` returns only the definition, the two `discover`
  accesses, three doc references and one test).
- **Evidence**: `pub(super) fn sf_cdb_cache() -> &'static Mutex<HashMap<String,
  Arc<[u8]>>>` (`material.rs:148-152`), inserted at `:194` and never removed.
  Measured payload: 233,395,272 B across the 13 CDBs an install with the
  Creations set will discover; base + SFBGS007 alone is 209.9 MB.
- **Impact**: up to ~233 MB of resident RAM held for the process lifetime to
  answer `has_starfield_cdb() -> bool`. It is bounded (by the number of distinct
  CDBs), so it is not a leak — but it is the largest single non-GPU allocation
  in a Starfield session and it buys nothing today.
- **Related**: #2705 (the fix that introduced this shape), #2359 (the Phase 2
  work whose future need is the stated justification), #2621.
- **Suggested Fix**: cache the probe *result*, not the bytes — a
  `HashMap<String, CdbHeaderInfo>` (or a bare `HashSet<String>`) collapses 233 MB
  to a few hundred bytes while keeping the re-extract avoidance. Promote to
  byte-caching only when Phase 2 actually needs the payload, and give it a cap
  then.

#### SF-D3-02: `ComponentDatabaseFile::parse` peaks at 9.19 GB RSS on the vanilla CDB, with no ceiling
- **Severity**: MEDIUM
- **Dimension**: 3 — CDB material database correctness
- **Location**: `crates/sfmaterial/src/reader.rs:30`
- **Status**: NEW
- **Description**: the full CDB parse expands a 105 MB file into an owned
  `Vec<Value>` instance tree with no instance-count, depth, or allocation
  ceiling. Measured on the real vanilla database it peaks at **9,188,820 kB
  (9.19 GB)** resident — an 87× blow-up — for 1,438,780 top-level instances in
  4.75 s. `parse` is `pub`, has no production caller today (`register_starfield_cdb`
  deliberately uses `probe_header` instead), and is exactly the entry point the
  `.mat` arm's own comment says Phase 2 "should still *overwrite* … with
  CDB-authored data when a lookup succeeds".
- **Evidence**: `/usr/bin/time -v` on `crates/sfmaterial/examples/_tmp_d3_parse.rs`
  → `cdb bytes = 105037616` / `parsed in 4.747618077s: 97 classes, 1438780
  top-level instances` / `Maximum resident set size (kbytes): 9188820`. The
  reader's own `ComponentDatabaseFile` doc confirms `instances: Vec<Value>` is a
  flat retained list with no path index.
- **Impact**: any consumer that calls `parse` on vanilla Starfield content —
  Phase 2, a tool, a mod path — takes a 9.19 GB allocation spike. On a machine
  without that headroom it is an OOM, not a slow load. The existing safety work
  (#2100/#2101/#2102/#2614) hardened the *header* and per-chunk reserves; the
  instance tree itself is unbounded.
- **Related**: #2359 (Phase 2), #2614, #2633, SF-D3-01.
- **Suggested Fix**: before Phase 2 lands, give `parse` a caller-supplied
  instance/allocation budget, or add a streaming/indexed variant that resolves a
  single material path without materialising the whole tree. Record the measured
  87× factor in `docs/engine/memory-budget.md`.

### Dimension 4 — Starfield ESM resolve-rate baseline — 0 findings

`--sf-smoke citycydoniamainlevel` against `Starfield.esm`:
**25,437 / 27,898 resolved (91.2%)**, unresolved 2,461 (8.8%), all in master
slot 0x00. By-type: STAT 22,758 · LIGH 656 · MSTT 466 · MISC 454 · PKIN 370 ·
FURN 292 · ACTI 130 · IDLM 95 · ALCH 93 · DOOR 41 · CONT 37 · FLOR 25 · TERM 8 ·
BOOK 6 · ARMO 4 · WEAP 2. **Byte-identical to the 2026-08-13 `/audit-esm`
baseline — no regression.** The #1567 LIGH-`DAT2` guard holds (the 656 lights
are the same figure that fix restored), and #1568's PDCL conscious skip emits
exactly one named WARN rather than vanishing into the catch-all.

Open, not re-filed: #2637.

### Dimension 5 — ESM + cell bring-up regression surface — 2 findings

Verified clean: `XCLL_SIZES_STARFIELD: &[usize] = &[28, 108]`
(`crates/plugin/src/esm/cell/walkers.rs:57`, selected for `GameKind::Starfield`
at `:95`, #1291); #1294's `base_layer` gate
(`byroredux/src/cell_loader/spawn/mesh_instance.rs:750,771`); #1295's
`DoorTeleport` stamping in `byroredux/src/cell_loader/spawn.rs`; #1284's
`SkinSlotPool` cap-sizing in
`crates/core/src/ecs/resources/skin_slot_pool.rs`; #1568's named PDCL skip.

#### SF-D5-01: Starfield ARMO `MODL` is a fixed-width 4-byte payload, mislabelled "corrupt" on 848 forms / 1,480 WARNs per parse
- **Severity**: MEDIUM
- **Dimension**: 5 — ESM + cell bring-up regression surface
- **Location**: `crates/plugin/src/esm/cell/support.rs:61-74`
- **Status**: NEW
- **Description**: `build_static_object_from_subs` runs `read_mesh_path` (a
  NUL-terminated-string reader) on every `MODL` sub-record. Starfield `ARMO`
  does not store a mesh path there: it stores a repeated
  `INDX` (2 B) + `MODL` (4 B) pair — an indexed armor-addon list. Every one of
  those 4-byte payloads fails the string check and is reported as
  `#1620 — ARMO XXXXXXXX: corrupt MODL mesh path (control bytes)` at WARN.
- **Evidence**: `dump_record_subs Starfield.esm ARMO 0x00000D64` (`Skin_Naked`):
  ```
  10 INDX len= 2 hex=[00, 00]
  11 MODL len= 4 hex=[04, 22, 01, 00]
  12 INDX len= 2 hex=[00, 00]
  13 MODL len= 4 hex=[80, 05, 03, 00]
  ```
  A full `--sf-smoke` run emits **1,480** such warnings from **848 distinct ARMO
  form IDs**, and they are **100%** of the run's WARN volume (the only other
  warning is the single PDCL notice). The u32 values sit in FormID range; **what
  record they target is unconfirmed** and is deliberately not asserted here.
- **Impact**: three-fold. (1) Every Starfield ARMO base form resolves
  `model_path = ""` and is treated as model-less, so a REFR pointing at one
  spawns no geometry — the same class of loss #1576 tracks for the BFCB
  component-block families, arriving by a different route. (2) The message
  actively misdiagnoses a known schema divergence as data corruption, which is
  the kind of wrong premise that costs a session. (3) 1,480 WARN lines drown any
  real warning on every Starfield ESM load, including the PDCL notice #1568
  deliberately made visible.
- **Related**: #1576 (model-less STAT/BNDS/ACTI/ARMO via BFCB), #1620 (the guard
  itself), #1567 (the LIGH `DAT2` precedent — same shape of fix).
- **Suggested Fix**: gate the `MODL` arm on `GameKind::Starfield` and treat a
  4-byte payload as a fixed-width value rather than a string — at minimum
  downgrade to a one-shot `debug!` naming the real cause, and capture the
  `(INDX, u32)` pairs for a later decode instead of discarding them. Do not
  guess the target record type without a byte-level trace.

#### SF-D5-02: `IsCollisionOnly` is backticked as live in two audit skills but exists nowhere in the tree
- **Severity**: LOW
- **Dimension**: 5 — ESM + cell bring-up regression surface
- **Location**: `.claude/commands/audit-starfield/SKILL.md:203`,
  `.claude/commands/_audit-common.md:103`
- **Status**: NEW
- **Description**: both files backtick `IsCollisionOnly` as a marker component
  living in `byroredux/src/components.rs`. `grep -rn "IsCollisionOnly"
  --include="*.rs" .` returns nothing — the symbol does not exist. Per the
  path-reference convention (`_audit-common.md` §Path-Reference Convention),
  backticks assert present existence. `_audit-validate.sh` cannot catch it: its
  symbol heuristic matches only backticked **snake_case** tokens (line 161), and
  it is advisory rather than fatal — a run against the current tree reports
  "OK: all path references valid" with `IsCollisionOnly` unmentioned.
- **Evidence**: the marker structs that do exist in
  `byroredux/src/components.rs` are `IsFxMesh` (`:90`) and `IsLodTerrain`
  (`:138`). The invariant the skill asks the auditor to confirm — synthesized
  colliders staying out of the BLAS — is genuinely enforced, but by a different
  mechanism: "The ghost carries no `MeshHandle`, so it takes no BLAS entry, no
  TLAS …" (`byroredux/src/cell_loader/spawn.rs:378`).
- **Impact**: an auditor following the Dimension 5 checklist verbatim either
  reports a false negative ("the marker is missing, colliders are in the BLAS")
  or silently skips the check. This is exactly the stale-path class the
  path-reference convention exists to prevent.
- **Related**: #1114 (the convention), the TD7-* stale-path family.
- **Suggested Fix**: replace both references with the real mechanism (no
  `MeshHandle` ⇒ no BLAS entry, `spawn.rs:378`), and widen the
  `_audit-validate.sh` symbol heuristic past snake_case so CamelCase type names
  are covered too.

### Dimension 6 — NIF shader blocks, BSVER 155+ — 0 findings

Whole-corpus probe over all five vanilla mesh archives (89,276 NIFs):

```
nifs parsed ok         : 89276      nifs hard parse error  : 0
nifs truncated         : 0          dropped blocks total   : 0
recovered blocks total : 6          NiUnknown: 6 × BSWeakReferenceNode
BSLSP full-body starfield_tail length hist: {38: 2538}
```

- **#1510 regression guard: PASS.** `BSLightingShaderProperty` NiUnknown count
  is **0**; the only NiUnknown in the entire corpus is the known #2105
  `BSWeakReferenceNode` residual, still **exactly 6**.
- **#1606 guard: PASS.** `starfield_tail` is 38 B on 2,538 / 2,538 full-body
  blocks and captured to `block_size` (the stub path yields an empty tail —
  verified directly on `lgt_marker_directspot_staticshadow.nif` and
  `conveyorbeltthin_endcap02.nif`).
- Retail header BSVER measured at **173** on sampled meshes.
- Stub/full split is **403,562 / 2,538** (99.4% stub). Every full-body block:
  `shader_type == 0`, NULL `texture_set_ref`, no `MODELSPACENORMALS` CRC.

Open, not re-filed: #2622, #2624, #2639.

### Dimension 7 — Real-data validation — 0 findings

129/129 archives open; 12,674 stratified extractions with 0 failures.
89,276/89,276 NIFs parse with 0 hard errors and 0 truncations — the #746/#747
truncation tail has **not** grown, and the #2105 residual is unchanged at 6.
390,766 `ImportedMesh` produced, 100% carrying normals/UVs/tangents/colours.
Five representative shapes (clutter `conveyorbeltthin_endcap02`, architecture
`catindwalksm2wayc_60_l01`, ship interior `shiphatchdbl_01`, weapon
`beowulf_ironsights_update_substance`, marker `dummylarge01`) traced end-to-end
through `import_nif_scene`; all resolve as expected, the marker correctly
emitting zero meshes.

### Dimension 8 — NIFAL canonical material translation for Starfield — 1 finding

The priority-lead analysis and its two disproved sub-candidates are in the
Executive Summary and *Disproved Candidates* respectively.

#### SF-D8-01: the shared slot→role table has zero Starfield coverage by construction, and nothing records that
- **Severity**: LOW
- **Dimension**: 8 — NIFAL canonical material translation
- **Location**: `crates/nif/src/import/material/slot_role.rs:1-29`,
  `byroredux/src/cell_loader/refr_texture_overlay_tests.rs:488-600`
- **Status**: NEW
- **Description**: #2695 unified two divergent slot→role tables into
  `slot_to_role`, and its module doc plus the overlay test file present it as
  *the* cross-game boundary for `BSShaderTextureSet` slot semantics. Measured on
  vanilla Starfield, the table cannot execute at all: no `BSShaderTextureSet`
  block exists, no full-body property carries a texture set, no REFR carries an
  XATO/XTXR, and the `.mat` arm never populates a role. Every arm's supporting
  evidence is drawn from `Skyrim - Meshes0.bsa`. Nothing in the code or tests
  states that the Starfield column is empty.
- **Evidence**: 0 `BSShaderTextureSet` blocks / 0 of 2,538 non-NULL
  `texture_set_ref` in 89,276 NIFs; XATO = XTXR = XMSP = 0 over 1,971,135 REFRs
  in `Starfield.esm`; `ImportedMaterial.shader_type == 0` and
  `model_space_normals == false` on 100% of 390,766 imported meshes; the `.mat`
  arm at `byroredux/src/asset_provider/material.rs:973-1010` returns
  `PresenceOnly` without touching `material.textures`.
- **Impact**: no runtime defect today. The risk is forward-looking and concrete:
  #2359 Phase 2 will make the CDB a role producer, and it will arrive at a table
  whose arms, constants (`FACE_TINT = 4`, `SKIN_TINT = 5`, `HAIR_TINT = 6`,
  `MULTI_LAYER_PARALLAX = 11`) and tests have never seen this game's shader-type
  vocabulary — which is the `BSShaderType155` enum, not `BSLightingShaderType`.
- **Related**: #2695, #2359, #2579 (the FO76 half of the enum-numbering leak),
  #2713.
- **Suggested Fix**: record the measured Starfield-coverage-is-zero fact in
  `slot_role.rs`'s module doc alongside the existing per-arm evidence, and note
  that a Starfield producer must decide the `BSShaderType155`-vs-
  `BSLightingShaderType` question before reusing this table.

### Dimension 9 — BGSM/BGEM external material flow — 1 finding

Verified clean: `merge_external_material`'s signature is still narrowed to
`&mut ImportedMaterial` (`byroredux/src/asset_provider/material.rs:913-917`), so
it cannot reach geometry, skinning or scene ownership — no NIFAL boundary
widening. BGEM is dispatched distinctly from BGSM by a magic-wins /
extension-fallback rule with a mismatch warning (`:1049-1071`), and #2709's
`MergeOutcome` tri-state is in place.

#### SF-D9-01: 1,639 vanilla Starfield shader properties point at `.bgsm`/`.bgem` files that exist in no Starfield archive, and the `.mat`-keyed PBR gate excludes them silently
- **Severity**: HIGH
- **Dimension**: 9 — BGSM/BGEM external material flow
- **Location**: `byroredux/src/asset_provider/material.rs:973` (the `.mat`-suffix
  CDB gate) and `:1073-1075` (the silent BGSM miss)
- **Status**: NEW
- **Description**: Starfield's stub rule for `BSLightingShaderProperty` is
  `!name.is_empty()`, so a `.bgsm`-named property becomes a
  `material_reference` stub and `apply_bs_lighting_shader` returns at
  `crates/nif/src/import/material/dedicated_shader.rs:112-114` **before copying
  any inline field** — every remaining value is a parser placeholder. The mesh
  then reaches `merge_external_material`, where the CDB arm is gated on
  `path.ends_with(".mat") && provider.has_starfield_cdb()`, so a `.bgsm` path
  never receives the `is_pbr = true` Disney-BSDF routing its `.mat` neighbours on
  the same mesh do. Dispatch falls to the BGSM arm, `resolve_bgsm` returns
  `None` because the file does not exist, and the function does
  `return MergeOutcome::Unresolved` **with no log statement** — the
  `unresolved_material_warning` path is only reached by the unknown-extension
  arm.
- **Evidence**: across all 129 Starfield archives there are **0 `.bgsm` and 0
  `.bgem` files** (only 20 loose `.mat`, all mod-authored). Across the 89,276
  vanilla NIFs there are **1,639 `.bgsm`/`.bgem` shader-property references
  spanning 234 distinct paths**, all on shipped content:
  `materials\common\metal\metalgenericpaintedwhite02.bgsm` ×241,
  `materials\shared\t_metal_clean_white.bgsm` ×73,
  `materials\ships\discovery\sciencestation1.bgsm` ×48,
  `materials\architecture\hangar\hangar_metalsteel01default01.bgsm` ×47,
  `materials\landscape\caves\mine\caveminewall01.bgsm` ×27,
  `materials\weapons\beowulf\beowulf_receiver.bgsm` ×15,
  `materials\architecture\city\newatlantis\glowwhite.bgem` ×12.
  Host NIFs include `hangarext_wallc02.nif`, `hangarext_floormid01.nif`,
  `na_lobbyu_chunksext_walla_str01x02_003.nif`,
  `beowulf_ironsights_update_substance.nif` and
  `shpgenintsegsmrcagecorinend_r01.nif`.
- **Impact**: those shapes render on the **legacy Lambert / simple-GGX path**
  using `BSLightingShaderProperty::material_reference_stub`'s placeholder
  constants (`specular_color [1,1,1]`, `specular_strength 1.0`,
  `glossiness 1.0`, `emissive_color [0,0,0]`) while every adjacent surface on the
  same mesh takes the Disney lobe. Per the severity scale's NIFAL rows, a
  divergent canonical `Material` is HIGH minimum — and because both the parse
  stub and the resolve miss are silent, the discrepancy produces no diagnostic
  anywhere. Visible on New Atlantis lobby architecture and hangar exteriors,
  which is high-traffic content.
- **Related**: #2359 (the CDB blackout this compounds), #2601 (the generic form
  of the silent BGSM-resolve-failure half), #2594, #2626, #2627.
- **Suggested Fix**: decouple the PBR routing decision from the file suffix —
  when `has_starfield_cdb()` is true and the ESM/NIF is Starfield-era, a
  `.bgsm`/`.bgem` reference is a CDB lookup key like any other, so it should
  reach the same arm as `.mat` rather than the FO4 loose-file resolver. At
  minimum, make the `resolve_bgsm` miss at `:1073-1075` log once per path so a
  234-path blackout is not invisible.

---

## CRC32 Flag Table

No new empirical CRC32 → flag-name mappings were derivable this pass, and the
reason is itself a measurement: **0 of 2,538** full-body Starfield
`BSLightingShaderProperty` blocks carry the `MODELSPACENORMALS` CRC, and 99.4%
of all Starfield shader properties are material-reference stubs whose
`sf1_crcs`/`sf2_crcs` arrays are empty by construction
(`crates/nif/src/blocks/shader.rs:797-798`). The vanilla Starfield corpus
therefore offers almost no CRC population to mine; the flag vocabulary lives in
the CDB, not the NIF. The existing table in
`crates/nif/src/shader_flags.rs` is unchanged and its `MODELSPACENORMALS` entry is
correct but unexercised on this game.

## Remaining-Work Chain

Per `docs/engine/starfield-esm-roadmap.md` (Phases 0+1 done, 2–4 invalidated by
the 99.9%-parity measurement), in order:

1. **Per-field CDB extraction** (#2359 Phase 2) — `.mat`-resolved materials still
   reach the Disney lobe with NIF defaults, and this pass adds two hard numbers
   to scope it: the parse costs **9.19 GB RSS** (SF-D3-02) and the reader exposes
   no path→instance index. SF-D9-01 should be folded in, since the same lookup
   would serve the 234 orphaned `.bgsm` paths.
2. **Starfield ARMO / component-block record decode** — SF-D5-01 plus #1576.
3. **Exterior worldspace tiles.**
4. **Space-cell / planet / GBFM records.**
5. **The #746/#747 NIF truncation tail** — confirmed *not* grown; the residual is
   6 `BSWeakReferenceNode` recoveries (#2105).

## Disproved Candidates

Recorded so they are not re-chased:

1. **`slot_to_role` mismaps Starfield slots the way it mismaps FO4's.** Disproved
   on data — the table is unreachable in all three producers (Executive Summary).
2. **`BSShaderType155` numbering leaks into `slot_to_role`.** The mechanism is
   real (`normalize_shader_type` only remaps the `Fo76SkinTint` variant, so a
   155-enum type 3 "Face Tint" would take the default column's Emissive at slot 2
   and Height at slot 3), but it **cannot fire on Starfield**: all 2,538
   full-body blocks measure `shader_type == 0`. The FO76 half is already OPEN as
   #2579.
3. **The `material_reference` early return skips `info.shader_type`.** True —
   `dedicated_shader.rs:112-114` returns before `:127` — and it contradicts the
   #2695 comment at `:123-126` claiming the type is recorded so an XTXR-only
   placement "still tells the REFR overlay which table to resolve its slots
   with". But `material_reference_stub` (`crates/nif/src/blocks/shader.rs:792`)
   hardcodes `shader_type: 0`, which is also `MaterialInfo`'s default, so the
   outcome is identical. Comment is overstated; behaviour is correct.
4. **The 359 no-suffix Starfield stub names are truncated/mis-indexed strings.**
   They are a genuine authored sentinel: the header string table contains the
   literal `"Materials\\"` alongside the real `.mat` paths, and it is only ever
   bound to `EditorMarker*` shapes (verified on
   `lgt_marker_directspot_staticshadow.nif` and `conveyorbeltthin_endcap02.nif`).
   No content is lost.
5. **A 4.5% BSGeometry→ImportedMesh drop is content loss.** Traced to the
   deliberate editor-marker name skip in `crates/nif/src/import/walk/mod.rs`
   (`catindwalksm2wayc_60_l01.nif`: 7 blocks → 6 meshes, the dropped block is
   named `EditorMarker:9`).
6. **`extract_bs_geometry`'s whole-block Stage A / Stage B choice strands mixed
   internal/external slot orders.** Impossible by construction —
   `BSGeometry::parse` (`crates/nif/src/blocks/bs_geometry.rs:105-112`) reads
   `internal` once from `av.flags` and applies it to all four slots.
7. **The walker's `av.flags & 0x01` hidden-shape skip silently drops Starfield
   geometry.** It fires on **0 of 66,202** Starfield BSGeometry blocks.
8. **`Starfield - LODMeshes.ba2` shipping zero `.mesh` files breaks distant LOD.**
   Its NIFs resolve 42,318/42,538 (99.5%) against the shared `geometries\` hash
   tree in the mesh archives.

## Deduplication

Baseline: 269 open issues (`gh issue list --state open --limit 400`, fetched
2026-08-16), plus a scan of `docs/audits/AUDIT_STARFIELD_2026-07-25.md`,
`_2026-08-03.md`, `_2026-08-07.md`, `_2026-08-12.md` and
`AUDIT_ESM_2026-08-13.md`. Every finding's keywords were grepped against the
open set. The following OPEN issues were matched and **deliberately not
re-filed**: #2359, #2360, #2361, #2362, #2097, #2533, #2579, #2584, #2585,
#2594, #2601, #2621, #2622, #2624, #2626, #2627, #2628, #2633, #2637, #2639,
#2641, #2642, #1576, #2713.

## Scope Note

Per `_audit-common.md`'s un-owned-subsystem list, this audit did not touch the
P2 gameplay slice, FaceGen, the mod runtime, FSR3, the Havok packfile reader, or
the debug server — none are Starfield-specific and all are out of this skill's
dimension set.

---

Suggested next step:

```
/audit-publish docs/audits/AUDIT_STARFIELD_2026-08-16.md
```
