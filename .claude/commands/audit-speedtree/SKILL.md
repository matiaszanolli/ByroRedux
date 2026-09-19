---
description: "Audit the SpeedTree (.spt) TLV walker (crates/spt), the placeholder-billboard import, and the TREE→billboard/wind wiring for Oblivion / FO3 / FNV"
argument-hint: "--focus <dimensions> --depth shallow|deep"
---

# SpeedTree Subsystem Audit

Audit `crates/spt` (~2k LOC in `src/`; `.spt` is pre-Skyrim only — Skyrim+ trees are NIFs rooted
at `BSTreeNode`, `/audit-nif`). It does two things: (1) walks the `.spt` parameter stream as
tag-length-value data, and (2) emits a **placeholder billboard** `ImportedScene` so TREE cells
render *something* instead of failing or going treeless.

**Settled facts** (verify against code, then treat as premises):
- `.spt` is a procedural tree *definition* (parameters, BezierSpline curves, texture names) —
  there is **no geometry tail to decode** (#3808, 2026-09-07; the largest of 159 corpus files is
  8,793 B). Past `parser::TAG_MAX` (13,999) the stream continues as more parameter TLV in the
  14000–22000 tag bands; the earlier `0x4E25`/`0x4E21` "geometry-tail markers" were arithmetic
  slips (0 of 159 files). Layout notes: `crates/spt/docs/format-notes.md`.
- **Known-open, dated 2026-09-19 (#4122)**: the walker desyncs 1–3 bytes before `tail_offset` on
  46% of the 159-file corpus (86 need no shift, 36 one, 33 two, 4 three) — invisible today
  because nothing consumes bytes past `tail_offset`; fixing it is the precondition for raising
  `TAG_MAX`. Report only new evidence, and any consumer that starts reading past `tail_offset`.
- SNAM/CNAM are parsed but deliberately not consumed: `SpeedTreeWind` uses the neutral `(1, 0)`
  pair because TREE.CNAM's layout is unpinned (#3190).

**Not here** (`/audit-exterior`): distant tree/object LOD, generating branch/leaf geometry or
sourcing real tree meshes — an open design question in `docs/engine/exal-trees.md` §3/§10.

**Architecture**: Single-pass — small enough to run all dimensions inline.

Read `.claude/commands/_audit-common.md` and `.claude/commands/_audit-severity.md` for shared protocol.

## Scope

**Crate**: `crates/spt/src/` — `parser.rs`, `tag.rs`, `version.rs`, `stream.rs`, `scene.rs`,
`import/mod.rs`, feature-gated `recon/mod.rs`; public surface in `lib.rs` (`parse_spt`,
`import_spt_scene`, `SptImportParams`, `SptScene`, `detect_variant`, `dispatch_tag`).

**Wiring cross-cuts**:
- `byroredux/src/cell_loader/references/synth_child.rs` (`.spt` dispatch) →
  `references/import.rs` (`parse_and_import_spt`, `resolve_spt_model_path`,
  `resolve_tree_icon_path`); `cell_loader/nif_import_registry.rs` (`spt_cache_key`,
  `CachedNifImport.speedtree_wind`); `cell_loader/spawn/mesh_instance.rs` (attaches `Billboard`
  from `mesh.billboard_mode` and `SpeedTreeWind`, routes through `translate_material`);
  `byroredux/src/streaming.rs` (skips `.spt` in prefetch).
- `byroredux/src/scene/nif_loader.rs` — the `--tree` loose route: a **parallel** path calling
  `import_spt_scene` with `SptImportParams::default()` (no TREE metadata).
- `crates/plugin/src/esm/records/tree.rs` (`parse_tree` → `TreeRecord`: OBND/ICON/MODB/SNAM/CNAM/
  BNAM/PFIG; `has_speedtree_binary`); `byroredux/src/systems/billboard.rs` (rotation + shared-wind
  bend); `crates/core/src/ecs/components/billboard.rs` (`SpeedTreeWind`).

**Acceptance**: ≥ 95% unknown-tag-clean per game in `crates/spt/tests/parse_real_spt.rs`
(`parse_rate_{fnv,fo3,oblivion}_spt`; `#[ignore]`, env `BYROREDUX_{FNV,FO3,OBL}_DATA`; 133 vanilla
files, Oblivion 113); un-decoded trees render a billboard, never an `Err` out of the cell loader.

## Parameters (from $ARGUMENTS)

- `--focus <dimensions>`: comma-separated numbers. Default: all 6.
- `--depth shallow|deep`: `shallow` = walker contract + wiring from source; `deep` = also run the
  corpus harness. Default `deep`.

## Extra Per-Finding Fields

- **Dimension**: Walker Byte-Accounting | Placeholder Fallback | TREE→Billboard Wiring | Per-Game Variants | Tag Dictionary | NIFAL Material Translation

## Phase 1: Setup

1. `mkdir -p /tmp/audit/speedtree`.
2. `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json number,title,state,labels --search "speedtree OR spt OR TREE" > /tmp/audit/speedtree/issues.json`.
3. Read the most recent `docs/audits/AUDIT_SPEEDTREE_*.md` (sort by date). Findings it lists as
   closed are regression guards — `gh issue view` before treating any list as current.
4. `cargo test -p byroredux-spt` (default lane). CI also compiles the feature-gated recon
   examples and unit tests (`cargo check -p byroredux-spt --features recon --examples`,
   `cargo test -p byroredux-spt --features recon --lib`).
5. `deep` only — corpus: `BYROREDUX_FNV_DATA=… cargo test -p byroredux-spt --release --test parse_real_spt -- --ignored --nocapture`
   (and `_FO3_DATA` / `_OBL_DATA`); per-file dumps via the recon examples
   (`cargo run -p byroredux-spt --features recon --example spt_walk`; also `spt_tail`, `spt_tagmap`, …).

## Phase 2: Dimensions

Ordered by risk.

### Dimension 1: Walker Byte-Accounting
Paths: `crates/spt/src/{parser,stream}.rs`
First step: `cargo test -p byroredux-spt` then diff `read_payload` sizes against `tag.rs`
**Highest risk**: most tags have no length framing — the walker advances by the payload kind's
fixed size, so one wrong size desyncs everything after it.
**Guards**: `parser.rs` unit tests (`tag_13005_*` family incl. `tag_13005_at_eof_does_not_panic`,
`empty_candidate_is_not_a_plausible_curve_string`), `tests/parse_synthetic_spt.rs`
(`generator_output_matches_pinned_bytes`, `parser_decodes_every_dispatch_arm_against_pinned_fixture`),
`tests/parse_real_spt.rs` (opt-in).
**Checklist**:
- Each `SptTagKind` advances exactly its size: `U8`=1, `U32`=4, `Vec3`=12, `FixedBytes(n)`=n,
  `String`=4+len, `ArrayBytes{stride}`=4+count×stride, `Bare`=0. Cross-check `read_payload`
  against `tag.rs`.
- `MaybeStringElseBare` (tag 13005): peek decides Bare vs String; re-syncs on both arms, an EOF
  peek can't panic, and a **zero-length candidate is Bare** (#3531: `Iterator::all` is vacuously
  true on an empty slice; `Bare` discards nothing, a mis-read leading `0` shifts everything).
