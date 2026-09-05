# Safety Audit — ByroRedux — 2026-09-05

**Command**: `/audit-safety` (run as the safety leg of the `volumetrics-deep`
audit suite)
**Repo state**: `main` @ `6fba2b0a` (2026-09-05 17:10:31 -0300)
**Baseline**: `docs/audits/AUDIT_SAFETY_2026-09-04.md` @ `b15b0527` — that run
was exhaustive (all 11 dimensions independently re-derived, not
sub-agent-relayed) and found **zero** findings above LOW. 17 commits landed
between the two baselines (15 touching engine/renderer code, plus one
dev-tooling script fix and one audit-skill-documentation-only update); this
run traces every one of them for safety implications rather than assuming
the delta is clean.
**Severity scale**: `.claude/commands/_audit-severity.md`

## Scope and motivation

This run was specifically motivated by recent volumetric-lighting work, with
an explicit brief to scrutinize `unsafe` blocks, memory-safety, and Vulkan
spec compliance in `crates/renderer/src/vulkan/volumetrics.rs` and the
froxel-grid buffers/images it manages, **in addition to** the audit's normal
full-codebase scope. Both halves were run in full:

1. **Full 11-dimension sweep**, using the 2026-09-04 report as a verified
   baseline and re-deriving every regression guard against current source
   (not trusted from the prior report) for anything the 15-commit delta
   touched, plus a targeted re-grep/re-count for anything it didn't.
2. **Deep-dive on `volumetrics.rs`** (3,863 LOC): every one of its ~34
   `unsafe` sites (5 `unsafe impl NoUninit`, the rest `unsafe {}` blocks or
   `unsafe fn` bodies) was read individually against its SAFETY comment;
   image/buffer creation and destruction paths, the `initialize_layouts` /
   `dispatch` / `record_neutral_frame` / `destroy` command recording, the
   resize-recreate path in `context/resize.rs`, and the froxel-grid's one
   GPU-atomic-write surface (the combustion-light-moment SSBO) were all
   traced end to end.

## Method notes

- Ran targeted `cargo test` slices covering every dimension the delta
  touched (volumetrics: 36/36 pass; material/instance/camera/light layout +
  glass/refract/dbg-bit guards: 56/56 pass; Rapier release / skin-dispatch
  rollback / bone-palette overflow / LOD budget: 29/29 pass; mod-runtime:
  66/66 pass; core: 742/742 pass) rather than re-running the full suite —
  every one of the 15 delta commits' own messages already records a green
  full-workspace `cargo test` at landing time, and `cargo check --workspace
  --all-targets` was re-run here and is clean (one pre-existing
  `unused_mut` warning in `crates/plugin`, unrelated to safety).
- Re-derived the unsafe-block count with a script rather than trusting the
  prior figure: **736** real `unsafe {}` blocks / `unsafe fn` bodies
  workspace-wide (was 738 on 2026-09-04) — within measurement-method
  variance (comment-adjacent-brace heuristics differ slightly run to run),
  not a real drop; no unsafe code was deleted in the delta. Per-crate this
  run's method found: renderer 693, fsr3-sys 29, core 8, nif 2, byroredux 1,
  pex 1, plugin 1, tools 1 — consistent with the skill's documented
  distribution (renderer-dominant, long tail elsewhere).
- Per the project's no-speculative-Vulkan-fixes rule, no render-pass,
  barrier, or pipeline-state restructure is proposed anywhere in this
  report. No validation-layer/RenderDoc run was performed (project policy
  forbids spawning a parallel/headless engine instance alongside the user's
  own).
