**HEAD**: `4a3e89d56` · **Baseline**: `docs/audits/AUDIT_PERFORMANCE_2026-10-12.md` (HEAD `5d31fd637`, the last full run; 784 commits since) for Dims 1–6 and 8, and `docs/audits/AUDIT_PERFORMANCE_2026-10-19.md` (HEAD `41f9c2c6c`, streaming-deep, area-scoped; 157 commits since) for Dim 7 · **Audited**: Dim 1 (55 newly registered systems, scanned; the per-actor ones read), Dim 3 (delta read), Dim 4 (delta read plus one micro-benchmark), Dim 5 (cloud-shadow bake, terrain height blend, ground-cover blades), Dim 7 (delta read, verification of the five 10-19 findings, plus one real-data probe), Dim 8 (bracket count, scratch rows, harness provenance) · **Unchanged since baseline (skimmed)**: Dim 2 (no batching or sort change; only the eye-field pack moved into a helper), Dim 6 (zero functional commits in its paths)

# Performance Audit — 2026-10-21

**Command**: `/audit-performance`, one leg of `/audit-suite --preset comprehensive`. Run solo, with no sub-agents. Scratch: `/tmp/audit/performance/`.

**Mode**: static analysis, the guard test lanes, and three throwaway probes. No engine, GPU process or bench was launched (suite rule). **No GPU frame time is claimed.** Each CPU number is labelled as measured (probe on the Ryzen 7950X against real game data, or a micro-benchmark) or derived (from code plus a measured input).

Probe crates (private target dirs): `/mnt/data/tmp/suite-1021-performance/{fxprobe,hkxprobe,obsprobe}`. The guard lanes ran in the repo `target/`.

## Executive Summary

