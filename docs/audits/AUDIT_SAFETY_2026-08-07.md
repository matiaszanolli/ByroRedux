# Safety Audit — 2026-08-07

Scope: `unsafe` blocks, FFI boundaries, memory-lifecycle code across the
ByroRedux workspace, per `.claude/commands/audit-safety/SKILL.md` (10
dimensions) and the shared protocol in `_audit-common.md` / `_audit-severity.md`.
HEAD at audit time: `79bfc76e`.

Method: this is the 22nd `/audit-safety` pass (prior reports run from
2026-04-05 through 2026-08-03, 4 days ago). The ten dimensions were split
across six parallel workers (D1+D2, D3, D4, D5, D6+D7, D8+D9+D10), each
re-verifying the prior pass's regression guards against current HEAD and
giving focused attention to code that changed in the intervening window
(collision-authoring-summary `8ee151e0`, packed-collision-compat `716b7ee9`,
day-night cycle `7a851ab9`, quest lifecycle/observability `0775df28`/`a844c26b`,
plus the brand-new `crates/mod-runtime` sandboxed executable-mod host, which
had **zero** prior safety-audit coverage per the repo's "Un-owned subsystems"
table and got a first-ever pass this run). Every finding below was
independently re-derived from source (file:line citations), not assumed from
"no diff since last time," and every worker ran the mandatory dedup check
against `/tmp/audit/issues.json` (73 open issues, fetched fresh this session).

