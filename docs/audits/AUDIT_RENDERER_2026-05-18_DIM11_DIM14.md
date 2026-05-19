# Renderer Audit — Dimensions 11 (TAA) + 14 (Material Table R1) — 2026-05-18

**Scope**: Focused audit on TAA (dimension 11) and Material Table R1 (dimension 14). Engine HEAD `bdbab7d8` (post-#968 close).
**Methodology**: per-dimension agents + manual backfill (both agents got cut off mid-investigation, so I filled in the gaps via targeted greps + reads). Findings cross-checked against `/tmp/audit/renderer/issues.json` for dedup.

## Executive Summary

Both audited subsystems are in solid shape. **One LOW finding** total — a stale path comment in `water.vert` pointing at a Rust source file moved during the Session-34 refactor.

- **TAA (M37.5)**: structurally clean. Halton jitter applied AFTER motion-vector capture in `triangle.vert`; YCoCg 3×3 clipping with `γ=1.5` widened per #1108 for current soft-shadow noise; alpha-blend disocclusion via bit-31 mesh-id marker; first-N-frames bootstrap gate prevents reading UNDEFINED history; `initialize_layouts` uses Vulkan-1.3 NONE-stage barriers; SPIR-V reflection pins the descriptor contract; composite samples TAA output via `rebind_hdr_views` (post-#1166). **No new findings.**
- **Material Table (R1)**: well-defended. `GpuMaterial` exactly 260 bytes (pinned by `gpu_material_size_is_260_bytes`, was 272 B pre-#804); every named-field offset across 16 vec4 slots is asserted by `gpu_material_field_offsets_match_shader_contract` (#806); all fields scalar; Hash/Eq use raw `as_bytes()`; over-cap returns id 0 with one-shot warn (#797); dedup-ratio telemetry exposed via the `tlm` console command (#780). All 5 shaders that declare `struct GpuInstance` are present per the canonical drift grep. **One LOW finding** — stale path comment.

## Rasterization Assessment

- **TAA path is canonical** — no per-frame UNDEFINED transitions on history, no descriptor aliasing across frame-in-flight slots, no NaN propagation through the temporal blend (`taa.comp:197-207` dormant defense). γ=1.5 absorbs the current `sunAngularRadius=0.020` soft-shadow noise without ghosting.
- **Material table dedup is hot** — ~14× hit rate on Prospector (1200 placements → 87 unique). Hot-path intern via precomputed hash + factory closure (#781 / PERF-N4) skips the 260-byte `to_gpu_material` construction on the ~97% dedup-hit path.
- **5-shader lockstep contract holding** — `feedback_shader_struct_sync.md` requires `triangle.vert` / `triangle.frag` / `ui.vert` / `water.vert` / `caustic_splat.comp` to track `GpuInstance` byte-for-byte. All 5 currently declare the struct. The water.vert finding is documentation drift, not a layout drift.

## RT Pipeline Assessment

Not in scope for dimensions 11 / 14. Brief note: TAA reads the same `mesh_id` G-buffer attachment the RT path writes; the bit-31 ALPHA_BLEND_NO_HISTORY flag is the contract that lets temporal accumulation honor RT-routed alpha-blend fragments.

## Findings — Grouped by Severity

### LOW

#### REN-D14-NEW-01 — `water.vert` cites stale Rust-struct path
- **File**: `crates/renderer/shaders/water.vert:36-40`
- **Observation**: comment says "Layout must match the Rust struct at `crates/renderer/src/vulkan/instance.rs` byte-for-byte." That path no longer exists post-Session-34; the struct now lives at `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (pinned by `gpu_instance_is_112_bytes_std430_compatible` at `scene_buffer/gpu_instance_layout_tests.rs:24`).
- **Why bug**: Doc rot. A future maintainer following the cited path lands on nothing and may either re-create a duplicate `instance.rs` or fall back to GLSL-only reasoning. Same pattern flagged in `feedback_audit_findings.md` — ~5 of 30 audit findings in the 2026-04 sweep were stale on premise specifically because of this kind of path drift.
- **Fix**: Replace the path with `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` and reference the byte-pin test (`gpu_instance_is_112_bytes_std430_compatible`). Consider scanning the other 4 shaders that declare `struct GpuInstance` for the same stale citation.

## What's Verified Clean (no findings)

### TAA (dim 11)
- Halton (2,3) jitter applied AFTER motion-vector capture in `triangle.vert:163, 227-232`. Motion vectors derive from un-jittered `currClip` so reprojection stays jitter-free.
- Camera UBO layout (`scene_buffer/gpu_types.rs:188-211`) carries jittered `view_proj` for `gl_Position`, un-jittered `prev_view_proj` for motion, and `jitter: [f32; 4]` (xy = NDC sub-pixel, zw reserved).
- Per-frame-in-flight history slots at `taa.rs:544-547` (`prev = (f + 1) % MAX_FRAMES_IN_FLIGHT = 2`); no cross-frame aliasing.
- 3×3 YCoCg variance clipping with `γ=1.5` (`taa.comp:164-193`) widened from 1.25 per #1108 for the bumped `sunAngularRadius=0.020`.
- Luma-weighted blend (`taa.comp:213-220`) reduces flicker on highlights.
- Disocclusion + alpha-blend rejection via mesh-id bit-31 marker (`taa.comp:149-158`).
- NaN/Inf guard pre-clamp (`taa.comp:197-207`, #903) — dormant defense.
- First-N-frames bootstrap via `should_force_history_reset(c) := c < MAX_FRAMES_IN_FLIGHT` (`taa.rs:104-109`).
- `initialize_layouts` walks UNDEFINED → GENERAL once at startup using `PipelineStageFlags::NONE` as src (#949 / #1100 / #1122).
- SPIR-V reflection at `taa.rs:307` (`validate_set_layout`) pins the 7-binding contract against GLSL.
- Composite rewires HDR descriptor to TAA output via `rebind_hdr_views` (mod.rs:1715-1717). Bloom intentionally stays on raw HDR pre-TAA (#1166 doc-only resolution).
- Disable path: TAA pipeline is `Option<TaaPipeline>`, `rebind_hdr_views` only fires when both exist. When TAA is None, composite stays on raw HDR by construction.

### Material Table R1 (dim 14)
- `gpu_material_size_is_260_bytes` test live (`material.rs:709`). Pre-#804 was 272 B.
- `gpu_material_field_offsets_match_shader_contract` test live (`material.rs:791`, #806). Catches within-vec4 reorders.
- Scalar-only invariant enforced; no `[f32; 3]` or `Vec3` in `GpuMaterial`.
- `Hash` / `PartialEq` / `Eq` via raw `as_bytes()` (`material.rs:356-373`); named pad fields zeroed at construction.
- Hot-path intern via `intern_by_hash` (precomputed SipHash + factory closure, `material.rs:619-655`); skips `to_gpu_material` construction on dedup hit.
- Over-cap policy: returns id `0` (neutral default) with `INTERN_OVERFLOW_WARNED.call_once` warn (#797 / SAFE-22, `material.rs:640-650`).
- `MAX_MATERIALS = 4096` sourced from `scene_buffer::MAX_MATERIALS`; per-frame upload truncates at `scene_buffer/upload.rs:261-535` as a safety net.
- Dedup-ratio telemetry wired (#780 / PERF-N1): `material.rs:673-676` exposes `interned_count()`; `commands.rs:349-355` prints `"materials: {N} unique / {M} interned ({R:.1}× dedup)"`. Prospector baseline 14× hit rate.
- `GpuInstance.material_id: u32` present in the Phase 3+ instance struct; 5 of 5 shaders consume it via `materials[instance.material_id].foo`.
- All 5 shaders declaring `struct GpuInstance` per the canonical `grep -l "struct GpuInstance" crates/renderer/shaders/` drift check: `triangle.vert`, `triangle.frag`, `ui.vert`, `water.vert`, `caustic_splat.comp`.
- `gpu_instance_is_112_bytes_std430_compatible` companion pin at `scene_buffer/gpu_instance_layout_tests.rs:24`.

## Prioritized Fix Order

1. **REN-D14-NEW-01** (LOW, doc rot) — replace stale path in `water.vert:39`, optionally sweep the other 4 GpuInstance-declaring shaders for the same citation.

That's the whole list. Both subsystems are otherwise clean.

## Dedup Notes

- TAA: no prior open issues match. Closed precedents: #1108 (γ width), #903 (NaN guard), #1166 (composite-vs-bloom HDR routing), #949/#1100/#1122 (NONE-stage barriers).
- Material Table R1: no prior open issues match. Closed precedents: #804 (260-byte size), #806 (offset pin), #797 (over-cap), #780 (telemetry), #781 (hot-path intern), #785 (`ui.vert` lockstep regression).

## Verification Commands

- `cargo test -p byroredux-renderer --lib gpu_material_size_is_260_bytes` — pass
- `cargo test -p byroredux-renderer --lib gpu_material_field_offsets_match_shader_contract` — pass (verified earlier under #806)
- `cargo test -p byroredux-renderer --lib gpu_instance_is_112_bytes_std430_compatible` — pass
- `grep -l "struct GpuInstance" crates/renderer/shaders/*.{vert,frag,comp}` returns the canonical 5 files.

Suggest: `/audit-publish docs/audits/AUDIT_RENDERER_2026-05-18_DIM11_DIM14.md`
