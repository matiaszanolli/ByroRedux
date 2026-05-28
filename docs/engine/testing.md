# Testing

ByroRedux uses two layers of tests:

1. **Unit tests** (`#[cfg(test)] mod tests` inside source files, plus
   dedicated `*_tests.rs` sibling files after the Session 34/35 file
   splits) — fast, no game data required, run on every `cargo test`.
2. **Integration tests** (`#[ignore]`'d by default) — exercise real game
   archives, parse rates, ESM record counts, and end-to-end byte-level
   round-trips. Need the relevant game installed and resolve paths via
   env vars or Steam defaults. Run with `cargo test ... -- --ignored`.

The split keeps CI fast and game-data-free while letting developers run
the heavy sweeps locally on demand. Live test totals (workspace +
ignored) live in [ROADMAP.md → Project Stats](../../ROADMAP.md), which is
the authoritative count refreshed each `/session-close`. As of
2026-05-28 the workspace reports ~2628 tests passing with ~129
`#[ignore]`d real-data integrations spread across the crates; treat the
ROADMAP figure as ground truth and the numbers below as a structural map
of where coverage lives, not a frozen tally.

## At-the-keyboard live count

The default-runnable (`#[ignore]`-free) library tests:

```bash
cargo test --workspace --lib 2>&1 | grep "^test result:" | \
    awk '{s+=$4} END {print s}'
```

This undercounts the workspace total — integration-test binaries under
each crate's `tests/` directory and the binary crate's own
`byroredux/tests/` are not `--lib`. For the full reconciled number run
`cargo test --workspace` and read the aggregate, or consult ROADMAP.

## Unit test coverage by area

### ECS + animation — `byroredux-core`
- **Storage backends** (sparse set, packed) — insert / remove / iterate / overwrite, swap-remove invariants, sort-order maintenance
- **World basics** — spawn, multi-storage coexistence, get/get_mut, lazy storage init
- **Single-component queries** — read, write, register-without-insert
- **Multi-component queries** — read+write coexistence, TypeId-sorted lock ordering, deadlock detection on same-type pairs
- **Resources** — insert/read/mutate, type-name in panic messages, missing-resource handling, overwrite, scheduler visibility
- **Scheduler** — closures, struct systems, ordering, mutation propagation, system names, and (since **M27**) declared-access conflict detection on parallel-stage systems
- **Names + StringPool** — attach, find_by_name, missing pool, missing components
- **Math + types** — `Vec3`/`Quat` round-trips, `Color`, `NiTransform` defaults
- **Form IDs** — pool allocation, plugin slot mapping, content-addressed identity
- **Animation engine** — the `crates/core/src/animation/` submodule split (`types`, `registry`, `player`, `stack`, `root_motion`, `interpolation`, `text_events`, `controller`) carries clip registry, player advance, blending stack, root-motion split, and the interpolation kernels (linear, Hermite, TBC tangents, translation/rotation/scale/float/color/bool sampling)

### NIF — `byroredux-nif`
- **Header parser** — minimal Skyrim header, blocks + strings, NetImmerse pre-Gamebryo, BSStreamHeader for FO4/FO76, user_version threshold
- **Stream reader** — primitives, version-dependent string format, block refs, transforms
- **Block parsers** — every supported block type (NiNode, NiTriShape, BSTriShape variants, NiSkinPartition, BSLightingShaderProperty across 8 shader-type variants, BSEffectShaderProperty, particle systems, FO76 CRC32 flag arrays, FO76 stopcond, FO76 luminance/translucency). Collision (`bhk*`) coverage lives next to the parsers in the Session-35 split [`crates/nif/src/blocks/collision/`](../../crates/nif/src/blocks/collision/) — dedicated sibling tests for `bhk_rigid_body`, `bhk_ragdoll`, `bhk_breakable_constraint`, `bhk_blend_collision_object`, and `hk_packed_ni_tri_strips_data`
- **Dispatch regression tests** (`blocks::dispatch_tests`) — the original N26-audit module has grown into a per-topic directory [`crates/nif/src/blocks/dispatch_tests/`](../../crates/nif/src/blocks/dispatch_tests/) (Session-35 split out of a 3 667-LOC monolith, ~67 tests today across `shader`, `havok`, `interpolators`, `controllers`, `extra_data`, `nodes`, `effects`, `starfield`). Shared fixtures (`oblivion_header`, `oblivion_bsshader_bytes`) live in `mod.rs`. Each test drives a minimal game-shaped byte stream through `parse_block`, downcasts the result, and asserts *exact* stream consumption so any future byte-width or version-gate drift fails fast — the original goal of catching regressions on Oblivion's block-sizes-less (v20.0.0.5) path, now extended to FO76/Starfield block families
- **Animation import** — the `crates/nif/src/anim/` submodule split (`coord`, `controlled_block`, `transform`, `sequence`, `keys`, `channel`, `bspline`, `entry`) with its own `tests.rs`: `NiTransformInterpolator`, `NiKeyframeData`, `NiTextKeyExtraData`, controller-manager and `NiControllerSequence` import, compressed B-spline evaluation
- **Mesh import** — the `crates/nif/src/import/mesh/` submodule split keeps geometry extraction beside its tests (Session-35 stage-A split out of the old `mesh.rs`): `bs_geometry` + `_skin_tests` + `_tangent_tests`, `bs_tri_shape` + `_kind_passthrough` + `_partition_remap` + `_shader_flag` siblings, `material_path_capture_tests`, `shader_type_fields_tests`, `skin_tests`, `sse_skin_geometry_reconstruction_tests`, `tangent_convention_tests`
- **Coordinate conversion** — Z-up→Y-up identity, 90° rotation around each axis, vertex positions, vertex normals, winding-order preservation
- **Scene parsing** — empty file, minimal node, unknown block recovery, downcasting via `get_as`

### Plugin — `byroredux-plugin`
- **Manifest parsing** — valid TOML, invalid TOML, no-deps case
- **Records** — `RecordType` 4-char codes, ECS spawn integration, `find_by_form_id`, equality / hashing
- **DataStore + resolver** — depth resolution, three-way chains, transitive deps, deterministic tiebreak
- **Legacy ESM/ESP/ESL bridge** — slot-to-PluginId mapping, save-generated forms, reserved slots
- **ESM cell parser** — the live CELL walker is split under [`crates/plugin/src/esm/cell/`](../../crates/plugin/src/esm/cell/) with a per-topic `tests/` directory (Session-35 split out of a 3 329-LOC monolith): `addn_stat`, `cell`, `cell_for_refr` (M40 door-teleport reverse-lookup), `light`, `merge`, `movs` (FO4 Movable Static), `refr`, `txst`, `wrld`, plus a real-ESM `integration` topic. STAT extraction, REFR position/scale, group walking, XCLW water height, all-8 TXST slots, plugin-merge last-wins, worldspace parent links
- **ESM record parser (M24 + M24.2)** — WEAP / ARMO / MISC field extraction, CONT inventory, LVLI leveled entries, NPC race/class/factions/inventory/AI, FACT relations + ranks, GLOB/GMST typed values, group walker, total counters; M24.2 adds QUST stage + objective block-walker tests and PERK PRKE/PRKF entry-type tests (Quest / Ability / EntryPoint)
- **SubReader migration (R2 Phase B)** — the sequential `SubReader` cursor primitive replaced 169 ad-hoc `read_*_at` field reads across 15 record files; behaviour-preservation is pinned by the existing per-record tests plus the real-data parity tests below

### BSA / BA2 — `byroredux-bsa`
- **Path normalization** — case-insensitive, slash agnostic
- **Reject non-archive files** — both BSA and BA2
- **DDS header reconstruction** — 148-byte layout invariants, BC1/BC7 linear-size, unknown-format fallback

### Physics — `byroredux-physics`
- **Shape conversion** (`convert.rs`) — glam ↔ nalgebra Vec3/Quat round-trips, every `CollisionShape` variant mapping to the right Rapier `SharedShape` constructor, compound shape recursive mapping, empty trimesh fallback to a tiny ball
- **World stepping** (`world.rs`) — empty world has zero bodies, a dynamic ball actually falls under gravity, a static floor blocks a dynamic ball to rest at `y ≈ radius`, the substep cap clamps wall-clock spikes so the physics system never spiral-of-deaths on a hitch; plus the **M28.5** `KinematicCharacterController` collide-and-slide / autostep / jump behaviour
- **Player body** (`components.rs`) — `CharacterController::HUMAN` constructs a sane capsule
- **Contact config** (`config.rs`) — the `ContactConfig` resource that hoisted the previously-inlined TriMesh flags, contact-skin, and KCC offset/autostep tunables out of three call sites

### Materials — `byroredux-bgsm` / `byroredux-sfmaterial` / `byroredux-facegen` / `byroredux-spt`
- **`byroredux-bgsm`** — FO4 `.bgsm` / `.bgem` material parsing; unit tests on field decode plus a `#[ignore]`d corpus integration test (`tests/parse_all.rs`) over `Fallout4 - Materials.ba2`
- **`byroredux-sfmaterial`** — Starfield CDB material database: synthetic minimum-header round-trip + magic recognition/rejection (`tests/header_smoke.rs`), and a `#[ignore]`d real-data smoke test against vanilla `materialsbeta.cdb` (`tests/real_cdb.rs`)
- **`byroredux-facegen`** — FaceGen `.tri` / `.egm` parsers with `#[ignore]`d real-data integrations against vanilla FNV / FO3 content (`tests/parse_real_facegen.rs`)
- **`byroredux-spt`** — SpeedTree `.spt` TLV walker; synthetic-fixture unit tests (`tests/parse_synthetic_spt.rs`) plus a `#[ignore]`d FNV corpus test (`tests/parse_real_spt.rs`) asserting the Phase-1.3 ≥95% geometry-tail acceptance gate

### Renderer — `byroredux-renderer`
The renderer crate carries real unit tests now, not just doc-tests. Coverage that runs without a GPU lives in pure-data modules:
- **Vertex + mesh** — `vertex.rs` pins the 100-byte stride / 9 attribute descriptions; `mesh.rs` exercises the cube/triangle/quad helpers and SSBO bookkeeping
- **Scene-buffer GPU contract** — [`crates/renderer/src/vulkan/scene_buffer/`](../../crates/renderer/src/vulkan/scene_buffer/) tests pin the `#[repr(C)]` std430 layout (`gpu_instance_layout_tests`, `instance_hash_tests`, `material_hash_tests`, `scene_descriptor_reflection_tests`), guarding the Shader Struct Sync contract
- **DDS / texture registry** — `vulkan/dds.rs` header parsing, `texture_registry_tests` + `texture_registry_bindless_tests`
- **Shader reflection** — `vulkan/reflect.rs` and `shader_constants.rs` keep the GLSL-side constants in lockstep
- **Deferred destroy + sync** — `deferred_destroy.rs`, `vulkan/sync.rs`, `vulkan/allocator.rs`, `vulkan/buffer.rs` exercise the frames-in-flight teardown window
- **Acceleration structures** — `vulkan/acceleration/tests.rs` covers the BLAS/TLAS predicate functions (`scratch_should_shrink`, `decide_use_update`, eviction thresholds) split out in Session 35

### Other crates
- **`byroredux-scripting`** — event marker round-trips, timer expiry, end-of-frame cleanup; plus the **M47.0** ECS-native script runtime (`papyrus_demo` dispatcher systems + `ScriptRegistry` + the 14 e2e tests in `papyrus_demo/tests.rs`) and the **M47.1** condition evaluator (`condition` module: CTDA `ConditionFunction` dispatch + the load-bearing OR-precedence regression pairs)
- **`byroredux-papyrus`** — lexer, Pratt expression parser, statement + top-level item parsers, and the **M30.2** `tests/r5_round_trip.rs` integration test round-tripping the four R5 source `.psc` files through `parse_script`
- **`byroredux-audio`** — **M44** kira backend: `AudioWorld` lifecycle, spatial sub-tracks, footstep accumulator, looping prune, reverb send (unit tests in `src/tests.rs`, plus `#[ignore]`d cpal real-data integrations)
- **`byroredux-platform`** — window creation, raw handle round-trip
- **`byroredux-debug-protocol`** — `wire.rs` length-prefixed JSON encode/decode round-trips
- **`byroredux-debug-server`** — `evaluator.rs` (Papyrus-AST → ECS query) and `listener.rs` command-queue tests
- **`byroredux-renderer`** also exposes doc-tests on its public API

### Binary crate — `byroredux`
After the Session-34 split, the binary's unit tests live beside the modules they cover under `byroredux/src/`:
- **Systems** — `systems/` submodule tests (`animation`, `particle`, `character`, `weather`, `audio`, `bounds`)
- **Cell loading** — `cell_loader/` sibling tests (terrain + splat, SCOL/PKIN expansion, transition, references, REFR texture overlay, LGTM fallback, NIF light-spawn gate, Euler→Y-up quat, unload skin cleanup, inventory release, finish-partial)
- **Render data** — `render/` submodule tests (lights, skinned, frustum, draw-sort key, bone-palette overflow, directional upload, fog-curve propagation, variant-pack gating)
- **Scene setup** — `scene/` tests (procedural fallback, climate TOD hours, cloud-tile scale, radius parsing)
- **Misc** — `streaming_tests`, `commands_tests`, `parsed_nif_cache`, `scene_import_cache`, `game_profiles`, `asset_provider`, `npc_spawn`, `helpers`, `components`

Dedicated integration-test binaries live under [`byroredux/tests/`](../../byroredux/tests/):
- `skinning_e2e.rs` — **M29** end-to-end skinning chain on real game content (FNV `NiTriShape`+`NiSkinData` legacy path and SSE `BSTriShape`+`BSSkinBoneData` global-buffer path, the latter covering the #638 fix). `#[ignore]`d.
- `golden_frames.rs` — golden-frame regression: boots the engine with `--bench-frames N --screenshot path`, per-pixel-compares the captured PNG against `tests/golden/`. Determinism via `BYROREDUX_FIXED_DT`. Catches "Phase X made things worse" rendering regressions.
- `m41_phase1bx_skinning.rs` — `#[ignore]`d diagnostic pinning the documented bind-pose disagreement between vanilla FNV `skeleton.nif` and `upperbody.nif`.

## Integration tests (`#[ignore]`'d)

These tests require real game data on disk and are gated behind the
`#[ignore]` attribute so CI doesn't fail without it. They resolve game
paths from environment variables, falling back to canonical Steam install
paths on the reference development machine.

| Test                                                       | Crate / file                                  | What it does                                                                |
|------------------------------------------------------------|-----------------------------------------------|-----------------------------------------------------------------------------|
| `parse_rate_oblivion`                                      | nif/`parse_real_nifs.rs`                       | Walks `Oblivion - Meshes.bsa`, asserts the per-game parse floor             |
| `parse_rate_fallout_3`                                     | nif/`parse_real_nifs.rs`                       | `Fallout - Meshes.bsa` (FO3)                                                |
| `parse_rate_fallout_nv`                                    | nif/`parse_real_nifs.rs`                       | `Fallout - Meshes.bsa` (FNV)                                                |
| `parse_rate_skyrim_se`                                     | nif/`parse_real_nifs.rs`                       | `Skyrim - Meshes0.bsa`                                                       |
| `parse_rate_fallout_4`                                     | nif/`parse_real_nifs.rs`                       | `Fallout4 - Meshes.ba2` (BA2 v8)                                            |
| `parse_rate_fallout_76`                                    | nif/`parse_real_nifs.rs`                       | `SeventySix - Meshes.ba2` (BA2 v1)                                          |
| `parse_rate_starfield`                                     | nif/`parse_real_nifs.rs`                       | `Starfield - Meshes01.ba2` (BA2 v2)                                         |
| `parse_rate_starfield_all_meshes` / `parse_rate_fo4_all_meshes` | nif/`parse_real_nifs.rs`                  | Full multi-archive sweeps for the two largest corpora                       |
| `parse_rate_smoke_all_games`                               | nif/`parse_real_nifs.rs`                       | First N NIFs from each available game                                       |
| `real_archive_torch_meshes_surface_particle_emitters`      | nif/`parse_real_nifs.rs`                       | Asserts torch meshes expose particle emitters end-to-end                    |
| `per_block_baseline_*` (all 7 games)                       | nif/`per_block_baselines.rs`                   | Compares per-header-type `parsed` vs `NiUnknown` histogram against checked-in TSV baselines under `tests/data/per_block_baselines/`; fails on any unknown growth or parsed shrinkage (R3). Regenerate with `BYROREDUX_REGEN_BASELINES=1` |
| `cross_game_translation_completeness`                      | nif/`translation_completeness.rs`              | #1277 Task 8: walks a bounded mesh sample per game through the importer, collects `MaterialStats`, asserts canonical `Material`-slot invariants — the NIFAL translation-layer regression net |
| heap-allocation bound test                                 | nif/`heap_allocation_bounds.rs`                | #1247: parses a synthetic NIF inside a `dhat::Profiler` scope, asserts upper bounds on block count + byte total (promotes the NIF-PERF allocation pins from audit-cadence to test-cadence) |
| `parse_rate_fnv_esm`                                       | plugin/`parse_real_esm.rs`                     | Loads `FalloutNV.esm`, asserts per-category record floors (items / NPCs / factions / globals; FNV ≈ 62 219 records observed, with a 60 000 floor) |
| `parse_rate_fo3_esm` / `parse_rate_oblivion_esm` / `parse_rate_fo4_esm` | plugin/`parse_real_esm.rs`         | Per-game ESM record-count floors (FO3 CREA/LVLC/SCPT, FO4 architecture, etc.) |
| `parse_real_fo3_megaton_cell_baseline`                     | plugin/`esm/cell/tests/integration.rs`         | Asserts Megaton Player House carries 929 REFRs on-disk                       |
| `clas_oblivion_knight_against_vanilla` / `race_oblivion_data_and_subs_against_vanilla` | plugin/`parse_real_esm.rs` | Oblivion CLAS / RACE parity against vanilla Oblivion.esm                     |
| `dump_prospector_saloon_refrs`                             | plugin/`parse_real_esm.rs`                     | Diagnostic REFR dump for the FNV bench-of-record cell                        |
| cell `integration` topic                                   | plugin/`esm/cell/tests/integration.rs`         | Walks FNV / Oblivion / FO3 / Skyrim / FO4 masters through the CELL walker    |
| `bsa_real.rs` / `ba2_real.rs`                              | bsa/`tests/`                                   | FNV BSA + BA2 open/list/contains/extract/decompress round-trips             |
| `parse_all`                                                | bgsm/`tests/parse_all.rs`                      | FO4 BGSM/BGEM corpus parse-rate                                             |
| `real_cdb`                                                 | sfmaterial/`tests/real_cdb.rs`                 | Starfield CDB material DB smoke                                            |
| `parse_real_facegen`                                       | facegen/`tests/parse_real_facegen.rs`          | FaceGen `.tri` / `.egm` against vanilla FNV / FO3                            |
| `parse_real_spt`                                           | spt/`tests/parse_real_spt.rs`                  | FNV SpeedTree corpus geometry-tail gate                                     |
| `skinning_e2e`                                             | byroredux/`tests/skinning_e2e.rs`              | M29 end-to-end skinning chain (FNV legacy + SSE global-buffer)              |
| `byroredux` binary doc tests / args parsing                | byroredux                                      | CLI help, env-var override                                                  |

## Game data resolution

The NIF integration test suite shares one helper module
[`crates/nif/tests/common/mod.rs`](../../crates/nif/tests/common/mod.rs)
that exposes a `Game` enum and a `MeshArchive` enum wrapping both BSA and
BA2 archives behind a single `list_files()` / `extract()` API. Each game
declares its env-var name (`BYROREDUX_FNV_DATA`, `BYROREDUX_FO3_DATA`,
`BYROREDUX_OBLIVION_DATA`, `BYROREDUX_SKYRIMSE_DATA`, `BYROREDUX_FO4_DATA`,
`BYROREDUX_FO76_DATA`, `BYROREDUX_STARFIELD_DATA`) and a default Steam
path; the helper picks whichever resolves first and prints a skip notice
when neither does. The module also carries the shared `ParseStats`
accumulator (`success_rate` / `recoverable_rate`) and the per-block
histogram (`to_tsv` / `from_tsv` / `compare_histograms`) used by the
baseline gate.

The same helper backs the `parse_real_nifs.rs`, `per_block_baselines.rs`,
and `translation_completeness.rs` integration tests and the `nif_stats`
example binary in `crates/nif/examples/nif_stats.rs`. The plugin, bgsm,
sfmaterial, facegen, and spt integration tests each carry their own
small env-var-resolving `data_dir` helper following the same shape.

## Running tests

```bash
# Default — fast, no game data required.
cargo test

# A single crate
cargo test -p byroredux-core
cargo test -p byroredux-nif
cargo test -p byroredux-plugin

# A single module
cargo test -p byroredux-core -- ecs::world

# Integration tests requiring game data
cargo test -p byroredux-nif --release --test parse_real_nifs -- --ignored
cargo test -p byroredux-plugin --release --test parse_real_esm -- --ignored
cargo test -p byroredux-bsa -- --ignored
cargo test -p byroredux --test skinning_e2e -- --ignored

# Per-block baseline regression gate (regen with BYROREDUX_REGEN_BASELINES=1)
cargo test -p byroredux-nif --release --test per_block_baselines -- --ignored

# nif_stats CLI: walks one archive and reports parse rate + block histogram
cargo run -p byroredux-nif --example nif_stats --release -- \
    "/path/to/Fallout - Meshes.bsa"
cargo run -p byroredux-nif --example nif_stats --release -- --tsv \
    "/path/to/Fallout - Meshes.bsa"
```

## Smoke tests

A third layer sits outside `cargo test`: manual end-to-end checks that
need a Vulkan device *and* on-disk game data, documented in
[`docs/smoke-tests/README.md`](../smoke-tests/README.md). They follow the
`--bench-hold` → `byro-dbg`-attach pattern — boot the engine with
`--bench-frames N --bench-hold` so it stays open after the bench, then
attach `byro-dbg` (port 9876) and drive console commands (`tex.missing`,
`tex.loaded`, `entities <Component>`, `skin <id>`, `script.activate
<id>`) against the loaded scene. Current scripts:
[`m41-equip.sh`](../smoke-tests/m41-equip.sh) (Skyrim+ / FO4 NPC outfit
equip end-to-end) and [`m-trees.sh`](../smoke-tests/m-trees.sh)
(SpeedTree placeholder billboards).

## Test infrastructure milestones

Historical (through Session 12 / 2026-04), preserved for context:

- **N23.10** introduced the per-game integration test infrastructure plus
  graceful per-block parse recovery in the NIF top-level walker — turning
  single-block parser bugs from NIF-killing errors into measurable
  telemetry.
- **M26+** added the `MeshArchive` enum and BA2 game entries (Fallout 4,
  Fallout 76, Starfield) on top of the existing BSA games.
- **M24 Phase 1** added the `parse_rate_fnv_esm` record-count integration
  test that verifies the structured record parser against real FNV.esm.
- **N26 audit** landed the `blocks::dispatch_tests` module — minimal
  game-shaped byte streams driven through `parse_block` asserting exact
  stream consumption, catching any future byte-width or version-gate
  drift on Oblivion's v20.0.0.5 block-sizes-less path. (Since grown into
  the per-topic `dispatch_tests/` directory described above.)
- **M28 Phase 1** added the `byroredux-physics` crate with unit tests
  proving the Rapier bridge end-to-end: shape mapping, dynamic bodies
  falling under gravity, static floors blocking them.
- **#638** extended the BSTriShape skin-payload tests (SSE 12-byte
  `VF_SKINNED` block decode) so the M29 GPU pre-skinning path has
  parser-side regression coverage.