**Result: one HIGH, one MEDIUM, four LOW.** The HIGH is a genuine new defect
(not a regression) introduced by this window's packed-collision-compatibility
feature. Two of the four LOW findings are same-content regression checks
against already-open issues (#2273, #2274); the other two are new
documentation/hygiene gaps. Every other dimension's guard set — FFI lifetime
contracts, the ECS cached-pointer contract, `#[repr(C)]` GPU-struct layout,
the three drop-ordering regression guards, TLAS/BLAS build discipline,
glass/IOR guards, NPC-spawn safety, NIFAL's NaN boundary, debug-ui teardown —
was re-verified intact.

---

## Findings

### SAFE-2026-08-07-01: `synthesize_packed_havok_proxy` can build an unbounded/infinite collider from unclamped REFR scale; the only guard is a `debug_assert!` compiled out of release builds
- **Severity**: HIGH
- **Dimension**: 9 (NIFAL Boundary — NaN/Inf / unbounded values reaching a live subsystem)
- **Location**: `byroredux/src/cell_loader/spawn.rs:90-193` (`transformed_mesh_aabb`, `synthesize_packed_havok_proxy`, `spawn_packed_havok_proxy`); consumed at `crates/physics/src/convert.rs:117-133` (`flatten_to_parts`, `CollisionShape::Cuboid` arm)
- **Status**: NEW
- **Description**: `716b7ee9` ("improve packed collision compatibility") added a
  proxy path: when a placement authors FO4+/FO76/Starfield packed collision
  (`BhkNPCollisionObject`, opaque `BhkSystemBinary`) with no decoded collision
  shape, on a `Clutter`/`Actor`-layer object, the cell loader synthesizes a
  conservative `CollisionShape::Cuboid` from the render-mesh AABB:
  ```rust
  // spawn.rs:150
  let half_extents = ((max - min) * 0.5 * ref_scale.abs()).max(Vec3::splat(0.5));
  ```
  `ref_scale` is the placement's REFR `XSCL` scale, read as a raw,
  **unclamped** `f32` off disk (`crates/plugin/src/esm/cell/walkers.rs:680-682`:
  `scale = r.f32().unwrap_or(1.0)`), threaded through unmodified as
  `outer_scale` (`byroredux/src/cell_loader/references/mod.rs:397`) into
  `ref_scale` here. `synthesize_packed_havok_proxy` checks only
  `ref_scale.is_finite()` (`spawn.rs:127`) — it does not bound the
  *magnitude* of `ref_scale`, and does not re-check the *computed*
  `half_extents` after the multiply. A large-but-finite `ref_scale` produces
  an unbounded `half_extents`; an `f32` overflow produces a literal `Infinity`
  that still passes `is_finite()` upstream of the multiply (the check runs on
  the input, not the product) and is inserted directly into the ECS as
  `CollisionShape::Cuboid { half_extents }` with no further validation
  (`spawn.rs:168-172`).
  This breaks the idiom used at every other Cuboid-construction site in the
  codebase — e.g. `BhkBoxShape` (`crates/nif/src/import/collision/shape.rs:139-150`)
  wraps its computed half-extents in `finite_vec(half_extents)?` before
  returning. `synthesize_packed_havok_proxy` is the one new call site in this
  diff range that skips that pattern.
  The only remaining backstop is inside the physics shape flattener:
  ```rust
  // crates/physics/src/convert.rs:117-125
  CollisionShape::Cuboid { half_extents } => {
      debug_assert!(
          half_extents.is_finite()
              && half_extents.x >= 0.0 && half_extents.y >= 0.0 && half_extents.z >= 0.0,
          "canonical Cuboid half-extents must be finite non-negative magnitudes, got {half_extents:?}"
      );
      out.push((parent_iso, SharedShape::cuboid(
          half_extents.x.max(1e-3), half_extents.y.max(1e-3), half_extents.z.max(1e-3),
      )));
  }
  ```
  `debug_assert!` is compiled out of `cargo build --release` (this project's
  documented release build). In release, `Infinity.max(1e-3) == Infinity`, so
  `SharedShape::cuboid(Infinity, Infinity, Infinity)` (or an astronomically
  large finite equivalent) is handed to Rapier3D unfiltered.
- **Evidence**: `spawn.rs:121-153` (no post-multiply finite/bounds check),
  `walkers.rs:680-682` (raw unclamped `XSCL` read), `references/mod.rs:397`
  (unmodified passthrough), `convert.rs:117-133` (debug-only guard).
- **Impact**: A malformed or crafted ESM plugin with an extreme `XSCL` on a
  `Clutter`/`Actor` REFR referencing an FO4+/FO76/Starfield mesh with opaque
  packed Havok collision and no other decoded collider triggers this path. In
  a release build the resulting collider has effectively-infinite (or merely
  astronomically large) half-extents, spawned as a live kinematic body parented
  into the world. Rapier3D's broad-phase AABB tree then treats this collider as
  overlapping essentially everything in the scene, corrupting collision
  queries/contact generation engine-wide for the running session — not just
  for the one bad placement. Real, reachable, engine-wide physics-integrity
  regression from a genuinely new feature, gated only by a build-profile-
  dependent assert.
- **Related**: Introduced by `716b7ee9`. No related open issue found in
  `/tmp/audit/issues.json` (closest, #2302, is an unrelated
  `NiTriStripsData.normals` cross-check finding). Falls into the Physics/
  PHYSAL coverage gap noted in `_audit-common.md`'s "Un-owned subsystems"
  table — nothing else in the audit rotation checks Rapier-bound shape
  parameter bounds outside the NIF-import boundary itself.
- **Suggested Fix**: In `synthesize_packed_havok_proxy`, replace the bare
  `.max(Vec3::splat(0.5))` with a `finite_vec(half_extents)?`-style check
  (return `None` on non-finite, matching every other shape-construction site),
  and clamp the upper bound to a sane ceiling (e.g. a multiple of the cell's
  expected extent, or whatever ceiling the `Architecture` trimesh fallback
  already assumes) so a corrupt-but-finite `ref_scale` can't produce a
  degenerate collider. Promoting the `convert.rs` `debug_assert!` to a real
  runtime clamp (matching the `Ball`/`Capsule`/`Cylinder` arms' `.max(1e-3)`
  pattern, plus an upper bound) would additionally close this class of gap for
  any future unguarded `CollisionShape::Cuboid` producer, not just this one
  call site.

### SAFE-2026-08-07-02: `fsr3-sys`'s Vulkan smoke example — 20 of 23 unsafe blocks/fns carry no SAFETY comment
- **Severity**: MEDIUM
- **Dimension**: 4 (Unsafe-Block Discipline)
- **Location**: `crates/fsr3-sys/examples/vulkan_context_smoke.rs:52,63,64,86,101,107,110,112,117,120,124,134-135,139,177,198,200,215,221` (also the two `unsafe fn` declarations at `:50` `run()` and `:116` `create_and_destroy_context()`, neither carrying a `# Safety` doc block)
- **Status**: NEW (pre-existing since the file's introduction on 2026-07-22, `34e26ca8`; not touched since 2026-08-03 — a scope gap in the prior audit, not a regression)
- **Description**: The one example binary in the workspace doing raw `ash`
  FFI (`cargo run -p byroredux-fsr3-sys --example vulkan_context_smoke`) sits
  outside every `src/` tree the 2026-08-03 audit explicitly scanned. It has 23
  `unsafe` occurrences and only 3 `SAFETY` comments (`validation_callback`'s
  `CStr::from_ptr` at `:24`, a blanket comment on `main()`'s call into `run()`
  at `:38-39`, and one on `ash::Entry::load()` at `:51`). Everything else —
  instance/device creation, debug-messenger create/destroy,
  physical-device/queue-family enumeration, extension enumeration,
  `CStr::from_ptr` for extension-name compare, `get_physical_device_features2`,
  `Context::create`, `device_wait_idle`, `destroy_device` — has no individual
  justification. A "does this unsafe block have a SAFETY comment" convention
  check silently skips this file if scoped to `src/` as the prior audit was.
- **Evidence**: `grep -c unsafe` → 23, `grep -c SAFETY` → 3, in
  `crates/fsr3-sys/examples/vulkan_context_smoke.rs`. No other `examples/`
  file in the workspace contains any `unsafe` at all — isolated gap, not a
  systemic `examples/` problem.
- **Impact**: Low blast radius — opt-in smoke-test binary, not linked into the
  engine or any `cargo test` run. Manual inspection shows the sequence is
  actually correct (device → context → `device_wait_idle` → context dropped →
  device destroyed → debug messenger destroyed → instance destroyed, proper
  reverse order, `?`-propagated errors throughout). The gap is
  discipline/documentation: a future editor extending this file has no
  per-call precondition text to check their change against.
- **Related**: None — no open issue references this file.
- **Suggested Fix**: Add per-call `// SAFETY:` comments matching house style,
  or at minimum widen `run()`'s existing blanket comment at `:38-39` to
  explicitly cover every raw call inside it and note that
  `create_and_destroy_context` inherits the same contract.

### SAFE-2026-08-07-03: `audit-safety` SKILL's Dimension-3 leak-inventory text still stale (regression check)
- **Severity**: LOW
- **Dimension**: 3 (Memory & Resource Leaks) / meta
- **Location**: `.claude/commands/audit-safety/SKILL.md:115`, `:136`
- **Status**: Existing: #2274 (OPEN) — regression-checked, still unfixed
- **Description**: The 2026-08-03 pass (`SAFE-2026-08-03-04`) flagged two
  stale claims in this skill's Dimension-3 bullets and filed #2274. Both are
  still present verbatim at current HEAD, 4 days later:
  1. Line 115 still says `DeferredDestroyQueue<T>` is "shared by mesh + BLAS +
     BLAS-scratch buffer (#1782) + texture + skin compute" — a fresh grep
     (`grep -rn "DeferredDestroyQueue<" crates/renderer/src/`) again finds
     exactly three instantiation sites (`mesh.rs:188`,
     `vulkan/acceleration/mod.rs:158,175`), none for texture or skin-compute.
  2. Line 136 still frames "The MaterialTable dedup map … [is a] known
     per-cell-growth risk" — `byroredux/src/render/mod.rs:559` still calls
     `material_table.clear()` unconditionally at the top of every
     `build_render_data` frame, so the map cannot grow across cells or the
     session.
- **Evidence**: `.claude/commands/audit-safety/SKILL.md:114-137` unchanged
  (`git diff --stat HEAD -- .claude/commands/audit-safety/` empty).
- **Impact**: None on running code — doc/skill-text drift that risks a future
  audit chasing a non-existent leak or over-trusting an unverified
  texture/skin-compute deferred-destroy claim, exactly as #2274 describes.
- **Related**: #2274 (open 4 days with a same-file text-edit fix available —
  no blocker).
- **Suggested Fix**: Unchanged from #2274 — drop the MaterialTable
  growth-risk framing at line 136 and narrow line 115's claim to the three
  confirmed `DeferredDestroyQueue<T>` users.

### SAFE-2026-08-07-04: BLAS-scratch-shrink call site lost its explicit `SAFETY:` tag during the unload-batching refactor
- **Severity**: LOW
- **Dimension**: 4 (Unsafe-Block Discipline)
- **Location**: `byroredux/src/cell_loader/unload.rs:265-282` (`finish_unload_batch`, unsafe block at `:278-280`)
- **Status**: NEW (introduced by this window's batch-despawn/unload refactor; `git diff 7e4db743..HEAD` shows the block was moved out of `unload_cell` into the new `finish_unload_batch` helper shared by `unload_cell`/`unload_cells`)
- **Description**: Before this window, the call site carried an explicit
  `// SAFETY: ...` comment restating both of
  `shrink_blas_scratch_to_fit`'s `# Safety` preconditions. The refactor that
  split this call into `finish_unload_batch` (run once per logical unload
  boundary rather than once per cell) replaced that comment with a shorter
  one that preserves the *substance* ("Retiring the old scratch allocation is
  deferred for frames-in-flight by AccelerationManager (#1782), so this is
  safe from the about_to_wait streaming path") but drops the literal
  `SAFETY:` tag and no longer restates the same-device/allocator precondition
  in words (trivially true here — both come from the same `&mut VulkanContext`
  — but no longer stated).
- **Evidence**: `crates/renderer/src/vulkan/acceleration/memory.rs:32-49`
  carries the callee's unchanged, correct `# Safety` doc block. All five call
  sites of `unload_cell`/`unload_cells` were checked
  (`streaming_helpers.rs:204,207`, `app_step.rs:100`, `main.rs:1084`,
  `cell_loader/exterior.rs:431`, `cell_loader/transition.rs:196`) — none
  require the "no BLAS build in flight" precondition directly, since the
  callee's own deferred-destroy contract makes that a non-issue regardless of
  caller frame-loop phase. The invariant still holds.
- **Impact**: None on running code — hygiene/documentation regression only.
  Risk is to future maintainability: a `grep SAFETY` convention audit could
  mis-flag this site as "missing" even though the reasoning is present in
  different words.
- **Related**: #495, #1782/CONC-D1-01, #2148/ECS-2507-02 (all correctly
  referenced by the surrounding comments).
- **Suggested Fix**: Re-prefix the second paragraph with `// SAFETY:` and
  restate the same-device/allocator precondition in one clause. One-line fix,
  no behavior change.

### SAFE-2026-08-07-05: Stale field-count in `MaterialTable::intern_by_hash`'s collision-policy comment (regression check)
- **Severity**: LOW
- **Dimension**: 6 (R1 Material Table Layout Soundness)
- **Location**: `crates/renderer/src/vulkan/material.rs:1143` (doc comment above `intern_by_hash` at `:1145`)
- **Status**: Existing: #2273 (filed as `SAFE-2026-08-03-03`) — regression-checked, still open
- **Description**: The doc comment reads "rare on FxHash's 64-bit output over
  75 scalar fields, #1368" but `GpuMaterial` has carried 87 fields (348 B)
  since the 2026-07-27 growth. The size/offset pins themselves
  (`gpu_material_size_is_348_bytes`, `gpu_material_field_offsets_match_shader_contract`)
  are correct and current — only this prose comment is stale. `git blame`
  confirms no touch to this comment in the intervening 4 days.
- **Evidence**: `material.rs:1143` vs. the 87-field struct at `material.rs:74-292`.
- **Impact**: Cosmetic; no functional effect. Carried open as #2273.
- **Suggested Fix**: Unchanged from the original — update the comment to 87
  fields, or drop the specific count so it can't go stale again.

### SAFE-2026-08-07-06: `audit-safety` SKILL's Dimension-7 text misdescribes the #789 glass-passthrough guard as texture-equality; it's now `materialKind == MATERIAL_KIND_GLASS`
- **Severity**: LOW
- **Dimension**: 7 (RT IOR-Refraction Safety) / meta (skill doc-rot)
- **Location**: `.claude/commands/audit-safety/SKILL.md:225-227`
- **Status**: NEW (no open issue covers Dimension 7 of this skill; #2274 covers only Dimension 3)
- **Description**: The skill's Dimension-7 checklist states the passthrough
  guard is "the texture-equality identity check." That describes #789's
  *original* 2026-05 fix (`b38d16bc`). The mechanism was replaced on
  2026-07-19 (`a09d2b76`, "Enhance alpha blending logic for glass materials"):
  the check is now keyed on
  `materials[hInst.materialId].materialKind == MATERIAL_KIND_GLASS`, not
  texture-index equality — texture-equality misfired whenever glass shared a
  texture with opaque geometry, letting the refraction ray skip through solid
  walls. This exact staleness was already caught once in a sibling report
  (`docs/audits/AUDIT_RENDERER_2026-06-09.md`), but `audit-safety`'s own
  Dimension-7 prose was never updated, so every subsequent `/audit-safety` run
  re-inherits the wrong mechanism description.
- **Evidence**: Skill text at `SKILL.md:225-227`; current mechanism at
  `crates/renderer/shaders/triangle.frag:1710-1711`; mechanism-change commit
  `a09d2b76`; prior catch at `docs/audits/AUDIT_RENDERER_2026-06-09.md:36`.
- **Impact**: Documentation-only. The safety property (no unbounded
  recursion) does not depend on which identity check is used — it is
  structurally bounded by `REFRACT_PASSTHRU_BUDGET = 2`
  (`triangle.frag:1659,1680`, a fixed loop-iteration cap independent of the
  identity check). Risk is to future auditors/engineers who trust the skill
  text and either go looking for a check that no longer exists, or "fix" the
  current `materialKind` check back toward texture-equality believing it's a
  regression.
- **Related**: #789 (original bug), `docs/audits/AUDIT_RENDERER_2026-06-09.md`
  Dim 9 (independently caught the same drift), #2274 (sibling doc-rot issue,
  same root-cause pattern, different dimension).
- **Suggested Fix**: Update the skill's Dimension-7 bullet to read "keyed on
  `materialKind == MATERIAL_KIND_GLASS`," and note the loop's actual
  unbounded-recursion guard is the fixed `REFRACT_PASSTHRU_BUDGET = 2` cap,
  independent of which identity check gates continuation.

---

## Regression Guards Re-Verified Intact

- **Dimension 1 — cxx-bridge still a placeholder.** `crates/cxx-bridge/src/lib.rs`
  unchanged since 2026-08-03: one `native_hello() -> String` bridge fn, no
  raw pointers, no borrowed-string handoff, no `Box<>` ownership transfer.
- **Dimension 1 — fsr3-sys `# Safety` contracts.** `Context::create`
  (`lib.rs:342`) and `Context::dispatch` (`lib.rs:371`) still carry `# Safety`
  docs; the sole production call site (`frame_upscaler.rs:116-123`) satisfies
  both the device/proc-addr-outlives-context and same-device-handle contracts;
  `Drop for Context` fires only from inside `VulkanContext::drop`, which calls
  `device_wait_idle()` first.
- **Dimension 2 — ECS cached-pointer contract (#35/#1367).** `World::get`
  returns `ComponentRef<'_, T>` (guard-carrying); field layout and SAFETY
  comments in `query.rs` unchanged. The one file that did change
  (`world.rs`, adding `despawn_batch` in `ede92928`) uses `lock.get_mut()`
  directly and never touches `ComponentRef`/`QueryRead`/`QueryWrite` internals.
- **Dimension 2 — `#[repr(C)]` GPU-struct vec3 soundness.** Zero
  `[f32; 3]` fields in `gpu_types.rs`; every vec3-shaped GPU quantity is
  `[f32; 4]` or three separate scalar `f32`s.
- **Dimension 2 — NIF bulk POD reads.** `read_pod_vec` and its header mirror
  both retain `checked_mul` overflow guards and the sealed `AnyBitPattern`
  bound.
- **Dimension 2 — sfmaterial/pex decode.** `BuiltinType::from_u32` remains a
  checked `match` with an `Err` arm (no transmute). `OpCode::from_u8` remains
  a sound transmute: `#[repr(u8)]`, 51 contiguous discriminants (`0..=50`,
  `MAX_OPCODE=51`), range-checked before the transmute — no new opcode was
  added in the audit window.
- **Dimension 2 — recursion guards, including the two flagged collision
  commits.** `MAX_NIF_NODE_DEPTH=128` and `MAX_COLLISION_SHAPE_DEPTH=64` +
  cycle-detection both intact. `8ee151e0`'s `CollisionAuthoringSummary`
  addition is a flat, non-recursive block-classification loop; `716b7ee9`'s
  `ragdoll.rs` change adds only an early-exit gate and log diagnostics. Neither
  commit introduces recursion or unsafe code.
- **Dimension 3 — Rapier release on cell unload (#1520/#1531), re-verified
  through a real refactor.** This window's exterior-streaming batch-unload
  split (`3205506d`/`ede92928`/`30d421cd`) moved BLAS-scratch and
  sparse-storage shrink into a shared `finish_unload_batch` tail, but
  `release_victim_rapier_bodies` stays inside `unload_cell_inner`, called once
  per cell root from both the single-cell and new batch entry points — no
  bypass path. The new `716b7ee9` packed-Havok-proxy spawn path
  (`spawn_packed_havok_proxy`) reuses the same `RigidBodyData`-component +
  `physics_sync_system` pattern the release guard already keys on generically,
  and the ghost entity is captured by the same `CellRootIndex` range-stamping
  as every other cell entity — no new leak surface (independent of the
  SAFE-2026-08-07-01 shape-bounds defect above, which is a corruption risk,
  not a leak).
- **Dimension 3 — deferred-destroy tick/drain ordering.** Tick still runs
  after `wait_for_fences` (`context/draw.rs:1273-1281`/`1365-1401`); shutdown
  drains remain unconditional and gated on caller-side `device_wait_idle`.
- **Dimension 3 — `AllocatorResource` drop ordering (#1406), including
  panic-unwind.** `impl Drop for App` (`main.rs:278-304`) still removes
  `AllocatorResource` before `self.renderer.take()`; the two commits that
  touched `main.rs` this window are confined to unrelated bench/telemetry
  fields.
- **Dimension 3 — new batch-release APIs preserve refcount semantics.**
  `MeshRegistry::drop_meshes`/`TextureRegistry::drop_textures` still call the
  single-handle release once per input slice entry (duplicates included);
  only the cache-purge pass is batched. `World::despawn_batch` does a single
  sorted merge-pass compaction (packed storage) / per-entity `remove()` loop
  (sparse storage) — correct 1:1 removal either way.
- **Dimension 3 — `crates/mod-runtime` first-ever pass: clean.** Zero
  `unsafe` blocks. Memory/fuel/stack/table/instance/log-volume ceilings are
  all enforced *during* instantiation (not just declared), each with a
  positive test (`memory_ceiling_is_enforced_during_instantiation`,
  `fuel_exhaustion_quarantines_runaway_guest`, `component_byte_limit_is_checked_before_compilation`,
  `log_size_limit_is_enforced_at_the_host_boundary`). Per-mod-load resources
  (`ModInstance`'s `Store`, `CompiledMod`'s `Component`) are owned directly
  with no crate-internal registry to leak from — RAII, not call-site
  discipline. The capability boundary is structural: `CapabilitySet::grant` is
  never bound into the WIT linker, `HostState` lives outside guest-addressable
  memory, and `wasi_imports_are_absent_by_default` empirically confirms no
  WASI surface leaks through the shared `Linker`. A quarantined instance
  cannot be re-entered. **Not yet wired into the engine** (no cell loader,
  save system, or command surface calls into it), so current blast radius is
  zero regardless of design quality. One documented, phase-scoped gap (no
  host-wide aggregate budget across concurrently loaded mods) is explicitly
  deferred to Phase 4 of its own requirements doc and not reported as a
  finding.
- **Dimension 5 — `VulkanContext::Drop` ordering.** `device_wait_idle()`
  first, full reverse-creation teardown, device destroyed strictly before
  surface/debug-messenger/instance.
- **Dimension 5 — TLAS resize wait (#1390).** `device.device_wait_idle()`
  still runs before the old allocation is freed on the resize path
  (`tlas.rs:726`, a one-line shift from an unrelated edit, not a removal).
- **Dimension 5 — TLAS UPDATE/BUILD count discipline & skinned BLAS refit.**
  `decide_use_update` forces BUILD on any map/address-sequence mismatch, plus
  a post-decision instance-count guard and a debug-time `debug_assert_eq!`.
  `refit_skinned_blas` validates flags and vertex/index counts against the
  original BUILD before emitting UPDATE, dropping and rebuilding on mismatch.
- **Dimension 5 — SHADER_DEVICE_ADDRESS coverage, ray-query gating,
  `VOLUMETRIC_OUTPUT_CONSUMED`, compute-layout hygiene, clear-before-compute.**
  All confirmed intact by direct source read.
- **Dimension 5 — actually executed, not just read.** `cargo check --workspace`
  passes clean (0 warnings/errors). `cargo test -p byroredux-renderer reflect`
  passes 21/21, including every `scene_descriptor_reflection_tests` and
  `reflect::tests` case.
- **Dimension 6 — `GpuMaterial` size/offset pins, intern cap, upload clamp.**
  All intact and green under `cargo test` (25 + 57 + 6 targeted tests
  passing). No `[f32; 3]` fields anywhere in the 87-field struct.
- **Dimension 7 — glass ray budget, Frisvad basis, DBG-bit catalog.**
  `GLASS_RAY_BUDGET`/`GLASS_RAY_COST` match exactly between Rust and GLSL. The
  #1438 atomicAdd-overshoot nuance is present verbatim and unchanged. Frisvad
  is the sole basis-construction path at every lighting/refraction call site
  — no naive `cross(N, world-up)` code remains. All 22 `DBG_*` bits are
  distinct; `dbg_bits_catalog_covers_every_dbg_constant` passes.
- **Dimension 8 — B-spline sentinel, `AnimationClipRegistry` interning,
  `SkinSlotPool` overflow guard.** All three intact, none touched in the audit
  window.
- **Dimension 9 — Material NaN-sentinel boundary.** `resolve_pbr` remains the
  sole `is_nan()`-detecting production call site
  (`material_translate.rs:216`); no new `Material { .. }` producer bypasses
  it. Typed particle-emitter extraction (`extract_emitter_params`/
  `extract_emitter_rate`) still gates on full finiteness + positivity +
  the `FLT_MAX` sentinel reject. `BhkMultiSphereShape`/`BhkConvexListShape`
  (`crates/nif/src/import/collision/shape.rs`) — the two block types the
  audit brief named — were **not** touched by the recent collision commits
  (those landed in `mod.rs`/`ragdoll.rs`, not `shape.rs`) and their
  per-value finite checks are unchanged. The one real new risk from this
  window's collision work is SAFE-2026-08-07-01 above, in a different file
  (`byroredux/src/cell_loader/spawn.rs`) than the brief's named targets.
- **Dimension 10 — egui teardown ordering, deferred texture-free, queue-mutex
  scoping (CONC-D1-01/#1713).** All intact; no commits touched any of
  `egui_pass.rs`, `context/mod.rs`'s Drop, or `crates/debug-ui/src/` in this
  window.

---

## Summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 1 |
| MEDIUM | 1 |
| LOW | 4 |
| **Total** | **6** |

One genuine new HIGH-severity defect this pass: the packed-Havok-compatibility
feature (`716b7ee9`) can synthesize a physics collider with unbounded or
infinite half-extents from unclamped REFR scale data, guarded only by a
`debug_assert!` that release builds compile out (SAFE-2026-08-07-01). One
MEDIUM is a real but low-blast-radius documentation gap in an example binary
outside the prior audit's stated scan scope (SAFE-2026-08-07-02). The four LOW
findings are two regression re-confirmations of already-open issues (#2273,
#2274) and two new skill-doc-rot findings, none of which reflect a live code
defect. Every other guard this audit's ten dimensions exist to protect —
including a first-ever, clean review of the brand-new `crates/mod-runtime`
sandboxed executable-mod host — was re-verified intact against current HEAD.

Next: `/audit-publish docs/audits/AUDIT_SAFETY_2026-08-07.md`
