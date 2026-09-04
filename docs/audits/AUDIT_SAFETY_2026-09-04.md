# Safety Audit — ByroRedux — 2026-09-04

**Command**: `/audit-safety` (run as the safety leg of the `water-deep` audit suite)
**Repo state**: `main` @ `b15b0527`, working tree carries one small uncommitted
change (`byroredux/src/cell_loader/precombined.rs`, an in-progress #3641
LOD-tie-break refactor unrelated to safety — reviewed, not a finding)
**Severity scale**: `.claude/commands/_audit-severity.md`

## Scope

All eleven dimensions of `.claude/commands/audit-safety/SKILL.md` were run in
full, using direct `Read`/`Grep`/`Bash` investigation and `cargo test` runs —
no sub-agents were used for the final analysis (an earlier attempt to
parallelize this audit across background sub-agents was abandoned mid-run per
project policy: `feedback_audit_suite_nested_agent_relay` in the project's own
memory notes that a subagent's nested sub-agents' results don't reliably relay
back, so every finding and PASS claim below was independently re-derived by
this agent directly against current source, not trusted from any relayed
summary).

**Water/WATAL-adjacent coverage** (this audit runs as part of the `water-deep`
suite): `crates/physics/src/water.rs` (buoyancy/current/submerged-fraction),
`byroredux/src/systems/water.rs` and `byroredux/src/systems/character.rs`
(swim/drowning), the water shaders (`water.vert/frag`), and the WATR ESM
record parser were all specifically re-checked for memory-corruption, leak,
and NaN/Inf risk beyond the skill's baseline checklist. **Result: the water
surface is thoroughly defended** — every production `WaterVolume` construction
site was traced to confirm a structural (not just runtime-checked)
`min <= max` invariant, every division in the buoyancy/current-force math is
floored or finiteness-gated, and the one HEAD-commit change touching water
this session (`b15b0527`, water.frag alpha-blend fix + a Skyrim
`water_material_from_mesh` legacy-flag fix) is a rendering-correctness change
with no safety implication (reviewed, not re-litigated — it is already
committed and tested, not this audit's finding to make).

## Method notes

- Ran the actual test suites rather than trusting prior reports' PASS claims:
  `cargo test -p byroredux-renderer` (846 passed), `-p byroredux-core --lib`
  (742 passed), `-p byroredux-nif --lib` (1230 passed), `-p byroredux-physics
  --lib` (156 passed), `-p byroredux-mod-runtime` (66 passed), `--bin
  byroredux` (1918 passed, 20 ignored — expected smoke-test gates). All green.
- Per the project's no-speculative-Vulkan-fixes rule, no render-pass, barrier,
  or pipeline-state restructure is proposed anywhere in this report. No
  validation-layer/RenderDoc run was performed (project policy forbids
  spawning a parallel/headless engine instance alongside the user's own); the
  one device-selection-level Vulkan item (non-RT GPU rejection) was confirmed
  via static SPIR-V-capability reasoning that is already fixed and tested, not
  asserted as a new bug.
- Counts re-derived, not taken from prior reports: **738** real `unsafe { }`
  blocks (this count excludes `unsafe fn`/`impl`/`trait` declarations,
  string-literal/comment false matches) across the workspace at audit time —
  up from 724 at the 2026-08-30 baseline, consistent with the intervening
  ~150 commits.

## Dedup baseline

The most recent prior safety report is `docs/audits/AUDIT_SAFETY_2026-08-30.md`
(10 findings: 2 HIGH, 5 MEDIUM, 3 LOW). **All 10 were confirmed CLOSED as
GitHub issues** (#3758–3765 cover 7 of them directly by title match; the
remaining 3 LOW items — a stale line-anchor note, a `GpuCamera` doc-rot note,
and a `GpuMaterial` hash-doc note — are covered by adjacent closed issues
#3447/#3450/#2491 addressing the same underlying doc-rot). Fixes were verified
two ways: (1) `gh issue view` confirms CLOSED state for all of #3758, #3759,
#3760, #3761, #3763, #3764, #3765; (2) independently, `git log` shows the
actual fix commits landed (`e7c1e4e7` Fix #3761, `d270e9b3` Fix #3763,
`9ce6b7a5` Fix #3765, plus the #3758/#3759/#3760/#3764 fixes earlier in the
log) — and this audit re-ran the regression-guard tests those fixes added
(all green; see per-dimension detail below), rather than trusting the closed
label alone.

`gh issue list --state all` (300 most recent) was scanned for keyword overlap
with every candidate finding before filing; the one new finding below (a
doc-rot / audit-coverage gap) has no matching open or closed issue.

## Findings summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 1 |
| **Total** | **1** |

| ID | Sev | Dim | Title |
|---|---|---|---|
| DOC-ROT-1 | LOW | 11 | mod-runtime's "no consumer" premise is stale — `byroredux/src/extensions.rs` (10,652 LOC) is now a live, native, engine-side consumer |

**Dimensions that produced zero findings**: 1 (FFI lifetime), 2 (memory
corruption/UB), 3 (leaks), 4 (unsafe-block discipline), 5 (Vulkan spec
compliance), 6 (R1 material layout), 7 (RT IOR-refraction), 8 (NPC/animation
spawn), 9 (NIFAL NaN/Inf boundary — including the water-adjacent extra-scope
check), 10 (debug-ui teardown). Every regression guard, size pin, offset
test, and teardown-ordering invariant the skill names for these ten
dimensions was independently re-verified present, correct, and (where a
guard test exists) passing.

---

# Findings

### DOC-ROT-1: mod-runtime's "no consumer" premise is stale — `extensions.rs` is now a live, native, 10,652-LOC engine consumer
- **Severity**: LOW
- **Dimension**: 11 — Sandboxed Mod Runtime Trust Boundary
- **Location**: `.claude/commands/audit-safety/SKILL.md` Dimension 11 (states
  mod-runtime "has **no consumer in the engine yet**" — audit it "as a
  contract, not a live path"); actual live code at `byroredux/src/extensions.rs`
- **Status**: NEW
- **Description**: `byroredux/src/extensions.rs` is a real, 10,652-line file
  (confirmed via `wc -l`), added by commit `24df5304` ("feat(engine): host
  sandboxed extensions natively") and most recently touched `2026-09-03` — one
  day before this audit. It is wired into the binary (`mod extensions;` in
  `byroredux/src/main.rs:28`, called from `main.rs:704` and `main.rs:760`) and
  directly drives `byroredux_mod_runtime::{SandboxRuntime, SandboxError, ...}`
  — constructing a live `SandboxRuntime`, calling `compile()`/`instantiate()`,
  and bridging ECS events/commands to guest WASM components. This flips the
  premise that closed issue **#3748** ("`byroredux-mod-runtime` is a dangling
  `[workspace.dependencies]` alias with no member consumer") established: that
  was accurate as of its closure, but `extensions.rs` landed afterward.
  Compounding this, `.claude/commands/_audit-common.md`'s project-layout map —
  the shared reference every audit skill is told to trust — has **no entry at
  all** for `extensions.rs`, even though it individually lists files an order
  of magnitude smaller (`interaction.rs` 1493 LOC, `inventory.rs` 1008 LOC,
  `combat.rs` 952 LOC). The file is currently invisible to the whole
  audit-suite's routing logic, not just this one skill's Dimension 11.
- **Evidence**:
  ```
  $ wc -l byroredux/src/extensions.rs
  10652 byroredux/src/extensions.rs
  $ grep -n "SandboxRuntime" byroredux/src/extensions.rs | head -3
  22:use byroredux_mod_runtime::{... SandboxError, SandboxRuntime};
  330:    runtime: SandboxRuntime,
  369:            runtime: SandboxRuntime::new(sandbox_config)?,
  $ grep -n "mod extensions" byroredux/src/main.rs
  28:mod extensions;
  $ grep -n "extensions" .claude/commands/_audit-common.md
  (no output)
  ```
  Spot-checked (not a full audit of the 10,652 lines): host-registration
  functions in `extensions.rs` gate on `grants.contains(SCRIPT_FUNCTIONS_REGISTER_CAPABILITY)`
  / `grants.contains(CONSOLE_REGISTER_CAPABILITY)` (lines 445, 612, 629) —
  consistent in shape with the check-before-act capability pattern verified
  directly in `crates/mod-runtime` itself (Dimension 11 below). The
  `byroredux-mod-runtime` capability catalog and test suite have both grown
  substantially to support this consumer (test count 23 → 66 since the
  2026-08-30 report).
- **Impact**: An auditor following the skill's current "audit as a contract,
  not a live path" framing will under-invest scrutiny on a trust boundary that
  is now live, native, and two days old, with real blast radius (community/
  guest code driving a 10k-LOC bridge into ECS events/commands). This is a
  **documentation/coverage-gap finding**, not a confirmed bug in
  `extensions.rs` itself — this audit did not perform a full line-by-line
  review of the 10,652-line file (reasonably out of scope for one dimension of
  one audit pass); the capability-gating shape spot-checked looks consistent
  with the established trust model.
- **Related**: #3748 (closed, established the prior "no consumer" state this
  finding supersedes).
- **Suggested Fix**: Update `audit-safety/SKILL.md` Dimension 11 to name
  `byroredux/src/extensions.rs` as the live consumer and drop the "contract,
  not a live path" framing; add a layout-map row for it in
  `_audit-common.md`. Separately (a process suggestion, not a code fix): given
  its size and freshness, `extensions.rs` is a strong candidate for a
  dedicated, deeper safety/security pass beyond what a single `/audit-safety`
  dimension budget covers.

---

# Per-Dimension Detail

## Dimension 1 — FFI Lifetime Safety — PASS, no findings

- **`crates/fsr3-sys`** (only live FFI crossing): exactly two `unsafe fn`
  (`Context::create` at `lib.rs:379`, `Context::dispatch` at `lib.rs:408`),
  both carrying full `# Safety` doc sections stating real lifetime contracts
  (handles must outlive the `Context`; device must be idle wrt FSR resources
  before `Drop`; dispatch handles must belong to the creating device).
  Verified `VulkanContext::drop` (`context/teardown.rs:190-199`) calls
  `device_wait_idle()` FIRST, satisfying the idle-before-destroy precondition,
  and that FSR context retirement (`teardown.rs:267-268`) runs before
  `destroy_device(None)` (`:414`).
- **cxx-bridge scope guard holds.** `crates/cxx-bridge/src/lib.rs` (full
  26-line file read) still exposes exactly one bridge fn,
  `native_hello() -> String`. No pointer/slice/`Box` signature exists.
- **Ruffle/wgpu boundary** (`crates/ui/src/player.rs`): zero `unsafe` blocks
  (grep confirmed). `render()` (line 448-509) returns `Option<&[u8]>` borrowed
  from its own owned `pixel_buffer`, populated via `copy_from_slice` BEFORE
  the borrow is returned — no use-after-free window. `shared_descriptors()`
  (line 94-109) is a process-wide `OnceLock` holding its OWN wgpu
  instance/device (a separate GPU context from the engine's `ash::Device`) —
  no teardown-ordering hazard with `VulkanContext::drop()`.

## Dimension 2 — Memory Corruption / UB — PASS, no findings

- **ECS cached-pointer contract** (#35/#1367): `World::get`
  (`crates/core/src/ecs/world.rs:362`) returns `Option<ComponentRef<'_, T>>`.
  `QueryRead`/`QueryWrite`/`ComponentRef` (`query.rs`) each hold their lock
  guard as a struct field alongside a cached pointer resolved once in `new()`;
  every deref carries a correct SAFETY comment (verified all four sites
  directly, `query.rs:59-64, 131-135, 140-143, 285-289`).
- **`#[repr(C)]` GPU-struct soundness**: zero `[f32; 3]` occurrences in
  `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (grep, whole file).
- **NIF bulk POD reads**: `read_pod_vec`/`read_pod_vec_from_cursor` both use
  `checked_mul` overflow guards before allocation; `T: AnyBitPattern` is a
  sealed trait.
- **sfmaterial enum decode**: `BuiltinType::from_u32` is a checked `match`
  with an `Err` arm; zero `transmute` in the crate.
- **pex opcode transmute**: range-checked (`byte >= MAX_OPCODE`) before the
  transmute; manually counted the `OpCode` enum body — 51 variants, only
  `Nop = 0` explicit, contiguous 0..=50, matches `MAX_OPCODE = 51`.
- **`crates/bsa/src/safety.rs` bounds**: `MAX_ENTRY_COUNT` (10M),
  `MAX_CHUNK_BYTES` (1 GiB), `MAX_RECORD_TOTAL_BYTES` (2 GiB), and
  `inflate_bounded` all confirmed present and correctly shaped. ESM
  counterpart `read_sub_records`/`record_inflation_ceiling` (#3399) confirmed
  same-shape.
- **LZ4 `safe-decode` pin**: workspace `Cargo.toml:160-166` pins
  `lz4_flex = { version = "0.11", default-features = false, features = [...,
  "safe-decode", "checked-decode"] }`; sole dependent is `crates/bsa`
  (confirmed no other Cargo.toml references it).
- **Byte-range `&str` slicing sweep** (#3391 pattern): every `&ident[a..b]`
  hit on `&str` (not `&[u8]`) across `crates/nif`, `crates/bsa`,
  `byroredux/src/asset_provider` traced individually — all slice at
  `rfind`-returned (always-char-boundary) offsets or after confirming an
  ASCII prefix. **No new instances found.**
- **Stack-overflow / recursion bounds**: `MAX_GRUP_NESTING_DEPTH = 64`
  (`crates/plugin/src/esm/reader.rs:73`) and `MAX_COLLISION_SHAPE_DEPTH = 64`
  (`crates/nif/src/import/collision/shape.rs:28`) both confirmed enforced.
- **Water-adjacent** (explicit ask): `crates/physics/src/water.rs` has
  **zero** `unsafe` blocks. The WATR ESM record parser's one
  length-relative slice (`&sub.data[sub.data.len() - 2..]`,
  `crates/plugin/src/esm/records/misc/water.rs:1405`) is guarded by
  `sub.data.len() >= 2` immediately before — no underflow panic possible.

## Dimension 3 — Memory & Resource Leaks — PASS, no findings

- **Rapier release on cell unload** (#1520): `release_victim_rapier_bodies`
  runs before `despawn_batch` in `byroredux/src/cell_loader/unload.rs:381,387`.
  Ran the guard test suite directly: `cargo test --bin byroredux
  rapier_release_tests` → **9 passed, 0 failed**.
- **Deferred-destroy drain** (#418/#732): exactly three production
  `DeferredDestroyQueue<T>` instantiations (mesh buffers, BLAS entries, BLAS
  scratch — unchanged). **File-location drift** (not a bug): the fence-wait →
  tick sequence was extracted from `draw.rs` into a new file
  `context/sync_and_acquire_frame.rs` by commit `7463204e` (2026-09-02, after
  the 2026-08-30 report) as part of a `draw_frame`-splitting refactor; the
  invariant itself (tick runs strictly after `wait_for_fences`) is intact and
  re-confirmed by direct read, with the #418 rationale restated in-code at
  the new location.
- **`AllocatorResource` drop ordering** (#1406): both the orderly
  (`app_events.rs:66-69`) and panic-unwind (`impl Drop for App`,
  `main.rs:622-651`) paths remove the resource before `renderer.take()`,
  confirmed by direct read of both sites.
- **GPU allocation inventory**: spot-checked destroy-fn presence for
  bloom/caustic/water_caustic/svgf/taa/gbuffer/restir/volumetrics — all
  present. Water-adjacent: `water_caustic.rs::recreate_on_resize` and
  `caustic.rs::recreate_on_resize` both drain and destroy all old per-FIF
  slots before creating replacements (no resize leak).
- **CPU-side unbounded growth**: `MaterialTable::clear()` confirmed still the
  first material-table operation of every frame (`byroredux/src/render/mod.rs:853`).
  `AnimationClipRegistry::release()` still never recycles a slot index — this
  is a **documented, deliberate design trade-off** with observability added
  (`stub_slot_count`/telemetry), tracked as **Existing: #2689** (closed via
  "surface AnimationClipRegistry's stranded stub-slot count" — the
  disposition was observability, not elimination, and the design doc is
  explicit that this is intentional).

## Dimension 4 — Unsafe-Block Discipline — PASS, no findings

- Scripted sweep of every `unsafe {` block (excluding `unsafe fn`/`impl`/
  `trait` declarations) across `crates/`, `byroredux/src/`, `tools/`:
  **738 real blocks** found (up from 724 at the 2026-08-30 baseline —
  consistent growth, not a regression). 21 candidates initially flagged as
  "no nearby SAFETY comment"; manually inspected all 21 — 19 are genuine
  SAFETY comments the scan window (too short) missed, and 2 are false
  regex matches (a prose comment mentioning `` `unsafe { ... }` `` and a test
  string literal). **Zero real gaps.**
- High-risk-pattern sweep: `align_to`, `MaybeUninit`, `assume_init`,
  `get_unchecked`, `Box::from_raw`, `Box::into_raw`, `static mut`,
  `write_unaligned` — zero occurrences workspace-wide. `transmute` — exactly
  one real site (`crates/pex/src/opcode.rs:136`, verified sound). `set_len` —
  exactly one real site (`crates/nif/src/stream.rs:829`, verified sound).

## Dimension 5 — Vulkan Spec Compliance — PASS, no findings

- **SPIR-V reflection binding-drift guard**: `cargo test -p byroredux-renderer
  scene_descriptor_reflection` → **5 passed** (covers both RT-enabled/disabled
  × triangle/water shader permutations — water-adjacent confirmed too).
- **Non-RT GPU rejection** (regression of the closed SAFE-D5-01/#3759):
  `is_device_suitable` (`crates/renderer/src/vulkan/device.rs:404-455`)
  rejects a device outright (`return Ok(None)`) when
  `!ray_query_supported`, with an in-code citation of the VUID this avoids.
  Confirmed a genuine device-selection fix, not a speculative pipeline
  restructure.
- **TLAS resize wait** (#1390): `device.device_wait_idle()` gated on
  `retiring_old`, called before old TLAS/scratch allocation is freed
  (`acceleration/tlas.rs:993-998`).
- **Queue submission ordering**: wait-before-signal, per-image semaphores,
  the image-fence-aliasing guard, and the #952 `reset_fences`-before-submit
  placement are all intact — confirmed directly, noting the frame-start half
  moved to a new file `context/sync_and_acquire_frame.rs` (2026-09-02 split,
  post-dates the last report).
- **`VOLUMETRIC_OUTPUT_CONSUMED`** (`volumetrics.rs:579` = `true`) is read
  correctly by `context/post_passes.rs:514`, not assumed.
- **CLEAR-before-COMPUTE** (water-adjacent): `caustic.rs` clears its R32_UINT
  accumulator every frame before the sole `imageAtomicAdd` use in
  `caustic_splat.comp`; both `initialize_layouts` fns (caustic + water_caustic)
  present for the one-time UNDEFINED→GENERAL transition.
- Drop ordering (device-destroy-last) reconfirmed via the same
  `context/teardown.rs` read used in Dimension 1.
- **Substantial `context/` file split** since 2026-08-30 (new files:
  `sync_and_acquire_frame.rs`, `begin_frame_recording.rs`,
  `build_and_upload_instances.rs`, `dispatch_skin_and_cluster.rs`,
  `geometry_pass.rs`, `render_debug.rs`, `assemble_camera_and_lights.rs`) — a
  pure reorg; every invariant checked was confirmed to have moved intact.

## Dimension 6 — R1 Material Table Layout Soundness — PASS, no findings

Ran the full guard-test set directly:
```
cargo test -p byroredux-renderer -- gpu_material          -> 7 passed
cargo test -p byroredux-renderer -- gpu_instance gpu_camera gpu_light -> 17 passed
```
`GpuMaterial` confirmed still 432 B (`gpu_material_size_is_432_bytes`,
`gpu_material_field_offsets_match_shader_contract` both pass). `MAX_MATERIALS
= 16384` confirmed (`scene_buffer/constants.rs:192`). The 2026-08-30 report's
LOW doc-rot findings on `GpuCamera` (352→368 B) and the missing `GpuLight`
lockstep test are both confirmed FIXED and now test-covered
(`gpu_camera_is_368_bytes`, `gpu_light_glsl_copies_stay_in_lockstep`,
`gpu_light_is_64_bytes` all pass) — closed issues #3447/#3450/#3763
independently reconfirmed via passing tests, not just issue state.

## Dimension 7 — RT IOR-Refraction Safety — PASS, no findings

```
cargo test -p byroredux-renderer -- glass   -> 18 passed
cargo test -p byroredux-renderer -- refract -> 11 passed
cargo test -p byroredux-renderer -- dbg_bit -> 3 passed
```
`MAX_REFRACT_PASSTHRUS = 8` (`triangle.frag:2022`) still the compile-time loop
bound; `materialKind == MATERIAL_KIND_GLASS` check present at 3 sites; the
phantom `REFRACT_PASSTHRU_BUDGET = 2` string-absence assertion (#3052) still
passes. `GLASS_RAY_BUDGET` has drifted to `2_097_152` (from the skill's
historical "8192" figure) — this is expected drift the skill text explicitly
anticipates ("check the constant by name"), not a finding. `DBG_VIZ_GLASS_PASSTHRU
= 0x80` bit-collision checking is now fully test-automated
(`dbg_bits_are_single_bit_and_pairwise_disjoint`,
`dbg_bits_catalog_covers_every_dbg_constant`), a strengthening since the last
report's manual 32-value enumeration.

## Dimension 8 — NPC/Animation Spawn Safety — PASS, no findings

```
cargo test -p byroredux-nif --lib -- bspline flt_max      -> 29 passed
cargo test -p byroredux-core --lib -- animation::registry -> 11 passed
cargo test -p byroredux-core --lib -- skin_slot_pool      -> 24 passed
cargo test --bin byroredux bone_palette_overflow          -> 2 passed
```
The B-spline `FLT_MAX_SENTINEL` (#772), the `AnimationClipRegistry`
case-insensitive dedup (#790), and the `MAX_TOTAL_BONES`
overflow-warn/fallback-to-bind-pose guard (`SkinSlotPool`) are all confirmed
intact via passing regression tests.

## Dimension 9 — NIFAL NaN/Inf Boundary — PASS, no findings (water-adjacent emphasis)

- `translate_material` (`byroredux/src/material_translate.rs:583-584`) seeds
  `f32::NAN` into metalness/roughness on missing overrides; `Material::resolve_pbr`
  (`crates/core/src/ecs/components/material.rs:1275-1309`) is confirmed the
  sole detector, with an unconditional final `.clamp()` regardless of branch
  (verified the `f32::clamp` panic precondition — `min`/`max` NaN — cannot
  fire here, both are float literals; a NaN *receiver* is replaced by the
  `is_nan()` branch before the clamp runs).
- Collision translate finiteness (`BhkMultiSphereShape`/`BhkConvexListShape`)
  confirmed gated per-shape, with an independent second line of defense at
  the Rapier boundary (`clamp_shape_extent`, `crates/physics/src/convert.rs`).
- Particle emitter finiteness (`extract_emitter_params`/`extract_emitter_rate`)
  confirmed exhaustive, including the `FLT_MAX`-sentinel rejection.
- **Water-adjacent deep-dive** (explicit extra-attention ask for this suite):
  - `crates/physics/src/water.rs::buoyancy_force`/`current_force`/
    `submerged_fraction`: all confirmed bounded — `submerged_fraction` clamps
    to `[0,1]`, `current_force` finiteness-checks every input and uses
    `normalize_or_zero()` (never a raw `normalize()` that NaNs a zero
    vector), `submerged_fraction`'s AABB-height division floors the
    denominator at `1e-6`.
  - **Investigated and resolved a speculative concern**: could
    `byroredux/src/systems/water.rs::disturbance_rate`'s `cam_pos.x.clamp
    (volume.min[0], volume.max[0])` panic on an inverted `WaterVolume`
    (`min > max`)? Traced every `WaterVolume { ... }` construction site
    workspace-wide: **all** sites in `physics/water.rs` and
    `systems/water.rs` are inside `#[cfg(test)] mod tests` blocks (confirmed
    by locating each file's `mod tests` line and checking every construction
    site falls after it); the **only** production construction site is
    `byroredux/src/cell_loader/water.rs` (2 sites), built from
    `terrain_water_components`'s flood-fill min/max accumulation, which is
    **structurally guaranteed ordered** (min/max start equal, then only ever
    widen via `.min()`/`.max()`) — not merely runtime-checked. **No reachable
    panic path exists.** Not filed as a finding.
  - `byroredux/src/systems/character.rs` swim/drowning path
    (`swim_vertical_velocity`, `advance_breath`, `water_damage_for_contact`):
    every branch ends in `.clamp()`/`.max()`; no division anywhere.

## Dimension 10 — debug-ui Teardown & Shared-Allocator Safety — PASS, no findings

`DebugUiState` confirmed CPU-only (zero `vk::`/`ash::` refs). `EguiPass`
teardown (`context/teardown.rs:205`) confirmed to run immediately after
`device_wait_idle()` and well before `destroy_device`. One-frame deferred
texture free and the tightly-scoped queue mutex (#1713/CONC-D1-01) both
reconfirmed by direct read of `egui_pass.rs::dispatch`.

## Dimension 11 — Sandboxed Mod Runtime Trust Boundary — 1 finding (LOW, doc-rot)

Zero `unsafe`, no WASI transitively (`cargo tree` confirmed), check-before-act
capability gating, zero shared mutable state, both-direction resource-limit
validation, and the compile-time size bound on untrusted input are all
confirmed intact — `cargo test -p byroredux-mod-runtime` → **66 passed** (up
from 23 at the 2026-08-30 baseline). See **DOC-ROT-1** above: the skill's
"no consumer in the engine" premise no longer holds — `byroredux/src/extensions.rs`
(10,652 LOC, landed 2026-09-03) is now a live, native consumer.

---

## Regression-guard confirmation table (this run vs. 2026-08-30 baseline)

| Guard | 2026-08-30 status | This run |
|---|---|---|
| fsr3-sys `# Safety` docs | PASS | **PASS** (re-verified) |
| cxx-bridge no-pointer placeholder | PASS | **PASS** (re-verified) |
| ECS cached-pointer contract (#35/#1367) | PASS | **PASS** (re-verified) |
| NIF POD read overflow guards | PASS | **PASS** (re-verified) |
| sfmaterial checked match | PASS | **PASS** (re-verified) |
| pex opcode transmute guard | PASS | **PASS** (re-verified, 51 variants recounted) |
| bsa/safety.rs bounds | PASS | **PASS** (re-verified) |
| LZ4 safe-decode pin | PASS | **PASS** (re-verified) |
| #3391 byte-range slicing fix | PASS | **PASS** (swept again, no new instances) |
| GRUP/collision-shape recursion bounds | PASS | **PASS** (re-verified) |
| Rapier release on cell unload (#1520) | PASS | **PASS** (9/9 tests) |
| Deferred-destroy drain (#418/#732) | PASS | **PASS** (location moved to `sync_and_acquire_frame.rs`, invariant intact) |
| AllocatorResource drop ordering (#1406) | PASS | **PASS** (re-verified) |
| MaterialTable per-frame clear | PASS | **PASS** (re-verified) |
| AnimationClipRegistry monotonic growth (#2689) | Existing, tracked | **Existing, tracked** (unchanged disposition) |
| Unsafe-block SAFETY-comment coverage | 724/724 | **738/738** (re-verified, grew with codebase) |
| Non-RT GPU rejection (SAFE-D5-01/#3759) | Fixed (HIGH, closed) | **PASS** (fix re-confirmed sound) |
| TLAS resize wait (#1390) | PASS | **PASS** (re-verified) |
| Queue submission wait-before-signal | PASS | **PASS** (location moved, invariant intact) |
| `GpuMaterial` 432 B + offset pins | PASS | **PASS** (7/7 tests) |
| `GpuCamera` 352→368 B doc-rot (SAFE-D6-01) | Fixed (LOW, closed) | **PASS** (fix re-confirmed via passing test) |
| `GpuLight` lockstep leg (SAFE-D6-02/#3763) | Fixed (MEDIUM, closed) | **PASS** (fix re-confirmed via passing test) |
| `MAX_REFRACT_PASSTHRUS`/glass identity check | PASS | **PASS** (re-verified) |
| `DBG_VIZ_GLASS_PASSTHRU` collision-free | PASS (manual) | **PASS** (now test-automated) |
| B-spline FLT_MAX sentinel (#772) | PASS | **PASS** (29 tests) |
| B-spline dequantised NaN guard (SAFE-D9-01/#3765) | Fixed (MEDIUM, closed) | **PASS** (fix re-confirmed) |
| `MAX_TOTAL_BONES` overflow guard | PASS | **PASS** (26 tests) |
| Embedded-clip #790 path memo (SAFE-D8-01/#3764) | Fixed (MEDIUM, closed) | **PASS** (registry tests green) |
| NaN-seed / `resolve_pbr` boundary | PASS | **PASS** (re-verified, water-adjacent too) |
| `SceneImportCache` uncapped (SAFE-D3-01/#3760) | Fixed (MEDIUM, closed) | **PASS** (re-verified: `DEFAULT_MAX_ENTRIES = 300`, `BYRO_NIF_CACHE_MAX` override, half-eviction-on-overflow shape confirmed present in `byroredux/src/scene_import_cache.rs`) |
| `write_mapped` false safety claim (SAFE-D4-01/#3761) | Fixed (MEDIUM, closed) | **PASS** (fix re-confirmed via #2683 guard test names) |
| debug-ui / EguiPass teardown ordering | PASS | **PASS** (re-verified) |
| mod-runtime: zero unsafe, no WASI, capability gating | PASS | **PASS** (66 tests, up from 23) |
| mod-runtime "no consumer" premise | Stated as still true | **STALE — see DOC-ROT-1** |
| CsgArchive::read_psg allocation bound (SAFE-D2-01/#3758) | Fixed (HIGH, closed) | **PASS** (re-verified: `read_psg`, `crates/bsa/src/csg.rs:224`, bounds `offset+len` against the archive's own PSG space via `checked_add`/`saturating_mul` BEFORE any allocation, with the #3758 rationale restated in-code) |

---

## Next step

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-09-04.md
```
