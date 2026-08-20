# SpeedTree Subsystem Audit — 2026-08-20

**Scope**: `crates/spt/` (`byroredux-spt`) — the `.spt` TLV parameter-section
walker (`parser.rs`, `stream.rs`, `tag.rs`, `version.rs`, `scene.rs`) and the
placeholder-billboard importer (`crates/spt/src/import/mod.rs`) — plus the
cross-cut wiring that actually invokes it:
`byroredux/src/cell_loader/references/synth_child.rs` (the live `is_spt`
dispatch), `byroredux/src/cell_loader/references/import.rs`
(`parse_and_import_spt`), `byroredux/src/cell_loader/spawn.rs`,
`byroredux/src/cell_loader/spawn/mesh_instance.rs`,
`byroredux/src/scene/nif_loader.rs`, `crates/plugin/src/esm/records/tree.rs`,
`crates/core/src/ecs/components/billboard.rs`,
`byroredux/src/systems/billboard.rs`, `byroredux/src/boot.rs`, and — new this
cycle — the two *other* consumers of the now-shared wind model,
`byroredux/src/render/water.rs` and `crates/physics/src/water.rs`.

Single-pass, all six dimensions inline per the skill's own architecture note.
**No sub-agents dispatched.**

**Depth**: source-only (`shallow`+). Per the suite briefing, `cargo` was not
run — a concurrent process holds the target lock and 25 agents contending on
it would stall the suite. The corpus acceptance gate (`parse_real_spt`) was
therefore **not** re-run this cycle; it is carried forward from 2026-08-16 on
the strength of `crates/spt/src/{parser,tag,stream,version,scene}.rs` being
**byte-identical since 2026-06-09** (`git log` on those five paths returns
`67e1baaf`, 2026-06-09, as the most recent touch). Dimensions 1, 4 (magic /
variant half) and 5 are dedup carry-forwards for that reason and are marked as
such below.

**Method**: diffed direction against `AUDIT_SPEEDTREE_2026-08-16.md`, then
re-derived the *consumer* side from current source. Delta emphasis per the
briefing: the water work in session 70 shared its wind model with SpeedTree, so
the shared ceiling/clamp was checked against **both** consumers rather than the
one it was tuned for. Every finding below was re-read at HEAD (`bb0b92f2`) and
an attempt made to disprove it.

---

## Dedup pass (mandatory)

