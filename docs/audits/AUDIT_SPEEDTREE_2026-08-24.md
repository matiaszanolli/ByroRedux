# SpeedTree Subsystem Audit — 2026-08-24

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker (`parser.rs`, `stream.rs`, `tag.rs`, `version.rs`, `scene.rs`) and the
placeholder-billboard importer (`crates/spt/src/import/mod.rs`) — plus the
cross-cut wiring: `byroredux/src/cell_loader/references/synth_child.rs` (the
`is_spt` dispatch), `byroredux/src/cell_loader/references/import.rs`
(`parse_and_import_spt`), `byroredux/src/cell_loader/spawn.rs`,
`byroredux/src/cell_loader/spawn/mesh_instance.rs`,
`byroredux/src/scene/nif_loader.rs`, `crates/plugin/src/esm/records/tree.rs`,
`crates/core/src/ecs/components/billboard.rs`,
`byroredux/src/systems/billboard.rs`, `byroredux/src/boot.rs`,
`byroredux/src/cell_loader/nif_import_registry.rs`.

Single-pass, solo execution per this run's explicit constraint — **no
sub-agents dispatched**, all six dimensions read, traced, and verified
directly (Read/Grep/Bash only).

**Depth**: source + build/test verification. `cargo check -p byroredux-spt`,
`cargo check -p byroredux`, `cargo test -p byroredux-spt --lib` (48/48 pass),
and `cargo test -p byroredux --bin byroredux billboard` (11/11 pass,
including `cell_loader::references::import_tests`) were all run and are
clean. The bare `cargo test --workspace` build failure noted in the dispatch
(`crates/scripting/examples/fragment_coverage.rs:59`, E0004) is confirmed
unrelated to this crate and was worked around with per-crate `cargo test`.

**Method**: diffed direction against `AUDIT_SPEEDTREE_2026-08-20.md`
(previous cycle, 7 findings, all in the newly-added shared wind model), then
re-derived the current state of every one of that cycle's open findings by
reading the actual commits that touched `crates/spt/` and its consumers since
(`7453f565`, `4e1afcbe`, `5428e872`). The dispatch for this run specifically
flagged a claim from a concurrent `/audit-ecs` run — that `4e1afcbe` deleted
the SpeedTree geometry wind-bending loop and "orphaned `SpeedTreeWind` on
cell-loader mesh entities that don't also carry `Billboard`" — for direct,
independent investigation. That investigation is reported in full below
(**Primary Investigation**) rather than folded into a regular finding, since
its outcome (disproved) is itself the most load-bearing result of this cycle.

---

## Dedup pass (mandatory)

Cached issues: `/tmp/audit/speedtree/issues.json` (fresh pull, `speedtree OR
spt OR TREE` search, this run).

