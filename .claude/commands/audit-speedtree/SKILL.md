---
description: "Audit the SpeedTree (.spt) TLV walker + placeholder-billboard fallback shipped in Session 33 Phase 1"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# SpeedTree Subsystem Audit

Audit the `byroredux-spt` crate — a young, deliberately small subsystem
(Session 33 Phase 1, "S1"). It does two things: (1) walks the `.spt`
parameter section as a tag-length-value stream for FNV / FO3 / Oblivion,
and (2) emits a **placeholder billboard** `ImportedScene` so TREE cells
render *something* instead of panicking or going treeless. The real
geometry tail is **not** decoded — everything past `tail_offset` is left
on the floor by design.

Keep the audit proportionate. The highest-risk surface is the **walker's
byte accounting** (one mis-sized payload desyncs the whole stream), then
the **placeholder fallback correctness**, then **TREE record → billboard
wiring**, then **per-game `.spt` differences**. The tag dictionary is
large but each entry is a fixed-size decode — low risk individually.

**Architecture**: Single-pass — small enough to run all dimensions inline
rather than spawning Tasks.

See `.claude/commands/_audit-common.md` for project layout, methodology,
deduplication, context rules, severity, and finding format. Do not
re-derive any of those here.

## Scope

**Crate**: `crates/spt/src/` — `parser.rs`, `tag.rs`, `version.rs`,
`stream.rs`, `scene.rs`, `crates/spt/src/import/mod.rs`, plus the
feature-gated `crates/spt/src/recon/mod.rs`. Public surface re-exported
from `crates/spt/src/lib.rs`: `parse_spt`, `import_spt_scene`,
`SptImportParams`, `SptScene`, `detect_variant`, `dispatch_tag`.
(`compute_billboard_size` is a *private* helper in the import module, not
an entry point.)

**Cross-cuts** (the wiring that actually invokes the crate):
- `byroredux/src/cell_loader/references/mod.rs` — the production route. An
  `is_spt` extension check (`model_path … eq_ignore_ascii_case("spt")`)
  dispatches to `parse_and_import_spt`, which looks up the matching TREE
  record from `record_index.trees` and threads its metadata into
  `SptImportParams`. `refr.rs` does **not** carry a `.spt` route.
- `byroredux/src/scene/nif_loader.rs` — the `--tree` / loose-file
  direct-visualiser route (`parse_import_and_merge`, `is_spt` branch).
  This is a **parallel** path: it calls `import_spt_scene` with
  `SptImportParams::default()` — **no TREE metadata** (no ICON override,
  no OBND/MODB/BNAM sizing). Verify the two routes don't silently diverge
  in ways that matter (Dimension 4).
- `crates/plugin/src/esm/records/tree.rs` — `parse_tree` → `TreeRecord`.
  Captures OBND / ICON / MODB / SNAM / CNAM / BNAM / PFIG. `has_speedtree_binary()`
  is the case-insensitive `.spt` predicate. Pre-S1 TREE fell into the
  generic MODL-only path and dropped every field but MODL.
- `byroredux/src/systems/billboard.rs` — `make_billboard_system` rotates
  any entity carrying a `Billboard` component. `BsRotateAboutUp` (the mode
  the spt placeholder uses) currently falls back to the world-up yaw lock
  — confirm that's still the behaviour and that the `-Z` front-face
  convention in `import/mod.rs` matches the rotation arc used here.
- `byroredux/src/cell_loader/spawn.rs` — attaches the `Billboard`
  component from `cached.placement_root_billboard`, and routes the
  placeholder `ImportedMesh` through the NIFAL boundary
  (`crate::material_translate::translate_material`).

**Phase 1 ("S1") acceptance** (ground truth — verify before reporting):
- TLV walker recovered against the FNV/FO3/Oblivion `.spt` corpus
  (133 vanilla files; Oblivion ≈ 113).
- Acceptance gate ≥ 95 % *unknown-tag-clean* rate, asserted in
  `crates/spt/tests/parse_real_spt.rs` (env-var gated, `#[ignore]`).