Cached issues: `/tmp/audit/issues.json` (400, all states, **#2671–#3103 only** —
older numbers are carried on the prior report's word, per the briefing).

| Issue | State | Disposition this cycle |
|---|---|---|
| **#3076** (SPT-D3-2026-08-16-01) — ".spt placeholder billboards never face the camera" | **CLOSED** | **Fix verified at HEAD.** `placeholder_billboard_mesh` now sets `billboard_mode: Some(BILLBOARD_MODE_BS_ROTATE_ABOUT_UP)` on the *mesh* (`crates/spt/src/import/mod.rs:336`) and `placeholder_root_node(/* billboard */ false)` leaves the root a plain anchor (`:158`). `mesh_instance.rs:640` therefore fires the per-mesh `Billboard` attach on the entity that also carries `MeshHandle`, and `references/import.rs:377-381` now derives `placement_root_billboard` from a `None`, so `spawn.rs:778` no longer stamps the non-renderable root. The loose route is covered symmetrically by `nif_loader.rs:978`. Guard `placeholder_uses_default_size_without_bounds` pins both halves (`import/mod.rs:381,385`). **The carry-over question in the dispatch is answered: billboards do face the camera at HEAD.** |
| **#3077** (SPT-D2-2026-08-16-01) — dead tag-4003 leaf-texture tier | **CLOSED** | Fix verified: `is_relative_texture_path` (`import/mod.rs:343-350`) rejects drive-letter and leading-`\`/`/` paths, and the tier-2 lookup is `.filter(is_relative_texture_path)` (`:140`). Tier 3 ("leave unset → renderer placeholder") is reachable again. |
| **#3078** (SPT-D1-2026-08-16-01) — fatal `parse_spt` err discards a recoverable placeholder | **CLOSED** | Fix verified **on the cell route only** (`references/import.rs:305-311`, `Err(_) => SptScene::default()`). The loose `--tree` route still `return None`s. Filed below as SPT-D4-2026-08-20-01 (partial fix, not a regression). |
| **#3079** (SPT-D3-2026-08-16-02) — skill points at `references/mod.rs`, dispatch lives in `synth_child.rs` | **OPEN** | Re-verified still true at HEAD (`grep 'eq_ignore_ascii_case("spt")'` → `synth_child.rs:514` and `nif_loader.rs:203`, zero hits in `references/mod.rs`). Noted and skipped. |
| **#3080** (SPT-D2-2026-08-16-02) — `import/mod.rs` docstring documents the pre-#1001/#1002 two-tier size chain | **OPEN** | Re-verified still true at HEAD (`import/mod.rs:22-23`). Noted and skipped. |
| **#1822** (SPT-NEW-07) — tag-13005 tail-swallow misparse | OPEN (pre-#2671, carried) | `parser.rs` untouched since 2026-06-09. Unchanged, not re-reported. |
| #994–#1002, #1214, #1235, #1594, #1707, #1711, #1715, #1819, #1820, #1821, #2206, #2409, #2426 | CLOSED (pre-#2671, carried) | Treated as regression guards; guard table below. |
| #1374 (billboard dirty-set cost) | CLOSED (pre-#2671, carried) | **Its guarantee has been weakened by this delta** — see SPT-D2-2026-08-20-02. |
| #2923 (hot-path Fx-hashing convention) | CLOSED (pre-#2671, carried) | New off-convention site introduced this delta — folded into SPT-D3-2026-08-20-02 (relayed from `/audit-performance`, confirmed not re-derived). |

No open issue covers any of the seven findings below.

---

## Change window

`crates/spt/src/import/mod.rs` changed 3× since 08-16 (`aee8783f`, `73896726`,
`4ddf7062`). The other five crate files: **unchanged since 2026-06-09**.

`byroredux/src/systems/billboard.rs` changed **8×**, all inside this delta —
`73896726`, `a6c0a5c9`, `4ddf7062`, `6f67c79b`, `07e8a972`, `0304538c`,
`6096f19f`, `1a428278` — growing 146 → 465 LOC. `crates/plugin/src/esm/records/tree.rs`
(+8), `crates/core/src/ecs/components/billboard.rs` (+26, the new `SpeedTreeWind`
component), `references/import.rs` (+2), `spawn.rs` (+5),
`spawn/mesh_instance.rs` (+59), `boot.rs` (+2).

Net effect: SpeedTree acquired a **wind response** driven by the same
`WindField` resource that now drives water's weather scroll and wave scale.
That shared model is where five of this cycle's seven findings sit.

### The shared wind model, as it stands at HEAD

| Consumer | Site | Gust | Non-finite guard | Trough clamp | Normalised by |
|---|---|---|---|---|---|
| Water (render) | `byroredux/src/render/water.rs:99-133` | `speed + amp·sin(t·f·τ)` | **yes** (`is_finite` → `0.0`) | `gust.max(0.0)` | `MAX_WIND_SPEED` for wave scale; raw `gust` for scroll |
| Water (physics) | `crates/physics/src/water.rs:328-348` | same | **yes** | `gust.max(0.0)` | same |
| SpeedTree | `byroredux/src/systems/billboard.rs:168-188` | same | **no** | implicit, via `.clamp(0.0, 1.0)` | `MAX_WIND_SPEED` |

`MAX_WIND_SPEED = 220.0` (`crates/core/src/ecs/components/groundcover.rs:299`)
is documented as a **ground-cover blade-scale calibration parameter**, not a
physical ceiling; it is now the shared normaliser for three subsystems. That is
defensible as a shared unit, and both the direction fallback (`[1,0]`) and the
one-sided-magnitude contract are genuinely consistent across all three. The
**non-finite** column is not — see SPT-D2-2026-08-20-03.

---

## Findings

### SPT-D3-2026-08-20-01: `SpeedTreeWind` is built from two CNAM floats whose meaning the parser itself documents as unpinned — and the chosen clamp silences sway on any tree whose second float is ≥ 1.0

- **Severity**: MEDIUM
- **Dimension**: TREE→Billboard Wiring (secondary: Placeholder Fallback)
- **Location**: `byroredux/src/cell_loader/references/import.rs:335-345`,
  `crates/plugin/src/esm/records/tree.rs:25-28,93-97`,
  `byroredux/src/systems/billboard.rs:182-184`,
  `crates/core/src/ecs/components/billboard.rs:93-113`
- **Status**: NEW (introduced by `4ddf7062`)
- **Description**: The delta added a wind response sourced straight off the
  TREE record's `CNAM` array:

  ```rust
  let response  = values.next()?.max(0.0);          // canopy_params[0]
  let stiffness = values.next()?.clamp(0.0, 1.0);   // canopy_params[1]
  ```

  and consumes it as
  `bend = strength * 0.16 * response * (1.0 - stiffness)`
  (`billboard.rs:184`).

  Nothing in the repository establishes that `CNAM[0]` is a wind-response
  multiplier or that `CNAM[1]` is a normalised stiffness. The record's own
  module docstring says the opposite, twice:
  *"`CNAM` — canopy shadow / wind parameters as a contiguous f32 array. Field
  count varies per game (5 floats Oblivion, 8 floats FO3/FNV); **semantics not
  pinned down here** — we surface the raw values for the future SpeedTree
  runtime to interpret."* (`tree.rs:25-28`). The parser's cited upstream
  reference, OpenMW's `components/esm4/loadtree.cpp`, `skipSubRecordData()`s
  `CNAM` outright (verified in `/mnt/data/src/reference/openmw/`), so it
  supplies no field layout either. This is exactly the pattern the project's
  **No Guessing Policy** exists to prevent: a heuristic derived from field
  *position* rather than from a documented layout.

  The guess is not neutral, because of the clamp. `stiffness` is
  `.clamp(0.0, 1.0)` and enters the bend as `(1.0 - stiffness)`. **Any tree
  whose `CNAM[1]` is ≥ 1.0 gets `bend == 0` — no sway at all**, silently and
  with no diagnostic. If `CNAM[1]` is an angle in degrees, a count, or a
  dimming value (all plausible for a "canopy" struct), that is every tree in
  the game. Symmetrically, `response` is clamped to `[0, 4]` at
  `billboard.rs:182`, so a `CNAM[0]` of 4.0 with `CNAM[1]` of 0.0 yields
  `bend = 0.64 rad` — a **±37° trunk swing**, which is not a tree, it is a
  windscreen wiper. A single record field, read from an unpinned slot, spans
  "completely inert" to "grossly wrong" with nothing in between.

  Secondary defect in the same expression: the `.filter(|v| v.is_finite())`
  runs **before** `.take(2)`, so the field *indices* consumed are
  data-dependent. A non-finite `CNAM[0]` silently promotes `CNAM[1]` to
  `response` and `CNAM[2]` to `stiffness` — the mapping shifts rather than
  rejecting the record.
- **Evidence**:
  - `references/import.rs:335-345` — the mapping, including the
    filter-then-take ordering.
  - `tree.rs:25-28` and `:93-97` — "semantics aren't pinned down here", in the
    same file, describing the same field the consumer now interprets.
  - `/mnt/data/src/reference/openmw/components/esm4/loadtree.cpp` — `CNAM`
    falls in the `skipSubRecordData()` arm; `loadtree.hpp` stores only
    `mEditorId`, `mModel`, `mBoundRadius`, `mLeafTexture`.
  - **The repo's own fixture disproves the mapping.** `tree.rs:250`,
    docstring *"Realistic FNV TREE record … CNAM carries 8 floats. This is the
    modal vanilla shape across `FalloutNV.esm`"*, uses
    `[0.5, 1.0, 0.7, 2.5, 1.2, 0.3, 0.4, 1.0]` → `stiffness = 1.0` →
    `bend = 0`. The Oblivion fixture (`:288`) uses
    `[0.4, 0.9, 0.6, 1.8, 1.0]` → `stiffness = 0.9` → 90 % of the response
    thrown away. The only two CNAM value sets anywhere in the repository both
    land in the "feature does nothing" regime.
  - The only end-to-end test,
    `parse_and_import_spt_preserves_tree_cnam_wind_response`
    (`references/import_tests.rs:74-83`), hand-picks `canopy_params:
    vec![2.0, 0.25]` — values chosen so the feature works, not drawn from
    game data. Every `billboard.rs` test likewise constructs
    `SpeedTreeWind::new(1.0, 0.0)` directly, so nothing exercises the
    `canopy_params` → `SpeedTreeWind` edge against real content.
- **Impact**: The delta's headline vegetation feature is driven by
  misattributed record data across FNV / FO3 / Oblivion — the three games that
  ship `.spt` trees at all. Depending on what `CNAM[1]` actually holds, either
  the entire canopy response is inert on vanilla content (and the eight
  billboard commits bought nothing visible), or trees swing at up to ±37°.
  Neither failure produces a log line. This is a compatibility-data
  correctness defect, not a crash.
- **Related**: #3076 (the wiring this rides on), TD5-011 (the
  parse-but-don't-consume gate that `CNAM` was explicitly *inside* until this
  delta), the No-Guessing Policy.