Continuing the timeline through Session 42 (sourced from git history):

- **R3** (`per_block_baselines.rs`) shipped the per-game `parsed` vs
  `NiUnknown` histogram baseline gate with checked-in TSV baselines under
  `crates/nif/tests/data/per_block_baselines/` for all 7 games. It runs
  as an opt-in `cargo test … -- --ignored` invocation (no CI workflow
  yet), so "fail on regression" is the test's *contract* rather than an
  enforced pipeline.
- **Session 34/35 file splits** (2026-05-10..14) broke most >2000-LOC
  files into submodule directories, and the co-located tests moved with
  them: the CELL `tests.rs` monolith → `esm/cell/tests/` (per-topic), the
  `dispatch_tests.rs` monolith → `dispatch_tests/`, the mesh-import tests
  → `import/mesh/*_tests.rs` siblings, the collision parsers → `blocks/collision/*_tests.rs`,
  and the binary's `systems.rs` / `render.rs` / `scene.rs` / `cell_loader`
  into their respective submodule test files.
- **M29 / M29.5** (GPU skinning) added `byroredux/tests/skinning_e2e.rs`
  (FNV legacy + SSE global-buffer paths) and renderer-side
  `skin_compute.rs` tests.
- **M28.5** (kinematic character controller) added collide-and-slide /
  autostep / jump coverage in physics `world.rs` and the `ContactConfig`
  resource tests in physics `config.rs`.