- Placeholder fallback: un-decoded trees render as a billboard card,
  never an `Err` out of the cell loader.
- `.spt` REFRs route to the SpeedTree importer, not NIF.
- `--tree` smoke path parses + imports.

**Future phases (NOT shipped — do not flag as missing unless `--focus`
explicitly includes them)**: real branch/leaf mesh recovery from the
geometry tail, wind-bone animation from SNAM/CNAM, distance-LOD swap,
baked-shadow lookup. SNAM/CNAM are *parsed but not consumed* (TD5-011
gate) — that's intentional, not a drop.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated dimension numbers. Default: all.
- `--depth shallow|deep`: `shallow` = walker contract + wiring review from
  source only; `deep` = also run the corpus harness against on-disk BSAs.
  Default: `deep`.

## Extra Per-Finding Fields

- **Dimension**: Walker Byte-Accounting | Placeholder Fallback | TREE→Billboard Wiring | Per-Game Variants | Tag Dictionary | NIFAL Material Translation

## Phase 1: Setup

1. `mkdir -p /tmp/audit/speedtree`
2. Dedup query (per `_audit-common.md`):
   `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels --search "speedtree OR spt OR TREE" > /tmp/audit/speedtree/issues.json`
3. Read the **most recent** `docs/audits/AUDIT_SPEEDTREE_*.md` report (sort by
   date — do not hardcode a filename here, it rots every cycle) and diff
   direction against it rather than re-deriving from scratch. Findings
   SPT-D4-01/02/03/04, SPT-D5-01/02, SPT-D2-01, SPT-D3-01, SPT-D1-01 are all
   **closed** (#994/#995/#996/#997/#998/#999/#1000/#1001/#1002). Of the later
   SPT-NEW batch, SPT-NEW-01 (dead-code `detect_variant`, #1820) and
   SPT-NEW-06 (format-notes byte-align, #1821) are also **closed** — only
   SPT-NEW-07 (`MaybeStringElseBare` misparse risk on a bare tag-13005
   immediately before the geometry tail, #1822) remains open. Treat closed
   findings as **regression guards**, not open items; only re-flag if a guard
   has actually broken.
4. `deep` only — corpus + recon harness:
   - corpus location: `Fallout - Meshes.bsa` (FNV/FO3), `Oblivion - Meshes.bsa`.
   - acceptance run: `BYROREDUX_FNV_DATA=… cargo test -p byroredux-spt --release --test parse_real_spt -- --ignored --nocapture` (and `_FO3_DATA` / `_OBL_DATA`).
   - per-file dumps: the `recon` examples are **feature-gated** —
     `cargo run -p byroredux-spt --features recon --example spt_walk` (also
     `spt_tagmap`, `spt_transitions`, `spt_dissect`, `spt_recon`). Findings
     log to `crates/spt/docs/format-notes.md`.

## Phase 2: Dimensions

Ordered by SpeedTree risk.

### Dimension 1: Walker Byte-Accounting
**Highest risk.** The walker has no length-prefixed framing for most tags —
it advances by the *payload kind's* fixed size. A single wrong size in the
dictionary desyncs every subsequent read.
**Entry points**: `crates/spt/src/parser.rs` (`parse_spt`, `read_payload`,
`TAG_MIN`/`TAG_MAX`), `crates/spt/src/stream.rs` (`SptStream` LE readers +
`is_eof`/`remaining` guards), `crates/spt/src/parser.rs` tests.
**Checklist**:
- Each `SptTagKind` decode advances the cursor by exactly the bytes it
  claims: `U8`=1, `U32`=4, `Vec3`=12, `FixedBytes(n)`=n, `String`=4+len,
  `ArrayBytes{stride}`=4+count×stride, `Bare`=0. Cross-check `read_payload`
  against the dictionary in `tag.rs` for any size mismatch.
- The `MaybeStringElseBare` branch (tag 13005, #999) consumes the tag,
  then peeks the next u32 to decide Bare vs String. Confirm it re-syncs
  cleanly on **both** arms and that a `None` peek (EOF) can't panic —
  the `tag_13005_at_eof_does_not_panic` regression guard.
- Walker stops cleanly at `is_eof()` (sets `reached_eof`) and at an
  out-of-range / `Unknown` tag (records `tail_offset` + `unknown_tags`),
  never reads one tag past EOF.
- Pathological lengths: `read_string_lp` and `ArrayBytes` both cap at
  64 KiB and bail with `Err`, not OOM. Confirm the cap is on the *byte
  count* (count × stride), not just `count`.
- `parse_spt` returns `Err` only on the two fatal conditions (missing
  magic, mid-payload underflow). In-range-but-unknown tags are non-fatal
  (recorded, walker stops) — this is the contract the placeholder relies
  on. Any new fatal-error path is a HIGH finding (it kills the cell-loader
  fallback).
- Endian: LE-only, unconditional (no version-gated readers — every `.spt`
  is `__IdvSpt_02_`). Flag any big-endian assumption or host-endian read.

### Dimension 2: Placeholder Fallback Correctness
**Entry points**: `crates/spt/src/import/mod.rs` (`import_spt_scene`,
`compute_billboard_size`, `placeholder_billboard_mesh`,
`placeholder_root_node`, `DEFAULT_BILLBOARD_WIDTH`/`_HEIGHT`),
`byroredux/src/systems/billboard.rs`.
**Checklist**:
- `import_spt_scene` **always** returns a one-node / one-mesh
  `ImportedScene` — there is no `Err` path out of it. The only way the
  cell loader gets `None` is `parse_spt` returning `Err` (magic /
  underflow); confirm that path logs + skips the REFR without aborting
  the rest of the cell (graceful-degradation contract).
- Leaf-texture precedence: TREE.ICON override → `.spt` tag 4003 (first
  wins, #997) → unset (renderer magenta placeholder). Regression guards:
  `leaf_texture_override_wins_over_spt_tag`,
  `empty_texture_leaves_path_unset_for_renderer_placeholder`.
- Billboard sizing precedence in `compute_billboard_size`: **OBND →
  BNAM → MODB → 256×512 default**, every path clamped to `[16, 8192]`
  (#1001/#1002). OBND beats BNAM intentionally (BNAM clamps tall trees,
  e.g. WhiteOak01 OBND 802×1567 vs BNAM 768×768). Vanilla Oblivion ships
  MODB-only / no OBND, so an OBND-first-or-default path would render
  Cyrodiil pines at half scale. Guard the ordering.
- Normal / winding convention (#1000): front-face normal is `-Z`, indices
  `[0, 3, 2, 2, 1, 0]`. The billboard system rotates via
  `Quat::from_rotation_arc(-Z, look_dir)`, so object `-Z` ends up facing
  the camera — the textured face must be `-Z`. Pre-#1000 normals were `+Z`
  and `two_sided: true` masked it. Guards:
  `placeholder_normals_point_negative_z_for_billboard_arc`,
  `placeholder_index_winding_produces_negative_z_geometric_normal`.
- `bs_bound` Z-up → Y-up swap (#995): center via
  `byroredux_core::math::coord::zup_to_yup_pos`, half-extents reshuffled
  `(hx, hz, hy)`. Guard: `placeholder_uses_obnd_bounds_when_present`.
- Two-sided + alpha-test cutout: `two_sided: true`, `alpha_test: true`,
  `alpha_threshold: 0.5`, `alpha_test_func: 6` (GREATEREQUAL),
  `has_alpha: false` (cutout and blend are exclusive).
- `BsRotateAboutUp` handling in `billboard.rs::compute_billboard_rotation`
  currently falls back to the world-up yaw lock (it lacks the local frame).
  Verify that approximation is still acceptable for tree imposters and
  documented — drifting it silently into pitch would tilt every tree.

### Dimension 3: TREE → Billboard Wiring
**Entry points**: `byroredux/src/cell_loader/references/mod.rs`
(`is_spt` dispatch, `parse_and_import_spt`),
`byroredux/src/cell_loader/spawn.rs` (`placement_root_billboard` →
`Billboard::new`), `crates/plugin/src/esm/records/tree.rs` (`parse_tree`,
`TreeRecord`, `has_speedtree_binary`).
**Checklist**:
- The `.spt` route fires when the REFR's TREE base's MODL ends in `.spt`;
  TREE record is fetched from `record_index.trees` by the same form id
  resolved against `index.statics`. Mixed `.nif` + `.spt` REFRs in one
  cell must coexist.
- `parse_and_import_spt` returns the **same** `CachedNifImport` shape as
  every other model, with synthetic defaults the generic spawn path must
  not mis-read as NIF-rooted:
  `placement_root_billboard = Some(BsRotateAboutUp)` (#994),
  `bsx_flags = 0` (#1214), `root_flags = 0` (#1235),
  `flame_attach_offset = None`. Confirm the spawn site never assumes the
  placeholder carries a real NiAVObject root / BSXFlags / flame marker.
- `spawn.rs` actually inserts the `Billboard` component when
  `placement_root_billboard.is_some()` — without it the quad spawns static
  (this was SPT-D4-01, now closed; it's the regression guard for the whole
  dimension).
- `TreeRecord` field capture is lossless for the fields the importer reads
  (OBND→`bounds`, ICON→`leaf_texture`, MODB→`bound_radius`,
  BNAM→`billboard_size`). SNAM/CNAM are parsed-but-not-consumed (TD5-011) —
  don't flag as a drop, but DO flag if they're silently *mis-parsed*
  (e.g. CNAM length not shape-tolerant across the 5-float Oblivion vs
  8-float FO3/FNV split).
- `.spt` files in BSAs resolve through the same `extract_mesh` lookup chain
  as `.nif` (sibling-BSA auto-load, AE pipeline-path strip if relevant) —
  no parallel "spt resolver".
- Cell unload despawns the placeholder entities cleanly; no leaked BLAS
  entries for the billboard quad.

### Dimension 4: Per-Game Variants & Route Divergence
**Entry points**: `crates/spt/src/version.rs` (`detect_variant`,
`SpeedTreeVariant`, `MAGIC_HEAD`), `byroredux/src/scene/nif_loader.rs`
(`parse_import_and_merge` `is_spt` branch).
**Checklist**:
- `detect_variant` recognises any `__IdvSpt_02_`-prefixed file but cannot
  tell V4Oblivion from V5Fnv at the magic level — it defaults to `V5Fnv`
  and the caller is meant to override via game context. Verify nothing
  downstream actually *depends* on the variant being correct today (the
  placeholder path is variant-agnostic); if a consumer branches on it,
  that's a real bug. Guards: `detect_variant_recognises_idvspt_magic`,
  `detect_variant_unknown_for_non_speedtree_inputs`.
- `MAGIC_HEAD` is the exact 20 bytes (`u32 1000`, `u32 12`,
  `"__IdvSpt_02_"`). A one-byte flip must reject → placeholder. Confirm no
  partial-prefix leakage (input shorter than 20 bytes rejects).
- **Route divergence** (the real per-game risk here): the cell-loader
  route threads TREE metadata; the `--tree` / loose route uses
  `SptImportParams::default()`. That means a loose-loaded Oblivion `.spt`
  gets the 256×512 default (no MODB sizing) and no ICON override. Confirm
  this is understood/documented and isn't masking a sizing bug that would
  also bite the cell route. Flag if the two routes have drifted in the
  *parse* call (they must both call `parse_spt` + `import_spt_scene`).
- Oblivion (SpeedTree 4.x) vs FO3/FNV (5.x): the parameter walker is
  assumed unified across all three (same magic, same tag dictionary). The
  geometry-tail layout is **not** confirmed unified — but the tail is
  out-of-scope for Phase 1, so only flag a tail-decode assumption if
  `--focus` includes the future phases.

### Dimension 5: Tag Dictionary
Lower risk (fixed-size decodes), but a wrong size here is the Dimension-1
desync trigger, so spot-check rather than skip.
**Entry points**: `crates/spt/src/tag.rs` (`SptTagKind`, `dispatch_tag`),
`crates/spt/docs/format-notes.md`, recon examples (`spt_tagmap`,
`spt_transitions`).
**Checklist**:
- `dispatch_tag` currently maps ~90 tag values across the payload kinds.
  This is conservative-by-design: any tag not in the table → `Unknown` →
  walker stops cleanly. The old "~14 tags / 40-tag aspirational target"
  framing is stale — do **not** report dictionary size as a gap.
- Cross-check a sample of fixed-size assignments against the
  `format-notes.md` 2026-05-09 table and the `tag.rs` unit tests
  (`fixed_byte_payload_tags`, `string_payload_tags`, etc.): e.g. 8003/8005/8009
  = 52 B, 13008 = 11 B, 13013 = 7 B, 12002 = 16 B, 12003 = 20 B,
  ArrayBytes 10002 stride 1 / 10003 stride 8. A size that contradicts the
  observed histogram is a Dimension-1 desync waiting to happen → MEDIUM.
- Confounder tags (`4096`, `5376`, string-length values that fell in the
  tag band) must stay `Unknown` so the walker bails rather than misparses
  (`unknown_for_out_of_dictionary_tags`).
- Any tag observed in the corpus at ≥1 % frequency that's still `Unknown`
  should have a `format-notes.md` rationale; an undocumented common-tag
  bail is a LOW finding.

### Dimension 6: NIFAL Material Translation for Placeholders
The placeholder `ImportedMesh` flows through the single NIFAL boundary like
any other mesh. Cross-cuts `/audit-nifal` — report single-boundary /
no-fabrication findings *there*, not here.
**Entry points**: the material defaults in
`crates/spt/src/import/mod.rs` (`placeholder_billboard_mesh`),
`byroredux/src/material_translate.rs` (`translate_material`, consumed at
the `spawn.rs` call site), `crates/core/src/ecs/components/material.rs`
(`Material::resolve_pbr`).
**Checklist**:
- The placeholder is canonicalised at the **single** `translate_material`
  boundary — no parallel "spt material" path that bypasses it (the `--tree`
  loose route and the cell route must both land here via `spawn.rs`).
- Non-PBR defaults survive translation: `is_pbr: false`, `from_bgsm: false`
  (#1076/#1077); `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)`
  — explicit foliage defaults set at import (#1819/SPT-NEW-05,
  `placeholder_billboard_mesh` in `crates/spt/src/import/mod.rs`), NOT `None`.
  A regression to `None` re-opens the keyword-classifier substring collision
  (Boxwood→wood, Elderberry→glass). Guard:
  `placeholder_billboard_sets_foliage_pbr_overrides_regardless_of_texture_path`.
  `resolve_pbr` must fill the canonical `metalness`/`roughness` f32 from the
  non-PBR keyword path, never promote the billboard to metallic-roughness.
  SpeedTree never resolves a BGSM/BGEM (#1241/#1353) — guard that import-side PBR plumbing
  (a82366e9-style) left the billboard non-PBR.
- `emissive_source: EmissiveSource::None` (#1280) holds — a tree billboard
  must not pick up an emissive lobe.
- The two-sided alpha-test cutout maps to the correct canonical `Material`
  flags after translation (foliage silhouette preserved, not
  opaque-blitted).

## Phase 3: Output

Write findings to **`docs/audits/AUDIT_SPEEDTREE_<TODAY>.md`** using the
base finding format from `_audit-common.md`. Mark anything already covered
by #994–#1002 as a regression guard, not a new finding. Suggest
`/audit-publish` on completion.
