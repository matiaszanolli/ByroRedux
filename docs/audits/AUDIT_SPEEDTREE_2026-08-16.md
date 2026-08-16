# SpeedTree Subsystem Audit — 2026-08-16

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker (`parser.rs`, `stream.rs`, `tag.rs`, `version.rs`, `scene.rs`) + the
placeholder-billboard importer (`crates/spt/src/import/mod.rs`) and the
feature-gated `crates/spt/src/recon/mod.rs`, plus its cross-cut wiring in
`byroredux/src/cell_loader/references/import.rs`,
`byroredux/src/cell_loader/references/synth_child.rs`,
`byroredux/src/cell_loader/spawn.rs`,
`byroredux/src/cell_loader/spawn/mesh_instance.rs`,
`byroredux/src/scene/nif_loader.rs`,
`crates/plugin/src/esm/records/tree.rs`,
`byroredux/src/systems/billboard.rs`,
`byroredux/src/material_translate.rs`, and
`crates/core/src/ecs/systems.rs`.

Single-pass, all six dimensions run inline per the skill's own architecture
note. **No sub-agents were dispatched.**

**Depth**: `deep` — the corpus acceptance gate was re-run live against the
on-disk FNV / FO3 / Oblivion BSAs, the full `byroredux-spt` unit +
integration suite was run, and the `recon` harness (`spt_dissect`,
`spt_tagmap`, plus a throwaway corpus sweep, since removed) was used to
byte-level-verify the leaf-texture claim in Dimension 2 rather than assert it.

**Method**: diffed direction against `AUDIT_SPEEDTREE_2026-08-07.md`, then
re-derived the two highest-risk contracts from current source rather than
inheriting the prior report's clean verdict — specifically the *consumer* side
of `placement_root_billboard`, which nine consecutive prior cycles recorded as
"clean" on the strength of the component being **inserted**, without tracing
whether the insert reaches the entity that actually draws.

---

## Dedup pass (mandatory)

Cached open issues: `/tmp/audit/issues.json` (269 OPEN). Live queries run
against `gh issue list --state all` for the keyword sets *speedtree / spt /
TREE*, *billboard / placement_root / propagation*, *leaf texture / ICON*, and
*2409*. `docs/audits/AUDIT_SPEEDTREE_*.md` (10 prior reports) scanned.

| Issue | State | Relevance |
|---|---|---|
| #1822 (SPT-NEW-07) | **OPEN** | Tag-13005 tail-swallow misparse. Unchanged, already filed, **not re-reported**. SPT-D1-2026-08-16-01 below is adjacent but distinct (and corrects #1822's stated impact). |
| #994, #995, #996, #997, #998, #999, #1000, #1001, #1002 | CLOSED | Original SPT-D* batch — treated as regression guards (all verified in place, see the guard table). |
| #1707, #1711, #1715, #1819, #1820, #1821 | CLOSED | SPT-NEW-* batch — guards verified. |
| #2206, #2527 | CLOSED | The per-mesh / hierarchical billboard-attach fixes for the **NIF** paths. The `.spt` path was never brought into their scope — this is the root of SPT-D3-2026-08-16-01. |
| #1214, #1235, #1594, #2409 | CLOSED | Synthetic-default and file-split context. |
| #2426 | CLOSED | `thiserror` removal from `crates/spt/Cargo.toml`; verified inert. |

No open issue covers any of the five findings below.

---

## Verification runs (this audit)

### Corpus acceptance gate — live run

```
BYROREDUX_FNV_DATA=… BYROREDUX_FO3_DATA=… BYROREDUX_OBL_DATA=… \
  cargo test -p byroredux-spt --release --test parse_real_spt -- --ignored --nocapture

[FO3] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries | 100.00 % coverage
[FNV] 10 files  | 10 with entries | 0 hit unknown tag | 1800  entries | 100.00 % coverage
[OBL] 113 files | 113 with entries| 4 hit unknown tag | 20425 entries | 96.46 % coverage
    trees\treems14canvasfreesu.spt     | tag=768 (0x0300) at offset 6211
    trees\shrubms14boxwood.spt         | tag=768 (0x0300) at offset 4507
    trees\treems14willowoakyoungsu.spt | tag=768 (0x0300) at offset 5946
    trees\treecottonwoodsu.spt         | tag=768 (0x0300) at offset 5641
```