- **Suggested Fix**: Do not infer the layout. Pin `CNAM` against a citable
  source first — the TES4 CS / GECK Tree dialog exposes the struct field-by-field
  and is the natural authority, and a corpus histogram over `FalloutNV.esm` /
  `Fallout3.esm` / `Oblivion.esm` will disambiguate the 5-float vs 8-float
  split — then record the layout in `crates/plugin/src/esm/records/tree.rs`'s
  docstring and read the *named* wind fields. Until that lands, either (a) gate
  `SpeedTreeWind` behind a neutral default `(1.0, 0.0)` for every record and
  drop the `CNAM` read, or (b) reject rather than clamp: treat `CNAM[1] > 1.0`
  as "this slot is not a normalised stiffness" and fall back to the default
  instead of silently producing a rigid tree. Move `.take(2)` before the
  finiteness filter so the field indices are positional. Add a corpus-gated
  test asserting the derived `(response, stiffness)` distribution over real
  TREE records is not degenerate.

---

### SPT-D2-2026-08-20-01: the wind bend is composed in the *object-local* frame but weighted by *world-space* wind components — on the only reachable consumer (a camera-facing billboard) that frame is the view axis

- **Severity**: MEDIUM
- **Dimension**: Placeholder Fallback (secondary: TREE→Billboard Wiring)
- **Location**: `byroredux/src/systems/billboard.rs:161-189` (`apply_speedtree_wind`),
  called from `:120-126` (billboard arm) and `:146` (geometry arm);
  `crates/spt/src/import/mod.rs:296-312` (quad layout / pivot)
- **Status**: NEW (introduced by `4ddf7062`, weighting added by `6f67c79b`)
- **Description**: `apply_speedtree_wind` returns

  ```rust
  base * Quat::from_rotation_z(wave * bend * along_weight)
       * Quat::from_rotation_x(cross * bend * 0.65 * cross_weight)
  ```

  Post-multiplication means both rotations are applied in the **object frame**,
  before `base`. The weights that select between them are world-space:
  `along_weight = |wind_dir.x|` (world X) and `cross_weight = |wind_dir.y|`
  (world Z), at `:185-186`.

  That pairing is coherent for the **geometry** consumer, where `base` is the
  authored near-identity world rotation: local Z ≈ world Z, so `Rz` leans the
  trunk along ±X and `Rx` leans it along ±Z, each weighted by the matching
  wind component. It is **not** coherent for the **billboard** consumer, where
  `base = Quat::from_rotation_arc(-Z, look_dir)` (`:243`) maps object `-Z` onto
  the direction of the camera. Object-local Z is therefore the **view axis**,
  and:

  - `Rz` becomes a **roll in the screen plane**, pivoting at the quad's origin
    (the trunk base — the quad spans `y ∈ [0, height]`, `x ∈ ±w/2`, `z = 0`,
    `import/mod.rs:286-290`). It reads as a lean, but a lean whose on-screen
    direction is fixed by the camera, not by the wind. Orbit the tree and the
    lean stays put; stand downwind, where a real tree leans away from you and
    shows almost no lateral motion, and it still sways left-right.
  - `Rx` becomes a **pitch about the screen-horizontal axis**, tipping the flat
    card toward or away from the viewer. A zero-thickness card tipped about its
    base does not lean — it **foreshortens**, so the tree's apparent height
    pulses with the gust.

  Independently of the frame issue, both weights are `.abs()`, so the wind's
  **sign never reaches the bend**. `wave` is a zero-mean sine, so there is no
  mean lean at all: reversing the wind direction changes only `phase`
  (`:177`, a travelling-wave offset), never which way the tree leans. Real wind
  produces a sustained lean plus oscillation about it; this produces
  oscillation about vertical whose amplitude depends only on `|wind_dir|`
  components, with a `.max(0.25)` floor that keeps a cross-bend alive even when
  the wind is exactly axis-aligned.

  The existing test asserts only that direction `[1,0]` and `[0,1]` give
  *different* results (`billboard.rs:307-314`) — which the differing
  `along_weight`/`cross_weight` satisfy — so it cannot distinguish a correct
  directional lean from an amplitude-only reweighting.
- **Evidence**:
  - `billboard.rs:185-188` — the `.abs()` weights and the post-multiplied
    local-frame rotations.
  - `billboard.rs:255-259` — `Quat::from_rotation_arc(from, look_dir)` with
    `from = -Vec3::Z`, establishing object `-Z` = toward camera, hence object
    `Z` = view axis.
  - `import/mod.rs:286-290` — quad vertices with origin at bottom-centre,
    which is what makes `Rz` read as a base-pivoted lean and `Rx` read as
    foreshortening.
  - `billboard.rs:172-176` — `wave`/`cross` are zero-mean `sin`/`cos`; no mean
    term exists anywhere in the expression.