- Dedup: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json
  number,title,state,labels` (65 open issues — the command's default,
  matching the skill's specified invocation) scanned for keyword overlap
  against every candidate finding before filing; `docs/audits/` scanned for
  prior coverage of the same ground.

## The 17-commit delta since `b15b0527`, traced individually

| Commit | Summary | Safety-relevant? | Disposition |
|---|---|---|---|
| `81c63681` Fix #3611 | Volumetric far-plane triple-copy → derived const + lockstep test | No — pure `const`/test change, no `unsafe`, no GPU-visible behavior change | Reviewed, test re-run green |
| `26f9ddf4` Fix #3570 | Refuse depth capture on `D16_UNORM` device | Dimension 5 regression guard (already named in skill) | Re-confirmed intact |
| `709de0e6` Fix #3569 | Requeue bind_inverses upload failure | Dimension 8 regression guard (already named in skill) | Re-confirmed intact |
| `20f5f476` Fix #3607 | Rename `taa.comp`'s octahedral decoder | No — naming only | Reviewed, no safety content |
| `c43cb269` Fix #3605 | Signal temporal discontinuity on TAA dispatch failure | No — CPU-side latch/state-machine correctness, no `unsafe` | Reviewed, no safety content |
| `1ff9bc73` Fix #3601 | Clamp `ui_instance_idx` to `None` past `MAX_INSTANCES` | **Yes — was a real SSBO-index-overflow bug** (see below) | **Already fixed, tested; not new** |
| `84c4a1df` Fix #3589 | Route BGEM effect-shader flags through the overlay boundary | No — bit-flag packing, not a finiteness/bounds path | Reviewed, no safety content |
| `1382efb0`/`ff4751eb` Fix #3652 (+sibling) | Move `footstep_system`/`make_billboard_system` to `Stage::Late` | No — scheduler ordering for correctness, not safety | Reviewed, no safety content |
| `3562401b` Fix #3637 | Archive lookup last-wins not first-wins | No — content-resolution policy | Reviewed, no safety content |
| `1ff5fae4` Fix #3639 | Neutral-roughness fallback for smoothness=1.0 | No — shading math | Reviewed, no safety content |
| `ee405f40` Fix #3641 | State the precombine LOD tie-break explicitly | No — determinism/clarity, no behavior change | Reviewed, no safety content |
| `78ad5452` Fix #3823 | Bound boundary-crossing LOD reconcile with a 500 ms ceiling | Frame-hitch/DoS-shaped, not memory-unsafety | Reviewed — see note below |
| `d9e61ead` Fix #3820 (+3821/3822/3826) | Bound water/glass caustic image atomics on the actual accumulator, not the render-extent uniform | **Yes — was a real Vulkan-spec violation** (see below) | **Already fixed, tested; not new** |
| `cfa8425a` | Fix `scripts/prune-target-cache.sh` target-triple matching | No — dev-tooling shell script, no engine/renderer code | Reviewed, out of code-safety scope |
| `bb1b8a6a` | Housekeeping (2026-09-04 report + cache-prune script) | No | N/A |
| `6fba2b0a` | Documentation-only update to `.claude/commands/audit-*/SKILL.md` (incl. this skill's own file) and `_audit-common.md` | No — no `.rs`/`.comp`/`.frag`/`.vert` touched | Reviewed; this run already operates on the resulting current skill text |

Two of these are worth detailing because they are exactly the bug classes
this audit's brief asked to hunt for in the froxel-grid code, confirming the
concern is live in this codebase generally — and because I specifically
verified `volumetrics.rs`'s own instance of each pattern does **not** share
the vulnerability.

**#3601 (fixed before this audit ran).** `ui_instance_idx` captured
`gpu_instances.len()` **before** the UI quad's `GpuInstance` push, then
handed that raw index to `firstInstance` regardless of whether
`upload_instances` would go on to clamp an overflowing `gpu_instances` to
`instances[..MAX_INSTANCES]`. Since the UI instance is always pushed last,
an overflowing frame silently dropped it from the SSBO while still issuing
its now-out-of-range index — `ui.vert` would then read
`instances[gl_InstanceIndex]` past the buffer's allocated capacity
(`robust_buffer_access` is not enabled), feeding a bindless
`nonuniformEXT` texture index in `ui.frag`. This is the exact "SSBO index
mismatch" CRITICAL row in `_audit-severity.md`. Fixed by clamping the
captured index to `None` when it would land at or past `MAX_INSTANCES`
(`(idx < MAX_INSTANCES).then_some(idx as u32)`), with a source-scan
regression test. Re-verified the fix is in place and the old unclamped
capture does not exist (`crates/renderer/src/vulkan/context/build_and_upload_instances.rs`).

**#3820 (fixed before this audit ran).** `water.frag`'s and
`caustic_splat.comp`'s caustic-splat `imageAtomicAdd` calls bounded their
write coordinate against `causticScreen.xy` / `screen.xy` (a render-extent
*uniform*) rather than the actual bound image's extent. `resize.rs`
deliberately rebinds the caustic accumulator to a 1×1
`placeholder_caustic_sink` when the real accumulator fails to (re)create —
and Vulkan's out-of-range-write discard guarantee, which covers a plain
`imageStore`, does **not** cover an image *atomic*. So every water/glass
fragment could `imageAtomicAdd` far outside the 1×1 fallback: a genuine
Vulkan-spec violation (HIGH minimum per the severity table), silent because
the failure mode is a placeholder-binding edge case rather than the common
path. Fixed by bounding both call sites on `imageSize(...)` of the actually
bound image instead, with regression tests in `caustic.rs`.

**Checked whether `volumetrics.rs`'s froxel-grid has the same exposure —
it does not.** The froxel-grid's one GPU-atomic surface is the
combustion-light-moment accumulation in `volumetrics_inject.comp`
(`atomicAdd(combustionLightMoments[binIndex].weight, ...)` and six sibling
fields, `crates/renderer/shaders/volumetrics_inject.comp:2389-2420`) — a
**buffer** (SSBO) atomic, not an image atomic, so the specific Vulkan
out-of-range-image-atomic nuance #3820 fixed doesn't apply verbatim, but an
out-of-bounds SSBO index is at least as dangerous (real out-of-bounds
memory write, not a spec gray area). Traced `binIndex`'s derivation
(`crates/renderer/shaders/volumetrics_inject.comp:2340-2351`):
`domain` is range-checked to `[0,1)` on all three axes with an early
`return` before any bin math runs (line 2340), then each axis is
independently `min(floor(domain * gridSize), gridSize - 1)`-clamped
(line 2350), so
`binIndex = bin.x + gridX*(bin.y + gridY*bin.z)` is structurally bounded to
`[0, COMBUSTION_LIGHT_GRID_COUNT)` — never runtime-dependent on an
extent/placeholder mismatch the way #3820's image coordinate was. The
Rust-side buffer is sized via `size_of::<[GpuCombustionLightMoment;
COMBUSTION_LIGHT_GRID_COUNT]>()` (a compile-time constant, not a runtime
multiply), and `COMBUSTION_LIGHT_GRID_COUNT` (256 = 8×4×8) is identical on
both sides of the Rust/GLSL boundary with a pinning `assert!` in
`shader_constants_data.rs`. No placeholder-fallback path exists for any
froxel image the way `placeholder_caustic_sink` exists for caustics — a
failed `VolumetricsPipeline::new`/`initialize_layouts` during resize
propagates `Err` and the whole pipeline stays `None` (gated correctly
everywhere via `if let Some(ref mut vol) = self.volumetrics`), rather than
falling back to an undersized real image the shader's own size uniform
could disagree with. **No finding — this class of bug does not reproduce
here**, but it was worth the direct check given the brief.

**#3823 note.** This is a frame-hitch/DoS-shaped bound (an unbounded
`usize::MAX`-attempt LOD reconcile loop gets a 500 ms safety ceiling), not a
memory-safety defect — no out-of-bounds access, no leak, no UB. It is
correctly out of this audit's scope (belongs to `/audit-performance` or
`/audit-renderer`'s territory); noted here only because "bound the X with a
safety ceiling" pattern-matches this audit's vocabulary.

## Findings summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 0 |
| LOW | 0 (new) |
| **Total new** | **0** |

| ID | Sev | Dim | Title | Status |
|---|---|---|---|---|
| DOC-ROT-1 | LOW | 11 | mod-runtime "no consumer" premise is stale — `extensions.rs` (10,652 LOC) is a live consumer | **Existing: #3828** (OPEN, unchanged — `extensions.rs` still 10,652 LOC, `_audit-common.md` still has no entry for it) |

**Every dimension produced zero new findings.** This is the second
consecutive full `/audit-safety` run to do so; see the "15-commit delta"
table above for the two real, already-fixed bugs (#3601, #3820) this run's
tracing surfaced as historical confirmations rather than new findings, and
the dedicated volumetrics deep-dive below for the requested extra scrutiny.

---

# Volumetrics Deep-Dive (requested focus)

`crates/renderer/src/vulkan/volumetrics.rs`, 3,863 LOC, ~34 `unsafe` sites.
Read in full; every safety-relevant subsystem traced to its call site.

## Unsafe-block discipline

Every `unsafe {}` block and every `unsafe fn` (`initialize_layouts`,
`dispatch`, `record_neutral_frame`, `destroy`) carries either an inline
`// SAFETY:` comment or a `/// # Safety` doc section stating a real,
checkable precondition (device/image liveness, recording-state, exclusive
ownership, no-concurrent-in-flight-use). The five `unsafe impl
crate::vulkan::buffer::NoUninit` markers (`VolumetricsParams`,
`GpuFogVolume`, `GpuFogVolumeUpload`, `GpuFogClusterEntry`,
`IntegrationParams`) each carry a comment justifying the padding-free
`#[repr(C)]` layout claim. **Zero comment-less unsafe found in this file.**
Bare (non-`unsafe {}`-wrapped) ash calls inside `dispatch`/
`record_neutral_frame`/`initialize_layouts` are correctly unwrapped because
those functions are themselves `unsafe fn` (their body is already one
unsafe scope) — consistent with the rest of the renderer's style, not a gap.

