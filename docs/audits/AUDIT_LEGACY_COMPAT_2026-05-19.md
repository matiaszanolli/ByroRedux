# Legacy Compatibility Audit — 2026-05-19

**Scope:** Full sweep, dimensions 1–6 per `/audit-legacy-compat`.

**Predecessors:**
- [AUDIT_LEGACY_COMPAT_2026-05-07.md](AUDIT_LEGACY_COMPAT_2026-05-07.md) (dims 2 + 3 spot-audit).
- [AUDIT_LEGACY_COMPAT_2026-05-07_DIM6.md](AUDIT_LEGACY_COMPAT_2026-05-07_DIM6.md) (dim 6 spot-audit).
- [AUDIT_LEGACY_COMPAT_2026-04-30.md](AUDIT_LEGACY_COMPAT_2026-04-30.md) (last full sweep).

---

## Executive Summary

This audit lands on the heels of today's #1188 (FO4 PreCombined Mesh fallback,
commit `eeddc81b`) — the post-mortem at
[POST_MORTEM_2026-05-19_PRECOMBINED.md](POST_MORTEM_2026-05-19_PRECOMBINED.md)
already flagged the audit-side miss that allowed Diamond City Dugout Inn to
render as "props floating in a void." This sweep surfaces the **second leg of
the same gap on the exterior path** (dim 3, HIGH) plus several spawn-site
plumbing issues that have been silently degrading downstream subsystems
(dim 1, HIGH) — none of which were on prior audits' radar.

The NIF parser, animation runtime, transform math, property→material walker,
and string-interning layer are all in steady-state. The gaps are concentrated
at the **cell-loader spawn boundary** (D1-*) and the **exterior CELL walker**
(D3-NEW-01 / D3-NEW-02).

### Severity rollup

| Severity | Count | Dims (where) |
|----------|-------|--------------|
| CRITICAL | 0     | — |
| HIGH     | 4     | D1-NEW-01, D1-NEW-02, D3-NEW-01, D3-NEW-02 |
| MEDIUM   | 1     | D1-NEW-03 |
| LOW      | 9     | D3-NEW-03, D4-NEW-01, D4-NEW-02, FIND-1..6 (dim 2) |
| Steady-state | dims 5, 6 | zero new findings |

### Recommended next steps

1. **D3-NEW-01** — lift XCRI/XPRI parsing into the exterior CELL walker.
   Single-file change in `crates/plugin/src/esm/cell/wrld.rs` mirroring
   `walkers.rs:158-204`. Fast follow-up commit alongside today's #1188.
2. **D1-NEW-01 + D1-NEW-02** — attach `FormIdComponent` + `LocalBound` at
   `spawn_placed_instances`. Two-line spawn-site change; transitively
   unlocks `prid` console command + future culling + RT budget refinement.
3. **D1-NEW-03** — plumb `BSXFlags` through the cached import. Bounded.
4. **D2 FIND-1 / FIND-2 / FIND-3** — observability one-liners around the
   silent-zero-mesh path that cost hours during today's Dugout Inn
   debugging session.

Suggested publish: `/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-05-19.md`

---

## Dimension 1 — Scene Graph Decomposition

**Today: 2026-05-19. Prior baseline: AUDIT_LEGACY_COMPAT_2026-04-30.md dim 1.**

NiAVObject's "static" fields (translation, rotation, scale, parent edge) decompose
cleanly into `Transform` / `Parent` / `Children` / `GlobalTransform`, all of which
the cell-loader spawn site does populate. The conversion math itself is steady-state.

What's **not** populated at the spawn site is a sibling set of components that exist
in `crates/core/src/ecs/components/` and that callers (debug console, bounds
propagation, RT light/shadow gating) expect to read. Today's #1188 added
`PlacedRef.form_id` distinct from `base_form_id`, but the spawn site doesn't carry
the placement form-id into the ECS either. Three findings, all NEW.

---

### D1-NEW-01: `FormIdComponent` never attached to cell-loaded REFR mesh entities

- **Severity**: HIGH
- **Dimension**: Scene Graph Decomposition
- **Location**: [byroredux/src/cell_loader/spawn.rs](../../byroredux/src/cell_loader/spawn.rs) — entire `spawn_placed_instances` body
- **Status**: NEW
- **Description**:
  `FormIdComponent` exists at `crates/core/src/ecs/components/form_id.rs` (re-exported
  from `mod.rs:39`) and is the backing storage for `World::find_by_form_id` (queries
  it as a SparseSet). Console commands like `prid <fid>` and the debug-server reach
  for it. Today the cell-loader **never inserts it** on any spawned mesh /
  placement-root / light / particle / collision entity:

  ```
  $ grep -rn "FormIdComponent" byroredux/src/
  (no results)
  ```

  Only `crates/core/src/ecs/world_tests.rs` exercises the component end-to-end.
  Production has unit tests but no integration.

  #1188 widened `PlacedRef` to carry both `form_id` (placement identity) and
  `base_form_id` (referenced base record), but the spawn site never reads either
  into an ECS row — both stay parser-local and die at the function boundary.

- **Evidence**:
  - `spawn_placed_instances:142-153` inserts Transform / GlobalTransform / Billboard
    on the placement root. No `FormIdComponent`.
  - `spawn_placed_instances:636-846` inserts MeshHandle / TextureHandle / Material /
    NormalMapHandle / DarkMapHandle / ExtraTextureMaps / AlphaBlend / TwoSided /
    RenderLayer / Name on each mesh entity. No `FormIdComponent`.
  - `references.rs:188-237` reads `placed_ref.form_id` and `placed_ref.base_form_id`
    for the absorption check + REFR loop but doesn't pass them into spawn.

- **Impact**:
  - `World::find_by_form_id(fid)` returns `None` for every REFR loaded by the cell
    loader.
  - `prid <fid>` console command is dead on cell-loaded content.
  - The debug-server's "inspect by formid" path is dead.
  - Any future Papyrus-script ECS adapter that resolves a script `ObjectReference`
    by formid hits the same dead path.
  - Quest / story-manager systems that fire on `OnActivate(<fid>)` markers can't
    locate the target entity.

- **Suggested Fix**:
  Insert `FormIdComponent(placed_ref.form_id)` on the placement root in
  `spawn_placed_instances`. Pass `placed_ref` (or just the two form-ids) through
  the call signature from `load_references`. Optional: also attach on the
  first mesh entity for debug-server lookup ergonomics, but the placement-root
  is the canonical anchor.

---

### D1-NEW-02: `LocalBound` / `WorldBound` never seeded for cell-loaded mesh entities