- **Impact**: Visual fidelity on every `.spt` tree in three games whenever
  weather wind is non-calm. The sway is view-locked (wrong as the camera
  orbits) and gust-synchronised vertical pulsing is visible on the card. This
  is the delta-emphasis answer the dispatch asked for: the shared wind model
  is tuned for the *geometry* consumer and applied unchanged to the *billboard*
  consumer, which is the only one that is actually reachable today (see
  SPT-D3-2026-08-20-02).
- **Related**: SPT-D3-2026-08-20-02 (the geometry consumer this model *was*
  written for is dead), #1000 (the `-Z` front-face convention this interacts
  with), #1715.
- **Suggested Fix**: Build the bend in world space and conjugate it into the
  entity's frame, rather than post-multiplying object-local axis rotations —
  i.e. form the lean as a rotation about the world-horizontal axis
  perpendicular to `wind_dir` (`axis = (−wind_dir.y, 0, wind_dir.x)`) and
  pre-multiply: `Quat::from_axis_angle(axis, angle) * base`. Carry the wind's
  sign by giving the lean a non-zero mean (`mean + osc·sin(...)`) instead of
  weighting a zero-mean sine with `.abs()` magnitudes. Guard with a test that
  orbits the camera 180° around a fixed tree under fixed wind and asserts the
  world-space lean direction is unchanged, plus one asserting `dir` and `−dir`
  produce opposite mean leans.

---

### SPT-D2-2026-08-20-02: the #1374 camera-parked early-out is now bypassed on every frame of every windy exterior, and the loop then re-dirties *every* `Billboard` entity, not just the SpeedTree ones

- **Severity**: MEDIUM
- **Dimension**: Placeholder Fallback (secondary: TREE→Billboard Wiring)
- **Location**: `byroredux/src/systems/billboard.rs:53-92,96-132`;
  regression against `byroredux/src/systems/billboard.rs` as of `73896726~1`
- **Status**: NEW (regression of #1374's guarantee, introduced by `73896726`,
  broadened by `0304538c`)
- **Description**: The #1374 gate existed to stop `gq.get_mut(entity)` arming
  `GlobalTransform`'s TRACK_CHANGES dirty set for every billboard entity on a
  frame where nothing moved — the file's own header comment (`:14-21`) says so:
  *"Without this gate, `world_bound_propagation`'s incremental-bounds fast path
  was defeated every frame in billboard-heavy cells (vegetation impostors,
  sprite quads)."* Before this delta the gate was unconditional:

  ```rust
  if last_cam == Some((cam_pos, cam_forward)) { return; }   // 73896726~1:56
  ```

  It is now:

  ```rust
  if last_cam == Some((cam_pos, cam_forward)) && !wind_active && !wind_state_changed { return; }   // :90
  ```

  with `wind_active = wind.speed > 1.0e-4 || wind.gust_amplitude > 1.0e-4`
  (`:53`). `WindField` is installed for **every exterior worldspace**
  (`byroredux/src/scene/world_setup.rs:522-536`, `install_ground_cover`) from
  `WindField::from_weather_byte(WTHR.wind_speed, …)`, so any weather with a
  non-zero wind byte makes `wind_active` permanently true. The early-out then
  never fires outdoors, and the loop runs at full cost every frame with the
  camera stationary.

  Worse, the loop is not scoped to trees. `:98-132` iterates **all** `Billboard`
  entities and unconditionally does `gq.get_mut(entity)` followed by
  `global.rotation = new_rot`, applying the wind only afterwards and only when
  `tree_wind.is_some()`. So an FX impostor or sprite quad with no
  `SpeedTreeWind` is still re-fetched mutably and re-written with a *bit-identical*
  rotation every frame purely because the weather is windy. The write is
  redundant; the dirty-set arming is not.

  `wind_state_changed = last_wind != Some(wind)` (`:58`) is the correct fix for
  the stationary-camera-weather-transition case it was added for
  (`0304538c`) — the problem is that `wind_active` in the same condition makes
  it moot: the gate is already open.
- **Evidence**:
  - `billboard.rs:14-21` — the header comment stating the gate's purpose.
  - `git show 73896726~1:byroredux/src/systems/billboard.rs` line 56 vs HEAD
    line 90 — the exact widening.
  - `git log --reverse -S "wind_active" -- byroredux/src/systems/billboard.rs`
    → `73896726`, i.e. inside this delta window.
  - `scene/world_setup.rs:508-536` — `WindField` installed unconditionally for
    exteriors; `groundcover_translate.rs:266-274` — speed comes from the WTHR
    byte, so it is non-zero for most vanilla exterior weathers.
  - `billboard.rs:99-131` — `get_mut` + `global.rotation = new_rot` are
    outside the `tree_wind.is_some()` branch.
- **Impact**: In exactly the cells SpeedTree exists for — tree-heavy vanilla
  exteriors, hundreds of billboard entities — the incremental world-bound
  propagation fast path is defeated every frame whenever the weather is windy,
  including with the camera parked. This is the cost #1374 was closed to
  remove, now re-incurred by default and extended to non-SpeedTree billboards
  that gain nothing from it. CPU-side only; no correctness impact.
- **Related**: #1374 (the closed issue whose guarantee this weakens), #823,
  #829.
- **Suggested Fix**: Split the two motivations. When the camera has not moved,
  skip the full `Billboard` walk and update only the entities that actually
  need a wind refresh — iterate `SpeedTreeWind` (already queried at `:96`) and
  intersect with `Billboard`, rather than the reverse. Keep the unconditional
  full walk for the camera-moved case. Regression guard: a test that parks the
  camera under active wind with one `Billboard`-without-`SpeedTreeWind` entity
  and asserts its `GlobalTransform` is not re-written.

---

### SPT-D3-2026-08-20-02: the geometry-tree wind branch is unreachable — no production entity can carry `SpeedTreeWind` + `MeshHandle` without also carrying `Billboard` — and its per-frame cache is off the #2923 hashing convention

- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring
- **Status**: NEW (introduced by `6096f19f`)
- **Location**: `byroredux/src/systems/billboard.rs:31-36,135-154`,
  `byroredux/src/cell_loader/spawn/mesh_instance.rs:640-650`,
  `byroredux/src/cell_loader/spawn.rs:778-783`,
  `crates/spt/src/import/mod.rs:336`,
  `byroredux/src/cell_loader/references/import.rs:395`
- **Description**: `6096f19f` added a second loop for "full SpeedTree geometry
  (rather than billboard impostors)", predicated on entities that carry
  `SpeedTreeWind` and `MeshHandle` but **not** `Billboard` (`:141`). No such
  entity can be constructed today:

  - `SpeedTreeWind` has exactly two production insert sites,
    `spawn.rs:782` (the placement root) and `mesh_instance.rs:649` (the mesh
    entity). Both read `cached.speedtree_wind`.
  - `cached.speedtree_wind` is `Some` at exactly one construction site,
    `references/import.rs:395` — the `.spt` route. Every other
    `CachedNifImport` constructor (`partial.rs:115`, `precombined.rs:776`,
    `import.rs:185`) hard-codes `None`.
  - On that route the imported scene has exactly one mesh, and
    `import/mod.rs:336` gives it `billboard_mode: Some(...)`, so
    `mesh_instance.rs:640` always attaches `Billboard` to the same entity that
    `:649` attaches `SpeedTreeWind` to.
  - The placement root gets `SpeedTreeWind` but never a `MeshHandle`, so it
    fails the `mesh_q.contains` half instead.

  The branch is therefore dead: `continue` fires for every entity `swq.iter()`
  yields. Skyrim+ TREE records point `MODL` at a `.nif` and never reach
  `parse_and_import_spt`, so no NIF path supplies the marker either.

  Two consequences worth recording rather than filing separately:

  1. **Off-convention hashing (relayed from `/audit-performance`, confirmed
     not re-derived).** `geometry_bases: HashMap<u32, Quat>`
     (`billboard.rs:9,36`) is `std::collections::HashMap`, i.e. SipHash, in a
     `PostUpdate` per-frame system. The repository convention established by
     #2923 for entity-keyed per-frame maps is `rustc_hash::FxHashMap`
     (`byroredux/src/main.rs:159-164` states it explicitly; also
     `render/skinned.rs`, `render/static_meshes.rs`, `interaction.rs:654`,
     `crates/core/src/ecs/resources/skin_slot_pool.rs:78-116`). The
     `retain` at `:153` also runs unconditionally on every frame the system
     body executes. Both are currently free *because the map is always empty* —
     which is precisely the point: the cost is latent, not absent, and lands
     the moment the branch becomes live.
  2. **Latent stale-base hazard.** `geometry_bases.entry(entity).or_insert(global.rotation)`
     (`:144`) snapshots the authored rotation the first frame an entity is
     seen and never refreshes it. `retain` (`:153`) prunes only ids that have
     lost `MeshHandle` or `SpeedTreeWind`, so an entity id recycled within a
     single cell transition (despawn + respawn between two runs of this
     system) inherits the previous tree's base pose. Any later legitimate
     rewrite of the entity's `GlobalTransform` (parent motion, structural
     rebuild) is likewise ignored in favour of the cached base.
- **Evidence**: `grep -rn "SpeedTreeWind" byroredux/src crates/` — exactly two
  non-test insert sites; `grep -rn "speedtree_wind"` — exactly one `Some`
  producer. `import/mod.rs:336` + `import/mod.rs:381,385` (the guard asserting
  node `None` / mesh `Some`) close the argument.
- **Impact**: Dead code in a hot per-frame system, carrying a cache whose
  hashing and invalidation are both wrong for the moment it becomes reachable.
  No present runtime effect. Its real cost is diagnostic: the branch's presence
  reads as "geometry trees are handled", which is the sort of claim that gets
  inherited by the next cycle's report.
- **Related**: #2923 (hashing convention), SPT-D2-2026-08-20-01 (the wind model
  is coherent only for this dead consumer), `/audit-performance` 2026-08-20.
- **Suggested Fix**: Either wire a real producer (Skyrim+ `.nif` trees are the
  obvious candidate and would make the branch the *correct* consumer of the
  current wind model) or remove the branch until one exists. If it stays,
  switch `geometry_bases` to `rustc_hash::FxHashMap`, key it on the entity's
  generation as well as its index, and refresh the base when the entity's
  `Transform` is dirty rather than caching for the entity's lifetime.

---

### SPT-D2-2026-08-20-03: SpeedTree is the one wind consumer with no non-finite guard, while both water consumers document themselves as "matching the SpeedTree contract"

- **Severity**: LOW
- **Dimension**: Placeholder Fallback
- **Status**: NEW
- **Location**: `byroredux/src/systems/billboard.rs:168-171`,
  `byroredux/src/render/water.rs:99-106`,
  `crates/physics/src/water.rs:328-334`,
  `crates/core/src/ecs/components/groundcover.rs:272-284`
- **Description**: Both water consumers sanitise the instantaneous gust:

  ```rust
  let gust = if gust.is_finite() { gust.max(0.0) } else { 0.0 };
  ```

  and both annotate that line as deference to SpeedTree —
  `render/water.rs:102-105`: *"SpeedTree treats a negative instantaneous gust
  as calm weather by clamping its bend strength to zero. Keep water's UV drift
  on that same one-sided magnitude contract"*; `physics/water.rs:329-332`:
  *"Match the renderer and SpeedTree wind contract."* Half of that is true —
  `strength = (gust / MAX_WIND_SPEED).clamp(0.0, 1.0)` (`billboard.rs:170-171`)
  does floor negatives at zero. The other half is not: Rust's `f32::clamp`
  returns `NaN` for a `NaN` input (it is `if self < min … if self > max …`,
  both false for `NaN`), so a non-finite gust propagates straight through
  `strength` → `bend` → `Quat::from_rotation_z` → `GlobalTransform.rotation`,
  poisoning the entity's world transform and everything downstream that reads
  it (bounds propagation, instance upload, BLAS/TLAS transforms).

  A second-order symptom of the same input: `wind_state_changed =
  last_wind != Some(wind)` (`:58`) is derived-`PartialEq`, so a `NaN` anywhere
  in `WindField` makes it permanently `true` and the camera gate never closes.

  This is **not reachable in production today**: the only production producer is
  `WindField::from_weather_byte` (`groundcover.rs:260-269`) via
  `resolve_wind` (`groundcover_translate.rs:266-274`), which derives every field
  from a `u8` and a `cos`/`sin` pair — all finite, and with
  `gust_amplitude = speed·(0.25 + 0.55·n) ≤ 0.8·speed` the gust cannot even go
  negative. `WindField::is_well_formed` (`groundcover.rs:275-286`) exists
  precisely to catch a malformed field at the translation boundary, and has
  **zero callers**. So the guard asymmetry is a defense-in-depth gap plus a
  documentation defect (two comments assert a contract the third site does not
  honour), not a live bug.
