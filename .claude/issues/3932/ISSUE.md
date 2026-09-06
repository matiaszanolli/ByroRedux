# #3932: SF2-2026-09-05-D3-01: `crates/sfmaterial`'s module doc points the consumer-side mapping at `byroredux/src/asset_provider.rs`, a file deleted in the Session 34 split

Filed from `docs/audits/AUDIT_STARFIELD_2026-09-05b.md` (SF2-2026-09-05-D3-01) via `/audit-publish`, 2026-09-05 (`/audit-suite --preset per-game-all`). Labels: `low,game:starfield,legacy-compat,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3932 --json state`.

---

**Source**: `docs/audits/AUDIT_STARFIELD_2026-09-05b.md` (SF2-2026-09-05-D3-01), `/audit-suite --preset per-game-all`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW
- **Dimension**: 3 — CDB material database
- **Location**: `crates/sfmaterial/src/lib.rs` (module doc, "Scope (Stage B per
  audit #762)" section)
- **Status**: NEW
- **Description**:
  The crate-level doc closes with: *"The consumer-side mapping (Starfield-specific
  material → `ImportedMesh` fields) happens in `byroredux/src/asset_provider.rs`
  and is a separate concern from the format parsing here."* That path does not
  exist — `asset_provider` became a directory during the Session 34 refactor and
  the CDB consumer specifically lives in `byroredux/src/asset_provider/material.rs`
  (`discover_starfield_cdbs`, `register_starfield_cdb`, `apply_cdb_pbr_fallback`).
  This is the only pointer from the CDB parser to its consumer, so it is the
  first place someone tracing the boundary looks.
- **Evidence**:
  ```
  $ ls byroredux/src/asset_provider.rs
  ls: cannot access 'byroredux/src/asset_provider.rs': No such file or directory
  ```
  The live consumer sites are `byroredux/src/asset_provider/material.rs:177`
  (`discover_starfield_cdbs`), `:599` (`register_starfield_cdb`), `:970`
  (`apply_cdb_pbr_fallback`).
- **Impact**:
  Doc-rot only, but on a cross-crate boundary pointer with no other signpost.
  The same statement also predates `MergeOutcome` — the mapping today produces
  `PresenceOnly`, i.e. one routing flag and no fields, which the sentence's
  "material → `ImportedMesh` fields" phrasing overstates.
- **Related**: #3889 (`register_starfield_cdb` is a test-only duplicate of the
  shipped registration path — same neighbourhood, already filed, not re-reported
  here); #3398 (the Phase-2 work this doc is describing).
- **Suggested Fix**:
  Repoint at `byroredux/src/asset_provider/material.rs` and name
  `merge_external_material` / `apply_cdb_pbr_fallback` as the actual entry
  points, noting that the mapping is currently `PresenceOnly` pending #3398.

---

## Verified clean (measured, not assumed)

### Dimension 1 — BA2 v2 / v3 + LZ4 block decompression

Version and compression-method census over **all 129 installed archives**, read
from the raw header bytes and cross-checked against `Ba2Archive::open`:

```
  92  v2 GNRL  (no compression_method field)
  22  v2 DX10  (no compression_method field)
  15  v3 DX10  compression_method = 3  (LZ4 block)
   0  v3 GNRL
   0  v3 with compression_method = 0 (zlib)
```

Extraction sample (~50 stride-sampled entries per archive, 5,924 total):
**0 failures**, and every one of the 1,920 sampled `.dds` entries came back with
a valid `DDS ` magic from the reconstructed header. Independently, the mesh-side
sweep extracted **120,543 GNRL entries with 0 failures and 0 header-parse
failures**, so the GNRL path is swept rather than sampled.

