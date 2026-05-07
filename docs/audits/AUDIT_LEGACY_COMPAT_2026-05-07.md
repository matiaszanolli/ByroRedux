# Legacy Compatibility Audit — 2026-05-07

**Scope:** Dimensions 2 and 3 (NIF Format Readiness, Transform Compatibility) per the
`/audit-legacy-compat 2 3` invocation. Dimensions 1, 4, 5, 6 deferred to a future sweep.

**Predecessor:** [AUDIT_LEGACY_COMPAT_2026-04-30.md](AUDIT_LEGACY_COMPAT_2026-04-30.md).
All MEDIUM/LOW carry-overs in that report were closed during the intervening week
(see "Carry-over Reconciliation" below).

---

## Summary

Both audited dimensions are in steady-state. The NIF parser has 195 dispatch arms,
covers NIF v4.x → v20.2.0.7 (Morrowind through Starfield), and resolves cross-references
through the `BlockRef` + `NifScene::validate_refs` pair. Transform composition uses
Shepperd-method Matrix3 → Quat (normalised post-extraction, #333) plus Z-up → Y-up
basis swap (#232). World-transform propagation is BFS-based with cached root sets (#825).

Two NEW findings — both LOW — surfaced during this sweep. Three known-OPEN issues
intersect the audited dimensions (#841, #532, #839) and are listed verbatim with
status notes; they do not require re-investigation.

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 0     |
| MEDIUM   | 0     |
| LOW      | 2 NEW + 3 existing |

---

## Carry-over Reconciliation

All findings flagged "STILL OPEN" in the 2026-04-30 audit have since closed.
Re-verified in current code; no regressions.

| Prior ID            | Status                | Notes                                                              |
|---------------------|-----------------------|--------------------------------------------------------------------|
| D3-NEW-01 (MEDIUM)  | **FIXED** via #771    | `bind_inverses[i]` now encodes both per-bone bind-inverse AND global skin-to-skel offset (per nifly `Skin.hpp:49-51`); `compute_palette_into` no longer needs the separate global term. See [skinned_mesh.rs:139-154](../../crates/core/src/ecs/components/skinned_mesh.rs#L139-L154). |
| D5-NEW-03 (LOW)     | **FIXED** via #772    | NPC `AnimationPlayer` attach landed; bind-pose mismatch dissolved together with #771. |
| D1-NEW-01 (LOW)     | **FIXED** via #606    | `NiNode.culling_mode` now honoured beyond the `BsMultiBoundNode` path. |
| #337 (LOW)          | **FIXED** via #607    | Stencil property pipeline integration. (Bundled with the D4-NEW-01 closure.) |
| D4-NEW-01 NiFog…    | **FIXED** via #607    | Fog / wireframe / dither / shade properties wired into the material pipeline (#558 / #607). |
| N2-NEW-01 (LOW)     | **FIXED** via #608    | `VF_INSTANCE` decoder landed. |

---

## Dimension 2 — NIF Format Readiness

### Verified Working — Confirmed No Gaps

- **Parser dispatch breadth:** 195 explicit dispatch arms in
  [crates/nif/src/blocks/mod.rs](../../crates/nif/src/blocks/mod.rs) cover
  NiNode (+ 7 alias subclasses + 9 dedicated subclass parsers), BSTriShape /
  BSGeometry / BSSubIndexTriShape / BSDynamicTriShape / BSLODTriShape /
  BSMeshLODTriShape / BSSegmentedTriShape, every NiProperty, every shader
  property family across Oblivion → Starfield, the full particle stack
  (~48 NiPSys* types), Havok (rigid bodies, shapes, constraints, packed
  trees, MOPP), legacy particles, animation interpolators (transform,
  bspline-comp-transform, look-at, path, blend), 18 controller types, and
  every extra-data variant we've encountered in vanilla content. Unknown
  block types fall through to `NiUnknown` placeholders that preserve
  block-index integrity.
- **Version coverage:** `NifVersion::detect` covers Morrowind (v4.0.0.2)
  → Starfield (v20.2.0.7 with `user_version_2 ≥ 170`); see
  [version.rs:84-118](../../crates/nif/src/version.rs#L84-L118). 21 feature
  predicates encode per-game wire-format gates so parsers can ask
  `variant.has_dedicated_shader_refs()` rather than re-deriving the
  `user_version` / `bsver` arithmetic. Pre-Gamebryo NetImmerse files
  (inline block-type names, no header type table) have a dedicated branch
  at [lib.rs:237-245](../../crates/nif/src/lib.rs#L237-L245).
- **Link resolution:** `BlockRef` (a `u32`-indexed handle with `NULL == u32::MAX`)
  is the canonical cross-reference type. `NifScene::validate_refs` walks every
  block, downcasts to the `HasObjectNET` / `HasAVObject` / `HasShaderRefs`
  trait family, and emits a `RefError` for any out-of-range link. Coverage
  includes `controller_ref`, `extra_data_refs`, `collision_ref`, `properties[]`,
  `shader_property_ref`, `alpha_property_ref`, `NiNode.children[]`,
  `NiNode.effects[]`, and `NifScene.root_index`. See
  [scene.rs:106-197](../../crates/nif/src/scene.rs#L106-L197).
- **Recovery paths:** Three independent fallbacks for malformed / unknown
  blocks (header `block_sizes`, runtime size cache via `parsed_size_cache`,
  user-supplied `oblivion_skip_sizes` for v20.0.0.5 NIFs without a size
  table). All three bump `recovered_blocks` so the integration parse-rate
  gate doesn't silently treat realigned NIFs as clean (#568).
- **Pre-Gamebryo `groupID` quirk:** NIFs in `[10.0.0.0, 10.1.0.114)`
  carry a 4-byte `groupID` on every NiObject before subclass payload;
  consumed and discarded at [blocks/mod.rs:140-143](../../crates/nif/src/blocks/mod.rs#L140-L143)
  per nifly `BasicTypes.hpp:972`.
- **Scene root selection:** `is_ni_node_subclass` covers the dedicated
  subclasses with their own `block_type_name` (BSOrderedNode, BSValueNode,
  BSMultiBoundNode, BSTreeNode, NiBillboardNode, NiSwitchNode, NiLODNode,
  NiSortAdjustNode, BSRangeNode); aliased subclasses (BSFadeNode,
  BSLeafAnimNode, RootCollisionNode, AvoidNode, NiBSAnimationNode,
  NiBSParticleNode) report `"NiNode"` and are caught by the first arm.
  Regression-tested at [lib.rs:884-931](../../crates/nif/src/lib.rs#L884-L931).
- **KFM (KeyFrame Metadata) parser:** Full binary-format KFM parser in
  [crates/nif/src/kfm.rs](../../crates/nif/src/kfm.rs) covers v1.2.0.0 →
  v2.2.0.0; ASCII variant intentionally omitted (Gamebryo's reference
  reader rejects ASCII KFMs).

### NEW — LOW

#### LC-D2-NEW-01: `NiTextureEffect` parsed but never imported

- **Severity**: LOW
- **Dimension**: NIF Format Readiness — importer coverage gap
- **Game Affected**: Oblivion exterior cells (terrain sun gobo / projected
  shadow), Oblivion magic FX (projected env maps), FO3 / FNV interior
  light cookies, occasional Skyrim-LE projected decals.
- **Location**: parser at
  [crates/nif/src/blocks/texture.rs:567-684](../../crates/nif/src/blocks/texture.rs#L567-L684);
  dispatch at [crates/nif/src/blocks/mod.rs:207](../../crates/nif/src/blocks/mod.rs#L207).
  No consumer exists in `crates/nif/src/import/` or the binary crate.
- **Status**: NEW (companion to closed #163, which scoped the parser only).
- **Description**: `NiTextureEffect` is a `NiDynamicEffect` subclass that
  attaches a projected texture (sphere map / env map / gobo / projected
  shadow / clipping plane) to its `Affected Nodes` list — the legacy
  Gamebryo equivalent of a projector light. Issue #163 landed the parser
  with all 12 wire fields (model_projection_matrix, model_projection_translation,
  texture_filtering, texture_clamping, texture_type, coordinate_generation_type,
  source_texture ref, clipping_plane / plane, PS2 L/K). The parser is
  reachable from the dispatch and downcasts cleanly. But:
  - No code path under `crates/nif/src/import/` queries
    `scene.blocks` for `NiTextureEffect` downcasts;
  - `import_nif_lights` (`crates/nif/src/import/mod.rs:886`) only walks
    `NiLight`-derived blocks (point / spot / directional / ambient);
  - No ECS component represents a projected-texture effect, so even
    plumbing wouldn't have a consumer today.

  Net effect: every `NiTextureEffect` block in vanilla Oblivion content
  is parsed, validated, and silently discarded.

- **Evidence**: `grep -rn NiTextureEffect crates/nif/src/import/ byroredux/src/ crates/renderer/src/`
  returns zero hits. `grep` against `crates/nif/src/blocks/texture.rs` is
  the only match — i.e. the type exists but has no readers outside the
  parser itself.
- **Impact**: Oblivion exterior sun gobos and FO3/FNV light cookies
  appear as plain unprojected lighting. Oblivion magic-FX meshes that
  rely on projected env maps render without the env contribution.
  Volume of affected content is small (single-digit instances per
  vanilla cell), and the visual delta is subtle on top of the existing
  RT lighting, hence LOW. No parser-side regression risk.
- **Suggested Fix**: Two-phase. Phase 1 (~30 LOC): add an `ImportedTextureEffect`
  struct to `import/mod.rs` populated alongside `ImportedLight`, keyed by
  the same `NiDynamicEffect.affected_nodes` resolution that #335 / #461
  already implements for lights. Phase 2 (deferred): renderer-side
  projector pass — currently no infrastructure exists; a `ProjectedTexture`
  ECS component plus a fragment-shader sample step could ride on top of
  the existing texture-registry plumbing once a milestone needs it.

### NEW — LOW

#### LC-D2-NEW-02: NIF parser does not surface a `validate_refs` policy lever

- **Severity**: LOW (defensive-coding / observability gap, not a runtime bug)
- **Dimension**: NIF Format Readiness — link integrity
- **Location**: [crates/nif/src/lib.rs:135-165](../../crates/nif/src/lib.rs#L135-L165),
  [crates/nif/src/scene.rs:106-197](../../crates/nif/src/scene.rs#L106-L197)
- **Status**: NEW
- **Description**: `parse_nif` and `parse_nif_with_options` never invoke
  `NifScene::validate_refs` — the link-validity walk is opt-in for callers,
  and the doc-comment at `scene.rs:90-93` makes that explicit ("an optional
  post-parse sanity pass — `parse_nif` does not run it"). In practice, the
  walker (`crates/nif/src/import/walk.rs`) and the cell-loader pre-parse
  both consume `NifScene` directly via `scene.get(idx)` / `scene.get_as::<T>(idx)`,
  which silently returns `None` for out-of-range indices. A truncated
  parse (`scene.truncated == true`) can leave dangling block indices for
  references in earlier blocks; today these are masked by the `Option`
  return from `get`, but no upper layer reports the dangling-ref count.

  This is structurally fine — `truncated` already gates downstream
  consumers — but `recovered_blocks > 0` does NOT imply dangling refs,
  and `validate_refs` is the only mechanism that actually quantifies the
  link damage. Without surfacing the count, regressions in cross-reference
  stability (e.g. a parser that under-consumes and shifts the *next* block
  by N bytes, leaving a downstream BlockRef pointing into the middle of
  another block) only show up as a render artifact, not a parser-level
  signal.

- **Evidence**: No call site invokes `validate_refs` outside the
  `validate_refs_tests` module in `scene.rs` itself. `nif_stats`
  (`crates/nif/examples/nif_stats.rs`) does not run it.
- **Impact**: Latent observability gap. A parser regression that produces
  technically-Ok scenes with semantically-broken links wouldn't trip any
  integration gate. No active bug today — both the truncated path and
  the recovery paths leave `BlockRef`s either correctly null-guarded or
  pointing at `NiUnknown` placeholders that share the original index.
- **Suggested Fix**: Three options of increasing weight:
  1. Add an opt-in `ParseOptions::validate_links: bool` flag that runs
     `validate_refs` and bumps a new `link_errors: usize` field on `NifScene`.
  2. Run validation unconditionally in `parse_nif_with_options` and store
     the count; let the existing parse-rate gate downgrade scenes with
     `link_errors > 0` to non-clean.
  3. Run validation as part of the integration sweep only (`tests/parse_real_nifs.rs`),
     with a per-game histogram of dangling-ref kinds.

  Option 1 is the minimum-surface change; Option 3 is the cheapest in
  hot-path overhead. Either leaves the existing call-site contract
  intact.

### Existing Open Issues — Verified Still Relevant

| Issue | Title | Note |
|-------|-------|------|
| #532  | KFM binary parser is complete but not wired to actor-controller loading | Parser at [crates/nif/src/kfm.rs](../../crates/nif/src/kfm.rs) is full-featured (verified line-by-line vs Gamebryo 2.3 `NiKFMTool::ReadBinary`). No call site under `crates/scripting/src/` or `byroredux/src/npc_spawn.rs` invokes `parse_kfm`. Animation runtime today uses `import_kf` directly. |
| #720  | FO4 / FO76 `BSEyeCenterExtraData` undispatched — head meshes lose eye anchors | Confirmed: dispatch arm absent. Head NIFs lose the per-vertex eye-center reference used for gaze tracking. |
| #728  | FO76 `BSCollisionQueryProxyExtraData` + `NiPSysRotDampeningCtlr` undispatched | Confirmed absent from dispatch table. |
| #766  | SE Meshes1 Havok long-tail: `bhkBallSocketConstraintChain`, `bhkPlaneShape` undispatched | Confirmed absent. |
| #571  | `BSDynamicTriShape` with `data_size==0` produces renderable shape with zero triangles — silent import failure | `BsTriShape::parse_dynamic` accepts the zero-size case; importer should skip. |
| #839  | Per-block stream realignments invisible to `nif_stats` parse-rate gate — silent data loss masked by `clean==total` | Same observability theme as LC-D2-NEW-02 above. |

---

## Dimension 3 — Transform Compatibility

### Verified Working — Confirmed No Gaps

- **Matrix3 → Quat conversion:** `zup_matrix_to_yup_quat` at
  [crates/nif/src/import/coord.rs:33-59](../../crates/nif/src/import/coord.rs#L33-L59)
  applies the Z-up → Y-up basis change (`R_yup = C × R_zup × C^T`),
  validates determinant ≈ 1.0 (tolerance ±0.1 to admit minor export-tool
  drift), and extracts the quaternion via Shepperd's method
  (`matrix3_to_quat` at lines 74-116). The post-extraction
  `normalize_quat` step (added by #333) ensures unit-length output even
  when the input determinant drifts up to ±7 %, preventing shear/scale
  contamination of the ECS `Transform.rotation`. Degenerate inputs
  (rank-deficient, det near zero) fall through to nalgebra SVD repair
  (`svd_repair_to_quat`), which is the only nalgebra dependency on the
  hot path.
- **Rotation sanitisation at parse time:** `crate::rotation::sanitize_rotation`
  ([rotation.rs:60-67](../../crates/nif/src/rotation.rs#L60-L67)) is called
  during NIF parse so downstream consumers (`compose_transforms`,
  `zup_matrix_to_yup_quat`) can assume valid rotations on the fast path.
  See #277.
- **NiTransform composition:** `compose_transforms` at
  [crates/nif/src/import/transform.rs:13-25](../../crates/nif/src/import/transform.rs#L13-L25)
  matches Gamebryo's `NiTransform::operator*`:
  `world.rot = parent.rot × child.rot`,
  `world.trans = parent.trans + parent.rot × (parent.scale × child.trans)`,
  `world.scale = parent.scale × child.scale`. Operates in NIF space (Z-up)
  before the per-leaf basis swap; this keeps composition algebraically
  identical to the legacy engine and isolates the coordinate change to
  one helper.
- **GlobalTransform composition (ECS runtime):** `GlobalTransform::compose`
  at [global_transform.rs:48-61](../../crates/core/src/ecs/components/global_transform.rs#L48-L61)
  is the quaternion equivalent of the matrix formula above:
  `parent.translation + parent.rotation * (parent.scale * local_translation)`,
  `parent.rotation * local_rotation`, `parent.scale * local_scale`. Under
  uniform scale (the only mode Gamebryo and Redux both support), the
  matrix and quaternion forms produce bit-equivalent results. Three
  unit tests pin the formula at component, two-level, and three-level
  depths.
- **World-transform propagation:** `make_transform_propagation_system` at
  [crates/core/src/ecs/systems.rs:41-163](../../crates/core/src/ecs/systems.rs#L41-L163)
  is the ECS analogue of Gamebryo's `NiNode::UpdateDownwardPass`. Phase 1
  identifies roots (Transform without Parent) and copies their local
  transform to GlobalTransform. Phase 2 BFS-walks `Children` from each
  root and composes via `GlobalTransform::compose`. The root set is
  cached behind a `(Transform.len, Parent.len, next_entity_id)` generation
  key — rescanned only when entities are spawned / despawned / re-parented.
  Per-frame cost on Megaton (~6k entities, ~30 roots) is dominated by
  the BFS walk; the four ECS query handles are acquired once and held
  across the entire walk (#238). 8 regression tests pin the behaviour.
- **NPC spawn parenting topology:** Confirmed clean — `npc_spawn.rs` at
  lines 319-323 / 366-367 / 446-447 / 595-596 attaches body / head under
  the placement_root through standard `Parent` / `Children` ECS components.
  Single canonical transform path; no parallel propagation system.
- **Skin-palette composition:** `bind_inverses[i]` (per-bone) is now the
  only term in `compute_palette_into`, mirroring nifly's `Skin.hpp:49-51`
  which encodes both the per-bone bind-inverse AND the global skin-to-skel
  offset in one matrix. Verified at
  [skinned_mesh.rs:139-171](../../crates/core/src/ecs/components/skinned_mesh.rs#L139-L171)
  and pinned by the
  `palette_matches_nifly_skin_to_bone_semantics_with_non_identity_global`
  test (#771).
- **Z-up → Y-up axis swap consistency:** The `[x, z, -y]` swap is
  implemented in exactly one helper (`zup_point_to_yup` at
  [coord.rs:18-20](../../crates/nif/src/import/coord.rs#L18-L20)) and
  one matrix counterpart. 13 import sites across `mesh.rs` and `walk.rs`
  share these — copy-paste drift is structurally prevented (#232).
- **CW-positive rotation handling:** `zup_matrix_to_yup_quat` doc-comment
  documents that Gamebryo's CW convention produces matrices that are the
  transpose of the standard CCW form, but `M × p` gives the same physical
  result regardless of convention — so no transpose is needed before
  Shepperd extraction. Saved in memory at `gamebryo_cw_rotation.md`.

### Existing Open Issues — Verified Still Relevant

| Issue | Title | Note |
|-------|-------|------|
| #841  | M41-PHASE-1BX: body-skinning spike artifact under cell-load placement_root parent chain | Investigation in flight. The transform composition path itself is verified clean (above); the artifact is suspected to be in the order in which `Parent` edges are inserted relative to skel_root spawn — see the "Spawn order mirrors `byroredux::npc_spawn::spawn_npc_entity`" test at `crates/core/src/ecs/systems.rs:340-349`. Carries on its own track. |

### Dimension 3 — No NEW Findings

Re-reviewed each composition site with attention to:
- non-uniform scale in the parent chain (Gamebryo and Redux both gate to uniform
  scale; cannot drift);
- negative determinant inputs to `matrix3_to_quat` (gated out by the det ≈ 1.0
  check, fall through to SVD repair);
- silent failure when an intermediate parent lacks `GlobalTransform` (the BFS
  `continue`s for that subtree, leaving descendants at their previous frame's
  value — defensive behaviour, matches Bevy semantics, not a defect);
- `NiSkinData::skinTransform` field-order on disk (#767, fixed; Rotation-first
  per nif.xml STRUCT order, distinct from inline NiAVObject Translation-first);
- `BsPackedGeomDataCombined.transform` field-order (same fix, also #767).

No drift detected.

---

## Priority Fix Order

1. **LC-D2-NEW-01** (LOW) — `NiTextureEffect` importer plumbing. Phase 1
   (importer struct + walker entry) is ~30 LOC. Phase 2 (renderer pass)
   waits for a milestone that needs projected gobos.
2. **LC-D2-NEW-02** (LOW) — Surface a `validate_refs` policy lever in
   `ParseOptions`. Pairs naturally with #839 (parse-rate gate observability).
3. **#532** (LOW) — KFM wired to actor-controller loading. Carry as a
   milestone reminder; no current consumer needs the state-machine layer
   today since `import_kf` already feeds the runtime directly.
4. **#720 / #728 / #766 / #571** — long-tail dispatch / import gaps. Track
   on the existing per-game audit cadence (audit-fo4, audit-fo76,
   audit-skyrim).

---

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-05-07.md
```
