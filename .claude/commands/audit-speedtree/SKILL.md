---
description: "Audit the SpeedTree (.spt) TLV parser + placeholder-billboard fallback shipped in Session 33 Phase 1"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# SpeedTree Subsystem Audit

Audit the `byroredux-spt` crate (Session 33 Phase 1, since grown to 7 `src/` modules + 5 example analyzers) for TLV walker correctness, tag coverage against the FNV/FO3/Oblivion `.spt` corpus, the ≥95% acceptance gate, the placeholder-billboard fallback that keeps cell loads alive when a tree fails to decode, and the NIFAL material translation the placeholder mesh now flows through.

**Architecture**: Single-pass — small enough to run dimensions inline rather than spawning Tasks.

See `.claude/commands/_audit-common.md` for project layout, methodology, deduplication, context rules, and finding format.

## Scope

**Crate**: `crates/spt/src/` (Session 33 Phase 1 — TLV walker + placeholder fallback).

**Cross-cuts**:
- `byroredux/src/cell_loader/references.rs` — extension switch routes `.spt` to the SpeedTree importer instead of NIF (`is_spt` test at ~478-490 → `parse_and_import_spt` at ~1080). `refr.rs` does NOT carry the `.spt` route.
- `byroredux/src/scene/nif_loader.rs` — `--tree` direct-visualisation CLI entry (`--tree trees\\joshua01.spt`; `--mesh foo.spt` is equivalent — both branch on the `.spt` extension)
- `crates/plugin/src/esm/records/tree.rs` — TREE record parser (was previously falling into the generic record path and losing texture/billboard data)
- `byroredux/src/material_translate.rs::translate_material` — the NIFAL canonical boundary that consumes the spt placeholder `ImportedMesh` at `cell_loader/spawn.rs:861` (`Material::resolve_pbr` fills any unset PBR slot). See `docs/engine/nifal.md` and `/audit-nifal`.

**Crate layout** (Session 33+ — no longer a single file; `crates/spt/src/`):
- `parser.rs` — `parse_spt(&[u8]) -> io::Result<SptScene>`, tag-band guards `TAG_MIN = 100` / `TAG_MAX = 13_999`
- `tag.rs` — `SptTagKind` (9 payload kinds: `Bare`/`U8`/`U32`/`Vec3`/`FixedBytes(u8)`/`String`/`ArrayBytes{stride}`/`Unknown`/`MaybeStringElseBare`) + `dispatch_tag(u32)`
- `version.rs` — `detect_variant(&[u8])`, `SpeedTreeVariant` (`V4Oblivion`/`V5Fo3`/`V5Fnv`/`Unknown`)
- `stream.rs` — `SptStream<'a>` (LE readers, EOF guard)
- `scene.rs` — `SptScene` / `SptValue` / `TagEntry`
- `recon/mod.rs` — feature-gated reconstruction
- `import/mod.rs` — placeholder importer (`compute_billboard_size`, `ImportedMesh` build)
- Recon analyzers live in `crates/spt/examples/` (`spt_dissect.rs`, `spt_tagmap.rs`, `spt_transitions.rs`, `spt_walk.rs`, `spt_recon.rs`); format notes at `crates/spt/docs/format-notes.md`

**Phase 1 acceptance** (ground truth — verify before reporting):
- Single-file dissector + tag dictionary recovered against the corpus
- TLV walker ≥95% on FNV/FO3/Oblivion `.spt` corpus (`>5 GB joshua01.spt` etc.)
- Importer placeholder fallback — un-decoded trees render as a billboard card (better than parse panic)
- `.spt` references in cell records route to the SpeedTree importer, not NIF
- `--tree` smoke test passes

**Future phases (NOT yet shipped — do not flag as missing unless scope includes them)**: full geometry recovery (real branch/leaf mesh, not billboard), wind-bone animation from `BSTreeNode`, distance-LOD swap, baked-shadow lookup.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: Comma-separated dimension numbers (e.g., `1,3`). Default: all 6.
- `--depth shallow|deep`: `shallow` = walker contract check; `deep` = run corpus against the walker + diff against the baseline. Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: TLV Format | Tag Coverage | Corpus Acceptance | Placeholder Fallback | Routing & CLI | NIFAL Material Translation

## Phase 1: Setup