- **Evidence**: the three sites above; `grep -rn "is_well_formed"` returns only
  the definition and its own unit tests.
- **Impact**: A hand-authored, modded, or future procedurally-driven
  `WindField` with a non-finite field silently produces `NaN` world rotations
  on every tree, where the same input produces calm water. The renderer is
  documented as hard-failing non-finite environment values (EX-05, quoted in
  `groundcover.rs:277-279`), so this would surface as a validation abort or
  garbage geometry rather than a graceful degrade.
- **Related**: SPT-D2-2026-08-20-01, EX-05.
- **Suggested Fix**: Apply the water sites' guard verbatim in
  `apply_speedtree_wind`, or — better, since three subsystems now share the
  field — hoist it: call `WindField::is_well_formed` at the single install site
  (`world_setup.rs:536`) and substitute `WindField::CALM` when it fails, so all
  three consumers inherit one sanitised value and the two water-side comments
  become true.

---

### SPT-D4-2026-08-20-01: #3078's recoverable-placeholder fix landed on the cell route only — the loose `--tree` route still deletes the tree on a parse error, and never attaches `SpeedTreeWind` at all

- **Severity**: LOW
- **Dimension**: Per-Game Variants & Route Divergence
- **Status**: NEW (partial fix of the CLOSED #3078; not a regression)
- **Location**: `byroredux/src/scene/nif_loader.rs:205-236` vs
  `byroredux/src/cell_loader/references/import.rs:305-311`
- **Description**: #3078 established the contract that a malformed `.spt`
  parameter section must not erase the tree, since the placeholder needs
  nothing from the parse except an optional relative tag-4003 path. The cell
  route now honours it:

  ```rust
  Err(e) => {
      log::warn!("Failed to parse SPT '{}': {}", label, e);
      // TREE metadata is sufficient for the placeholder. A malformed
      // parameter section must not erase the REFR (#3078).
      byroredux_spt::SptScene::default()
  }
  ```

  The loose `--tree` / `--mesh foo.spt` visualiser route does not:

  ```rust
  Err(e) => {
      log::error!("Failed to parse SPT '{}': {}", label, e);
      return None;
  }
  ```

  Both routes still call `parse_spt` + `import_spt_scene` (the skill's
  Dimension-4 parse-parity requirement holds); it is the error arm that
  diverged. The loose route is also now the *less* capable of the two in a
  second way: `SpeedTreeWind` is attached only from `cached.speedtree_wind`
  (`mesh_instance.rs:648`), which is a cell-loader concept — the loose route
  builds no `CachedNifImport`, so a `--tree`-loaded `.spt` gets its `Billboard`
  (`nif_loader.rs:978`) but never a `SpeedTreeWind`, and consequently cannot
  exercise the wind path this delta added.
- **Evidence**: the two arms above; `grep -rn "SpeedTreeWind" byroredux/src`
  shows no `nif_loader.rs` hit.
- **Impact**: The `.spt` visualiser — the tool a developer reaches for when a
  tree is wrong — is the one path that still fails closed on a malformed file,
  and is structurally unable to reproduce the wind behaviour it would be used
  to debug. Dev-workflow only; no shipped-content impact.
- **Related**: #3078, #3076, SPT-D3-2026-08-20-01.
- **Suggested Fix**: Mirror the cell route's `SptScene::default()` fallback in
  `nif_loader.rs` (downgrading the `error!` to `warn!`), and attach a default
  `SpeedTreeWind::new(1.0, 0.0)` on the loose `.spt` branch so `--tree`
  exercises the same system the cell route does.

---

### SPT-D3-2026-08-20-03: the billboard system's `Access` declaration omits the `TotalTime` resource read added in this delta

- **Severity**: LOW
- **Dimension**: TREE→Billboard Wiring
- **Status**: NEW
- **Location**: `byroredux/src/boot.rs:1182-1191`,
  `byroredux/src/systems/billboard.rs:48-51`
- **Description**: The delta added a `TotalTime` read so SpeedTree gust phase
  shares water's clock:

  ```rust
  let wind_time = world.try_resource::<TotalTime>().map(|t| t.0).unwrap_or(elapsed);
  ```

  `6096f19f` extended the system's declared access with `SpeedTreeWind` and
  `MeshHandle` but not `TotalTime`, so the declaration at `boot.rs:1183-1190`
  lists `ActiveCamera`, `WindField`, `Billboard`, `SpeedTreeWind`, `MeshHandle`
  and `writes GlobalTransform` — no `TotalTime`. The sibling `submersion_system`
  registered 30 lines below declares exactly that resource
  (`boot.rs:1224`). The registration's own comment (`boot.rs:1175-1181`, #2391)
  states the reason these exclusives declare access at all: *"a blank
  `sys.accesses` row is exactly the wrong place for that to be invisible."*
  `scheduler_access_tests.rs` pins several systems' resource reads
  (`late_telemetry_declarations_read_all_their_resources`,
  `camera_follow_declaration_reads_player_mode`) but has no billboard case, so
  nothing catches the omission.
- **Evidence**: `boot.rs:1183-1190` vs `billboard.rs:48-51`; `boot.rs:1224` for
  the sibling that declares it.
- **Impact**: Documentation/analysis only — the scheduler does not pair
  exclusives, so there is no live scheduling consequence. The declared-access
  rows are the project's authority for "who touches what when", and one is now
  incomplete.
- **Related**: #2391.
- **Suggested Fix**: Add `.reads_resource::<byroredux_core::ecs::resources::TotalTime>()`
  to the billboard registration, and add a `scheduler_access_tests.rs` case in
  the style of `late_telemetry_declarations_read_all_their_resources`.

---

## Dimension summary (every dimension enumerated)

| Dimension | Findings | Verdict / basis this cycle |
|---|---:|---|
| 1 — Walker Byte-Accounting | **0** | **Carry-forward.** `parser.rs` / `stream.rs` byte-identical since 2026-06-09 (`67e1baaf`), i.e. untouched across two audit cycles. Spot-checked at HEAD: the `ArrayBytes` cap is still on `count.saturating_mul(stride)` bytes not on `count` (`parser.rs:158-171`), the `MaybeStringElseBare` peek still treats `None` as "not a known tag" and re-syncs on both arms (`:102-118`), `reached_eof`/`tail_offset` still set on the clean exit (`:134-136`), LE-only throughout. The 2026-08-16 corpus gate (FO3 100 %, FNV 100 %, OBL 96.46 %) is inherited, not re-run — see the Depth note. #1822 remains the one open item. |
| 2 — Placeholder Fallback | **3** (2 MEDIUM, 1 LOW) | Structure intact: `import_spt_scene` still has no `Err` path, `compute_billboard_size` still **OBND → BNAM → MODB → 256×512** with the `[16, 8192]` clamp on every tier (`import/mod.rs:226-249`), `bs_bound` Z-up→Y-up still via `zup_to_yup_pos` with `(hx, hz, hy)`, `-Z` normals + `[0,3,2,2,1,0]` winding unchanged, `two_sided`/`alpha_test`/`0.5`/func `6`/`has_alpha:false` unchanged. `BsRotateAboutUp` still falls back to the world-up yaw lock and the comment still says so (`billboard.rs:234-246`). All three findings are in the **new** wind layer bolted onto this dimension, not the placeholder itself. |
| 3 — TREE→Billboard Wiring | **3** (1 MEDIUM, 2 LOW) | **#3076 is genuinely fixed** — the `Billboard` now lands on the `MeshHandle`-carrying entity on both routes; the dispatch's carry-over question is answered affirmatively. `CachedNifImport` synthetic defaults (`bsx_flags = 0`, `root_flags = 0`, `flame_attach_offset: None`, `attach_points: None`, `furniture: None`, `collision_authoring: Default::default()`) all still correct. `TreeRecord` capture still lossless and CNAM-length-tolerant across the 5/8-float split. Findings are all in the new `SpeedTreeWind` edge: its *source* (CNAM slots), its *reachability* (dead geometry branch), and its *declaration* (missing `TotalTime`). |
| 4 — Per-Game Variants & Route Divergence | **1** (LOW) | **Carry-forward** on the variant half: `version.rs` untouched since 2026-06-09, `MAGIC_HEAD` still the exact 20 bytes, `detect_variant` still has exactly two callers and both are `log::debug!`-only (`references/import.rs:281-294`, `nif_loader.rs:205-215`) — nothing branches on the variant. Both routes still call `parse_spt` + `import_spt_scene`. The new divergence is the error arm plus the missing `SpeedTreeWind` on the loose route. |
| 5 — Tag Dictionary | **0** | **Carry-forward.** `tag.rs` untouched since 2026-06-09; the 2026-08-16 spot-check (8003/8005/8009 = 52 B, 13008 = 11 B, 13013 = 7 B, 12002 = 16 B, 12003 = 20 B, `10002` stride 1, `10003` stride 8, confounders `4096`/`5376`/etc. → `Unknown`) stands unmodified. Dictionary size is not a gap (skill §Dimension 5). |
| 6 — NIFAL Material Translation | **0** | Single boundary preserved: both routes still reach `translate_material` (`mesh_instance.rs:681-692`, `nif_loader.rs:985-996`) — no parallel "spt material" path, and the new wind layer touches only `GlobalTransform`, never `Material`. Placeholder material defaults unchanged at `import/mod.rs:308-325`: `metalness_override: Some(0.0)`, `roughness_override: Some(0.85)` (the #1819 guard against the Boxwood→wood / Elderberry→glass keyword collision), `alpha_test: true` / `0.5` / func `6`, `two_sided: true`, everything else `ImportedMaterial::default()` → `is_pbr: false`, `from_bgsm: false`, `emissive_source: None`. The new `is_relative_texture_path` filter (#3077) narrows what reaches `texture_path`, which strictly *reduces* the keyword-classifier surface. |

**Totals**: 6 dimensions, 7 findings.

---

## Regression guards (verified in place, NOT re-reported)

| Guard | Issue | Verified this cycle |
|---|---|---|
| `.spt` billboard mode rides on the **mesh**, root is a plain anchor | **#3076** | `import/mod.rs:158,336` + guard `placeholder_uses_default_size_without_bounds` (`:381,385`); `mesh_instance.rs:640`; `nif_loader.rs:978` |
| Absolute tag-4003 exporter paths rejected, tier-3 placeholder reachable | **#3077** | `import/mod.rs:140,343-350` |
| Malformed `.spt` still yields a placeholder on the cell route | **#3078** | `references/import.rs:305-311` (loose route excepted — SPT-D4-2026-08-20-01) |
| `bs_bound` Z-up → Y-up via `zup_to_yup_pos`, half-extents `(hx, hz, hy)` | #995 | `import/mod.rs:181-194` |
| `SptImportParams.wind` doc names CNAM, not BNAM | #996 | `import/mod.rs:71-78` (now *consumed*, see SPT-D3-2026-08-20-01) |
| "First wins" on duplicate tag 4003 | #997 | `import/mod.rs:131-143` |
| SHA-pinned synthetic regression fixture | #998 | `crates/spt/tests/parse_synthetic_spt.rs` present |
| Tag 13005 bimodal disambiguation | #999 | `parser.rs:84-120` unchanged; residual edge = #1822 |
| `-Z` normals + `[0,3,2,2,1,0]` winding | #1000 | `import/mod.rs:292-296` + both winding guards present |
| MODB drives Oblivion size / OBND beats BNAM | #1001/#1002 | `compute_billboard_size` four-tier chain intact (`:226-249`) |
| `bsx_flags = 0` / `root_flags = 0` synthetic defaults | #1214/#1235 | `references/import.rs:398,401` |
| Crate docstring reflects shipped scope | #1707 | `crates/spt/src/lib.rs` unchanged |
| Foliage PBR overrides beat the keyword classifier | #1819 | `import/mod.rs:330-332` |
| `detect_variant` has production callers, log-only | #1820 | `references/import.rs:287`, `nif_loader.rs:205` |
| Per-mesh `Billboard` attach (cell + loose) | #2206/#2527 | `mesh_instance.rs:640`, `nif_loader.rs:978` — now the `.spt` path's *primary* attach |
| `is_spt` dispatch intact after the #2409 split | #2409 | `synth_child.rs:510-522` |
| `collision_authoring: Default::default()` fabricates no TREE collider | (no issue) | `references/import.rs:390` |

**Still open, noted and skipped**: #1822 (tag-13005 tail swallow), #3079 (skill
entry-point path drift — re-confirmed true at HEAD), #3080 (`import/mod.rs`
two-tier size docstring — re-confirmed true at HEAD, `:22-23`).

