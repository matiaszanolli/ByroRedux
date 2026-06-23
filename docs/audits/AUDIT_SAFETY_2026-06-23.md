# Safety Audit — 2026-06-23

**Scope**: `unsafe` blocks, memory leaks, undefined behavior, Vulkan spec
compliance across all 21 crates + `byroredux/`. Includes the two newest crates
(`crates/pex` M47.2 decompiler, `crates/save` M45) and the uncommitted in-progress
M47.2 scripting work (`crates/scripting`), which compiles and is treated as
current code.

**Tree state**: `HEAD = 2d4c350d`, plus uncommitted M47.2 work in
`crates/scripting/src/{lib,quest_stages,fragment}.rs` and
`crates/scripting/src/translate/{mod,compose,effects,recognizers/quest_stage_gate}.rs`.

**Method**: All 10 skill dimensions run inline. Each regression-guard claim was
re-verified against current code and, where a test pin exists, the test was run
green. Findings deduped against the cached OPEN-issue list
(`/tmp/audit/issues.json`) and the closed-issue set via `gh`.

---

## Summary

**2 findings** (0 critical, 0 high, 1 medium, 1 low).

The safety posture is strong: every regression guard called out by the skill is
intact, and the recently-closed safety issues (#1406/#1477 allocator-before-device,
#1531 Rapier leak, #1533 precombine bounds, #1534 ragdoll finite guards, #1535
NaN glossiness, #1390 TLAS resize wait, #1269/#1270 recursion caps) are all still
in place with passing pins. The one open structural item is the residual
unsafe-without-SAFETY-comment population in the renderer FFI mass — partially
addressed by the closed #1644 (which fixed 124 of ~327), with a genuine residue
remaining. One new low-severity gap: the M47.2 `.pex` control-flow reconstructor
recurses on untrusted file input with no depth cap, unlike the NIF/Papyrus walkers
that already received caps under #1269/#1270.

---

## Findings

### SAFE-2026-06-23-01: Residual ~219 renderer `unsafe {` blocks lack a SAFETY comment

- **Severity**: MEDIUM
- **Dimension**: 4 — Unsafe-Block Discipline
- **Location**: `crates/renderer/src/` (worst offenders: `vulkan/composite.rs`,
  `vulkan/context/mod.rs`, `vulkan/context/helpers.rs`, `vulkan/texture.rs`,
  `vulkan/device.rs`, `vulkan/context/resize.rs`, `texture_registry.rs`,
  `vulkan/context/screenshot.rs`)
- **Status**: Existing: #1644 (CLOSED — partial fix) — reporting the un-fixed residue
- **Description**: The unified-severity Special Rules table sets "`unsafe` block
  without safety comment = MEDIUM". #1644 was closed by commit `607fafbb`
  ("add SAFETY comments to 124 renderer unsafe blocks"), which dented but did not
  eliminate the population the audit originally counted (~327). A re-count of the
  current tree (non-test files; `unsafe {` block-openers with no `SAFETY` within the
  preceding 6 lines) finds **219** remaining. A portion are batched-FFI false
  positives (one per-function SAFETY comment covering several consecutive ash
  `cmd_*` / `create_*` calls), but spot-checking confirms a large genuine residue:
  `texture_registry.rs` and `vulkan/context/screenshot.rs` carry **zero** SAFETY
  comments across all their `unsafe` blocks.
- **Evidence**:
  - `crates/renderer/src/texture_registry.rs` — 10 `unsafe {` / 0 SAFETY. e.g.
    `create_descriptor_set_layout` (~:297), `create_descriptor_pool` (~:313),
    `allocate_descriptor_sets` (~:324) all uncommented.
  - `crates/renderer/src/vulkan/context/screenshot.rs` — 5 `unsafe {` / 0 SAFETY.
    e.g. `create_buffer` (~:224), `destroy_buffer` (~:248, ~:272) uncommented.
  - Per-file genuine-residue counts (6-line window, non-test): composite.rs 17,
    context/mod.rs 16, context/helpers.rs 16, texture.rs 15, svgf.rs 13,
    skin_compute.rs 13, device.rs 13, context/resize.rs 13, texture_registry.rs 10,
    taa.rs 9, compute.rs 9, scene_buffer/upload.rs 8, caustic.rs 8, … (total 219).
- **Impact**: Defense-in-depth / maintainability gap, not a live UB. Each
  uncommented block is ash FFI whose invariant (live device, valid handle, correct
  call ordering) is real but undocumented, so a refactor can silently violate it.
  No correctness regression observed — these are sound calls today.
- **Related**: #1644 (closed, partial), #1432 (closed)
- **Suggested Fix**: Finish the #1644 sweep. Prioritise the zero-comment files
  (`texture_registry.rs`, `screenshot.rs`) and the high-residue creation/teardown
  paths; batch one SAFETY note per FFI cluster rather than per call.

### SAFE-2026-06-23-02: `.pex` control-flow reconstructor recurses on untrusted input with no depth cap

- **Severity**: LOW
- **Dimension**: 2 — Memory Corruption / UB (stack-overflow facet)
- **Location**: `crates/pex/src/decompile/control_flow.rs::rebuild` (recursive
  self-calls at ~:146, ~:154, ~:163, ~:164)
- **Status**: NEW
- **Description**: The M47.2 `.pex` decompiler's control-flow reconstruction
  (`Reconstructor::rebuild`) recurses to nest if/while regions. A `.pex` is
  untrusted on-disk file input — the same threat class that earned the NIF
  scene-graph/collision walkers a `MAX_NIF_NODE_DEPTH` cap (#1269) and the Papyrus
  expression parser a `MAX_EXPR_DEPTH = 256` cap (#1270). `rebuild` has **no
  equivalent depth bound**. The recursion is *normally* terminating because each
  nested region is a strictly-shrinking block-index sub-range and malformed ranges
  hit the `end < start` / `fail()` guards (`:86`, `:93`, `:103`, …), but a
  pathologically deep (yet structurally valid) nested-control-flow `.pex` could
  drive native-stack recursion to overflow → abort.
- **Evidence**:
  - `crates/pex/src/decompile/control_flow.rs:146` `let body = self.rebuild(body_start, body_end)?;`
  - lines 154, 163, 164 — three more self-recursive `rebuild(...)` calls for
    while-body / if-body / else-body regions.
  - No `depth` parameter, no `MAX_*` constant, no counter anywhere in the file
    (`grep -n "depth\|MAX_\|recursion"` returns only comments/doc text).
  - Contrast: `crates/nif/src/import/walk/mod.rs:186` threads `depth: u32` and
    bails at `MAX_NIF_NODE_DEPTH` (#1269); `crates/papyrus/src/parser/expr.rs:36`
    gates on `MAX_EXPR_DEPTH` (#1270).
- **Impact**: A crafted/corrupt `.pex` could crash the decompiler via stack
  overflow. Severity is LOW (not MEDIUM) because: (a) the shrinking-range invariant
  makes a *valid* file's recursion depth bounded by script size, so triggering it
  needs a deliberately adversarial file, and (b) the outcome is a clean abort, not
  memory corruption. It is the direct analogue of two issues that were nonetheless
  fixed, so consistency argues for closing it.
- **Related**: #1269 (NIF/collision recursion cap), #1270 (Papyrus expr cap)
- **Suggested Fix**: Thread a `depth: u32` through `rebuild` and return `self.fail()`
  past a `MAX_PEX_REGION_DEPTH` constant (mirror the #1269/#1270 pattern). Add a
  unit test with a synthetic deeply-nested CFG asserting graceful error rather than
  overflow.

---

## Verified-Intact Regression Guards (PASS — not findings)

Every item below was re-checked against current code; cited tests were run green.

**Dimension 1 — FFI lifetime (cxx)**: `crates/cxx-bridge/src/lib.rs` still exposes
only `native_hello() -> String` inside `unsafe extern "C++"`. No `*const`, `&[u8]`,
`Box<…>`, or Rust-reference-taking C++ fn. The no-pointer placeholder scope guard
holds — dimension remains dormant.

**Dimension 2 — Memory corruption / UB**:
- ECS cached-pointer contract (`crates/core/src/ecs/query.rs`): `QueryRead`,
  `StorageRefMut`, and `ComponentRef` all cache a `*const`/`*mut` resolved once in
  `new()` from a `RwLock*Guard` held as a struct field; SAFETY comments tie pointer
  validity to that guard, and `&mut *self.storage` is gated by `&mut self`. No
  guard-drop-before-pointer regression (the #35 unsound pattern stays excised).
- pex `OpCode::from_u8` (`crates/pex/src/opcode.rs:130`): the real
  `transmute::<u8, OpCode>` is guarded by `byte >= MAX_OPCODE` (=51) and the enum
  has **51 contiguous** discriminants (`Nop = 0` then implicit +1, no gaps). Both
  transmute preconditions hold. Pin `from_u8_round_trips_and_rejects_oob` green.
- sfmaterial `BuiltinType::from_u32` (`crates/sfmaterial/src/types.rs:37`): checked
  `match` with `_ => Err(UnsupportedBuiltin { raw })` — no transmute (doc prose is
  aspirational; #1396 already corrected the stale claim).
- NIF bulk POD reads (`crates/nif/src/stream.rs`): `read_pod_vec<T: AnyBitPattern>`
  has the `count.checked_mul(size_of::<T>())` overflow guard and a sealed local
  `unsafe trait AnyBitPattern` restricting `T` to bit-pattern-safe types
  (blocks `read_pod_vec::<bool>`).
- `crates/save/src/` — **zero `unsafe`** (confirmed). M45 stays safe-Rust.
- M47.2 `crates/scripting/src/` (in-progress, uncommitted) — **zero `unsafe`**.
- Recursion caps: NIF walk `MAX_NIF_NODE_DEPTH` (#1269), Papyrus expr
  `MAX_EXPR_DEPTH = 256` (#1270) both present. (The pex gap above is the lone
  uncapped recursion on untrusted input.)

**Dimension 3 — Leaks / drop ordering**:
- AllocatorResource-before-device (#1406 / #1477): `byroredux/src/main.rs`
  `impl Drop for App` (`:457`) explicitly `remove_resource::<AllocatorResource>()`
  **then** `renderer.take()`, no longer relying on field order. Hazard closed.
- Rapier release on cell unload (#1531): `release_victim_rapier_bodies`
  (`byroredux/src/cell_loader/unload.rs:380`, wired at `:187`) cascades
  bodies/colliders/joints + ragdoll via `PhysicsWorld::remove_ragdoll`. All 7
  `rapier_release_tests` green.
- Deferred-destroy drain (#418 / #732): `tick_deferred_destroy` runs **after**
  `wait_for_fences` (`crates/renderer/src/vulkan/context/draw.rs:584`); shutdown
  `drain` exists with a `device_wait_idle` first (`context/mod.rs` ~:2430).

**Dimension 5 — Vulkan spec (testable items)**:
- TLAS resize `device_wait_idle` before freeing old allocation (#1390):
  `acceleration/tlas.rs:322` present.
- Volumetrics dispatch gate: `VOLUMETRIC_OUTPUT_CONSUMED` (`volumetrics.rs:143`)
  honored at both `draw.rs` call sites (`:2331`, `:3269`); `vol.dispatch()` only
  fires inside the gate.
- SPIR-V reflection pins (`scene_descriptor_reflection_tests`): 5 green —
  descriptor layout matches shader bindings for triangle + water, RT-on and RT-off.
- *Barrier / image-layout spec claims invisible to `cargo test` are NOT reported*
  per the No-Speculative-Vulkan-Fixes rule. Running a debug build with validation
  layers + RenderDoc is the sound evidence channel; no validation run was performed
  in this audit, so no barrier/layout findings are asserted.

**Dimension 6 — Material table layout**: `GpuMaterial` pinned at **300 B**
(`gpu_material_size_is_300_bytes`), per-field offsets pinned
(`gpu_material_field_offsets_match_shader_contract`), GLSL field-name + order pins,
all flat scalar `f32`/`u32`. 6 layout pins + the instance-layout cross-pin green.

**Dimension 7 — RT IOR/glass**: `GLASS_RAY_BUDGET = 1048576` enforced
(`triangle.frag:1193`), `DBG_VIZ_GLASS_PASSTHRU = 0x80` uncollided, Frisvad basis
active (`triangle.frag:1271`), passthru loop bounded by `REFRACT_PASSTHRU_BUDGET`.
(The #1438-documented atomicAdd overshoot of the budget is pre-existing and noted,
not re-reported.)

**Dimension 8 — NPC/animation spawn**: `MAX_TOTAL_BONES` overflow guard
(`byroredux/src/render/skinned.rs`, `Once`-gated warn at `:113`) — both
`bone_palette_overflow_tests` green (`over_capacity_breaks_loop_and_truncates_offsets`,
`at_capacity_fills_palette_completely`).

**Dimension 9 — NIFAL NaN boundary**: `material_translate.rs:157-160` seeds
`f32::NAN` then calls `resolve_pbr()` immediately; `Material::resolve_pbr`
(`crates/core/src/ecs/components/material.rs:638`) detects the NaN sentinels,
fills from the classifier (finite constants, #1535), and applies an unconditional
final `clamp` — no NaN reaches the SSBO. Ragdoll finite guards (#1534) present
(`ragdoll.rs:278`), precombine triangle-index bounds check (#1533) present
(`precombine.rs:156`).

**Dimension 10 — debug-ui teardown**: `DebugUiState` (egui) teardown not
re-audited beyond confirming the crate exists and holds the shared
`Arc<Mutex<allocator>>`; no change since prior audits and no new resource added.

---

## Coverage Note

All 21 `crates/` + `byroredux/` were swept for `unsafe` (Dimension 4 grep covers
the whole tree). `unsafe` totals: renderer 638, nif 11, core 6, byroredux 2,
plugin/pex/facegen/cxx-bridge 1 each; save 0, scripting 0, audio/bsa/bgsm/papyrus/
physics/spt/sfmaterial/debug-* 0. The non-renderer `unsafe` tail
(pex transmute, nif POD reads, core ECS cached pointers, plugin/facegen) was each
individually verified commented + sound.