| Severity | NEW | Existing / tracked |
|---|---:|---:|
| CRITICAL | 0 | 0 |
| HIGH | 0 | 0 |
| MEDIUM | 2 | 2 (#5715, #5959; both still present, not re-filed) |
| LOW | 5 | 0 |

- **Medium: the parked-camera SVGF denoiser never reaches progressive accumulation in daytime exteriors** (PERF-D4-01). #5767 (`24eb7dba8`) added inputs to the "scene unchanged" key that change every frame: the time-of-day sun direction and colours, and any animated material's UV, emissive or alpha values. The key therefore changes every frame. This is the same scene-wide-gate class the still-camera noise investigation removed for NPC poses. It is a quality issue (still-camera noise). The cost side is that caustic history is also thrown away every frame.
- **Medium: ground-cover blade fragments recompute a per-blade constant** (PERF-D5-01). Each blade fragment recomputes the terrain colour at the blade root with up to ~18 bindless fetches at zero gradient. #6196 (`ea45af067`) roughly doubled that work by running the height blend there. This is the heaviest-overdraw pass in exteriors.
- Five LOW findings: a doubled per-frame material hash (measured: 57 µs at 4,000 unique materials), the unmeasured cloud-shadow bake, per-frame residue in two new gameplay systems, and the terrain-height cache path.
- **No guard eroded.** Every `Guard:` symbol in the skill still exists. Renderer lib passed 1458, core (`--features inspect`) 832, bin 3135, with 0 failures.
- **All five 10-19 streaming findings landed**: #6123, #6124, #6125, #6126, #6127.
- **No observed-vs-ROADMAP delta can be stated.** No bench was run. The live record `a3a7a43b0` is R6a-stale-29 (515 commits stale), and the ReSTIR, SVGF, shadow-filter, cloud-shadow and terrain-height work all landed after it.

## Hot Path Analysis

### Per-pass GPU cost
Not measured (no engine). These are the deltas to the pass inventory since the baseline:

| Pass | Change since `5d31fd637` | Timer | Status |
|---|---|---|---|
| Sky-cube bake | + `cloud_shadow.comp`, 512² columns × 16 Beer–Lambert steps × 2–3 3D-noise fetches, every frame, interiors included | inside `sky_cube_ms` | unmeasured; skyal.md's `gpu_sky_cube ~0.1 ms` predates it (PERF-D5-02) |
| SVGF + shadow filter | own nested `shadow_filter_ms` bracket (`fc83608d3`) | `shadow_filter_ms` ⊂ `svgf_ms` | #5959 open; REN-M-01 (this suite) covers its correctness |
| Main pass, terrain | height-blend fetches on painted lanes (`byroTerrainHeightBlend`) | `main_opaque_ms` | unmeasured; bounded to lanes with share > 0 |
| Main pass, grass blades | same blend per blade fragment, on top of the existing root-albedo fetches | `groundcover_blade_draw_ms` | PERF-D5-01 |

Bracket count: `QUERIES_PER_FRAME` = 58, i.e. 29 brackets, which matches the skill and the module doc (`gpu_timers.rs:135`).

Harness provenance (`scripts/check-bench-harness-provenance.sh`): against `a3a7a43b0` the verdict is **DIVERGED, 1 edit** (`fc83608d3`, which appends the `gpu_shadow_filter` column and carries a recorded "column append only" disposition from `1be33af66`). That is the state the skill documents, so it is not a finding.

### Per-frame CPU (derived from code unless marked)

| Site | Steady-state cost | Note |
|---|---|---|
| `fold_svgf_scene_inputs` | **measured** 57 µs @ 4,000 unique materials, 113 µs @ 8,000 | duplicate of hashes the interning already computed (PERF-D4-02) |
| `trap_contact_system` | O(trap bodies) chains + owner resolution + O(actors) std map, only while a Trap-layer body is loaded | PERF-D1-01 |
| `encumbrance_system` | one `Inventory` deep clone + weight sum + perk-entry composition per frame | PERF-D1-02 |
| `graph_locomotion_system` / `graph_root_motion_system` | **measured** 10–56 ns per variable lookup on FO4's 299-variable `raiderrootbehavior.hkx`, ~5 lookups/actor | not filed |
| `obscript_object_system` GameMode pass | one interpreter run per GameMode-scripted reference per frame; **measured counts** (FNV): 186 in the WastelandNV persistent CELL, 17 in the Goodsprings 3×3, 8 in Gomorrah01 | per-run cost unmeasured; see the SCR-2026-10-21-06 note |

## Findings

### Eroded guards

None. Every symbol named on a `Guard:` line resolves. The lanes that contain those guards pass: `byroredux-renderer --lib` 1458, `byroredux-core --features inspect --lib` 832, `byroredux --bin byroredux` 3135 (74 ignored).

### New issues

### PERF-D4-01: #5767's SVGF "scene unchanged" key folds inputs that change every frame, so progressive accumulation never engages in daytime exteriors or around any animated material
- **Severity**: MEDIUM (denoiser quality, the "denoiser ghosting/disocclusion" class, plus a per-frame caustic-history reset)
- **Dimension**: SSBO Sizing & Upload (a dirty gate that never hits because of per-frame jitter in a hashed field)
- **Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:34-75` (`fold_svgf_scene_inputs`), `:797-838` (key compare → `svgf_scene_static`, `caustic_scene_static`), `:1245-1259` (`next_svgf_temporal_alpha(.., svgf_history_valid)`); inputs from `byroredux/src/systems/weather.rs:789-796` and `:1064` (`game_time.tick(dt)` → `compute_sun_arc(hour, ..)`), and `byroredux/src/render/static_meshes.rs:392-420` (`AnimatedUvTransform` / `AnimatedAlpha` / `AnimatedEmissiveColor` / `AnimatedShaderFloat` written into the draw's material).
- **Status**: NEW. This is a side effect of the #5767 fix (`24eb7dba8`). Searched open and closed issues for "SVGF history material", "progressive accumulation animated" and "svgf scene key": only #5767 and #4046, both closed, both introducing the key. The renderer audit in this suite did not report it (REN-M-01/M-02 are about the shadow filter and ReSTIR taint).
- **Description**: SVGF's progressive-accumulation drop needs `svgf_history_valid = camera_static && !rigid_set_changed && caustic_scene_key == prev`. #5767 folded three groups of inputs into that key: every interned `GpuMaterial`'s full byte hash, the fog inputs, and the sky inputs (`sun_direction`, `sun_color`, `sun_illuminance`, zenith/horizon/lower colours). The function's own doc says continuous inputs are left out "so they do not keep the whole image at the recovery alpha floor". However, two of the included inputs are continuous:
  1. `weather_system` advances `GameTimeRes` by `dt × time_scale` every frame, and `compute_sun_arc` turns the fractional hour into `sun_direction`. The TOD colour pair is also re-interpolated from the same hour (`pick_tod_pair`). In any exterior with the clock running, the key differs on every frame.
  2. `GpuMaterial` carries `uv_offset_u/v`, `emissive_*` and `material_alpha`. These are written per frame by NIF texture-transform, material-colour and alpha controllers (waterfalls, fire and FX cards, signs, flickering emissives) and by a playing magic shader (`magic_shader_tint_system`). One such material anywhere in the frame's material table re-keys the whole image, including RT-only draws that are off-screen.
- **Evidence**: `fold_floats(&mut key, &sky.sun_direction)` and the per-material `hash_gpu_material_fields(material)` loop (`:46-60`); the unit test `material_fog_and_sky_changes_rekey_progressive_history` pins that a changed emissive re-keys, by design. `GpuMaterial` field list: `crates/renderer/src/vulkan/material.rs:86-142`. Static analysis only; not observed on screen.
- **Impact**: a parked camera in a daytime exterior, or in an interior with any animated material, stays at the recovery alpha. That is the per-pixel speckle and boiling #5370 and #4046 removed, and that the 2026-10-15 noise investigation removed for NPC poses (the "scene-wide gate" class). `caustic_history_valid` is also false every frame, so the caustic accumulator restarts continuously.
- **Related**: #5767, #4046, #5370; memory note *noise_investigation_2026-10-15*; PERF-D4-02 (same function).
- **Suggested Fix**: key only discrete changes. Quantize or omit the TOD-driven sky terms; for example, fold the TOD slot pair rather than the interpolated values, or fold them through a coarse step the way the ReSTIR rig key ignores intensity. Exclude the per-frame-animated `GpuMaterial` lanes (UV offset/scale, emissive, alpha) from the material fold, or skip materials whose draw carries an `Animated*` component. A changed texture index or kind still re-keys.

### PERF-D5-01: Every grass-blade fragment recomputes the blade's constant root ground colour with up to ~18 bindless fetches, and #6196 roughly doubled it
- **Severity**: MEDIUM (GPU cost on the heaviest-overdraw exterior pass; unmeasured, and the fix is a known cost-moving shape)
- **Dimension**: GPU Pipeline
- **Location**: `crates/renderer/shaders/groundcover_blade.frag:109-160` (ground albedo block), `crates/renderer/shaders/groundcover_blade.vert:419-433` (every input is per-blade constant: `vTerrainUv` comes from the blade `base`, splats from the base terrain sample, and the tile slot is `flat`), `crates/renderer/shaders/include/bindings.glsl:576-612` (`byroTerrainHeightBlend`: one `textureGrad` per lane with a height map and a share above 0, up to 9).
- **Status**: NEW. The root-colour coupling predates the baseline (#4056/#4907/#5174). The added height-blend fetches came in `ea45af067` (#6196) and `5f1a88929` (#6202). No issue was found for "groundcover blade fragment albedo".
- **Description**: the ground colour under a blade is a function of `vTerrainUv`, `vTerrainSplat0/1` and `vTerrainTileSlot`. All of them are the same for every vertex of a blade, so they are the same for every fragment. The fragment stage nevertheless evaluates, per fragment: one base-diffuse fetch, `byroTerrainHeightBlend` (now up to 9 height fetches plus the share and fade ALU), and `byroTerrainSplatAlbedo` (up to 8 layer-diffuse fetches). Because the UV is constant across a blade, `dFdx/dFdy(vTerrainUv)` are ~0 inside it, so the `textureGrad` calls sample near mip 0. These are full-resolution, cache-unfriendly reads, scattered across blades.
- **Evidence**: code read above. Unmeasured: `groundcover_blade_draw_ms` is the bracket. `TerrainTile.heightBlendBand = 0` (the setting at 0) disables only the #6196 half, which gives a ready A/B.
- **Impact**: GPU time scales with blade fragments × painted lanes in every grass exterior. #6196 added up to 9 fetches per fragment on top of the existing ~9.
- **Related**: #6195, #6196, #6202 (the height-map chain), #5959 (Whiterun main-pass reading).
- **Suggested Fix**: resolve the root albedo once per blade. Either compute it in the vertex shader with `textureLod` at a distance-derived LOD and pass it as a `flat` varying, or compute it in the scatter/layout pass into the blade instance. Measure `groundcover_blade_draw_ms` with the band on and off first (Speculative-Vulkan caveat: confidence that it costs is high, the size is unknown).

### PERF-D4-02: `fold_svgf_scene_inputs` re-hashes every unique `GpuMaterial` each frame although the material table already holds each hash
- **Severity**: LOW
- **Dimension**: SSBO Sizing & Upload
- **Location**: `crates/renderer/src/vulkan/context/build_and_upload_instances.rs:46-52`; the existing hashes are in `crates/renderer/src/vulkan/material.rs:1095-1120` (`MaterialTable.index: FxHashMap<u64, u32>`, keyed on `hash_gpu_material_fields`).
- **Status**: NEW. The skill's Dim 4 asked for a number ("O(unique materials), unmeasured; quantify before filing").
- **Description**: one frame now hashes the material bytes three times. `intern` hashes once per draw (#4201). `hash_material_slice` hashes once per unique material for the upload gate. `fold_svgf_scene_inputs` hashes once per unique material again.
- **Evidence**: **measured** on the 7950X (`fxprobe`, release, FxHasher over 432-B records, best of 200): 1,000 unique materials 14 µs, 4,000 57 µs, 8,000 113 µs. memory-budget.md records 4,000+ unique materials on a Skyrim radius-3 exterior.
- **Impact**: ~0.06–0.11 ms of main-thread CPU per exterior frame. No quantitative guard exists for this site.
- **Related**: PERF-D4-01 (same function; its fix may remove the loop).
- **Suggested Fix**: have `MaterialTable` keep an insertion-ordered `Vec<u64>` of the hashes it already computes, and fold those (O(unique) u64 folds, ~µs). Alternatively fold the upload gate's single `hash_material_slice` value.

### PERF-D5-02: The cloud-shadow bake is an unmeasured fixed per-frame pass, it runs in interiors that read one texel of it, and it has no bracket of its own
- **Severity**: LOW (measurement gap; a gate proposal waits on the number)
- **Dimension**: GPU Pipeline / Telemetry
- **Location**: `crates/renderer/shaders/cloud_shadow.comp` (512² invocations × `CLOUD_SHADOW_STEPS` 16 × `cloud_column_coverage` + `cloud_density`, i.e. 2–3 3D-noise fetches per step), dispatched unconditionally by `crates/renderer/src/vulkan/sky_cube.rs:569-571` → `sky_cube/cloud_shadow.rs:195-255`, inside the `sky_cube_ms` bracket.
- **Status**: NEW (`c231f5b5f`, `fdf4bfc1e`, `b6d6f8320`, 2026-10-20). The skill's inventory row already lists it, and memory-budget.md has its 512 KiB row. No timing is recorded anywhere.
- **Description**: ~262 k columns × 16 steps is ~4 M density evaluations per frame, about 2.7× the sky cube's own texel count. `skyal.md:248-250`'s `gpu_sky_cube ~0.1 ms` was measured before the bake existed. An interior consumes only the camera column (`cameraSunTransmittance`, read back for the portal sun), yet the whole map is rebaked. The clouds drift with wind at sub-texel speed per frame (125 m texels), so amortising the bake (half the rows per frame, or rebaking on a wind or texel-snap delta) is plausible.
- **Evidence**: shader read; `record_bake` has no gate (only `to_sun.y <= 0` short-circuits each invocation).
- **Impact**: unknown fixed GPU cost on every frame of every cell.
- **Related**: skill Dim 5 "sky-cube bake ... runs in interiors, quantify first"; PERF-D5-01.
- **Suggested Fix**: give the bake a nested `cloud_shadow_ms` bracket (+2 queries) and measure. Then gate interiors to a 1×1 camera-column dispatch, and consider amortising exteriors.

### PERF-D1-01: `trap_contact_system` still rebuilds its ancestor chains, script owners and a std `HashMap`/`HashSet` every tick while any Trap-layer body is loaded
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/systems/trap_hits.rs:109-245`. In order: the `chains` `Vec<Vec<EntityId>>` (`:141-156`); the `scripted` resolution with a std `HashSet` plus `source_reference` → `FormIdPool` + `ReferenceScriptIndex::lookup` (`:163-191`); `live_actor_bodies` std `HashMap` over every live actor (`:62-87`, `:211`); `now` std `HashSet` (`:230`).
- **Status**: NEW (residual of #5943, closed). #5943 cached the Trap-layer list on `RigidBodyData`'s structural generation, but the derived chains and owners are recomputed every tick.
- **Description**: the trap → scripted-owner map depends only on `Parent`, `PapyrusReferenceScripts` and `RapierHandles` structure. It is invariant between structural changes, yet it is rebuilt every frame with fresh allocations (one `Vec` per trap body). The actor-body map is rebuilt over all live actors with SipHash on the per-frame path. The skill treats a std map on a per-frame per-entity keyspace as a regression class.
- **Evidence**: code read. Derived: a Skyrim dungeon with ~20–50 trap bodies and ~20 actors costs tens of µs per frame. Unmeasured; no quantitative guard exists.
- **Related**: #5943, #2923.
- **Suggested Fix**: cache `traps: Vec<(owner, body)>` beside `TrapLayerCache`, keyed on the same generation plus the `Parent` / `PapyrusReferenceScripts` generations. Use `FxHashMap` / `FxHashSet` for `live_actor_bodies` and `TrapContacts`.

### PERF-D1-02: Two new per-frame systems do whole-structure work with no change gate: `encumbrance_system` clones the player's inventory, `head_look_system` walks the skeleton per looking actor
- **Severity**: LOW
- **Dimension**: CPU Hot Paths
- **Location**: `byroredux/src/encumbrance.rs:114-163` (`world.get::<Inventory>(player).map(|i| i.clone())` at `:125`, `catalog.inventory_weight`, `compose_max_carry_weight` → `compose_entry_point` perk evaluation at `:150`, every frame); `byroredux/src/systems/head_look.rs:36-44` + `:73` (`find_head` → `collect_subtree(world, root)` plus a `Name` scan, every frame per actor carrying `HeadLook`).
- **Status**: NEW (M47.3 / M42 work since the baseline). Searched "encumbrance", "head_look": only #6062 (lock order, closed).
- **Description**: the player's weight and capacity change only on inventory, actor-value or perk changes, yet the system deep-clones `Inventory.items` (one allocation plus N `ItemStack` copies) and recomposes perk entry points every frame. `HeadLook` persists until `StopLook`, and each frame re-collects the whole skeleton subtree (~80 entities, one `Vec`) to find `bip01 head`, an entity that never changes.
- **Evidence**: code read. Derived: a few µs per frame each. Small, but the shape the skill lists ("per-entity component deep-clone inside a per-frame loop", "whole-world scans with no change-detection key").
- **Suggested Fix**: borrow the inventory instead of cloning it (compute the weight under the guard, or keep a running total updated by the inventory mutators), and recompose only when the inventory, `ActorValues` or perk generation moves. Cache the head entity in `HeadLook` at `LookAt` time.

### PERF-D3-01: Terrain height maps decode the whole source DDS on a cache hit, and the resident maps have no memory-budget row
- **Severity**: LOW
- **Dimension**: GPU Memory Pressure / Streaming
- **Location**: `byroredux/src/asset_provider/terrain_height.rs:113-134` (`run_job`: `extract_first` → `Rgba8::decode_dds` before `cached_or_generate`), `:156-171` (the cache lookup needs only `bytes`, not `image`); `crates/terrain-height/src/lib.rs:30-34` ("a 1024² BC4 map with mips is 0.67 MiB, and a resident exterior carries a few dozen distinct layers"); `docs/engine/memory-budget.md` (no row).
- **Status**: NEW (`5017a64c2`, `2975d2906`, 2026-10-20).
- **Description**: `cache_key` hashes the raw source bytes, so a warm-cache job could stop at the key and the metadata read. Instead every first request per session also fully decodes the source texture (up to 2048² BC → RGBA8) on the two-thread `byro-terrain-height` pool. The decode is off the main thread; it only delays the height lane (bound later by `apply_ready_heights`). Separately, a few dozen 0.67 MiB BC4 maps (~20–30 MiB) are a new resident texture class, but memory-budget.md has no row for them.
- **Evidence**: code read; size from the crate's own constant.
- **Suggested Fix**: compute the key from the extracted bytes and return the cached entry before `decode_dds`. Add a memory-budget.md row for the per-layer height maps.

### Already tracked (verified still present; not re-filed)

| Issue | State at `4a3e89d56` |
|---|---|
| #5715 | Stale-payload salvage still runs synchronously (unchanged since 10-19). |
| #5959 | The Whiterun regression's +0.35 ms half is now isolated in `shadow_filter_ms` (`fc83608d3`); the +1.4 ms `main_render_ms` half is still untraced. |
| #5365 | Top-of-frame all-slots fence wait, unchanged. |
| #6106 / #6114 / #6116 | Unchanged; the skill's known-open notes still hold (`0188ce132` and `6e1853d24` are partial "Refs"). |
| #6097 | FO4 exterior worker pre-parses precombine-absorbed refs, unchanged. |

### Suite cross-references (not re-filed)

- **SCR-2026-10-21-06** (ObScript GameMode runs on stubs whose home cell isn't loaded) has a performance consequence. The FNV probe counts **186 GameMode-scripted references in the WastelandNV persistent CELL**, against 17 for a Goodsprings 3×3 exterior. Each of those 186 runs one interpreter pass per frame (`obscript_objects.rs:232-252`), and each run allocates four std `HashMap`s (`seed_locals`/`seed_refs` and two clones, `:657-669`) plus two `format!` strings per ref-local (`ref_variable_halves`, `:774-776`). The per-run cost was not measured. Fixing SCR-06 removes the multiplier.
- **REN-M-01 / REN-M-02 / REN-L-01** (shadow filter and ReSTIR taint): correctness findings in the pass that #5959 now times. Not duplicated here.

### Checked and not filed (measured or bounded)

- **Behaviour-graph variable lookups by name** (`variable_id` is a linear, case-insensitive scan; `graph_locomotion_system` and `graph_root_motion_system` do ~5 per actor per frame). **Measured** on FO4's largest character graph (`raiderrootbehavior.hkx`, 299 variables before inlining): `Direction` 10 ns, `Speed` 17 ns, `bGraphDriven` 38 ns, `SpeedSmoothed` 56 ns. That is under 0.3 µs per actor per frame.
- **Per-reference ObScript attach analysis** (`attach.rs:318-372`: `analyze_obscript_compatibility` + two `legacy_script_fingerprint` + `compile_legacy_obscript_program` per scripted REFR). The 10-19 scripting audit deferred this to #5999, which then closed after fixing only the Papyrus path. **Measured** on all 2,106 FNV object scripts: mean 4.6 µs, p50 2.3, p95 14.8, max 164 µs per reference. All 913 scripted refs of the WastelandNV persistent CELL total 4.9 ms; Gomorrah01 0.16 ms. A memo is not worth it.
- **#6076 `persistent_refs_for_tile`** (the skill's open quantify-first item): a linear filter over the ~6,069 WastelandNV persistent refs with one divide and one floor per ref. Derived ~25 µs per tile, under ~1.3 ms per radius-12 crossing, spread over the apply slices.
- **MSWP matching** (`mesh_instance.rs:113-160`, #5755): `normalize_material_path` borrows except for the `/`-rewrite and bare-path cases. Per-shape cost is negligible.
- **Loose `Data\` override** (#6200): the tree is indexed once per directory per process, so lookups gain no filesystem probe.
- **Terrain-height service**: generation is on its own two-thread pool. The main thread does one `std::fs::read` of a ≤0.67 MiB `.dds` per newly bound layer, which is negligible.
- **Ground-cover collect** (#4906 cover masks, view wedge): per resident cell, one binary search per authored-grass entry; all containers are reused scratch with `ScratchTelemetry` rows (`app_events.rs:39-57`).
- **`challenge_system`**: `ChallengeDefs` is `Arc`-backed, so the per-frame clone is a refcount bump. `MapMarkers.visible` holds only script-shown markers.
- **`magic_shader_tint_system`, `magic_effect_system`, `projectile_flight_system`, `use_weapon_system`, `obscript_item_system`, `distance_event_system`**: each early-outs on an empty storage or resource, and the empty `collect`s do not allocate.
- **New screen-sized allocations**: the shadow-filter images and reservoir tails are billed in `render_extent_bytes_per_pixel_x1024` (#6035, `predicates.rs:906-920`). The cloud-shadow map is fixed size and has its memory-budget row.

## Prioritized Fix Order

1. **PERF-D4-01**: drop the continuous TOD and animated-material lanes from the SVGF key. This is a small host-side change, and it restores parked-camera denoising in exteriors. **PERF-D4-02** falls out of the same edit.
2. **PERF-D5-02**: add the `cloud_shadow_ms` bracket (two queries) so the bake's cost is visible before anyone gates it.
3. **PERF-D5-01**: measure `groundcover_blade_draw_ms` with the height-blend band on and off, then move the root albedo to the vertex or scatter stage.
4. Quick wins: **PERF-D1-01** (cache trap owners on the existing generation; Fx maps), **PERF-D1-02** (borrow instead of clone; cache the head entity), **PERF-D3-01** (key before decode; memory-budget row).

## Stale skill premises

- Dim 5 says the sky-cube bake costs a fixed amount with no figure; the only figure (`skyal.md` ~0.1 ms) predates the cloud-shadow bake. The inventory row should say "unmeasured since `c231f5b5f`" until PERF-D5-02 lands.
- Dim 4's note on `fold_svgf_scene_inputs` ("O(unique materials), unmeasured") can now cite 57 µs @ 4,000 / 113 µs @ 8,000 (PERF-D4-02). Its premise that the key is an "unchanged" signal does not hold in exteriors (PERF-D4-01).
- Dim 7's open quantify-first note on #6076 can be closed with the derived bound above (~25 µs/tile).
- The skill does not list the new Update-stage systems (graph locomotion, encumbrance, trap contacts, head look, magic shaders, ObScript object/GameMode). Dim 1's `Paths:` should add `byroredux/src/systems/{graph_locomotion,trap_hits,head_look,magic_shader}.rs`, `byroredux/src/encumbrance.rs` and `crates/scripting/src/obscript_objects.rs`.

Next step: `/audit-publish docs/audits/AUDIT_PERFORMANCE_2026-10-21.md`
