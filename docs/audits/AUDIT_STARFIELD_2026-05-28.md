# Starfield Compatibility Audit — 2026-05-28

**Scope**: All 6 dimensions. Engine HEAD `bf386879` (post-#1283 audit-runtime skill, pre-/audit-renderer findings #1285-#1287).
Live data: `/mnt/data/SteamLibrary/steamapps/common/Starfield/Data/` (108 BA2s, 30 texture archives, 21 ESMs available — none parsed).
Methodology: Self-audit (agent fanout deferred due to API rate-limit hits earlier in the session). Findings cross-checked against `/tmp/audit/starfield/issues.json` (200 closed issues) — all 2026-05-18 audit findings closed.

## Executive Summary

Starfield is at "geometry renders, materials still render Lambert" parity. The parser side is now COMPLETE end-to-end (NIF + BA2 + CDB binary database), but **the CDB consumer is unwired**, so vanilla Starfield content silently falls through to the legacy Lambert + magenta-placeholder path despite the .cdb parser landing 4 days ago.

State map vs 2026-05-18 audit:

- **NIF parse**: unchanged at 98.6% aggregate / 100% recoverable across all 5 vanilla mesh BA2s. Per-archive: Meshes01 97.21% (31 058 NIFs), Meshes02 100% (7 552), MeshesPatch 98.11% (29 849), LODMeshes 99.92% (19 535), FaceMeshes 100% (1 282).
- **BA2 v2/v3**: production-correct. Exhaustive `match` over `{1, 2, 3, 7, 8}`, 12-byte v3 extension with `compression_method` dispatch, GNRL + DX10 unified through `decompress_chunk()`. All 2026-05-18 LOW findings closed (#1184 corpus-wide sweep, #1185 stale "22 archives" claim, #1186 trailing-bytes sanity log).
- **NIF BSVER 155-172+ shader blocks**: complete. The 2026-05-18 MEDIUM SF-D1-NEW-01 (Root Material discard) closed as #1183; fallback wired at `walker.rs:141-142`. Post-#1279 split shader.rs into `parse_skyrim` / `parse_fo4` / `parse_fo76_plus` arms.
- **BSGeometry import**: complete + hardened. #1232 (empty tangents → Mikkelsen synthesis), #1263 (3 remaining per-vertex push loops), #1203 (skin_instance_ref unused — every Starfield NPC had `skin: None`) all closed since 2026-05-18.
- **Material CDB (Stage B of #762)**: parser landed in `crates/sfmaterial/` (1048 LOC). Closed 2026-05-24.
- **Material CDB consumer**: **NOT WIRED**. `byroredux/src/asset_provider.rs` has zero references to the sfmaterial crate. Starfield meshes get `material_path` captured (e.g., "materials/cargobay.mat") but `ImportedMesh.is_pbr` stays false → `MAT_FLAG_PBR_BSDF` never set → Disney BSDF unreachable for SF content. **This is the new top blocker** (SF-D3-NEW-01).
- **ESM parser**: still unimplemented. No SF-specific record types (`PNDT` / `STDT` / `BIOM` / `SFBK` / `SUNP` / `GBFM` / `GBFT`). 21 SF ESMs in `Data/` (vanilla + Constellation + Shattered Space DLCs) unreachable.

## Dimension Findings

| Dim | Findings | CRITICAL | HIGH | MEDIUM | LOW |
|-----|---------:|---------:|-----:|-------:|----:|
| 1 — Shader Blocks | 0 | 0 | 0 | 0 | 0 |
| 2 — BA2 v2/v3 LZ4 | 0 | 0 | 0 | 0 | 0 |
| 3 — BGSM Material Reference Flow | 1 | 0 | 1 | 0 | 0 |
| 4 — Vertex Format & Variants | 0 | 0 | 0 | 0 | 0 |
| 5 — Real-Data Validation | 0 | 0 | 0 | 0 | 0 |
| 6 — ESM Roadmap | 1 | 0 | 0 | 0 | 1 |
| **Total** | **2** | **0** | **1** | **0** | **1** |

### HIGH

#### SF-D3-NEW-01 — Starfield CDB parser exists but consumer is unwired; every Starfield mesh renders Lambert instead of Disney BSDF

- **File**: `byroredux/src/asset_provider.rs` (no `sfmaterial` references); breaks the chain at `byroredux/src/cell_loader.rs:192-194` (`if mesh.is_pbr { flags |= BGSM_PBR; }`)
- **Observation**: #762 closed 2026-05-24 with the CDB parser landed (`crates/sfmaterial/`, 1048 LOC). The lib.rs at lines 44-49 documents that "consumer-side mapping … happens in `byroredux/src/asset_provider.rs` and is a separate concern from the format parsing here." That file today has ZERO references to the new crate. Chain:
  1. Starfield mesh parses, stopcond captures `material_path = "materials/cargobay.mat"` ✓
  2. Walker plumbs `material_path` into `MaterialInfo` ✓
  3. `pack_bgsm_material_flags` checks `mesh.is_pbr` — **always false for Starfield** because nothing reads the CDB ✗
  4. `flags = 0` → no `MAT_FLAG_PBR_BSDF` → Disney BSDF unreachable; Starfield content silently renders Lambert
- **Why bug**: Exact regression pattern the audit checklist warned about. Starfield is PBR-canonical (vanilla content expects metalness/roughness Disney BSDF). Visible symptom: magenta-checker albedo (missing texture) WITH wrong lighting model on top. Closing the texture half (textures BA2 already wired) without the PBR flag means even materials that DO resolve textures get Lambert instead of Disney.
- **Fix** (cheap → correct):
  1. Lift `ComponentDatabaseFile::parse(materialsbeta.cdb)` once at engine init in `asset_provider.rs` — CDB lives inside `Starfield - Materials.ba2`.
  2. Build a `material_path → MaterialFields` lookup table keyed on the `.mat` path captured by the NIF stopcond.
  3. Extend `pack_bgsm_material_flags` (or sibling `pack_sfmaterial_flags`) to consult the SF-material table when `mesh.material_path` has a `.mat` suffix. Set `BGSM_PBR | BGSM_AUTHORED` plus any translucency / model-space-normals bits the CDB authoring carries.
- **Effort**: 1-2 PRs. Already-landed CDB parser does the hard binary-format work; integration is wiring + table walk.

### LOW

#### SF-D6-NEW-01 — Forward-blocker chain has SHIFTED post-#762: CDB consumer is now the top blocker, not the parser

- **File**: ROADMAP.md compat-matrix section + prior audit's forward-blocker chain
- **Observation**: The 2026-05-18 audit listed `#762` (CDB parser) at the top of the forward-blocker chain. That issue closed 2026-05-24 — but the **consumer-side mapping** didn't land in the same PR. The top blocker today is SF-D3-NEW-01 (sfmaterial → asset_provider consumer), not the parser.
- **Why bug**: Roadmap drift. Audits N months from now might assume Starfield is rendering with real materials based on the closed #762, when actually nothing is plumbed.
- **Fix**: Re-order the next ROADMAP refresh:
  1. **SF-D3-NEW-01** (sfmaterial → asset_provider consumer) — actual top blocker
  2. Optional `.mat` JSON sidecar parser (deferred — no vanilla content uses it)
  3. `--sf-smoke` resolve-rate measurement
  4. SF-only record types + ESM parsing

## CRC32 Flag Table

No empirical mapping table for `BSShaderCRC32 → flag name` is maintained in-tree today. The parser captures the raw u32 CRC32 hashes (via the `BSShaderCRC32` enum) but the semantic mapping is unused — none of the downstream rendering paths consult these flags for Starfield content. Adding a known-hash table would unblock per-flag-name diagnostics (currently the only diagnostic is "this mesh has N flags set" with no labels). Tracked as a forward task; not blocking the dimension audit.

## Forward Blocker Chain (re-ordered post-2026-05-28)

To unlock "Starfield mesh renders with real PBR material":

1. **SF-D3-NEW-01** — Wire `crates/sfmaterial::ComponentDatabaseFile` → `MaterialInfo` → `MAT_FLAG_PBR_BSDF`. Single-PR effort; visible result is "any Starfield mesh with a CDB-resolved material renders with Disney BSDF instead of Lambert + magenta-checker."
2. *(Optional)* `.mat` JSON sidecar parser — for mod authoring; deferred since vanilla content ships nothing.
3. CRC32-flag-name reverse table — empirical sampling against known Bethesda flag-bit names (low priority; observability only).

To unlock "Starfield interior cell renders":

4. **`--sf-smoke` resolve-rate measurement** — quantifies how many Starfield form-id references resolve against the (unparsed) ESM corpus. Tool already exists per #763 doc; running it publishes the number that decides whether SF ESM work is a fix-up patch or a from-scratch parser.
5. **Starfield ESM parser** — entirely new record types (`PNDT` planet, `STDT` star, `BIOM` biome, `SFBK` space-block, `SUNP` sun-plane, `GBFM` grav-form, `GBFT` grav-table). xEdit reverse engineering is the available reference. Effort: weeks-to-months for a basic interior-cell-only stub.
6. **Space-cell concept** — Bethesda's procedural-planet model needs first-class engine support beyond the cell streaming chain. M64-tier (decoupled from form-linker; orthogonal to ESM work).
7. **Procedural ship assembly** — gameplay-driven (Form-linker chains MODL→STAT→hull snap-point references). Out of scope until ESM works.

## What's Possible Today

- **Individual Starfield mesh visualization**: `cargo run -- --bsa "Starfield - Meshes01.ba2" --mesh path\to.nif --textures-bsa "Starfield - Textures01.ba2"`. Geometry + textures resolve correctly; lighting is Lambert (the SF-D3-NEW-01 finding). Lighting will become Disney BSDF the moment the CDB consumer wires.
- **No cell loading**: no SF ESM parser. The 21 ESMs in `Data/` are not consumed.
- **No NPC bodies with skinning**: were broken pre-#1203 (every Starfield NPC body imported with `skin: None`); now should work but not re-validated this audit.

## References

- Per-dimension outputs: `/tmp/audit/starfield/dim_1.md` through `dim_6.md`
- Dedup baseline: `/tmp/audit/starfield/issues.json` (200 issues, all CLOSED)
- Prior audit: `docs/audits/AUDIT_STARFIELD_2026-05-18.md` (most findings closed; SF-D3-NEW-01 is the new top blocker that emerged post-#762 closeout)
- Key commits since prior audit:
  - 2026-05-24: #762 closed (CDB parser landed in `crates/sfmaterial/`, but consumer unwired — root cause of SF-D3-NEW-01)
  - 2026-05-22+: #1232, #1263, #1203 closed (BSGeometry hardening)
  - 2026-05-22+: #1185, #1184, #1186 closed (BA2 doc + sweep + sanity log)
  - 2026-05-22+: #1183 closed (Root Material fallback wired)
  - 2026-05-20: Session 34 split shader.rs into per-variant dispatch (#1279 follow-up)

Suggest: `/audit-publish docs/audits/AUDIT_STARFIELD_2026-05-28.md` (will file 1 HIGH + 1 LOW).