---

## Candidates raised and disproved (not reported)

1. **"`MAX_WIND_SPEED = 220.0` is a ground-cover constant being misused as a
   water/vegetation ceiling."** It is documented as a calibration parameter
   (`groundcover.rs:293-299`), but all three consumers use it identically — as
   the denominator of a `[0,1]` normalisation of the same `WindField.speed`
   whose own producer caps at exactly `220.0`
   (`from_weather_byte`: `n·MAX_WIND_SPEED`, `n ≤ 1`). It is a shared *unit*,
   consistently applied. The gust *can* exceed it (crest `≤ 1.8·speed`), so all
   three saturate at 1.0 in strong weather — but they saturate together, which
   is the property the sharing was for. Not a finding.
2. **Direction-handling divergence between water and SpeedTree.** Both
   normalise with a `length_squared() > 1e-6` test and both fall back to
   `+X` (`billboard.rs:172-177`, `render/water.rs:107-124`,
   `physics/water.rs:335-341`). `Vec2::NAN.length_squared() > 1e-6` is `false`,
   so even the non-finite case degrades identically. Genuinely consistent.
   Disproved.
3. **`wind_state_changed` misses a transition because `WindField` is `Copy`.**
   `last_wind: Option<WindField>` stores a copy and compares by derived
   `PartialEq` over all four fields, so direction-only and gust-shape-only
   transitions are caught — which is what `0304538c` was for, and its test
   (`active_weather_direction_change_rebends_stationary_speedtree`) pins it.
   Correct as written. Disproved.
