# Safety Audit — ByroRedux — 2026-09-11

**Command**: `/audit-safety`
**Repo state**: `main` @ `b3db49fa` (2026-09-11 19:53:07 -0300)
**Baseline**: `docs/audits/AUDIT_SAFETY_2026-09-05.md` @ `6fba2b0a` — that run found
zero findings above LOW. 225 commits landed between the two baselines,
including the 15-part `refactor(renderer): migrate X to GpuImage` series, the
`extensions.rs` → `extensions/` eight-module split (#3843), `#3909`'s
`GpuMaterial` field removal, and the CI clippy gate hardening (#4090).
**Severity scale**: `.claude/commands/_audit-severity.md`

## Method

All eleven dimensions were run as independent, parallel deep-dives (one or
two agents per dimension group: Dim 1 alone, Dim 2 alone, Dims 3/5/6/7
together, Dim 4 alone, Dims 8/9/10 together, Dim 11 alone), each reading
current source directly rather than trusting skill prose or the prior
report. The orchestrating pass then independently re-verified every
candidate finding and a wide sample of the PASS confirmations by re-reading
the cited code before accepting them into this report (see "Independent
verification" below each finding and the cross-checks noted per dimension).

Dedup: `gh issue list --repo matiaszanolli/ByroRedux --limit 200 --json
number,title,state,labels` (56 open issues, saved to `/tmp/audit/issues.json`)
was checked against every candidate finding; `docs/audits/` was scanned for
prior coverage. No candidate below matches an open issue title; none is a
regression of a closed one.

Per the project's no-speculative-Vulkan-fixes rule, no render-pass, barrier,
or pipeline-state restructure is proposed anywhere in this report, and no
validation-layer/RenderDoc run was performed (project policy forbids
spawning a parallel/headless engine instance alongside the user's own).

## Findings summary

| Severity | Count |
|---|---|
| CRITICAL | 0 |
| HIGH | 0 |
| MEDIUM | 2 |
| LOW | 1 |
| **Total** | **3** |

| ID | Sev | Dim | Title |
|---|---|---|---|
| SAFE-D6-2026-09-11-01 | MEDIUM | 6 | `GpuMaterial`'s Rust-side docs still say 432 B / cite a dead test name; live struct is 428 B |
| SAFE-D11-01 | MEDIUM | 11 | `queue_increment_own_i64` skips the entity-visibility check every sibling write function enforces |
| SAFE-D11-02 | LOW | 11 | `extensions.rs` was split into 8 files (#3843); `_audit-common.md`'s layout row still describes it as one file |

---

## Findings

### SAFE-D6-2026-09-11-01: `GpuMaterial`'s Rust-side size contract still says 432 B and cites a test name that no longer exists — the live struct is 428 B
- **Severity**: MEDIUM
- **Dimension**: 6 - R1 Material Table Layout Soundness
- **Location**: `crates/renderer/src/vulkan/material.rs:43-49,392,1047,1321,1363,1380`; `crates/renderer/src/vulkan/scene_buffer/constants.rs:177`; `crates/renderer/src/vulkan/scene_buffer/shader_contract_tests.rs:2214`; `byroredux/src/material_translate.rs:104`; `docs/engine/shader-pipeline.md:370,417,493`; `.claude/commands/_audit-common.md:101`; `.claude/commands/audit-safety/SKILL.md:261,269,282`
- **Status**: NEW
- **Description**: `#3909` (commit `06c182fb`, 2026-09-07) removed the unsampled `texture_index` lane from `GpuMaterial`, shrinking it 432 → 428 B. The fix correctly updated the GLSL mirror (`bindings.glsl`), the live test name (`gpu_material_size_is_432_bytes` → `gpu_material_size_is_428_bytes`), and the offset pins. It did **not** update the surrounding Rust-side documentation, which is the authoritative statement of the GPU layout contract elsewhere in the same file and in two audit-skill files. Nine sites still assert 432 B; five of them point readers at `gpu_material_size_is_432_bytes`, a symbol that exists nowhere in the tree. The struct's own last-field comment carries an internally-inconsistent sum (`// offset 424 → total 432`, i.e. 424 + 4 ≠ 432).
- **Evidence**: Live pin, independently re-derived (107 fields × 4 B, no `[f32;3]`, `#[repr(C)]`, no implicit padding):
  ```rust
  // crates/renderer/src/vulkan/material_tests.rs:62-64
  fn gpu_material_size_is_428_bytes() {
      assert_eq!(std::mem::size_of::<GpuMaterial>(), 428);
  }
  ```
  Stale contract in the same crate: `material.rs:43` *"432 bytes per material."*; `material.rs:49` *"Pinned by `gpu_material_size_is_432_bytes`."*; `material.rs:392` `pub back_lighting_map_index: u32, // offset 424 → total 432`. `grep -rn "gpu_material_size_is_432_bytes"` finds 4 production/doc sources; the only test function actually named `gpu_material_size_is_*` in the tree is `_428_bytes`. Confirmed the GLSL mirror (`crates/renderer/shaders/include/bindings.glsl`) has exactly 107 matching scalar fields — **the Rust↔GLSL layout is in lockstep; this is not a runtime layout drift**, only a stale documentation trail.
- **Impact**: No runtime defect today — both pins (Rust size, Rust↔GLSL field-for-field) pass, independently re-verified. The damage is to the contract's trustworthiness: an author extending `GpuMaterial` next reads "432 B, pinned by `gpu_material_size_is_432_bytes`," searches for a test that isn't there, and has no working pointer to the pin that must move in lockstep with `bindings.glsl`. This is the third recorded recurrence of this exact sentence drifting in this one struct (prior: `#3846`/`#4031` in the opposite direction — GLSL stale, Rust correct), and two of the nine stale sites are the audit-skill files themselves, so the audit process that exists to catch this class was primed with the wrong number going into this very run.
- **Related**: `#3909` (the field removal that triggered the drift), `#3846`/`#4031` (the prior, opposite-direction instance of the same defect class).
- **Suggested Fix**: Sweep 432 → 428 and `gpu_material_size_is_432_bytes` → `gpu_material_size_is_428_bytes` across the nine sites (including `_audit-common.md` and `audit-safety/SKILL.md`), and fix `material.rs:392`'s `→ total 432` to `→ total 428`. Longer-term: have `build.rs` emit a `GPU_MATERIAL_SIZE_BYTES` constant into the generated `shader_constants.glsl` and assert the Rust-side pin against that constant instead of a hand-typed literal, so this is the last recurrence rather than the fourth.

---

### SAFE-D11-01: `queue_increment_own_i64` accepts a component write to any well-formed `EntityRef`, bypassing the callback-visibility invariant every sibling write function enforces
- **Severity**: MEDIUM
- **Dimension**: 11 - Sandboxed Mod Runtime Trust Boundary
- **Location**: `crates/mod-runtime/src/runtime/host/state.rs:6-51` (the WIT `state::Host::queue_increment_own_i64` impl); contrast `crates/mod-runtime/src/runtime/host/animation.rs:19-51`, `reputation.rs:19-60`, `actor_values.rs:31-80`, `packages.rs:19-50` (all check `self.entity_projections.get(&entity)`/`contains_key` before queuing); commit path `crates/sdk/src/component.rs` (`ExtensionComponentStore::apply_batch`, `IncrementI64` arm), sole call site `byroredux/src/extensions/commands.rs:335-336`
- **Status**: NEW
- **Description**: Every other guest-reachable command that targets a specific entity (`animation::queue_play_idle`, `actor_values::queue`, `reputation::queue`, `packages::queue_evaluate`) validates that the target entity is present in `self.entity_projections` — the set the host populated for *this specific callback* from the actual event's subject/activator/aggressor, built in `extensions/capture.rs` + `extensions/dispatch.rs::bind_entity` — before it will queue a mutation, bailing with an explicit "target is not visible in this callback" error otherwise. `state::Host::queue_increment_own_i64` skips this check entirely: it validates `accepting_commands`, `COMPONENTS_WRITE_OWN_CAPABILITY`, the per-entry command budget, and the schema/field/type of the write, then goes straight from the raw guest-supplied `EntityRef` to a queued `ExtensionCommand::IncrementI64` command. The commit-side `ExtensionComponentStore::apply_batch` (`crates/sdk/src/component.rs`) does not backfill the gap — it validates schema/field/type/size/row-budget but never checks the entity against any live/visible set, and stores the row keyed by `(principal, schema, entity)` regardless of whether that entity was ever shown to the guest.
- **Evidence** (verified by direct read):
  ```rust
  // crates/mod-runtime/src/runtime/host/state.rs — no entity_projections check anywhere
  fn queue_increment_own_i64(&mut self, entity: state::EntityRef, schema_index: u32,
      field_index: u32, delta: i64) -> wasmtime::Result<()> {
      if !self.accepting_commands { wasmtime::bail!(...); }
      if !self.grants.contains(COMPONENTS_WRITE_OWN_CAPABILITY) { wasmtime::bail!(...); }
      if self.pending_commands.len() >= self.max_commands_per_entry { wasmtime::bail!(...); }
      // ... schema/field lookup + type check ...
      let entity = byroredux_sdk::identity::EntityRef::new(entity.world_generation, entity.object)
          .ok_or_else(|| wasmtime::Error::msg("entity reference contains a reserved zero value"))?;
      self.pending_commands.push(HostCommand::Component(ExtensionCommand::IncrementI64 { entity, .. }));
      Ok(())
  }
  ```
  vs. the sibling pattern (`animation.rs::queue_play_idle`, confirmed present):
  ```rust
  let entity = sdk_entity_ref(entity)?;
  if !self.entity_projections.contains_key(&entity) {
      wasmtime::bail!("animation target is not visible to this callback");
  }
  ```
  No guest-reachable *read* WIT function exists for extension component rows — `state::Host` exposes only `queue_increment_own_i64` (no `get`), and `ExtensionComponentStore::row` is called only from tests and internal save/dispatch accessors, not from any WIT host function.
- **Impact**: A guest can guess or brute-force `EntityRef{world_generation, object}` pairs (small, likely-sequential integers) and stamp component data onto entities it was never introduced to via any callback, undermining the "operate only on what you've been shown" model the rest of the interface enforces. Currently narrow and mostly inert: with no guest-reachable read-back path for these rows today, the observable effect is limited to extra, budget-capped, save-persisted rows with no in-game effect, and no principal-isolation boundary is crossed. The risk is forward-looking — the moment any consumer of `ExtensionComponentStore` reads these rows back (the store's entire purpose), this becomes a live way to tag/track arbitrary world entities without ever being shown them, exactly the leak class `entity_projections` exists elsewhere in this file to prevent.
- **Related**: None found in `/tmp/audit/issues.json` or `docs/audits/`.
- **Suggested Fix**: Add the same `self.entity_projections.contains_key(&entity)` check used by `animation.rs`/`reputation.rs`/`actor_values.rs`/`packages.rs` to `queue_increment_own_i64`, bailing with an explicit "component target is not visible in this callback" error before queuing the command, matching the established sibling pattern exactly.

---

### SAFE-D11-02: `extensions.rs` was split into 8 files today (#3843); `_audit-common.md`'s layout row and `audit-safety/SKILL.md`'s Dimension 11 preamble still describe it as one 10,652-LOC file
- **Severity**: LOW (doc-rot, not a security gap)
- **Dimension**: 11 - Sandboxed Mod Runtime Trust Boundary
- **Location**: `.claude/commands/_audit-common.md:82` (the `extensions.rs (10,652 LOC, 24df5304 ...)` row); `.claude/commands/audit-safety/SKILL.md` Dimension 11 preamble; actual code now at `byroredux/src/extensions/{mod,install,systems,legacy_compat,tests,persist,capture,commands,dispatch}.rs`
- **Status**: NEW
- **Description**: `_audit-common.md`'s layout map — the shared path/line reference every audit skill is told to trust — still names `byroredux/src/extensions.rs` as a single file. Commit `55cbc0d6` ("Fix #3843: split extensions.rs into eight modules," 2026-09-11 12:24, closing #3843) turned it into a 9-file directory (8 production files + `tests.rs`) totaling 10,730 LOC, same `ExtensionHost` struct and behavior — file split only. `_audit-common.md` was itself touched again later the same day (commit `1efc5251`, 18:27, "sweep nine renderer doc-rot findings") without updating this row. This is the same class of gap as the previously-fixed `DOC-ROT-1`/`#3828` (which flagged the file as missing from the map entirely) — now the row exists but names a path that no longer resolves to a single file.
- **Evidence**: `byroredux/src/extensions.rs` — confirmed absent (`ls`: "No such file or directory"). `byroredux/src/extensions/` — confirmed present, 9 files. `git merge-base --is-ancestor 55cbc0d6 1efc5251` → `yes`, confirming the split predates the doc file's last touch without being picked up.
- **Impact**: Documentation-accuracy only; no code defect. No behavior changed in the split.
- **Related**: `#3828` (closed — the prior "missing from the map" state this supersedes), `#3843` (closed — the split itself).
- **Suggested Fix**: Update the `_audit-common.md` layout-map row for `extensions.rs` to point at the `byroredux/src/extensions/` directory and its file list, matching the style already used for other split subsystems in the same table (e.g. `render/`, `cell_loader/`).

---

## Per-Dimension Detail

### Dimension 1 — FFI Lifetime Safety — PASS, no findings
`crates/cxx-bridge` confirmed still the pointer-free `native_hello()` placeholder. `crates/fsr3-sys` (the one real FFI crossing): both `unsafe fn`s (`Context::create`, `Context::dispatch`) carry correct `# Safety` docs; traced live call sites in `frame_upscaler.rs` and teardown ordering in `context/teardown.rs` — FSR context destroyed before the Vulkan device in all paths. `crates/ui/src/player.rs`: zero `unsafe` blocks; the captured pixel-buffer borrow never escapes past the single `update_rgba` call site (enforced by the borrow checker); Ruffle's wgpu device is a disjoint, deliberately-never-torn-down singleton sharing no Vulkan state with `VulkanContext`, so no allocator-before-device hazard exists.

### Dimension 2 — Memory Corruption / UB — PASS, no findings
ECS cached-pointer contract (`ComponentRef`/`QueryRead`/`QueryWrite`), `#[repr(C)]` GPU structs (no `[f32;3]` anywhere), NIF POD reads (`checked_mul` overflow guard, sealed `AnyBitPattern`), `sfmaterial::BuiltinType::from_u32` (checked match, no transmute), `pex::OpCode::from_u8` (real transmute, but range-gated + contiguity-pinned by test), `crates/bsa/src/safety.rs` bounds (all three ceilings enforced, all `Vec::with_capacity`/`read_to_end` sites pre-bounded), the LZ4 `safe-decode` feature pin (workspace `Cargo.toml`, `lz4_flex 0.11.6` locked with the feature explicitly enabled), the #3391-class byte-range slicing fix (all current sites byte-wise or ASCII-delimiter-adjacent), and GRUP-walker/collision-shape recursion depth bounds — all independently re-verified against current source, all intact.

### Dimension 3 — Memory & Resource Leaks — PASS, no findings
Rapier release on cell unload (`unload.rs` releases bodies/colliders/joints before `despawn_batch`, guard test asserts emptiness), deferred-destroy drain (tick runs after fence wait in `sync_and_acquire_frame.rs`, confirmed by direct read of the #418 comment and code; shutdown sweep `flush_pending_destroys` drains all three registries), `AllocatorResource` removed before renderer teardown on both the normal-shutdown and panic-unwind paths (`app_events.rs`, `impl Drop for App` in `main.rs`), and the full GPU allocation inventory (GBuffer/SVGF/TAA/caustic/volumetrics/bloom/composite/SSAO/exposure/upscaler/reservoir/skin-slot/material SSBO) all destroy correctly through the new `GpuImage` consolidation (#3860), including on construction- and resize-failure paths. `MaterialTable::clear()` confirmed called every frame; `AnimationClipRegistry` confirmed still interns lowercased.

### Dimension 4 — Unsafe-Block Discipline — PASS, no findings
682 `unsafe {}` blocks recounted workspace-wide (638 renderer, 29 fsr3-sys, 8 core, 2 plugin, 2 nif, 1 pex, 1 byroredux), plus 81 `unsafe fn` and 39 `unsafe impl`. `crates/renderer/src/lib.rs` carries `#![deny(clippy::undocumented_unsafe_blocks)]` (#1904); `cargo clippy` with that lint confirms zero comment-less blocks in the renderer, and only two adjacency-formatting false positives elsewhere (`crates/nif/src/blocks/bs_geometry.rs`, `byroredux/src/cell_loader/unload.rs`) — both have a correct SAFETY comment a few lines above, already dismissed as false positives in a prior audit, and the CI clippy gate was hardened workspace-wide by `#4090` the same day. Deep invariant-truth spot checks (ECS cached-pointer contract, NIF POD read bounds, `Vertex` byte-cast, `upload_pending_bind_inverses` staging-bound arithmetic, queue-submission mutex comments, `fsr3-sys` contracts) all held.

### Dimension 5 — Vulkan Spec Compliance — PASS, no findings
Create/destroy pairing and Drop ordering (device destroyed last, with the documented #665 leak-guard early-return); per-image semaphore + fence/acquire handling (#952 reset-fences move, #910 recovery path) intact; TLAS UPDATE / skinned-BLAS-refit geometry-count assertions present (`acceleration/tlas.rs:128,339`, `blas_skinned.rs` `debug_assert!`s); TLAS resize `device_wait_idle` before free confirmed at `tlas.rs:1037`; depth-capture `D32_SFLOAT`-only guard confirmed at `depth_capture.rs:148`; `VK_KHR_ray_query` feature enable is correctly gated on `caps.ray_query_supported` (`device.rs:713`); SPIR-V reflection test module (`scene_descriptor_reflection_tests`) confirmed wired in `scene_buffer/mod.rs`; volumetric far-plane triple-copy lockstep (`DEFAULT_GRID_FAR_METERS`/`VOLUME_FAR`) and `GLASS_RAY_BUDGET` Rust↔GLSL lockstep both confirmed by direct read of both sides.

### Dimension 6 — R1 Material Table Layout Soundness — 1 finding (MEDIUM, doc-only)
See SAFE-D6-2026-09-11-01 above. The layout itself is sound and in lockstep (428 B, 107 fields, Rust↔GLSL field-for-field match independently re-verified) — only the Rust-side prose/test-name references are stale. Intern cap (`MAX_MATERIALS = 16384`, one-shot warn, `.min()` clamp on upload) confirmed intact. `ui.vert` declares no `MaterialBuffer` at all (reads `inst.textureIndex` from `GpuInstance` only, pinned by a dedicated test) — the skill's "`ui.vert` MaterialBuffer" bullet is itself stale prose, not a code gap.

### Dimension 7 — RT IOR-Refraction Safety — PASS, no findings
`MAX_REFRACT_PASSTHRUS` is the hard loop bound, now generated (`2 + 2 * MAX_RAY_QUALITY_TIER`) rather than a literal; `refractPassthruBudget` is adaptive 2/4/6/8 by quality tier, not a fixed 2; the passthrough identity check is `materialKind == MATERIAL_KIND_GLASS` at all three shader sites; `GLASS_RAY_BUDGET` (2,097,152) confirmed identical in `shader_constants_data.rs` and the generated `shader_constants.glsl`; Frisvad orthonormal basis confirmed the active path (`include/math_common.glsl`, cited at the IOR call site); `DBG_VIZ_GLASS_PASSTHRU = 0x80` confirmed non-colliding against the full flag catalog (neighbors `0x40`, `0x20000`).

### Dimension 8 — NPC/Animation Spawn Safety — PASS, no findings
B-spline `FLT_MAX` sentinel gate, `AnimationClipRegistry` case-insensitive interning, and the `MAX_TOTAL_BONES` one-shot overflow-warn latch all confirmed intact. The `#3569` bind_inverses-requeue guard is not just intact but further hardened by `#3991`: the rollback check now reads the stricter submit-time flag `!ctx.skin_state_submitted || ctx.bind_inverse_upload_failed` (subsuming the old record-time `skin_dispatch_ran` condition, which could read `true` on a frame whose command buffer was later discarded) — a widening, not a regression, independently confirmed by reading `app_frame.rs:762` and its surrounding rationale comment plus the reset/rollback call sites in `draw.rs`.

### Dimension 9 — NIFAL NaN/Inf Boundary — PASS, no findings
Both production `Material` constructors (`translate_material`, `translate_texture_only_material`) seed `f32::NAN` and unconditionally call `resolve_pbr()` immediately after, independently confirmed by direct read of `material_translate.rs`. `resolve_pbr`'s NaN-replace-then-clamp ordering is correct. `studio_host.rs`'s external `SetMaterial` write path gates on `is_finite()` before mutation. Collision-shape finiteness checks (`BhkMultiSphereShape`/`BhkConvexListShape`, bounded recursion) and the particle-emitter extraction boundary (finite/positive/sentinel-rejecting) both confirmed present at their current (post-refactor) paths.

### Dimension 10 — debug-ui Teardown & Shared-Allocator Safety — PASS, no findings
`crates/debug-ui` confirmed zero Vulkan/`unsafe` (CPU-only). `EguiPass` teardown confirmed to run immediately after `device_wait_idle()` and well before `destroy_device`, independently re-read. Texture `pending_free` deferred-by-one-frame confirmed correct (frees previous frame's list after the fence wait, stashes the new list at the end). Queue-mutex scope (CONC-D1-01/#1713) confirmed still minimal — held only around `set_textures`, released before `tessellate`/`cmd_draw`.

### Dimension 11 — Sandboxed Mod Runtime Trust Boundary — 2 findings (1 MEDIUM, 1 LOW)
See SAFE-D11-01 and SAFE-D11-02 above. WASI absence confirmed by `Cargo.toml` feature list, `cargo tree`, and a dedicated regression test. Capability gating confirmed centralized in `crates/mod-runtime/src/runtime/capabilities.rs` (`require_*` per capability) plus 19 `host/*.rs` WIT-interface modules (88 gate call sites), all erroring via `wasmtime::bail!` rather than degrading to a silent no-op — except the one gap filed as SAFE-D11-01, which is a missing *entity-visibility* check, not a missing *capability* check (the capability gate on that same function is present and correct). Per-instance isolation, `SandboxConfig::validate()`, fuel-exhaustion quarantine, the log-entry/byte cap, lifecycle quarantine-on-fault plus gate-on-`Active`, and `compile()`'s non-panicking rejection of malformed input were all independently re-verified by direct read.

---

## Next step

```
/audit-publish docs/audits/AUDIT_SAFETY_2026-09-11.md
```