- **Severity**: HIGH
- **Dimension**: Scene Graph Decomposition
- **Location**: [byroredux/src/cell_loader/spawn.rs:636-846](../../byroredux/src/cell_loader/spawn.rs), bounds propagation at [byroredux/src/systems/bounds.rs:1-72](../../byroredux/src/systems/bounds.rs)
- **Status**: NEW
- **Description**:
  `LocalBound` and `WorldBound` exist in `crates/core/src/ecs/components/`
  (re-exported from `mod.rs:46,60`) and the bounds propagation system
  documents itself as:

  > **Leaf bounds** — for every entity with a `LocalBound` (set at import
  > time), project the local sphere into world space via `GlobalTransform`,
  > store as `WorldBound`.

  But **no import-time site sets `LocalBound`**:

  ```
  $ grep -rn "LocalBound::new\|insert.*LocalBound" byroredux/src/ crates/nif/src/
  byroredux/src/systems/bounds.rs:188:        world.insert(e, LocalBound::new(...))  — test fixture only
  ```

  `ImportedMesh` from the NIF importer carries `local_bound_radius` per shape
  (used at `spawn.rs:844` for the small-STAT escalation heuristic), but the
  spawn loop never converts that into a `LocalBound` component row.

- **Evidence**:
  - `bounds.rs:43-66` iterates `world.query::<LocalBound>()` and computes
    `WorldBound` from it. On a fresh cell load the query is empty.
  - `bounds.rs:127-153` Pass 2 (interior nodes) merges children's `WorldBound`
    via `WorldBound::merge`. Empty leaves → empty interiors.
  - Every renderer / RT subsystem that wants to cull or sort by bounds reads
    `WorldBound` (e.g. `byroredux/src/render.rs` per-frame data collection
    enumerates draws); they get the component-default zero sphere.