Byte-identical to every audit run since 2026-07-03. All three gates clear the
≥ 95 % floor.

### Unit + integration suite

`cargo test -q -p byroredux-spt --release` — 46 unit + 3 synthetic integration
tests pass, 0 failures (3 corpus tests correctly `#[ignore]`d in the default run).

### Change window (`git log --since=2026-08-06`)

Three commits touch `crates/spt/`, all mechanical and verified inert:
`ad8335ba` (drop unused `thiserror` from `Cargo.toml`), `0c3d0f27` (+2 lines:
`ImportedScene::lights: Vec::new()` on the placeholder), `b09cec5c` (+1 line:
`bs_geometry_lod_slot: None` on the placeholder mesh). `tree.rs` and
`billboard.rs` are byte-identical. `references/mod.rs` was split under #2409 —
`spawn_synth_child`, which carries the `is_spt` dispatch, moved verbatim to
`references/synth_child.rs`.

---

## Findings

### SPT-D3-2026-08-16-01: `.spt` placeholder billboards never face the camera — `Billboard` lands on the non-renderable placement root

- **Severity**: HIGH
- **Dimension**: TREE→Billboard Wiring (secondary: Placeholder Fallback)
- **Location**: `crates/spt/src/import/mod.rs:322-329`,
  `byroredux/src/cell_loader/spawn.rs:775-777`,
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:537-548`,
  `byroredux/src/scene/nif_loader.rs:506-511`,
  `crates/core/src/ecs/systems.rs:93,120,179-200`
- **Status**: NEW (related to the closed #994 / #2206 / #2527)
- **Description**: The SpeedTree placeholder deliberately routes its billboard
  mode through the *parent* entity: `placeholder_billboard_mesh` sets
  `billboard_mode: None` on the mesh, and the mode rides on the sibling
  `ImportedNode` root → `CachedNifImport::placement_root_billboard` → a
  `Billboard` component on the REFR's **placement root** (`spawn.rs:775`,
  the #994 guard). The placement root carries no `MeshHandle`; the drawn
  entity is a separate child spawned by `spawn_mesh_instance` and linked via
  `Parent`/`Children` (`mesh_instance.rs:547-548`).

  `make_billboard_system` writes `GlobalTransform.rotation` directly on
  whatever entity holds `Billboard`. `make_transform_propagation_system` is
  driven by the **`Transform`** dirty set (`systems.rs:93`) with an
  all-clean fast path (`systems.rs:120`) and an incremental seed that only
  walks subtrees whose *local* `Transform` moved (`systems.rs:179-200`). A
  billboard-only `GlobalTransform` write marks no `Transform` dirty, so the
  rotation is never composed into the child mesh's `GlobalTransform`. On the
  structural-rebuild path the parent's `GlobalTransform` is instead *reset*
  from its local `Transform` (`systems.rs:164-171`) before the billboard
  system runs again — either way the child never sees the rotation, and the
  renderer reads the child (`byroredux/src/render/static_meshes.rs:97`
  queries `(GlobalTransform, MeshHandle)`).

  This is precisely the mechanism the engine already documents — and already
  fixed for both NIF paths. `scene/nif_loader.rs:961-972` (#2527) spells it
  out verbatim: *"the `NiBillboardNode`'s own container entity … is a separate
  ECS entity from the actual geometry linked via `Parent`/`Children` —
  `make_billboard_system` writes `GlobalTransform.rotation` directly on that
  container, which `make_transform_propagation_system` never re-walks into …
  so the rotation never reached the child mesh. Attach directly to the mesh
  entity instead of relying on parent→child propagation."* #2206 landed the
  same per-mesh attach on the cell-loader side (`mesh_instance.rs:542`). Both
  fires are gated on `mesh.billboard_mode.is_some()`, which the `.spt`
  placeholder explicitly sets to `None`.

  The loose `--tree` route has the identical defect from the other side:
  `nif_loader.rs:509` attaches `Billboard` to the *node* entity
  (`SptPlaceholderRoot`, whose `billboard_mode` is `Some(BsRotateAboutUp)`),
  and the mesh-entity attach at `nif_loader.rs:972` is skipped because
  `mesh.billboard_mode` is `None`.
- **Evidence**:
  - `crates/spt/src/import/mod.rs:328` — `billboard_mode: None,` with the
    comment *"Billboard mode for the .spt placeholder rides on the sibling
    `ImportedNode` root (`CachedNifImport::placement_root_billboard`, #994)"*.
  - `byroredux/src/cell_loader/spawn.rs:775-777` — `Billboard::new(mode)`
    inserted on `placement_root`, which receives `Transform`,
    `GlobalTransform`, `WorldBound::ZERO`, attach graph, `FormIdComponent`,
    `DoorTeleport`, `BSXFlags`, `SceneFlags` — and never a `MeshHandle`.
  - `byroredux/src/cell_loader/spawn/mesh_instance.rs:542-548` — per-mesh
    `Billboard` attach immediately followed by `Parent(placement_root)` +
    `add_child`; the attach is skipped for `.spt`.
  - `crates/core/src/ecs/systems.rs:120` — `if transform_dirty.is_empty() &&
    last_state == Some(state) { return; }`, the fast path a parked/panning
    camera in a static tree-heavy exterior hits every frame.
- **Impact**: Every SpeedTree placement in FNV / FO3 / Oblivion renders as a
  quad frozen at the REFR's authored rotation instead of yaw-tracking the
  camera. Because the quad is a zero-thickness card, walking around a tree
  makes it thin to an invisible edge and flip — the exact "static quads"
  symptom #994 was filed to eliminate, still present after the fix, because
  the fix targeted the wrong entity. Blast radius is every exterior cell in
  three games (Cyrodiil forest content leans entirely on TREE REFRs). Second
  order: the `Billboard` component on the placement root is not merely inert —
  `make_billboard_system`'s `gq.get_mut(entity)` arms the `GlobalTransform`
  dirty set for every tree placement on every camera move, which is the
  #1374 cost with none of the benefit.
- **Related**: #994 (the fix this supersedes), #2206 (per-mesh attach, cell
  loader), #2527 (per-mesh attach, loose loader + hierarchical walker), #1374
  (billboard dirty-set cost).
- **Suggested Fix**: Stamp the mode on the placeholder mesh, not (only) the
  node: set `billboard_mode: Some(BILLBOARD_MODE_BS_ROTATE_ABOUT_UP)` in
  `placeholder_billboard_mesh` so both existing per-mesh attaches
  (`mesh_instance.rs:542`, `nif_loader.rs:972`) fire, and keep
  `placement_root_billboard` only if a consumer of the root's rotation is
  identified (otherwise drop it to avoid the per-frame dirty-set churn).
  Regression guard: assert `imported.meshes[0].billboard_mode ==
  Some(BILLBOARD_MODE_BS_ROTATE_ABOUT_UP)` in `crates/spt/src/import/mod.rs`'s
  test module, plus a source-level pin that the `.spt` path reaches a
  `Billboard`-carrying entity that also carries `MeshHandle`.

---

### SPT-D2-2026-08-16-01: the `.spt` tag-4003 leaf-texture fallback is unresolvable on 100 % of vanilla content

- **Severity**: MEDIUM
- **Dimension**: Placeholder Fallback (secondary: Per-Game Variants & Route Divergence)
- **Location**: `crates/spt/src/import/mod.rs:129-137`,
  `crates/spt/src/scene.rs:128-133`, `byroredux/src/scene/nif_loader.rs:230-234`
- **Status**: NEW
- **Description**: `import_spt_scene` resolves the billboard texture as
  `params.leaf_texture_override` (TREE.ICON) `.or_else(|| scene.leaf_textures()
  .first())` — the `.spt`'s own tag-4003 value — and interns whatever it gets
  into the `StringPool` as `MaterialTextureSet::base_color`. Measured over the
  full vanilla FNV + Oblivion corpus (123 `.spt` files), **every single
  tag-4003 value is an absolute authoring path that cannot exist in any game
  archive**: `C:\Hope\IDV\Cottonwood\\TreeCottonwoodLeavesSU.tga`,
  `C:\Noah\Fallout\Trees\WastelandShrub01\\WastelandShrub01Foliage01.dds`,
  `C:\Projects\Fallout3\Game\Data\Trees\\TreeWOakLeaves01b.tga`, the UNC form
  `\\Vault\tes4\Users\MeganS\TreeMS14WillowOak\\TreeMS14WillowOakLeaves01SU.tga`,
  and in one Oblivion file the literal exporter error string
  `c:\program files\speed\speedtreecad v3.4\FileLoadError.tga`. 121 of 123 are
  `.tga`. After `strip_build_prefix` (the engine's build-pipeline path
  normaliser), **0 of 123** begin with `textures\`.

  The consequence is not "the fallback is merely unused" — it changes the
  documented behaviour. The module contract (`import/mod.rs:17-21`) is
  ICON → tag 4003 → *unset, so the renderer's missing-texture placeholder
  takes over*. Because tier 2 always produces a non-empty string, the third
  tier is unreachable: the importer never leaves `base_color` unset, and the
  engine instead performs a guaranteed-failing archive lookup and carries a
  bogus `Material::texture_path` (which also becomes an input to
  `classify_glass_into_material` and the mesh-name/texture keyword classifiers).
- **Evidence**: corpus sweep over `Fallout - Meshes.bsa` (FNV) and
  `Oblivion - Meshes.bsa` via `parse_spt` + `SptScene::leaf_textures()`; raw
  paths saved to `/tmp/audit/speedtree/obl_leafpaths.txt`. Cross-checked
  against `spt_dissect` printable-ASCII runs on `trees\whiteoak01.spt`
  (`C:\Hope\Fallout3\Trees\WhiteOak\\TreeWOakLeaves01b.tga` at offset 4768,
  inside the parameter section — `tail_offset = 5656`) and
  `trees\treecottonwoodsu.spt`. The same absolute-path shape is already
  visible for the sibling bark tag 2000 in `crates/spt/docs/format-notes.md`.
- **Impact**: On the loose `--tree` / `--mesh foo.spt` visualiser route
  (`nif_loader.rs:230-234`, which passes `SptImportParams::default()` and so
  has no ICON override) **every** load textures the placeholder with a dead
  path — the route is permanently magenta and emits a spurious `tex.missing`
  entry, which is misleading precisely in the tool built to diagnose missing
  textures (see the project's "chrome/posterized → run `tex.missing` first"
  rule). On the cell route the damage is bounded to TREE records with an
  empty ICON. Not a crash; the placeholder still renders.
- **Related**: #997 (established "first wins" among duplicate tag-4003
  entries — the corpus shows 2–4 entries per file, all equally unresolvable,
  so the precedence reasoning was moot); #1819 (the other consumer of this
  texture path).
- **Suggested Fix**: Reject non-relative tag-4003 values at the importer
  boundary — drop anything containing a drive letter or a leading `\\`, or
  anything that does not normalise to `textures\…` after `strip_build_prefix`
  — so the documented tier-3 "leave unset, renderer placeholder" path is
  actually reachable. Optionally attempt a basename rewrite into
  `textures\…` before giving up. Record the corpus measurement in
  `crates/spt/docs/format-notes.md` so the arm is not re-added as if live.

---

### SPT-D1-2026-08-16-01: a fatal `parse_spt` error discards a fully recoverable placeholder — the tree disappears instead of degrading

- **Severity**: MEDIUM
- **Dimension**: Walker Byte-Accounting
- **Location**: `byroredux/src/cell_loader/references/import.rs:301,329-332`,
  `crates/spt/src/parser.rs:113,155-168`, `crates/spt/src/stream.rs:96-110`
- **Status**: NEW (adjacent to, and correcting an impact claim in, the OPEN #1822)
- **Description**: `parse_and_import_spt` treats any `Err` from `parse_spt` as
  terminal: it logs a warning and `return None`, which drops the REFR
  entirely (`references/synth_child.rs` then records a cache miss and spawns
  nothing). But `import_spt_scene` needs *nothing* from the parse except the
  optional tag-4003 string — size comes from the TREE record's OBND/BNAM/MODB
  and the texture from TREE.ICON. A partially-decoded `SptScene`, or even
  `SptScene::default()`, produces a perfectly correct placeholder. The
  crate's Phase-1 acceptance contract is "un-decoded trees render as a
  billboard card, never an `Err` out of the cell loader"; today an `Err`
  anywhere past the magic silently deletes the tree.

  Three reachable `Err` sources sit *after* the magic check, i.e. on content
  the engine has already accepted as a SpeedTree binary:
  1. `read_string_lp` when the length exceeds the 64 KiB cap
     (`stream.rs:98-107`);
  2. `ArrayBytes` when `count × stride` exceeds 64 KiB (`parser.rs:160-168`);
  3. `UnexpectedEof` mid-payload on any truncated file.

  Source (1) is directly reachable through the OPEN #1822: when a bare
  tag 13005 sits immediately before the geometry tail, `read_string_lp`
  consumes the tail's first `u32` **as a byte length**. #1822's impact section
  asserts *"no crash and no `Err` out of the cell loader; the placeholder
  billboard still renders"* — that holds only when the swallowed value happens
  to be small. A tail word that is a float bit pattern (e.g. `0x44898000` =
  1 150 681 088) exceeds the cap and hard-fails the parse, so the tree
  vanishes rather than degrading.
- **Evidence**: `references/import.rs:329-332` —
  `Err(e) => { log::warn!("Failed to parse SPT '{}': {}", label, e); return None; }`.
  `parse_spt`'s doc comment at `parser.rs:41-46` already frames `Err` as
  reserved for "truly fatal conditions", but the fatal set is broader than the
  two conditions it names (magic mismatch, underflow) — the two 64 KiB
  sanity caps also produce `Err`.
- **Impact**: A corrupt, truncated, or (per #1822) merely tail-adjacent
  mod-authored `.spt` removes its tree from the world with only a `warn!` line,
  rather than falling back to the placeholder the whole Phase-1 design exists
  to guarantee. Vanilla content is unaffected (123/123 parse clean), so this is
  a defense-in-depth / mod-compatibility gap, not a live regression.
- **Related**: #1822 (SPT-NEW-07 — the misparse that makes source (1)
  reachable; its stated impact should be amended), #999.
- **Suggested Fix**: Make `parse_spt` return the partial scene rather than
  discarding it — e.g. have the walker catch a payload-level `Err`, record it
  alongside `unknown_tags`, set `tail_offset`, and return `Ok(partial)`,
  reserving `Err` for the magic-header mismatch alone. Alternatively, keep the
  parser signature and have `parse_and_import_spt` fall back to
  `import_spt_scene(&SptScene::default(), &params, pool)` on `Err` when the
  magic *did* match. Guard with a truncated-fixture test asserting a
  placeholder `CachedNifImport` is still produced.

---

### SPT-D3-2026-08-16-02: the `is_spt` dispatch moved out of `references/mod.rs` — skill and prior report both point at the wrong file

- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring (documentation)
- **Location**: `.claude/commands/audit-speedtree/SKILL.md` (Scope bullet 1 and
  Dimension 3 "Entry points"), `docs/audits/AUDIT_SPEEDTREE_2026-08-07.md`
  (change-window table); live code at
  `byroredux/src/cell_loader/references/synth_child.rs:418-429`
- **Status**: NEW
- **Description**: Both documents state that the production `.spt` route is an
  `is_spt` extension check in `byroredux/src/cell_loader/references/mod.rs`
  (the prior report even cites "lines ~1403–1431"). Under #2409 / TD1-006 that
  file crossed 2000 LOC and `spawn_synth_child` — which carries the entire
  dispatch, including the `record_index.trees.get(&child_form_id)` lookup —
  was moved verbatim into the sibling `references/synth_child.rs`. `mod.rs`
  now only re-`use`s `parse_and_import_spt` for the submodule. Because both
  file paths still exist, `.claude/commands/_audit-validate.sh` cannot catch
  this: the backticked path resolves, the claim about it does not.
- **Evidence**: `grep -rn "eq_ignore_ascii_case(\"spt\")" byroredux/src/`
  returns exactly two production hits —
  `byroredux/src/cell_loader/references/synth_child.rs:422` and
  `byroredux/src/scene/nif_loader.rs:203`. Zero hits in
  `byroredux/src/cell_loader/references/mod.rs`.
- **Impact**: An auditor following the skill's own entry-point list reads a
  file that no longer contains the dispatch, sees no `.spt` code, and records
  the dimension as unchanged/clean — which is close to what happened for the
  root cause of SPT-D3-2026-08-16-01, whose consumer chain starts one file
  further along than the skill's map goes.
- **Related**: #2409, #1114 (the structural fix for audit-skill path drift).
- **Suggested Fix**: Update the Scope bullet and Dimension 3 entry-point list
  to name `byroredux/src/cell_loader/references/synth_child.rs`
  (`spawn_synth_child`), and extend the Dimension 3 checklist past the
  `Billboard` *insert* to the entity that actually carries `MeshHandle`.

---

### SPT-D2-2026-08-16-02: `import/mod.rs` module docstring still documents the pre-#1001/#1002 two-tier size precedence

- **Severity**: LOW
- **Dimension**: Placeholder Fallback (documentation)
- **Location**: `crates/spt/src/import/mod.rs:22-23`
- **Status**: NEW
- **Description**: The crate-facing module docstring says *"**Size** comes
  from the TREE record's `OBND` bounds, falling back to a 256 × 512 game-unit
  default (Bethesda standard tree)"* — the two-tier chain as it shipped before
  #1001 (MODB) and #1002 (BNAM) added two intermediate tiers. The real
  precedence, correctly documented on `compute_billboard_size` itself
  (`import/mod.rs:194-214`) and covered by six unit tests, is
  **OBND → BNAM → MODB → 256 × 512**, every tier clamped to `[16, 8192]`.
  The stale two-tier text is the summary a reader hits first, and it is the
  exact claim #1001 was filed against (Oblivion ships MODB and no OBND, so
  "OBND or default" means Cyrodiil pines at half scale — a bug that is fixed
  in code but still described as current behaviour here).
- **Evidence**: `crates/spt/src/import/mod.rs:22-23` vs the four-tier
  `compute_billboard_size` doc at `:194-214` and its implementation at
  `:215-232`.
- **Impact**: Documentation only. Risk is a future contributor "restoring"
  the documented two-tier behaviour, re-opening #1001/#1002.
- **Related**: #1001, #1002, #996 (the last docstring correction in this file).
- **Suggested Fix**: Replace the bullet with the four-tier chain and the
  `[16, 8192]` clamp, cross-referencing `compute_billboard_size`.

---

## Dimension summary (every dimension enumerated)

| Dimension | Findings | Verdict / basis this cycle |
|---|---:|---|
| 1 — Walker Byte-Accounting | **1** (MEDIUM) | Payload sizes re-derived against the `tag.rs` dictionary: no mismatch. Both 64 KiB caps confirmed to bound the **byte** count (`count × stride` via `saturating_mul`). `MaybeStringElseBare` re-syncs on both arms and cannot panic on a `None` peek. LE-only, no host-endian reads. Live corpus gate byte-identical to nine prior runs. Finding is the *consequence* of an `Err`, not a byte-accounting error. |
| 2 — Placeholder Fallback | **2** (1 MEDIUM, 1 LOW) | `import_spt_scene` still has no `Err` path (1 node / 1 mesh always). Size precedence, `[16, 8192]` clamps, `-Z` normals + `[0,3,2,2,1,0]` winding, `bs_bound` Z-up→Y-up via `zup_to_yup_pos`, `two_sided`/`alpha_test`/threshold `0.5`/func `6`/`has_alpha: false` all verified in code and by passing guards. Findings are the dead leaf-texture tier and its stale docstring sibling. |
| 3 — TREE→Billboard Wiring | **2** (1 HIGH, 1 LOW) | `CachedNifImport` synthetic defaults (`bsx_flags = 0`, `root_flags = 0`, `flame_attach_offset: None`, `collision_authoring: Default::default()`) all correct; `TreeRecord` capture lossless and CNAM length-tolerant across the 5-float/8-float split; `.spt` shares the `extract_mesh` chain; mixed `.nif`/`.spt` REFRs coexist. The `Billboard` **insert** (#994) is intact — its **consumer chain** is not. |
| 4 — Per-Game Variants & Route Divergence | **0** | `MAGIC_HEAD` is the exact 20 bytes; a one-byte flip and a 19-byte prefix both reject (guards present). `detect_variant` has exactly two production callers, both log-only (`references/import.rs:310`, `nif_loader.rs:214`); nothing branches on the variant. Both routes call `parse_spt` + `import_spt_scene` and funnel through `translate_material`. The route-divergence *consequence* is filed under Dimension 2. |
| 5 — Tag Dictionary | **0** | Every fixed size spot-checked against `crates/spt/docs/format-notes.md`: 8003/8005/8009 = 52 B, 13008 = 11 B, 13013 = 7 B, 12002 = 16 B, 12003 = 20 B, `10002` stride 1, `10003` stride 8, `6017`/`13001` String — all consistent with the 2026-05-09 table plus its corrections section, and each class pinned by a unit test. Confounders (`100`, `110`, `4096`, `5376`, `11776`, `13568`, `0`, `1`, `50`, `19985`, `u32::MAX`) all resolve `Unknown`. Live histogram unchanged. |
| 6 — NIFAL Material Translation | **0** | Single boundary preserved: both routes reach `translate_material` (`mesh_instance.rs:575`, `nif_loader.rs:979`) — no parallel "spt material" path. `ImportedMaterial::default()` still `is_pbr: false`, `from_bgsm: false`, `bgsm_pbr_scalars_authored: false`, `emissive_source: EmissiveSource::None`, `material_kind: 0`. `metalness_override: Some(0.0)` / `roughness_override: Some(0.85)` intact (#1819 guard passing) and `translate_material:214-215`'s `unwrap_or(f32::NAN)` still bypasses the keyword classifier for them. `two_sided` reaches the ECS as the `TwoSided` marker (`mesh_instance.rs:667-669`); alpha-test cutout survives. |

**Totals**: 6 dimensions, 5 findings.

---

## Regression guards (verified in place, NOT re-reported)

| Guard | Issue | Verified this cycle |
|---|---|---|
| Cell placeholder gets `Billboard` on the placement root | #994 | `spawn.rs:775-777` present — **but see SPT-D3-2026-08-16-01: the insert is on the wrong entity** |
| `bs_bound` Z-up → Y-up via `zup_to_yup_pos`, half-extents `(hx, hz, hy)` | #995 | `import/mod.rs:170-180`; `placeholder_uses_obnd_bounds_when_present` passing |
| `SptImportParams.wind` doc names CNAM, not BNAM | #996 | `import/mod.rs:71-76` unchanged |
| "First wins" on duplicate tag 4003 | #997 | `import/mod.rs:129-132` unchanged (see SPT-D2-2026-08-16-01 for why the tier is moot) |
| SHA-pinned synthetic regression fixture | #998 | `crates/spt/tests/parse_synthetic_spt.rs`, 3 tests passing |
| Tag 13005 bimodal disambiguation | #999 | `parser.rs:84-120` unchanged; residual tail edge tracked as #1822 |
| `-Z` normals + `[0,3,2,2,1,0]` winding | #1000 | Both guards passing |
| MODB drives Oblivion size | #1001 | `modb_drives_placeholder_size_when_obnd_absent` passing |
| OBND beats BNAM | #1002 | `obnd_precedence_over_bnam` / `bnam_precedence_over_modb` passing |
| `bsx_flags = 0` synthetic default | #1214 | `references/import.rs:409` |
| `root_flags = 0` synthetic default | #1235 | `references/import.rs:412` |
| Crate docstring reflects shipped scope | #1707 | `crates/spt/src/lib.rs:36-44` unchanged |
| OBND-derived `bs_bound` survives the cell route | #1711 | `import/mod.rs:170-180` + `import_tests.rs` |
| `BsRotateAboutUp` doc matches world-Y lock | #1715 | `systems/billboard.rs` comment + code agree |
| Foliage PBR overrides beat the keyword classifier | #1819 | `placeholder_billboard_sets_foliage_pbr_overrides_regardless_of_texture_path` passing |
| `detect_variant` has production callers | #1820 | Two log-only callers |
| `format-notes.md` tail-tag worked example byte-aligns | #1821 | Doc untouched |
| Mesh-level `billboard_mode` doesn't double-fire | #2206 | `billboard_mode: None` unchanged — **and this is what suppresses the only working attach; see SPT-D3-2026-08-16-01** |
| `collision_authoring: Default::default()` fabricates no collider for TREE | (no issue) | `RenderLayer::Architecture` → `ArchitectureTriMesh` arm → `!mesh.material.alpha_test` gate still excludes the always-`alpha_test` billboard |
| `thiserror` removal is inert | #2426 | Crate builds and tests clean without it |

The **14000-band Oblivion tail** (4 files bail at tag `768`) remains the
documented `format-notes.md` Phase-1 limitation, above the 95 % gate,
placeholder-covered — not re-reported.

---

## Candidates raised and disproved (not reported)

1. **Glass mis-promotion of the leaf billboard.**
   `classify_glass_into_material` runs *after* `resolve_pbr` with
   `has_transparent_coverage = has_alpha || alpha_test` (true for every
   placeholder) and `metalness = 0.0` (< 0.3), so only a keyword match could
   force `MATERIAL_KIND_GLASS` and overwrite the #1819 foliage roughness.
   Measured against the real inputs: **0 of 123** vanilla tag-4003 paths
   contain any `is_glass_keyword_path` token (`glass`, `crystal`, `window`,
   `bottle`, `jar`, `vial`, word-bounded `ice`/`gem`), and the mesh name is the
   constant `"SptPlaceholderBillboard"`. `is_mirror_pane` likewise cannot fire
   (name has no `mirror`). Disproved.
2. **`from_rotation_arc` roll on the antipodal case.** For a `BsRotateAboutUp`
   billboard directly "behind" the camera the look direction is exactly `+Z`,
   the antipode of the `-Z` source axis. glam falls back to
   `any_orthonormal_vector(from)`, which for `(0,0,-1)` yields `(0,-1,0)` — a
   180° rotation about `-Y`, i.e. a pure yaw. No roll, no upside-down tree.
   Disproved.
3. **`reached_eof` set with 1–3 trailing bytes.** When `peek_u32_le` returns
   `None` on a short tail the walker breaks and sets `reached_eof = true` even
   though bytes remain. Cosmetic only — `tail_offset` is still correct and no
   consumer distinguishes the two. Not worth a finding.
4. **Post-#2409 dispatch relocation breaking the route.** `spawn_synth_child`
   handles the lone-default placement (`is_primary_synth`), not just SCOL/PKIN
   expansion, so the `.spt` route is intact after the move — only the
   documentation is wrong (filed as SPT-D3-2026-08-16-02).
5. **`crates/spt` dependency-removal fallout (#2426).** `thiserror` was
   genuinely unreferenced; the crate hand-rolls `io::Error`. Clean.

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 2 |
| LOW | 2 |
| **Total** | **5** |

The subsystem's *parser* half remains in the shape nine prior audits recorded:
byte-accounting sound, dictionary consistent with the corpus, acceptance gate
unmoved, unit suite green. Both substantive findings are on the **consumer**
side, and both share a root cause worth naming: prior cycles verified that the
SpeedTree importer *produces* the right data (a billboard mode; a leaf-texture
path) without following either value to the entity or archive lookup that
consumes it. The billboard mode reaches an entity that never draws; the leaf
path reaches an archive that can never contain it. Neither is visible from
inside `crates/spt`, and neither is caught by any existing test — which is the
coverage lesson for the next cycle more than any individual fix.

### Suggested next step

```
/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-08-16.md
```

#1822 (SPT-NEW-07) remains open and tracked; SPT-D1-2026-08-16-01 recommends
amending its impact statement when it is fixed.
