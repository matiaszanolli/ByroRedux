# Issues 2739, 2749, 2748, 2750

All four filed from `docs/audits/AUDIT_RENDERER_2026-08-12b.md`. Domain: **renderer** (`byroredux-renderer`).

## #2739 — REN-D4-01: recreate_for_swapchain's fence loop destroys before a fallible recreate with no null-out
- **Severity**: HIGH
- **Location**: `crates/renderer/src/vulkan/sync.rs` (`recreate_for_swapchain`, `in_flight` loop); `crates/renderer/src/vulkan/context/resize.rs` (`recreate_screen_passes`)
- **Bug**: `in_flight` loop does `destroy_fence` then fallible `create_fence(...)?` without nulling/clearing first, unlike the `render_finished` loop right above it which does `clear()` before rebuilding. If `create_fence` fails mid-loop, `self.in_flight` holds destroyed handles that get reused/double-destroyed later (incl. in `Drop`). Also `recreate_screen_passes` assigns `self.framebuffers = create_main_framebuffers(...)` **before** calling `recreate_for_swapchain(...)?`, defeating the #1211 `framebuffers.is_empty()` sentinel.
- **Fix**: Mirror `render_finished`'s `clear()`-before-rebuild pattern for `in_flight`; move `framebuffers` assignment after `recreate_for_swapchain` succeeds.

## #2749 — REN-D11-2026-08-12-02: four production early-returns in triangle.frag skip FSR reactive/transparency mask writes
- **Severity**: MEDIUM
- **Location**: `crates/renderer/shaders/triangle.frag` — mask init (~line 117-118), `MATERIAL_KIND_EFFECT_SHADER` arm (~759), `MATERIAL_KIND_NO_LIGHTING` arm (~1024), IOR/RT glass arm, `DBG_VIZ_GLASS_PASSTHRU` arm (~2102), tail policy (~3683-3684).
- **Bug**: Masks default to 0 at top of `main()`. Four production early-return arms (effect shader, no-lighting, RT/IOR glass, debug glass passthrough) return without setting `outFsrReactive`/`outFsrTransparency`, unlike `FIRE_REFRACTION` which sets both to 1.0 before returning. Tail policy says glass should get `outFsrTransparency = 1.0`, but glass taking the IOR branch reports 0.0 while the same glass falling back to Fresnel reaches the tail and reports 1.0 — inconsistent frame-to-frame.
- **Fix**: Hoist tail policy into helper invoked before each production early return (or compute `fsrCoverage` early, set masks once before branching); narrow the top-of-main comment.

## #2748 — REN-D3-2026-08-12-01: GpuInstance's five-mirror lockstep guard is presence-only
- **Severity**: MEDIUM
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs` (`every_shader_struct_gpu_instance_names_material_kind_slot`); mirrors in `bindings.glsl`, `triangle.vert`, `ui.vert`, `water.vert`, `caustic_splat.comp`; Rust struct in `gpu_types.rs`.
- **Bug**: Existing guard is `src.contains()` needle checks only — no field-order/completeness comparison across mirrors or vs Rust struct. Siblings `gpu_light_glsl_copies_stay_in_lockstep` (#1916) and `gpu_material_glsl_field_order_matches_rust_struct` (#1657) already do full comparisons; `GpuInstance`'s guard doesn't. No drift exists today (verified) but nothing would catch future drift.
- **Fix**: Add `gpu_instance_glsl_copies_stay_in_lockstep` test modeled on `gpu_light_glsl_copies_stay_in_lockstep`: extract+strip struct bodies across all 5 sites, assert byte-identical field lists, and assert order matches Rust struct via `parse_rust_struct_fields`/`normalize_ident`. Fix stale `gpu_types.rs` protocol comment.

## #2750 — REN-D3-2026-08-12-02: GpuCamera::dof_params.zw documented reserved(0) but carries live data
- **Severity**: MEDIUM
- **Location**: `crates/renderer/src/vulkan/scene_buffer/gpu_types.rs` (`GpuCamera::dof_params` doc, ~line 317); GLSL mirrors `triangle.vert:92`, `water.vert:82`, `cluster_cull.comp:68`, `caustic_splat.comp:75` (stale); `bindings.glsl:232` (already correct).
- **Bug**: `.z` = `light_atten_knee` (live-tunable via `light.atten`), `.w` = `camera_static` (drives GI seed decorrelation) — both live, but Rust doc and 4/5 GLSL mirrors say "zw = reserved (0)". Byte layout is fine; this is a stale-comment/semantic-doc-drift issue only. `docs/engine/shader-pipeline.md` already has the correct wording (fixed 2026-07-09).
- **Fix**: Replace `zw = reserved (0)` wording in `gpu_types.rs` with `bindings.glsl`'s correct wording + consumer list; propagate to the 4 stale GLSL mirrors. Comment-only, no `.spv` recompile needed.

## Domain
renderer → `byroredux-renderer`
