# Legacy Compatibility Audit — 2026-05-23

**Scope:** Full sweep, dimensions 1–6 per `/audit-legacy-compat`.

**Predecessors:**
- [AUDIT_LEGACY_COMPAT_2026-05-19.md](AUDIT_LEGACY_COMPAT_2026-05-19.md) (last full sweep — surfaced 14 findings across D1/D2/D3/D4; all NEW findings since closed).
- [AUDIT_NIF_2026-05-22_DIM5.md](AUDIT_NIF_2026-05-22_DIM5.md) (yesterday's NIF coverage spot — clean against the 36+ orphan-parse sweep).
- [AUDIT_LEGACY_COMPAT_2026-05-07.md](AUDIT_LEGACY_COMPAT_2026-05-07.md) (steady-state confirmation for D2/D3).

---

## Executive Summary

The five-day window since the previous sweep landed **86 commits** — the
heaviest in M30.2 (full Papyrus parser), M47.0 (script triggering), M47.1
(Condition eval), M28.5 (character controller), and M40 (cell-swap). None
of that work touched the NIF parser, animation runtime, transform math,
property→material walker, or string-interning layer directly, so the
"core six dimensions" are largely steady-state.

Every NEW finding from the 2026-05-19 sweep is verified closed end-to-end:

| Finding | Issue | Closure evidence |
|---|---|---|
| D1-NEW-01 (FormIdComponent never attached) | #1212 | [`spawn.rs:182`](../../byroredux/src/cell_loader/spawn.rs#L182) |
| D1-NEW-02 (LocalBound never seeded) | #1213 | [`spawn.rs:725-735`](../../byroredux/src/cell_loader/spawn.rs#L725-L735) |
| D1-NEW-03 (BSXFlags dropped at spawn) | #1214 | [`spawn.rs:207`](../../byroredux/src/cell_loader/spawn.rs#L207) |
| D2 FIND-1 (silent zero-mesh import) | #1215 | [`references.rs:913-925`](../../byroredux/src/cell_loader/references.rs#L913-L925) |
| D2 FIND-2 (BSTriShape zero-vertex non-skinned no counter) | #1216 | [`bs_tri_shape.rs:378-386`](../../crates/nif/src/blocks/tri_shape/bs_tri_shape.rs#L378-L386) |
| D2 FIND-3 (cache-hit on zero-mesh hides drop) | #1217 | [`precombined.rs:103-120`](../../byroredux/src/cell_loader/precombined.rs#L103-L120) |
| D2 FIND-4 (CLAUDE.md parse-rate drift) | #1218 | docs refreshed in [`7635c1d0`](../../) |
| D2 FIND-5 (NifVariant v20.0.0.4/u11 routing) | #1219 | resolved via sample sweep |
| D3-NEW-01 (exterior CELL XCRI/XPRI hardcoded empty) | #1220 | [`wrld.rs:342-385`](../../crates/plugin/src/esm/cell/wrld.rs#L342-L385) |
| D3-NEW-02 (no exterior spawn_precombined_meshes call) | #1221 | [`exterior.rs:305-312`](../../byroredux/src/cell_loader/exterior.rs#L305-L312) |
| D3-NEW-03 (spawn_precombined_meshes lacks cell_origin) | #1222 | [`precombined.rs:63-79`](../../byroredux/src/cell_loader/precombined.rs#L63-L79) |
| D4-NEW-01 (BSLSP under-reads on FO4 shared-precombined) | #1223 | [`shader.rs:939-963`](../../crates/nif/src/blocks/shader.rs#L939-L963) (root cause was duplicate env_map_scale read, not Shared-variant trailer) |
| D4-NEW-02 (NiFogProperty no walker arm) | #1224 | deliberately not dispatched; behaviour codified at [`properties.rs:483-492`](../../crates/nif/src/blocks/properties.rs#L483-L492) and in the audit-skill carve-out (Dimension 4 note) |

This sweep surfaces **one NEW finding (LOW)** — `SceneFlags` is inserted
by the loose-NIF loader but dropped at the cell-loader spawn boundary,
the same shape as the D1-NEW-01/02/03 cluster but distinctly not part of
that closure. Severity is LOW because no runtime system reads `SceneFlags`
yet; the gap is parity / debug-introspection only.

### Severity rollup

| Severity | Count | Dims (where) |
|----------|-------|--------------|
| CRITICAL | 0 | — |
| HIGH     | 0 | — |
| MEDIUM   | 0 | — |
| LOW      | 1 | D1-NEW-01 (SceneFlags parity) |
| Steady-state | dims 2, 3, 4, 5, 6 | zero new findings |

### Recommended next step

1. **D1-NEW-01 (this sweep)** — two-line spawn-site change in
   [`byroredux/src/cell_loader/spawn.rs`](../../byroredux/src/cell_loader/spawn.rs):
   wire `mesh.flags` → `SceneFlags::from_nif` on the per-mesh insert (mirrors
   `nif_loader.rs:790`) and optionally on the placement root from `ImportedNode.flags`
   on the path that produces the cache entry. Pairs naturally with the next
   cell-loader cleanup commit.

Suggested publish: `/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-05-23.md`

---

## Dimension 1 — Scene Graph Decomposition

**Today: 2026-05-23. Prior baseline: AUDIT_LEGACY_COMPAT_2026-05-19.md dim 1 (3 NEW findings, all closed: #1212, #1213, #1214).**

The three closed findings re-verified by direct read:
- `FormIdComponent` lands on every placement root with a valid form-id
  pair at [`spawn.rs:175-183`](../../byroredux/src/cell_loader/spawn.rs#L175-L183).
- `LocalBound` is inserted alongside Transform / GlobalTransform on every
  mesh entity at [`spawn.rs:725-735`](../../byroredux/src/cell_loader/spawn.rs#L725-L735);
  bounds propagation at [`systems/bounds.rs`](../../byroredux/src/systems/bounds.rs) consumes
  it as designed.
- `BSXFlags` rides through `CachedNifImport.bsx_flags` (set at
  [`references.rs:937-944`](../../byroredux/src/cell_loader/references.rs#L937-L944))
  and lands on the placement root at [`spawn.rs:201-208`](../../byroredux/src/cell_loader/spawn.rs#L201-L208).

`Transform` / `Parent` / `Children` / `GlobalTransform` / `Name` /
`MeshHandle` / `TextureHandle` / `Material` / `AlphaBlend` / `TwoSided` /
`NormalMapHandle` / `DarkMapHandle` / `ExtraTextureMaps` / `IsFxMesh` /
`RenderLayer` / `LightSource` / `Billboard` / `ParticleEmitter` / collision
body+shape are all populated correctly. One NEW gap surfaces below.

---

### D1-NEW-01: `SceneFlags` inserted by loose-NIF loader but dropped at cell-loader spawn boundary

- **Severity**: LOW
- **Dimension**: Scene Graph Decomposition
- **Location**: [`byroredux/src/cell_loader/spawn.rs`](../../byroredux/src/cell_loader/spawn.rs)
  (no `SceneFlags` insert anywhere) vs.
  [`byroredux/src/scene/nif_loader.rs:451`](../../byroredux/src/scene/nif_loader.rs#L451)
  (NiNode path) +
  [`byroredux/src/scene/nif_loader.rs:790`](../../byroredux/src/scene/nif_loader.rs#L790)
  (mesh path) — both inserting `SceneFlags::from_nif(.flags)`.
- **Status**: NEW
- **Description**:
  `SceneFlags` exists at
  [`crates/core/src/ecs/components/scene_flags.rs`](../../crates/core/src/ecs/components/scene_flags.rs)
  with bits for `APP_CULLED` / `SELECTIVE_UPDATE` / `SELECTIVE_XFORMS` /
  `SELECTIVE_PROP_CONTROLLER` / `SELECTIVE_RIGID` / `DISPLAY_OBJECT` /
  `DISABLE_SORTING` / `SELECTIVE_XFORMS_OVERRIDE` / `IS_NODE`. `ImportedNode.flags`
  and `ImportedMesh.flags` both carry the raw `NiAVObject.flags` value
  through the importer per #222.

  The loose-NIF loader (`load_nif_bytes_with_skeleton`) inserts the
  component on both NiNode and mesh entities; the comment block at
  [`nif_loader.rs:442-449`](../../byroredux/src/scene/nif_loader.rs#L442-L449)
  is explicit about the rationale ("Attach raw NiAVObject flags so
  gameplay systems can branch on DISABLE_SORTING, SELECTIVE_UPDATE,
  IS_NODE, DISPLAY_OBJECT, etc. without re-reading the source NIF").

  The cell-loader spawn path — which is the dominant entry point for
  every cell-loaded entity (Megaton, Diamond City, every grid-loaded
  exterior REFR) — never inserts `SceneFlags`. `mesh.flags` is read
  exactly once across the entire cell-loader subtree (a `LightData.flags`
  copy at [`spawn.rs:980`](../../byroredux/src/cell_loader/spawn.rs#L980),
  unrelated to `SceneFlags`); `ImportedNode.flags` is never read at all.

- **Evidence**:
  ```
  $ grep -rn "SceneFlags" byroredux/src/cell_loader/
  (no results)

  $ grep -n "mesh.flags" byroredux/src/cell_loader/spawn.rs
  980:                        flags: ld.flags,    # LightData, not SceneFlags
  ```
  The loose-NIF path uses the component identically at two sites
  ([`nif_loader.rs:450-452`](../../byroredux/src/scene/nif_loader.rs#L450-L452)
  and [`nif_loader.rs:789-791`](../../byroredux/src/scene/nif_loader.rs#L789-L791)),
  guarding on `flags != 0` to avoid empty rows. The cell-loader has no
  parallel insert.

- **Impact**:
  - **Today, functional**: nil. No runtime system reads `SceneFlags`
    post-spawn — `APP_CULLED` is already filtered at the importer walker
    ([`walk/mod.rs:344`](../../crates/nif/src/import/walk/mod.rs#L344) +
    `:388` + `:789` + `:815`), so culled shapes never reach the spawn
    site. The other bits have no consumer yet.
  - **Today, debug**: console commands that introspect ECS rows
    (`inspect`, `prid`) see `SceneFlags` on loose-NIF-loaded entities
    but never on cell-loaded ones, which is a confusing inconsistency
    when debugging cell content. Same shape as the dead-`prid`-on-cell
    case that motivated #1212.
  - **Forward-looking**: any future system that toggles visibility
    (`set_culled`, scripted Disable / Enable on a REFR), respects
    `DISABLE_SORTING` (alpha-stack draw order), or branches on
    `SELECTIVE_UPDATE` for animation-cost gating will need the row.
    The Papyrus-side `ObjectReference::Disable()` event is one obvious
    near-term consumer (the [Papyrus ObjectReference API memory](../../memory/objectreference_api.md)
    flags it as a Visibility entry point).

- **Related**: closure cluster #1212 / #1213 / #1214 (same shape — parsed
  data dropped at the cell-loader spawn boundary). The parent issue
  [#222](https://github.com/matiaszanolli/ByroRedux/issues/222) was
  closed when the loose-NIF path landed; the cell-loader path was never
  wired alongside it.

- **Suggested Fix**:
  At [`spawn.rs:725`](../../byroredux/src/cell_loader/spawn.rs#L725)
  (the per-mesh insert block), add:
  ```rust
  if mesh.flags != 0 {
      world.insert(entity, SceneFlags::from_nif(mesh.flags));
  }
  ```
  For the placement-root parity: thread `ImportedNode.flags` through
  `CachedNifImport` (same pattern as `bsx_flags` post-#1214) so the
  root-node bits land on `placement_root`. Loose-NIF parity tests in
  [`scene/nif_loader_tests.rs`](../../byroredux/src/scene/nif_loader_tests.rs)
  already exist; mirror them under `cell_loader/spawn_tests.rs`.

---

### Steady-state checks (no findings)

- **Transform composition**, **Parent / Children edges**, **placement-root
  convention** (#544): unchanged since the last sweep.
- **`build_subtree_name_map` lookup** through `Name(FixedString)` rows
  seeded at spawn time: still single-pass.
- **Component re-export surface** at
  [`crates/core/src/ecs/components/mod.rs`](../../crates/core/src/ecs/components/mod.rs):
  25 modules, all wired into the dispatcher. `inventory.rs` /
  `attach_points.rs` / `animated.rs` (the post-2026-05-07 additions)
  are inserted by the appropriate import paths (NPC spawn, FaceGen,
  shader controllers).

---

**Summary**: dim 1 surfaced 1 NEW finding, LOW. The three HIGH-severity
plumbing gaps from 2026-05-19 are closed end-to-end. SceneFlags is the
last sibling in that cluster that wasn't picked up by the closure.

---

## Dimension 2 — NIF Format Readiness

**Today: 2026-05-23. Prior baseline: AUDIT_LEGACY_COMPAT_2026-05-19.md dim 2 (6 findings — 5 NEW LOW + 1 cross-link to #1188; all closed).**

Dispatch arm count today: **249** (unchanged from 2026-05-19). No new
arms landed in the audit window — the 86-commit window touched
papyrus/scripting/physics/cell-streaming, not NIF dispatch. The
yesterday's [AUDIT_NIF_2026-05-22_DIM5.md](AUDIT_NIF_2026-05-22_DIM5.md)
deep audit re-verified every orphan-parse case from #974 and found the
sweep clean.

### Observability closures (verified)

- **FIND-1 / #1215 — zero-contribution-import warn**:
  [`references.rs:905-925`](../../byroredux/src/cell_loader/references.rs#L905-L925).
  Fires when `meshes.is_empty() && collisions.is_empty() && lights.is_empty()
  && particle_emitters.is_empty() && embedded_clip.is_none()` — the exact
  predicate the audit proposed.
- **FIND-2 / #1216 — BSTriShape zero-vertex non-skinned counter**:
  [`bs_tri_shape.rs:378-386`](../../crates/nif/src/blocks/tri_shape/bs_tri_shape.rs#L378-L386).
  Surfaces at `log::debug!` rather than `warn` (audit memory note: vanilla
  FO4 ships 124,871 legitimate zero-vertex shapes that would flood the
  default log — debug is the right level).
- **FIND-3 / #1217 — cache-hit on zero-mesh entry surfaces in
  precombined-spawn**: [`precombined.rs:103-120`](../../byroredux/src/cell_loader/precombined.rs#L103-L120).
- **FIND-4 / #1218 — CLAUDE.md vs ROADMAP.md parse-rate matrix**:
  reconciled in the 2026-05-23 README/ROADMAP refresh commit
  [`7635c1d0`](../../).
- **FIND-5 / #1219 — NifVariant v20.0.0.4/u11 routing**: resolved via
  sample-data sweep; routing pinned.
- **FIND-6 — dispatch-arm growth** (informational): no further growth
  since 2026-05-19; the +54 spurt has stabilised.

### Spot-checks (verified clean)

- **Dispatch growth is real and matches new content** — re-counted today
  via the same `grep -nE '"[A-Za-z_][^"]*"\s*=>' crates/nif/src/blocks/mod.rs | wc -l`
  pattern; 249 arms, identical to the 2026-05-19 sweep.
- **`BlockRef::index().unwrap()` unsafety**: zero sites today (verified
  yesterday in AUDIT_NIF_2026-05-22_DIM5.md "Verified-Clean List").
- **`NifVersion` coverage** (4.0.0.0 → 20.5.0.4) — no new constants since
  2026-05-19; no TODO / FIXME / XXX comments in `version.rs` or `header.rs`.
- **Byte-budget guards** at header.rs:155-165 / 175-185 / 235-242 — intact.

**Zero NEW findings this sweep.**

---

## Dimension 3 — Transform Compatibility

**Today: 2026-05-23. Prior baseline: AUDIT_LEGACY_COMPAT_2026-05-19.md dim 3 (3 NEW findings — #1220 / #1221 / #1222; all closed).**

### Closures verified

- **#1220 — exterior CELL walker XCRI / XPRI parse**: arms landed at
  [`wrld.rs:342-385`](../../crates/plugin/src/esm/cell/wrld.rs#L342-L385).
  Same shape as the interior walker's
  [`walkers.rs:158-204`](../../crates/plugin/src/esm/cell/walkers.rs#L158-L204).
  `precombined_mesh_hashes` and `absorbed_refs` are now populated for
  exterior cells.
- **#1221 — exterior `spawn_precombined_meshes` call site**: added at
  [`exterior.rs:303-312`](../../byroredux/src/cell_loader/exterior.rs#L303-L312)
  with the matching conditional-absorption gate at
  [`exterior.rs:324-330`](../../byroredux/src/cell_loader/exterior.rs#L324-L330).
- **#1222 — `cell_origin: Vec3` parameter on `spawn_precombined_meshes`**:
  signature widened at [`precombined.rs:63-79`](../../byroredux/src/cell_loader/precombined.rs#L63-L79).
  Exterior caller passes `cell_grid_to_world_yup(gx, gy)` per the audit
  suggestion; interior passes `Vec3::ZERO`.

### Steady-state checks (no findings)

- **Shepperd Matrix3→Quat + normalisation** (#333) — unchanged.
- **Z-up → Y-up basis swap** at the NIF-import boundary — unchanged.
- **BFS world-transform propagation** — unchanged post Session-34/35 split.
- **Skin bind inverses** (#771) — unchanged.
- **CSG reader for `_oc.nif` Shared-variant geometry** — still deferred
  (parent issue #1188); when it lands, the precombined-spawn path is
  ready end-to-end (interior + exterior + cell_origin parameter).

**Zero NEW findings this sweep.**

---

## Dimension 4 — Property → Material Mapping

**Today: 2026-05-23. Prior baseline: AUDIT_LEGACY_COMPAT_2026-05-19.md dim 4 (2 findings — #1223 / #1224, both LOW; both closed).**

### Closures verified

- **#1223 — BSLightingShaderProperty 4-byte under-read on FO4 shared
  precombined NIFs**: the root cause was a duplicate `env_map_scale` read,
  not a Shared-variant trailing field. Gate now at
  [`shader.rs:939-963`](../../crates/nif/src/blocks/shader.rs#L939-L963)
  with the empirical pin (`5211 / 6455` BSLSP at size=140, `1192` at
  size=146) baked into the comment. Starfield (BSVER 168+) corpus
  parse-rate guarded — the prior `FO4_ENV_SCALE = 140` gate that would
  have dropped SF Meshes01 from 97.21% to 95.77% has been replaced with
  the empirically-correct shader_type=1 trailing read.
- **#1224 — NiFogProperty walker dispatch**: deliberately not wired.
  Docstring at [`properties.rs:480-509`](../../crates/nif/src/blocks/properties.rs#L480-L509)
  records the rationale; the audit-skill body itself carves out this
  case (per the skill text passed in this invocation: "Do not re-file as
  a finding — see walker.rs near the end of extract_material_info for
  the deliberate-skip comment"). The corresponding walker comment is at
  [`walker.rs:924-934`](../../crates/nif/src/import/material/walker.rs#L924-L934).

### Steady-state checks (no findings)

- **17 dispatched property types** in the material walker — unchanged
  surface (BSLightingShaderProperty, BSEffectShaderProperty,
  BSSkyShaderProperty, BSWaterShaderProperty, NiAlphaProperty,
  NiZBufferProperty, NiMaterialProperty, NiTexturingProperty,
  BSShaderPPLightingProperty, BSShaderNoLightingProperty,
  TileShaderProperty, SkyShaderProperty, TallGrassShaderProperty,
  NiStencilProperty, NiFlagProperty cluster (Specular / Wireframe /
  Shade / Dither), NiVertexColorProperty).
- **8 NiTexturingProperty slots** (Base, Dark, Detail, Gloss, Glow,
  Bump, Decal 0/1/2/3) — wired post-#382.
- **TXST slot routing on skin/hair tint** — intact post-#563.
- **WetnessParams BSVER >= 130 gate** (#403) — intact.
- **FO76 LuminanceParams + TranslucencyParams BSVER >= 155 gate** (#746)
  — intact.

**Zero NEW findings this sweep.**

---

## Dimension 5 — Animation Readiness

**Today: 2026-05-23. Prior baseline: AUDIT_LEGACY_COMPAT_2026-05-19.md dim 5 (steady-state).**

The NIF dispatch covers **43 controller + interpolator arms** (counted
today via `grep -nE '"(Ni[A-Za-z]*(Controller|Interpolator)|BS[A-Za-z]*(Controller|Interpolator))"\s*=>' crates/nif/src/blocks/mod.rs | wc -l`).
The animation runtime (`AnimationPlayer`, `AnimationStack`, `advance_time`,
`advance_stack`, `sample_blended_transform`, `split_root_motion`,
`visit_text_key_events`) is unchanged in shape since 2026-05-07.

### Verified-clean checks

- **Cell-loader animation binding** (#544): per-placement `AnimationPlayer`
  spawned with `root_entity = Some(placement_root)`; subtree name map
  binds through `Name` rows seeded at spawn.
- **Embedded clip discovery** (`references.rs:885-904`): clips captured
  on `CachedNifImport.embedded_clip` survive the registry LRU.
- **B-spline path on FNV / FO3** (per `feedback_bspline_not_skyrim_only`):
  dispatch at `mod.rs:771-806` is unconditional on version.
- **NiBlendTransform/Float/BoolInterpolator** consumers: present at
  [`controlled_block.rs:87-95`](../../crates/nif/src/anim/controlled_block.rs#L87-L95).
- **NiFlipController** consumed at
  [`sequence.rs:95-110`](../../crates/nif/src/anim/sequence.rs#L95-L110)
  and [`entry.rs:204-205`](../../crates/nif/src/anim/entry.rs#L204-L205) /
  [`:376-383`](../../crates/nif/src/anim/entry.rs#L376-L383).

### Known deferred orphan-parse cases (NOT raised)

`BsTreadTransfInterpolator` and `NiBsBoneLodController` parse cleanly
but have no animation-runtime consumer today. Both are covered under the
closed meta-issue [#974](https://github.com/matiaszanolli/ByroRedux/issues/974)
(Band B bucket [#986](https://github.com/matiaszanolli/ByroRedux/issues/986),
closed with explicit "stay deferred — each is blocked on a downstream
subsystem not yet built"). Re-raising would be a stale finding per
[Audit Finding Hygiene](../../../../home/matias/.claude/projects/-mnt-data-src-gamebyro-redux/memory/feedback_audit_findings.md).

**Zero NEW findings this sweep.**

---

## Dimension 6 — String Interning Alignment

**Today: 2026-05-23. Prior baseline: AUDIT_LEGACY_COMPAT_2026-05-19.md dim 6 (steady-state).**

`StringPool` semantics (case-folding on intern, integer-equality on
`FixedString`) remain aligned with Gamebryo's `NiFixedString` +
`NiGlobalStringTable`. The stack-buffer fast-path landed via
[`c43e7405`](../../) (#893) — `intern` / `get` lowercase via a 256-byte
stack scratch with heap fallback for longer inputs.

The case-loss-on-resolve documentation closure (#895) is intact:
[`crates/core/src/string/mod.rs:72-78`](../../crates/core/src/string/mod.rs#L72-L78)
notes "returns the lowercased canonical form — the case the caller
originally passed to `StringPool::intern` is *not* preserved" (the
informational note from the 2026-05-19 sweep, now codified).

### Verified-clean checks

- **Symbol stability across pool growth**: `string_interner::backend::StringBackend`
  is slab-style; symbols are slot indices, never reassigned.
- **Lock hoisting at the cell-loader spawn site** (#882): single
  `world.resource_mut::<StringPool>()` per spawn pass, resolves all
  texture-slot paths + mesh names in one write-lock scope.
- **Animation subtree name lookup** (`build_subtree_name_map`): walks
  `Parent → Children` BFS, matches against `Name(FixedString)` rows
  via integer-equality on the symbol.

**Zero NEW findings this sweep.**

---

## Summary

| Dimension | Findings | Status |
|---|---|---|
| 1 — Scene Graph Decomposition | 1 NEW (LOW) | D1-NEW-01 SceneFlags parity |
| 2 — NIF Format Readiness | 0 | Steady-state; all 2026-05-19 closures verified |
| 3 — Transform Compatibility | 0 | Steady-state; #1220/#1221/#1222 closures verified |
| 4 — Property → Material Mapping | 0 | Steady-state; #1223/#1224 closures verified |
| 5 — Animation Readiness | 0 | Steady-state; orphan-parse meta closed (#974) |
| 6 — String Interning | 0 | Steady-state; #893/#895 closures intact |

**Headline:** The five-day window cleared every NEW finding from the
prior sweep. The core legacy-compatibility surface (NIF parser, animation
runtime, transform math, property→material walker, string-interning
layer) is in steady-state. The sole NEW finding is a parity gap —
`SceneFlags` is inserted by the loose-NIF loader (#222) but never by the
cell-loader spawn path, the last sibling of the D1-NEW-01..03 cluster
not picked up by the 2026-05-19 closure. Two-line fix in `spawn.rs`;
publish via `/audit-publish`.