- **M44** (audio) added the `byroredux-audio` test suite (`src/tests.rs`):
  kira lifecycle, spatial sub-tracks, footstep accumulator, looping
  prune, reverb send, plus `#[ignore]`d cpal real-data integrations.
- **M30.2** (Papyrus `.psc` → AST) added the statement / top-level item
  parser tests and `crates/papyrus/tests/r5_round_trip.rs`.
- **M47.0 / M47.1** (event-hook + condition runtime) added the
  `papyrus_demo` dispatcher e2e tests and the `condition` CTDA-parser +
  OR-precedence evaluator tests in the scripting crate.
- **R2 Phase B** (ESM `SubReader` migration) is regression-covered by the
  existing per-record tests plus the Oblivion vanilla parity tests.
- **Golden-frame regression** (`byroredux/tests/golden_frames.rs`) boots
  the real renderer with a fixed dt and per-pixel-diffs against a checked-in
  baseline — the automated guard for the "Phase X made things worse"
  failure mode that previously only surfaced on manually-shared screenshots.
- **#1247 heap-allocation bound test** and **#1277 Task 8
  `translation_completeness.rs`** promoted two formerly audit-only checks
  (allocation hygiene; canonical-`Material` translation completeness) to
  test-cadence — the latter is the regression net for the NIFAL ("NIF
  Abstraction Layer") canonical translation tier landed post-Session-42
  (material, particle, and node-passthrough slices; see
  [`docs/engine/nifal.md`](nifal.md)).

See [Game Compatibility](game-compatibility.md) for the per-game parse
rate matrix the integration tests produce, and
[ROADMAP.md → Project Stats](../../ROADMAP.md) for the authoritative live
test count.