**Headline result this cycle**: of the six SpeedTree issues opened by the
2026-08-20 audit, **five are fixed in code at HEAD** (still shown `OPEN` on
GitHub — the fix→issue citation gap tracked separately by #3218) and the
sixth (#3193, the dead geometry loop) is the subject of the primary
investigation and was **resolved by deletion**, not by a fix. Recommend
closing all six; not done here per the "no GitHub issues" instruction for
this run.

| Issue | Title (short) | GitHub state | Code state at HEAD |
|---|---|---|---|
| **#1822** (SPT-NEW-07) | tag-13005 tail-swallow misparse | **CLOSED** (2026-08-22) | Verified fixed. `7453f565` added `SptStream::peek_string_lp_bytes` + `is_plausible_spt_curve_string` (printable-ASCII/whitespace gate on the peeked candidate bytes) so the `MaybeStringElseBare` branch only takes the `String` arm when the bytes actually look like `BezierSpline` text; otherwise falls back to `Bare` without consuming anything. Re-read the diff and the rewritten `tag_13005_at_eof_does_not_panic` test — sound. Confirmed zero regression against the live 133-file corpus per the commit's own A/B stash comparison. |
| **#3079** | skill's `is_spt` dispatch pointed at `references/mod.rs`, lives in `synth_child.rs` | **CLOSED** (2026-08-23) | Doc-only fix (`a9a42a16`), confirmed in `SKILL.md`'s current entry-point list (already correct at the top of this file). |
| **#3080** | `import/mod.rs` docstring documents pre-#1001/#1002 two-tier size chain | **OPEN** | **Still true, unchanged.** Re-verified at `import/mod.rs:22-23`: "Size comes from the TREE record's `OBND` bounds, falling back to a 256 × 512 game-unit default" — omits the BNAM/MODB middle tiers `compute_billboard_size` (`:205-243`) actually implements. Not re-filed (already tracked). |
| **#3190** (SPT-D3-2026-08-20-01) | `SpeedTreeWind` built from unpinned CNAM floats (No-Guessing violation) | OPEN | **Fixed in code.** `4e1afcbe` deleted the CNAM read entirely — `references/import.rs:328-332` now hardcodes `let wind = Some((1.0, 0.0));` with a comment citing #3190 by number, and `tree.rs`'s `canopy_params` docstring was updated to say "parse-but-don't-consume … until a citable layout … lands (#3190)". Test renamed `parse_and_import_spt_does_not_guess_wind_from_tree_cnam`, asserting the constant `(1.0, 0.0)` regardless of the fixture's `canopy_params`. This is the correct fix shape per the prior report's own suggested-fix option (a). |
| **#3191** (SPT-D2-2026-08-20-01) | wind bend composed in object-local frame, weighted by world-space components | OPEN | **Fixed in code.** `apply_speedtree_wind` (`billboard.rs:135-174`) now builds `axis = Vec3::new(-wind_dir.y, 0.0, wind_dir.x)` (world-horizontal, perpendicular to wind) and **pre-multiplies**: `Quat::from_axis_angle(axis, angle) * base` — the lean is applied in world space regardless of what `base` is, fixing the billboard-view-axis coupling the finding identified. New tests `speedtree_world_lean_is_camera_orbit_invariant` and `reversing_wind_reverses_mean_lean` pin exactly the two properties the finding said were missing (orbit invariance, signed mean lean). Independently re-derived both properties algebraically from the axis-angle formula during this audit — they hold. |
| **#3192** (SPT-D2-2026-08-20-02) | #1374 camera-parked gate bypassed every frame in windy exteriors; re-dirties every `Billboard` | OPEN | **Fixed in code.** The per-entity gate moved inside the loop: `if !camera_changed && tree_wind.is_none() { continue; }` (`billboard.rs:96-98`) — a stationary camera now skips `get_mut`/write entirely for `Billboard` entities that carry no `SpeedTreeWind`, while SpeedTree entities still refresh under active/changing wind. New test `parked_camera_under_wind_does_not_redirty_ordinary_billboard` asserts the sprite-without-`SpeedTreeWind` entity is absent from the `GlobalTransform` dirty set after a parked-camera windy frame while the tree entity is present. |
| **#3193** (SPT-D3-2026-08-20-02) | geometry-tree wind branch unreachable — no entity can carry `SpeedTreeWind`+`MeshHandle` without `Billboard` | OPEN | **Resolved by deletion — subject of the Primary Investigation below.** The dead branch (and its `FxHashMap`-should-have-been-`rustc_hash` cache) was removed wholesale by `4e1afcbe`, matching the prior report's own suggested fix ("remove the branch until [a real producer] exists"). Investigated independently below; the removal does **not** orphan any live entity. |
| **#3194** (SPT-D2-2026-08-20-03) | SpeedTree has no non-finite gust guard | **CLOSED** (2026-08-21, pre-window) | Verified still in place: `billboard.rs:152` `let gust = if gust.is_finite() { gust.max(0.0) } else { 0.0 };`, comment cites #3194 by number. |
| **#3195** (SPT-D4-2026-08-20-01) | loose `--tree` route deletes tree on parse error, never attaches `SpeedTreeWind` | OPEN | **Fixed in code.** `nif_loader.rs:218-221` now degrades a parse error to `SptScene::default()` with `log::warn!` (was `log::error!` + `return None`), matching the cell route's `#3078` contract. Both `node.billboard_mode`/`mesh.billboard_mode` attach sites (`:538-542`, `:1010-1014`) now insert `SpeedTreeWind::new(1.0, 0.0)` alongside `Billboard` when `is_spt`. New regression test `malformed_loose_spt_still_imports_placeholder` (`nif_loader.rs` tests module) pins the recoverable-placeholder half. |
| **#3123** (CONC-2026-08-20-03) | `make_billboard_system` reads `TotalTime` undeclared | OPEN | **Fixed in code.** `5428e872` added `.reads_resource::<TotalTime>()` to the `Access` declaration (`boot.rs:1229`... now `:1231`). New test `scheduler_access_tests::billboard_declaration_includes_shared_total_time_clock` pins it — but see **SPT-D3-2026-08-24-01** below: the same commit that added this correct declaration left a now-stale one in place two lines below it. |

No open issue in the search covers either finding filed below.

---

## Change window

Since the 2026-08-20 report: `crates/spt/src/parser.rs` (+`7453f565`, #1822
fix — every other crate file unchanged), and a nine-file-touching squashed
commit `4e1afcbe` (billboard.rs, references/import.rs, spawn.rs,
mesh_instance.rs, nif_loader.rs, tree.rs, billboard.rs component doc,
import_tests.rs) that rewrote the entire wind-model consumer side, followed
by `5428e872` (scheduler access cleanup, adds `TotalTime` + `WindField` to
several declarations including billboard's).

Net effect this cycle: **five of six previously-open findings fixed, one
resolved by deletion, zero new findings in the areas those commits touched —
except for one stale-declaration seam the deletion left behind** (below).

---

## Primary Investigation: did `4e1afcbe` orphan `SpeedTreeWind` on
non-`Billboard` mesh entities?

**Claim under investigation** (relayed from a concurrent `/audit-ecs` run,
via this run's dispatch): commit `4e1afcbe` deleted the SpeedTree geometry
wind-bending loop from `byroredux/src/systems/billboard.rs` (the
`geometry_bases`-cache loop that bent non-impostor SpeedTree trunk/branch
meshes), **orphaning `SpeedTreeWind` on cell-loader mesh entities that don't
also carry `Billboard`**, and that the regression-guard test was rewritten to
assert the *absence* of the cache instead of catching the regression.

**Verdict: the deletion happened exactly as described, but it does not
orphan any live production entity. The premise that such entities exist is
false at HEAD. Not a regression — it is the correct resolution of an already
-filed, already-investigated dead-code finding (#3193, 2026-08-20 cycle).**

### What was actually deleted

`git show 4e1afcbe -- byroredux/src/systems/billboard.rs` confirms the loop:

```rust
if let (Some(swq), Some(mesh_q)) = (swq.as_ref(), world.query::<MeshHandle>()) {
    let mut live_geometry_count = 0usize;
    for (entity, tree_wind) in swq.iter() {
        if bq.as_ref().is_some_and(|q| q.contains(entity)) || !mesh_q.contains(entity) {
            continue;
        }
        // ... bend `entity` using a per-entity `geometry_bases` cache ...
    }
    // ... retain-prune `geometry_bases` ...
}
```

was removed in full, along with its `FxHashMap<u32, Quat>` cache and the
`MeshHandle` import. The test `geometry_speedtree_mesh_bends_without_billboard
_component` (which spawned an entity with `MeshHandle` + `SpeedTreeWind` but
no `Billboard` and asserted the loop bent it) was replaced by
`parked_camera_under_wind_does_not_redirty_ordinary_billboard`, which tests a
different property entirely (the #1374 dirty-set gate, #3192's fix). So yes —
the specific synthetic scenario that test constructed (`SpeedTreeWind` +
`MeshHandle`, no `Billboard`) no longer has any test coverage.

### Is that scenario ever constructed in production?

Traced every `SpeedTreeWind` insert site at HEAD (`grep -rn "SpeedTreeWind"`
across `byroredux/src` and `crates/`, excluding tests):

| Site | Gate |
|---|---|
| `byroredux/src/cell_loader/spawn/mesh_instance.rs:755-762` | `if let Some(raw) = mesh.billboard_mode { world.insert(Billboard) }` (independent `if`) followed by `if let Some((r,s)) = cached.speedtree_wind { world.insert(SpeedTreeWind) }` |
| `byroredux/src/scene/nif_loader.rs:538-542` (node loop) | `if let Some(raw) = node.billboard_mode { world.insert(Billboard); if is_spt { world.insert(SpeedTreeWind); } }` — **nested** inside the `Billboard` arm |
| `byroredux/src/scene/nif_loader.rs:1010-1014` (mesh loop) | same nested shape |
| `byroredux/src/cell_loader/spawn.rs` (placement root) | **removed by `4e1afcbe`** — the root marker mirror (`spawn.rs:791-793` pre-delta) is gone; the root has no `MeshHandle` regardless, so it was never the entity of concern |

The `nif_loader.rs` sites are trivially safe: `SpeedTreeWind` is inserted
**inside** the `if let Some(raw) = …billboard_mode` block, so it can never
fire without the sibling `Billboard` insert on the same statement group.

The `mesh_instance.rs` site is the one that needs checking, because its
`SpeedTreeWind` insert is a **separate** `if let`, not nested inside the
`billboard_mode` arm — gated only on `cached.speedtree_wind`, a
per-*placement* value threaded through `CachedNifImport`, not a per-*mesh*
value. If a `.spt` placement's imported scene ever produced more than one
mesh, or a mesh whose own `billboard_mode` was `None`, that mesh would still
inherit the placement's `SpeedTreeWind` and this loop's premise would break.

Traced `cached.speedtree_wind`'s only `Some`-producing constructor,
`parse_and_import_spt` (`references/import.rs:272-398`) — the **only**
place in the codebase that builds a `CachedNifImport` with `speedtree_wind:
Some(_)` (every other constructor — `partial.rs:115`, `precombined.rs:787`,
the generic NIF `import.rs:185` — hardcodes `None`). That function's
`imported = byroredux_spt::import_spt_scene(&scene, &params, pool)` call
feeds directly into `crates/spt/src/import/mod.rs:163-165`:

```rust
ImportedScene {
    nodes: vec![root_node],
    meshes: vec![mesh],   // exactly one mesh, always
    ...
}
```

and that one `mesh` is built by `placeholder_billboard_mesh` (`:279-336`),
whose last field is unconditional:

```rust
billboard_mode: Some(BILLBOARD_MODE_BS_ROTATE_ABOUT_UP),   // import/mod.rs:336
```

— not gated on any input parameter. So the invariant holds structurally, not
by coincidence: every `.spt` import produces exactly one mesh, and that mesh
always carries `billboard_mode: Some(...)`. `mesh_instance.rs`'s loop over
`imported.meshes` therefore always attaches `Billboard` and `SpeedTreeWind`
to the *same single entity* on the cell-loader route, exactly as it did
before `4e1afcbe` (only the placement-root mirror was removed, and the root
never had a `MeshHandle` to begin with).

Compiled and ran the relevant test surfaces to confirm no behavioural gap:
`cargo check -p byroredux-spt`, `cargo check -p byroredux`,
`cargo test -p byroredux-spt --lib` (48/48), and
`cargo test -p byroredux --bin byroredux billboard` (11/11, including
`cell_loader::references::import_tests::parse_and_import_spt_surfaces_
billboard_mode_on_mesh`, which independently pins "billboard mode always
lands on the mesh").

### Conclusion

The 2026-08-20 audit's own conclusion for #3193 — "no production entity can
carry `SpeedTreeWind` + `MeshHandle` without `Billboard`" — **still holds at
HEAD, after `4e1afcbe`.** The branch really was dead before the deletion, and
deleting genuinely-unreachable code cannot orphan a component that no
producer attaches without also attaching the component the removed code
required. The suggested fix in the 2026-08-20 report was explicitly "wire a
real producer … **or remove the branch until one exists**" — the commit took
the second option, which is a legitimate resolution of that finding, not a
new bug. The rewritten test is not a "regression guard weakened to hide a
bug" — it replaced a test for dead code with a test for a different, live
property (#3192's per-entity gate), which is the correct thing to do when
deleting dead code its old test existed only to exercise.

One residual, forward-looking observation (not filed as a finding, since it
describes work that hasn't landed and is explicitly out of Phase-1 scope per
this skill's own framing): if a *future* producer ever attaches
`SpeedTreeWind` to a mesh entity without `Billboard` (the obvious candidate,
per the 2026-08-20 report, is a Skyrim+ `.nif`-rooted full-geometry SpeedTree
path), there is now genuinely no consumer for it at all — the wind marker
would be silently inert rather than wrongly-computed, which is a safer
failure mode than the frame-confused geometry loop it replaces, but still
worth remembering when that producer is built.

---

## Findings

### SPT-D3-2026-08-24-01: `make_billboard_system`'s `Access` declaration still reads `MeshHandle`, a lock the system no longer takes

- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring
- **Location**: `byroredux/src/boot.rs:1228-1236` (declaration), vs.
  `byroredux/src/systems/billboard.rs` (no `MeshHandle` reference anywhere in
  the file)
- **Status**: NEW (introduced by `4e1afcbe`, not cleaned up by the
  immediately-following `5428e872`)
- **Description**: `4e1afcbe` deleted the geometry-tree loop that queried
  `world.query::<MeshHandle>()` (see Primary Investigation above) and removed
  `MeshHandle` from `billboard.rs`'s `use` list entirely — confirmed by
  `grep -n "MeshHandle" byroredux/src/systems/billboard.rs` returning no
  hits. The scheduler's declared `Access` for this system was not updated to
  match:

  ```rust
  scheduler.add_exclusive_with_access(
      Stage::PostUpdate,
      make_billboard_system(),
      Access::new()
          .reads_resource::<ActiveCamera>()
          .reads_resource::<TotalTime>()
          .reads_resource::<byroredux_core::ecs::components::groundcover::WindField>()
          .reads::<byroredux_core::ecs::Billboard>()
          .reads::<byroredux_core::ecs::SpeedTreeWind>()
          .reads::<byroredux_core::ecs::MeshHandle>()      // boot.rs:1235 — stale
          .writes::<byroredux_core::ecs::GlobalTransform>(),
  );
  ```

  This is the mirror image of the already-fixed #3123
  (CONC-2026-08-20-03, "reads `TotalTime` undeclared") — that finding was an
  **under**-declaration (missing a real read), and it was fixed by
  `5428e872`, in the *same declaration block*, four lines away from this
  stale `MeshHandle` line, which that commit left untouched. The declaration
  now claims a lock the system never acquires.

  `boot.rs`'s own comment for this three-system chain (`:1218-1223`, citing
  #2391) states the reason these exclusives declare access explicitly even
  though the scheduler doesn't pair exclusives: *"a blank `sys.accesses` row
  is exactly the wrong place for [who touches what when] to be invisible."*
  A stale row is the same failure in the opposite direction — it is not
  blank, but it is wrong, and it is exactly as invisible to anyone reading
  `sys.accesses` output or reasoning about lock ordering from the
  declaration alone.
- **Evidence**:
  - `boot.rs:1235` — the stale `.reads::<byroredux_core::ecs::MeshHandle>()`.
  - `grep -n "MeshHandle" byroredux/src/systems/billboard.rs` → no hits.
  - `git show 4e1afcbe -- byroredux/src/systems/billboard.rs` — removes
    `MeshHandle` from the `use` list and the only query that read it.
  - `git show 5428e872 -- byroredux/src/boot.rs` — adds
    `.reads_resource::<TotalTime>()` to this exact declaration block without
    touching the `MeshHandle` line two entries below.
  - `byroredux/src/scheduler_access_tests.rs:114-126` — the one test that
    exists for this declaration (`billboard_declaration_includes_shared_
    total_time_clock`) checks only for the presence of the `TotalTime`
    string; nothing asserts the declaration's absence of components the
    system doesn't query.
- **Impact**: Documentation/analysis only. `add_exclusive_with_access`
  exclusives are not paired against each other by the scheduler's analyzer
  (per the same `#2391` comment this file already carries), so there is no
  live scheduling or lock-ordering consequence today. The cost is the same
  class as #3123 before its fix: `sys.accesses` (and any future tooling or
  auditor reading this declaration as the authority for "what does the
  billboard system touch") is told the system reads `MeshHandle`, and it
  does not.
- **Related**: #3123 (CONC-2026-08-20-03, the sibling under-declaration,
  fixed by the same commit that left this one stale), #2391 (the rationale
  for declaring access on these three exclusives at all), the Primary
  Investigation above (the deletion that made this line stale).
- **Suggested Fix**: Remove `.reads::<byroredux_core::ecs::MeshHandle>()`
  from the `make_billboard_system` registration in `boot.rs`. Extend
  `scheduler_access_tests.rs`'s `billboard_declaration_includes_shared_
  total_time_clock` (or add a sibling) to assert the declaration does
  **not** mention `MeshHandle`, so a future re-addition of the geometry
  branch has to consciously re-add the declaration rather than the reverse
  drift silently persisting.

---

### SPT-D2-2026-08-24-01: two `SpeedTreeWind`-adjacent field docstrings still describe the CNAM-derived wind model `4e1afcbe` deleted

- **Severity**: LOW
- **Dimension**: Placeholder Fallback (secondary: TREE→Billboard Wiring)
- **Location**: `crates/spt/src/import/mod.rs:70-77`
  (`SptImportParams::wind`), `byroredux/src/cell_loader/nif_import_registry.rs:156-157`
  (`CachedNifImport::speedtree_wind`)
- **Status**: NEW (introduced by `4e1afcbe`'s CNAM-removal fix for #3190;
  the two doc comments were not updated alongside it)
- **Description**: The fix for #3190 (see Dedup table above) replaced the
  CNAM-derived wind computation with a hardcoded neutral constant
  (`let wind = Some((1.0, 0.0));`, `references/import.rs:332`) specifically
  *because* CNAM's field layout is unpinned — and the commit correctly
  updated `TreeRecord::canopy_params`'s own docstring (`tree.rs:91-98`) and
  `SpeedTreeWind`'s struct-level doc (`billboard.rs` component,
  `crates/core/src/ecs/components/billboard.rs:92-98`) to say so. It missed
  two sibling docs one hop further down the same data path:

  `crates/spt/src/import/mod.rs:70-77` (`SptImportParams::wind` field doc):

  ```rust
  /// Wind sensitivity / strength from the TREE record's `CNAM`
  /// (Oblivion ships 5 × f32; FO3/FNV ship 8 × f32 — exact field
  /// semantics not pinned). The first two finite values are carried to the
  /// spawned SpeedTree billboard and modulate its response to the shared
  /// weather `WindField`; ...
  ```

  `byroredux/src/cell_loader/nif_import_registry.rs:156-157`
  (`CachedNifImport::speedtree_wind` field doc):

  ```rust
  /// TREE.CNAM response/stiffness for SpeedTree sway. `None` for NIF and
  /// generated imports, which use the shared weather response fallback.
  ```

  Both still claim the two `f32`s carried on these types are read from
  `TREE.CNAM`'s "first two finite values." That is no longer true anywhere
  in the codebase: the sole production writer of both fields
  (`references/import.rs:328-332`) reads no `TreeRecord` field at all for
  `wind` — it is a compile-time constant. A contributor who trusts either of
  these two docstrings over the actual call site (a reasonable thing to do
  when the type's own field-level doc is the more specific, more-local
  source) would believe CNAM parsing is live here and could reintroduce
  exactly the No-Guessing violation #3190 was filed and fixed to remove —
  this is the scenario the project's own "audit finding hygiene" memory
  warns about in the opposite direction (trusting a stale doc instead of
  re-deriving from source).
- **Evidence**: the two snippets above; `references/import.rs:328-332` (the
  actual, now doc-divergent, producer); `tree.rs:91-98` and
  `crates/core/src/ecs/components/billboard.rs:92-98` (the two sibling docs
  on the same data path that **were** correctly updated in the same commit,
  demonstrating the omission was incomplete propagation, not a deliberate
  choice).
- **Impact**: Documentation-only; no behavioural effect today (verified: the
  code paths that read these two docstrings' host fields do not read
  `TreeRecord::canopy_params`). Risk is entirely forward-looking — a future
  contributor extending this path who reads the field doc instead of the
  call site.
- **Related**: #3190 (the fix this documentation lagged), the No-Guessing
  Policy, #3080 (a sibling doc-rot finding in the same file, already
  tracked and still open).
- **Suggested Fix**: Update both docstrings to match `tree.rs`'s and
  `billboard.rs`'s current wording — e.g. "Currently a neutral runtime
  constant (`(1.0, 0.0)`); `TreeRecord.canopy_params` (CNAM) is
  parsed-but-not-consumed until a citable field layout lands (#3190)." Small
  enough to fold into the same PR as the #3080 fix if convenient, since both
  sit in the same docstring neighborhood.

---

## Dimension summary (every dimension enumerated)

| Dimension | Findings | Verdict / basis this cycle |
|---|---:|---|
| 1 — Walker Byte-Accounting | **0** | **#1822 closed this window** — the sole open item carried from every prior cycle. `parser.rs`'s fix (`7453f565`) re-read in full: `peek_string_lp_bytes` correctly bounds-checks with `checked_add` before slicing, doesn't consume on a `None`/failed peek, and the printable-ASCII/whitespace gate is a sound discriminator given the corpus's only observed curve-string shape. `stream.rs`/`tag.rs`/`version.rs`/`scene.rs` untouched since 2026-06-09 (now three audit cycles). Dictionary/EOF/cap/endian checklist items spot-re-verified, all still true. |
| 2 — Placeholder Fallback | **1** (LOW) | Structure intact: `import_spt_scene` still has no `Err` path, `compute_billboard_size` still OBND → BNAM → MODB → 256×512 with `[16, 8192]` clamps, `bs_bound` Z-up→Y-up unchanged, `-Z` normals + winding unchanged, alpha-test cutout fields unchanged, `BsRotateAboutUp` still the documented world-up-yaw-lock approximation. The one finding is the doc lag described above; the CNAM-clamp bug the prior cycle found in this dimension (#3190) is now fixed by deleting the guessed computation entirely. |
| 3 — TREE→Billboard Wiring | **1** (LOW, plus the Primary Investigation) | `#3076`/`#2206` billboard-on-mesh wiring re-confirmed intact; `CachedNifImport` synthetic defaults (`bsx_flags`/`root_flags`/`flame_attach_offset`/etc.) unchanged and correct; `TreeRecord` capture still lossless and shape-tolerant across the 5/8-float CNAM split (untouched by this cycle's changes, which only edited its docstring). The Primary Investigation closes out #3193 as a non-issue. The one finding is the stale `MeshHandle` access declaration left behind by the same deletion. |
| 4 — Per-Game Variants & Route Divergence | **0** | **#3195 fixed this window** — both routes now share the same recoverable-placeholder contract on parse error, and the loose route now attaches `SpeedTreeWind` alongside `Billboard` exactly like the cell route. `version.rs`/`detect_variant`/`MAGIC_HEAD` untouched, still log-only, still zero downstream branching. Both routes still call `parse_spt` + `import_spt_scene` identically. |
| 5 — Tag Dictionary | **0** | `tag.rs` untouched since 2026-06-09; spot-checked sizes from the 2026-08-16/08-20 table still stand (not re-derived from scratch this cycle — no code changed to warrant it). |
| 6 — NIFAL Material Translation | **0** | Untouched this cycle — none of `4e1afcbe`'s SpeedTree-adjacent sub-commits touched `material_translate.rs`, `placeholder_billboard_mesh`'s material defaults (`import/mod.rs:308-332`), or `Material::resolve_pbr`. Single boundary still holds on both routes. |

**Totals**: 6 dimensions, 2 findings, both LOW, plus one fully-resolved
external investigation (disproved).

---

## Candidates raised and disproved (not reported)

1. **The dispatch's core claim**: "`4e1afcbe` orphaned `SpeedTreeWind` on
   cell-loader mesh entities without `Billboard`." See Primary Investigation
   above — disproved by tracing every production `SpeedTreeWind` insert site
   and confirming each is structurally paired with a `Billboard` insert on
   the same entity, both before and after the commit.
2. **"The rewritten test hides a regression."** The replaced test
   (`geometry_speedtree_mesh_bends_without_billboard_component` →
   `parked_camera_under_wind_does_not_redirty_ordinary_billboard`) tested
   dead code; its replacement tests a different, live, previously-untested
   property (#3192's per-entity camera-parked gate). Removing a test for
   deleted dead code is correct hygiene, not guard-weakening. Disproved.
3. **"The new `apply_speedtree_wind` world-space axis construction might
   have a sign error that the orbit-invariance test wouldn't catch."**
   Independently re-derived the rotation algebraically for the
   `reversing_wind_reverses_mean_lean` fixture (base = identity, wind along
   ±world-X) using the axis-angle→matrix action on `Vec3::Y`; the two
   results are mirror images in `x` with equal `y`, matching what the test
   asserts. Disproved.
4. **"`references/import.rs`'s new hardcoded `wind = Some((1.0, 0.0))`
   might silently diverge between the cell route and the (also-fixed) loose
   route, since the loose route hardcodes its own `SpeedTreeWind::new(1.0,
   0.0)` in a completely separate call site (`nif_loader.rs`)."** Both
   constants are `(1.0, 0.0)` and both feed the same `SpeedTreeWind::new`
   constructor; a future change to one without the other would silently
   diverge the two routes' wind response, but that is not the case today —
   verified byte-for-byte identical at both sites. Not worth its own LOW
   finding given both are one-line constants with an obvious grep to keep
   in sync (`grep -rn "SpeedTreeWind::new(1.0, 0.0)"` — two production hits,
   both correct); noting here for the next cycle's diff rather than filing.

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 2 |
| **Total** | **2** |

This cycle's headline result is negative-but-load-bearing: the specific
regression this run was dispatched to confirm **did not happen**. Commit
`4e1afcbe` is, on independent re-derivation, a well-executed single commit
that fixed five of the six findings the 2026-08-20 cycle left open (CNAM
No-Guessing violation, object-frame/world-frame wind bug, the #1374
perf-gate regression, and the loose-route parity gap) and correctly deleted
the sixth (the dead geometry branch) per that cycle's own suggested
resolution — verified by reading every diff, re-deriving the wind math
algebraically, and running the full `byroredux-spt` and billboard test
suites (59/59 pass). The `MeshHandle` access-declaration line is the one
piece of the old branch that should have been deleted alongside it and
wasn't; the two field docstrings are the one piece of #3190's fix that
didn't fully propagate. Both are LOW, both are one-line fixes, and neither
has any runtime consequence today.

The parser half of this subsystem (`parser.rs`/`stream.rs`/`tag.rs`/
`version.rs`/`scene.rs`) closed its last open finding (#1822) this window
and has no known defects remaining in three consecutive audit cycles.

### Suggested next step

```
/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-08-24.md
```

Recommend the publish step (or a manual pass) also close #3190, #3191,
#3192, #3193, #3195, and #3123 on GitHub — all six are verified fixed (or,
for #3193, resolved by deletion) in code at this report's HEAD but remain
open on the tracker.

TALLY: CRITICAL=0 HIGH=0 MEDIUM=0 LOW=2