- Stops cleanly at `is_eof()` (`reached_eof`) and at an out-of-range/`Unknown` tag (records
  `tail_offset` + `unknown_tags`); never reads past EOF.
- `read_string_lp` and `ArrayBytes` cap at 64 KiB on the **byte count** (count × stride), not
  just `count`, and return `Err` rather than OOM.
- `parse_spt` has **five** fatal `Err` conditions (its docstring lists them: magic mismatch,
  mid-payload underflow, string cap, array cap, unrecognized context-sensitive kind); all discard
  the whole `SptScene`. In-range-but-unknown tags are non-fatal — the contract the placeholder
  relies on. A new fatal path is HIGH (it kills the cell-loader fallback).
- Little-endian, unconditional (every `.spt` is `__IdvSpt_02_`); flag any host-endian read. The #4122 desync above.
**Output**: `/tmp/audit/speedtree/dim_1.md`

### Dimension 2: Placeholder Fallback Correctness
Paths: `crates/spt/src/import/mod.rs`, `byroredux/src/systems/billboard.rs`
First step: `cargo test -p byroredux-spt placeholder`
**Guards**: `import/mod.rs` tests: `placeholder_normals_point_negative_z_for_billboard_arc`,
`placeholder_index_winding_produces_negative_z_geometric_normal`, `placeholder_uses_obnd_bounds_when_present`,
`non_finite_bnam_falls_through_instead_of_producing_a_nan_quad`, `leaf_texture_override_wins_over_spt_tag`,
`empty_texture_leaves_path_unset_for_renderer_placeholder`.
**Checklist**:
- `import_spt_scene` **always** returns a one-node / one-mesh `ImportedScene`; the cell loader only
  gets `None` when `parse_spt` returns `Err`, and that path must log + skip the REFR without
  aborting the cell.
- Leaf-texture precedence: TREE.ICON override → `.spt` tag 4003 (first wins) → unset (renderer
  placeholder).