1. `mkdir -p /tmp/audit/speedtree`
2. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels --search "speedtree OR .spt OR TREE" > /tmp/audit/speedtree/issues.json`
3. Confirm corpus path: `find /mnt/data/SteamLibrary/steamapps/common -iname '*.spt' 2>/dev/null | head` — and the in-BSA paths (`trees\\*.spt` in FNV `Fallout - Meshes.bsa`, FO3 `Fallout - Meshes.bsa`, Oblivion `Oblivion - Meshes.bsa`)

## Phase 2: Dimensions

### Dimension 1: TLV Format Correctness
**Entry points**: `crates/spt/src/parser.rs` (`parse_spt`, `TAG_MIN`/`TAG_MAX`), `crates/spt/src/stream.rs` (`SptStream` LE readers + EOF guard), `crates/spt/src/version.rs` (`detect_variant`, `SpeedTreeVariant`), `crates/spt/tests/parse_synthetic_spt.rs`
**Checklist**:
- Header magic + version bytes recognised across FNV/FO3/Oblivion variants (TLV format isn't versioned by a global field — verify each entry point claims its own header)
- Tag-length-value walker correctly skips unknown tags using their length (no byte-misalignment cascade past the first unknown tag)
- Length field byte-width is consistent (LE u32 per current dissector; flag if any variant ships u16)
- Walker stops cleanly at EOF — no off-by-one read past file end on the last tag
- Negative / zero / pathological lengths bail with `Err`, not panic — `.spt` is artist-shipped data, must not crash the cell loader
- Endian: LE everywhere (no big-endian fallback path); compile-error gate if a future big-endian host is added

### Dimension 2: Tag Coverage
**Entry points**: `crates/spt/src/tag.rs` (`SptTagKind` + `dispatch_tag`), recon analyzers in `crates/spt/examples/` (`spt_tagmap.rs`, `spt_transitions.rs`, `spt_dissect.rs`), format notes at `crates/spt/docs/format-notes.md`
**Checklist**:
- `dispatch_tag` currently recognises ~14 tag values across 9 `SptTagKind` payload kinds; the ~40-known-tags target (texture path, billboard descriptor, branch geometry, leaf cluster, wind params, LOD distances) is aspirational for Phase 1 — flag the gap as an open finding, not a stale claim
- Any tag that appears in the corpus at ≥1% frequency MUST have either a parser or an explicit skip-with-rationale comment
- Texture-path tags resolve through the same `resolve_texture` / sibling-BSA auto-load path that NIF uses — verify no parallel "spt resolver" duplicates the logic
- Billboard tag captures: texture, world-space width/height, mip bias — these flow into the placeholder importer
- "Last tag wins" vs "first tag wins" semantics for duplicate tags — confirm and document

### Dimension 3: Corpus Acceptance (≥95% gate)
**Entry points**: `crates/spt/tests/parse_real_spt.rs` (or equivalent), corpus location resolved via env-var (mirror the NIF `BYROREDUX_*_DATA` pattern)
**Checklist**:
- Acceptance harness runs over FNV + FO3 + Oblivion `.spt` corpus and reports walker-clean rate
- Threshold: ≥95% (Phase 1 gate). Under-95% = audit finding even if every per-file failure is graceful
- Walker-clean ≠ semantically-correct — failures should be bucketed (truncation, unknown tag exceeding length, header mismatch, etc.) with corpus-wide histogram
- Regression-guard sample: 3-5 specific `.spt` files pinned by SHA, in-tree, should parse byte-stable across runs
- Memory: `parse_spt` currently takes a `&[u8]` (caller pre-loads the file). The "never materialise the whole file" bar is a forward-looking quality goal that the present `&[u8]` API cannot satisfy — flag as design debt, not a walker bug; verify the largest `joshua01.spt` corpus entry doesn't blow the cell-loader's transient memory budget when slurped whole

### Dimension 4: Placeholder Fallback
**Entry points**: `crates/spt/src/import/mod.rs` (`compute_billboard_size`, the `ImportedMesh` billboard build at ~281+ with the normal/winding convention documented at ~249-258), `byroredux/src/scene/nif_loader.rs`, billboard mesh creation in `crates/renderer/src/mesh.rs`
**Checklist**:
- When the walker fails OR no billboard tag was captured, importer returns a placeholder card (Quad mesh + magenta-checker placeholder texture OR the texture from the billboard tag if available)
- Placeholder return is non-null — must never `Err` out of the cell loader (graceful degradation is the Phase 1 contract)
- Placeholder sizing precedence is **OBND → BNAM (#1002) → Oblivion MODB (#1001) → 256×512 default** (`DEFAULT_BILLBOARD_WIDTH` / `DEFAULT_BILLBOARD_HEIGHT`); guard the `compute_billboard_size` ordering — vanilla Oblivion TREEs ship MODB-only (OBND on none) so an OBND-first-or-default path renders junipers at half scale
- **Billboard normal faces `-Z`, winding flipped to `[0, 3, 2, 2, 1, 0]`** (#1000): the entity rotates via `Quat::from_rotation_arc(-Z, look_dir)`, so the object-space `-Z` axis ends up pointing at the camera and the front face must be `-Z` to render toward it. Pre-#1000 normals pointed `+Z` and `two_sided: true` masked the inverted convention. Verify the Z-up→Y-up `bs_bound` swap (#995) is applied
- Two-sided rendering enabled on the placeholder (foliage shouldn't disappear when camera looks from behind); leaf-card path uses alpha-test cutout (`alpha_test: true`, `alpha_threshold: 0.5`, `alpha_test_func: 6` GREATEREQUAL)
- Material slot: placeholder goes through the same `MaterialTable::intern` path as NIF meshes — verify dedup applies (a forest of 1000 juniper bushes should produce 1 material, not 1000)

### Dimension 5: Routing & CLI
**Entry points**: `byroredux/src/cell_loader/references.rs` (`is_spt` extension dispatch ~478-490; `parse_and_import_spt` ~1080), `byroredux/src/scene/nif_loader.rs` (`--tree` flag), `crates/plugin/src/esm/records/tree.rs` (TREE parser)
**Checklist**:
- Cell-loader `.spt` route fires when the REFR's base record is TREE and the model path ends in `.spt`; mixed `.nif` + `.spt` in the same cell coexist
- `.spt` references in BSA archives resolve through the same lookup chain as `.nif` (sibling-BSA auto-load, AE pipeline-path strip applied if relevant)
- `--tree path/to/x.spt` CLI entry instantiates the same code path as the cell-loader route (no parallel "direct viz" stub that drifts from the in-engine path)
- TREE record parser (Session 33 dedicated dispatch) captures texture / billboard / shadow data without dropping fields — pre-fix every `.spt`-referencing TREE silently lost its authoring
- `parse_and_import_spt` returns the same `CachedNifImport` shape every other model uses, with synthetic defaults the generic spawn path must not mis-read as NIF-rooted: `placement_root_billboard = Some(BillboardMode::BsRotateAboutUp)` (#994), `bsx_flags = 0` (#1214), `root_flags = 0` (#1235), `flame_attach_offset = None` (Phase 18). Verify the spawn site reads these without assuming an `.spt` placeholder carries a real NiAVObject root / BSXFlags / flame marker
- Cell unload despawns the SpeedTree entities cleanly; no leaked BLAS entries for placeholder billboards
- Failed `.spt` import does NOT block the rest of the cell from loading (graceful degradation)

### Dimension 6: NIFAL Material Translation for Placeholders
**Entry points**: the `ImportedMesh` material defaults in `crates/spt/src/import/mod.rs` (~281+), the canonical boundary `byroredux/src/material_translate.rs::translate_material` consumed at `byroredux/src/cell_loader/spawn.rs:861`, `crates/core/src/ecs/components/material.rs::Material::resolve_pbr`
**See also**: `/audit-nifal` (the dedicated NIFAL canonical-translation-tier audit) — spt is one of its `Imported* → translate() → Canonical` producers; cross-check single-boundary / no-fabrication findings there rather than duplicating them here.
**Checklist**:
- The spt placeholder `ImportedMesh` is canonicalised at the **single** NIFAL boundary (`translate_material` → `resolve_pbr`), not at render time — no parallel "spt material" path that bypasses `material_translate.rs`
- Billboard material defaults survive translation intact: `is_pbr: false`, `metalness_override: None`, `roughness_override: None`, `from_bgsm: false` — `resolve_pbr` should fill `Material.metalness`/`roughness` (now plain resolved `f32`, not `Option`) from the non-PBR keyword path, NOT promote the billboard to metallic-roughness
- `emissive_source: EmissiveSource::None` (#1280) holds through translation — a tree billboard must not pick up an emissive lobe
- `#1241` (BSLightingShaderProperty PBR scalars surfaced at import) does not regress spt: SpeedTree never resolves a BGSM/BGEM, so the PBR-scalar fields stay at their non-PBR defaults — guard that a82366e9-style import-side PBR plumbing left the spt billboard non-PBR
- Two-sided alpha-test cutout (Dimension 4) maps to the correct canonical `Material` flags after translation (foliage silhouette preserved, not opaque-blitted)

## Phase 3: Output

Write findings to **`docs/audits/AUDIT_SPEEDTREE_<TODAY>.md`** following the base finding format. Suggest `/audit-publish` on completion.