- **Impact**:
  - **Culling**: any future frustum / portal / occlusion cull that reads
    `WorldBound` sees zero-radius spheres → either everything passes (no cull)
    or everything fails (everything invisible) depending on the test direction.
    Today: the renderer falls through to a "draw all" path that masks the gap.
  - **RT shadow / GI budgeting**: importance-sorted shadow budget (#270) and
    distance-based ray fallback (#271) use camera-relative bounds. With no
    `WorldBound` they fall back to entity position only — coarser approximation,
    wasted ray budget on far entities.
  - **CellRoot bound aggregation** (the comment in `byroredux/src/cell_loader/load.rs:196-199`
    notes "the cell's reference bounds are not yet aggregated at this site"):
    the water-plane centering, the LOD selector, and the audit-flagged "cell
    bounding sphere for culling" path all stay stubbed until this lands.

- **Suggested Fix**:
  At `spawn.rs:639-647` insert `LocalBound::new(mesh.local_center,
  mesh.local_bound_radius)` alongside the Transform / GlobalTransform pair.
  The bounds-propagation system already handles the rest. Add an integration
  test that loads a fixture cell and asserts at least one `WorldBound` row
  exists post-load.

---

### D1-NEW-03: `BSXFlags` parsed but dropped at spawn boundary

- **Severity**: MEDIUM
- **Dimension**: Scene Graph Decomposition
- **Location**: [crates/nif/src/import/](../../crates/nif/src/import/) → [byroredux/src/cell_loader/spawn.rs](../../byroredux/src/cell_loader/spawn.rs)
- **Status**: NEW
- **Description**:
  `BSXFlags` is a Gamebryo extra-data block that flags the entire NIF (havok-managed,
  ragdoll, editor-marker, articulated, externally-emitted-particles, etc.). The
  parser reads it via `byroredux_nif::import::extract_bsx_flags(&scene)` (used at
  `references.rs:840` to filter editor-marker NIFs by bit 5). Beyond that filter,
  the bits never reach the ECS: there's no `BSXFlags(u32)` component row on the
  spawned entities. Decisions like "this entity needs a havok body" or "this entity
  is articulated and needs its own AnimationPlayer subtree wiring" go through
  ad-hoc heuristics elsewhere instead of the authoritative BSX bits.

- **Evidence**:
  - `crates/core/src/ecs/components/bsx.rs` exports `BSBound` and `BSXFlags`
    (re-export at `mod.rs:35`) but the latter is unused outside the component
    crate's own tests.
  - `spawn.rs` has no insert call for it.

- **Impact**:
  Future havok / ragdoll integration (M28 phase 3+), articulated-mesh
  animation wiring, and per-NIF debug introspection (`mesh.info` console command)
  all re-derive what BSX already authoritatively says. Today only the
  editor-marker bit is honoured (and only inside the cell-load decision path,
  not as a component).

- **Suggested Fix**:
  Pass `bsx_flags: u32` through `CachedNifImport` so the spawn site can attach
  `BSXFlags(bits)` to the placement root. Audit downstream consumers that
  currently sniff for havok / ragdoll via heuristics and route them through
  the bit checks.

---

### Steady-state checks (no findings)

- **Transform / Parent / Children / GlobalTransform**: all populated correctly
  by `spawn_placed_instances` post-#544 (placement-root + parent-edge convention).
  No regression since 2026-04-30.
- **Name**: interned via the single-lock pre-pass (#882). Animation subtree
  walker (`build_subtree_name_map`) finds it.
- **MeshHandle / TextureHandle / Material / AlphaBlend / TwoSided / NormalMapHandle /
  DarkMapHandle / ExtraTextureMaps / IsFxMesh / RenderLayer**: all present.
- **LightSource / Billboard / ParticleEmitter / collision body+shape**: present
  on their respective entities.
- **NiAVObject::Flags bit 0 (APP_CULLED)**: honoured as early-return at
  `walk/mod.rs:341,385` (NiTriShape and BsTriShape paths). Other bits
  (SELECTIVE_UPDATE = 0x02, IS_FOCUS = 0x04, etc.) are read into
  `ImportedNode.flags` but not surfaced on any ECS row — flagged within D1-NEW-03's
  scope.

---

**Summary**: dim 1 surfaced 3 findings, all NEW, all about spawn-site
component coverage. The conversion math and core component set are correct;
the gap is plumbing — parsed data that doesn't reach the ECS. D1-NEW-01 and
D1-NEW-02 are HIGH because they break console / debug-server / culling /
RT budget paths; D1-NEW-03 is MEDIUM (forward-looking havok integration).

---

## Dimension 2 — NIF Format Readiness (2026-05-19)

Specialist: legacy-engine-specialist
Branch: main · audit date: 2026-05-19

## Methodology

- Counted dispatch arms via `grep -nE '"\w[^"]*"\s*=>' crates/nif/src/blocks/mod.rs` (string-literal arm form).
- Read full `version.rs` (749 LOC) + `header.rs` (615 LOC) for version-gate coverage and TODO markers.
- Grepped every `BlockRef::index()` use site for unsafe slice indexing / panics.
- Cross-checked the prior 2026-05-07 audit's "195 arms" claim, the parse-rate matrix on CLAUDE.md / ROADMAP.md, and #1188 (today) for the `_oc.nif` zero-vertex CSG case.
- Inspected `parse_and_import_nif` + `spawn_placed_instances` + walker for empty-`meshes` observability.

## Headline Numbers

| Metric                                   | Today (2026-05-19) | Prior (2026-05-07) | Delta |
|------------------------------------------|--------------------|--------------------|-------|
| Dispatch arms in `blocks/mod.rs`         | **249**            | 195                | **+54** |
| Version constants on `NifVersion`        | 27                 | 23                 | +4    |
| `NifVariant` feature predicates          | 9                  | 21 (dead removed)  | -12 (#938 cull) |
| `BlockRef::index()` use sites            | 143                | n/a                | —     |
| `BlockRef::index().unwrap()` sites       | **0**              | 0                  | clean |

## Findings

---

### FIND-1 — NEW · LOW · `meshes.is_empty()` is silently passed through `parse_and_import_nif` → empty `CachedNifImport`

**Location:** `/mnt/data/src/gamebyro-redux/byroredux/src/cell_loader/references.rs:812-890`

**Evidence.** `parse_and_import_nif` returns `Some(Arc::new(CachedNifImport { meshes, ... }))` unconditionally when the underlying `parse_nif` succeeds — even when `import_nif_with_collision_and_resolver` produces an empty `meshes` Vec. The only diagnostic on the path is `log::warn!` for the truncation case and `log::debug!("Skipping editor marker NIF")` for BSX 0x20. The cached entry is then committed to the `NifImportRegistry` and handed to `spawn_placed_instances`, which iterates `cached.meshes` and silently produces zero mesh entities while still spawning the placement-root + lights + particle emitters.

For the Diamond City Dugout Inn case (#1188, today's commit), this means a `_oc.nif` whose every `BSTriShape` has `num_vertices=0` (Shared variant, CSG-deferred) returns a `CachedNifImport { meshes: [], collisions: [], lights: [], particle_emitters: [], embedded_clip: None, .. }` and the operator gets **no log at all** that the file produced no geometry.

The audit's premise is real: the only path that surfaces an empty-mesh outcome today is the `pc_spawned == 0` gate in `load.rs:172` that flips the absorption set off. Anywhere else (loose NIFs, per-REFR architecture) the silent-zero-mesh case is invisible.

**Why this matters.** The 2026-05-19 Dugout Inn debugging session required adding diagnostic logs to discover the zero-vertex condition — exactly the failure mode the audit calls out. The `_oc.nif` shared variant will keep producing zero-mesh imports until a CSG reader lands; until then operators need an out-of-the-box signal.

**Fix.** At the end of `parse_and_import_nif`, when `meshes.is_empty() && collisions.is_empty() && lights.is_empty() && particle_emitters.is_empty() && embedded_clip.is_none()`, emit:

```rust
log::warn!(
    "NIF '{}' imported with zero meshes / collisions / lights / emitters / clips \
     — likely CSG-deferred (`_oc.nif` Shared variant, #1188) or pure marker scene",
    label,
);
```

The cell-loader can keep returning `Some(Arc::new(...))` so cache invariants don't change; only the observability gap closes.

**Severity:** LOW (defense-in-depth; doesn't cause incorrect rendering, just makes future debugging painful — Dugout Inn took multiple sessions because this log didn't exist).

---

### FIND-2 — NEW · LOW · No dispatch-arm-side warn for `BSTriShape { num_vertices: 0 }` when not in a `BSPackedCombinedSharedGeomDataExtra` host

**Location:** `/mnt/data/src/gamebyro-redux/crates/nif/src/blocks/tri_shape/bs_tri_shape.rs:316-360`

**Evidence.** The parser already gates the `data_size != expected_data_size` warning on `num_vertices != 0` (line 328, #836 / SK-D5-NEW-02) because SSE skinned bodies legitimately ship `num_vertices=0` (data lives on a sister `NiSkinPartition` per #559). That gate is correct.

But there is no symmetric warning for the **opposite** case: `num_vertices == 0` outside of (a) the SSE-skinned-reconstruction path AND (b) the FO4-precombined Shared-variant path. Either the file is a CSG-only shape that needs a reader we haven't shipped, or it's a malformed shape — and we can't tell from the parser side.

The walker (`import/walk/mod.rs:50-65`) skips the whole subtree when a `BSPackedCombinedGeomDataExtra` extra-data ref resolves under the host NiNode — so the Shared case is identifiable at walk time. But the BSTriShape parser itself doesn't carry the host-context bit and so can't differentiate at parse time. The minimum surface area today: tag the dispatch arm or the importer to count "CSG-deferred zero-vertex shapes" vs "unknown zero-vertex shapes" per `nif_stats` run.

**Fix.** Add a `tracing` counter in `BsTriShape::parse` when `num_vertices == 0 && data_size == 0 && (vertex_attrs & VF_SKINNED) == 0`, surfaced through `nif_stats` so the parse harness reports `zero_vertex_bs_tri_shape_count` per archive. The skinned-SSE path already early-returns by emptying the inline arrays cleanly; this counter would distinguish "Shared CSG content" from "true bug" without changing parse behaviour.

**Severity:** LOW (purely observability; the parse result is already correct).

---

### FIND-3 — NEW · LOW · `CachedNifImport` cache stores zero-mesh imports — first cache hit looks identical to a successful import

**Location:** `/mnt/data/src/gamebyro-redux/byroredux/src/cell_loader/precombined.rs:128-132`, `references.rs:802-810`

**Evidence.** When a `_oc.nif` imports to zero meshes (CSG case), `parse_and_import_nif` returns `Some(arc)` and the precombined loader inserts that arc into `NifImportRegistry`. Subsequent cells in the same load order that reference the same hash get the cached zero-mesh entry on the cache-hit path (precombined.rs:85-88) and re-skip silently — they never even hit the `parse_and_import_nif` log site, so a single `warn!` from FIND-1 only fires once per process per `_oc.nif` path.

This is not strictly a bug (cache is content-addressed by `path` and the on-disk file genuinely has zero meshes), but it weakens FIND-1's fix: the warn would fire only on first-touch and a long session loading 200 FO4 cells would still emit only ~50 lines total even though every cell silently dropped its precombines.

**Fix.** When the cache **hits** on a zero-mesh `CachedNifImport`, fire `log::debug!` (not `warn`) at the `precombined::spawn_precombined_meshes` call site so the operator can see post-mortem how many cells took the CSG-deferred fallback. Pair with `pc_spawned` count already logged at info.

**Severity:** LOW (depends on FIND-1 landing first).

---

### FIND-4 — NEW · LOW · Parse-rate matrix in CLAUDE.md is stale vs ROADMAP.md (Oblivion 95.21% vs 96.24%)

**Location:** `/mnt/data/src/gamebyro-redux/CLAUDE.md:288` vs `/mnt/data/src/gamebyro-redux/ROADMAP.md:668`

**Evidence.** CLAUDE.md line 287-289 cites the 2026-04-26 sweep: Oblivion 95.21%, FO4 96.46%, FO76 97.34%, Starfield 97.19%. ROADMAP.md line 668 (the "Project Stats" ground-truth section) cites a different sweep date: Oblivion 96.24%, FO4 96.46%, FO76 97.34%, Starfield 98.6% aggregate.

FO4 and FO76 agree; Oblivion drifts by ~1pp, Starfield by ~1.4pp. Since ROADMAP.md is explicitly designated by CLAUDE.md as "ground truth on test/file/LOC/crate counts (refreshed each /session-close)", the CLAUDE.md numbers should track. Audit-side impact: future readers ask "which number do I cite?" — this is the [Audit Finding Hygiene](feedback_audit_findings.md) failure mode where a stale rate becomes a false-positive premise for a "regression" finding.

**Spot check #5 (per audit spec):** the spirit of the claim ("100% on FO3/FNV/Skyrim SE, ~95-97% on Oblivion/FO4/FO76/Starfield") still holds — both documents agree on the ceiling games and both cluster the rough-edge games in the 95-97% band. **No regression of #1185** (which calls out a different stale claim — Starfield BA2 count).

**Fix.** Refresh CLAUDE.md from ROADMAP.md or pick a single authoritative number per game. Probably a `/session-close` followup; not a code change.

**Severity:** LOW (documentation drift).

---

### FIND-5 — NEW · LOW · `NifVariant::detect` ambiguous `(V20_0_0_4, user=11, _)` routing left as Oblivion without sample data

**Location:** `/mnt/data/src/gamebyro-redux/crates/nif/src/version.rs:294-308`

**Evidence.** `NifVariant::detect` has a tagged-but-unresolved fork at the `(V20_0_0_4, user_version=11, _)` boundary. nif.xml line 196 lists v20.0.0.4 as "Oblivion, Fallout 3" — genuinely ambiguous — and nif.xml's `#FO3#` verset (line 44) explicitly includes `V20_0_0_4__11`. The current code returns `Oblivion` (line 307); the comment at line 297-306 acknowledges this is pinned by a test (`detect_oblivion_edge_cases`, line 563) rather than by sample data.

The impact is bounded — no retail FO3 NIF ships at v20.0.0.4 (retail FO3 is `(V20_2_0_7, user=11, uv2=34)`) — so this only bites pre-release / mod content. But the comment is honest about needing sample data and no follow-up exists.

**Fix.** Either (a) settle the routing with a sample-data sweep against any FO3 mod corpus that ships v20.0.0.4 NIFs, or (b) downgrade to `Unknown` with a one-shot `log::warn!("ambiguous v20.0.0.4/u11 — routed as Oblivion; please file with sample")`. Probably (a); deferred until someone actually sees the case in the wild.

**Severity:** LOW (no in-the-wild content exercises this today).

---

### FIND-6 — NEW · LOW · Dispatch arm count grew +54 since last audit (195 → 249) — prior audit's "all OPEN closed by #771/#772/#606/#607/#608" doctrine needs a refresh pass

**Location:** `/mnt/data/src/gamebyro-redux/crates/nif/src/blocks/mod.rs` (whole file)

**Evidence.** Spot count of the dispatch arms today: 249 string-literal `"…" => …` arms (grep regex on the match block between lines 207 and 1117). Prior audit (2026-05-07, 12 days ago) reported 195. Delta = +54 arms, ~28% growth in 12 days.

Spot-sampled new arms include:
- `BSPackedCombinedSharedGeomDataExtra` (line 614, shared with the existing Geom variant)
- `BSPackedAdditionalGeometryData` (line 404)
- `BSMeshLODTriShape` (line 352)
- `bhkOrientHingedBodyAction`, `bhkLiquidAction`, `bhkConvexListShape`, `bhkBreakableConstraint`, `bhkRagdollTemplate`, `bhkPoseArray`, `bhkPCollisionObject`, `bhkPhysicsSystem`, `bhkRagdollSystem`, `bhkNPCollisionObject` (lines 1083-1117 cluster)
- The full `NiPSys*FieldModifier` family (lines 957-972)
- `NiBSplineCompFloatInterpolator` / `…CompPoint3Interpolator` / `…TransformInterpolator` / `…Data` / `…BasisData` (lines 780-808)

The new arms are real coverage gains — none are obvious regressions of #771 / #772 / #606 / #607 / #608. **No issue raised**, but the prior audit's "verified working — no gaps" stance on this dimension is now 12 days stale and the next refresh should re-sample the unknown-block histogram against the live archives (the `unknown_types.rs` example).

**Severity:** LOW (informational; the growth is healthy and expected).

---

### FIND-7 — Existing: #1188 (today) — `_oc.nif` Shared-variant geometry deferred to CSG, no reader yet

**Location:** `/mnt/data/src/gamebyro-redux/byroredux/src/cell_loader/precombined.rs:11-21`, `crates/nif/src/blocks/extra_data.rs:865-923`

**Evidence.** #1188 itself documents this; the dispatch arm exists (line 614 — `BSPackedCombinedGeomDataExtra | BSPackedCombinedSharedGeomDataExtra`), the walker correctly skips host subtrees with this extra-data, and the load.rs absorption gate correctly falls back to per-REFR rendering when `pc_spawned == 0`. The CSG reader itself is a future milestone, **not** part of this audit's scope.

What is in scope and uncovered: observability (FIND-1, FIND-2, FIND-3 above). Linking #1188 here for traceability rather than re-raising.

**Severity:** N/A (parent issue; linked for cross-ref).

---

## Re-checks Where Nothing Was Found (Verified Clean)

- **Dispatch growth is real and matches new content** — no orphaned arms, no duplicates, no name-collision races with the `impl_ni_object!` macro forms.
- **Version range coverage** — `NifVersion` covers v4.0.0.0 (Civ IV / Morrowind era) through v20.5.0.4 (Gamebryo 3.x SDK). All 27 named constants are in monotonic order and every comment cites either nif.xml or a specific issue (#724, #170, #171, #388, #408, #876, #934, #935, #937, #938). **No TODO / FIXME / XXX comments** in either `header.rs` or `version.rs`.
- **BSStreamHeader gate** (header.rs:113-119) matches nif.xml `#BSSTREAMHEADER#` exactly per the #170 fix.
- **String-table threshold** (`STRING_TABLE_THRESHOLD = V20_1_0_1`, version.rs:143) is documented as MUST-stay-in-lockstep with `stream.rs::NifStream`, with the call-site reference baked into the doc comment.
- **`BlockRef::index()` unwrap safety** — 143 use sites grepped, **zero** `.unwrap()` / `.expect()` / `[…]` direct-index patterns. The only consumers that walk `.index()` are `walk/mod.rs:140,148` (collected via `Option::into_iter`, panic-safe) and `scene.blocks.get(idx)` patterns. The "silent panic / out-of-range path" the audit asked about does not exist.
- **`NifVariant::bsver()`** hard-pinned per nif.xml retail values; `bsver_values` test (version.rs:683-694) covers the full enum surface including `Unknown -> 0`.
- **Header byte-budget guards** (#388 / #408) — `num_blocks`, `num_block_types`, `num_strings` all bound-checked against remaining file bytes before allocation, so a corrupt u32 in the header can't OOM the parser. Verified at header.rs:155-165, 175-185, 235-242.

## Summary

| Severity | Count | Disposition |
|----------|-------|-------------|
| CRITICAL | 0     | — |
| HIGH     | 0     | — |
| MEDIUM   | 0     | — |
| LOW      | 6     | 5 NEW + 1 cross-link to #1188 |

**Headline:** The NIF parser itself is in excellent shape — dispatch coverage grew +54 arms in 12 days with no regressions, version coverage is fully gated against nif.xml, `BlockRef` resolution has zero panic surfaces, and the byte-budget guards harden the header against corrupt-u32 OOM. The only audit-surfaced gaps are **observability** around the FO4 `_oc.nif` Shared-variant CSG case (FIND-1, FIND-2, FIND-3) and minor doc drift (FIND-4, FIND-5). Recommend landing FIND-1's one-line `warn!` first — it's the smallest fix that would have shortened today's Dugout Inn investigation by hours.

---

## Dimension 3 — Transform Compatibility

**Today: 2026-05-19. Prior baseline: AUDIT_LEGACY_COMPAT_2026-05-07.md (steady-state — Shepperd Matrix3→Quat + post-extraction normalisation per #333; Z-up→Y-up swap per #232; BFS world-transform propagation per #825).**

The conversion math itself remains steady-state. All OPEN issues from the prior audit
(#841, #532, #839) are unchanged and not re-investigated here. **Today's #1188 (eeddc81b)
landed the FO4 PreCombined Mesh pipeline and surfaced a transform-related gap on the
exterior path that is new to this sweep.**

---

### D3-NEW-01: Exterior CELL walker hardcodes empty XCRI/XPRI on a wrong premise

- **Severity**: HIGH
- **Dimension**: Transform Compatibility (cross-cuts with #1188's scope)
- **Location**: [crates/plugin/src/esm/cell/wrld.rs:354-361](../../crates/plugin/src/esm/cell/wrld.rs#L354-L361)
- **Status**: NEW (regression of #1188's incomplete scope, not yet filed)
- **Description**:
  The exterior CELL walker in `wrld.rs` initialises both new precombined fields
  to empty with a code comment that reads:
  ```rust
  // Exterior cells don't author XCRI / XPRI
  // (FO4 precombines are interior-only in
  // vanilla; mod content rare). Empty here
  // is safe — the cell loader skips the
  // precombined-spawn step when both are
  // empty. #1188.
  precombined_mesh_hashes: Vec::new(),
  absorbed_refs: std::collections::HashSet::new(),
  ```
  This premise is **factually wrong**. FO4's PreCombined Mesh system was designed
  primarily for **exterior** cells — Commonwealth open-world tiles (Concord, Sanctuary
  Hills, Boston downtown, Diamond City Marketplace) ship per-tile precombined NIFs
  that bake the full architectural facade into a single asset; this is the FO4
  performance headline feature documented as "Previs+PreCombined" in CK docs.
  `Fallout4 - MeshesExtra.ba2` ships 124,871 `_oc.nif` files — far more than the
  vanilla interior count.

- **Evidence**:
  - The interior walker in `crates/plugin/src/esm/cell/walkers.rs:158-204` correctly
    parses XCRI / XPRI sub-records on interior CELL records.
  - The exterior walker in `crates/plugin/src/esm/cell/wrld.rs:198-371` iterates
    sub-records (CRGB, RGBM, XGLB, XCAS, XCWT, etc.) but never matches `b"XCRI"`
    or `b"XPRI"`. It hardcodes both to empty at the construct site.
  - File count: `Fallout4 - MeshesExtra.ba2` contains 124,871 `_oc.nif` entries
    (probed 2026-05-19). Interior cells number in the low thousands at most;
    the vast majority of those NIFs are exterior tile precombines.
  - Without the parse, the `pc_spawned == 0` fallback in
    [byroredux/src/cell_loader/load.rs:170-176](../../byroredux/src/cell_loader/load.rs)
    is moot for exterior — there is nothing to fall back from. Today the exterior
    loader renders all REFRs unconditionally (correct end-state by accident), but
    when the CSG reader lands the parser-side gap means we'd never invoke the
    precombined-spawn path on the cells where it matters most.
- **Impact**:
  Today: silent under-coverage of the XPRI absorption set — no functional regression
  because precombined-spawn returns 0 anyway. Tomorrow (CSG-reader milestone): every
  Commonwealth exterior cell would skip the optimisation and pay the per-REFR draw
  cost (hundreds of architecture pieces × thousands of cells = the headline FO4
  performance feature missing).
- **Related**: #1188 (today's commit). The post-mortem
  `docs/audits/POST_MORTEM_2026-05-19_PRECOMBINED.md` should be updated to flag
  the exterior parse-side gap as the second leg of the same audit miss.
- **Suggested Fix**:
  Lift the XCRI / XPRI sub-record arms from `walkers.rs:158-204` into
  `wrld.rs`'s sub-record loop and assign the parsed values to the construct
  fields. Mirror the interior path's `mesh_count + ref_count` header decode and
  `n × u32` tail. Add a regression test against a known-precombined-bearing
  Commonwealth exterior cell.

---

### D3-NEW-02: Exterior cell-loader call site for `spawn_precombined_meshes` doesn't exist

- **Severity**: HIGH (forward-looking — blocks CSG-reader milestone)
- **Dimension**: Transform Compatibility (call-site placement)
- **Location**: [byroredux/src/cell_loader/exterior.rs](../../byroredux/src/cell_loader/exterior.rs)
- **Status**: NEW
- **Description**:
  Today's #1188 added the precombined-spawn call to the interior loader only
  (`byroredux/src/cell_loader/load.rs:152-159`). The exterior loader passes
  `&cell.absorbed_refs` to `load_references` (line 308) — which is correct for the
  fallback gate — but never invokes `super::precombined::spawn_precombined_meshes`.
  When the CSG reader lands and `pc_spawned > 0` becomes the common case, the
  exterior loader will need the same precombined-spawn pass plus the same
  conditional-absorption gate. Today it's silent (paired with D3-NEW-01 above
  it's correct-by-accident), but the wiring gap is structural.
- **Evidence**:
  ```
  $ grep -n "spawn_precombined" byroredux/src/cell_loader/*.rs
  byroredux/src/cell_loader/load.rs:148
  byroredux/src/cell_loader/load.rs:155
  byroredux/src/cell_loader/precombined.rs:54
  ```
  No exterior.rs call site.
- **Impact**: When CSG support arrives, exterior cells will skip the
  precombined-spawn path and render per-REFR — the FO4 headline performance
  feature missing on the cells it was designed for.
- **Related**: D3-NEW-01 (parser-side companion gap), #1188.
- **Suggested Fix**:
  Add the same Phase-3a precombined call + conditional-absorption gate to
  `exterior.rs`. Coord-frame consideration:
  - Each exterior cell's `cell.precombined_mesh_hashes` paths are keyed by the
    cell's form_id (Bethesda CK convention). The bake transform is local to
    that cell.
  - Exterior cell origin in world space is
    `(cell_x * EXTERIOR_CELL_UNITS, 0, cell_y * EXTERIOR_CELL_UNITS)` where
    `EXTERIOR_CELL_UNITS = 4096`.
  - The interior precombined-spawn call passes `Vec3::ZERO + Quat::IDENTITY`
    — correct for interior because the cell origin IS the world origin.
  - The exterior call must pass `Vec3::new(cell_x * 4096.0, 0.0, cell_y * 4096.0)`
    so the bake lands in the correct world-space position.

---

### D3-NEW-03: Precombined-spawn coord-frame assumption is undocumented

- **Severity**: LOW
- **Dimension**: Transform Compatibility (forward-looking)
- **Location**: [byroredux/src/cell_loader/precombined.rs:61-65](../../byroredux/src/cell_loader/precombined.rs#L61-L65)
- **Status**: NEW
- **Description**:
  `spawn_precombined_meshes` currently hardcodes
  ```rust
  let pos = Vec3::ZERO;
  let rot = Quat::IDENTITY;
  let scale = 1.0;
  ```
  with the comment "precombined NIFs are baked in cell-local coords so they sit
  at the cell origin with no rotation / scale." That's correct for the **interior**
  caller in `load.rs` but assumes the caller is itself at the cell origin. The
  helper accepts no explicit cell-origin argument, so the exterior caller can't
  pass a non-zero offset without a signature change. Today this is benign (no
  exterior caller, no CSG geometry); when D3-NEW-01 / D3-NEW-02 are addressed
  the helper will need either an explicit `cell_origin: Vec3` parameter OR a
  documented invariant that callers must pre-translate.
- **Suggested Fix**:
  Extend the signature to take `cell_origin: Vec3` and apply it to the spawn
  transform. Mirror the existing `spawn_placed_instances` composition shape.

---

### Steady-state checks (no findings)

- **Shepperd Matrix3→Quat + normalisation** (`crates/nif/src/import/transform.rs`):
  unchanged since #333. Re-read 2026-05-19; no new conversion path that bypasses
  normalisation surfaced.
- **Z-up → Y-up basis swap** (`crates/nif/src/import/coord.rs`): every NIF-import
  site that emits world-space transforms still goes through it. The new
  `precombined.rs` doesn't extract any coords directly — it just calls
  `spawn_placed_instances` with identity (so the basis swap inside the cached
  imported meshes still applies).
- **Skin bind inverses** (`crates/core/src/ecs/components/skinned_mesh.rs`,
  `compute_palette_into`): unchanged since #771. No regression marker.
- **BFS world-transform propagation** (`byroredux/src/systems/`): post Session-34/35
  split the system is in `systems/` submodule; logic unchanged. The new
  `placement_root` parent-edge convention (#544) is consumed correctly — verified
  via the seeded initial GlobalTransform at `spawn.rs:639-643` which matches
  what BFS computes on tick 1.
- **Uniform scale**: every emit site continues to use `f32`; no Vec3-scale drift.

---

**Summary**: dim 3 surfaced 3 findings, all NEW, all tied to #1188's exterior-path
scope gap. The conversion math remains correct end-to-end. D3-NEW-01 and D3-NEW-02
are HIGH because they break the FO4 performance headline once CSG support lands;
D3-NEW-03 is LOW (API ergonomics).

---

## Dimension 4 — Property → Material Mapping

**Today: 2026-05-19. Prior baseline: AUDIT_LEGACY_COMPAT_2026-04-30.md dim 4.**

All 12+ legacy property types and all 8 NiTexturingProperty slots are wired into
the material walker and reach GpuInstance / GpuMaterial. The fog / wireframe /
dither / shade closure (#558 / #607) and stencil pipeline integration (#337 /
#607) remain in place. The walker dispatches on **17 distinct property types**
post-2026-04-30 (BSLightingShaderProperty, BSEffectShaderProperty,
BSSkyShaderProperty, BSWaterShaderProperty, NiAlphaProperty, NiZBufferProperty,
NiMaterialProperty, NiTexturingProperty, BSShaderPPLightingProperty,
BSShaderNoLightingProperty, TileShaderProperty, SkyShaderProperty,
TallGrassShaderProperty, NiStencilProperty, NiFlagProperty (covers Specular /
Wireframe / Shade / Dither), NiVertexColorProperty — see [walker.rs:115-888](../../crates/nif/src/import/material/walker.rs)).

Two NEW findings, both LOW.

---

### D4-NEW-01: BSLightingShaderProperty under-reads by 4 bytes on FO4 shared-precombined NIFs

- **Severity**: LOW (observability; block_size recovery masks the symptom)
- **Dimension**: Property → Material Mapping (parser-side)
- **Location**: [crates/nif/src/blocks/shader.rs:760-1100 (BSLightingShaderProperty::parse)](../../crates/nif/src/blocks/shader.rs#L760)
- **Status**: NEW
- **Description**:
  Parsing a vanilla FO4 interior precombined `_oc.nif` file
  (`Fallout4 - MeshesExtra.ba2 → meshes\precombined\00001e5d_03ebca62_oc.nif`,
  Dugout Inn cell) produces a consistent stream-drift WARN:

  ```
  NIF parse: 23 block(s) parsed Ok but consumed != block_size;
  stream realigned by header size table: BSLightingShaderProperty=23
  ```

  Every BSLightingShaderProperty block in the file under-reads. The
  block_size table recovers the stream so downstream parsing succeeds, but
  the four bytes per BSLSP are silently absorbed into the recovery skip and
  whichever trailing field they belong to ends up zeroed.

  The FO4 audit's #403 / FO4-D1-C1 widened the WetnessParams `Unknown 1`
  gate to `>= 130` to fix a similar drift on regular FO4 meshes. The
  precombined `_oc.nif` content variant (BSPackedCombinedSharedGeomDataExtra
  on the parent node, BSTriShape with `num_vertices = 0`) wasn't part of
  that sweep — those NIFs ship with the Shared-precombined layout that
  may omit or add a per-BSLSP field relative to regular FO4 BSVER-130
  geometry. Likely candidate: the `Root Material` NiFixedString sidecar
  (`shader.rs:670`) — in precombined NIFs the material is referenced from
  the absorbed REFRs' TXST overrides, not from a per-BSLSP path, so the
  sidecar may legitimately be absent (4-byte u32 zero string-ref).

- **Evidence**:
  - `nif_stats /tmp/dugout_pc0.nif` reports `BSLightingShaderProperty=23`
    drift events; ALL 23 BSLSP blocks under-read by 4 bytes.
  - Regular FO4 BSVER-130 NIFs (non-precombined) parse with zero drift on
    BSLSP — verified via the 2026-04-26 100% parse-rate sweep.
  - The discriminator is the BSPackedCombinedSharedGeomDataExtra extra_data
    ref on the parent NiNode: every NIF that carries that block-type also
    exhibits the BSLSP drift.

- **Impact**:
  Today: cosmetic — the geometry path drops on `num_vertices == 0`
  anyway (no CSG reader). When CSG support lands, four bytes of the
  trailing BSLSP layout are silently zeroed per shader instance, which
  may translate to "wrong specular tint" or "wrong wetness on the rim"
  on every precombined surface. Worst case: shader behaviour for an
  entire FO4 cell tile silently off-target.

- **Suggested Fix**:
  Either:
  1. Diagnose empirically: add a hex dump probe to capture the four bytes
     consumed by the recovery skip on a sample precombined NIF, compare
     to a regular BSVER-130 NIF, identify the field, and gate the read
     appropriately. (Likely a per-BSLSP path-string that's u32-zero in
     shared-precombined content.)
  2. Add a `bsx_carries_packed_combined: bool` parameter to
     `BSLightingShaderProperty::parse` so the path can be variant-aware.

  Defer to whoever owns the future CSG-reader milestone — both share the
  same wire-format empirical investigation.

---

### D4-NEW-02: Material walker doesn't dispatch on NiFogProperty

- **Severity**: LOW
- **Dimension**: Property → Material Mapping
- **Location**: [crates/nif/src/import/material/walker.rs](../../crates/nif/src/import/material/walker.rs) (no arm; parser exists at `crates/nif/src/blocks/properties.rs:480-509`)
- **Status**: NEW
- **Description**:
  `NiFogProperty` is parsed by the dispatch at `crates/nif/src/blocks/mod.rs:483`
  but the material walker has no `scene.get_as::<NiFogProperty>(idx)` arm. The
  parsed `fog_depth` / `fog_color` / `flags` triplet is silently dropped.

  The 2026-04-30 audit listed NiFog under D4-NEW-01 as "wired into material
  pipeline (#558 / #607)" but that closure covered the per-node generic
  fog enable bit, not the NiFogProperty per-node fog override.

- **Evidence**:
  - `grep -n "NiFogProperty" crates/nif/src/import/material/walker.rs`: zero
    matches.
  - The property's own docstring (`properties.rs:475`) notes "1 FO3 block
    observed in the wild" — extremely rare in vanilla content.

- **Impact**:
  In production: negligible. Per the docstring, 1 NiFogProperty exists across
  the entire vanilla FO3 corpus (and the audit isn't aware of any in Skyrim+,
  FO4, etc.). Modded content could carry more.

- **Suggested Fix**:
  Either accept the gap and update the prior audit's claim (most pragmatic),
  or add a minimal walker arm that surfaces `(fog_depth, fog_color)` onto
  the Material component's existing `fog_far_color` / `fog_far` fields when
  no cell-lighting fog is authored.

---

### Steady-state checks (no findings)

- **All 8 NiTexturingProperty slots** (Base, Dark, Detail, Gloss, Glow, Bump,
  Decal 0/1/2/3) wired (walker.rs:545-625 secondary-slot block).
- **NiAlphaProperty src/dst blend + alpha-test + threshold + func**: wired
  end-to-end (post #263 alpha-test func fix). The walker also has a fallback
  branch at lines 416-425 for BSEffectShaderProperty that implicit-enables
  blend when NiAlphaProperty is absent.
- **NiVertexColorProperty mode** (Source / Ambient / Emissive): wired into
  `mesh.vertex_color_mode` → GpuMaterial.
- **NiZBufferProperty** (z_test / z_write / z_function): wired post-#398.
- **BSEffectShaderProperty falloff cone**: wired post-#620.
- **BSShaderNoLightingProperty falloff cone**: wired post-#451 (sibling fix).
- **NiStencilProperty pipeline integration**: wired post-#337 / #607.
- **NiSpecularProperty flags=0 disables specular** (`walker.rs:846`): correct.
- **NiWireframeProperty flags=1 enables wireframe** (`walker.rs:855`): correct.
- **NiShadeProperty flags=0 → flat shading** (`walker.rs:864`): correct.
- **NiDitherProperty flags=1 → 16-bit dithering** (`walker.rs:867`): correct.
- **BSLightingShaderProperty WetnessParams gate widening to BSVER >= 130**:
  intact (`shader.rs:945`, #403 / FO4-D1-C1).
- **BSLightingShaderProperty FO76 LuminanceParams + TranslucencyParams gates
  to `>= 155`**: intact (`shader.rs:985`, #746 / SF-D1-01).
- **TXST slot routing on `BSLightingShaderType` skin-tint / hair-tint**: intact
  (#563, d9bc363).

---

**Summary**: dim 4 surfaced 2 findings, both LOW. The walker dispatch is
comprehensive (17 property types). The BSLightingShaderProperty drift on
precombined NIFs is observable but masked by block_size recovery; the
NiFogProperty walker gap is documented but its blast radius is negligible
on vanilla content. No CRITICAL / HIGH / MEDIUM issues this sweep.

---

## Dimension 5 — Animation Readiness

**Today: 2026-05-19. Prior baseline: AUDIT_LEGACY_COMPAT_2026-04-30.md dim 5.**

Steady-state across the board. The NIF dispatch covers 16 controllers + 18
interpolators, the interpolation library has 6 sample functions (translation /
rotation / scale / float / color / bool), TBC tangents + Hermite + step / linear
keys are all implemented, embedded clips flow through the same
`AnimationClip` shape as loose-KF imports, text-key events are consumed by
the scripting layer (`byroredux/src/systems/animation.rs:417-433`), and root
motion is split + written into `RootMotionDelta` (`animation.rs:467-495`).

The cell-loader animation wiring (#544) — placement-root + per-mesh `Name` +
subtree binding — landed earlier and is exercised by every cell load.

Carry-overs from prior audits closed:
- D3-NEW-01 (skin bind-inverse) → #771.
- D5-NEW-03 (NPC AnimationPlayer attach) → #772.
- B-spline path → #155.

**Zero NEW findings this sweep.** Animation infrastructure is mature.

---

### Inventory (for reference, not findings)

**Controllers dispatched** (count: 16) — `crates/nif/src/blocks/mod.rs`:
- `NiParticleSystemController` / `NiBSPArrayController`
- `NiTextureTransformController`, `NiUVController`
- `NiFloatExtraDataController`
- `NiLightColorController`, `NiLightDimmerController`, `NiLightIntensityController`, `NiLightRadiusController`
- `NiMaterialColorController`
- `NiMultiTargetTransformController`
- `NiGeomMorpherController`
- `NiLookAtController`
- `NiPathController`
- `BSLightingShaderProperty(Float|Color|UShort)Controller`
- Plus the `BsLagBoneController` + `BsProceduralLightningController` + generic
  `NiTimeController` fallback in `controller/mod.rs`.

**Interpolators dispatched** (count: 18) — `crates/nif/src/blocks/mod.rs`:
- `NiTransformInterpolator` / `BSRotAccumTransfInterpolator`
- `NiBSpline(Comp)?TransformInterpolator`
- `NiBSpline(Comp)?FloatInterpolator`
- `NiBSpline(Comp)?Point3Interpolator`
- `NiFloatInterpolator`, `NiPoint3Interpolator`, `NiColorInterpolator`
- `NiPathInterpolator`, `NiLookAtInterpolator`

**Sample functions** — `crates/core/src/animation/interpolation.rs`:
- `sample_translation` (Vec3, post #828 Hermite normalisation)
- `sample_rotation` (Quat, SLERP + step + TBC)
- `sample_scale` (f32)
- `sample_float_channel`
- `sample_color_channel`
- `sample_bool_channel`

**Animation runtime systems** — `byroredux/src/systems/animation.rs`:
- `advance_time` → `AnimationPlayer` per-clip
- `advance_stack` + `sample_blended_transform` → per-layer blending
- `split_root_motion` → `RootMotionDelta` write
- `visit_text_key_events` / `visit_stack_text_events` → scripting
  `AnimationTextKeyEvents`

---

### Spot-checks done (all clean)

1. Cell-loader animation binding (`spawn.rs:880-895`): per-placement `AnimationPlayer`
   spawned with `root_entity = Some(placement_root)`. Subtree name map binds
   correctly through the `Name` rows seeded at `spawn.rs:659-661`.
2. Embedded clip discovery (`references.rs:867-877`): clips captured on the
   `CachedNifImport` survive the NifImportRegistry LRU.
3. BSPackedCombinedSharedGeomDataExtra-bearing NIFs (today's #1188 content):
   no animation controllers parsed — these are static architecture meshes
   by design. Confirmed via `dump_pc` probe (zero NiKeyframeData / NiInterpolator
   blocks in `dugout_pc0.nif`'s 115-block histogram).
4. B-spline path on FNV: per the feedback memory note
   `feedback_bspline_not_skyrim_only.md`, `NiBSplineCompTransformInterpolator`
   is reachable on FNV (and FO3); dispatch in `mod.rs:771-784` is unconditional
   on version, so no regression.

---

**Summary**: dim 5 is in steady-state. Zero NEW findings.

---

## Dimension 6 — String Interning Alignment

**Today: 2026-05-19. Prior baseline: AUDIT_LEGACY_COMPAT_2026-05-07_DIM6.md (steady-state).**

`StringPool` (case-folding on intern, integer-equality on `FixedString`) and the
`build_subtree_name_map` lookup remain semantically aligned with Gamebryo's
`NiFixedString` + `NiGlobalStringTable`. The single-lock pre-pass in
`spawn_placed_instances` (#882) is intact.

**Zero NEW findings this sweep.** One observation worth recording for the
post-#1188 milestone reviewer (NOT raised to a formal finding).

---

### Spot-checks done (all clean)

1. **Case-folding intern semantic** — `crates/core/src/string/mod.rs:52-90`:
   `intern` and `get` both lowercase via `ascii_lowercase_into_buf` (256-byte
   stack-pinned scratch with heap fallback for longer inputs). Matches
   Gamebryo's `NiFixedString` case-insensitive hash. Verified via
   `feedback_color_space` / `feedback_chrome_means_missing_textures` memory
   notes that already established this invariant.
2. **Symbol stability across pool growth**: `string_interner::backend::StringBackend`
   is a slab-style backend; symbols are slot indices. Inserting new strings
   never reassigns existing slots. `FixedString::eq` is `u32`-equality on
   the slot. Confirmed via the `string_interner` crate's documented
   invariants and `world_tests.rs:514-571` round-trip tests.
3. **Lock hoisting at the cell-loader spawn site**: post-Session-35 split,
   `spawn.rs:391-444` still acquires `world.resource_mut::<StringPool>()`
   exactly once, resolves all 10 texture-slot paths + interns the mesh
   name in the same write-lock scope, and collects into `resolved_paths`
   before re-entering the per-mesh spawn loop. Pattern unchanged from #882.
4. **Animation subtree name lookup**: `anim_convert.rs::build_subtree_name_map`
   walks `Parent → Children` BFS and matches against the `Name(FixedString)`
   rows seeded at spawn time. Both paths intern through `StringPool`, so
   matches are integer-equality on the symbol. No string-compare hotspot.
5. **#1188 capacity check (Diamond City Dugout Inn)**: 893 REFRs × ~10
   texture-slot paths per REFR = ~9k path-string interns per cell. The
   `string_interner` `StringBackend` has no fixed cap — growth is amortised
   `O(1)` per insert. Production memory: ~256 KB pool after Diamond City
   load (bounded by unique paths, which de-duplicate; vast majority of
   the 9k resolves hit existing slots).

---

### Observation (informational, not a finding)

**FixedString does not preserve original-case form on resolve.** The
`StringPool::resolve` docstring (`mod.rs:63`) explicitly notes:

> Returns the lowercased canonical form — the case the caller originally
> passed to `StringPool::intern` is *not* preserved.

This is correct for the **lookup** semantic (Bethesda content is
case-insensitive everywhere) but means any UI / debug surface that wants
to display a path in its authored case has to capture the original case
before interning. Today only the debug-server console (`prid`, `inspect`,
`mesh.info`) shows path strings to a human, and it does so via the
post-intern lowercase form. That's acceptable behaviour. Worth recording
because a future "show the authored TXST path next to its lowercase
canonical form" surfacing in an audit tool would need a separate
preservation channel (e.g. SourceStringPool resource keyed by symbol).

Not raising to a formal finding because:
- Zero production callers care about authored case today.
- Adding case-preservation would double the storage overhead for a
  non-functional cosmetic benefit.

---

**Summary**: dim 6 is in steady-state. Zero NEW findings.
