# Safety Audit — 2026-06-14

- **Scope**: Full `audit-safety` sweep — all 10 skill dimensions (FFI lifetime,
  memory-corruption/UB, per-frame/per-cell leaks, unsafe-block discipline, Vulkan
  spec, R1 material-table layout, IOR refraction, NPC/animation spawn, NIFAL
  NaN boundary, debug-ui teardown). Part of a `comprehensive` `/audit-suite` sweep.
- **Baseline**: `main` @ `435e265d` (post PHYSAL ragdoll cascade #1528/#1529, audit
  skill deep-rewrite #1530, FO4 NIF parse-gate fixes #1524, Rapier-leak fix #1520
  #1523, DoF focus guard #1525/#1527).
- **Context**: The prior safety audit (`AUDIT_SAFETY_2026-06-11.md`, baseline
  `1e8a25ab`) found 5 issues. Two of its five (the VolumetricsParams UBO pin
  SAFE-D7-NEW-05 → #1493, and the residual hostQueryReset gate SAFE-D2-NEW-04)
  were published+closed; the camera-relative & DoF UBO-size guard families landed.
  **Three of its findings were never published and remain live in current code** —
  re-confirmed and re-issued below with fresh dedup. This sweep also covers the
  large new surface that landed since June 11: **PHYSAL** (the Rapier-multibody
  ragdoll path — `crates/physics/src/ragdoll.rs`, `byroredux/src/ragdoll.rs`,
  the NIF ragdoll/constraint CInfo decode).
- **Dedup pool**: `gh issue list` snapshot (open: `/tmp/audit/issues.json`;
  all-states incl. bodies: `/tmp/audit/issues_all.json`, `/tmp/audit/issues_body.json`);
  cross-checked against `docs/audits/AUDIT_SAFETY_2026-06-11.md` and the closed
  June-1 issue range (#1382–#1449).

## Summary

| Severity | Count |
|----------|-------|
| CRITICAL | 0 |
| HIGH     | 2 |
| MEDIUM   | 3 |
| LOW      | 0 |
| **Total**| **5** |

The single most severe finding is **SAFE-D3-NEW-01** — the new PHYSAL ragdoll path
has no cell-unload teardown, so a ragdolling actor's rigid bodies, colliders, and
multibody joints leak into the Rapier solver on every cell crossing (the #1520
class, for the new component). The `PhysicsWorld::remove_ragdoll` cleanup function
exists but is **dead code** — never called from anywhere in the binary.

The unsafe-block discipline sweep came back **clean** (no NEW comment-less blocks,
no false SAFETY claims; PHYSAL/ragdoll code is pure safe Rust). All R1
material-layout pins, IOR/glass guards, B-spline FLT_MAX sentinels, AnimationClip
case-insensitive dedup, the three drop-ordering regression guards (deferred-destroy
drain after fence wait, AllocatorResource-before-VulkanContext, TLAS resize wait),
and the cxx no-pointer placeholder are all intact — recast as PASS below.

---

## Findings

### SAFE-D3-NEW-01: Ragdoll bodies/colliders/joints leak on cell unload — `remove_ragdoll` exists but is never wired
- **Severity**: HIGH
- **Dimension**: 3 — Memory & Resource Leaks (per-cell)
- **Location**: `byroredux/src/cell_loader/unload.rs:186` + `:365-388`
  (`release_victim_rapier_bodies`); cleanup fn at
  `crates/physics/src/ragdoll.rs:319-323` (`PhysicsWorld::remove_ragdoll`);
  ragdoll bodies attached at `byroredux/src/ragdoll.rs:229-236`
- **Status**: NEW
- **Description**: The PHYSAL ragdoll path (landed #1528/#1529) attaches a `Ragdoll`
  component to an actor on `ragdoll <id>` activation. `Ragdoll` carries its own
  `Vec<(EntityId, RigidBodyHandle)>` (`crates/physics/src/components.rs`) — these
  bodies are inserted directly into `PhysicsWorld::{bodies, colliders,
  multibody_joints}` by `build_ragdoll`, **not** via the `RapierHandles` component
  that the character/physics-sync path uses. The cell-unload leak guard
  `release_victim_rapier_bodies` only sweeps victims carrying `RapierHandles`
  (`unload.rs:370-377`); it never inspects the `Ragdoll` component. So when a cell
  unloads with a ragdolling actor in it, `world.despawn(eid)` drops the `Ragdoll`
  ECS row and **orphans** its Rapier bodies + colliders + multibody joints in the
  solver's sets and broad-phase / query-pipeline BVH — the exact unbounded-leak
  shape #1520 was filed to close, re-introduced for the new component.
- **Evidence**:
  - `grep -rn remove_ragdoll byroredux/src/` → **zero call sites.** The cleanup
    function `crates/physics/src/ragdoll.rs:319` (`pub fn remove_ragdoll`, whose
    own doc-comment says *"Mirrors the #1520 no-leak discipline so a cell unload
    mid-ragdoll doesn't strand bodies"*) is dead code.
  - `unload.rs:365-388` removes only `RapierHandles` rows; there is no `Ragdoll`
    branch and no `remove_ragdoll` call in the unload sequence.
  - There is **no deactivation path at all**: `activate_ragdoll`
    (`byroredux/src/ragdoll.rs:170`) inserts `Ragdoll` + `RagdollActive`; nothing
    in the binary ever removes either, so even an in-place re-load or actor death
    can't reclaim the bodies in-session.
- **Impact**: Per-cell leak of N rigid bodies + colliders + multibody joints
  (N = ragdoll bone count, ~10-20 for a humanoid) into the Rapier sets and BVH for
  every ragdolling actor present at unload. Multibody joints accumulating in the
  solver also degrade step cost over the app lifetime. The trigger today is
  manual-only (`ragdoll <id>` console command, no automatic death-ragdoll yet), so
  it is not yet a steady streaming leak in ordinary play — but it leaks
  deterministically the moment any manually-ragdolled actor's cell unloads, and
  becomes a continuous exterior-streaming leak the instant ragdoll-on-death is
  wired (the obvious next PHYSAL step). Rated HIGH per the #1520 precedent (same
  leak class, same severity) and because the fix already exists and is simply
  unconnected.
- **Related**: #1520 (CLOSED, `34c7a218` — the `RapierHandles` sibling of this
  exact leak); `crates/physics/src/ragdoll.rs:314-324`; the `Ragdoll` component
  `crates/physics/src/components.rs`.
- **Suggested Fix**: In `release_victim_rapier_bodies`, also collect each victim's
  `Ragdoll` component and call `pw.remove_ragdoll(&ragdoll)` (it already cascades
  colliders + multibody joints via `remove_body`). Add the `Ragdoll` row to the
  same victim sweep, and extend the `rapier_release_tests` guard to assert the
  body/collider/joint sets are empty after unloading a ragdolling actor. (Separately,
  wire a deactivation command/path so manual ragdolls can be reclaimed without a
  cell crossing.)

### SAFE-D2-NEW-02: Mesh-index overshoot guard is log-only — inconsistent geometry still uploads, draws, and feeds BLAS builds
- **Severity**: HIGH
- **Dimension**: 2 — Memory Corruption / UB (SSBO indexing / AS build input)
- **Location**: `crates/renderer/src/mesh.rs:387-404` (`accumulate_global_geometry`)
- **Status**: Carryover of unpublished SAFE-D6-NEW-01 (2026-06-11); **still live**
- **Description**: Commit `01251733` added a `#markarth-fragments` diagnostic that
  detects a mesh whose maximum local index is `>= vertices.len()` — but it only
  `log::error!`s and then **uploads the mesh anyway**: the
  `pending_vertices.extend_from_slice` / `pending_indices.extend_from_slice` calls
  at `mesh.rs:403-404` run unconditionally right after the check, with no `bail!`,
  clamp, or skip. The code is byte-for-byte unchanged since the June 11 audit
  flagged it; no issue was ever opened.
- **Evidence**:
  - `mesh.rs:388-401` — `if max_idx as usize >= vertices.len() { log::error!(...) }`
    (no early return); `:403-404` append to the global pool regardless.
  - `device.rs` never enables `robustBufferAccess` (no `robust_buffer_access` hits
    anywhere in `crates/renderer/src/`), so an out-of-range vertex fetch is UB, not
    a clamped read.
  - Static BLAS builds declare `max_vertex(vertex_count.saturating_sub(1))`
    (`blas_static.rs`); an index above `maxVertex` is an invalid AS build input per
    the Vulkan spec.
- **Impact**: A self-inconsistent (index, vertex) pair — from a NIF decode remap
  bug (the class the diagnostic was added to bisect), a corrupt file, or a
  mispointed CSG offset (see SAFE-D2-NEW-03) — produces (a) raster reads into
  *other meshes'* vertices in the shared global pool (the "exploding spike"
  artifact), (b) for a pool-tail mesh, an OOB GPU vertex fetch with robustness off
  (UB, potential DEVICE_LOST), and (c) an invalid BLAS build input. GPU-level UB on
  the AS/SSBO-indexing axis ⇒ HIGH (impact-based; per the severity table, wrong
  SSBO index / AS geometry is the CRITICAL family, here gated behind a malformed-
  decode trigger so held at HIGH).
- **Related**: SAFE-D2-NEW-03 (a producer that can emit exactly this); #1392
  (CLOSED — the analogous `instance_custom_index` guard was hardened from
  debug-only to a release runtime check, the template for this fix); 2026-06-11
  audit finding SAFE-D6-NEW-01 (never published).
- **Suggested Fix**: Turn the guard into a hard gate — `return` (skip the mesh, keep
  the log) or clamp offending indices to `vertices.len() - 1` before appending. The
  diagnostic value is preserved either way; the upload of known-inconsistent
  geometry is not.

### SAFE-D2-NEW-03: FO4 precombine decode emits triangle indices with no bounds check against `num_verts`
- **Severity**: MEDIUM
- **Dimension**: 2 — Memory Corruption / UB / NIF parse
- **Location**: `crates/nif/src/import/precombine.rs:109-146`
  (`decode_shared_geom_object`)
- **Status**: Carryover of unpublished SAFE-D6-NEW-02 (2026-06-11); **still live**
- **Description**: The M49 precombine path reads raw u16 triples from the PSG blob
  (`stream.read_u16_triple_array(tri_count)?`, `precombine.rs:140`) and converts
  them straight to `u32` indices in a plain push loop (`:141-145`) **without
  validating any index `< num_verts`**. `num_verts` is in scope and unused for
  validation. Unlike inline NIF geometry, the PSG slice is located by a
  `(filename_hash, data_offset)` pointer into a separate `.csg` blob — a hash
  collision or stale/mispointed offset silently decodes arbitrary bytes as indices
  (values up to 65535) against an arbitrary vertex count. Unchanged since June 11.
- **Evidence**: `precombine.rs:140-145` — read then push, no `< num_verts` check;
  the result flows into `ImportedMesh` → `accumulate_global_geometry`, whose only
  guard is the log-only diagnostic of SAFE-D2-NEW-02.
- **Impact**: Producer-side half of SAFE-D2-NEW-02 — a corrupt CSG read becomes OOB
  draw/BLAS input instead of a rejected object. MEDIUM per the "translatable block /
  parse mismatch" class; the escalating GPU consequences are owned by SAFE-D2-NEW-02.
- **Related**: SAFE-D2-NEW-02; `docs/engine/fo4-csg-format.md` (reverse-engineered
  format, M49); 2026-06-11 SAFE-D6-NEW-02 (never published).
- **Suggested Fix**: After the read loop,
  `if indices.iter().any(|&i| i as usize >= num_verts) { return Err(io::Error::new(InvalidData, ...)) }`
  — one pass, decode-time rejection with the object's hash in the message.

### SAFE-D9-NEW-04: PHYSAL ragdoll body/joint NIF extraction has no finite guards — NaN seed pose / joint limit reaches the Rapier solver and the GPU via writeback
- **Severity**: MEDIUM
- **Dimension**: 9 — NIFAL NaN/Inf boundary (UB facet) / physics-solver input
- **Location**: `crates/nif/src/import/collision.rs:291-305` (`extract_ragdoll`
  body fields) + `:377-401` (`ragdoll_joint` / `limited_hinge_joint`); consumed by
  `byroredux/src/ragdoll.rs:190-202` → `crates/physics/src/ragdoll.rs:99-127`
  (`build_ragdoll` → `RigidBodyBuilder::position`) and `:245-264`
  (`ragdoll_writeback_system`, no finite guard)
- **Status**: NEW
- **Description**: The collision **shape** extraction in this same file has finite
  guards on every radius / half-extent / center / vertex (`collision.rs:461-571`,
  the #1409 fix). The ragdoll **body** and **joint** extraction does **not**:
  `body.translation`, `body.rotation`, `body.mass`, and the joint scalars
  (`cone_max`, `twist_min`/`twist_max`, hinge `min_angle`/`max_angle`) are read as
  raw Havok floats and forwarded with no `is_finite()` check. They flow into
  `RagdollBodySpec.translation/rotation` → `iso_from_trs`
  (`crates/physics/src/convert.rs:47`, no guard) →
  `RigidBodyBuilder::dynamic().position(...)`, and the joint limits into
  `GenericJointBuilder::limits(JointAxis::AngX, [tmin, tmax])`
  (`crates/physics/src/ragdoll.rs:250-252`).
- **Evidence**:
  - `collision.rs:293` `mass: body.mass`, `:299-304` `translation: havok_to_engine(...)`
    / `rotation: havok_quat_to_engine(...)` — no `finite()` (contrast the guarded
    shape path two functions below).
  - `collision.rs:385-389` `cone_max: r.cone_max_angle`, `twist_min/twist_max` —
    raw, ungated.
  - Partial existing defenses (not full coverage): `frame_rot`
    (`ragdoll.rs:286-295`) falls back to identity for degenerate **axis** vectors
    (so a NaN twist/plane axis is contained), and `b.mass.max(1e-3)` survives a NaN
    mass (Rust `f32::max` returns the non-NaN operand). But a **NaN translation**
    has no fallback — it seeds the body position directly — and **NaN joint limits**
    `[NaN, NaN]` are handed to the solver. `ragdoll_writeback_system`
    (`byroredux/src/ragdoll.rs:256-264`) copies `body_pose` straight into
    `GlobalTransform.translation/rotation` with no `is_finite()` check, and that
    `GlobalTransform` feeds the bone palette → GPU skinning.
- **Impact**: A non-finite ragdoll body pose or joint limit from a corrupt /
  truncated Havok CInfo decode (the per-game seam PHYSAL warns is fragile) either
  seeds a NaN Rapier body or destabilizes the solver, and the un-guarded writeback
  propagates the resulting NaN pose into `GlobalTransform` → bone matrices → GPU
  skinned vertices (NaN-on-GPU = UB; NaN pixels stick through SVGF/TAA history).
  Trigger requires malformed ragdoll content + an active ragdoll, consistent with
  the MEDIUM precedent of the raw-NIF-scalar finite-guard class (#1411 CLOSED,
  #1434 OPEN).
- **Related**: #1409 (CLOSED — the shape-side finite guards in the *same file* that
  this path was not extended to); #1434 / #1382 (NIFAL scalar finite-guard class);
  PHYSAL spec `docs/engine/physal.md`.
- **Suggested Fix**: Apply the existing `finite(_)` / `finite_vec(_)` helpers
  (already defined at `collision.rs:468-475`) to the ragdoll body mass / translation
  / rotation and to the joint limit angles at the extract boundary — drop a body or
  joint whose CInfo is non-finite (mirroring the shape path's `?`-on-`None` drop).
  Belt-and-suspenders: add an `is_finite()` skip in `ragdoll_writeback_system`
  before writing `GlobalTransform`.

### SAFE-D11-NEW-05: NaN `glossiness` propagates through `clamp` into canonical `Material.roughness` after `resolve_pbr`
- **Severity**: MEDIUM
- **Dimension**: 11 — NIFAL Canonical-Translation Safety (NaN-on-GPU)
- **Location**: `byroredux/src/material_translate.rs:232-233`
  (`normal_alpha_spec_roughness`) + `:286-288`
  (`resolve_normal_alpha_spec_roughness` writeback)
- **Status**: Carryover of unpublished SAFE-D11-NEW-03 (2026-06-11); **still live**
- **Description**: The #1480 fix moved the normal-alpha-as-spec roughness derivation
  from per-draw render time to a once-at-spawn resolve — correct for the
  resolve-once contract, but the formula `(1.0 - glossiness / 100.0).clamp(0.05,
  0.95)` (line 233) runs in `resolve_normal_alpha_spec_roughness`, which executes
  **after** `resolve_pbr()` and **overwrites** the resolved canonical roughness
  (`m.roughness = r`, line 287). `Material.glossiness` is a raw NIF binary float
  (`walker.rs:314` `shader.glossiness`, `:600` `mat.shininess`) with no
  `is_finite()` guard anywhere on its path, and Rust's `f32::clamp` **propagates
  NaN** (`NaN.clamp(a, b) == NaN`). A non-finite `glossiness` on an
  alpha-bearing-normal lit surface therefore ships `roughness = NaN` past the only
  NaN gate in the pipeline (`resolve_pbr`'s `is_nan` check at `material.rs:639`,
  which already ran) into the `GpuMaterial` SSBO. Code unchanged since June 11.
- **Evidence**: gate `normal_alpha_spec_applies` (`material_translate.rs:189-201`)
  checks `metalness`/`env_map_scale` (NaN comparisons are false, so those NaNs
  self-block) but **not** `glossiness`; `glossiness` is only consumed in the
  NaN-surviving `clamp` at line 233. The `specular_strength > 1.2` arm (line 234)
  self-blocks on NaN; the `normal_has_alpha` arm does not.
- **Impact**: NaN roughness on the GPU for the affected draw — NaN GGX terms poison
  the lit color, and through SVGF/TAA temporal accumulation a single NaN pixel
  contaminates history buffers (sticky, frame-persistent). Gate population is large
  (every Skyrim/Gamebryo-era lit surface with an alpha-bearing normal map and no
  gloss map); trigger needs a malformed/non-finite `glossiness`, consistent with
  the MEDIUM precedent (#1411/#1434).
- **Related**: #1434 (OPEN — same class, `NiPSysGrowFadeModifier.base_scale`);
  #1480 (CLOSED — created this code, did not flag the NaN path); 2026-06-11
  SAFE-D11-NEW-03 (never published).
- **Suggested Fix**: In `normal_alpha_spec_roughness`, early-return `None` when
  `!glossiness.is_finite()` (one line), or sanitize `glossiness` to a finite
  default at the `translate_material` boundary so every downstream consumer
  (including `resolve_pbr`'s classifier arm at `material.rs:579`) is protected.

---

## Verified clean / regression guards intact (PASS — not re-reported)

| Guard | Verified at | Verdict |
|---|---|---|
| **Dim 1 — cxx no-pointer placeholder** | `crates/cxx-bridge/src/lib.rs:9` — single `unsafe extern "C++"`, no `*const`/`&[u8]`/`Box` signatures | HOLDS |
| **Dim 4 — unsafe-block discipline** | full sweep (subagent): ~540 unsafe blocks / ~227 SAFETY comments; gap is exclusively ash FFI clusters + `unsafe fn` call sites whose contract lives on the callee. No NEW comment-less blocks, no false SAFETY claims. ECS cached-pointer derefs (`query.rs:64/135/143/289`) sound (guard co-located with pointer). PHYSAL/ragdoll code is **pure safe Rust** (zero unsafe). #1432 coverage holds. | CLEAN |
| **Dim 2 — NIF POD reads** | `stream.rs:379` / `header.rs:382` `from_raw_parts_mut` — `checked_mul` length, `AnyBitPattern` bound, big-endian compile gate; sound + commented (#1439) | HOLDS |
| **Dim 2 — `BuiltinType::from_u32`** | `crates/sfmaterial/src/types.rs` — checked match + `_ => Err(UnsupportedBuiltin)` (#1396) | HOLDS |
| **Dim 3 — Rapier release on unload (`RapierHandles`)** | `unload.rs:365-388` + `rapier_release_tests.rs` (#1520) — intact for the character/sync path (the *ragdoll* gap is SAFE-D3-NEW-01) | HOLDS |
| **Dim 3 — deferred-destroy drain after fence wait** | `context/draw.rs:440-462` — `tick_deferred_destroy` runs AFTER `wait_for_fences` | HOLDS |
| **Dim 3 — AllocatorResource before VulkanContext** | `byroredux/src/main.rs` Drop removes resource + takes renderer before field drop (#1406/#1477); allocator-independent destroys hoisted out of the allocator guard (#1483, `5c2b0137`) | HOLDS |
| **Dim 5 — TLAS resize `device_wait_idle`** | `acceleration/tlas.rs` before freeing old allocation (#1390) | HOLDS |
| **Dim 5/6 — GpuCamera 336 B + reflect pin** | `gpu_camera_is_336_bytes` (`gpu_instance_layout_tests.rs:56`) + `camera_ubo_size_matches_gpu_camera_in_every_shader` (`reflect.rs:433`); VolumetricsParams now pinned too (#1493, `2db2d900`) | HOLDS |
| **Dim 6 — GpuMaterial 300 B + offset pin** | `gpu_material_size_is_300_bytes` + `gpu_material_field_offsets_match_shader_contract` (`material.rs:1338`, `offset_of!` per field); intern cap 16384 + upload `.min(MAX_MATERIALS)` | HOLDS |
| **Dim 6 — GpuInstance 112 B, 5-shader lockstep** | `gpu_instance_is_112_bytes_std430_compatible` + name-drift guard incl. `water.vert` (#1498) | HOLDS |
| **Dim 7 — IOR/glass guards** | `GLASS_RAY_BUDGET = 1048576` + `REFRACT_PASSTHRU_BUDGET = 2` enforced at the IOR entry; Frisvad basis active (`triangle.frag:420/438`); passthrough identity check present | HOLDS |
| **Dim 8 — B-spline FLT_MAX sentinel** | `crates/nif/src/anim/bspline.rs:178/328/359/394` — translation/quat/scale all FLT_MAX-gated | HOLDS |
| **Dim 8 — AnimationClipRegistry case-insensitive dedup** | `crates/core/src/animation/registry.rs:99-106` ASCII-lowercase keying | HOLDS |
| **Dim 11 — `translate_material` resolve_pbr** | `material_translate.rs:160` runs `resolve_pbr()` unconditionally (NaN sentinels seeded + resolved); the one residual gap is SAFE-D11-NEW-05 | HOLDS (w/ noted gap) |
| **DoF focus_dist degenerate guard** | `dof_effective_view_proj` + `focus_dist <= DOF_MIN_FOCUS_DIST` pinhole fallback (#1525, `4d61e802`) — dormant path, guard landed | HOLDS |

## Existing-issue overlaps observed and skipped (dedup)

| Issue | State | Relation |
|---|---|---|
| #1500 (REN2-15) | OPEN | `NORMAL_ALPHA_SPEC_BIT` Rust-GLSL lockstep pin — adjacent to SAFE-D11-NEW-05's gate, different concern (missing pin vs NaN propagation); not duplicated. |
| #1434 (NIFAL-S5) | OPEN | GrowFadeModifier finite guard — same class as SAFE-D11-NEW-05 / SAFE-D9-NEW-04, different field/site. |
| #1438 (IOR-03) | OPEN | ray-budget atomicAdd overshoot — shader-side, unchanged (documented #1438). |
| #1427 (EGUI-03) | OPEN | EguiPass pending_free flush — debug-ui teardown, unchanged. |
| #1426 (VKC-005) | OPEN | allocator-leak early-return skips device_wait_idle — unchanged. |
| #1404 (NCPS-04) | OPEN | R32_UINT atomic format-feature query — unchanged. |
| #1387 (RT-04) | OPEN | skin output buffer VERTEX_BUFFER flag — unchanged. |
| #1384 (IOR-04) | OPEN | three bitfields sharing 128u — covers the DBG_VIZ_GLASS_PASSTHRU cross-bitfield concern. |
| #1445 (LC-D9-02) | OPEN | `extract_emitter_params` planar_angle finite-sweep gap — adjacent to Dim 9, not duplicated. |

## Method notes

- Per the speculative-Vulkan-fix policy, no barrier/stage-mask changes are proposed;
  none of today's findings require RenderDoc to verify (all are CPU-side gates,
  decode-time validation, or ECS teardown wiring). `cargo check -p byroredux-physics
  -p byroredux` clean at baseline.
- Three of the five findings are direct carryovers of the 2026-06-11 audit's
  unpublished findings (SAFE-D6-NEW-01/02, SAFE-D11-NEW-03), re-confirmed against
  current code — none were ever filed as issues, none were fixed. The two NEW
  findings (SAFE-D3-NEW-01, SAFE-D9-NEW-04) are both in the PHYSAL ragdoll surface
  that did not exist on June 11.
- Scratch: `/tmp/audit/` (issue snapshots, dim notes).

Next step: `/audit-publish docs/audits/AUDIT_SAFETY_2026-06-14.md`
