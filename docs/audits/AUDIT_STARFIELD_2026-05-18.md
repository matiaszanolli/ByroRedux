# Starfield Compatibility Audit — 2026-05-18

**Scope**: All 6 dimensions. Engine HEAD `82b7be9a` (post-#1175 Backlight gate fix).
Live data: `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/`.
Methodology: per-dimension agents + manual backfill (dims 1, 3, 4 — agents kept getting cut off mid-investigation); findings cross-checked against `/tmp/audit/issues.json` for dedup.

## Executive Summary

Starfield is at "geometry renders, materials don't" parity. The parser side is in much better shape than the 2026-04-27 audit framing implies:

- **NIF parse**: 87,829 clean / 89,276 NIFs (**98.38%** clean, **100%** recoverable) on a full sweep of all 5 vanilla mesh BA2s — Meshes01 (97.21%), Meshes02 (100%), MeshesPatch (98.11%), LODMeshes (99.92%), FaceMeshes (100%). Truncation tail tracked at #746/#747. The often-cited "31,058 NIFs" figure is Meshes01 alone.
- **BA2 v2/v3**: production-correct. Exhaustive `match` over `{1, 2, 3, 7, 8}` (#811), v3 reads exactly 12 trailing bytes (8 + `compression_method` u32), `compression_method` dispatch `0 → Zlib | 3 → Lz4Block | other → error` (#755). GNRL + DX10 unify through `decompress_chunk()` with explicit `unpacked_size` hint to `lz4_flex::block::decompress`.
- **NIF BSVER 155-172+ shader blocks**: CRC32 flag arrays, `BSShaderType155`, `LuminanceParams`, `TranslucencyParams`, `BSTextureArray`, all four BSEffectShaderProperty FO76+ textures (Reflectance / Lighting / Emittance / Emit Gradient) parsed with correct BSVER gates. No new fields surface at Starfield BSVER ≥ 172 beyond the FO76 baseline for shader blocks.
- **BGSM stopcond + `.mat` capture**: BSVER ≥ FO76 short-circuits on `.bgsm` / `.bgem` / `.mat` Name with the narrow suffix gate (#749). Material path captured into `ImportedMesh.material_path` for downstream consumers.
- **`BSGeometry` import**: fully wired — inline (`BSGeometryMeshKind::Internal`) AND external `.mesh` companion via `MeshResolver` (`BSGeometryMeshKind::External`). UDEC3 (10:10:10:2) packed normals/tangents unpacker pinned. Y-up native (no Z-up→Y-up swap).
- **5 representative spot-checks** (clutter / ship hull / weapon / landscape / FaceGen) all parse with **zero NiUnknown** blocks. No new block type has slipped through dispatch.

**The remaining wall is the material side.** `crates/bgsm/` caps at v22 (FO4/Skyrim/FO76) and explicitly disclaims Starfield. No `.mat` (JSON) parser. No `materialsbeta.cdb` (binary component database) reader. Vanilla Starfield meshes that DO extract geometry render with NIF stopcond defaults — magenta-checker placeholder where the missing `.mat` would have lived. This is tracked as **#762 (SF-D6-03)** — the only Starfield-specific open issue.

**Two MEDIUM/LOW new findings**: `Root Material` field on `BSLightingShaderProperty` is read but discarded (SF-D1-NEW-01 MEDIUM, forward-risk for Starfield NIFs that author non-material editor labels in `net.name`); the texture-archive sweep test covers only 3 representative archives, not all 30 shipped (SF-D2-NEW-01 LOW). Plus 2 LOW doc-rot items (SF-D2-NEW-02 stale "22 archives" claim in CLAUDE.md, SF-D2-NEW-03 read-and-dropped trailer bytes with no sanity log).

**Forward blockers** (in order):
1. **#762 / SF-D6-03** — Starfield `.mat` JSON parser + `MaterialProvider` integration (unblocks "Starfield mesh renders with real material")
2. **`materialsbeta.cdb` reader** — deferred under #762; vanilla Starfield ships no loose `.mat` files (everything inside the CDB)
3. **`--sf-smoke` resolve-rate measurement** (tool already exists per #763) — publishes the number that decides whether SF ESM work is a fix-up patch or a from-scratch parser
4. Then: SF-only record types (`PNDT` / `STDT` / `BIOM` / `SFBK` / `SUNP` / `GBFM` / `GBFT`), space-cell concept, procedural ship assembly (gameplay-driven, out of scope until form-linker works)

---

## Findings — Grouped by Severity

### MEDIUM

#### SF-D1-NEW-01 — `BSLightingShaderProperty.Root Material` is read but discarded
- **File**: `crates/nif/src/blocks/shader.rs:850-852`
- **Observation**: `let _root_material = stream.read_string()?;` — read to keep stream alignment but never surfaced on the struct.
- **Why bug**: For Starfield (BSVER 172+), the stopcond at `shader.rs:771-777` already captures the common case (`net.name` IS the BGSM/BGEM/MAT path). The forward-risk case is when `net.name` carries a non-material editor label AND Root Material carries the actual material path — that BGSM reference is silently dropped. Unknown how often Starfield authors this shape; needs empirical sampling.
- **Fix**: Promote `_root_material` to a real field on `BSLightingShaderProperty` (e.g., `root_material_path: Option<Arc<str>>`). In the importer, when `material_path_from_name(net.name, ...)` returns `None`, fall back to `root_material_path`. Sample a few Starfield NIFs first to confirm the shape is real.

### LOW

#### SF-D2-NEW-01 — Starfield BA2 sweep regression-test is single-archive, not corpus-wide
- **File**: `crates/bsa/tests/ba2_real.rs:358-482`
- **Observation**: The committed `#[ignore]` tests only open ONE archive per code-path (Meshes01 / Textures01 / Constellation). Session 7's "22 archives / 128K DX10 textures / 0 failures" was an external one-shot sweep, not in-tree.
- **Why bug**: The prompt's "confirm session 7's claim still holds" check has no in-tree mechanism. A regression that breaks any of the other 27 archives would only be caught externally.
- **Fix**: Add a sweep test that `read_dir`s `BYROREDUX_STARFIELD_DATA`, opens every `Starfield - Textures*.ba2`, extracts one entry per `Ba2Variant` per archive. Keep `#[ignore]`, report per-archive pass/fail. Modeled on `parse_rate_*` sweeps.

#### SF-D2-NEW-02 — CLAUDE.md "22 Starfield texture BA2s" claim is stale (count is 30)
- **File**: `CLAUDE.md` Session 7 paragraph
- **Observation**: `ls "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data" | grep -i 'Textures.*\.ba2'` returns 30. The "22" predates a Bethesda content update.
- **Fix**: Re-stamp the Session 7 paragraph with the current archive count when the SF-D2-NEW-01 sweep test lands (the test itself becomes the source of truth).

#### SF-D2-NEW-03 — v2/v3 trailing "2×u32 unknown" bytes are read-and-dropped without sanity logging
- **File**: `crates/bsa/src/ba2.rs:221-227`
- **Observation**: 8 (v2) or 12 (v3) trailing bytes after the main header are read into an `extra` buffer and discarded. When a malformed archive disagrees, failure surfaces 50+ records deep with a confusing `failed to fill whole buffer` instead of "trailer disagrees with `name_table_offset`."
- **Fix**: One-line `log::trace!("BA2 v{} extra: {:?}", version, extra)` + sanity-check the first u32 against `name_table_offset`. Same philosophy as the `padding != 0xBAADF00D` debug-log at line 442.

---

## What's Verified Clean (no findings)

### NIF shader blocks (dim 1)
- CRC32 flag arrays: `sf1_crcs` at BSVER ≥ 132 (`FO4_CRC_FLAGS`), `sf2_crcs` at BSVER ≥ 152 (`FO76_SF2_CRCS`). Lockstep across `BSLightingShaderProperty`, `BSEffectShaderProperty`, umbrella accessor.
- `BSShaderType155` dispatch with `Fo76SkinTint { skin_tint_color: [f32; 4] }` Color4 variant; legacy `materialKind == 5u` reconciled in `apply_shader_type_data`. Starfield reuses FO76 mapping.
- `LuminanceParams` + `TranslucencyParams` + variable-length `BSTextureArray` triple gated on BSVER ≥ 155.
- WetnessParams `unknown_1` widened to BSVER > 130 (#403); `unknown_2` gated to BSVER ≥ 155 (#746).
- BSEffectShaderProperty FO76+ textures (Reflectance, Lighting, Emittance Color, Emit Gradient) all parsed at `shader.rs:1485-1498`.
- Narrow `.bgsm` / `.bgem` / `.mat` suffix gate (#749) replaced any-non-empty-Name trigger.

### BA2 v2/v3 LZ4 (dim 2)
- Exhaustive `match` over `{1, 2, 3, 7, 8}` at `ba2.rs:218-258`; `unknown_version_rejected` guards regression.
- v3 reads exactly 12 trailing bytes (not generic `if version >= 3`); `v3_unknown_compression_method_rejected` pins the closed-set dispatch.
- LZ4 path: `lz4_flex::block::decompress(packed, unpacked_size)` — explicit hint, no `decompress_size_prepended`.
- `unpacked_size` double-bounded (record-read time + `decompress_chunk` re-check).
- Real-data: v2 GNRL + v2 DX10/zlib + v3 DX10/LZ4 all extract cleanly on representative archives.

### BGSM material reference flow (dim 3)
- Stopcond at BSVER ≥ FO76 short-circuits on `.bgsm` / `.bgem` / `.mat` Name. Mirrored in `BSEffectShaderProperty::parse`.
- `material_path_from_name` plumbing at `walker.rs:122` + `:289` flows BGSM path → `ImportedMesh.material_path`.
- BGEM distinct from BGSM (separate `BgemFile` parser, magic-byte dispatch overrides extension on mismatch with warn-once).
- FO4 BSVER 130 intentional exemption (#1080): stopcond doesn't fire; inline body is the canonical source.

### Vertex format + mesh variants (dim 4)
- `BSGeometry` block fully wired — internal + external `.mesh` companion via `MeshResolver`.
- UDEC3 (10:10:10:2 unsigned-fixed) unpacker pinned by `unpack_udec3_zero_maps_to_minus_one`.
- Y-up native; no Z-up→Y-up swap needed.
- Classic `BSTriShape` / `BSSubIndexTriShape` / `BSMeshLODTriShape` still dispatched.
- VF_* set stops at `VF_INSTANCE` (0x200) + `VF_FULL_PRECISION` (0x400). No new bits for Starfield (high-density meshes use `BSGeometry`).
- `BSGeometrySegmentData` three layouts (Skyrim / FO4 / shared-data) all routed through `allocate_vec` fuzz caps.

### Real-data validation (dim 5)
- Full mesh-archive sweep: 89,276 NIFs / 5 BA2s / 98.38% clean / 100% recoverable / 0 hard failures.
- 5 spot-checks (clutter / ship hull / weapon / landscape / FaceGen) all parse with zero `NiUnknown` blocks.
- 3 BA2 packaging variants (v2 GNRL, v2 DX10+zlib, v3 DX10+LZ4 block) all extract end-to-end.
- External-`.mesh` resolution wired through engine at `byroredux/src/scene/nif_loader.rs:208`.
- Skinned FaceGen NIFs parse `BSSkin::Instance` + `BSSkin::BoneData` correctly (×11 in the spot-check head).

### ESM roadmap + forward blockers (dim 6)
- `GameKind::Starfield` classification (HEDR ≈ 0.96) at `crates/plugin/src/esm/reader.rs:99-140`; walks under current FO4 dispatch path (resolve-rate unmeasured).
- `--sf-smoke <CELL_EDID>` planning tool wired at `byroredux/src/sf_smoke.rs:42-86` (#763 / SF-D6-04). Outputs resolve-rate + record-type histogram + unresolved-form high-byte distribution.
- `BSWeakReferenceNode` parser (#754) unblocked Meshes02 from 0% → 100%.
- ESH (Medium Master, SF 1.11+) slot 0xFD plumbed in `crates/plugin/src/legacy/mod.rs:89-104`.
- `MaterialProvider` warns-once on `.mat` paths (`asset_provider.rs:1046-1063`) instead of spamming.

---

## CRC32 Flag Table

`Num SF1` u32 + `SF1[Num SF1]` array of u32 CRC32 hashes (BSVER ≥ 132). `Num SF2` u32 + `SF2[Num SF2]` array (BSVER ≥ 152). Each u32 is the CRC32 of a flag-name string.

**Current state**: parsed-but-unmapped. No CRC32-name lookup table exists in-tree. Flag arrays are stored as raw `Vec<u32>` on `BSLightingShaderProperty.sf1_crcs` / `sf2_crcs` (and the mirror fields on `BSEffectShaderProperty`); no downstream code consults them. Pre-#411 the GpuMaterial pipeline could not branch on individual flag-name bits.

**Building the table**: each flag-name CRC32 is well-defined (standard CRC32) — given a list of known flag-name strings, the hash is trivial to compute. The nif.xml `BSShaderCRC32` type has named members in some niftools branches; the latest nif.xml has the bare hash list at `BSShaderCRC32`. xEdit (SF1Edit) and the Starfield Material-Editor port both ship name lists derivable from xEdit's `SFShaderTypeFlags1.dat` and `SFShaderTypeFlags2.dat`.

**Not in this audit**: building the table empirically. This is a feature-add, not a bug, and it's blocked by needing the GpuMaterial side to branch on individual flag bits (which depends on #762's material data being available).

---

## BGSM-vs-`.mat` Readiness Table — Starfield-side

| Format | Parser | Status |
| --- | --- | --- |
| BGSM v1-v22 | `crates/bgsm/src/bgsm.rs` | DONE (FO4/Skyrim/FO76) |
| BGEM v1-v22 | `crates/bgsm/src/bgem.rs` | DONE |
| Starfield `.bgsm` (>v22) | — | **No parser** — Starfield doesn't use v22 binary form; uses `.mat` JSON instead. Out of scope. |
| Starfield `.mat` (JSON) | — | **#762** — open. Loose `.mat` only appears in SDK content (Tools/ContentResources.zip) and mod packs. |
| Starfield `materialsbeta.cdb` | — | Deferred under #762 — vanilla Starfield ships ALL materials inside this single binary CDB; loose `.mat` is SDK-only. |

---

## Forward Blocker Chain — "Starfield mesh renders with real material"

Per the 2026-04-27 audit, steps 1, 2, 4, 5, 6 are all DONE since:
1. NIF parse (BSGeometry etc.) — **DONE** (#708)
2. BA2 v2/v3 + LZ4 block extract — **DONE** (Session 7)
3. `bsver == 155 → >= 155` gate sweep (SF-D1-01..04 ≡ #109 family) — status unclear, verify on the open-issue list
4. BSGeometry → ImportedMesh internal path — **DONE** (`bs_geometry.rs:29-35`)
5. External `.mesh` companion decoder — **DONE** (`bs_geometry.rs:36-62`)
6. `BSWeakReferenceNode` parser — **DONE** (#754)
7. **Starfield `.mat` JSON parser + `MaterialProvider` integration** — **NOT DONE** (#762 / SF-D6-03) — critical-path
8. `materialsbeta.cdb` component-database reader — **NOT DONE** (deferred under #762; vanilla needs this)

**Critical path today: step 7. Step 3 needs a closeout-verification check.**

## Forward Blocker Chain — "Starfield cell renders"

Builds on the mesh chain plus:
9. `Starfield.esm` CELL group walked under current FO4 dispatch — **MEASUREMENT TOOL EXISTS** (`--sf-smoke` per #763); no published resolve-rate number
10. SF REFR resolution (FormID → STAT → mesh) — UNVERIFIED, gated on 9
11. SF-only record types (`PNDT` / `STDT` / `BIOM` / `SFBK` / `SUNP` / `GBFM` / `GBFT`) — NOT DONE
12. Evolved-from-FO4 records (`STAT`/`CELL`/`REFR`/`LIGH`/`DOOR`/`MSTT`/`LGTM`) validated on SF schema — UNVERIFIED
13. Space-cell concept — NOT DONE (different addressing/transform regime than interior 1g cells)
14. **Procedural ship assembly** — **out of scope** until 9-13 land; gameplay-code-driven, not statically authored. Impossible without working SF form-linker.

## Lowest-effort visible-progress milestone

Already works today: `cargo run -- --bsa "Starfield - Meshes01.ba2" --mesh meshes\<some.nif>` yields actual geometry on screen with the magenta checkerboard where the missing `.mat` would have been.

**Next concrete step**: run `--sf-smoke <CELL_EDID>` (already wired) against the smallest interior in `Starfield.esm` and publish the resolve-rate number. That single measurement decides whether SF ESM work is a fix-up patch or a from-scratch parser, and unblocks Milestone B sizing.

---

## Verification Commands Run

- `cargo test --release -p byroredux-nif --test parse_real_nifs -- --ignored parse_rate_starfield_all_meshes` → 89,276 NIFs / 87,829 clean / 100% recoverable (live numbers reproduce the table above)
- `cargo test --release -p byroredux-nif --test parse_real_nifs -- --ignored parse_rate_starfield --nocapture` → 31,058 / Meshes01 alone (matches ROADMAP)
- 5 representative spot-checks via `d5_starfield_import` → zero unknowns, expected block types only

---

## Dedup Notes

- SF-D2-NEW-01/02/03 are fresh — no duplicates in `/tmp/audit/issues.json`.
- SF-D1-NEW-01 (`Root Material` discard) — fresh; no prior issue mentions `root_material`.
- #762 (`SF-D6-03: Starfield .mat (JSON) material file parser + provider integration`) is the only open Starfield-specific issue and the canonical home for material-side work.
- #109 family (`bsver == 155 → >= 155` gate sweep) — referenced in dim 6; status verification deferred to the publish phase.
- The dim 5 spot-check results match what the 2026-04-27 audit reported, just with an updated total (89k vs whatever Session 7 cited).

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-05-18.md`
