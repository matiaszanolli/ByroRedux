# SpeedTree Subsystem Audit — 2026-05-13

Audit of the `byroredux-spt` crate (Session 33 Phase 1) — TLV walker
correctness, tag coverage against the FNV/FO3/Oblivion `.spt` corpus,
the ≥95 % acceptance gate, and the placeholder-billboard fallback that
keeps cell loads alive when a tree fails to decode. Cross-cuts the
cell-loader extension dispatch, the `--tree` CLI direct-visualiser, and
the TREE record parser added alongside the SpeedTree path.

- **Scope**: `crates/spt/src/` + `byroredux/src/cell_loader/references.rs`
  + `byroredux/src/scene/nif_loader.rs` + `crates/plugin/src/esm/records/tree.rs`.
- **Phase 1 acceptance gate (live)**:
  FNV `100.00 %` (10/10), FO3 `100.00 %` (10/10), Oblivion `96.46 %` (109/113).
  Run via `BYROREDUX_{FNV,FO3,OBL}_DATA=… cargo test -p byroredux-spt --release
  --test parse_real_spt -- --ignored --nocapture`. All three games clear ≥ 95 %.
- **Issue search**: `gh issue list … "speedtree OR .spt OR TREE"` returned
  no open issues against the `crates/spt/` path. Prior `BSTreeNode`
  issues (#159 / #363) cover Skyrim+ NIF-baked tree paths, not `.spt`.

The Phase 1 contract is *graceful fallback over zero rendering*, and the
TLV walker / tag dictionary / corpus gate all hold up against that
contract on the disk-corpus run. The findings below are concentrated in
the placeholder-import → cell-loader spawn handoff: the SpeedTree
importer hoists `billboard_mode` and `bs_bound` onto its `ImportedScene`
return, but the cell-loader's `CachedNifImport` adapter drops both
fields on the floor, so the *cell-spawned* placeholder is a static quad
instead of a yaw-billboard.

---

## SPT-D4-01: Cell-loader placeholder loses `Billboard` — quads spawn static, never face the camera

- **Severity**: HIGH
- **Dimension**: Placeholder Fallback
- **Location**: [byroredux/src/cell_loader/references.rs:936-947](byroredux/src/cell_loader/references.rs#L936-L947), [byroredux/src/cell_loader/nif_import_registry.rs:34-59](byroredux/src/cell_loader/nif_import_registry.rs#L34-L59)
- **Status**: NEW
- **Description**: The SpeedTree importer correctly emits an
  `ImportedScene` whose root node carries
  `billboard_mode: Some(BILLBOARD_MODE_BS_ROTATE_ABOUT_UP)`
  ([crates/spt/src/import/mod.rs:169](crates/spt/src/import/mod.rs#L169)).
  The loose-NIF / `--tree` path consumes that field correctly:
  [byroredux/src/scene/nif_loader.rs:391-393](byroredux/src/scene/nif_loader.rs#L391-L393)
  inserts a `Billboard` ECS component. But the cell-loader path
  goes through `parse_and_import_spt → CachedNifImport`, and
  `CachedNifImport` only holds `meshes / collisions / lights /
  particle_emitters / embedded_clip` — `nodes` are never carried
  through. The cell-loader spawn (`spawn_placed_instances`) iterates
  only `cached.meshes` and never inserts a `Billboard` component.
- **Evidence**:
  ```rust
  // crates/spt/src/import/mod.rs:161-176 — placeholder root node
  ImportedNode {
      name: Some(Arc::from("SptPlaceholderRoot")),
      ...
      billboard_mode: billboard.then_some(BILLBOARD_MODE_BS_ROTATE_ABOUT_UP),
      ...
  }
  ```
  ```rust
  // byroredux/src/cell_loader/references.rs:936-947 — adapter drops nodes
  Some(Arc::new(CachedNifImport {
      meshes: imported.meshes,
      collisions: Vec::new(),
      lights: Vec::new(),
      particle_emitters: Vec::new(),
      embedded_clip: None,
  }))
  // imported.nodes (with billboard_mode) goes out of scope unused.
  ```
  ```rust
  // byroredux/src/scene/nif_loader.rs:391-393 — the path that DOES wire Billboard
  if let Some(raw) = node.billboard_mode {
      world.insert(entity, Billboard::new(BillboardMode::from_nif(raw)));
  }
  ```
  No call to `world.insert(_, Billboard::new(_))` exists anywhere in
  `byroredux/src/cell_loader/`. `grep -rn "Billboard"
  byroredux/src/cell_loader/` returns only comments.
- **Impact**: Every `.spt`-routed REFR in a loaded cell spawns as a
  static quad with its normal facing world +Z, *not* a yaw-billboard.
  Effects: (1) trees show as flat cards locked to the placement REFR's
  rotation — walk around them and one face vanishes (would be invisible
  if `two_sided` weren't true); (2) the entire premise of the
  "yaw-to-camera billboard" Phase 1 fallback is defeated; (3) the
  `--tree foo.spt` smoke test renders correctly because it bypasses
  `CachedNifImport`, masking the bug in any in-engine validation. This
  is the kind of regression CLAUDE.md warns about: success in the
  loose-file CLI doesn't imply success in the cell loader.
- **Related**: `feedback_speculative_vulkan_fixes.md` (failure invisible
  to `cargo test`); the loose-NIF cell loader's `parse_nif → import_nif_scene
  → spawn` also relies on `imported.nodes` to wire `Billboard`,
  but it doesn't go through `CachedNifImport`.
- **Suggested Fix**: Either (a) extend `CachedNifImport` with a
  `billboard_modes: Vec<(usize /* mesh_idx */, BillboardMode)>` so the
  spawn loop can re-insert; or (b) keep the SPT placeholder Billboard
  out of the cache and re-attach it post-spawn keyed on a
  `parent_node == 0` heuristic; or (c) make the placeholder spawn its
  own ECS components inline (bypass the cache, since there's only one
  mesh anyway — caching it is over-engineered for a static placeholder).
  Option (a) is the most general fix and unblocks future Skyrim+
  `NiBillboardNode`-rooted meshes that hit the same gap.

---

## SPT-D4-02: `bs_bound` not Z-up → Y-up converted in SpeedTree importer

- **Severity**: MEDIUM
- **Dimension**: Placeholder Fallback
- **Location**: [crates/spt/src/import/mod.rs:124-137](crates/spt/src/import/mod.rs#L124-L137), [crates/nif/src/import/mod.rs:208-211](crates/nif/src/import/mod.rs#L208-L211)
- **Status**: NEW
- **Description**: `params.bounds` is sourced from TREE.OBND, which is
  Bethesda **Z-up** (`[−x,−y,−z,+x,+y,+z]` i16 triplets). The NIF
  importer applies the axis swap when it hoists `BSBound` to
  `ImportedScene.bs_bound`:
  `center = [bb.center[0], bb.center[2], -bb.center[1]]`,
  `half_extents = [bb.dimensions[0], bb.dimensions[2], bb.dimensions[1]]`.
  The SpeedTree importer copies OBND through raw:
  ```rust
  let center = [
      (min[0] + max[0]) * 0.5,
      (min[1] + max[1]) * 0.5,
      (min[2] + max[2]) * 0.5,
  ];
  let half_extents = [
      (max[0] - min[0]) * 0.5,
      (max[1] - min[1]) * 0.5,
      (max[2] - min[2]) * 0.5,
  ];
  ```
  Meanwhile the *mesh* it builds is correctly in Y-up
  (`positions = [[-half_w, 0, 0] … [half_w, height, 0]]`,
  `normals = [[0, 0, 1]; 4]`). So the bounds and the geometry use
  different coordinate frames for the same scene.
- **Evidence**: Mesh is Y-up native (height along Y). `bs_bound` is
  raw Bethesda Z-up — a Joshua tree's OBND `[(−128,−128,0),(128,128,512)]`
  produces `center=[0,0,256]`, `half_extents=[128,128,256]` in the
  importer's output. After the swap should be `center=[0,256,128]`
  (tree extends up and forward in Y), `half_extents=[128,256,128]`
  (Y is the tall axis).
- **Impact**: Wide and shallow AABB vertically when it should be tall.
  Frustum culling rejects trees too early when looking up at them, or
  keeps them visible far below the camera. Spatial-query systems
  (selection, tex.missing diagnostics, BLAS bounds for ray tracing if
  the placeholder ever gets ray-traced) all read this AABB. Same
  blast-radius shape as the NIF-side bs_bound bug that #232 / #872
  fought.
- **Related**: SPT-D4-01 also discards this field via `CachedNifImport`
  before it reaches the ECS, so this bug is currently masked at runtime
  — but the fix for SPT-D4-01 will surface it. Address both together.
- **Suggested Fix**: Apply the same axis swap as
  `crates/nif/src/import/mod.rs:208-211`. Centralise the swap in a
  helper (e.g. lift `crates/nif/src/import/coord.rs::zup_point_to_yup`
  to a `byroredux-core::coord` module) so SPT and NIF can't drift
  again. Add a unit test using the existing FNV Joshua TREE OBND
  literal in `placeholder_uses_obnd_bounds_when_present`.

---

## SPT-D5-01: `SptImportParams.wind` docstring claims BNAM, but BNAM is billboard size; CNAM is the wind / canopy field

- **Severity**: LOW
- **Dimension**: Routing & CLI
- **Location**: [crates/spt/src/import/mod.rs:65-67](crates/spt/src/import/mod.rs#L65-L67), [byroredux/src/cell_loader/references.rs:919-923](byroredux/src/cell_loader/references.rs#L919-L923)
- **Status**: NEW
- **Description**: `SptImportParams.wind: Option<(f32, f32)>` is
  documented as *"Wind sensitivity / strength from the TREE record's
  `BNAM`. Captured for Phase 2 wind animation; not consumed today."*
  But the TREE parser
  ([crates/plugin/src/esm/records/tree.rs:29-31, 172-180](crates/plugin/src/esm/records/tree.rs#L29-L31))
  documents BNAM as "billboard width/height (two f32) on FO3/FNV;
  absent on Oblivion", and the cell-loader call site **already**
  acknowledges this:
  *"Wind sensitivity / strength would come from CNAM, not BNAM (BNAM
  is billboard-card width/height per UESP)"*. The `SptImportParams`
  docstring is the stale one — it pre-dates the parser doc clarification.
- **Evidence**:
  ```rust
  // crates/spt/src/import/mod.rs:65-67
  /// Wind sensitivity / strength from the TREE record's `BNAM`.
  /// Captured for Phase 2 wind animation; not consumed today.
  pub wind: Option<(f32, f32)>,
  ```
  ```rust
  // byroredux/src/cell_loader/references.rs:919-923 — correct doc
  // Wind sensitivity / strength would come from CNAM, not BNAM
  // (BNAM is billboard-card width/height per UESP). CNAM semantics
  // aren't pinned down yet — Phase 2 wires it. Leave None so the
  // placeholder doesn't pretend to know the wind response.
  let wind = None;
  ```
- **Impact**: Future Phase-2 wiring reads the stale doc and pulls BNAM
  into the wind field, producing tree-meadow trees that sway with their
  authored billboard width as the wind period. Cheap to fix now; will
  cost a debug round-trip if missed.
- **Suggested Fix**: Update the docstring to read "Wind sensitivity /
  strength from the TREE record's `CNAM` (Oblivion 5 × f32; FO3/FNV
  8 × f32). Captured for Phase 2 wind animation; not consumed today."
  Also: BNAM (FO3/FNV billboard size) is currently *unused* on the
  importer — consider folding it into `SptImportParams.bounds` when
  TREE.OBND is absent (Oblivion lacks BNAM; FO3/FNV have both, OBND
  wins).

---

## SPT-D2-01: No "first wins" semantics documented for duplicate-tag handling in the importer

- **Severity**: LOW
- **Dimension**: Tag Coverage
- **Location**: [crates/spt/src/import/mod.rs:94-97](crates/spt/src/import/mod.rs#L94-L97), [crates/spt/src/scene.rs:128-133](crates/spt/src/scene.rs#L128-L133)
- **Status**: NEW
- **Description**: The walker preserves authoring order
  (`SptScene::entries` is a `Vec`), and `scene.leaf_textures()` returns
  `Vec<&str>` — every tag-4003 entry. The importer collapses to
  `.first()`, implicitly choosing "first wins" but with no inline
  comment pinning the semantics. The audit checklist explicitly calls
  this out as needing a documented choice. Two corpus files in vanilla
  FNV/FO3 ship multiple tag-4003 entries (per the
  `entries_with_tag_handles_repeats` test); future dictionary work that
  swaps to `.last()` would silently change which texture renders.
- **Evidence**:
  ```rust
  let leaf_texture: Option<String> = params
      .leaf_texture_override
      .map(|s| s.to_string())
      .or_else(|| scene.leaf_textures().first().map(|s| s.to_string()));
  ```
  No comment on why `.first()` over `.last()`. The accessor docs
  ([crates/spt/src/scene.rs:128-133](crates/spt/src/scene.rs#L128-L133))
  just say "all leaf-texture paths (tag 4003)" without a precedence
  note.
- **Impact**: Future contributor confusion / regression risk only.
- **Suggested Fix**: Add a one-line comment at the `.first()` call site
  documenting "first-authored leaf texture wins; matches SpeedTree
  exporter convention where later tags are LOD-tier overrides". Or, if
  the SpeedTree exporter actually emits LOD tiers later in the stream,
  flip to `.last()` and document that.

---

## SPT-D3-01: No SHA-pinned regression sample inside the repo — CI without game data has zero corpus coverage

- **Severity**: LOW
- **Dimension**: Corpus Acceptance
- **Location**: [crates/spt/tests/parse_real_spt.rs](crates/spt/tests/parse_real_spt.rs), [crates/spt/src/parser.rs:164-198](crates/spt/src/parser.rs#L164-L198)
- **Status**: NEW
- **Description**: The corpus harness is env-var gated and `#[ignore]`,
  same pattern as `parse_real_nifs.rs`. The synthetic fixture
  (`build_synthetic_spt` in parser.rs) is hand-built from scratch and
  doesn't exercise any real vanilla bytes. There is no in-tree
  byte-stable sample (3-5 small `.spt` files pinned by SHA), so a CI
  without `BYROREDUX_FNV_DATA` or similar runs zero coverage on the
  corpus side. Phase 1's "regression-guard sample" checklist item is
  unsatisfied.
- **Evidence**: `find crates/spt -name '*.spt'` returns no files;
  `crates/spt/tests/parse_real_spt.rs` is the only corpus test and
  early-returns when the env var / fallback path is missing.
- **Impact**: A future parser refactor that breaks byte-stable parse of
  (say) `WastelandShrub01.spt` cannot be caught by CI unless the
  contributor has local game data installed. Easy to land a regression
  on the dictionary unnoticed.
- **Suggested Fix**: Either (a) commit one or two small (≤ 10 KB)
  `.spt` files under `crates/spt/tests/fixtures/` from a corpus you
  have redistribution rights to (none of the vanilla `.spt` files are
  redistributable — but a hand-authored / public-domain SpeedTree
  Reference Application output would be); or (b) generate a
  deterministic synthetic SPT under a `#[test]` setup helper that
  emits real-byte-shape fixtures (tag 2000 + 49-byte string +
  tag 2001 f32 + …) covering every dispatch arm, then SHA-pin the
  generator output. Option (b) is the cleanest given the
  redistribution constraint and matches the project's clean-room
  policy stance.

---

## SPT-D1-01: Unknown-in-range tag bails the whole walker; no partial-recovery skip

- **Severity**: LOW
- **Dimension**: TLV Format
- **Location**: [crates/spt/src/parser.rs:74-83](crates/spt/src/parser.rs#L74-L83)
- **Status**: NEW
- **Description**: When the walker encounters a tag in
  `[TAG_MIN, TAG_MAX]` that isn't in `dispatch_tag`, it records the
  tag into `unknown_tags`, sets `tail_offset = tag_offset`, and
  returns. There is no "skip-and-resume" mode — every entry past the
  unknown tag is discarded, even if the rest of the parameter section
  would parse cleanly. The 4 Oblivion outliers
  (`treems14canvasfreesu`, `treecottonwoodsu`, `shrubms14boxwood`,
  `treems14willowoakyoungsu`) all bail at the same shape:
  `tag=104 (0x0068)` at offset ~4-6 KB. Format notes (2026-05-09 entry)
  identify these as the **length prefix of a trailing curve text
  blob** in a section that has no leading tag header. Bailing is
  correct (we don't know how to read that section); discarding all
  *prior* successful entries is *also* correct — but the audit
  checklist's tail-detection contract is "walker stops cleanly at the
  offset where it bailed", which the current impl satisfies.
- **Evidence**: Test output:
  ```
  [OBL] 113 files | 113 with entries | 4 hit unknown tag | 20397 entries total | 96.46 % coverage
    unknown-tag samples (path / tag / offset):
      trees\treems14canvasfreesu.spt | tag=104 (0x0068) at offset 6047
      trees\treecottonwoodsu.spt | tag=104 (0x0068) at offset 5477
      trees\shrubms14boxwood.spt | tag=104 (0x0068) at offset 4343
      trees\treems14willowoakyoungsu.spt | tag=104 (0x0068) at offset 5782
  ```
- **Impact**: Phase 1.3 acceptance gate hit at 96.46 % Oblivion / 100 %
  FNV / 100 % FO3 — all ≥ 95 %. The 4 Oblivion outliers each preserve
  `scene.entries` from before the bail offset, so the leaf-texture
  resolution still works and the placeholder still renders. No
  runtime impact today. Recording as LOW for future dictionary work:
  once tag 104 is properly classified (likely a new section header
  that needs an entry in `dispatch_tag`), Oblivion coverage jumps to
  100 %.
- **Suggested Fix**: Dictionary refinement — `spt_dissect
  trees\treems14canvasfreesu.spt --offset 6047 --bytes 256` to dump
  the post-bail bytes, classify the section, add a `dispatch_tag`
  arm. No parser shape change needed. This is the kind of follow-up
  work tracked in `docs/format-notes.md` "Next sub-phase" already.

---

## SPT-D4-03: Mesh normal direction inconsistent with billboard rotation arc convention

- **Severity**: LOW
- **Dimension**: Placeholder Fallback
- **Location**: [crates/spt/src/import/mod.rs:194](crates/spt/src/import/mod.rs#L194), [byroredux/src/systems/billboard.rs:119-123](byroredux/src/systems/billboard.rs#L119-L123)
- **Description**: The placeholder mesh sets `normals = vec![[0.0,
  0.0, 1.0]; 4]` (face normal pointing +Z). The billboard system
  computes the camera-facing rotation as
  `Quat::from_rotation_arc(-Z, look_dir)` — i.e. it expects the mesh
  to face **-Z** in object space so that under identity rotation the
  mesh faces the camera (which by convention sits along +Z looking at
  -Z). Because the placeholder mesh is `two_sided: true`, both faces
  render and the inconsistency is visually invisible. But once
  SPT-D4-01 is fixed and the Billboard component is actually attached,
  the front face of the quad will be the *back* of the texture for
  half a frame of every rotation cycle, until the rotation arc points
  the +Z face away from the camera. Subtle Z-fighting on the leaf
  cutout alpha edge during that transition.
- **Status**: NEW
- **Impact**: Visible only as a one-frame flicker during fast
  camera-around-tree motion when SPT-D4-01 is fixed. Hidden today
  because the Billboard never attaches.
- **Suggested Fix**: Flip the normals to `[0, 0, -1]` AND swap the
  index order from `[0, 1, 2, 2, 3, 0]` to `[0, 3, 2, 2, 1, 0]` to
  preserve winding when the visible side flips. Add a unit test
  asserting the front-face normal points along -Z after the swap.
  Match the convention used by the NIF importer's billboard
  meshes (verify via `grep -rn "normal" crates/nif/src/import/mesh.rs`
  for a representative billboard-flagged NiTriShape's normal output).

---

## SPT-D4-04: Default placeholder size (256×512 game units) is FNV/FO3-scaled; Oblivion trees render at half-scale

- **Severity**: LOW
- **Dimension**: Placeholder Fallback
- **Location**: [crates/spt/src/import/mod.rs:73-77](crates/spt/src/import/mod.rs#L73-L77), [crates/plugin/src/esm/records/tree.rs:172-180](crates/plugin/src/esm/records/tree.rs#L172-L180)
- **Status**: NEW
- **Description**: The default
  `(DEFAULT_BILLBOARD_WIDTH, DEFAULT_BILLBOARD_HEIGHT) = (256.0,
  512.0)` is chosen as "a believable middle ground for FNV creosote /
  Joshua trees and Cyrodiil shrubs". But the Oblivion TREE corpus
  ships records *without* BNAM (FO3/FNV-only field) and **often
  without OBND** (per the test `parse_oblivion_short_cnam_no_bnam_no_pfig`
  asserting `bounds.is_none()`). For those Oblivion records, the
  placeholder falls back to the default 256×512 — but vanilla
  Oblivion tree NIFs typically span ~512 × 1024 game units (taller
  conifers in Great Forest). Result: every Oblivion `.spt` that
  lacks OBND renders as a Joshua-tree-sized placeholder, not a pine.
- **Evidence**: `crates/plugin/src/esm/records/tree.rs:296` —
  `assert!(tree.bounds.is_none(), "Oblivion TREE often omits OBND")`.
- **Impact**: Cyrodiil forests look like Mojave creosote groves. Low
  because (1) the placeholder is admittedly placeholder geometry, and
  (2) Phase 2 / Phase 3 will replace this with real geometry. But the
  default constant is currently mis-tuned for Oblivion.
- **Suggested Fix**: Per-game default selection — when `params.form_id`
  + the cell-loader-known `GameKind` says Oblivion, use a different
  default (e.g. 384 × 1024). Or: compute the default from `MODB`
  (bound radius) when present, since Oblivion ships MODB more reliably
  than OBND. MODB resolves to a sphere radius → bbox half-extent
  conversion.

---

## SPT-D5-02: BNAM (billboard size) parsed but never reaches `SptImportParams.bounds`

- **Severity**: LOW
- **Dimension**: Routing & CLI
- **Location**: [byroredux/src/cell_loader/references.rs:909-917](byroredux/src/cell_loader/references.rs#L909-L917), [crates/plugin/src/esm/records/tree.rs:95-97](crates/plugin/src/esm/records/tree.rs#L95-L97)
- **Status**: NEW
- **Description**: `TreeRecord.billboard_size: Option<(f32, f32)>`
  carries FO3/FNV BNAM (billboard width × height). The cell-loader's
  `parse_and_import_spt` wires `bounds` from OBND only, and `wind`
  from nothing. BNAM goes unused. The audit checklist expected
  "billboard tag captures: texture, world-space width/height, mip
  bias — these flow into the placeholder importer".
- **Evidence**:
  ```rust
  let bounds = tree_record.and_then(|t| t.bounds).map(|b| {
      let min = [b.min[0] as f32, b.min[1] as f32, b.min[2] as f32];
      let max = [b.max[0] as f32, b.max[1] as f32, b.max[2] as f32];
      (min, max)
  });
  // tree_record.billboard_size unreferenced.
  ```
- **Impact**: FO3/FNV TREE records that ship BNAM but lack OBND fall
  back to the 256×512 default instead of the authored billboard size.
  Combined with SPT-D4-04, this means the placeholder doesn't yet
  honour the most authoritative size field the TREE record can offer
  on FO3/FNV.
- **Suggested Fix**: Add `billboard_size: Option<(f32, f32)>` to
  `SptImportParams`, prefer it over OBND when both are present (since
  BNAM is authored for the leaf billboard specifically and OBND is the
  tree's physical bounding box). Update
  `compute_billboard_size` to check `params.billboard_size` first,
  then `params.bounds`, then the default.

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 1 |
| LOW | 7 |
| **Total** | **9** |

Filed as GitHub issues #994 (HIGH) · #995 (MEDIUM) · #996–#1002 (LOW).

### Per-dimension bill of health

| Dimension | Verdict | Notes |
|---|---|---|
| D1 — TLV Format | ✓ Clean | Defensive 64 KiB caps, clean EOF, unknown-in-range bail records and returns. One LOW (SPT-D1-01) on the no-skip-resume design. |
| D2 — Tag Coverage | ✓ Clean | ~98 tags catalogued with explicit confounder rejection. One LOW (SPT-D2-01) on undocumented "first-wins" duplicate semantics. |
| D3 — Corpus Gate | ✓ ≥ 95 % all games | FNV 100 % · FO3 100 % · Oblivion 96.46 %. One LOW (SPT-D3-01) on missing in-tree byte-stable fixture for CI without game data. |
| D4 — Placeholder | ⚠ Significant gap | SPT-D4-01 (HIGH): Billboard component is dropped on the cell-loader path — the entire premise of the yaw-billboard fallback is defeated in-game. SPT-D4-02 (MEDIUM): `bs_bound` Z-up. Two LOWs (orientation convention, Oblivion default scale). |
| D5 — Routing & CLI | ✓ Mostly clean | Extension dispatch, BSA sibling-load, TREE parser all correct. Two LOWs (stale BNAM/CNAM docstring, BNAM unused). |

The HIGH (SPT-D4-01) and the MEDIUM (SPT-D4-02) are wired
together — the cell-loader's `CachedNifImport` adapter drops both
fields, but SPT-D4-02 is currently *masked* by SPT-D4-01 and will
surface the moment the Billboard component is reinstated. Fixing both
in one go is the right path.

The corpus gate, tag dictionary, and TLV walker are all in good shape
against the Phase 1 contract. The placeholder-fallback regression is
the audit's main story: the importer's *output shape* is correct, but
the cell-loader's *cache adapter* throws away the fields that make the
fallback work. That mismatch is the kind of gap CLAUDE.md's
"feedback_speculative_vulkan_fixes.md" memory warns about — invisible
to `cargo test`, visible only by walking around a tree in a loaded
cell.

### Suggested next step

Run `/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-05-13.md` to file
these as GitHub issues.