4. **Double wind application on `.spt` trees** (marker mirrored onto both the
   placement root and the mesh, `spawn.rs:782` + `mesh_instance.rs:649`). The
   root has no `MeshHandle` and no `Billboard`, so it is skipped by both loops;
   the mesh is handled exactly once, in the billboard arm. The code comment at
   `billboard.rs:133-141` claims the geometry loop is what prevents the double
   application — that is the wrong reason (the root fails `mesh_q.contains`),
   but the outcome is right. Not worth a finding on its own.
5. **`from_rotation_arc` roll on the antipodal case.** Re-checked; glam's
   `any_orthonormal_vector((0,0,-1))` yields a pure yaw, no roll. Disproved
   (unchanged from 2026-08-16).
6. **`SpeedTreeWind` missing from the save registry.** It is explicitly
   allowlisted as re-derived on load
   (`save_io/registry_completeness_tests.rs:111`), and `6096f19f` updated the
   rationale string when the component gained a consumer. Correct. Disproved.
7. **`elapsed` accumulator drift now that `TotalTime` is preferred.** The
   closure-local `elapsed` remains only as the fallback for worlds with no
   `TotalTime` (tests, synthetic). Both are `f32` seconds and feed a `sin`, so
   long-session precision loss is shared with every other clock consumer in the
   engine — not a SpeedTree-specific defect. Not reported.

---

## Summary

| Severity | Count |
|---|---:|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 3 |
| LOW | 4 |
| **Total** | **7** |

The parser half of this subsystem has now been untouched for two full audit
cycles and remains in the shape ten prior reports recorded. Every finding this
cycle is in code that did not exist on 2026-08-16.

The headline result is a genuine one: **#3076 is fixed, and billboards face the
camera at HEAD.** The dispatch's carry-over hypothesis — that a gate measuring
parse success while the visible output stayed wrong would still be wrong — does
not hold here. The `Billboard` component now lands on the entity that carries
`MeshHandle`, on both routes, with a guard pinning it.

What replaced it is a variant of the same shape one level up. The wind model
shared with water is *numerically* consistent across its three consumers — same
ceiling, same direction fallback, same one-sided magnitude — and that
consistency was clearly the design goal. But the two ends that were never
checked are both wrong: the **input** (`CNAM[0]`/`CNAM[1]` read as
response/stiffness, from a record whose own parser documents the layout as
unpinned, with a clamp that silences the feature on the repo's own vanilla
fixture) and the **output frame** (an object-local bend weighted by world-space
wind, applied to a view-locked billboard). The model is coherent end-to-end for
exactly one consumer — the geometry branch — which no production entity can
reach. So the shared-ceiling question the dispatch posed has a clean answer:
the ceiling is fine, and it is the only part of the handoff that is.

Cheapest first cut: pin `CNAM` against a real source before shipping any more
of this feature, and add one test that orbits the camera around a fixed tree
under fixed wind.

### Suggested next step

```
/audit-publish docs/audits/AUDIT_SPEEDTREE_2026-08-20.md
```

TALLY: CRITICAL=0 HIGH=0 MEDIUM=3 LOW=4