- Size precedence in `compute_billboard_size`: **OBND → BNAM → MODB → 256×512 default**, each
  through `clamp_billboard_extent` (returns `Option`, so a non-finite field falls through to the
  next tier; a bare `f32::clamp` is NaN-transparent and lets a NaN BNAM reach the quad, bounds and
  BLAS build, #3529). OBND beats BNAM on purpose (BNAM clamps tall trees). Measured: vanilla
  Oblivion has BNAM on 142/142 TREE records and no OBND, so the MODB tier is reached by 0 vanilla
  records; whether Oblivion should size from BNAM or MODB is an open format question (#3740).
- Winding: front-face normal is `-Z`, indices `[0, 3, 2, 2, 1, 0]`, because the billboard system
  rotates via `Quat::from_rotation_arc(-Z, look_dir)`; `bs_bound` Z-up→Y-up swap uses
  `zup_to_yup_pos` with half-extents `(hx, hz, hy)`.
- Billboard mode `BsRotateAboutUp` in `compute_billboard_rotation` is a world-up yaw lock (no
  local frame available) — confirm it never drifts into pitch, which would tilt every tree.
**Output**: `/tmp/audit/speedtree/dim_2.md`

### Dimension 3: TREE → Billboard Wiring (highest report yield)
Paths: `byroredux/src/cell_loader/references/{synth_child,import}.rs`, `byroredux/src/cell_loader/{nif_import_registry,spawn/mesh_instance}.rs`, `byroredux/src/systems/billboard.rs`, `crates/plugin/src/esm/records/tree.rs`
First step: `cargo test -p byroredux parse_and_import_spt` and, with data, `cargo test -p byroredux vanilla_tree -- --ignored`
**Guards**: `references/import_tests.rs` — `vanilla_tree_models_all_resolve` and
`vanilla_tree_icons_all_resolve` (env-gated corpus gates), the pure-closure resolver tests
(`tree_model_resolves_leading_separator_bare_names_under_the_measured_directory`,
`tree_icon_resolves_bare_filenames_under_the_measured_directory`),
`parse_and_import_spt_surfaces_billboard_mode_on_mesh`, `malformed_spt_still_produces_placeholder`;
`systems/water.rs` pins the billboard system's `SpeedTreeWind` query shape by source text.
**Checklist**:
- The `.spt` route fires when the TREE base's MODL ends in `.spt`; the TREE record comes from
  `record_index.trees`; mixed `.nif` + `.spt` REFRs coexist.
- **Model path**: every vanilla `.spt` MODL is a leading-separator bare filename (154/154) and the
  binaries live under a top-level `trees\` folder, outside `meshes\`. `resolve_spt_model_path` probes
  verbatim → `meshes\`-rooted → bare name under `SPT_CANDIDATE_DIRS` via exact-key lookup
  (`has_mesh_exact` / `extract_mesh_exact`), for `.spt` only; the streaming prefetch skips `.spt`
  (a miss there would write a negative registry entry that masks the sync-side resolve). Pre-fix
  0/154 records resolved. Hoisting `trees\` prefixing into the shared normaliser is the regression.
- **Icon path**: every vanilla `TREE.ICON` is a bare filename; `resolve_tree_icon_path` probes
  verbatim, then `textures\trees\leaves\` (93/93 measured), then `…\billboards\`; a miss warns
  naming the ICON. Same scoping rule — never in the shared `normalize_texture_path`.
- **Cache key** is per-(model path, TREE record) via `spt_cache_key` (records sharing one `.spt`
  carry different ICON/OBND/BNAM); NIF keying unchanged.
- `parse_and_import_spt` returns the same `CachedNifImport` shape as any model with synthetic
  defaults the spawn path must not mis-read as NIF-rooted (`bsx_flags = 0`, `root_flags = 0`,
  `flame_attach_offset = None`).
- **Billboard attach is at mesh level**: `import_spt_scene` sets
  `mesh.billboard_mode = Some(BILLBOARD_MODE_BS_ROTATE_ABOUT_UP)` and the functional insert is
  `spawn/mesh_instance.rs`'s `if let Some(raw) = mesh.billboard_mode` (without it the quad spawns
  static). `placement_root_billboard` is a dead seam for SpeedTree (no producer sets it;
  `spawn.rs` consumer is unreachable) — target findings at the mesh-level insert.
- **Wind**: placeholders carry `SpeedTreeWind::new(1.0, 0.0)` (neutral; cached as
  `CachedNifImport.speedtree_wind`, attached in `mesh_instance.rs` and both loose-route sites in
  `nif_loader.rs`). `apply_speedtree_wind` in `billboard.rs` bends the canopy from the shared
  `WindField`: gust is clamped to a finite non-negative value before use (#3194), response/stiffness
  clamped, keyed off the `SpeedTreeWind` marker rather than one billboard enum. The billboard
  system's scheduler access declares `reads::<SpeedTreeWind>()` (`boot/schedule/late.rs`).
  `SpeedTreeWind` is rebuilt on import, not saved (`save_io/registry_completeness_tests.rs`).
  Do not project TREE.CNAM into it (unpinned, #3190).
- `TreeRecord` capture is lossless for what the importer reads (OBND→`bounds`, ICON→`leaf_texture`,
  MODB→`bound_radius`, BNAM→`billboard_size`); CNAM is 8 × f32 on all three games (#3751) — flag
  mis-parses, not the unconsumed SNAM/CNAM.
- Cell unload despawns the placeholders; no leaked BLAS entries for the quad.
**Output**: `/tmp/audit/speedtree/dim_3.md`

### Dimension 4: Per-Game Variants & Route Divergence
Paths: `crates/spt/src/version.rs`, `byroredux/src/scene/nif_loader.rs`
First step: `grep -rn 'detect_variant' byroredux/src crates/spt/src`
**Checklist**:
- `detect_variant` recognises any `__IdvSpt_02_`-prefixed file but cannot separate Oblivion (4.x)
  from FO3/FNV (5.x) at the magic level — it defaults to `V5Fnv`. It is a logged sanity check
  today; a consumer that *branches* on it is a real bug (the placeholder path is
  variant-agnostic). Guards: `detect_variant_recognises_idvspt_magic`,
  `detect_variant_unknown_for_non_speedtree_inputs`.
- `MAGIC_HEAD` is the exact 20 bytes (`u32 1000`, `u32 12`, `"__IdvSpt_02_"`); a flip or short input rejects.
- **Route divergence**: the cell route threads TREE metadata; the `--tree` route uses defaults
  (256×512, no ICON override). Both must call `parse_spt` + `import_spt_scene`; flag drift in
  that call, and any sizing bug the default route would mask for the cell route.
- Oblivion vs FO3/FNV share one walker (same magic, same dictionary); the undictionaried
  14000–22000 bands recur across ~151/159 files — a claim they are identical across games is unverified.
**Output**: `/tmp/audit/speedtree/dim_4.md`

### Dimension 5: Tag Dictionary
Paths: `crates/spt/src/tag.rs`, `crates/spt/docs/format-notes.md`
First step: `cargo test -p byroredux-spt tag::`
Lower risk, but a wrong size is the Dim 1 desync trigger — spot-check.
**Guards**: `tag.rs` tests (`fixed_byte_payload_tags`, `string_payload_tags`,
`unknown_for_out_of_dictionary_tags`, …).
- `dispatch_tag` maps ~120 tags conservatively: anything absent → `Unknown` → the walker stops.
  Dictionary size is not a gap. Sample fixed sizes against the `format-notes.md` tables (8003/8005/
  8009 = 52 B, 13008 = 11 B, 13013 = 7 B, ArrayBytes 10002 stride 1 / 10003 stride 8); a size
  contradicting the observed histogram is MEDIUM. 12002 (16 B) / 12003 (20 B) are size-only with
  no recorded corpus evidence — flag only if a real sample contradicts them.
- Confounder tags (`4096`, `5376` — string-length values inside the tag band) must stay `Unknown`.
- A tag at ≥ 1% corpus frequency still `Unknown` needs a `format-notes.md` rationale (LOW).
**Output**: `/tmp/audit/speedtree/dim_5.md`

### Dimension 6: NIFAL Material Translation for Placeholders
Paths: `crates/spt/src/import/mod.rs` (`placeholder_billboard_mesh`), `byroredux/src/material_translate.rs`
First step: `cargo test -p byroredux-spt placeholder_billboard_sets_foliage_pbr_overrides`
The placeholder flows through the single NIFAL boundary; single-boundary / no-fabrication
findings belong to `/audit-nifal`, not here.
- Both routes (`scene/nif_loader.rs`, `spawn/mesh_instance.rs`) reach `translate_material`; no
  parallel "spt material" path.
- Import-side non-PBR defaults hold: `is_pbr: false`, `from_bgsm: false`, explicit foliage
  overrides `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)` (a `None`
  re-opens the keyword-classifier substring collision — Boxwood→wood, Elderberry→glass);
  `emissive_source: EmissiveSource::None`; two-sided alpha-test cutout (`alpha_test`, threshold
  0.5, func 6 GREATEREQUAL, `has_alpha: false`) survives translation with the silhouette
  preserved. SpeedTree never resolves a BGSM/BGEM.
**Output**: `/tmp/audit/speedtree/dim_6.md`

## Phase 3: Output

Write findings to `docs/audits/AUDIT_SPEEDTREE_<TODAY>.md` in the base finding format. Suggest
`/audit-publish` (labels speedtree + terrain-exterior; add `game:*` when specific to one
title's `.spt` corpus).