Code re-verified: the v3 arm reads the 8-byte extra **then** the 4-byte
`compression_method` (36-byte header), logs the boundary *after* that read
(#2360), maps `0 → Zlib` / `3 → Lz4Block`, and **hard-errors** on anything else
rather than falling through — confirmed, not a silent default. Unknown BA2
majors hit an exhaustive-match error arm (#811). The `packed_size == 0` raw/LZ4
chunk selector is present on both the GNRL (`ba2.rs:856`) and DX10
(`ba2.rs:890`) paths, and both dispatch into the single `decompress_chunk`.
`lz4_flex`'s `safe-decode` is pinned on, making the documented undersized-hint
panic structurally impossible, with `catch_unwind` retained as defence in depth.

Two honest coverage notes: **the v3 + zlib (`compression_method = 0`)
combination has zero coverage in this install** — no archive exercises it — and
one mod archive (`starfieldresourcerevival - textures.ba2`) ships `.dds` files
inside a **GNRL** archive rather than DX10, which extracts correctly (97/97).

### Dimension 5 — ESM classifier, on real data

Every `.esm` in the Starfield Data directory, header-only read (no GRUP walk,
no memory risk):

```
  55 plugins, HEDR version histogram: {0.96: 55}
  plugins outside the Starfield band 0.955..=0.97: 0 of 55
  record_version range observed: 552 … 582
```

All 55 — base game, ShatteredSpace, every SFBGS Creation, and installed
community plugins — classify as `GameKind::Starfield`. Worth recording that the
`from_header` bands are `(0.945..=0.955)` with `record_version >= 100` → FO4
*before* `(0.955..=0.97)` → Starfield, so `0.955` exactly is in both and the
first arm would win. Not reachable on real data (everything is 0.96), so this is
a note, not a finding.

### Dimension 3 / 9 — CDB and sidecar reality, swept over all 129 archives

```
  TOTAL archives=129  cdbs=13  mat_files=20  bgsm_files=0  bgem_files=0
```

This confirms three claims the skill makes and grounds a fourth:

- **13 CDBs**, matching the 2026-08-30 count, and **two are full-size**:
  `materials\materialsbeta.cdb` (105,037,616 B) and
  `materials\creations\sfbgs007\materialsbeta.cdb` (104,868,172 B). The
  ~18 GB corpus-wide parse-peak estimate for a Phase-2 reader reusing today's
  `parse` is well-founded, not the single-CDB 9.19 GB figure.
- **Exactly one CDB per archive**, and 12 of the 13 live in *content* archives
  (`ShatteredSpace - Main01.ba2`, `SFBGS0xx - Main.ba2`, `kgcdoom - main.ba2`, …)
  at `materials\creations\<plugin>\materialsbeta.cdb`, not in
  `Starfield - Materials.ba2`. This is precisely why #1571's scan-don't-hardcode
  discovery and #2621's numeric-sibling extension are load-bearing: a hardcoded
  base path would find 1 of 13. Both are intact, and `--bsa` archives are
  scanned for CDBs too, so a session that loads DLC meshes also gets that DLC's
  material database. **Not a finding** — the default `starfield` profile
  discovers only the base CDB, but it also loads none of the content the other
  twelve describe.
- **Zero `.bgsm` and zero `.bgem` files exist in any Starfield archive** (SF-D9-01
  re-confirmed at full sweep), so every `.bgsm`/`.bgem`-*named* reference is a
  guaranteed resolver miss falling through to the CDB flip. The #3230
  try-then-fall-through order is intact and no early `PresenceOnly` return has
  been re-added above the resolvers.
- **20 `.mat` files exist**, all in installed mod archives
  (`qog-pawnshop` 12, `sp2_factionrequisitionkiosks` 5,
  `starfieldresourcerevival` 2, `avontechshipyards` 1) — confirming #3782's
  correction that the `.mat` short-circuit is retained for want of a JSON `.mat`
  resolver, not because such files structurally cannot exist.

`read_user_class` still reads field values in **declaration order and never
consults `Field::offset`** — the *XMCOLOR* channel-binding defect is live and
unchanged. That is #3398's known open scope, not re-reported. `ChunkType`
decoding returns `Error::UnknownChunkType` rather than panicking, and the
allocation sites are bounded (`chunk_count.min(bytes.len() / 8)`,
`field_count.min(payload.len() / 12)`).

### Dimension 8 — today's palette-remap change is a Starfield no-op

`79194306` (#3897/#3898, landed today, after the first 2026-09-05 pass) made
`BSLightingShaderProperty` a producer of the greyscale→palette enable bits via
`is_palette_color_from_modern_shader_flags`, which unions the typed flag word
with the FO76/Starfield CRC32 arrays — so it is reachable on Starfield content
by construction. Traced to the end: it cannot misfire here. The pack site
(`byroredux/src/cell_loader.rs`, `pack_imported_material_flags`) gates on
`material.textures.greyscale_lut.is_some() && material.bgsm_greyscale_lut_enabled`,
and both shader branches in `triangle.frag` gate again on
`mat.greyscaleLutIndex != 0u`. Vanilla Starfield authors **no `BSShaderTextureSet`
at all** (see the histogram), so the `greyscale_lut` role is never filled and
the flag is never packed. Double-gated, verified clean — recorded because it is
new, Starfield-reachable code that no prior Starfield audit has seen.

### Other guards re-read and intact

`normalize_mesh_path` leaves a `geometries\` head untouched (#1292);
`extract_bs_geometry` skips `scale<=0` sentinel slots on **both** the Stage-A
`find_map` and the Stage-B external-`.mesh` loop (#1828/#1829) and iterates every
LOD slot rather than `meshes.first()` (#1209); `BSGeometryMeshData::parse` gates
the meshlet/cull trailer on `stream.remaining() == 0` so a body ending at the LOD
array is "no trailer" while one truncated *mid*-trailer still errors (#3777) —
`FaceMeshes` parses 1,282/1,282 clean, which is the population that regression
protects; `XCLL_SIZES_STARFIELD = [28, 108]` (#1291); the Starfield LIGH `DAT2`
component-block decode with its `starfield_ligh_dat2_decodes_to_light_data` test
(#1567); the PDCL named skip with its "recorded in skip telemetry, not lost to
the catch-all" test (#1568); the `base_layer` (not `final_layer`) static-trimesh
collision gate (#1294); `5bca381a`'s patch-archive ordering fix is applied to the
`starfield` profile (`MeshesPatch`/`LODMeshesPatch`/`TexturesPatch01,02` all
listed last), and all 21 archives the profile names exist on disk.

---

## Relationship to the first 2026-09-05 pass

`AUDIT_STARFIELD_2026-09-05.md` (HEAD `fa5c4191`, `texture-roles-deep` preset)
filed 6 findings. Status at this pass's HEAD `6fba2b0a`:

| Its finding | Status now |
|---|---|
| D7-01 archive precedence inverted by #3637 | **FIXED** by `5bca381a` (#3896, CLOSED). Re-verified against the live profile. |
| D8-01 `slot_to_role` vs `canonical_shader_type` disagree on Starfield | Open as **#3900**. My histogram adds evidence: vanilla Starfield ships **zero** `BSShaderTextureSet`, so the inconsistency is latent-only today, exactly as filed. |
| D9-01 `record_external_texture_sources` has no exhaustiveness guard | Open as **#3903**. Not re-examined. |
| D3-01 `Mat` texture provenance unreachable | Open as **#3906**. Not re-examined. |
| D8-02 `nifal.md` BGEM glass roles | Open as **#3907**; doc fixed by `2853464f`. |
| D8-03 `_audit-common.md` says 18 roles, live struct has 22 | **FIXED** by `2853464f`. |

**No finding in this report duplicates one of those.** The first pass covered
the material/texture-role axis; this pass covers the skin chain, the archive
and parse-rate surface, and the ESM classifier — all of which it explicitly
listed as not re-measured.

Where this pass **supersedes** prior conclusions:

- **#3549's premise** ("the identity is not in the file at all") is
  contradicted by direct measurement — see SF2-…-D2-01.
- **#3524's scope** ("six residual MeshesPatch truncations") understates the
  population by 13 identical cases in a second archive — see SF2-…-D7-01.
- The skill's *"the residual truncation tail in Meshes01/MeshesPatch"* is stale
  on the Meshes01 half: **Meshes01 is 100.00% clean, 0 truncated**. The tail is
  MeshesPatch + ShatteredSpace - Main01.

---

## CRC32 Flag Table

No new flag-name → CRC32 mappings were derived this pass. The constants in use
on the Starfield path are unchanged and are consumed via
`modern_effect_shader_bit`'s typed-word ∪ SF1-CRC ∪ SF2-CRC union:

| Flag | CRC32 | Constant |
|---|---|---|
| `SLSF1::Greyscale_To_PaletteColor` | `442246519` | `bs_shader_crc32::GRAYSCALE_TO_PALETTE_COLOR` |
| `SLSF1::Greyscale_To_PaletteAlpha` | `2901038324` | `bs_shader_crc32::GRAYSCALE_TO_PALETTE_ALPHA` |
| `SLSF1::Soft_Effect` | see `shader_flags.rs` | `bs_shader_crc32::SOFT_EFFECT` |
| `SLSF2::Effect_Lighting` | see `shader_flags.rs` | `bs_shader_crc32::EFFECT_LIGHTING` |

The hashes remain opaque in the sense that there is no reverse table from an
*observed* CRC to a flag name — only forward constants for the four bits the
importer needs. Deriving the full inverse table would require the flag-name
vocabulary, which is not in nif.xml for BSVER ≥ 152.

## Remaining-Work Chain

Unchanged in ordering from the skill; the measurements above only sharpen the
sizing:

1. **Per-field CDB extraction** (#3398 Phase 2). Key and field vocabulary are
   **solved**; the blocker is the indexed reader that avoids the corpus-wide
   parse peak — now confirmed to be **13 CDBs, two of them full-size at
   ~105 MB each**, so the ~18 GB figure stands. Plus the *XMCOLOR*
   declaration-order-vs-`Field::offset` fix.
2. **`SkinAttach` bone-name consumption** — new to this chain, and cheaper than
   everything else on it: the data is parsed, exact, and count-checked on
   100% of the affected population (SF2-…-D2-01).
3. **PDCL ahead of GBFM** (SF-D4-01) — 74.9% of unresolved Cydonia REFRs vs
   0.081%, ~900× more impactful by the baseline doc's own promote/defer metric.
4. Exterior worldspace tiles; space-cell / planet / GBFM records.
5. The #2105/#3524 NIF truncation tail — now localised to the
   `BSWaterReferenceStruct` per-entry skip, 19 files, not growing.

## Dimension Summary

| Dim | Area | Result |
|---|---|---|
| 1 | BA2 v2/v3 + LZ4 | **Clean** — 129 archives censused, 5,924 extracts + 120,543 GNRL extracts, 0 failures |
| 2 | BSGeometry mesh extraction | **1 HIGH + 1 MEDIUM** — `SkinAttach` / `BoneTranslations` decoded and unconsumed; all named guards intact |
| 3 | CDB material database | **Clean + 1 LOW** (doc-rot); #3398 / #3889 known-open, not re-reported |
| 4 | ESM resolve-rate baseline | **Not measured** (memory constraint) — guards verified statically |
| 5 | ESM + cell bring-up | **Clean** — 55/55 plugins classify correctly; XCLL / PDCL / LIGH-DAT2 / base_layer guards intact |
| 6 | NIF shader blocks BSVER 155+ | **Clean + 1 LOW** — #1510 guard holds (0 NiUnknown on 483,837 blocks); tails quantified |
| 7 | Real-data validation | **Clean + 1 LOW** — 100% recoverable, 19 truncated (no growth), scope correction on #3524 |
| 8 | NIFAL canonical translation | **Clean** — today's palette change is double-gated and inert on Starfield |
| 9 | BGSM/BGEM external flow | **Clean** — 0 `.bgsm`/`.bgem` files corpus-wide; #3230 fall-through intact |

## Deduplication

Dedup list re-pulled mid-audit (max issue `#3912`, 121 open). Checked against
all 20 prior `AUDIT_STARFIELD_*` reports. Known-open items deliberately **not**
re-filed: #3398 (CDB Phase 2 + *XMCOLOR* offsets), #3889 (test-only
`register_starfield_cdb`), #3900 / #3903 / #3906 / #3907 (the first pass's open
findings), #2625 (drift telemetry — extended with measurement instead), #3524
(truncation tail — extended with scope + localisation instead), #2637, #1576.
Findings sourced from pre-2026-06-07 reports were verified against code rather
than against GitHub, per the shared protocol.

---

*Report generated by `/audit-starfield`. Suggested next step:*
`/audit-publish docs/audits/AUDIT_STARFIELD_2026-09-05b.md`
*(label every finding `game:starfield` + `legacy-compat`, plus its domain label:
D2-01/D2-02 → `nif` `import-pipeline`; D7-01 → `nif-parser`; D6-01 →
`nif-parser` `test-gap`; D3-01 → `doc-rot`.)*

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other block parsers, other games)
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **CANONICAL-BOUNDARY**: If the fix touches `translate_material` / `Material::resolve_pbr` / the emitter params, per-game logic stays at the NIFAL parser→`Material` boundary