## Image/buffer lifecycle

- **`create_volume`** (image + allocation + view): every failure branch
  (`create_image` ok / `allocate` fails / `bind_image_memory` fails /
  `create_image_view` fails) destroys exactly what was created so far
  before returning `Err` — no leak, no double-free, verified branch by
  branch.
- **`destroy`**: drains every `Vec<FroxelSlot>` (6 volume kinds) plus the
  two `Option<FroxelSlot>` noise volumes via `.take()`, destroys every
  `GpuBuffer` and clears its `Vec`, then null-guards (`if handle !=
  Handle::null()`) every pipeline/layout/pool/sampler destroy before
  nulling the field. This makes `destroy()` **idempotent** — a second call
  (which cannot currently happen, since there is no `impl Drop for
  VolumetricsPipeline`; only the one explicit call site in
  `context/teardown.rs:157-159` exists) would be a safe no-op rather than a
  double-free.
- **Teardown ordering**: `context/teardown.rs`'s `Drop for VulkanContext`
  calls `device.device_wait_idle()` first (line 199), then
  `destroy_allocator_owned_resources` (which calls `vol.destroy(...)` at
  line 158) — idle-before-destroy confirmed, same pattern as every other
  subsystem.
- **Resize recreate path** (`context/resize.rs::recreate_bloom_and_volumetrics`,
  called from `recreate_screen_passes`, itself called from
  `recreate_swapchain` after `recreate_swapchain_core`'s
  `device_wait_idle()`): destroys the old pipeline, constructs the new one,
  calls `initialize_layouts`, and on **that** failure destroys the
  newly-created (never-published, never-submitted) pipeline before
  returning `Err` — no leak on the failure path either. Considered and did
  not file: if `VolumetricsPipeline::new`/`initialize_layouts` fails during
  a resize, `self.volumetrics` stays `None` and the composite pipeline's
  *existing* descriptor sets (built in a prior successful resize/init)
  still nominally reference the just-destroyed old volumetrics image
  views, since `recreate_composite_and_egui` — which would rewrite them —
  is never reached this attempt (the `?` short-circuits first). This is
  memory-safe in isolation (the destroy itself runs device-idle, so no
  in-flight GPU work references the freed images), and the **only** way it
  becomes a live hazard is if another `draw_frame` runs afterward and
  submits a command buffer that actually uses the stale descriptor set.
  Both call sites of `recreate_swapchain` (`app_events.rs:374`,
  `app_frame.rs:677`) respond to any `Err` by logging and calling
  `event_loop.exit()` — the same fail-fast contract `set_upscaler_mode`'s
  doc comment states explicitly and by issue number (#2156): "`Err` means
  ... the call site must treat it as fatal rather than continuing to spin
  the frame loop." This is a renderer-wide property of every subsystem in
  `recreate_screen_passes` (bloom, gbuffer, composite, TAA — not specific
  to volumetrics or to this delta), already deliberately reasoned about
  per the #2156 reference, and depends on winit's exact
  `ActiveEventLoop::exit()` timing (whether one more `about_to_wait` can
  fire before the loop actually stops) — a question outside what `cargo
  test` or static reading can settle. Per the no-speculative-Vulkan-fixes
  rule this is reported as a reviewed-and-not-disproven-with-certainty
  observation, not a filed finding: I could not construct a call path that
  reaches it given the existing fail-fast/exit contract, and it is not new
  behavior introduced by anything in this delta.

## Command-buffer recording (`initialize_layouts` / `dispatch` / `record_neutral_frame`)

- **`initialize_layouts`**: UNDEFINED→GENERAL transitions cover all six
  writable froxel volume kinds plus TRANSFER_DST transitions for both noise
  volumes, in one batched `cmd_pipeline_barrier`; clears every writable
  volume to its documented neutral sentinel (`(0,0,0,1)` for
  lighting/integrated, `(0,0,0,0)` for the history/transport sidecars)
  before any shader ever reads them (guards the #1082 black-frame
  regression); noise images get a TRANSFER_DST→SHADER_READ_ONLY barrier
  after their buffer-to-image copies. Staging buffers are destroyed only
  after `with_one_time_commands` returns, which the surrounding comment
  correctly notes already waited the submission to completion.
- **`dispatch`**: per-frame UBO/SSBO writes precede a HOST→COMPUTE memory
  barrier (required even for HOST_COHERENT memory, per spec, for the
  execution dependency — correctly present); the pre-injection barrier
  batch sequences all six double-buffered volume kinds' prior-frame
  READ→this-frame WRITE and prior-slot WRITE→READ in one call; the
  injection→integration barrier is explicitly widened to also cover
  `TRANSFER_WRITE` in its source stage/access, with an inline comment
  explaining why (`record_neutral_frame`'s clear can be the integrated
  volume's most recent prior use, e.g. at scene load before any TLAS
  exists); the post-integration barrier makes the integrated volume's
  write visible to the composite fragment shader's sampler read; a final
  COMPUTE→HOST barrier publishes the combustion-light SSBO's atomic writes
  ahead of the next frame's mapped readback. Dispatch group counts
  (`inj_groups_*`/`int_groups_*`) are derived from `self.extent` — the
  same struct field the images were created with, not a separately-tracked
  copy — so there is no drift vector between "how big the image is" and
  "how many workgroups get dispatched."
  Per-frame `debug_assert!`s gate `dispatch` on `write_tlas`/
  `write_lights_and_clusters`/`write_boundary_geometry` having run first
  for that frame slot, each resetting its own latch so a skipped call the
  *next* frame re-trips the assert — verified in
  `crates/renderer/src/vulkan/context/post_passes.rs:567-736`: all three
  writers and the `dispatch` call live inside the same
  `if let (Some(tlas), Some(...), Some(...)) = (...)` branch, so every
  code path that reaches `dispatch` has just run all three writers first;
  the `false` branch of that same `if let` (TLAS/cluster/geometry inputs
  not yet ready) skips both the writers and `dispatch` together, so the
  latch is never checked on a frame where it wasn't also just set.
- **`record_neutral_frame`**: correctly scoped to the integrated volume
  only (the one thing composite samples); its own barrier's source access
  mask includes `TRANSFER_WRITE` for the repeat-neutral-frame case (#3647,
  same rationale as the `pre_int_write` barrier in `dispatch`).

## Vulkan spec / binding-drift coverage

- Both compute pipelines (injection, integration) call
  `validate_set_layout` against a `ReflectedShader` built from their own
  committed `.spv` at construction time (lines ~1400, ~1620) — this is a
  runtime check (requires a real device), not itself a `cargo test`, but
  `crates/renderer/src/vulkan/reflect.rs` separately pins
  `volumetrics_ubo_sizes_match_host_structs_in_every_shader` (UBO size
  lockstep for both `VolumetricsParams` and `IntegrationParams` against
  both shaders) and includes both `.spv`s in
  `every_committed_spv_is_spirv_1_0` — re-ran both, pass.
- `VOLUMETRIC_OUTPUT_CONSUMED` (line 581, `= true`) is read correctly (not
  assumed) by `context/post_passes.rs:514`, gating both the descriptor
  writes and the dispatch; the `record_neutral_frame` skip-clear latch
  (`skip_clear_decision`, shared shape with the caustic pass) is
  unchanged and still test-covered
  (`record_volumetrics_pass_routes_skip_clears_through_the_shared_latch`,
  re-ran, pass).
- Far-plane triple-copy lockstep (#3611, this delta's own volumetrics
  change): `VolumetricsConfig::DEFAULT` is now the single source
  `DEFAULT_GRID_FAR_METERS` derives from; `VOLUME_FAR`
  (`shader_constants_data.rs`, `build.rs`-included, can't derive from this
  crate's types) stays a pinned literal via
  `volume_far_shader_constant_agrees_with_the_volumetrics_config_default` —
  re-ran, pass.
- `froxel_xy_divisor` is range-validated (`2..=32`,
  `VolumetricsConfig::validate()`, called at `VolumetricsPipeline::new`
  entry) before any `div_ceil` uses it — no divide-by-zero vector.
  `COMBUSTION_LIGHT_GRID_COUNT` (256) is a compile-time-derived constant
  with a pinning `assert!`, and every buffer sized from it uses
  `size_of::<[T; N]>()`, not a runtime multiply — no integer-overflow
  vector on the froxel-grid's buffer sizing.

**Conclusion: no new safety finding in `volumetrics.rs` or the froxel-grid
buffers/images it manages.** The file's own regression-guard density (barrier
comments citing issue numbers for nearly every non-obvious ordering
decision) is unusually high even by this codebase's standard, and the two
real bugs this audit's tracing turned up in the *adjacent* water/caustic
code (#3601, #3820) were both already found and fixed — by the project's own
audit process — before this run started.

---

# Per-Dimension Detail

## Dimension 1 — FFI Lifetime Safety — PASS, no findings
No file in scope (`crates/fsr3-sys`, `crates/cxx-bridge`, `crates/ui/src/player.rs`)
appears in the 15-commit delta. Re-confirmed via direct read: `fsr3-sys` still
exactly two `unsafe fn` with `# Safety` docs; cxx-bridge still the one
pointer-free `native_hello()` fn; `crates/ui/src/player.rs` still zero
`unsafe` blocks. Carries forward from 2026-09-04.

## Dimension 2 — Memory Corruption / UB — PASS, no findings
`crates/core` (ECS cached-pointer contract), `crates/nif` (POD reads),
`crates/sfmaterial`, `crates/pex` (opcode transmute), `crates/bsa/src/safety.rs`,
and the LZ4 `safe-decode` pin are all untouched by the delta — carried
forward. The delta's renderer-side changes (`caustic.rs`, `water.rs`,
`volumetrics.rs`) were reviewed directly above and in the delta table; both
real issues found (#3601, #3820) were already fixed before this audit ran.

## Dimension 3 — Memory & Resource Leaks — PASS, no findings
Rapier release (#1520), deferred-destroy drain (#418/#732), and
`AllocatorResource` drop ordering (#1406) are untouched by the delta —
re-ran the Rapier guard tests directly (9/9 pass, folded into the 29/29
run above). `VolumetricsPipeline::destroy`'s allocation/handle inventory
was traced field-by-field above (no leak on any construction or
resize-recreate failure path).

## Dimension 4 — Unsafe-Block Discipline — PASS, no findings
736 real `unsafe` sites workspace-wide (measurement-method variance from
738 on 2026-09-04, not a regression — see Method notes). Every
`volumetrics.rs` site individually re-verified against its SAFETY comment
(see deep-dive above). No high-risk pattern (`align_to`, `MaybeUninit`,
`get_unchecked`, `Box::from_raw/into_raw`, `static mut`,
`write_unaligned`) appears anywhere in the delta's changed files.

## Dimension 5 — Vulkan Spec Compliance — PASS, no findings
Two real, already-fixed issues surfaced by tracing the delta: #3601 (SSBO
index overflow feeding a bindless texture index — CRITICAL class) and
#3820 (image-atomic write past an accumulator's actual bound extent when
resize rebinds it to a 1×1 placeholder — HIGH class, a genuine Vulkan-spec
nuance since the out-of-range-write discard guarantee does not cover
atomics). Both are already fixed and regression-tested; neither is new.
Verified `volumetrics.rs`'s own accumulator (the combustion-light-moment
SSBO) does not share either vulnerability (see deep-dive). TLAS resize
wait (#1390), queue submission ordering, and the SPIR-V reflection
binding-drift tests are all untouched by the delta and re-confirmed via
the targeted test runs above.

## Dimension 6 — R1 Material Table Layout Soundness — PASS, no findings
`crates/renderer/src/vulkan/material.rs` untouched by the delta. Re-ran
`gpu_material`/`gpu_instance`/`gpu_camera`/`gpu_light` test groups (56/56
pass, see Method notes) — `GpuMaterial` still 432 B with matching offset
pins.

## Dimension 7 — RT IOR-Refraction Safety — PASS, no findings
`triangle.frag` and `shader_constants_data.rs`'s glass/refraction constants
untouched by the delta. `water.frag`'s changes (#3820/#3822/#3826) are
coverage/blend-math and the already-covered caustic-atomic-bound fix, not
the passthrough-loop guard. Re-ran glass/refract/dbg-bit tests (folded into
the 56/56 run above) — pass.

## Dimension 8 — NPC/Animation Spawn Safety — PASS, no findings
No file in scope for this dimension appears in the delta except
`#3569`'s `bind_inverses` requeue fix, which is itself a named regression
guard the skill already documents as fixed — re-confirmed via the
`skin_dispatch_ran_rollback_scope_tests` run above (pass). B-spline
sentinel, `AnimationClipRegistry` dedup, and `MAX_TOTAL_BONES` overflow
guard untouched.

## Dimension 9 — NIFAL NaN/Inf Boundary — PASS, no findings
`byroredux/src/material_translate.rs`, the collision-shape finiteness
gates, and `extract_emitter_params`/`extract_emitter_rate` are all
untouched by the delta. `byroredux/src/systems/particle.rs` changed
(#3589) but only to route an existing bit-flag pack through the canonical
overlay boundary — no finiteness-relevant field is touched.

## Dimension 10 — debug-ui Teardown & Shared-Allocator Safety — PASS, no findings
`crates/debug-ui`, `crates/renderer/src/vulkan/egui_pass.rs` untouched by
the delta. Carries forward.

## Dimension 11 — Sandboxed Mod Runtime Trust Boundary — 0 new findings; 1 existing
`crates/mod-runtime` untouched by the delta; 66/66 tests re-run, pass.
`byroredux/src/extensions.rs` is also untouched (still 10,652 LOC, last
modified 2026-09-03) and `.claude/commands/_audit-common.md` still carries
no layout-map row for it — **DOC-ROT-1 / #3828 remains open and
unchanged**, carried forward as Existing rather than re-filed.

---

## Regression-guard confirmation table (this run vs. 2026-09-04 baseline)

| Guard | 2026-09-04 | This run |
|---|---|---|
| fsr3-sys `# Safety` docs / cxx-bridge placeholder / Ruffle boundary | PASS | **PASS** (untouched, carried forward) |
| ECS cached-pointer contract / NIF POD reads / sfmaterial / pex opcode | PASS | **PASS** (untouched, carried forward) |
| bsa/safety.rs bounds / LZ4 safe-decode pin / #3391 byte-slicing | PASS | **PASS** (untouched, carried forward) |
| Rapier release on cell unload (#1520) | PASS | **PASS** (9/9 tests) |
| Deferred-destroy drain / AllocatorResource ordering | PASS | **PASS** (untouched, carried forward) |
| Unsafe-block SAFETY-comment coverage | 738/738 | **736/736** (recount variance, not a regression; volumetrics.rs individually re-verified 100%) |
| TLAS resize wait / queue submission ordering | PASS | **PASS** (untouched, carried forward) |
| `GpuMaterial` 432 B + offset pins | PASS | **PASS** (re-ran, pass) |
| `MAX_REFRACT_PASSTHRUS` / glass identity check | PASS | **PASS** (re-ran, pass) |
| B-spline FLT_MAX sentinel / `MAX_TOTAL_BONES` overflow guard | PASS | **PASS** (re-ran, pass) |
| NaN-seed / `resolve_pbr` boundary | PASS | **PASS** (untouched, carried forward) |
| debug-ui / EguiPass teardown ordering | PASS | **PASS** (untouched, carried forward) |
| mod-runtime: zero unsafe, no WASI, capability gating | PASS | **PASS** (66/66 tests) |
| mod-runtime "no consumer" premise | STALE — see DOC-ROT-1 | **Still stale — #3828 open, unchanged** |
| Volumetrics far-plane triple-copy lockstep (#3611) | N/A (fixed same day as last audit's cutoff) | **PASS** (new since 09-04, test re-ran) |
| UI instance index vs. `MAX_INSTANCES` (#3601) | N/A (not yet fixed) | **PASS — fixed since 09-04, regression test re-verified** |
| Water/glass caustic image-atomic bound (#3820) | N/A (not yet fixed) | **PASS — fixed since 09-04, regression tests re-verified** |
| Volumetrics froxel-grid: image/buffer lifecycle, barriers, atomic-index bounds | Not separately itemized | **PASS — new deep-dive this run, see above** |

---

## Next step

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-09-05.md
```
