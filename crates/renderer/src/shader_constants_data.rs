// Single source of truth for constants that live in both Rust and GLSL.
// This file is included (via include!) by both:
//   - crates/renderer/src/shader_constants.rs   (library)
//   - crates/renderer/build.rs                   (build script → generates shader_constants.glsl)
//
// When updating a value here, rebuild with `cargo build -p byroredux-renderer`
// to regenerate the GLSL header, then recompile the affected GLSL shaders.

// Depth-buffer convention (#3308). Derived from the engine's single
// authority, `ACTIVE_DEPTH_MAPPING`, so the GLSL side cannot disagree with
// the projection / clear / compare state the Rust side installs — a stale
// GLSL copy would make every shader's background test answer backwards.
//
// The SPIR-V is still precompiled and checked in, so flipping the mapping
// means `cargo build -p byroredux-renderer` (regenerates the header) then
// recompiling the shaders that include it.
pub const BYRO_REVERSED_Z: u32 =
    byroredux_core::ecs::components::camera::ACTIVE_DEPTH_MAPPING.is_reversed() as u32;
pub const BYRO_DEPTH_CLEAR: f32 =
    byroredux_core::ecs::components::camera::ACTIVE_DEPTH_MAPPING.clear_value();

// Rec. 709 luma weights for linear-sRGB (R, G, B), emitted as the
// `LUMA_REC709` vec3. #4347 — every shader that weights a colour into a
// luminance used to retype these three numbers; a test in shader_constants.rs
// pins this array to `byroredux_core::radiometry::LINEAR_SRGB_LUMA`.
pub const LUMA_REC709: [f32; 3] = [0.2126, 0.7152, 0.0722];

// #5154 — dark-adaptation chroma compress (presentation.frag +
// `tonemap.rs`'s mirror). `EXPOSURE_METER_NEUTRAL` is the Frostbite
// middle-grey constant the meter produces at EV100 = 0 (exposure = 1.2 *
// 2^-EV100); `exposure.rs`'s EXPOSURE_CONSTANT resolves to it. As the
// meter lifts above that point, presentation desaturates the tonemapper
// input by 2^(-ADAPTATION_SAT_FALLOFF * lift_stops): one stop of lift
// costs a quarter stop of chroma — about 0.88 chroma at the 2× envelope
// cap (#5158, ~0.74 stops); `exposure ev` compensation can lift further.
// Lift-only — a bright scene never gains saturation.
pub const EXPOSURE_METER_NEUTRAL: f32 = 1.2;
pub const ADAPTATION_SAT_FALLOFF: f32 = 0.25;

// #5482 — the IMGS/IMAD cinematic contrast (presentation.frag's
// `gradeContrast` + `tonemap.rs`'s `grade_contrast` mirror). The pivot is
// middle grey in EXPOSED units — the meter's own output scale — so contrast
// stretches around the metered key whatever the adaptation. Below
// `GRADE_CONTRAST_TOE` the linear stretch hands over to a power law that
// approaches black instead of crossing it: the decompiled vanilla ISHDR
// pass's `lerp(avg, color, contrast)` floors every shade under
// `pivot * (1 - 1/contrast)` to zero, and Skyrim Community Shaders'
// `ISHDR.hlsl` ("Contrast modified to fix crushed shadows") blends in
// `pivot * (color/pivot)^contrast` by `saturate(power / 0.1)`; the toe is
// that 0.1.
pub const GRADE_CONTRAST_PIVOT: f32 = 0.18;
pub const GRADE_CONTRAST_TOE: f32 = 0.1;

// #5254 — the EV100 meter calibration, Frostbite §5.6: `EV100 =
// log2(L * S / K)` with the reflected-light meter constant K = 12.5
// (ISO 2720) and sensor sensitivity S = 100 (ISO 100). `vulkan/exposure.rs`
// resolves its constants from here, and the metering shader reads the
// ratio as a `#define` — hand-typing `8.0` in the shader let a retune of
// K (ISO 2720 allows 12.5 to 14) move the Rust mirror functions and the
// #5158 envelope tests while the GPU meter stayed behind.
pub const LIGHT_METER_CALIBRATION_K: f32 = 12.5;
pub const SENSOR_SENSITIVITY_S: f32 = 100.0;
pub const EXPOSURE_METER_S_OVER_K: f32 = SENSOR_SENSITIVITY_S / LIGHT_METER_CALIBRATION_K;

// Single-workgroup reductions (#5620). `exposure_meter.comp` and
// `sky_irradiance.comp` each reduce inside ONE workgroup with a shared-memory
// tree, so the workgroup width is coupled to the `shared[]` array sizes, the
// per-invocation sample stride and the first reduction step (width / 2) —
// all derived from these constants, never retyped. The host dispatches
// exactly one group (meter) / `SKY_SH_COEFFICIENTS` groups (irradiance).
pub const EXPOSURE_METER_WORKGROUP: u32 = 64;
/// The meter samples a `EXPOSURE_METER_SAMPLE_GRID`² grid across the image.
pub const EXPOSURE_METER_SAMPLE_GRID: u32 = 64;
pub const SKY_IRRADIANCE_WORKGROUP: u32 = 64;
/// L2 spherical harmonics: 9 RGB coefficients, one workgroup each; the
/// `SkyIrradiance` SSBO is `SKY_SH_COEFFICIENTS` `vec4`s.
pub const SKY_SH_COEFFICIENTS: u32 = 9;

const _: () = {
    // A halving tree reduction needs a power-of-two width.
    assert!(EXPOSURE_METER_WORKGROUP.is_power_of_two());
    assert!(SKY_IRRADIANCE_WORKGROUP.is_power_of_two());
};

// Cluster grid
pub const CLUSTER_TILES_X: u32 = 16;
pub const CLUSTER_TILES_Y: u32 = 9;
pub const CLUSTER_SLICES_Z: u32 = 24;
pub const CLUSTER_NEAR: f32 = 0.1;
pub const CLUSTER_FAR_FLOOR: f32 = 10_000.0;
pub const CLUSTER_FAR_FALLBACK: f32 = 50_000.0;
// Raised from 32 → 128 after the LIGH `falloff_exponent` plumb-through
// (which extended the per-light visible range to `radius * 2.5`)
// exposed densely-lit FO4 interior cells overflowing the cap. The
// cluster cull's `atomicAdd` gives arbitrary slot ordering when the
// actual light count exceeds the cap, so adjacent clusters drop
// DIFFERENT subsets of lights — producing visible tile boundaries on
// floors / walls (Institute Bioscience cargo room was the canonical
// regression). Buffer grows from 3456 * 32 * 4 = 442 KB to 3456 * 128
// * 4 = 1.7 MB — trivial against the multi-GB VRAM budget.
//
// 128 proved insufficient once the R2 GPU telemetry was exercised on
// Starfield: Cydonia measured 305 overlaps in its densest cluster and dropped
// 3,729 light references across 52 clusters. 512 retains the complete measured
// set with 68% headroom. The flat index list grows to 3456 * 512 * 4 = 6.75 MB
// per frame-in-flight and the workgroup-local index array to 2 KB, both modest
// on the renderer's 6 GB minimum target.
pub const MAX_LIGHTS_PER_CLUSTER: u32 = 512;

// ReSTIR reservoir word packing. The low ten bits store a scene-light index;
// their all-ones value is reserved as the packed invalid sentinel, so the
// upload capacity is a COUNT of 1023 and valid uploaded indices are 0..=1022.
// The remaining bits hold the stable surface ID used by spatial reuse.
//
// Keep this complete contract here rather than duplicating masks and shift
// literals between scene-buffer sizing and triangle.frag (#2778).
pub const RESERVOIR_LIGHT_BITS: u32 = 10;
pub const RESERVOIR_LIGHT_MASK: u32 = (1u32 << RESERVOIR_LIGHT_BITS) - 1;
pub const RESERVOIR_SURFACE_MASK: u32 = u32::MAX >> RESERVOIR_LIGHT_BITS;
pub const MAX_LIGHTS: usize = RESERVOIR_LIGHT_MASK as usize;

const _: () = {
    assert!(MAX_LIGHTS == 1023);
    assert!(RESERVOIR_SURFACE_MASK == 0x003f_ffff);
};

// ReSTIR-DI direct-light EMA (the shadow-ratio estimator in `triangle.frag`,
// #5369 / #5617). The per-surface history length is stored in a
// packHalf2x16 half and the EMA weight is `max(1/histLen, alphaFloor)`:
//
// - parked and scene-static (history mode >= `HISTORY_MODE_SCENE_STATIC`):
//   a 1024-frame tail with a 0.002 floor — measured on the Cornell penumbra
//   at one shadow ray per pixel (`cornell_parked_penumbra_converges_at_one_shadow_ray`);
// - otherwise: a 16-frame tail with a 0.1 floor, so ghosts stay responsive.
pub const RESTIR_HISTORY_CAP_PARKED: f32 = 1024.0;
pub const RESTIR_HISTORY_CAP_MOVING: f32 = 16.0;
pub const RESTIR_ALPHA_FLOOR_PARKED: f32 = 0.002;
pub const RESTIR_ALPHA_FLOOR_MOVING: f32 = 0.1;
// ReSTIR surface/plane compatibility gates shared by temporal reuse, decal
// substrate reuse and spatial-neighbour rejection in `triangle.frag`.
pub const RESTIR_SURFACE_NORMAL_COS: f32 = 0.906;
pub const RESTIR_TEMPORAL_DEPTH_FLOOR: f32 = 2.0;
pub const RESTIR_SUBSTRATE_DEPTH_FLOOR: f32 = 8.0;
pub const RESTIR_DEPTH_REL_TOL: f32 = 0.005;
pub const RESTIR_VIEW_COS_FLOOR: f32 = 0.1;
// Per-pixel mover taint: a pixel whose selected shadow ray hit an instance
// that moved this build (`INSTANCE_FLAG_MOVED_THIS_FRAME`) keeps the moving
// tail, and keeps it for this many hit-free frames past the moving cap before
// rejoining the parked tail, so a shadow an occluder leaves behind fades at
// the moving floor instead of ghosting at the parked one.
pub const RESTIR_MOVER_SETTLE_FRAMES: f32 = 32.0;

// Direct-shadow spatial filter (`shadow_filter.comp`): an edge-aware 5x5
// B3 a-trous pass over the per-pixel ReSTIR shadow ratio, the spatial half
// the ratio EMA lacked (shadows had no spatial filter at all, so they boiled
// in motion). Edge stops: geometric normal (power), view-ray distance against
// the local slope (scale, in units of the expected slope step), and the ratio
// itself (sigma, in ratio units: a 0-to-1 hard edge weighs exp(-1/sigma)).
pub const SHADOW_FILTER_NORMAL_POWER: f32 = 32.0;
pub const SHADOW_FILTER_DEPTH_SLOPE_SCALE: f32 = 2.0;
pub const SHADOW_FILTER_RATIO_SIGMA: f32 = 0.25;
// The correction fades with the centre pixel's history length: full strength
// up to this many frames (the moving tail's cap), then 1/length, so a ratio
// the EMA has already converged (a parked 1024-frame tail) is not blurred.
pub const SHADOW_FILTER_CONFIDENT_HISTORY: f32 = 16.0;
// The reservoir stores view-ray distance as a half-float (about one unit of
// step at 1 000-2 000 units); the depth edge-stop tolerates this fraction of
// the distance as quantization, or it reads the steps as depth edges and
// bands flat floors.
pub const SHADOW_FILTER_DEPTH_REL_TOL: f32 = 0.004;

// `GpuCamera.dof_params.w`: the history mode `restir_history_mode` encodes
// and `triangle.frag` decodes with two thresholds only — "camera parked" is
// `> HISTORY_MODE_PARKED - 0.5`, "scene-static (deep history)" is
// `> HISTORY_MODE_SCENE_STATIC - 0.5`. Mode 3 (parked, light-rig geometry
// static under intensity flicker) rides above the deep threshold.
pub const HISTORY_MODE_MOVING: f32 = 0.0;
pub const HISTORY_MODE_PARKED: f32 = 1.0;
pub const HISTORY_MODE_SCENE_STATIC: f32 = 2.0;
pub const HISTORY_MODE_RIG_STATIC: f32 = 3.0;

const _: () = {
    // packHalf2x16 halves are integer-exact only up to 2048: a deeper cap
    // would stall `histLen + 1` and freeze the EMA weight.
    const HALF_INTEGER_EXACT_MAX: f32 = 2048.0;
    assert!(RESTIR_HISTORY_CAP_PARKED <= HALF_INTEGER_EXACT_MAX);
    assert!(RESTIR_HISTORY_CAP_MOVING <= RESTIR_HISTORY_CAP_PARKED);
    assert!(RESTIR_ALPHA_FLOOR_PARKED <= RESTIR_ALPHA_FLOOR_MOVING);
    // The tainted count runs past the moving cap to the settle end; it must
    // stay integer-exact in the half it is stored in.
    assert!(RESTIR_HISTORY_CAP_MOVING + RESTIR_MOVER_SETTLE_FRAMES <= HALF_INTEGER_EXACT_MAX);
    // The thresholds sit half a step below their mode, so each mode is
    // strictly above its own threshold and the previous mode strictly below.
    assert!(HISTORY_MODE_MOVING < HISTORY_MODE_PARKED);
    assert!(HISTORY_MODE_PARKED < HISTORY_MODE_SCENE_STATIC);
    assert!(HISTORY_MODE_SCENE_STATIC < HISTORY_MODE_RIG_STATIC);
};

// Ray-query alpha-skip walk budget. Bounds any loop that walks a ray query
// past alpha-tested/uncovered geometry (reflection self-skips, water's
// foliage-cutout skips, opaque shadow-transmittance layering) — eight
// layers is enough for realistic authored foliage/grate stacks without
// letting a pathological stack turn one ray into an unbounded walk.
//
// #2265 / TD7-001 — previously hand-redeclared as `MAX_TRANSPARENT_SKIPS`
// (raytrace.glsl, water.frag) and `MAX_OPAQUE_LAYERS` (shadow_transport.glsl)
// at each call site; consolidated here so a future tuning pass has one
// value to change instead of three to find.
pub const MAX_ALPHA_SKIP_LAYERS: u32 = 8;

// #5249 — own-instance hops the transmission lobes' shadow ray may take
// before falling back to "lit" (#5192's conservative default). Each hop
// advances past one surface of the receiver's own body (the #5018
// light-side origin starts just inside it); the budget only has to cover
// a body's own surfaces along one lobe ray, never foreign blockers (the
// first foreign hit hands the leg to the shared alpha/glass-aware
// transport). Sibling budget of `MAX_ALPHA_SKIP_LAYERS` above — same
// order of body-surface crossings, so it shares the 8-layer sizing
// rationale rather than inventing a second number.
//
// The shader half of #5249 (`shadow_transport.glsl`'s
// `traceShadowTransmittanceSkippingInstance`) shipped using this name
// without the definition ever landing here, which left `triangle.frag`
// uncompilable from source and its committed `.spv` pair frozen at a
// pre-fix intermediate (the FNV-interior black-wall regression).
pub const MAX_TRANSMISSION_SELF_SKIPS: u32 = 8;

// RT reach budgets shared between `water.frag` and `triangle.frag`.
//
// #3745 / TD7-2026-08-30-01 — before this, both budgets were hand-typed
// literals at six call sites across the two shaders (`water.frag`'s own
// declarations, plus `triangle.frag:1092,1723,2712` for the reflection
// distance and `triangle.frag:2037`'s `REFRACT_MAX_REACH` plus its
// `:1723`/`:2025` re-issue sites for the refraction distance) with no
// shared definition — `water.frag`'s own comment claimed `DIST_FALLOFF`
// "matches triangle.frag", which was false: `0.0015` appeared nowhere in
// `triangle.frag`, and `triangle.frag` has no `DIST_FALLOFF` at all.
// Consolidated here so the claim is enforced, not aspirational.
pub const RT_REFLECTION_MAX_DIST: f32 = 5000.0;
pub const RT_REFRACTION_MAX_DIST: f32 = 2000.0;
/// Distance-based reflection/refraction intensity falloff rate. Despite
/// `water.frag`'s pre-fix comment, `triangle.frag` has no matching
/// consumer today (its nearest analogue is the glass optical-thickness
/// `0.004`, a different quantity) — `water.frag` is the sole consumer.
/// Centralized here anyway per the single-source-of-truth doctrine this
/// file exists for, and so a future `triangle.frag` consumer of the same
/// quantity has one definition to reach for instead of retyping it.
pub const RT_DIST_FALLOFF: f32 = 0.0015;

// Vertex layout (global SSBO)
pub const VERTEX_STRIDE_FLOATS: u32 = 26;
// Skinned-vertex OUTPUT stride (`SkinSlot::output_buffer`) — position
// only, deliberately NOT the full 26-float input layout.
//
// #2170 — the slot output used to mirror the input stride so a deferred
// "Phase 3" (raster reading pre-skinned vertices as a VBO) could swap in
// without a layout change. Phase 3 never landed, `create_slot` omits the
// `VERTEX_BUFFER` usage it would need anyway (#681/MEM-2-6), and the only
// live consumer is the skinned-BLAS build, which reads the buffer as
// `R32G32B32_SFLOAT` and touches 12 of every 104 bytes. Provisioning the
// other 92 cost an 8.7x over-allocation per slot plus the bandwidth to
// write pass-through lanes nothing reads.
//
// If Phase 3 is ever revived it needs the full layout back AND the
// `VERTEX_BUFFER` usage bit — that is one commit, not a silent
// dependency on this constant.
//
// 3 floats = 12 B satisfies the AS-build requirement that `vertexStride`
// be a multiple of the vertex format's component size (4 B for f32).
pub const SKIN_OUTPUT_STRIDE_FLOATS: u32 = 3;
// position(0..3) color RGBA(3..7) normal(7..10) uv(10..12) — see
// crates/renderer/src/vertex.rs.
pub const VERTEX_COLOR_OFFSET_FLOATS: u32 = 3;
pub const VERTEX_NORMAL_OFFSET_FLOATS: u32 = 7;
pub const VERTEX_UV_OFFSET_FLOATS: u32 = 10;
pub const VERTEX_TANGENT_OFFSET_FLOATS: u32 = 22;
// Bone lanes are u32/f32 at these byte offsets; GLSL views the packed
// vertex SSBO through float words, so derive the corresponding word indices
// from the host ABI instead of repeating the lane numbers in shaders.
pub const VERTEX_BONE_INDICES_OFFSET_FLOATS: u32 =
    (std::mem::offset_of!(crate::Vertex, bone_indices) / 4) as u32;
pub const VERTEX_BONE_WEIGHTS_OFFSET_FLOATS: u32 =
    (std::mem::offset_of!(crate::Vertex, bone_weights) / 4) as u32;

// Splat lanes are packed 4×u8 unorm, NOT floats — see the WARNING block above
// `GlobalVertices` in include/bindings.glsl. Recover with
// `unpackUnorm4x8(floatBitsToUint(vertexData[base + N]))`; reading them as
// floats yields NaN/denormal garbage.
pub const VERTEX_SPLAT0_OFFSET_FLOATS: u32 = 20;
pub const VERTEX_SPLAT1_OFFSET_FLOATS: u32 = 21;

// Render-layer and analytic fog-shape ids are enum-backed GPU discriminants.
pub const RENDER_LAYER_ARCHITECTURE: u32 =
    byroredux_core::ecs::components::RenderLayer::Architecture as u32;
pub const RENDER_LAYER_CLUTTER: u32 = byroredux_core::ecs::components::RenderLayer::Clutter as u32;
pub const RENDER_LAYER_ACTOR: u32 = byroredux_core::ecs::components::RenderLayer::Actor as u32;
pub const RENDER_LAYER_DECAL: u32 = byroredux_core::ecs::components::RenderLayer::Decal as u32;
pub const FOG_VOLUME_SHAPE_SPHERE: u32 =
    byroredux_core::ecs::components::FogShape::Sphere as u32;
pub const FOG_VOLUME_SHAPE_ELLIPSOID: u32 =
    byroredux_core::ecs::components::FogShape::Ellipsoid as u32;
pub const FOG_VOLUME_SHAPE_BOX: u32 =
    byroredux_core::ecs::components::FogShape::Box as u32;
pub const FOG_VOLUME_SHAPE_CONE: u32 =
    byroredux_core::ecs::components::FogShape::Cone as u32;

// Exterior LAND terrain grid (#4052). Re-exported from
// `byroredux_core::math::coord` rather than retyped: the ground-cover scatter
// pass locates a cell's terrain vertices in the global vertex SSBO by
// inverting the same mapping `cell_loader/terrain.rs` used to build them, and
// a GLSL copy that drifted from the Rust one would not fail any test — it
// would silently sample a neighbouring vertex.
pub const LAND_GRID_VERTS: u32 = byroredux_core::math::coord::LAND_GRID_VERTS as u32;
pub const LAND_VERTEX_SPACING: f32 = byroredux_core::math::coord::LAND_VERTEX_SPACING;
pub const EXTERIOR_CELL_UNITS: f32 = byroredux_core::math::coord::EXTERIOR_CELL_UNITS;
/// §12.3 ground-colour coupling (#4056): the blade vertex shader rebuilds
/// the terrain diffuse UV from world position with this factor, the same
/// factor `cell_loader/terrain.rs` authored the terrain vertices' UV with.
/// A drift would sample the grass's ground colour from the wrong texel —
/// visible only as a subtle colour mismatch against the terrain itself.
pub const LAND_TEXTURE_TILES_PER_CELL: f32 =
    byroredux_core::math::coord::LAND_TEXTURE_TILES_PER_CELL;

// Ground-cover chunking + the §11.1 sampling bench (#4052).
//
// `exal-groundcover.md` §4 makes the 512-unit chunk the unit of dispatch,
// culling and LOD selection — an 8×8 grid per exterior cell. The bench
// dispatches one workgroup per chunk for exactly that reason: a harness that
// batched samples some other way would not price the per-chunk setup the real
// scatter pays (chunk record fetch, cell-origin load, sequence scramble).
pub const GROUNDCOVER_CHUNK_UNITS: f32 = 512.0;

/// Width and per-species row height of the generated Tier-3 ground-cover
/// detail atlas. The #4056 plan requires a real per-species texture floor;
/// this compact resolution is a colour/detail lookup, while `d_ground`
/// supplies the non-repeating world-space density variation.
pub const GROUNDCOVER_DETAIL_ATLAS_EDGE: u32 = 16;
/// Terrain-normal perturbation of Tier-3's generated detail atlas. This is an
/// uncited visual calibration required by #4056; it is listed in
/// `exal-groundcover.md` §12.12 rather than hidden in GLSL.
pub const GROUNDCOVER_DETAIL_NORMAL_STRENGTH: f32 = 0.15;
pub const GROUNDCOVER_CHUNKS_PER_CELL_SIDE: u32 = 8;
/// No `u` suffix in GLSL — used in `layout(local_size_x = ...)`.
pub const GROUNDCOVER_BENCH_WORKGROUP: u32 = 64;
/// Vertices per blade in the raster half of the bench. §4's blade is a
/// quadratic Bezier ribbon whose near tier is 3 segments, i.e. 4 rings of 2
/// vertices. The raster consumer's whole point is that it samples once per
/// vertex per blade, so this factor is what separates its per-sample cost from
/// the scatter's.
pub const GROUNDCOVER_BENCH_BLADE_VERTS: u32 = 8;

// ── Ground-cover density field + scatter (§3/§4/§6, #4054) ──────────────
//
// Every *parameter* of the density field is canonical here and reaches GLSL
// through the generated header; the *formula* exists only in
// `include/groundcover_density.glsl`. §3 is explicit about why: a Rust mirror
// would be a second source of truth for something that must match exactly, and
// this codebase has already been bitten by that shape
// (`GpuInstance` across four shader files). Rust owns the numbers, GLSL owns
// the arithmetic, and there is exactly one copy of each.
//
// The cost, accepted in §3, is that the field is not unit-testable in the
// usual sense. It is pinned instead by the `groundcover.hist` density
// histogram over real cells (§11.3) — which is also how the values below get
// calibrated, since several are starting points rather than derived constants.

// #5177 removed `GROUNDCOVER_DEFAULT_AFFINITY` here (the fabricated default
// the pre-#4903 shader substituted for the unpainted base): no GLSL reads
// it, the contract test pins its absence, and the base's real affinity
// rides `GpuTerrainTile::base_cover_affinity` instead.

/// Sentinel water height meaning "no water plane in this cell"; see
/// `byroGcMoisture`, whose no-water path returns 1.0.
pub const GROUNDCOVER_NO_WATER: f32 = byroredux_core::ecs::components::groundcover::NO_WATER_HEIGHT;

/// §12.3 (#4056) — `GpuGroundCoverCell::terrain_tile_slot` sentinel: the
/// cell has no splat terrain, so the blade fragment skips the ground-colour
/// coupling. A valid slot index can be 0, which is why "absent" is not 0.
pub const GROUNDCOVER_NO_TERRAIN_TILE: u32 =
    byroredux_core::ecs::components::groundcover::GROUNDCOVER_NO_TERRAIN_TILE;

/// `slope_gate` — terrain `normal.y` below which no ground cover grows, and
/// above which it is unattenuated. Smoothstepped between.
///
/// Not arbitrary: these are the cosines of the soil angle of repose. Loose
/// soil holds a slope to roughly 30–45° depending on moisture and grain, and
/// beyond that it sheds rather than accumulating; vegetation follows the soil.
/// `cos(45°) = 0.707`, `cos(30°) = 0.866`. This term alone removes the single
/// most artificial thing about vanilla grass, which carpets cliff faces
/// wherever the texture happened to be painted.
pub const GROUNDCOVER_SLOPE_GATE_START: f32 = 0.707;
pub const GROUNDCOVER_SLOPE_GATE_FULL: f32 = 0.866;

/// `moisture` — how far above the water plane the riparian bonus decays over,
/// and the floor it decays to.
///
/// **The floor is the important number.** §3: a cell with no water plane must
/// resolve this term to 1.0, not 0.0 — in a pure product one undefined factor
/// takes the whole field, and the failure mode is an entire worldspace with no
/// ground cover and nothing in the log. So the shader's no-water path returns
/// 1.0 outright, and this floor applies only to ground that *does* have a
/// water plane and sits well above it.
///
/// Calibration starting point (§11.3). 1200 units is a little under a third of
/// an exterior cell (4096) — far enough that a lake shore reads as lusher than
/// the ridge behind it, close enough that the whole map is not one gradient.
pub const GROUNDCOVER_MOISTURE_FALLOFF_UNITS: f32 = 1200.0;
pub const GROUNDCOVER_MOISTURE_FLOOR: f32 = 0.55;
/// Below the water plane, cover stops. Submerged ground is not a shoreline.
pub const GROUNDCOVER_MOISTURE_SUBMERGED_DEPTH: f32 = 24.0;

/// `shelter` — the discrete-Laplacian curvature term. `SCALE` converts the
/// raw second difference (world units over the sample stencil) into the
/// [-1, 1] range the gate consumes; `STRENGTH` is how much of the result is
/// allowed to modulate density, so 0 disables the term without removing it.
///
/// Calibration starting points (§11.3).
pub const GROUNDCOVER_SHELTER_CURVATURE_SCALE: f32 = 0.006;
pub const GROUNDCOVER_SHELTER_STRENGTH: f32 = 0.35;

/// `clump` — the organic term, and the load-bearing one.
///
/// §3 is emphatic: the splat authority is a 17×17 alpha grid per 2048-unit
/// quadrant, so bilinear sampling fixes the hard step but cannot manufacture
/// detail below ~128 units. Noise is the **only** term in the product with
/// authority above that frequency. A build that stubs this to 1.0 "for now"
/// reproduces the vanilla patch look exactly and looks like the design failed.
///
/// Two octaves per §3: a Worley/cellular field at ~600 units for clump
/// structure, times a low-amplitude fBm at ~4000 units for regional variation.
/// Both frequencies are the design's own numbers.
pub const GROUNDCOVER_CLUMP_CELL_UNITS: f32 = 600.0;
/// Contrast applied to the Worley field. Above 1.0 tightens clumps; the floor
/// keeps the gaps between them thin rather than bare, so clumping reads as
/// density variation rather than as holes.
pub const GROUNDCOVER_CLUMP_CONTRAST: f32 = 1.4;
pub const GROUNDCOVER_CLUMP_FLOOR: f32 = 0.18;
pub const GROUNDCOVER_REGION_UNITS: f32 = 4000.0;
/// Amplitude of the regional fBm, as a fraction. 0.35 means regional variation
/// moves density within [0.65, 1.0] — visible as one meadow being richer than
/// the next, never as a hard regional boundary.
pub const GROUNDCOVER_REGION_AMPLITUDE: f32 = 0.35;
/// Mean of the raw clump field (`byroGcClumpRaw`: Worley cells x regional fBm).
/// MEASURED, not chosen: a Rust port of the noise averages 0.468 over two
/// 54 km x 54 km areas 3e6 units apart, and the
/// `the_clump_mean_constant_matches_the_measured_field` test fails if a clump
/// constant is retuned without updating this. The shader divides the raw field
/// by it, so clumping REDISTRIBUTES the authored density without changing its
/// average (until now the raw field was multiplied in as-is, silently thinning
/// every sward to ~47% of its authored density).
pub const GROUNDCOVER_CLUMP_MEAN: f32 = 0.468;
/// Per-clump variation, drawn from the id of the clump's nearest Worley feature
/// (so a clump is internally coherent and neighbouring clumps differ): blade
/// height varies by +-`HEIGHT_VARIATION` and colour is shifted warm/cool by
/// `TINT_VARIATION`. Art direction, recorded as such.
pub const GROUNDCOVER_CLUMP_HEIGHT_VARIATION: f32 = 0.20;
pub const GROUNDCOVER_CLUMP_TINT_VARIATION: f32 = 0.08;

/// `distance_fade` — tier 0's outer range and where the fade begins (§6).
///
/// **This is the only term `d_draw` adds over `d_ground`, and the only one the
/// accept/reject test may see.** Everything downstream reads `d_ground`; §3
/// spells out why (a view-faded value makes the shadow under a meadow lighten
/// as the camera retreats).
// WTHR cloud layers (#4230). A cloud sprite's UV tile scale is derived on
// the host as `baseline * CLOUD_REF_WIDTH / authored_width`, which makes its
// *texel* frequency `baseline * CLOUD_REF_WIDTH` whatever the sprite's size.
// `sky.glsl`'s analytic cloud mip was tuned against layer 0's baseline at the
// reference width, so the shader needs both numbers to give the other three
// layers the mip offset their finer decks call for. Both previously lived as
// private constants in `byroredux/src/scene/world_setup.rs`, where the shader
// could not see them.
pub const CLOUD_REF_WIDTH: f32 = 512.0;
pub const CLOUD_TILE_SCALE_LAYER_0: f32 = 0.15;

// The grass fade follows the games' own defaults: Skyrim's shipped
// `fGrassStartFadeDistance` (SkyrimPrefs / Ultra / High) and its
// `fGrassMaxStartFadeDistance` are 7000, and the fade range is
// `fGrassFadeRange` = 1000 (Fallout 3 / New Vegas `Fallout_default.ini`;
// Skyrim's own INI does not list it, so the sibling title's value is used).
// Every game gets Skyrim's distances: fade starts at 7000, gone at 8000.
// Only chunks in front of the camera are ever resident (a view wedge, see
// `render/groundcover.rs`), which is what makes this reach affordable: the
// ring holds `GROUNDCOVER_MAX_CHUNKS` wedge chunks, not a full disc of them.
pub const GROUNDCOVER_FADE_START: f32 = 7000.0;
pub const GROUNDCOVER_DRAW_DISTANCE: f32 = 8000.0;

/// Scatter dispatch shape. One workgroup per visible chunk, mirroring
/// `cluster_cull.comp` (§4).
pub const GROUNDCOVER_SCATTER_WORKGROUP: u32 = 64;
/// Candidate points each scatter thread draws. Total candidates per chunk is
/// `GROUNDCOVER_SCATTER_WORKGROUP × this` — 16,384 over a 512-unit chunk, one
/// candidate per 4 units (≈6 cm).
///
/// Raised from 64 (4,096 per chunk) on 2026-09-16 after the Skyrim exterior
/// capture at `2,-4` still read as isolated sprouts: 15% acceptance yielded
/// roughly 9 rendered blades/m². The limiter was not the cap, so scaling the
/// candidate budget retains every terrain-aware relative term — worn ground
/// stays thinner than meadow — while lifting the observed density to roughly
/// 36 blades/m², close to Outerra's ≈44 blades/m² full-detail reference.
/// See design §12.13.
pub const GROUNDCOVER_CANDIDATES_PER_THREAD: u32 = 256;
/// Fixed capacity of a chunk's blade slice.
///
/// §4: **this can overflow, and the overflow policy is part of the design.** A
/// chunk on rich flat ground at full density will hit the cap, the atomic
/// append must saturate rather than wrap, and the frame must not depend on
/// which threads won the race. That is what forces the progressive
/// low-discrepancy sequence — a blue-noise tile consumed in order is not
/// progressive, so truncating it leaves whatever the first N entries happen to
/// be, which clumps directionally in exactly the densest chunks.
///
/// Equal to the candidate count per chunk, so a chunk whose every candidate is
/// accepted never overflows; overflow can then only come from the cap being
/// lowered, never from the field. Derived rather than restated (#4377), so the
/// two tuning constants above cannot be edited out of step with it.
pub const GROUNDCOVER_MAX_BLADES_PER_CHUNK: u32 =
    GROUNDCOVER_SCATTER_WORKGROUP * GROUNDCOVER_CANDIDATES_PER_THREAD;
/// Ceiling on chunks resident in one frame: the chunks of the VIEW WEDGE, not a
/// disc. Only chunks in front of the camera (the horizontal field of view plus
/// a cache margin, capped at `VIEW_MAX_HALF_ANGLE`) and a small all-round disc
/// are ever resident, so the 8000-unit reach needs a sector of ~475 chunks at
/// the widest wedge the binary allows (a 180-degree sector) rather than the ~830 a full disc would —
/// bounded by the binary's `chunk_cap_covers_every_chunk_in_reach`. 512 holds
/// the blade buffer at `512 x 16,384 x 16 B` = 128 MiB (it was 64 MiB for a
/// 3000-unit disc of 256 slots).
///
/// The headroom is enforced rather than argued: the binary's
/// `chunk_cap_covers_every_chunk_in_reach` bounds the wedge from this file's
/// chunk size and draw distance, so a retune that outgrows the cap fails a test
/// instead of silently dropping chunks at runtime (#4338).
pub const GROUNDCOVER_MAX_CHUNKS: u32 = 512;
/// Density-histogram buckets (§11.3). The scatter tallies `d_ground` per
/// candidate so the field can be calibrated against real cells rather than
/// against a mirrored Rust function that would not be the thing shipping.
pub const GROUNDCOVER_HISTOGRAM_BUCKETS: u32 = 16;

// ── Blade geometry + wind (§4 "Blade geometry", §8, #4055) ──────────────

/// Segments in a tier-0 (near) blade. §4: a quadratic Bezier ribbon — base
/// point, a control point displaced by the bend, and a tip — drawn as one
/// triangle strip of `2N + 1` vertices, listed as `2N - 1` triangles
/// (`GROUNDCOVER_LIST_VERTS`). Three segments is the 7-vertex near blade.
pub const GROUNDCOVER_BLADE_SEGMENTS_NEAR: u32 = 3;
/// Segment-LOD tiers (near / mid / far) — one indirect stream each.
pub const GROUNDCOVER_LOD_TIERS: u32 = 3;
/// Segments in a tier-1 (mid) blade: the 5-vertex strip.
pub const GROUNDCOVER_BLADE_SEGMENTS_MID: u32 = 2;
/// Segments in a tier-2 (far) blade: one triangle, 3 vertices.
pub const GROUNDCOVER_BLADE_SEGMENTS_FAR: u32 = 1;
/// Blades per scatter point at each tier. The roots are the same fixed slab at
/// every tier; the far tiers keep a prefix of the near tuft (blades 0..k, which
/// share seeds with the near blades) and the vertex shader widens the survivors
/// so ground coverage is conserved (see `groundcover_lod.glsl`).
pub const GROUNDCOVER_BLADES_PER_POINT_MID: u32 = 2;
pub const GROUNDCOVER_BLADES_PER_POINT_FAR: u32 = 1;
/// Distance bands of the segment LOD, from the camera to the CHUNK centre (the
/// LOD unit is the 512-unit chunk, so a whole chunk changes level and morphs
/// together). Near blades (3 segments) up to `NEAR_END`, mid (2) up to
/// `MID_END`, far (1) beyond. Across the last `MORPH_UNITS` before each edge
/// the outgoing level's intermediate vertices slide onto the next level's
/// polyline, so the level switch (at exactly the edge) changes nothing visible.
///
/// Art direction, not sourced values: chosen so that a ~10-unit blade (the
/// arid/temperate palette heights are 4-14) is still ~15 px tall at the near
/// edge and ~7 px at the mid edge for a 1080p, 60-degree view, below which a
/// third/second segment stops being resolvable. Distance-keyed by design: the
/// level is a vertex-count decision, and the resolution-dependent anti-aliasing
/// floor stays pixel-keyed in the blade shader.
pub const GROUNDCOVER_LOD_NEAR_END: f32 = 600.0;
pub const GROUNDCOVER_LOD_MID_END: f32 = 1400.0;
pub const GROUNDCOVER_LOD_MORPH_UNITS: f32 = 200.0;
/// Where the blade tier's density fade starts. The blade tier's thinning is
/// paid back by widening the survivors (`GROUNDCOVER_FADE_COVERAGE_MAX_GAIN`),
/// so coverage stays level instead of stepping at one hard radius (the "bright
/// dense band" where density dropped while the blades' pixel-floor widening
/// kept growing their footprint); the authored-model tier's fade is not repaid.
/// It used to start well inside the draw distance (3000 against the model
/// tier's 7000). Since the reach change (`b731b01f5`) both tiers take Skyrim's
/// 7000-to-8000 fade, so this is the model tier's start, kept as its own knob
/// for the blade tier's compensated fade (#6030).
pub const GROUNDCOVER_BLADE_FADE_START: f32 = GROUNDCOVER_FADE_START;
const _: () = {
    assert!(
        GROUNDCOVER_FADE_START < GROUNDCOVER_DRAW_DISTANCE,
        "ground-cover density fade must start before its draw-distance end"
    );
    assert!(
        GROUNDCOVER_BLADE_FADE_START < GROUNDCOVER_DRAW_DISTANCE,
        "ground-cover blade fade must start before its draw-distance end"
    );
};
/// Largest widening a blade takes to pay back distance thinning, and the
/// largest total widening (LOD blade-count reduction x thinning) a blade may
/// take; coverage falls off gracefully past either (the terrain's own detail
/// layer carries the colour there).
pub const GROUNDCOVER_FADE_COVERAGE_MAX_GAIN: f32 = 3.0;
pub const GROUNDCOVER_COVERAGE_MAX_WIDEN: f32 = 6.0;
/// Where the quadratic Bézier's control point sits along the blade's UP axis:
/// half-way to the tip. This is geometry, not a visual tuning knob.
pub const GROUNDCOVER_BEZIER_CONTROL_FRACTION: f32 = 0.5;
/// How much of the tip's horizontal bend the control point carries, per blade
/// (min..max by seed). The control point of a straight blade carries exactly
/// half (it is the chord midpoint) — which is what every blade was before: a
/// rigid stick tilted about its root. A smaller carry keeps the blade vertical
/// at the root and lets it bow progressively toward its leaning tip (the offset
/// at parameter t is `t² · bend` at carry 0, `t · bend` at carry 0.5), the
/// curvature a real blade has. The range is art direction, recorded as such;
/// the spread gives neighbouring blades different arcs.
pub const GROUNDCOVER_BEZIER_BEND_CARRY_MIN: f32 = 0.05;
pub const GROUNDCOVER_BEZIER_BEND_CARRY_MAX: f32 = 0.30;
/// Numerical guards for zero-length blade/wind vectors; not artistic tuning.
pub const GROUNDCOVER_BLADE_VECTOR_EPSILON: f32 = 1.0e-4;
/// #5961 — `multiLayerRefractionScale` drives the view-dependent UV
/// offset of the inner layer's sample. Vanilla authors O(1) values
/// (0.5–1.2 across the Skyrim census), but no shipped reader pins the
/// transfer function, so this factor is the interpretive neutral — 1.0
/// of authored scale at a face-on-ish view offsets the inner layer by
/// ~this fraction of a UV, scaled further by the authored thickness.
/// 0 stays exactly the unoffset sample.
pub const MLP_REFRACTION_UV_PER_UNIT: f32 = 0.1;
pub const GROUNDCOVER_PROJECTED_DEPTH_EPSILON: f32 = 1.0e-3;
/// Blades the vertex shader grows from each accepted scatter point.
///
/// Outerra, "Procedural grass rendering" (2012): "At the most detailed level
/// there are 4 blades generated from a single point in the canopy texture",
/// over canopy data of "roughly 30cm" resolution
/// (<https://outerra.blogspot.com/2012/05/procedural-grass-rendering.html>).
/// The blades share the point's root and species and differ by their own seed
/// streams, multiplying the sward without adding scatter work. When this landed
/// the candidate grid matched Outerra's scale (512 / √1024 = 16 units ≈ 23 cm);
/// since the candidate budget rose (see `GROUNDCOVER_CANDIDATES_PER_THREAD`) the
/// grid is finer, and the justification is the resulting density, not the
/// spacing.
pub const GROUNDCOVER_BLADES_PER_POINT: u32 = 4;
/// Entries in the scatter's species selection table.
///
/// The scatter's per-candidate hash spends its low 24 bits on the accept test
/// and the blade seed, and its top 8 bits on the species. A table this size
/// maps those 8 bits straight to a species index, so selection by
/// `GroundCoverSpecies::climate_weight` (§7) costs one lookup and draws each
/// species with probability `buckets / 256`. The host fills it by
/// largest-remainder rounding with one bucket reserved for every species of
/// non-zero weight, so quantisation cannot silently drop a species the
/// palette says belongs here.
pub const GROUNDCOVER_SPECIES_TABLE_SIZE: u32 = 256;

// ── Authored-model tier (§12.12 Phase C, #4413) ─────────────────────────

/// Threads per authored-model workgroup. One workgroup per chunk in the
/// placement and emit phases.
pub const GROUNDCOVER_MODEL_WORKGROUP: u32 = 64;
/// Candidate slots per chunk in the placement slab. A chunk's candidate grid
/// is `ceil(GROUNDCOVER_CHUNK_UNITS / spacing)²`; the densest measured
/// spacing (`iMinGrassSize` 20, Skyrim SE and FO4) gives 26² = 676, so every
/// vanilla grid fits without truncation. Pinned by the binary's
/// `model_slab_holds_every_vanilla_candidate_grid`.
pub const GROUNDCOVER_MODEL_POINTS_PER_CHUNK: u32 = 1024;
/// Records one worldspace's tier can carry. Oblivion's 108 `GRAS` records
/// are the largest vanilla set.
pub const GROUNDCOVER_MODEL_MAX_RECORDS: u32 = 128;
/// Shapes across every record's model.
pub const GROUNDCOVER_MODEL_MAX_SHAPES: u32 = 256;
/// Instance slots the tier appends after the main instance list each frame.
/// An engine budget, not a game value: past it the layout phase grants every
/// record the same share of its placed plants, whole plants only (#4920), so
/// no record loses all of its instances and no plant loses one of its shapes.
pub const GROUNDCOVER_MODEL_MAX_INSTANCES: u32 = 32768;
/// The model shader's three phases, selected by push constant (see
/// `groundcover_models.comp`'s header).
pub const GROUNDCOVER_MODEL_PHASE_PLACE: u32 = 0;
pub const GROUNDCOVER_MODEL_PHASE_LAYOUT: u32 = 1;
pub const GROUNDCOVER_MODEL_PHASE_EMIT: u32 = 2;
/// Model-tier push-constant `flags` bit: the host has folded the authored
/// `GRAS` densities into the §3 affinity lanes (`AuthoredCover::localised`), and
/// the pick's lane-weighted union (`GcLocalPick.accept`) is that same value, so
/// the placement field must not multiply the affinity in a second time (#6027).
pub const GROUNDCOVER_MODEL_FLAG_AFFINITY_IN_PICK: u32 = 1;
/// Record flag bits: scale all three axes / tilt onto the terrain normal.
pub const GROUNDCOVER_MODEL_RECORD_FLAG_UNIFORM_SCALING: u32 = 1;
pub const GROUNDCOVER_MODEL_RECORD_FLAG_FIT_TO_SLOPE: u32 = 2;
/// Shape flag bit: the shape's own local transform scales non-uniformly.
pub const GROUNDCOVER_MODEL_SHAPE_FLAG_NON_UNIFORM: u32 = 1;
/// Low bits of `GcModelPoint.meta.x` that hold the record index; the candidate
/// index sits above them (`record | (candidate << RECORD_BITS)`).
pub const GROUNDCOVER_MODEL_RECORD_BITS: u32 = 8;
// #4923 — the packing only round-trips while both halves fit their bits.
const _: () = assert!(
    GROUNDCOVER_MODEL_MAX_RECORDS <= 1 << GROUNDCOVER_MODEL_RECORD_BITS,
    "every record index must fit GcModelPoint.meta.x's record bits"
);
const _: () = assert!(
    (GROUNDCOVER_MODEL_POINTS_PER_CHUNK as u64) << GROUNDCOVER_MODEL_RECORD_BITS <= 1 << 32,
    "every candidate index must fit above the record bits of meta.x"
);
/// `CoverWaterRule::gpu_code` values `gcWaterAdmits` switches on (#4923). The
/// binary's `CoverWaterRule` is the authority; the renderer test
/// `water_rule_codes_match_the_translated_rule` holds these to it.
pub const GROUNDCOVER_WATER_RULE_ABOVE_AT_LEAST: u32 = 0;
pub const GROUNDCOVER_WATER_RULE_ABOVE_AT_MOST: u32 = 1;
pub const GROUNDCOVER_WATER_RULE_BELOW_AT_LEAST: u32 = 2;
pub const GROUNDCOVER_WATER_RULE_BELOW_AT_MOST: u32 = 3;
pub const GROUNDCOVER_WATER_RULE_EITHER_AT_LEAST: u32 = 4;
pub const GROUNDCOVER_WATER_RULE_EITHER_AT_MOST: u32 = 5;
/// Salt separating the model candidate stream from the blade stream: both
/// derive from a chunk's seed, and must not place plants at blade roots.
pub const GROUNDCOVER_MODEL_SEED_SALT: u32 = 0x6D0D_E15A;
/// `gcCounts` layout: per-(chunk, record) counts, then their per-record
/// bases, then per-chunk totals, then per-shape (base, count) pairs, then
/// the stats words (demanded, emitted, 2 reserved).
pub const GROUNDCOVER_MODEL_COUNT_REGION: u32 =
    GROUNDCOVER_MAX_CHUNKS * GROUNDCOVER_MODEL_MAX_RECORDS;
pub const GROUNDCOVER_MODEL_BASE_REGION: u32 = GROUNDCOVER_MODEL_COUNT_REGION;
pub const GROUNDCOVER_MODEL_CHUNK_TOTAL_REGION: u32 = 2 * GROUNDCOVER_MODEL_COUNT_REGION;
pub const GROUNDCOVER_MODEL_SHAPE_REGION: u32 =
    GROUNDCOVER_MODEL_CHUNK_TOTAL_REGION + GROUNDCOVER_MAX_CHUNKS;
pub const GROUNDCOVER_MODEL_STATS_REGION: u32 =
    GROUNDCOVER_MODEL_SHAPE_REGION + 2 * GROUNDCOVER_MODEL_MAX_SHAPES;
pub const GROUNDCOVER_MODEL_STATS_WORDS: u32 = 4;

// ── Authored-model selection buffer (#4906) ─────────────────────────────

/// Resident terrain cells the ground-cover cell table can hold. Mirrors the
/// #4052 bench's cap: `--radius 3` resides 49 cells and this leaves room for
/// a wider ring without sizing every buffer for one nobody runs.
pub const GROUNDCOVER_MAX_CELLS: u32 = 128;
/// Lanes a cell's authored-cover masks carry: the BTXT base, then each splat
/// layer in the vertex splat lanes' order. Lane 0 is the base; lane `i + 1`
/// is splat layer `i`.
pub const GROUNDCOVER_MODEL_COVER_LANES: u32 = TERRAIN_SPLAT_LAYERS + 1;
/// 32-bit words of one lane's record bitset: bit `r` names record `r`.
pub const GROUNDCOVER_MODEL_COVER_MASK_WORDS: u32 = GROUNDCOVER_MODEL_MAX_RECORDS / 32;
/// Words one cell takes in the selection buffer's cover region.
pub const GROUNDCOVER_MODEL_COVER_CELL_WORDS: u32 =
    GROUNDCOVER_MODEL_COVER_LANES * GROUNDCOVER_MODEL_COVER_MASK_WORDS;
/// `gcSelection` layout: [`GROUNDCOVER_MODEL_MAX_RECORDS`] per-record weights
/// (`f32` bits: climate × authored density, 0 for a record that cannot place),
/// then the cover masks, `GROUNDCOVER_MODEL_COVER_CELL_WORDS` per resident
/// cell in `gcCells[]` order.
pub const GROUNDCOVER_MODEL_COVER_REGION: u32 = GROUNDCOVER_MODEL_MAX_RECORDS;
pub const GROUNDCOVER_MODEL_SELECTION_WORDS: u32 = GROUNDCOVER_MODEL_COVER_REGION
    + GROUNDCOVER_MAX_CELLS * GROUNDCOVER_MODEL_COVER_CELL_WORDS;
const _: () = assert!(
    GROUNDCOVER_MODEL_MAX_RECORDS.is_multiple_of(32),
    "the cover masks bitset the records in whole 32-bit words"
);

/// Wavelength of the wind flow-noise field, world units.
///
/// §8's requirement is that neighbouring blades sample a *continuous* field at
/// nearby points so they bend together — travelling gust waves across a meadow
/// rather than per-blade jitter, which is most of what sells grass as alive.
/// 1500 units is ~12 m: a gust front wide enough to read as weather crossing
/// the field, narrow enough that a meadow shows more than one at a time.
pub const GROUNDCOVER_WIND_NOISE_UNITS: f32 = 1500.0;
/// Maximum bend as a fraction of blade height, at full wind and zero
/// stiffness. Past ~0.8 the Bezier folds back through itself and the ribbon
/// self-intersects, which reads as flickering rather than as wind.
pub const GROUNDCOVER_WIND_MAX_BEND: f32 = 0.7;
/// Speed the wind field is advected downwind at, as a multiple of
/// `WindField::speed`. Gust *fronts* travel faster than the air; a factor of 1
/// makes the pattern appear frozen relative to the air it describes.
pub const GROUNDCOVER_WIND_ADVECTION_SCALE: f32 = 1.6;
/// The secondary wind mode's non-integer frequency ratio.
///
/// The ground-cover credibility track's Step 6 calls for a second mode at
/// `2.17×` the fundamental: an integer multiple makes the complete pose loop
/// visibly, while this ratio breaks that short repetition without adding a
/// second clock or a per-vertex noise sample.
pub const GROUNDCOVER_WIND_HARMONIC_FREQUENCY_MULTIPLIER: f32 = 2.17;
/// Secondary-mode and lateral-sway weight, relative to the downwind bend.
///
/// Step 6 specifies a lateral component of about a third of downwind motion;
/// using the same one-third contribution for the smaller harmonic keeps both
/// additions bounded under the existing maximum-bend contract.
pub const GROUNDCOVER_WIND_SECONDARY_AMPLITUDE: f32 = 1.0 / 3.0;
pub const GROUNDCOVER_WIND_LATERAL_FRACTION: f32 = 1.0 / 3.0;
/// `WindField::speed`'s ceiling — the denominator that turns it into a [0,1]
/// bend fraction. Re-exported so the shader and the translate boundary cannot
/// disagree about what "full wind" is.
pub const GROUNDCOVER_MAX_WIND_SPEED: f32 =
    byroredux_core::ecs::components::groundcover::MAX_WIND_SPEED;

/// Full turn in radians. Mathematical identity, named here so the seed-derived
/// yaw and wind phase share one exact conversion.
pub const GROUNDCOVER_TWO_PI: f32 = std::f32::consts::TAU;
/// Legacy wind-flow floor, rest-lean range, and stiffness attenuation.
///
/// These are the pre-#4378 shipping blade-shape values, named rather than
/// retuned. They remain uncited empirical shaping inputs; EXAL §12.12 records
/// that debt until Phase B replaces the engine palette with sourced dimensions.
pub const GROUNDCOVER_WIND_FLOW_FLOOR: f32 = 0.35;
pub const GROUNDCOVER_WIND_STIFFNESS_ATTENUATION: f32 = 0.7;
pub const GROUNDCOVER_REST_LEAN_BASE: f32 = 0.12;
pub const GROUNDCOVER_REST_LEAN_VARIATION: f32 = 0.10;
/// Terrain-normal blend used when a ribbon grows from a sloped triangle.
/// Migrated unchanged from the original shader literal (#4378); it is an
/// uncited compatibility value tracked in EXAL §12.12.
pub const GROUNDCOVER_TERRAIN_NORMAL_WEIGHT: f32 = 0.75;
/// Maximum seed-derived ribbon twist, in radians along a blade's length.
/// Migrated unchanged from the original shader literal (#4378); uncited as
/// recorded in EXAL §12.12.
pub const GROUNDCOVER_TWIST_RADIANS: f32 = 1.4;
/// Screen-space ribbon floor and cap for sub-pixel blades (§6).
/// These existing, uncited anti-aliasing shape values are named by #4378 and
/// recorded in EXAL §12.12; this change deliberately does not tune them.
pub const GROUNDCOVER_MIN_VISIBLE_HALF_WIDTH_PIXELS: f32 = 0.5;
pub const GROUNDCOVER_MAX_WIDTH_MULTIPLIER: f32 = 4.0;
/// Per-blade colour-jitter range. Existing visual variation, named without a
/// retune by #4378; its empirical status is recorded in EXAL §12.12.
pub const GROUNDCOVER_COLOUR_JITTER_MIN: f32 = 0.82;
pub const GROUNDCOVER_COLOUR_JITTER_MAX: f32 = 1.18;
/// Luminance anchor of a blade's albedo to the terrain it grows from (#5719's
/// grass overhaul, lighting phase). Measured cause of "blades glow": the arid
/// and temperate palettes carry *linear* albedos of 0.3-0.5 while the splat
/// terrain under them is a dark 0.05-0.15, and composite lights both with the
/// same terrain GI (`indirect * outAlbedo`), so a blade returned 3-6x the
/// terrain's radiance in sun and in shadow alike. The blade's HUE still comes
/// from the species palette; its LUMINANCE is the terrain's own at the root,
/// times these ratios (base, tip). They are art-direction defaults, not
/// sourced values: a blade is not darker than its soil at the root (sheath
/// shading is §12.1's job) and rises a little toward the sky-facing tip.
pub const GROUNDCOVER_BASE_LUMA_RATIO: f32 = 0.75;
pub const GROUNDCOVER_TIP_LUMA_RATIO: f32 = 1.10;
/// Bound on the anchor's brightening gain (a near-black ground sample must not
/// blow a palette hue up to white).
pub const GROUNDCOVER_LUMA_ANCHOR_MAX_GAIN: f32 = 4.0;

// ── The light response (§12.1, §12.2, §12.5, §12.6; #4057) ──────────────
//
// Every term here is analytic — no ray, no probe, no bake step — and every
// number below is a physical quantity with a source, not a knob. §11.6, §11.7,
// §11.9 and §11.10 asked "what should this scalar be"; the answer they got is
// that three of the four are not free scalars at all once the canopy is
// modelled as what it is: a participating slab of leaves.

/// Beer–Lambert extinction coefficient `K` for a canopy of randomly oriented
/// leaves — the classic spherical leaf-angle-distribution value from
/// Monsi & Saeki (1953), still the textbook constant (Campbell & Norman,
/// *Environmental Biophysics*).
///
/// Grass is erectophile rather than spherical, so its true `K` is a little
/// lower with the sun high and a little higher with it low. The spherical
/// value is the neutral middle of that swing, and §12.5's `1/cos θ` already
/// carries the dominant part of the angular dependence.
pub const GROUNDCOVER_CANOPY_EXTINCTION_K: f32 = 0.5;
/// Leaf area per unit of canopy depth, at `d_ground == 1.0`, in inverse
/// Gamebryo units.
///
/// A dense temperate sward has a leaf area index of 3–5; taking LAI = 4 over
/// the ~10-unit canopy the default temperate species stands up
/// (`height_range` 6–14) gives 0.4 per unit. Together with `K` above this
/// makes §12.5's whole coefficient `k = K × LAD = 0.2`, so a full-density
/// sward transmits `exp(−2) ≈ 0.14` of vertical sun to the ground — which is
/// the measured order for photosynthetically active radiation reaching a
/// dense pasture floor, and the reason §11.9's "the one number between grass
/// tints the ground and grass paints a black hole" is now two numbers that
/// each have a unit and a meaning.
pub const GROUNDCOVER_CANOPY_LEAF_AREA_DENSITY: f32 = 0.4;
/// Floor on `cos θ` in §12.5's `1 / max(cos θ, ε)` path-length term.
///
/// A numerical guard, not a physical claim: it caps the traversed canopy at
/// 10× vertical, which is a sun ~5.7° above the horizon. Below that the sun
/// is reddened and dimmed by the atmosphere anyway, while an uncapped
/// `1/cos θ` drives the transmittance to zero and paints the terrain black
/// through the last minutes of every sunset.
pub const GROUNDCOVER_CANOPY_MIN_COS: f32 = 0.1;
/// Diffusivity factor: the mean `1 / cos θ` of an isotropic hemisphere, used
/// to turn §12.5's directional extinction into §12.1's ambient one.
///
/// This is Elsasser's constant from two-stream radiative transfer (`5/3`),
/// and it is why §12.1 needs **no strength parameter of its own** — the
/// answer to §11.6. Ambient occlusion and canopy shadow are the same
/// extinction through the same slab, integrated over the sky instead of along
/// the sun; a second independent "occlusion strength" scalar would be a way
/// for the two to disagree about how thick the same grass is.
pub const GROUNDCOVER_SKY_DIFFUSIVITY: f32 = 1.6666666;

/// Extinction through a blade's own thickness (§12.2), per unit of blade
/// width.
///
/// A single green leaf transmits ~5–10% of visible light (chlorophyll absorbs
/// the blue and red bands hard; the near-infrared it passes freely is not
/// rendered). `exp(−2.5 × 1.0) ≈ 0.08` puts a mid-width blade in that band,
/// and because the shader feeds this the *tapered* width the tip transmits
/// more than the base for free — which is §11.7's answer: one transmission
/// colour per species is enough, the height variation real leaves show falls
/// out of the taper rather than needing its own authored gradient.
pub const GROUNDCOVER_BLADE_TRANSMISSION_EXTINCTION: f32 = 2.5;
/// Normal-bending applied to the light vector before the transmission lobe is
/// evaluated, and the lobe's exponent — the two parameters of the
/// Barré-Brisebois & Bouchard translucency approximation (GDC 2011,
/// "Approximating Translucency for a Fast, Cheap and Convincing
/// Subsurface-Scattering Look"), inside the ranges that paper recommends
/// (distortion 0.1–0.5, power 1–16).
pub const GROUNDCOVER_TRANSMISSION_DISTORTION: f32 = 0.2;
pub const GROUNDCOVER_TRANSMISSION_POWER: f32 = 4.0;

/// Normal-incidence Fresnel reflectance of the blade cuticle (§12.6).
///
/// Derived, not chosen: plant cuticular wax has a refractive index of ~1.45,
/// so `F0 = ((1.45 − 1) / (1.45 + 1))² = 0.034`. It is a species-independent
/// property of the wax, which is why `GroundCoverSpecies::sheen` scales the
/// lobe's *amount* rather than its index.
pub const GROUNDCOVER_SHEEN_F0: f32 = 0.034;
/// Roughness of the Charlie sheen lobe (Estevez & Kulla 2017, the
/// fabric/fibre model glTF's `KHR_materials_sheen` adopted).
///
/// §11.10 asked whether one sheen scalar per species suffices. For the dry
/// case it does, and this is why: the *shape* of the lobe is a property of
/// fibres — a ribbed, curved blade — and does not vary between grasses, while
/// the amount does and is per-species. Wetness would change both, and stays
/// deferred with §12.3's weather coupling exactly as §11.10 proposed.
pub const GROUNDCOVER_SHEEN_ROUGHNESS: f32 = 0.3;

// ── Interaction displacement field (§12.4, #4058) ───────────────────────
//
// Grass that does not move when something walks through it is static scenery,
// whatever else is right about it. The field is structurally the same object
// as §8's wind: a small world-space texture centred on the camera, sampled by
// the blade vertex shader at the blade base. The difference — and the part
// that is easy to get wrong — is that it is **stateful**: accumulated and
// decayed per frame rather than re-rendered from scratch, so a trail persists
// behind a runner and fades instead of snapping upright at the boundary.

/// Field resolution, texels per side.
///
/// With the extent below this is 8 world units per texel. A human capsule is
/// 36 units across (`CharacterController::HUMAN`), so a footfall covers ~4.5
/// texels — enough for the bilinear fetch in the blade shader to draw a
/// channel with shape, and coarse enough that neighbouring blades sample
/// almost the same value and therefore part *together*, which is the whole
/// point of using a field rather than a per-blade test.
pub const GROUNDCOVER_INTERACTION_TEXELS: u32 = 256;
/// World-space extent of the field, one side.
///
/// Half an exterior cell, ~29 m. Interaction is only legible at close range —
/// a blade is 6–14 units tall — so the field does not need the 2000-unit draw
/// distance, and covering it at this resolution would cost 64× the memory to
/// animate grass nobody can see move.
pub const GROUNDCOVER_INTERACTION_UNITS: f32 = 2048.0;
/// Half-life of a disturbance, seconds.
///
/// **The recovery is the part that is easy to get wrong.** Blades snapping
/// upright the instant an entity passes reads worse than no interaction at
/// all, because it draws the eye straight to the boundary. Real grass springs
/// most of the way back in a second or two after a light pass, and a trail
/// through a tall sward lingers longer; 1.5 s is inside that range and errs
/// toward the trail, which is the half that sells the effect.
pub const GROUNDCOVER_INTERACTION_HALF_LIFE_SECONDS: f32 = 1.5;
/// Maximum bend a fully-disturbed blade takes, as a fraction of its height.
///
/// 0.9 lays the blade nearly flat without letting the Bezier tip pass through
/// the ground, which a value at or above 1 would do on a downslope.
pub const GROUNDCOVER_INTERACTION_MAX_BEND: f32 = 0.9;
/// Disturbers the host uploads per frame, nearest first.
///
/// The field is 2048 units across; 64 actors inside it is already a crowd, and
/// the compute pass costs one loop iteration per texel per disturber.
pub const GROUNDCOVER_INTERACTION_MAX_DISTURBERS: u32 = 64;
/// Compute workgroup side for the field update. No `u` suffix — used in
/// `layout(local_size_x = ...)`.
pub const GROUNDCOVER_INTERACTION_WORKGROUP: u32 = 8;

// Skinning. #3882 — re-exported rather than restated: this file's whole
// purpose is that a shared constant has one definition, and ~40 of its
// entries already resolve through `byroredux_core::`. The survey that fixes
// the ceiling at 144 (FO76 prewardress = 133) lives with the declaration in
// `byroredux_core::ecs::components::skinned_mesh`. Editing core's value used
// to leave the generated shader header stale until the renderer's own test
// suite ran; now it cannot compile out of step.
pub const MAX_BONES_PER_MESH: u32 = byroredux_core::ecs::components::MAX_BONES_PER_MESH as u32;

// Skin-compute workgroup width. Both `skin_vertices.comp` (1 invocation per
// vertex) and `skin_palette.comp` (1 invocation per bone slot) run a 1D
// 64-wide dispatch; the Rust dispatch group-count math in
// `vulkan/skin_compute.rs` (`*.div_ceil(SKIN_WORKGROUP_SIZE)`) re-exports
// this same const so the layout qualifier and the group count can't drift.
// Distinct from `WORKGROUP_X = 8` (the 2D image-pass tile width) — skinning
// is a flat 1D dispatch. Emitted with no `u` suffix so it works in the
// `layout(local_size_x = SKIN_WORKGROUP_SIZE)` qualifier. #1758 / TD7-001.
pub const SKIN_WORKGROUP_SIZE: u32 = 64;

// Material kind enum (GpuMaterial.materialKind).
// Authoritative Rust-side values live in `scene_buffer/constants.rs`. #1401.
// MULTI_LAYER_PARALLAX is the one low-range (NIF-forwarded shader_type)
// value mirrored here too — see its doc comment in constants.rs (#2770 /
// REN-D1-03) for why it needs the same GLSL lockstep as the high-range
// engine-classified kinds below.
/// Decal slots per material — four named `GpuMaterial` fields
/// (`decalMap0Index`..`decalMap3Index`) built from the four decal indices,
/// matching `MaterialTextureSet::decals: [T; 4]` and the NIF importer's
/// `MAX_DECAL_SLOTS`. The three GLSL decal loops
/// (`ray_hit.glsl` ×2, `triangle.frag`) iterate `MATERIAL_DECAL_SLOTS` so
/// the count exists once per side of the shader boundary (#5848, the
/// #3879 lockstep class one level down).
pub const MATERIAL_DECAL_SLOTS: u32 = 4;
pub const MATERIAL_KIND_MULTI_LAYER_PARALLAX: u32 = 11;
/// BSLightingShaderProperty type 16 — the eye environment map (#5961).
/// The iris-centred reflection reads the material's authored cubemap
/// (`envMapIndex`, the shader's EnvMap texture slot) with the reflection
/// vector derived from the packed `eyeLeftCenter`/`eyeRightCenter` +
/// `eyeCubemapScale`.
pub const MATERIAL_KIND_EYE_ENVMAP: u32 = 16;
pub const MATERIAL_KIND_GLASS: u32 = 100;
pub const MATERIAL_KIND_EFFECT_SHADER: u32 = 101;
pub const MATERIAL_KIND_NO_LIGHTING: u32 = 102;
pub const MATERIAL_KIND_FIRE_REFRACTION: u32 = 103;

// #4584 — presentation display-transform operator ids (`params.tonemapOp`,
// push constant). The authoritative consumer contract lives in
// `tonemap.rs`, which re-exports these; presentation.frag compares
// against the generated `#define`s.
pub const TONEMAP_OP_ACES: u32 = 0;
pub const TONEMAP_OP_AGX: u32 = 1;

// Explicit geometry visibility layers shared with the ECS emitter contract.
// Each TLAS instance carries exactly one category bit. `GpuLight.params.z`
// stores any union of these bits as an exactly representable f32 integer.
pub const VISIBILITY_LAYER_ARCHITECTURE: u32 =
    byroredux_core::lighting::VisibilityMask::ARCHITECTURE.bits() as u32;
pub const VISIBILITY_LAYER_STATIC_PROP: u32 =
    byroredux_core::lighting::VisibilityMask::STATIC_PROP.bits() as u32;
pub const VISIBILITY_LAYER_DYNAMIC_ACTOR: u32 =
    byroredux_core::lighting::VisibilityMask::DYNAMIC_ACTOR.bits() as u32;
pub const VISIBILITY_LAYER_FOLIAGE: u32 =
    byroredux_core::lighting::VisibilityMask::FOLIAGE.bits() as u32;
pub const VISIBILITY_LAYER_GLASS: u32 =
    byroredux_core::lighting::VisibilityMask::GLASS.bits() as u32;
pub const VISIBILITY_LAYER_EFFECT: u32 =
    byroredux_core::lighting::VisibilityMask::EFFECT.bits() as u32;
pub const VISIBILITY_MASK_ALL_OPAQUE: u32 =
    byroredux_core::lighting::VisibilityMask::ALL_OPAQUE.bits() as u32;
pub const VISIBILITY_MASK_SOLID: u32 =
    byroredux_core::lighting::VisibilityMask::SOLID.bits() as u32;
pub const VISIBILITY_MASK_FULL: u32 = byroredux_core::lighting::VisibilityMask::FULL.bits() as u32;

pub const ATTENUATION_MODEL_LEGACY_SOFT_RANGE: u32 =
    byroredux_core::lighting::AttenuationModel::LegacySoftRange as u32;
pub const ATTENUATION_MODEL_INVERSE_SQUARE: u32 =
    byroredux_core::lighting::AttenuationModel::InverseSquare as u32;
pub const WORLD_UNITS_PER_METER: f32 = byroredux_core::lighting::BETHESDA_UNITS_PER_METER;
/// Authored influence range -> compatibility cull-range multiplier (#3575
/// follow-up). `gpu_light_from_emitter` ships the *derived* value in
/// `GpuLight.position_radius.w`, so every shader that needs the authored
/// range back has to divide by this. Emitting it here keeps that recovery
/// tied to the same `byroredux_core` constant the upload multiplies by,
/// instead of four independent `0.5` literals.
pub const LEGACY_LIGHT_CULL_RANGE_MULTIPLIER: f32 =
    byroredux_core::lighting::LEGACY_LIGHT_CULL_RANGE_MULTIPLIER;
pub const ADIABATIC_FLAME_TEMPERATURE_K: f32 =
    byroredux_core::combustion::ADIABATIC_FLAME_TEMPERATURE_K;
pub const COMBUSTION_REACTION_RATE_PER_SECOND: f32 =
    byroredux_core::combustion::REACTION_RATE_PER_SECOND;
pub const COMBUSTION_RICH_SOOT_YIELD: f32 = byroredux_core::combustion::RICH_SOOT_YIELD;
pub const COMBUSTION_LEAN_SOOT_YIELD: f32 = byroredux_core::combustion::LEAN_SOOT_YIELD;
pub const COMBUSTION_SOOT_OXIDATION_RATE_PER_SECOND: f32 =
    byroredux_core::combustion::SOOT_OXIDATION_RATE_PER_SECOND;
pub const COMBUSTION_SOOT_SINGLE_SCATTER_ALBEDO_RGB: [f32; 3] =
    byroredux_core::combustion::SOOT_SINGLE_SCATTER_ALBEDO_RGB;
pub const COMBUSTION_SOOT_OXIDATION_START_TEMPERATURE_K: f32 =
    byroredux_core::combustion::SOOT_OXIDATION_START_TEMPERATURE_K;
pub const COMBUSTION_SOOT_OXIDATION_FULL_TEMPERATURE_K: f32 =
    byroredux_core::combustion::SOOT_OXIDATION_FULL_TEMPERATURE_K;
pub const EXPLOSION_EXPANSION_TIME_SECONDS: f32 =
    byroredux_core::combustion::EXPLOSION_EXPANSION_TIME_SECONDS;
pub const EXPLOSION_IMPULSE_DURATION_SECONDS: f32 =
    byroredux_core::combustion::EXPLOSION_IMPULSE_DURATION_SECONDS;
pub const NUCLEAR_EXPLOSION_EXPANSION_SCALE: f32 =
    byroredux_core::combustion::NUCLEAR_EXPLOSION_EXPANSION_SCALE;
pub const NUCLEAR_EXPLOSION_BUOYANCY_SCALE: f32 =
    byroredux_core::combustion::NUCLEAR_EXPLOSION_BUOYANCY_SCALE;
pub const NUCLEAR_EXPLOSION_SMOKE_MASS_SCALE: f32 =
    byroredux_core::combustion::NUCLEAR_EXPLOSION_SMOKE_MASS_SCALE;
pub const COMBUSTION_OVERPRESSURE_DISSIPATION_PER_SECOND: f32 =
    byroredux_core::combustion::OVERPRESSURE_DISSIPATION_PER_SECOND;
pub const COMBUSTION_MAX_PRESSURE_ACCELERATION_MPS2: f32 =
    byroredux_core::combustion::MAX_PRESSURE_ACCELERATION_MPS2;
pub const COMBUSTION_MAX_DILUTION_RATE_PER_SECOND: f32 =
    byroredux_core::combustion::MAX_DILUTION_RATE_PER_SECOND;
pub const COMBUSTION_VORTICITY_CONFINEMENT_SPEED_MPS: f32 =
    byroredux_core::combustion::VORTICITY_CONFINEMENT_SPEED_MPS;
pub const COMBUSTION_MAX_VORTICITY_ACCELERATION_MPS2: f32 =
    byroredux_core::combustion::MAX_VORTICITY_ACCELERATION_MPS2;
pub const COMBUSTION_TURBULENCE_COARSE_EDDY_SCALE_METERS: f32 =
    byroredux_core::combustion::TURBULENCE_COARSE_EDDY_SCALE_METERS;
pub const COMBUSTION_TURBULENCE_DETAIL_EDDY_SCALE_METERS: f32 =
    byroredux_core::combustion::TURBULENCE_DETAIL_EDDY_SCALE_METERS;
pub const COMBUSTION_TURBULENCE_COARSE_RISE_SPEED_MPS: f32 =
    byroredux_core::combustion::TURBULENCE_COARSE_RISE_SPEED_MPS;
pub const COMBUSTION_TURBULENCE_DETAIL_RISE_SPEED_MPS: f32 =
    byroredux_core::combustion::TURBULENCE_DETAIL_RISE_SPEED_MPS;
pub const COMBUSTION_AEROSOL_DISSIPATION_PER_SECOND: f32 =
    byroredux_core::combustion::AEROSOL_DISSIPATION_PER_SECOND;
pub const COMBUSTION_AEROSOL_LINGER_SECONDS: f32 =
    byroredux_core::combustion::AEROSOL_LINGER_SECONDS;
pub const COMBUSTION_AEROSOL_LIFT_ACCELERATION_MPS2: f32 =
    byroredux_core::combustion::AEROSOL_LIFT_ACCELERATION_MPS2;
pub const COMBUSTION_AEROSOL_LIFT_EXTINCTION_SCALE: f32 =
    byroredux_core::combustion::AEROSOL_LIFT_EXTINCTION_SCALE;
pub const COMBUSTION_COOLED_AEROSOL_SOURCE_TEMPERATURE_K: f32 =
    byroredux_core::combustion::COOLED_AEROSOL_SOURCE_TEMPERATURE_K;
pub const COMBUSTION_EXPLOSION_SMOKE_EXTINCTION_SCALE: f32 =
    byroredux_core::combustion::EXPLOSION_SMOKE_EXTINCTION_SCALE;
pub const COMBUSTION_FUEL_VAPOUR_REMOVAL_PER_SECOND: f32 =
    byroredux_core::combustion::FUEL_VAPOUR_REMOVAL_PER_SECOND;
pub const COMBUSTION_RADIANCE_REMOVAL_PER_SECOND: f32 =
    byroredux_core::combustion::RADIANCE_REMOVAL_PER_SECOND;
pub const COMBUSTION_THERMAL_COOLING_PER_SECOND: f32 =
    byroredux_core::combustion::THERMAL_COOLING_PER_SECOND;
pub const COMBUSTION_THERMAL_BUOYANCY_ACCELERATION_MPS2: f32 =
    byroredux_core::combustion::THERMAL_BUOYANCY_ACCELERATION_MPS2;
pub const COMBUSTION_VELOCITY_DAMPING_PER_SECOND: f32 =
    byroredux_core::combustion::VELOCITY_DAMPING_PER_SECOND;
pub const COMBUSTION_REACTION_HEAT_RESPONSE: f32 =
    byroredux_core::combustion::REACTION_HEAT_RESPONSE;
pub const FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION: f32 =
    byroredux_core::combustion::FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION;
pub const FLAME_REACTION_ZONE_HEIGHT_FRACTION: f32 =
    byroredux_core::combustion::FLAME_REACTION_ZONE_HEIGHT_FRACTION;
pub const FLAME_REACTION_ZONE_FADE_START_FRACTION: f32 =
    byroredux_core::combustion::FLAME_REACTION_ZONE_FADE_START_FRACTION;
pub const FLAME_SOURCE_LATERAL_SPEED_MPS: f32 =
    byroredux_core::combustion::FLAME_SOURCE_LATERAL_SPEED_MPS;
pub const FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND: f32 =
    byroredux_core::combustion::FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND;
pub const COMBUSTION_BFECC_ERROR_CORRECTION_STRENGTH: f32 =
    byroredux_core::combustion::BFECC_ERROR_CORRECTION_STRENGTH;
pub const COMBUSTION_BFECC_ERROR_TRACE_THRESHOLD: f32 =
    byroredux_core::combustion::BFECC_ERROR_TRACE_THRESHOLD;

// Renderer-local medium-optics approximation for point/spot in-scattering in
// the froxel injector. The sun path carries its dual-lobe asymmetry through
// `VolumetricsParams` (authored per weather record); local lights have no
// per-source phase authoring anywhere in the content, so these three replace
// the former isotropic 1/4π with a fixed soot-like lobe. Mildly forward with
// a backward back-lobe: firelit smoke glows brighter when the lamp shines
// through it toward the viewer than when it is lit from the viewer's side.
pub const COMBUSTION_LOCAL_LIGHT_PHASE_FORWARD_G: f32 = 0.35;
pub const COMBUSTION_LOCAL_LIGHT_PHASE_BACKWARD_G: f32 = -0.12;
pub const COMBUSTION_LOCAL_LIGHT_PHASE_MIX: f32 = 0.65;

// Multiple-scattering energy compensation for the froxel in-scatter. The
// injector is single-scatter: every order >= 2 is dropped, which makes
// optically thick smoke read too dark and too contrasty against its own
// emission. Orders >= 2 are approximately isotropic and lose roughly one
// albedo per bounce, so a two-octave geometric series in the local medium's
// spectral single-scatter albedo recovers a bounded share of that energy
// without a second march. Weights are renderer-local optics shaping, not
// per-content authored values.
pub const COMBUSTION_MULTISCATTER_OCTAVE1_WEIGHT: f32 = 0.6;
pub const COMBUSTION_MULTISCATTER_OCTAVE2_WEIGHT: f32 = 0.3;

// Transported-combustion -> surface-light reduction. The froxel injector
// accumulates fixed-point radiant moments into this camera-centred grid; Rust
// drains the exact same ABI after the frame-slot fence. Keep the dimensions,
// domain, and quantization here so neither side can silently reinterpret a
// bin. 8 x 4 x 8 resolves distinct room-scale fires while keeping readback to
// 8 KiB per frame-in-flight slot (256 bins x 32 bytes).
pub const COMBUSTION_LIGHT_GRID_X: u32 = 8;
pub const COMBUSTION_LIGHT_GRID_Y: u32 = 4;
pub const COMBUSTION_LIGHT_GRID_Z: u32 = 8;
pub const COMBUSTION_LIGHT_GRID_COUNT: u32 =
    COMBUSTION_LIGHT_GRID_X * COMBUSTION_LIGHT_GRID_Y * COMBUSTION_LIGHT_GRID_Z;
pub const COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS: f32 = 32.0;
pub const COMBUSTION_LIGHT_HALF_EXTENT_Y_METERS: f32 = 16.0;
// 1/4096 radiant-intensity resolution keeps the 1e-3 irradiance cutoff
// representable even for centimetre-scale test scenes. Per-invocation values
// are still capped to 16 intensity units by the shader's 65535-word guard;
// bins accumulate those bounded contributions in u32.
pub const COMBUSTION_LIGHT_FIXED_SCALE: f32 = 4096.0;
// Luminous volume needs finer resolution: 1/65536 m^3 is a ~2.4 cm sphere.
pub const COMBUSTION_LIGHT_VOLUME_FIXED_SCALE: f32 = 65536.0;

const _: () = {
    assert!(COMBUSTION_LIGHT_GRID_COUNT == 256);
    assert!(COMBUSTION_LIGHT_FIXED_SCALE > 0.0);
    assert!(COMBUSTION_LIGHT_VOLUME_FIXED_SCALE > 0.0);
};

// Shared direct-shadow distance contract. These values apply to every
// environment; cell kind may change light sources and GI inputs, never the
// direct-light shadow reach. The trace distance covers the complete fade
// interval so the last shadowed samples taper out instead of ending early.
pub const SHADOW_FADE_START: f32 = 8_000.0;
pub const SHADOW_FADE_END: f32 = 12_000.0;
pub const DIRECTIONAL_SHADOW_TRACE_DISTANCE: f32 = SHADOW_FADE_END;

const _: () = {
    assert!(SHADOW_FADE_START >= 0.0);
    assert!(SHADOW_FADE_START < SHADOW_FADE_END);
    assert!(DIRECTIONAL_SHADOW_TRACE_DISTANCE >= SHADOW_FADE_END);
};

// Vulkan's instance mask is eight bits. Pin the complete cross-CPU/GPU
// contract and make accidental layer collisions a compile-time failure.
const _: () = {
    assert!(VISIBILITY_MASK_FULL <= 0xFF);
    assert!(VISIBILITY_MASK_FULL.count_ones() == 6);
    assert!(VISIBILITY_MASK_ALL_OPAQUE & VISIBILITY_LAYER_GLASS == 0);
    assert!(VISIBILITY_MASK_SOLID == VISIBILITY_MASK_ALL_OPAQUE | VISIBILITY_LAYER_GLASS);
    assert!(VISIBILITY_MASK_SOLID & VISIBILITY_LAYER_EFFECT == 0);
    assert!(
        VISIBILITY_MASK_FULL
            == VISIBILITY_MASK_ALL_OPAQUE | VISIBILITY_LAYER_GLASS | VISIBILITY_LAYER_EFFECT
    );
};

// Glass / IOR ray-work telemetry. GLASS_RAY_COST is the tier-0 estimate
// (one optional reflection + three refraction queries); the shader records
// two more estimated queries per adaptive quality tier as the budgeted
// march count grows (tiers grant 2→8 ray-pair opportunities; a single
// shadow-transport march's interface depth stays fixed at
// shadow_transport.glsl's shader-local MAX_GLASS_INTERFACES = 4 — #5848). GLASS_RAY_BUDGET is the tier-3 ceiling that
// `AdaptiveRayBudget::settings_for_tier` (ray_budget.rs) derives every
// tier's `glass_ray_limit` from (tiers 0-2 are /8, /4, /2 of this value) —
// #2686 / SAFE-D7-01 closed the prior state where the tier table was four
// independent literals that only happened to match this constant. The
// atomic counter itself is deliberately not a per-fragment admission pool:
// unordered winners split alpha glass between IOR and Fresnel paths and
// create permanent stipple. The controller chooses one coherent quality
// tier for the next frame instead; `glass_ray_limit` is retained in the
// GPU-side ABI for telemetry comparison only.
pub const GLASS_RAY_BUDGET: u32 = 2_097_152;
pub const GLASS_RAY_COST: u32 = 4;

// #3879 / TD7-2026-09-05-01 — the rest of the `GpuRayBudget` quality
// contract, on the same footing `GLASS_RAY_BUDGET` got in #2686.
//
// `AdaptiveRayBudget::settings_for_tier`'s tier-3 arm picks the values the
// CPU uploads; each consuming shader needs a *compile-time* ceiling for its
// bounded loop that is >= that tier-3 value. Before this, every one of these
// was an independently hand-typed GLSL literal — and the runtime clamp
// repeated it a second time on the next line (`clamp(rayBudget
// .directShadowSamples, 1u, 8u)` sat directly under `const int
// MAX_SHADOW_RAYS = 8;`). Raising a tier-3 value in ray_budget.rs was a
// silent no-op: the shader clamped the uploaded value back down to its own
// literal and the suite stayed green, because the guards asserted shader
// *source text* rather than the CPU/GPU relationship.
//
// These are emitted unsuffixed so a site can spell the type it needs
// (`int(MAX_PATH_SEGMENTS)` / `uint(MAX_PATH_SEGMENTS)`); GLSL treats a bare
// integer literal as `int`, so the uint clamps must cast explicitly.
pub const MAX_DIRECT_SHADOW_SAMPLES: u32 = 8;
pub const MAX_PATH_SEGMENTS: u32 = 6;
pub const MAX_SHADED_HITS: u32 = 2;
pub const MAX_FROXEL_LIGHTS: u32 = 8;

// The top adaptive quality tier. `settings_for_tier`'s `_ =>` arm is this
// tier, and `observe`'s escalation stops at `self.tier < MAX_RAY_QUALITY_TIER`.
pub const MAX_RAY_QUALITY_TIER: u32 = 3;

// The glass interface allowance grows by two estimated queries per quality
// tier, from 2 at tier 0 — so the tier-3 ceiling is `2 + 2 * 3`. This was a
// second independent literal (`const int MAX_REFRACT_PASSTHRUS = 8;`) with
// the tier count baked into it, which is why raising MAX_RAY_QUALITY_TIER
// alone would not have widened it.
pub const MAX_REFRACT_PASSTHRUS: u32 = 2 + 2 * MAX_RAY_QUALITY_TIER;

// First-bounce GI candidate pool. The shader ranks these locally, then stops
// after the first two VISIBLE contributors. Keeping eight candidates avoids a
// black bounce when the strongest one or two lamps are behind a wall without
// paying eight shadow rays on the common path.
pub const GI_HIT_LIGHT_CAP: u32 = 8;

// The "first two VISIBLE contributors" the comment above describes. #3880 —
// this was prose here and a separate hardcoded `const uint
// GI_VISIBLE_LIGHT_CAP = 2u;` in `include/lighting.glsl`, with no mechanical
// link between them; the provenance gate could not see it because include
// files were never scanned. Editing one and not the other changed what the
// pool means without changing what it does.
//
// #4017 — the consumer named above was `giHitIrradiance`, which had had no
// caller since `f8efde63` (2026-07-29) and has been deleted. The live
// consumer is `triangle.frag`'s `visibleLightLimit` at the
// `pathHitRadiance` call site, which #4017 repointed from a bare `2u` at
// the same time. Retuning this value now changes what renders; before, it
// changed nothing.
pub const GI_VISIBLE_LIGHT_CAP: u32 = 2;

// Mesh-ID G-buffer attachment (`R32_UINT`) ABI. Bit 31 is the
// ALPHA_BLEND_NO_HISTORY flag: when set, the low 31 bits switch into the
// alpha draw-index namespace and TAA/SVGF must treat the pixel as having no
// stable temporal history. Masking the bit off before comparing can alias
// unrelated surfaces, which is why `stableMeshIdsMatch` rejects rather than
// masks.
//
// #3881 — this is a genuine cross-CPU/GPU attachment contract, and it had no
// Rust-side definition at all: `scene_buffer/constants.rs`,
// `vulkan/context/helpers.rs` and `vulkan/pipeline.rs` each described the bit
// in prose, three independent restatements of a value with no code to point
// at. On the GLSL side it was declared once in `include/mesh_id.glsl` and
// hand-typed at five more sites, including the shader that *writes* it.
pub const MESH_ID_NO_HISTORY_BIT: u32 = 0x8000_0000;

// The complement — the stable-ID lane. Was `0x7FFFFFFFu` written out three
// times and never expressed as the complement of the bit above.
pub const MESH_ID_STABLE_MASK: u32 = !MESH_ID_NO_HISTORY_BIT;

// Chroma-preserving luminance ceiling for the complete one-pixel bounded-path
// sample before it enters SVGF. A single secondary GGX hit can line up with a
// point light's delta direction and produce a mathematically valid but extreme
// value at 1 spp; the temporal/spatial filters cannot average that heavy tail
// before it becomes a large low-frequency stain. Keep ordinary bounce energy
// untouched and bound only the outlier tail at the denoiser boundary.
pub const GI_SAMPLE_LUMINANCE_CLAMP: f32 = 1.0;

// Caustic accumulation
pub const CAUSTIC_FIXED_SCALE: f32 = 65536.0;

// Compute workgroup sizes (bloom, volumetrics, SSAO, TAA)
pub const WORKGROUP_X: u32 = 8;
pub const WORKGROUP_Y: u32 = 8;
pub const WORKGROUP_Z: u32 = 8;

// Clustered light culling thread count (one warp/wavefront wide on
// every IHV: NVIDIA = 32, AMD = 64 wavefront but a 32-thread workgroup
// still maps cleanly to half a wave, Intel = 8/16/32 SIMD width
// negotiates fine at this size). Consumed by `cluster_cull.comp` via
// the `#include`d `#define` for both `layout(local_size_x = ...)` and
// the thread-strided light scan loop. Omitted `u` suffix on the
// generated `#define` so it can be used in the layout qualifier
// (GLSL allows int literals but not `uint` literals there).
pub const THREADS_PER_CLUSTER: u32 = 32;

// M58 — bloom contribution coefficient. 0.15 is a hand-tuned perceptual
// constant (tuned down from 0.20 on Prospector saloon: sun-lit windows +
// chandelier globes were producing halos that bled too far across
// walls), chosen so Bethesda's LDR-authored emissives (0–1 monitor-space
// range, not HDR cd/m²) read as obviously bloomed without flooding dim
// surfaces.
//
// It is NOT tuned to cancel the upsample pyramid's own DC gain (see
// `bloom_upsample.comp`'s #1275 note) — that additive, non-renormalised
// up-chain carries an inherent ~5× DC gain at mip 0 for a
// spatially-uniform bright source. The two effects compose rather than
// cancel: the 5× (pyramid) × 0.15 (this constant) = 0.75× figure below
// is the UN-THRESHOLDED DC bound; since `62a09fd9`,
// `bloom_downsample.comp`'s `DownsampleParams::bright_pass` applies
// `BLOOM_THRESHOLD`'s soft knee first, so the intensity multiplies the
// knee-filtered pyramid — a highlight-selective glow, with the measured
// effective sky contribution ~1.01× post-knee (see `BLOOM_THRESHOLD`
// below). Absorbing the 5× gain down to Frostbite SIGGRAPH 2015's 0.04
// reference (which assumes a renormalised, unit-gain pyramid) would take
// ≈0.008, not 0.15 — this constant is doing LDR-authoring compensation,
// not gain cancellation.
//
// Consumed by `bloom_apply.comp` (`scene.rgb + bloom * BLOOM_INTENSITY`)
// via the `#include`d `#define`; `composite.frag` mentions it in
// comments only. Mirrored here so Rust-side
// `bloom::DEFAULT_BLOOM_INTENSITY` stays in lockstep. The global HDR
// emissive boost that would make this compensation unnecessary is named
// once, in `BLOOM_THRESHOLD`'s note below — see
// `feedback_color_space.md` for the policy discussion.
pub const BLOOM_INTENSITY: f32 = 0.15;

// M58 / sky-overdrive fix — bloom bright-pass knee.
//
// Measured defect (Skyrim SE, Tamriel 5,-24, weather `SkyrimCloudy`,
// deterministic `--camera-pos 5000,10500,-49000` capture): the sky
// rendered at a uniform linear 0.57-0.91 with its authored
// zenith->horizon gradient compressed to ~4% and the authored blue hue
// (WTHR zenith `[0.161, 0.380, 0.459]`, R/B = 0.35) washed to near-white
// (R/B = 0.81). Sampling the pre-bloom scene through the
// `composite_term` debug view and the post-bloom frame at the same
// camera isolated it: pre-bloom zenith `(0.335, 0.477, 0.539)`,
// post-bloom `(0.572, 0.822, 0.914)` — a flat **1.71x** multiply on all
// three channels, against the 1.75x that `BLOOM_INTENSITY`'s own
// derivation above predicts for a spatially-uniform source.
//
// That is bloom behaving exactly as documented, on content it was never
// meant to touch. With no bright-pass anywhere in the chain, the pyramid's
// "local blurred average" over a large uniform region converges to the
// region's own value, so `scene += bloom * BLOOM_INTENSITY` degenerates
// from a highlight glow into a **pure gain** — and the sky is the largest
// uniform bright region any exterior frame has. ACES then compresses the
// result into its shoulder, which is what destroys both the gradient and
// the hue: the defect is not that the sky is bright, it is that the sky is
// multiplied into the part of the curve that cannot represent it.
//
// The knee is a Karis/Jimenez soft threshold applied ONCE, when the
// pyramid is seeded from the scene (mip 0 of the down-chain); deeper
// levels must not re-apply it or the curve compounds per level.
// `DownsampleParams::bright_pass` carries the per-level enable.
//
// Values are chosen against the measured scene, not by feel:
//   * diffuse sky body measures 0.33-0.71 pre-bloom, so it must sit
//     below the knee's upper edge;
//   * the sun disc reaches `sun_color * sun_intensity * sun_glare` plus
//     the sky behind it (~1.8 for this weather), so it must sit above it.
// `THRESHOLD +/- KNEE` = [0.5, 1.5] separates them: the sky retains ~3%
// of its bloom (gain 1.71x -> ~1.02x) while the disc retains ~44%.
//
// Bethesda's LDR-authored emissives (0-1 monitor-space, the range
// `BLOOM_INTENSITY` was hand-tuned for) land in the knee's lower half and
// keep ~12% rather than the full broadband lift. That is a deliberate,
// visible reduction: they are 0-1 content being read as scene radiance,
// and the real fix is the global HDR emissive boost that
// `feedback_color_space.md` and `BLOOM_INTENSITY`'s note above both
// already name. Compensating for it here by dropping the threshold would
// only reinstate the sky gain this constant exists to remove.
// SKYAL volumetric cloud layer (`include/clouds.glsl`). Method follows
// Schneider & Vos, "The Real-Time Volumetric Cloudscapes of Horizon:
// Zero Dawn", SIGGRAPH 2015 Advances in Real-Time Rendering. Each value
// below is cited per constant (Schneider & Vos unless noted; the MS
// falloffs cite Skybolt as a secondary reference with owner sign-off) —
// the in-shader shape literals that are NOT here (height-gradient
// breakpoints, erosion strengths, advection/wind scales) are uncited and
// recorded as such in skyal.md §2.3 "Still open" (#4314).
//
// Shell altitudes in metres. The reference layer is 1.5 km base / 5 km
// top, which is the ordinary cumulus band.
pub const CLOUD_LAYER_BOTTOM: f32 = 1500.0;
pub const CLOUD_LAYER_TOP: f32 = 5000.0;
/// Planet radius in metres — the real one. The cloud layer is a spherical
/// shell rather than a plane so it curves away at the horizon instead of
/// terminating at an edge, which is what the UV-projected cloud planes
/// could never do (they need a `smoothstep` horizon fade to hide their
/// projection singularity).
pub const CLOUD_PLANET_RADIUS: f32 = 6_371_000.0;
/// Cheap-sample budgets for the adaptive cloud march (`clouds.glsl`).
/// Schneider & Vos 2015, slide 80: "we start with an initial potential 64
/// samples and end with a potential 128 at the horizon", because the ray
/// through the shell lengthens toward the horizon. The cheap step is the
/// ray length divided by this budget, interpolated on `dir.y`.
pub const CLOUD_CHEAP_SAMPLES_ZENITH: u32 = 64;
pub const CLOUD_CHEAP_SAMPLES_HORIZON: u32 = 128;
/// Hard bound on march iterations per ray. Derived so it never truncates
/// the adaptive march: cheap steps and cheap-capped full steps together
/// cover the ray once (<= 129); each cheap->full transition re-covers at
/// most one cheap step (<= 129 more); step-backs number <= 129; and
/// mean-free-path-limited full steps each add optical depth 1, so the
/// march exits (T < 0.01, depth ~4.6) after ~5 of them. That totals ~392;
/// 512 bounds it with margin. It is a loop bound, not an appearance knob.
pub const CLOUD_MAX_MARCH_ITERATIONS: u32 = 512;
/// Self-shadow samples toward the sun per full view sample: six, per
/// Schneider & Vos 2015 slide 89 ("6 light samples per march"). They are
/// spaced geometrically from one mean free path out to the shell's top
/// (see `clouds.glsl`), not evenly, because even spacing aliased.
pub const CLOUD_LIGHT_STEPS: u32 = 6;
/// Two-lobe cloud phase function (`clouds.glsl` `cloud_phase`). Values from
/// Hillaire 2016, "Physically Based Sky, Atmosphere and Cloud Rendering in
/// Frostbite" (SIGGRAPH 2016 PBS course), slides 36/38: "2-lobe HG phase
/// with g0=0.8, g1=-0,5 and lerp=0.5". The forward lobe gives the silver
/// lining; the backward lobe keeps clouds lit when the sun is behind the
/// viewer. They replace an unsourced `max(HG(0.8), 0.7 * HG(-0.15))`.
pub const CLOUD_PHASE_G0: f32 = 0.8;
pub const CLOUD_PHASE_G1: f32 = -0.5;
pub const CLOUD_PHASE_BLEND: f32 = 0.5;
/// Multiple-scattering octaves (`clouds.glsl`). Method: Wrenninge et al.,
/// "Oz: The Great and Volumetric", SIGGRAPH 2013, as applied to clouds by
/// Hillaire 2016 course notes §5.8, Eq. 19-20 — N single-scattering octaves
/// summed, octave n using scattering × a^n, extinction × b^n and phase
/// eccentricity × c^n. Energy-conserving only while a <= b (§5.8).
///
/// N = 2: Hillaire's Figure 40 shows N=1 (single), N=2 (multi) and N=3 as
/// "exaggerated multi scattering".
pub const CLOUD_MS_OCTAVES: u32 = 2;
/// a, b, c. Neither Wrenninge nor Hillaire publishes values. 0.5 for all
/// three is Skybolt's open-source implementation ("Rendering Planetwide
/// Volumetric Clouds in Skybolt", Prograda 2021), which cites the same method
/// and describes 0.5 as a falloff that "looks fine" — i.e. a secondary,
/// tuned reference, adopted with the project owner's explicit sign-off
/// (2026-09-13) rather than a primary citation. Satisfies a <= b.
pub const CLOUD_MS_SCATTERING_FALLOFF: f32 = 0.5;
pub const CLOUD_MS_EXTINCTION_FALLOFF: f32 = 0.5;
pub const CLOUD_MS_ECCENTRICITY_FALLOFF: f32 = 0.5;
/// Cloud extinction coefficient at density 1, per metre (`clouds.glsl`).
///
/// Hillaire 2016 course notes §5.2, citing Hess, Koepke & Schult 1998
/// ("Optical Properties of Aerosols and Clouds: The Software Package OPAC"):
/// measured water clouds have single-scattering albedo 1 and extinction in
/// [0.04, 0.06] m^-1 for stratus and [0.05, 0.12] m^-1 for cumulus at 550 nm,
/// so sigma_s = sigma_t. The layer is the cumulus band (1.5-5 km), so the
/// cumulus range applies.
///
/// The march's density field runs [0, 1] after the coverage remap, height
/// gradient and erosion, so density 1 is mapped to the top of the measured
/// cumulus range. Density is then the fraction of the densest measured
/// cumulus, and eroded edges and thin cloud fall naturally through the rest
/// of the range and below it. It replaces an unsourced 0.0016 — about 75x
/// too optically thin.
pub const CLOUD_EXTINCTION_PER_METER: f32 = 0.12;
/// Altitude in metres of the shell the authored WTHR cloud textures are drawn
/// on (`sky.glsl` `sky_upper_cloud_plane`), above the volumetric deck.
///
/// The textures are the games' own cloud art; the volumetric march is the low
/// deck in front of them. WMO International Cloud Atlas (2017), "Étages":
/// high-étage clouds (cirrus, cirrocumulus, cirrostratus) occur at 5-13 km in
/// temperate regions. 9 km is that range's midpoint, and it clears
/// [`CLOUD_LAYER_TOP`], so the plane is always behind the deck. At the zenith
/// the shell maps to the same UVs the former flat `dir.xz / dir.y` plane did.
pub const CLOUD_UPPER_PLANE_ALTITUDE: f32 = 9000.0;
/// Clear-air extinction for the aerial perspective between the viewer and the
/// cloud layers (`sky.glsl` `sky_air_optical_depth`). Earth parameters from
/// Hillaire 2020, "A Scalable and Production Ready Sky and Atmosphere
/// Rendering Technique" (EGSR), Table 1 (after Bruneton & Neyret 2008):
/// Rayleigh scattering (5.802, 13.558, 33.1) x 1e-6 m^-1 with an 8 km scale
/// height — Rayleigh does not absorb, so that is also its extinction — and Mie
/// scattering 3.996e-6 plus absorption 4.40e-6 m^-1 with a 1.2 km scale
/// height. Only the extinction is used: the in-scattered colour stays the
/// authored WTHR sky's, so the palette remains the games'.
pub const ATMOSPHERE_RAYLEIGH_EXTINCTION: [f32; 3] = [5.802e-6, 13.558e-6, 33.1e-6];
pub const ATMOSPHERE_RAYLEIGH_SCALE_HEIGHT: f32 = 8000.0;
pub const ATMOSPHERE_MIE_EXTINCTION: f32 = 8.396e-6;
pub const ATMOSPHERE_MIE_SCALE_HEIGHT: f32 = 1200.0;
/// Cloud-shadow map (`cloud_shadow.comp`): direct-sun transmittance through
/// the volumetric deck and the upper plane, per world position, so surfaces
/// get moving, position-dependent cloud shadows instead of one weather-wide
/// dimming factor. Texels per side of the camera-centred square.
pub const CLOUD_SHADOW_MAP_SIZE: u32 = 512;
/// Side of that square in metres. The base noise repeats every
/// 1 / 0.00008 = 12.5 km (`clouds.glsl`), so individual clouds are a few km
/// across: 64 km holds several cloud cells around the viewer in every
/// direction at 125 m per texel, which the bilinear sample resolves into
/// soft-edged shadows. Receivers beyond it take the edge texel.
pub const CLOUD_SHADOW_MAP_EXTENT: f32 = 64000.0;
/// Density samples per shadow column through the deck. The column is at most
/// (top - bottom) / sin(elevation) long; 16 samples put a 3.5 km zenith column
/// at 219 m per sample, finer than the 125 m texel's footprint can show only
/// near the horizon, where the sun's own light is weak.
pub const CLOUD_SHADOW_STEPS: u32 = 16;
/// Broad stratiform cover the discrete deck does not model. Coverage above
/// `CLOUD_OVERCAST_ONSET` — the "pleasant" weather's 0.40
/// (`fog_coverage_from_weather`), so clear and pleasant skies are untouched —
/// adds a uniform extinction that reaches `CLOUD_OVERCAST_OPTICAL_DEPTH` at
/// full coverage. 2.5 is the optical depth the removed weather-wide
/// `exp(-2.5 * coverage)` used for a full deck (about 8% of the sun left), so
/// overcast-class weathers (0.70 cloudy, 0.80 rain, 0.86 storm) keep a
/// comparable dark ground while the deck's local shadows ride on top. The
/// deck alone leaves a 0.70 sky as a few puffs over a sunlit ground
/// (skyal.md §2.5, "Known limitation").
pub const CLOUD_OVERCAST_ONSET: f32 = 0.4;
/// How far a volumetric cloud's chroma follows the weather's authored cloud
/// tint (`clouds.glsl`): 0 is physical white-ish scattering, 1 is the authored
/// cloud colour. The games' colour temperature and mood live in those tints
/// (FO3/FNV and each DLC author their own), so the puffs mostly follow them,
/// keeping a little physical colour so sunset and storm lighting still read.
pub const CLOUD_MOOD_CHROMA_WEIGHT: f32 = 0.75;
/// Coverage range over which the deck turns from scattered cumulus into a
/// continuous stratiform deck (`clouds.glsl` `cloud_morphology`): none at the
/// pleasant weather's 0.40 and below, full at the rain weather's 0.80.
pub const CLOUD_STRATIFORM_START: f32 = 0.55;
pub const CLOUD_STRATIFORM_FULL: f32 = 0.80;
pub const CLOUD_OVERCAST_OPTICAL_DEPTH: f32 = 2.5;
/// Third texture coordinate of the base-noise slice the cloud shell reads as
/// its 2D weather map: the centre of texel layer `BASE_NOISE_SIZE / 2` (64 /
/// 2 = 32, so (32 + 0.5) / 64). Pinned against the host's threshold slice by
/// `cloud_column_threshold_selects_the_coverage_fraction_of_columns`.
pub const CLOUD_WEATHER_SLICE_W: f32 = 0.5078125;

pub const BLOOM_THRESHOLD: f32 = 1.0;
/// Half-width of the soft knee around [`BLOOM_THRESHOLD`] — the quadratic
/// ramp spans `THRESHOLD - KNEE ..= THRESHOLD + KNEE`. Zero would make the
/// bright-pass a hard cutoff and put a visible contour wherever the sky
/// crosses it.
pub const BLOOM_KNEE: f32 = 0.5;

// M55 — default volumetric far plane in Bethesda world units. Runtime
// shaders now receive this through their UBO because the reach is configurable;
// the generated define remains as the canonical default for diagnostics and
// shader-contract tests.
//
// The renderer deliberately preserves Gamebryo coordinates (70 units per
// metre). The original 200.0 value was documented and tuned as 200 metres but
// consumed directly beside world-space positions, truncating the volume at
// 2.86 m. 8,960 units gives the default 128 m reach; volumetric density
// is converted from 1/m to 1/world-unit on the host side in volumetrics.rs.
pub const VOLUME_FAR: f32 = 8_960.0;

// Per-instance flag bits on `GpuInstance.flags` (lower 16 bits — the
// upper 16 bits pack the terrain-tile slot, mirrored below as
// `INSTANCE_TERRAIN_TILE_SHIFT`/`_MASK`). Authoritative Rust-side values
// live in `crates/renderer/src/vulkan/scene_buffer/constants.rs`; this
// shader-side mirror is pinned equal via
// `instance_flag_bits_match_scene_buffer_consts` so the two layers
// can't drift. See #1190 (TD4-NEW-01). The reserved PRESKINNED bit
// (bit 6) is not emitted as a shader-side flag because nothing in
// GLSL reads it today; if it grows a consumer, add the bit + a
// matching `#define` to keep the include the single source of truth.
pub const INSTANCE_FLAG_NON_UNIFORM_SCALE: u32 = 1 << 0;
pub const INSTANCE_FLAG_ALPHA_BLEND: u32 = 1 << 1;
pub const INSTANCE_FLAG_CAUSTIC_SOURCE: u32 = 1 << 2;
pub const INSTANCE_FLAG_TERRAIN_SPLAT: u32 = 1 << 3;
// Bit offset/mask for the `RenderLayer` classification packed into
// bits 4..5 of `GpuInstance.flags` (#2045 / TD7-101). Previously
// hand-written as `INST_RENDER_LAYER_SHIFT`/`_MASK` directly in
// `triangle.frag` with no lockstep test, unlike every other
// `INSTANCE_FLAG_*` bit; pinned equal to
// `scene_buffer::constants::INSTANCE_RENDER_LAYER_SHIFT`/`_MASK` via
// `instance_render_layer_bits_match_scene_buffer_consts`. Consumed by
// the fragment shader's `DBG_VIZ_RENDER_LAYER` debug-viz branch.
pub const INSTANCE_RENDER_LAYER_SHIFT: u32 = 4;
pub const INSTANCE_RENDER_LAYER_MASK: u32 = 0x3;
pub const INSTANCE_FLAG_FLAT_SHADING: u32 = 1 << 7;
pub const INSTANCE_FLAG_LOD_BLOCK: u32 = 1 << 9;
// bit 10 — this instance moved this build: a rigid model matrix that differs
// from its previous one, or a skinned pose marked dirty. Read by
// `triangle.frag` on the selected shadow ray's hit to taint that pixel's
// direct-light history (the per-pixel replacement for the scene-wide
// "anything moved" gate).
pub const INSTANCE_FLAG_MOVED_THIS_FRAME: u32 = 1 << 10;
// bit 8 — diffuse texture carries a genuine authored alpha channel
// (BC2/BC3/BC7/RGBA). Set CPU-side from the cached `handle_has_alpha`
// classification (`format_has_alpha`, which excludes BC1_RGBA). When
// CLEAR, `triangle.frag` pins `texColor.a` to 1.0 (unless an alpha test
// is active) so a BC1 3-colour-block texel (1-bit punch-through, not
// authored alpha) can't leak transparency into the discard / decalWeight
// / finalAlpha paths on a pure-blend mesh. See #1653.
pub const INSTANCE_FLAG_DIFFUSE_ALPHA: u32 = 1 << 8;
// Bit offset/mask for the terrain-tile slot packed into bits 16..31 of
// `GpuInstance.flags` (#470). This was the last packed field in `flags`
// that GLSL unpacked with hand-written literals — `triangle.frag` read
// `(inst.flags >> 16) & 0xFFFFu` while the CPU packed it with the named
// constants, exactly the pattern #2045 removed for the render-layer bits
// one field earlier. Pinned equal to
// `scene_buffer::constants::INSTANCE_TERRAIN_TILE_SHIFT`/`_MASK` via
// `instance_terrain_tile_bits_match_scene_buffer_consts` (#4027), so
// widening the window (`MAX_TERRAIN_TILES` is capped at 65535 *by this
// encoding*) can no longer leave the shader indexing `terrainTiles[]`
// through a stale window.
pub const INSTANCE_TERRAIN_TILE_SHIFT: u32 = 16;
pub const INSTANCE_TERRAIN_TILE_MASK: u32 = 0xFFFF;

// LAND splat geometry (#5113): `Vertex` carries splat weights as 2 × RGBA8
// unorm (`splat_weights_0`/`_1`), so the shader-side channel count is 8 and
// one lane word holds 4. Every consumer previously re-typed the bare `8`
// (the `GpuTerrainTile` array length on both sides of the SSBO, the three
// splat loops, the terrain packer's budget arithmetic) and the
// `i < 4 ? splat0 : splat1[i - 4]` lane split — #4496 pinned the packer's
// premise without naming the constant, and #4056 added a fourth shader
// loop after it closed. Sized from the same `Vertex` fields the weights
// upload from (`terrain_splat_constants_match_the_vertex_weight_lanes`),
// so growing the vertex lanes forces the whole chain to move together.
pub const TERRAIN_SPLAT_LAYERS: u32 = 8;
pub const TERRAIN_SPLAT_LANES_PER_WORD: u32 = 4;

// Per-material flag bits on `GpuMaterial.materialFlags`. Authoritative
// Rust-side values live in `crates/renderer/src/vulkan/material.rs`
// (`material_flag::*`); this shader-side mirror is pinned equal via
// `material_flag_bits_match_material_consts`. See #1190. build.rs emits
// these as `#define`s into `shader_constants.glsl`, so `triangle.frag`
// MUST get them from the `#include` — never hand-write them.
//
// Bits 5-9 (the #1147 Phase 2a / #1248-#1250 Disney BSDF + SSS +
// model-space-normals suite) were previously hand-written `#define`s in
// `triangle.frag` with no lockstep test; #1285 brought them into the
// generated header alongside bits 0-4.
pub const MAT_FLAG_VERTEX_COLOR_EMISSIVE: u32 = 1 << 0;
pub const MAT_FLAG_EFFECT_SOFT: u32 = 1 << 1;
pub const MAT_FLAG_EFFECT_PALETTE_COLOR: u32 = 1 << 2;
pub const MAT_FLAG_EFFECT_PALETTE_ALPHA: u32 = 1 << 3;
pub const MAT_FLAG_EFFECT_LIT: u32 = 1 << 4;
pub const MAT_FLAG_PBR_BSDF: u32 = 1 << 5;
pub const MAT_FLAG_TRANSLUCENCY: u32 = 1 << 6;
pub const MAT_FLAG_MODEL_SPACE_NORMALS: u32 = 1 << 7;
pub const MAT_FLAG_TRANSLUCENCY_THICK_OBJECT: u32 = 1 << 8;
pub const MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO: u32 = 1 << 9;
// Non-occluding glass sheet/shell. This is a canonical behavior flag, not a
// BGEM provenance bit: any source translator may select it.
pub const MAT_FLAG_THIN_GLASS: u32 = 1 << 11;
// #2826 (REN-D19-02) — set when the bound MODEL_SPACE_NORMALS map's blue
// channel carries authored Z (three-channel FO4 `_msn`, BC3) rather than
// being empty (two-channel `_msn`, BC1). Distinguished from the DDS
// compression format at texture-load time; see `material_flag::
// MSN_HAS_AUTHORED_Z` in `vulkan/material.rs` for the full rationale.
pub const MAT_FLAG_MSN_HAS_AUTHORED_Z: u32 = 1 << 12;
pub const MAT_FLAG_SOFT_LIGHTING: u32 = 1 << 13;
pub const MAT_FLAG_RIM_LIGHTING: u32 = 1 << 14;
pub const MAT_FLAG_BACK_LIGHTING: u32 = 1 << 15;
// `MAT_FLAG_EFFECT_LI_SHIFT` — bit offset for the 8-bit
// `BSEffectShaderProperty.lighting_influence` byte packed into bits
// 16-23 of `materialFlags`. Extract via
// `float((mat.materialFlags >> MAT_FLAG_EFFECT_LI_SHIFT) & 0xFFu) / 255.0`.
// Paired with `material_flag::EFFECT_LI_SHIFT` (Rust) and pinned by
// `material_flag_bits_match_material_consts`. See #890 Stage 2.
pub const MAT_FLAG_EFFECT_LI_SHIFT: u32 = 16;

// NOTE: `material_flag::BGSM_AUTHORED` (Rust-side bit 10) is
// NOT mirrored here — the shader is format-agnostic and doesn't
// branch on material provenance. BGSM → standardized PBR
// translation happens in `merge_external_material`, which writes
// `metalness_override` / `roughness_override` on the raw-tier
// `ImportedMesh`; `translate_material` then resolves those into
// `Material.{metalness,roughness}`. The Rust-side flag rides
// through for debug-server inspection only.
// See `feedback_format_translation.md`.

// High bit OR'd into `GpuMaterial.glossMapIndex` to tell the fragment shader
// "the gloss/smoothness mask lives in the NORMAL map's ALPHA channel"
// (Skyrim/Gamebryo normal-alpha-as-spec). Set per-draw CPU-side in
// `byroredux::render::static_meshes`; the shader masks it off for the index
// (`glossMapIndex & ~NORMAL_ALPHA_SPEC_BIT`) and samples `.a` instead of `.r`.
// Lockstep with `triangle.frag` and `byroredux::material_translate`, which
// re-exports this value rather than re-declaring it (#1500 / REN2-15).
pub const NORMAL_ALPHA_SPEC_BIT: u32 = 0x8000_0000;

// High bit OR'd into `GpuMaterial.parallaxMapIndex` to tell the fragment
// shader "this height map's values live in the ALPHA channel, not `.r`".
// Same mechanism and same value as `NORMAL_ALPHA_SPEC_BIT` above, on a
// different field — named separately because it encodes a different fact and
// the two are set from different predicates.
//
// #3530 — Oblivion's only authored parallax signal is
// `NiTexturingProperty`'s `APPLY_HILIGHT2` Apply Mode, and that game ships no
// separate height texture (`Oblivion - Textures - Compressed.bsa` has zero
// `_p.dds` entries), so the parser binds the NORMAL map into the height slot
// and flags the channel. Set per-draw CPU-side in
// `byroredux::render::static_meshes` from `Material.parallax_height_in_alpha`;
// `parallaxDisplaceUV` masks it off for the index and samples `.a`.
pub const PARALLAX_ALPHA_HEIGHT_BIT: u32 = 0x8000_0000;

// High bit OR'd into `GpuMaterial.tintMapIndex` to tell the fragment shader
// "the tint texture carries a usable ALPHA channel" — i.e. the
// `mix(albedo, albedo * tint.rgb, tint.a)` weight is authored data, not the
// sampler's format default. #4423 — the tint role's only vanilla producer is
// the Skin Tint shader's `*_sk.dds` map, which the Beyond Skyrim
// documentation defines as a light-diffusion (subsurface) input, and which
// ships BC1 (alpha-less: every sampled `.a` is 1.0). Multiplying its dark
// red RGB into albedo at full weight collapsed every Skyrim head, body and
// hands to 1/5–1/200 of their diffuse. The block therefore fires only when
// this bit says the alpha weight is real; alpha-less tint maps stay inert
// until the subsurface consumer (M56) exists. Set per-draw CPU-side in
// `byroredux::render::static_meshes` from the registry's `has_alpha` for the
// bound tint handle; the shader masks it off for the index.
pub const TINT_ALPHA_WEIGHT_BIT: u32 = 0x8000_0000;

// Water motion-kind enum (WATR-driven, mapped per-WATR record).
// Lockstep with `water.frag` and `byroredux/src/cell_loader/water.rs`.
pub const WATER_CALM: u32 = byroredux_core::ecs::components::water::WaterKind::Calm as u32;
pub const WATER_RIVER: u32 = byroredux_core::ecs::components::water::WaterKind::River as u32;
pub const WATER_RAPIDS: u32 = byroredux_core::ecs::components::water::WaterKind::Rapids as u32;
pub const WATER_WATERFALL: u32 =
    byroredux_core::ecs::components::water::WaterKind::Waterfall as u32;
pub const WATER_LAVA: u32 = byroredux_core::ecs::components::water::WaterKind::Lava as u32;
pub const DEFAULT_WATER_WAVE_AMPLITUDE: f32 =
    byroredux_core::ecs::components::water::DEFAULT_WATER_WAVE_AMPLITUDE;

/// Re-exported so `shader_constants.rs` emits the same value the canonical
/// `Material::default()` uses — #3459.
pub const DEFAULT_GLASS_BLUR_SCALE: f32 =
    byroredux_core::ecs::components::material::DEFAULT_GLASS_BLUR_SCALE;
/// See [`DEFAULT_GLASS_BLUR_SCALE`].
pub const DEFAULT_GLASS_REFRACTION_SCALE: f32 =
    byroredux_core::ecs::components::material::DEFAULT_GLASS_REFRACTION_SCALE;
pub const DEFAULT_WATER_WAVE_FREQUENCY: f32 =
    byroredux_core::ecs::components::water::DEFAULT_WATER_WAVE_FREQUENCY;
// #5582 — `water.vert`'s low-frequency two-wave displacement. Owned by core
// beside `WEATHER_SCROLL_PER_BU_PER_S`, where the CPU crest sampler
// (`physics::water::authored_wave_height_with_weather`) reads the same
// names; these were shader-local literals the physics mirror re-typed.
pub const WATER_WAVE_DEFAULT_SCROLL_A: f32 =
    byroredux_core::ecs::components::water::WATER_WAVE_DEFAULT_SCROLL_A;
pub const WATER_WAVE_DEFAULT_SCROLL_B: f32 =
    byroredux_core::ecs::components::water::WATER_WAVE_DEFAULT_SCROLL_B;
pub const WATER_WAVE_RATE_MIN: f32 = byroredux_core::ecs::components::water::WATER_WAVE_RATE_MIN;
pub const WATER_WAVE_RATE_MAX: f32 = byroredux_core::ecs::components::water::WATER_WAVE_RATE_MAX;
pub const WATER_WAVE_MIX_A: f32 = byroredux_core::ecs::components::water::WATER_WAVE_MIX_A;
pub const WATER_WAVE_MIX_B: f32 = byroredux_core::ecs::components::water::WATER_WAVE_MIX_B;
pub const WATER_WAVE_PHASE_B_RATE: f32 =
    byroredux_core::ecs::components::water::WATER_WAVE_PHASE_B_RATE;
pub const WATER_WAVE_AMPLITUDE_MAX: f32 =
    byroredux_core::ecs::components::water::WATER_WAVE_AMPLITUDE_MAX;
pub const WATER_WAVE_SPATIAL_FLOOR_A: f32 =
    byroredux_core::ecs::components::water::WATER_WAVE_SPATIAL_FLOOR_A;
pub const WATER_WAVE_SPATIAL_FLOOR_B: f32 =
    byroredux_core::ecs::components::water::WATER_WAVE_SPATIAL_FLOOR_B;
pub const WATER_WAVE_FALLBACK_DIR_A: [f32; 2] =
    byroredux_core::ecs::components::water::WATER_WAVE_FALLBACK_DIR_A;
pub const WATER_WAVE_FALLBACK_DIR_B: [f32; 2] =
    byroredux_core::ecs::components::water::WATER_WAVE_FALLBACK_DIR_B;
// Exponent of `water.frag`'s `absorbWaterColumn` transmission curve,
// normalised so the column reaches exactly the deep tint at the end of the
// authored fog ramp. Empirical shape; the saturation is the contract.
pub const WATER_COLUMN_ABSORPTION_SHAPE: f32 = 2.0;
/// Period of `water.frag`'s dual-phase BGSM flow-map advection (#4930):
/// each phase's UV offset saw-tooths over this many seconds, so the
/// offset stays bounded at any uptime. Engine presentation constant, not
/// authored data: at the canonical `0.02` UV/s rate (flow-map scale 1) one
/// phase travels 0.08 UV — under a tenth of a tile — before it resets.
pub const WATER_FLOW_MAP_CYCLE_SECONDS: f32 = 4.0;

// Local fog-volume clustering (M55/Session 62). Lockstep with
// `volumetrics_inject.comp`'s `sampleLocalMedium` and
// `vulkan::volumetrics`, which derive their own `usize` copies from these
// (#2229 / REN-D3-02 — previously hand-duplicated as GLSL literals with no
// shared source, the same defect class as #1190/#1401).
/// Camera-centered world-space cluster resolution used for local fog.
pub const FOG_VOLUME_CLUSTER_DIM: u32 = 16;
/// Bounded primitive references per cluster. FO3/FNV's busiest shipped cells
/// place more than 50 baked shafts within 512 world units of one another;
/// 64 avoids the obvious loss at the former 8-entry cap. Overflow keeps the
/// nearest volumes because the CPU input list is distance-sorted.
pub const MAX_FOG_VOLUMES_PER_CLUSTER: u32 = 64;
/// Candidate authored shaft apertures whose sun-swept footprints can reach a
/// cluster. Separate from density references so moving shafts cannot evict
/// smoke/fog from the existing bounded medium list.
pub const MAX_FOG_PORTALS_PER_CLUSTER: u32 = 128;
/// Authored box apertures passed to the composite so a modeled legacy shell
/// behind a window can be replaced by the outdoor sky. A 256-entry UBO stays
/// below Vulkan's 16 KiB minimum uniform-buffer range.
pub const MAX_COMPOSITE_SKY_APERTURES: u32 = 256;
/// Offset from a clear sun ray used to test whether architectural geometry
/// surrounds an unmarked opening in a sealed interior.
pub const FOG_APERTURE_RIM_PROBE_RADIUS_BU: f32 = 192.0;
/// Minimum absolute normal agreement among architectural rim hits.
pub const FOG_APERTURE_RIM_NORMAL_DOT_MIN: f32 = 0.9;
/// Maximum displacement of a rim hit from the first hit's plane.
pub const FOG_APERTURE_RIM_PLANE_TOLERANCE_BU: f32 = 24.0;

// Canonical local-medium profiles. Source-format/game interpretation ends at
// the FogVolume -> GpuFogVolume boundary; both Rust and GLSL consume these
// generated renderer-domain identifiers rather than maintaining parallel
// numeric tables.
pub const FOG_VOLUME_PROFILE_HOMOGENEOUS: f32 =
    byroredux_core::ecs::FogProfile::Homogeneous as u32 as f32;
pub const FOG_VOLUME_PROFILE_SMOKE: f32 = byroredux_core::ecs::FogProfile::Smoke as u32 as f32;
pub const FOG_VOLUME_PROFILE_FLAME: f32 = byroredux_core::ecs::FogProfile::Flame as u32 as f32;
pub const FOG_VOLUME_PROFILE_EXPLOSION: f32 =
    byroredux_core::ecs::FogProfile::Explosion as u32 as f32;
pub const FOG_VOLUME_PROFILE_EXPLOSION_OIL: f32 =
    byroredux_core::ecs::FogProfile::OilExplosion as u32 as f32;
pub const FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR: f32 =
    byroredux_core::ecs::FogProfile::NuclearExplosion as u32 as f32;
pub const FOG_VOLUME_PROFILE_LIGHT_SHAFT: f32 =
    byroredux_core::ecs::FogProfile::LightShaft as u32 as f32;
pub const FOG_VOLUME_PROFILES: &[(&str, f32)] = &[
    (
        "FOG_VOLUME_PROFILE_HOMOGENEOUS",
        FOG_VOLUME_PROFILE_HOMOGENEOUS,
    ),
    ("FOG_VOLUME_PROFILE_SMOKE", FOG_VOLUME_PROFILE_SMOKE),
    ("FOG_VOLUME_PROFILE_FLAME", FOG_VOLUME_PROFILE_FLAME),
    ("FOG_VOLUME_PROFILE_EXPLOSION", FOG_VOLUME_PROFILE_EXPLOSION),
    (
        "FOG_VOLUME_PROFILE_EXPLOSION_OIL",
        FOG_VOLUME_PROFILE_EXPLOSION_OIL,
    ),
    (
        "FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR",
        FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR,
    ),
    (
        "FOG_VOLUME_PROFILE_LIGHT_SHAFT",
        FOG_VOLUME_PROFILE_LIGHT_SHAFT,
    ),
];

// Main-pass ray-query decomposition. The runtime `DBG_DISABLE_*` bits below
// preserve the compiled shader's register allocation and isolate avoided
// execution. `RT_COMPILE_ABLATION_MASK` selects the same feature groups at
// shader compile time so glslang/the driver may eliminate dead code and lower
// register pressure. Shipping builds keep the mask at zero.
pub const RT_ABLATION_DIRECT_SHADOW: u32 = 1 << 0;
pub const RT_ABLATION_GI: u32 = 1 << 1;
pub const RT_ABLATION_REFLECTION_GLASS: u32 = 1 << 2;
pub const RT_ABLATION_ALL_RAYS: u32 = 1 << 3;
pub const RT_COMPILE_ABLATION_MASK: u32 = 0;

// Structured, mutually-exclusive renderer correctness views. These values
// ride `GpuCamera.render_debug.x`; the legacy all-bits sentinel preserves the
// launch-time `BYROREDUX_RENDER_DEBUG` categorical selectors until an operator
// explicitly chooses a named runtime mode.
pub const RENDER_DEBUG_FINAL: u32 = 0;
pub const RENDER_DEBUG_SHADOW_VISIBILITY: u32 = 1;
pub const RENDER_DEBUG_SELECTED_LIGHT: u32 = 2;
pub const RENDER_DEBUG_DIRECT_ONLY: u32 = 3;
pub const RENDER_DEBUG_INDIRECT_ONLY: u32 = 4;
pub const RENDER_DEBUG_MATERIAL_LOBE: u32 = 5;
pub const RENDER_DEBUG_COMPOSITE_TERM: u32 = 6;
pub const RENDER_DEBUG_RT_LOD: u32 = 7;
pub const RENDER_DEBUG_VOLUMETRIC_TERM: u32 = 8;
pub const RENDER_DEBUG_MATERIAL_ROLE: u32 = 9;
// Water-pass oracles (WATAL W2/W3). `WATER_TERM` packs R = refraction ray
// missed, G = foam mask, B = output coverage; `WATER_NORMAL` is the
// perturbed shading normal; `WATER_REFL` is the reflection term alone
// (post-magnitude, post-Fresnel-mix). Every non-water surface renders a
// flat dim grey so the water footprint reads unambiguously.
pub const RENDER_DEBUG_WATER_TERM: u32 = 10;
pub const RENDER_DEBUG_WATER_NORMAL: u32 = 11;
pub const RENDER_DEBUG_TERRAIN_LOD: u32 = 12;
pub const RENDER_DEBUG_WATER_REFL: u32 = 13;
// Shadow-diagnostic views (2026-09-21 single-sided-wall light-leak
// investigation). `FACING_RATIO` paints dot(geometricNormal, V): green-grey
// where the geometric normal faces the camera, RED where it faces away —
// the class where authored/inverted normals send shadow-ray origin offsets
// behind the surface. `RESTIR_LIGHT` paints the per-pixel ReSTIR-selected
// light index as a stable categorical hue (black = no selection) so leaks
// name the light responsible.
pub const RENDER_DEBUG_FACING_RATIO: u32 = 14;
pub const RENDER_DEBUG_RESTIR_LIGHT: u32 = 15;
// `RESTIR_HISTORY` paints the direct-light shadow-ratio history per pixel:
// grey = history length over the active cap (white = full window), green
// tint = the deep scene-static tail, RED = history rejected this frame (one
// raw shadow ray), BLUE = a blended decal borrowing its surface's history.
pub const RENDER_DEBUG_RESTIR_HISTORY: u32 = 16;
pub const RENDER_DEBUG_MODE_MAX: u32 = RENDER_DEBUG_RESTIR_HISTORY;
pub const RENDER_DEBUG_LEGACY_FLAGS: u32 = u32::MAX;

pub const RENDER_DEBUG_MODES: &[(&str, u32)] = &[
    ("RENDER_DEBUG_FINAL", RENDER_DEBUG_FINAL),
    (
        "RENDER_DEBUG_SHADOW_VISIBILITY",
        RENDER_DEBUG_SHADOW_VISIBILITY,
    ),
    ("RENDER_DEBUG_SELECTED_LIGHT", RENDER_DEBUG_SELECTED_LIGHT),
    ("RENDER_DEBUG_DIRECT_ONLY", RENDER_DEBUG_DIRECT_ONLY),
    ("RENDER_DEBUG_INDIRECT_ONLY", RENDER_DEBUG_INDIRECT_ONLY),
    ("RENDER_DEBUG_MATERIAL_LOBE", RENDER_DEBUG_MATERIAL_LOBE),
    ("RENDER_DEBUG_COMPOSITE_TERM", RENDER_DEBUG_COMPOSITE_TERM),
    ("RENDER_DEBUG_RT_LOD", RENDER_DEBUG_RT_LOD),
    ("RENDER_DEBUG_VOLUMETRIC_TERM", RENDER_DEBUG_VOLUMETRIC_TERM),
    ("RENDER_DEBUG_MATERIAL_ROLE", RENDER_DEBUG_MATERIAL_ROLE),
    ("RENDER_DEBUG_WATER_TERM", RENDER_DEBUG_WATER_TERM),
    ("RENDER_DEBUG_WATER_NORMAL", RENDER_DEBUG_WATER_NORMAL),
    ("RENDER_DEBUG_TERRAIN_LOD", RENDER_DEBUG_TERRAIN_LOD),
    ("RENDER_DEBUG_WATER_REFL", RENDER_DEBUG_WATER_REFL),
    ("RENDER_DEBUG_FACING_RATIO", RENDER_DEBUG_FACING_RATIO),
    ("RENDER_DEBUG_RESTIR_LIGHT", RENDER_DEBUG_RESTIR_LIGHT),
    ("RENDER_DEBUG_RESTIR_HISTORY", RENDER_DEBUG_RESTIR_HISTORY),
    ("RENDER_DEBUG_MODE_MAX", RENDER_DEBUG_MODE_MAX),
    ("RENDER_DEBUG_LEGACY_FLAGS", RENDER_DEBUG_LEGACY_FLAGS),
];

// Debug-viz bit flags packed into `jitter.z` by the renderer
// (`parse_render_debug_flags_env` + `GpuCamera` upload). Runtime-set
// via `BYROREDUX_RENDER_DEBUG=<bitmask>` env var or console for
// renderer-artifact bisection. Branches collapse to free no-ops when
// the env var is unset. Consumed by `triangle.frag` via the `#include`d
// `#define`s; this file is the single source of truth.

/// 0x1 — bypass parallax-occlusion mapping in the base-UV sampler.
pub const DBG_BYPASS_POM: u32 = 0x1;

/// 0x2 — bypass detail-map blend on the base albedo. Gated on the primary
/// combine in `triangle.frag` AND on every secondary-ray terminus through
/// `rayHitAlbedo` (reflection / GI / refraction / water-ray), so a
/// detail-map A/B shows the term's full contribution.
pub const DBG_BYPASS_DETAIL: u32 = 0x2;

/// 0x4 — visualize per-fragment world-space normal as colour.
pub const DBG_VIZ_NORMALS: u32 = 0x4;

/// 0x8 — visualize per-fragment tangent presence:
///   * green = tangent present (vertex shader fed authored or synthesized
///     data → Path 1 in `perturbNormal` fires).
///   * red = zero tangent → screen-space derivative fallback (Path 2).
///
/// Added under #783 follow-up.
pub const DBG_VIZ_TANGENT: u32 = 0x8;

/// 0x10 — skip the per-fragment normal-map perturbation entirely;
/// lighting uses the geometric vertex normal. Use to bisect whether a
/// chrome / posterization artifact originates from `perturbNormal`
/// (Path 1 or Path 2 TBN bug) or from downstream specular / ambient
/// code. Default-on path runs `perturbNormal`; this bit is the opt-out.
/// 2026-05-03 / #786 closeout reinstated the default-on behaviour after
/// the convention-swap fix at 5dde345 + the BSTriShape inline-tangent
/// decode at b63ab0c.
pub const DBG_BYPASS_NORMAL_MAP: u32 = 0x10;

/// 0x20 — RESERVED. Pre-#1035 (in the 77aa2de → 5dde345 window) this
/// bit was the opt-IN for `perturbNormal` while the default was off
/// (was named `DBG_FORCE_NORMAL_MAP`). After #786 closed (2026-05-03)
/// the default flipped back to on and the bit became a silent no-op.
/// Kept reserved so existing diagnostic scripts using
/// `BYROREDUX_RENDER_DEBUG=0x20` / `0x24` / `0x28` keep working as
/// no-ops; renamed at #1035 to make the no-op status explicit in the
/// bit catalog.
pub const DBG_RESERVED_20: u32 = 0x20;

/// 0x40 — visualize the per-entity content-class render layer driving
/// the depth-bias ladder. Tints fragments by layer:
///   * Architecture (0) → grey
///   * Clutter (1)      → cyan
///   * Actor (2)        → magenta
///   * Decal (3)        → yellow
///
/// The 2-bit layer is packed into `gpuInstance.flags` bits 4..5
/// (`INSTANCE_RENDER_LAYER_SHIFT` / `_MASK` on the Rust side).
pub const DBG_VIZ_RENDER_LAYER: u32 = 0x40;

/// 0x80 — glass IOR refraction passthru-loop diagnostic (#789
/// follow-up). Tints glass fragments by where the loop terminated:
///   * black   — IOR not allowed (thin glass, RT globally disabled, or an
///     architectural window whose portal classification remained valid).
///   * red     — IOR fired but ray escaped scene (sky fallback).
///   * yellow  — terminated on first hit, no passthru (different texture
///     from start — desk / wall / non-glass behind the surface).
///   * green   — passthru ×1, then non-self terminus (one self skip,
///     then real scene geometry).
///   * cyan    — passthru ×2 with non-self terminus (two self skips +
///     real geometry, e.g. through one stacked beaker to wall behind).
///   * magenta — interface allowance exhausted, terminus STILL glass
///     (passthru never escaped the overlapping surfaces at this tier).
pub const DBG_VIZ_GLASS_PASSTHRU: u32 = 0x80;

/// 0x100 — disable specular antialiasing (`specularAaRoughness`).
/// Every per-light + RT-reflection BRDF site widens the authored
/// `roughness` by the screen-space normal-variance kernel before
/// feeding it to GGX/Smith. Setting this bit returns to the raw
/// authored roughness so the Kaplanyan-Hoffman 2016 bug-class
/// (corrugated normal map → bright/dark stripes at distance) can be
/// A/B'd against a regression suspect that turns out to be the spec-AA
/// itself. Default-on; this bit is the opt-out.
pub const DBG_DISABLE_SPECULAR_AA: u32 = 0x100;

/// 0x200 — display the raw SSAO occlusion value the main pass samples,
/// before the 0.2 floor, before DALC/ambient floors, and before it
/// multiplies anything. White = no occlusion; dark = occluded. Recycled
/// from the reserved slot (formerly the interior-only isotropic
/// directional-fill disable, retired when interior and exterior
/// directionals converged on the standard BRDF + RT-shadow contract).
/// The instrument that answers "is SSAO finding occlusion at all" — a
/// tone-diff A/B against `DBG_DISABLE_AO` cannot distinguish "AO applies
/// but the texture is white everywhere" from "AO applies and changes
/// nothing visible", which is exactly the ambiguity hit while auditing
/// AO coverage on the Cornell scene.
pub const DBG_VIZ_AO: u32 = 0x200;

/// 0x400 — bypass the per-vertex color modulation of albedo
/// (`albedo *= fragColor`). Bethesda bakes per-vertex lighting / AO into
/// static-geometry vertex colors; on coarsely-tessellated meshes (e.g.
/// FNV casino floor tiles — `NV_TOPS_CasLoRmMid01` carries vertex-luma
/// 0.16–1.00 over only 40 verts) that baked term interpolates across
/// large triangles into hard-edged bright/dark patches whose boundaries
/// follow the triangulation. Set this bit to confirm a "lighting error
/// only on certain polygons, at a fixed position" is the baked
/// vertex-color term rather than a dynamic / RT-side cause (Tops floor
/// diagnosis 2026-05-27). Does not affect `SOURCE_EMISSIVE` vertex mode
/// (that path routes vertex color through the emissive accumulator).
pub const DBG_BYPASS_VERTEX_COLOR: u32 = 0x400;

/// 0x800 — force ambient occlusion to 1.0 (disable both the screen-space
/// SSAO sample and the RT-AO term in `combinedAO`). Use to bisect whether
/// a hard-edged dark floor patch is AO over-darkening (vanishes with this
/// bit) versus a cast shadow / direct-light occlusion (persists). Paired
/// with `DBG_BYPASS_VERTEX_COLOR` these isolate the two most common
/// "lighting only on certain polygons" causes without touching shadows.
pub const DBG_DISABLE_AO: u32 = 0x800;

/// 0x1000 — revert point/spot lights to the pre-REND-#1451 attenuation:
/// the anti-pop-in cull window doing the ENTIRE attenuation job
/// (`atten = pow(clamp(1 − (d/R)², 0, 1), shape)`, `R = .w`). That
/// formula reads 75% at the authored radius (`d = R/2`) — the bright
/// near-zone ring (Lonesome Road / Ulysses Temple). Default-off path
/// now uses the OpenMW-style two-term model: a physical near-zone
/// falloff keyed to the AUTHORED radius (`knee = dofParams.z × .w`)
/// MULTIPLIED by a soft cull window that fades full→zero from the
/// authored radius out to `.w`. Set this bit to A/B the new model
/// against the legacy one in the same live session (no rebuild) while
/// running the REND-#1451 controlled bench. Also settable via the
/// `light.atten legacy on|off` console command (routes through the
/// `LightTuning` resource → `VulkanContext::light_atten_legacy`).
pub const DBG_LEGACY_LIGHT_ATTEN: u32 = 0x1000;

/// 0x2000 — disable multi-scatter energy compensation
/// (`multiScatterEnergyCompensation`, Fdez-Agüera 2019 / Filament). The
/// default-on path multiplies the single-scatter Cook-Torrance specular
/// lobe by `1 + F0·(1/Ess − 1)` to restore the energy lost to microfacet
/// masking as roughness rises — without it, rough conductors (brushed
/// steel, satin, cookware) progressively darken. The factor is a no-op at
/// low roughness (`Ess → 1`), so it cannot shift the RT reflection
/// roughness gate. Set this bit to A/B the compensated rough metal
/// against the legacy single-scatter look in one live session.
pub const DBG_DISABLE_MULTISCATTER: u32 = 0x2000;

/// 0x4000 — disable the SVGF spatial à-trous wavelet pass
/// (`svgf_atrous.comp`, Schied 2017 §4.3). The default-on path runs the
/// variance-guided edge-stopping wavelet filter after temporal
/// accumulation to remove the per-pixel GI variance the temporal pass
/// leaves behind (the noisy / slow-moiré floor). Setting this bit turns
/// every à-trous iteration into a pass-through copy, so the composite
/// samples the raw temporal-only result — the pre-Phase-4 look — for live
/// A/B in one session.
pub const DBG_DISABLE_ATROUS: u32 = 0x4000;

/// 0x8000 — disable ReSTIR-DI reservoir reuse for a trustworthy raw-direct
/// ablation. In the default production build (`ENABLE_LEGACY_WRS == 0`) this
/// keeps the current-frame streaming-RIS reservoir and its final visibility
/// sample, but disables temporal selection reuse, spatial neighbour reuse,
/// and the direct-radiance EMA. The result is the noisy one-frame direct-light
/// estimator needed to separate transport defects from reuse artifacts.
///
/// In an evaluation build with `ENABLE_LEGACY_WRS == 1`, the same bit still
/// selects the legacy 16-slot WRS arm for historical A/B captures. It is never
/// a no-op: changing its production meaning silently would invalidate the
/// fixed-camera lighting ladder.
pub const DBG_DISABLE_RESTIR: u32 = 0x8000;

/// 0x10000 — disable ReSTIR-DI **spatial** reservoir reuse (ReSTIR "P2",
/// Bitterli 2020 §5) while leaving the temporal reuse (`DBG_DISABLE_RESTIR`
/// path) active. The default-on path samples a small disk of neighbour
/// reservoirs from the *previous* frame's buffer around the reprojected
/// pixel, re-evaluates each neighbour's selected light against the **current**
/// surface (target pdf p̂), and combines them with the same 1/M streaming-RIS
/// estimator the temporal path uses — so a freshly disoccluded or fast-moving
/// pixel inherits many effective samples from its neighbourhood instead of
/// restarting from a single noisy frame. It also seeds the soft-shadow colour
/// EMA from valid neighbours on disocclusion (where temporal reprojection
/// fails), which is what visibly removes the "convergence resets on camera
/// motion" restart noise. Set this bit to A/B temporal-only ReSTIR against the
/// full spatiotemporal path in one live session.
pub const DBG_DISABLE_SPATIAL: u32 = 0x10000;

/// 0x20000 — #1874 diagnostic: visualise the per-fragment screen-space motion
/// vector (`outMotion`, the G-buffer velocity SVGF + TAA reproject with) as
/// colour, so the "ghosted diagonal double-image" can be root-caused live
/// without a RenderDoc capture. Encoding: `rg = 0.5 + motion.xy * scale`,
/// `b = 0.5` — a static camera reads flat grey `(0.5, 0.5, 0.5)` everywhere.
///
/// The decisive read for issue #1874's hypothesis H1 (a *spatially-uniform*
/// bad motion vector shared by SVGF and TAA): under this view a real camera
/// translation shows motion that **varies with depth** (near geometry tints
/// harder than far — parallax), whereas the suspected fault paints the **whole
/// screen one uniform non-grey tint** (a post-projection screen shift with no
/// depth dependence — a stale/jittered `prevViewProj`, not real motion). Park
/// the camera on the artifact and set this bit: uniform tint ⇒ camera-level
/// (`prevViewProj`/origin), depth-varying-but-localised-to-a-body tint ⇒
/// skinning. Diagnostic-only — gated entirely behind the debug bit, no effect
/// on normal rendering.
pub const DBG_VIZ_MOTION: u32 = 0x20000;

/// 0x40000 — disable ReSTIR-DI **temporal** reservoir and radiance-history
/// reuse while leaving current-frame sampling and (unless separately disabled
/// by [`DBG_DISABLE_SPATIAL`]) previous-frame spatial-neighbour reuse active.
/// This separates the two reuse dimensions for controlled evaluation:
///
/// - default: temporal + spatial reuse;
/// - `DBG_DISABLE_SPATIAL`: temporal-only;
/// - `DBG_DISABLE_TEMPORAL`: spatial-only;
/// - both bits: current-frame reservoir only.
///
/// Unlike `DBG_DISABLE_RESTIR`, this never selects the compile-time-gated
/// legacy WRS implementation. It keeps the same ReSTIR estimator and only
/// removes the centre-pixel temporal candidate plus its colour EMA history.
pub const DBG_DISABLE_TEMPORAL: u32 = 0x40000;

/// 0x80000 — display the fragment shader's resolved indirect-light signal
/// directly, before SVGF history and before multiplication by local albedo in
/// composite. This distinguishes "GI rays returned darkness" from
/// "denoising/compositing/exposure buried valid indirect energy".
pub const DBG_VIZ_RAW_INDIRECT: u32 = 0x80000;

/// 0x100000 — display final raster material classification. Opaque surfaces
/// are grey, alpha-tested surfaces green, alpha-blended surfaces red, and
/// glass blue. The diagnostic writes alpha=1 so blend-state membership is
/// visible as a solid classification rather than being obscured by the very
/// transparency defect under investigation.
pub const DBG_VIZ_MATERIAL_STATE: u32 = 0x100000;

/// 0x200000 — display only the stochastic ray-traced diffuse GI bounce,
/// excluding authored cell ambient, AO, reflections, direct light, and SVGF.
/// This is the decisive probe for whether the GI ray estimator contributes
/// energy in a real-content scene.
pub const DBG_VIZ_GI_BOUNCE: u32 = 0x200000;

/// 0x400000 — display the projection jitter and FSR reset contract as a
/// uniform diagnostic colour. Red/green encode the exact render-pixel jitter
/// mapped from `[-0.5, 0.5]` to `[0, 1]`; blue is 1 while an FSR history reset
/// is pending and 0 otherwise. This makes projection/dispatch phase drift and
/// forgotten camera-cut resets visible without a RenderDoc capture.
pub const DBG_VIZ_FSR_TEMPORAL: u32 = 0x400000;

/// 0x800000 — #2218 diagnostic: bisect which shading term first goes
/// non-finite (NaN/Inf). FO3 Megaton's exterior geometry saturates to pure
/// white and survives a 42x exposure crush untouched — a value that ignores
/// a 42x reduction is non-finite, not merely bright (`ACES(Inf) → 1.0`;
/// `NaN` propagates to white through tone mapping). Checks, upstream to
/// downstream: `indirect` (the raw stochastic GI bounce, pre-ambient/AO) →
/// `indirectLight` (adds authored ambient + AO) → `directLight` (the
/// ReSTIR-shaded, shadow-ray-gated direct term + emissive) — the terms that
/// feed the final composite. A fault reports at the earliest term it
/// reaches, since a non-finite value there also poisons every downstream sum.
///   magenta = `indirect` (GI bounce) non-finite
///   yellow  = `indirectLight` (ambient + GI + AO) non-finite
///   red     = `directLight` (direct + shadow + emissive) non-finite
///   green   = every checked term finite at this fragment
pub const DBG_VIZ_NONFINITE: u32 = 0x800000;

/// 0x1000000 — display the scale-aware RT ray-origin offset magnitude as
/// false colour. Blue is approximately 2^-16 world units (the near-origin
/// additive fallback); red is approximately 2^1 world units at large float
/// exponents. A surface that still shows uniform speckle while this view is
/// warm-coloured is not suffering from an undersized self-hit epsilon.
pub const DBG_VIZ_SHADOW_OFFSET: u32 = 0x1000000;

/// 0x2000000 — display the angle between the actual triangle-plane normal
/// used by RT visibility and the fully resolved shading normal. Green is
/// aligned; red is 90 degrees apart. A band localised to warm/red geometry
/// is the shadow-terminator class, not floating-point self-intersection.
pub const DBG_VIZ_NORMAL_DIVERGENCE: u32 = 0x2000000;

/// 0x4000000 — display only the resolved direct-light attachment, including
/// emissive but excluding authored ambient/GI, SVGF, and albedo remodulation.
/// Pair with [`DBG_DISABLE_RESTIR`] for the raw one-frame direct-transport rung
/// of the lighting ablation ladder.
pub const DBG_VIZ_DIRECT: u32 = 0x4000000;

/// 0x8000000 — skip primary direct-light shadow queries while retaining the
/// unshadowed light estimator. GI/reflection hit lighting can still issue its
/// own visibility queries; this bit isolates the shipping direct-shadow path.
pub const DBG_DISABLE_DIRECT_SHADOWS: u32 = 0x8000000;

/// 0x10000000 — skip the bounded diffuse GI path. Authored ambient, direct
/// lighting, reflections, SSAO, and every non-GI pass remain active.
pub const DBG_DISABLE_GI_RAYS: u32 = 0x10000000;

/// 0x20000000 — skip main-pass window portal, fire-refraction, glass IOR, and
/// glossy/metal reflection queries. Glass keeps its zero-ray Fresnel fallback.
pub const DBG_DISABLE_REFLECTION_GLASS_RAYS: u32 = 0x20000000;

/// 0x40000000 — skip every ray-query path in `triangle.frag`. TLAS build and
/// ray-query work in other pipelines remain intact, so baseline→this bit is
/// the main-pass ray budget rather than a whole-frame RT-off measurement.
pub const DBG_DISABLE_ALL_MAIN_RAYS: u32 = 0x40000000;

/// 0x80000000 — false-colour the final ReSTIR-selected light index. A stable
/// integer hash maps each SSBO index to a distinct colour; black means the
/// fragment had no valid reservoir selection. This makes a selected-index /
/// shadow-ray payload mismatch visible without conflating it with BRDF,
/// visibility, denoising, or composite energy.
pub const DBG_VIZ_SELECTED_LIGHT: u32 = 0x80000000;

/// Compound selector (not a new bit): false-colour the material shading lobe
/// while preserving every legacy one-bit debug flag. Enable it by setting both
/// `DBG_VIZ_MATERIAL_STATE` and `DBG_VIZ_SELECTED_LIGHT`; the shader checks
/// this exact combination before either constituent view.
pub const DBG_VIZ_MATERIAL_LOBES: u32 = DBG_VIZ_MATERIAL_STATE | DBG_VIZ_SELECTED_LIGHT;

/// Compound selector (not a new bit): false-colour the continuous `rtLOD`
/// value used by reflection/GI and portal-cost gates. Glass identity and thick
/// transmission are deliberately not distance-gated. Enable both bits.
/// This is the measurement surface required before retuning RT_LOD_SCALE.
pub const DBG_VIZ_RT_LOD: u32 = DBG_VIZ_MATERIAL_STATE | DBG_VIZ_GI_BOUNCE;

/// Compound selector (not a new bit): display the final selected direct-light
/// visibility before BRDF, ReSTIR weight, temporal accumulation, indirect,
/// composite, exposure, or tone mapping. Greyscale is luminance of the
/// material-aware RGB transmittance (black=blocked, white=fully visible);
/// magenta means no valid reservoir sample/ray existed for the fragment, so
/// "occluded" can never be confused with "nothing selected".
pub const DBG_VIZ_SHADOW_VISIBILITY: u32 = DBG_VIZ_SELECTED_LIGHT | DBG_VIZ_DIRECT;

/// Debug-viz views that are correctness oracles, keyed by **any one bit**:
/// if the flag word carries any of these, raw output is required.
///
/// #2978 — this catalog plus [`DBG_VIZ_RAW_OUTPUT_ALL`] is the single source
/// of truth for the raw-output policy. `build.rs` folds it into the
/// `DBG_VIZ_RAW_OUTPUT_ANY_MASK` / `DBG_VIZ_REQUIRES_RAW_OUTPUT(flags)`
/// header emit that `composite.frag` and `presentation.frag` consume, and
/// `shader_constants::debug_viz_requires_raw_output` evaluates the same two
/// catalogs in Rust. Before that the policy was hand-written once in Rust and
/// twice in GLSL, held together only by a four-literal `source.contains`
/// subset check — so a fifth view could land in Rust while both shaders went
/// on tone-mapping the oracle, with the whole suite green.
///
/// Note the compound views need no entry here: `DBG_VIZ_SHADOW_VISIBILITY`
/// and `DBG_VIZ_MATERIAL_LOBES` both contain `DBG_VIZ_SELECTED_LIGHT`, so
/// they already match on that bit.
pub const DBG_VIZ_RAW_OUTPUT_ANY: &[(&str, u32)] = &[
    ("DBG_VIZ_SELECTED_LIGHT", DBG_VIZ_SELECTED_LIGHT),
    ("DBG_VIZ_DIRECT", DBG_VIZ_DIRECT),
    ("DBG_VIZ_RAW_INDIRECT", DBG_VIZ_RAW_INDIRECT),
    ("DBG_VIZ_AO", DBG_VIZ_AO),
];

/// Correctness-oracle views keyed by an **exact compound**: every bit in the
/// mask must be set, because the constituent bits individually select a
/// different (non-oracle) view. `DBG_VIZ_RT_LOD` is `MATERIAL_STATE |
/// GI_BOUNCE`, and neither of those alone requires raw output.
///
/// Companion to [`DBG_VIZ_RAW_OUTPUT_ANY`]; see that constant for the #2978
/// rationale.
pub const DBG_VIZ_RAW_OUTPUT_ALL: &[(&str, u32)] = &[("DBG_VIZ_RT_LOD", DBG_VIZ_RT_LOD)];

/// Folded [`DBG_VIZ_RAW_OUTPUT_ANY`] mask. A `const fn` rather than a
/// `pub const DBG_…: u32` so it stays out of the `DBG_*` *bit* census that
/// `dbg_bits_catalog_covers_every_dbg_constant` counts — it is derived from
/// bits, not one of them.
pub const fn dbg_viz_raw_output_any_mask() -> u32 {
    let mut mask = 0u32;
    let mut i = 0;
    while i < DBG_VIZ_RAW_OUTPUT_ANY.len() {
        mask |= DBG_VIZ_RAW_OUTPUT_ANY[i].1;
        i += 1;
    }
    mask
}

/// Single source of truth for every `DBG_*` debug-viz bit, in emit order.
/// Both `build.rs` (GLSL header emit) and `shader_constants.rs`'s test
/// module (`generated_header_contains_all_defines` value-pin,
/// `triangle_frag_dbg_bits_not_redeclared` no-shadow guard, and
/// `dbg_bits_catalog_covers_every_dbg_constant`) drive off this single list,
/// so a new `DBG_*` constant can no longer land covered by only one (or
/// zero) of those three contracts. Pre-#1860 `build.rs` hand-emitted each
/// `DBG_*` `writeln!` separately and this catalog (then test-only, living in
/// `shader_constants.rs`) had drifted to 13 of 18 constants — the 5 newest
/// bits (`DBG_DISABLE_MULTISCATTER`/`ATROUS`/`RESTIR`/`SPATIAL`,
/// `DBG_VIZ_MOTION`) bypassed both the value-pin and the no-redeclare guard
/// silently. See #1482 (original catalog fix) / #1860 (this fix, moving the
/// catalog here so `build.rs` can drive its emit from it too).
///
/// **The `u32` is full.** #3563 — the 32 single-bit `DBG_*` constants cover
/// bits 0-31 with no gaps, so there is no free bit to allocate and a 33rd
/// bit could only be written as a duplicate of an existing value.
/// `dbg_bits_are_single_bit_and_pairwise_disjoint` (`shader_constants.rs`)
/// is what catches that; the pre-existing count-parity guard cannot, because
/// it compares entry counts, never values.
///
/// One recyclable slot remains for exactly this purpose:
/// [`DBG_RESERVED_20`] (bit 5) — rename it in place rather than inventing a
/// value. (Bit 9 was recycled into [`DBG_VIZ_AO`].) Past that, future debug
/// expansion must coordinate with the history-dependent weather-surface
/// payload already carried in `GpuCamera.render_debug.w`; that lane is no
/// longer an unused flag word.
///
/// The last three entries are compound unions of bits already listed above
/// them, not bits of their own; the uniqueness guard skips them by name.
pub const DBG_BITS: &[(&str, u32)] = &[
    ("DBG_BYPASS_POM", DBG_BYPASS_POM),
    ("DBG_BYPASS_DETAIL", DBG_BYPASS_DETAIL),
    ("DBG_VIZ_NORMALS", DBG_VIZ_NORMALS),
    ("DBG_VIZ_TANGENT", DBG_VIZ_TANGENT),
    ("DBG_BYPASS_NORMAL_MAP", DBG_BYPASS_NORMAL_MAP),
    ("DBG_RESERVED_20", DBG_RESERVED_20),
    ("DBG_VIZ_RENDER_LAYER", DBG_VIZ_RENDER_LAYER),
    ("DBG_VIZ_GLASS_PASSTHRU", DBG_VIZ_GLASS_PASSTHRU),
    ("DBG_DISABLE_SPECULAR_AA", DBG_DISABLE_SPECULAR_AA),
    ("DBG_VIZ_AO", DBG_VIZ_AO),
    ("DBG_BYPASS_VERTEX_COLOR", DBG_BYPASS_VERTEX_COLOR),
    ("DBG_DISABLE_AO", DBG_DISABLE_AO),
    ("DBG_LEGACY_LIGHT_ATTEN", DBG_LEGACY_LIGHT_ATTEN),
    ("DBG_DISABLE_MULTISCATTER", DBG_DISABLE_MULTISCATTER),
    ("DBG_DISABLE_ATROUS", DBG_DISABLE_ATROUS),
    ("DBG_DISABLE_RESTIR", DBG_DISABLE_RESTIR),
    ("DBG_DISABLE_SPATIAL", DBG_DISABLE_SPATIAL),
    ("DBG_VIZ_MOTION", DBG_VIZ_MOTION),
    ("DBG_DISABLE_TEMPORAL", DBG_DISABLE_TEMPORAL),
    ("DBG_VIZ_RAW_INDIRECT", DBG_VIZ_RAW_INDIRECT),
    ("DBG_VIZ_MATERIAL_STATE", DBG_VIZ_MATERIAL_STATE),
    ("DBG_VIZ_GI_BOUNCE", DBG_VIZ_GI_BOUNCE),
    ("DBG_VIZ_FSR_TEMPORAL", DBG_VIZ_FSR_TEMPORAL),
    ("DBG_VIZ_NONFINITE", DBG_VIZ_NONFINITE),
    ("DBG_VIZ_SHADOW_OFFSET", DBG_VIZ_SHADOW_OFFSET),
    ("DBG_VIZ_NORMAL_DIVERGENCE", DBG_VIZ_NORMAL_DIVERGENCE),
    ("DBG_VIZ_DIRECT", DBG_VIZ_DIRECT),
    ("DBG_DISABLE_DIRECT_SHADOWS", DBG_DISABLE_DIRECT_SHADOWS),
    ("DBG_DISABLE_GI_RAYS", DBG_DISABLE_GI_RAYS),
    (
        "DBG_DISABLE_REFLECTION_GLASS_RAYS",
        DBG_DISABLE_REFLECTION_GLASS_RAYS,
    ),
    ("DBG_DISABLE_ALL_MAIN_RAYS", DBG_DISABLE_ALL_MAIN_RAYS),
    ("DBG_VIZ_SELECTED_LIGHT", DBG_VIZ_SELECTED_LIGHT),
    ("DBG_VIZ_MATERIAL_LOBES", DBG_VIZ_MATERIAL_LOBES),
    ("DBG_VIZ_RT_LOD", DBG_VIZ_RT_LOD),
    ("DBG_VIZ_SHADOW_VISIBILITY", DBG_VIZ_SHADOW_VISIBILITY),
];

/// #1799 / PERF-D5-NEW-01 — compile-time gate for the legacy 16-slot WRS
/// reservoir arrays (`resLight[16]` / `resWSel[16]`) that `DBG_DISABLE_RESTIR`
/// A/Bs against. `DBG_DISABLE_RESTIR` is a RUNTIME bit read from a uniform, so
/// even on the ~100% of production frames that take the ReSTIR path and never
/// touch those arrays, the compiler still had to budget their per-invocation
/// register / local-memory footprint — the declaration + init loop ran
/// unconditionally, ahead of the runtime `useRestir` branch that gated
/// everything else about them. glslangValidator's preprocessor, unlike the
/// runtime branch, can actually eliminate dead code — but only if the
/// legacy-WRS source text is behind a `#if`, not an `if` on a uniform value.
///
/// `0` (default): the legacy WRS arm — declarations, streaming writes, and
/// pass-2 shadow-ray reads — is preprocessed OUT of `triangle.frag` entirely.
/// `DBG_DISABLE_RESTIR` then disables all history/neighbour reuse while
/// retaining the current-frame reservoir, so the diagnostic remains useful
/// without reintroducing the legacy array footprint.
///
/// `1`: restores the pre-fix behavior verbatim — the legacy arrays exist and
/// `DBG_DISABLE_RESTIR` again live-toggles between the two paths at runtime.
/// Flip this to `1` and recompile `triangle.frag` to A/B; per the mechanism
/// #1758 established for `SKIN_WORKGROUP_SIZE`, that A/B now costs a shader
/// recompile instead of a per-frame register tax on every production build.
pub const ENABLE_LEGACY_WRS: u32 = 0;

// ---------------------------------------------------------------------------
// Generated-header emit table (#5097)
//
// The three-way hand-typing this table retires: a constant was declared here,
// hand-emitted by a `writeln!` in `build.rs` (choosing the `u` suffix or the
// `{:?}` float format by hand), and re-typed a third time in
// `shader_constants.rs`'s pin list — where a wrong suffix typed twice passed
// both gates. Now `SHADER_DEFINES` below is the one place the header's lines
// are declared, `build.rs` walks it, and the pin test re-renders it and
// compares the committed `shader_constants.glsl` byte for byte.
//
// Adding a constant is one edit plus one table line: declare it above, add
// its `shader_define!(Kind NAME)` row in its section (the sections and their
// order ARE the header), rebuild. The table-covers-all test in
// `shader_constants.rs` fails if the declaration landed without its row.
//
// #5808 — the generator (`ShaderValue`, `HeaderLine`, the header renderer and
// the row macros) lives in `shader_constants_gen.rs`, `include!`d ahead of this
// file by both consumers, so this file is the data alone. A row names its
// constant once: the macro `stringify!`s the header name from the identifier
// the value reads, so no row can emit one constant's value under another's
// name.

/// Every line of `shaders/include/shader_constants.glsl`, in emit order —
/// sections, comments and all. The table is the header.
pub const SHADER_DEFINES: &[HeaderLine] = &[
    HeaderLine::Blank,
    HeaderLine::Comment("Depth-buffer convention (#3308)"),
    shader_define!(UintPlain BYRO_REVERSED_Z),
    shader_define!(Float BYRO_DEPTH_CLEAR),
    HeaderLine::Blank,
    HeaderLine::Comment("Rec. 709 luma weights, linear sRGB (#4347)"),
    shader_define!(Vec3 LUMA_REC709),
    HeaderLine::Blank,
    HeaderLine::Comment("Cluster grid"),
    shader_define!(Uint CLUSTER_TILES_X),
    shader_define!(Uint CLUSTER_TILES_Y),
    shader_define!(Uint CLUSTER_SLICES_Z),
    shader_define!(Float CLUSTER_NEAR),
    shader_define!(Float CLUSTER_FAR_FLOOR),
    shader_define!(Float CLUSTER_FAR_FALLBACK),
    shader_define!(Uint MAX_LIGHTS_PER_CLUSTER),
    HeaderLine::Blank,
    HeaderLine::Comment("ReSTIR reservoir packing + scene-light upload count"),
    shader_define!(Uint MAX_LIGHTS as u32),
    shader_define!(Uint RESERVOIR_LIGHT_BITS),
    shader_define!(Uint RESERVOIR_LIGHT_MASK),
    shader_define!(Uint RESERVOIR_SURFACE_MASK),
    HeaderLine::Blank,
    HeaderLine::Comment("ReSTIR-DI direct-light EMA caps/floors + camera history mode (#5617)"),
    shader_define!(Float RESTIR_HISTORY_CAP_PARKED),
    shader_define!(Float RESTIR_HISTORY_CAP_MOVING),
    shader_define!(Float RESTIR_ALPHA_FLOOR_PARKED),
    shader_define!(Float RESTIR_ALPHA_FLOOR_MOVING),
    shader_define!(Float RESTIR_SURFACE_NORMAL_COS),
    shader_define!(Float RESTIR_TEMPORAL_DEPTH_FLOOR),
    shader_define!(Float RESTIR_SUBSTRATE_DEPTH_FLOOR),
    shader_define!(Float RESTIR_DEPTH_REL_TOL),
    shader_define!(Float RESTIR_VIEW_COS_FLOOR),
    shader_define!(Float RESTIR_MOVER_SETTLE_FRAMES),
    shader_define!(Float SHADOW_FILTER_NORMAL_POWER),
    shader_define!(Float SHADOW_FILTER_DEPTH_SLOPE_SCALE),
    shader_define!(Float SHADOW_FILTER_RATIO_SIGMA),
    shader_define!(Float SHADOW_FILTER_CONFIDENT_HISTORY),
    shader_define!(Float SHADOW_FILTER_DEPTH_REL_TOL),
    shader_define!(Float HISTORY_MODE_MOVING),
    shader_define!(Float HISTORY_MODE_PARKED),
    shader_define!(Float HISTORY_MODE_SCENE_STATIC),
    shader_define!(Float HISTORY_MODE_RIG_STATIC),
    HeaderLine::Blank,
    HeaderLine::Comment("Ray-query alpha-skip walk budget (#2265 / TD7-001)"),
    shader_define!(Uint MAX_ALPHA_SKIP_LAYERS),
    HeaderLine::Comment("Transmission-lobe self-skip hop budget (#5249)"),
    shader_define!(Uint MAX_TRANSMISSION_SELF_SKIPS),
    HeaderLine::Blank,
    HeaderLine::Comment("RT reach budgets shared between water.frag and triangle.frag (#3745 / TD7-2026-08-30-01)"),
    shader_define!(Float RT_REFLECTION_MAX_DIST),
    shader_define!(Float RT_REFRACTION_MAX_DIST),
    shader_define!(Float RT_DIST_FALLOFF),
    HeaderLine::Blank,
    HeaderLine::Comment("Vertex layout (global SSBO)"),
    shader_define!(Uint VERTEX_STRIDE_FLOATS),
    HeaderLine::Comment("Skinned-vertex output stride — position only (#2170)."),
    shader_define!(Uint SKIN_OUTPUT_STRIDE_FLOATS),
    shader_define!(Uint VERTEX_COLOR_OFFSET_FLOATS),
    shader_define!(Uint VERTEX_NORMAL_OFFSET_FLOATS),
    shader_define!(Uint VERTEX_UV_OFFSET_FLOATS),
    shader_define!(Uint VERTEX_BONE_INDICES_OFFSET_FLOATS),
    shader_define!(Uint VERTEX_BONE_WEIGHTS_OFFSET_FLOATS),
    HeaderLine::Comment("Splat lanes are packed 4x u8 unorm, not floats — recover with"),
    HeaderLine::Comment("unpackUnorm4x8(floatBitsToUint(vertexData[base + N]))."),
    shader_define!(Uint VERTEX_SPLAT0_OFFSET_FLOATS),
    shader_define!(Uint VERTEX_SPLAT1_OFFSET_FLOATS),
    HeaderLine::Blank,
    HeaderLine::Comment("Exterior LAND terrain grid (#4052)"),
    shader_define!(Uint LAND_GRID_VERTS),
    shader_define!(Float LAND_VERTEX_SPACING),
    shader_define!(Float EXTERIOR_CELL_UNITS),
    shader_define!(Float LAND_TEXTURE_TILES_PER_CELL),
    shader_define!(Uint VERTEX_TANGENT_OFFSET_FLOATS),
    HeaderLine::Blank,
    HeaderLine::Comment("Ground-cover chunking + §11.1 sampling bench (#4052)"),
    shader_define!(Float GROUNDCOVER_CHUNK_UNITS),
    shader_define!(Uint GROUNDCOVER_DETAIL_ATLAS_EDGE),
    shader_define!(Float GROUNDCOVER_DETAIL_NORMAL_STRENGTH),
    shader_define!(Uint GROUNDCOVER_CHUNKS_PER_CELL_SIDE),
    shader_define!(UintPlain GROUNDCOVER_BENCH_WORKGROUP),
    shader_define!(Uint GROUNDCOVER_BENCH_BLADE_VERTS),
    HeaderLine::Blank,
    HeaderLine::Comment("Ground-cover density field + scatter (#4054)"),
    shader_define!(Uint GROUNDCOVER_NO_TERRAIN_TILE),
    shader_define!(Float GROUNDCOVER_NO_WATER),
    shader_define!(Float GROUNDCOVER_SLOPE_GATE_START),
    shader_define!(Float GROUNDCOVER_SLOPE_GATE_FULL),
    shader_define!(Float GROUNDCOVER_MOISTURE_FALLOFF_UNITS),
    shader_define!(Float GROUNDCOVER_MOISTURE_FLOOR),
    shader_define!(Float GROUNDCOVER_MOISTURE_SUBMERGED_DEPTH),
    shader_define!(Float GROUNDCOVER_SHELTER_CURVATURE_SCALE),
    shader_define!(Float GROUNDCOVER_SHELTER_STRENGTH),
    shader_define!(Float GROUNDCOVER_CLUMP_CELL_UNITS),
    shader_define!(Float GROUNDCOVER_CLUMP_CONTRAST),
    shader_define!(Float GROUNDCOVER_CLUMP_FLOOR),
    shader_define!(Float GROUNDCOVER_REGION_UNITS),
    shader_define!(Float GROUNDCOVER_REGION_AMPLITUDE),
    shader_define!(Float GROUNDCOVER_CLUMP_MEAN),
    shader_define!(Float GROUNDCOVER_CLUMP_HEIGHT_VARIATION),
    shader_define!(Float GROUNDCOVER_CLUMP_TINT_VARIATION),
    shader_define!(Float GROUNDCOVER_FADE_START),
    shader_define!(Float CLOUD_REF_WIDTH),
    shader_define!(Float CLOUD_TILE_SCALE_LAYER_0),
    shader_define!(Float GROUNDCOVER_DRAW_DISTANCE),
    shader_define!(UintPlain GROUNDCOVER_SCATTER_WORKGROUP),
    shader_define!(UintPlain GROUNDCOVER_MODEL_WORKGROUP),
    shader_define!(Float GROUNDCOVER_WIND_NOISE_UNITS),
    shader_define!(Float GROUNDCOVER_WIND_MAX_BEND),
    shader_define!(Float GROUNDCOVER_WIND_ADVECTION_SCALE),
    shader_define!(Float GROUNDCOVER_WIND_HARMONIC_FREQUENCY_MULTIPLIER),
    shader_define!(Float GROUNDCOVER_WIND_SECONDARY_AMPLITUDE),
    shader_define!(Float GROUNDCOVER_WIND_LATERAL_FRACTION),
    shader_define!(Float GROUNDCOVER_MAX_WIND_SPEED),
    shader_define!(Float GROUNDCOVER_TWO_PI),
    shader_define!(Float GROUNDCOVER_WIND_FLOW_FLOOR),
    shader_define!(Float GROUNDCOVER_WIND_STIFFNESS_ATTENUATION),
    shader_define!(Float GROUNDCOVER_REST_LEAN_BASE),
    shader_define!(Float GROUNDCOVER_REST_LEAN_VARIATION),
    shader_define!(Float GROUNDCOVER_TERRAIN_NORMAL_WEIGHT),
    shader_define!(Float GROUNDCOVER_TWIST_RADIANS),
    shader_define!(Float GROUNDCOVER_MIN_VISIBLE_HALF_WIDTH_PIXELS),
    shader_define!(Float GROUNDCOVER_MAX_WIDTH_MULTIPLIER),
    shader_define!(Float GROUNDCOVER_COLOUR_JITTER_MIN),
    shader_define!(Float GROUNDCOVER_COLOUR_JITTER_MAX),
    shader_define!(Float GROUNDCOVER_BASE_LUMA_RATIO),
    shader_define!(Float GROUNDCOVER_TIP_LUMA_RATIO),
    shader_define!(Float GROUNDCOVER_LUMA_ANCHOR_MAX_GAIN),
    HeaderLine::Blank,
    HeaderLine::Comment("Ground-cover light response (§12.1/12.2/12.5/12.6, #4057)"),
    shader_define!(Float GROUNDCOVER_CANOPY_EXTINCTION_K),
    shader_define!(Float GROUNDCOVER_CANOPY_LEAF_AREA_DENSITY),
    shader_define!(Float GROUNDCOVER_CANOPY_MIN_COS),
    shader_define!(Float GROUNDCOVER_SKY_DIFFUSIVITY),
    shader_define!(Float GROUNDCOVER_BLADE_TRANSMISSION_EXTINCTION),
    shader_define!(Float GROUNDCOVER_TRANSMISSION_DISTORTION),
    shader_define!(Float GROUNDCOVER_TRANSMISSION_POWER),
    shader_define!(Float GROUNDCOVER_SHEEN_F0),
    shader_define!(Float GROUNDCOVER_SHEEN_ROUGHNESS),
    HeaderLine::Blank,
    HeaderLine::Comment("Ground-cover interaction field (§12.4, #4058)"),
    shader_define!(Float GROUNDCOVER_INTERACTION_UNITS),
    shader_define!(Float GROUNDCOVER_INTERACTION_HALF_LIFE_SECONDS),
    shader_define!(Float GROUNDCOVER_INTERACTION_MAX_BEND),
    shader_define!(Uint GROUNDCOVER_INTERACTION_TEXELS),
    shader_define!(Uint GROUNDCOVER_INTERACTION_MAX_DISTURBERS),
    shader_define!(UintPlain GROUNDCOVER_INTERACTION_WORKGROUP),
    shader_define!(Uint GROUNDCOVER_BLADE_SEGMENTS_NEAR),
    shader_define!(Uint GROUNDCOVER_BLADE_SEGMENTS_MID),
    shader_define!(Uint GROUNDCOVER_LOD_TIERS),
    shader_define!(Uint GROUNDCOVER_BLADE_SEGMENTS_FAR),
    shader_define!(Uint GROUNDCOVER_BLADES_PER_POINT_MID),
    shader_define!(Uint GROUNDCOVER_BLADES_PER_POINT_FAR),
    shader_define!(Float GROUNDCOVER_LOD_NEAR_END),
    shader_define!(Float GROUNDCOVER_LOD_MID_END),
    shader_define!(Float GROUNDCOVER_LOD_MORPH_UNITS),
    shader_define!(Float GROUNDCOVER_BLADE_FADE_START),
    shader_define!(Float GROUNDCOVER_FADE_COVERAGE_MAX_GAIN),
    shader_define!(Float GROUNDCOVER_COVERAGE_MAX_WIDEN),
    shader_define!(Uint GROUNDCOVER_BLADES_PER_POINT),
    shader_define!(Uint GROUNDCOVER_SPECIES_TABLE_SIZE),
    shader_define!(Uint GROUNDCOVER_CANDIDATES_PER_THREAD),
    shader_define!(Uint GROUNDCOVER_MAX_BLADES_PER_CHUNK),
    shader_define!(Uint GROUNDCOVER_MAX_CHUNKS),
    shader_define!(Uint GROUNDCOVER_MODEL_POINTS_PER_CHUNK),
    shader_define!(Uint GROUNDCOVER_MODEL_MAX_RECORDS),
    shader_define!(Uint GROUNDCOVER_MODEL_MAX_SHAPES),
    shader_define!(Uint GROUNDCOVER_MODEL_PHASE_PLACE),
    shader_define!(Uint GROUNDCOVER_MODEL_PHASE_LAYOUT),
    shader_define!(Uint GROUNDCOVER_MODEL_PHASE_EMIT),
    shader_define!(Uint GROUNDCOVER_MODEL_FLAG_AFFINITY_IN_PICK),
    shader_define!(Uint GROUNDCOVER_MODEL_RECORD_FLAG_UNIFORM_SCALING),
    shader_define!(Uint GROUNDCOVER_MODEL_RECORD_FLAG_FIT_TO_SLOPE),
    shader_define!(Uint GROUNDCOVER_MODEL_SHAPE_FLAG_NON_UNIFORM),
    shader_define!(Uint GROUNDCOVER_MODEL_RECORD_BITS),
    shader_define!(Uint GROUNDCOVER_WATER_RULE_ABOVE_AT_LEAST),
    shader_define!(Uint GROUNDCOVER_WATER_RULE_ABOVE_AT_MOST),
    shader_define!(Uint GROUNDCOVER_WATER_RULE_BELOW_AT_LEAST),
    shader_define!(Uint GROUNDCOVER_WATER_RULE_BELOW_AT_MOST),
    shader_define!(Uint GROUNDCOVER_WATER_RULE_EITHER_AT_LEAST),
    shader_define!(Uint GROUNDCOVER_WATER_RULE_EITHER_AT_MOST),
    shader_define!(Uint GROUNDCOVER_MODEL_SEED_SALT),
    shader_define!(Uint GROUNDCOVER_MODEL_COUNT_REGION),
    shader_define!(Uint GROUNDCOVER_MODEL_BASE_REGION),
    shader_define!(Uint GROUNDCOVER_MODEL_CHUNK_TOTAL_REGION),
    shader_define!(Uint GROUNDCOVER_MODEL_SHAPE_REGION),
    shader_define!(Uint GROUNDCOVER_MODEL_STATS_REGION),
    shader_define!(Uint GROUNDCOVER_MAX_CELLS),
    shader_define!(Uint GROUNDCOVER_MODEL_SELECTION_WORDS),
    shader_define!(Uint GROUNDCOVER_MODEL_COVER_LANES),
    shader_define!(Uint GROUNDCOVER_MODEL_COVER_MASK_WORDS),
    shader_define!(Uint GROUNDCOVER_MODEL_COVER_CELL_WORDS),
    shader_define!(Uint GROUNDCOVER_MODEL_COVER_REGION),
    shader_define!(Uint GROUNDCOVER_HISTOGRAM_BUCKETS),
    HeaderLine::Blank,
    HeaderLine::Comment("Ground-cover blade geometry guards"),
    shader_define!(Float GROUNDCOVER_BEZIER_CONTROL_FRACTION),
    shader_define!(Float GROUNDCOVER_BEZIER_BEND_CARRY_MIN),
    shader_define!(Float GROUNDCOVER_BEZIER_BEND_CARRY_MAX),
    shader_define!(Float GROUNDCOVER_BLADE_VECTOR_EPSILON),
    shader_define!(Float MLP_REFRACTION_UV_PER_UNIT),
    shader_define!(Float GROUNDCOVER_PROJECTED_DEPTH_EPSILON),
    HeaderLine::Blank,
    HeaderLine::Comment("Skinning"),
    shader_define!(Uint MAX_BONES_PER_MESH),
    shader_define!(UintPlain SKIN_WORKGROUP_SIZE),
    HeaderLine::Blank,
    HeaderLine::Comment("Material kind enum (`GpuMaterial.materialKind`)."),
    HeaderLine::Comment("Authoritative values: scene_buffer/constants.rs. #1401."),
    shader_define!(Uint MATERIAL_KIND_MULTI_LAYER_PARALLAX),
    shader_define!(Uint MATERIAL_KIND_EYE_ENVMAP),
    HeaderLine::Comment("Decal slots per material — the GpuMaterial decal quartet (#5848)"),
    shader_define!(Uint MATERIAL_DECAL_SLOTS),
    shader_define!(Uint MATERIAL_KIND_GLASS),
    shader_define!(Uint MATERIAL_KIND_EFFECT_SHADER),
    shader_define!(Uint MATERIAL_KIND_NO_LIGHTING),
    shader_define!(Uint MATERIAL_KIND_FIRE_REFRACTION),
    shader_define!(Uint RENDER_LAYER_ARCHITECTURE),
    shader_define!(Uint RENDER_LAYER_CLUTTER),
    shader_define!(Uint RENDER_LAYER_ACTOR),
    shader_define!(Uint RENDER_LAYER_DECAL),
    shader_define!(Uint TONEMAP_OP_ACES),
    shader_define!(Uint TONEMAP_OP_AGX),
    shader_define!(Float EXPOSURE_METER_NEUTRAL),
    shader_define!(Float LIGHT_METER_CALIBRATION_K),
    shader_define!(Float SENSOR_SENSITIVITY_S),
    shader_define!(Float EXPOSURE_METER_S_OVER_K),
    HeaderLine::Comment("Single-workgroup tree reductions + SH coefficient count (#5620)"),
    shader_define!(UintPlain EXPOSURE_METER_WORKGROUP),
    shader_define!(UintPlain EXPOSURE_METER_SAMPLE_GRID),
    shader_define!(UintPlain SKY_IRRADIANCE_WORKGROUP),
    shader_define!(UintPlain SKY_SH_COEFFICIENTS),
    shader_define!(Float ADAPTATION_SAT_FALLOFF),
    shader_define!(Float GRADE_CONTRAST_PIVOT),
    shader_define!(Float GRADE_CONTRAST_TOE),
    shader_define!(Uint FOG_VOLUME_SHAPE_SPHERE),
    shader_define!(Uint FOG_VOLUME_SHAPE_ELLIPSOID),
    shader_define!(Uint FOG_VOLUME_SHAPE_BOX),
    shader_define!(Uint FOG_VOLUME_SHAPE_CONE),
    shader_define!(Uint VISIBILITY_LAYER_ARCHITECTURE),
    shader_define!(Uint VISIBILITY_LAYER_STATIC_PROP),
    shader_define!(Uint VISIBILITY_LAYER_DYNAMIC_ACTOR),
    shader_define!(Uint VISIBILITY_LAYER_FOLIAGE),
    shader_define!(Uint VISIBILITY_LAYER_GLASS),
    shader_define!(Uint VISIBILITY_LAYER_EFFECT),
    shader_define!(Uint VISIBILITY_MASK_ALL_OPAQUE),
    shader_define!(Uint VISIBILITY_MASK_SOLID),
    shader_define!(Uint VISIBILITY_MASK_FULL),
    shader_define!(Uint ATTENUATION_MODEL_LEGACY_SOFT_RANGE),
    shader_define!(Uint ATTENUATION_MODEL_INVERSE_SQUARE),
    shader_define!(Float WORLD_UNITS_PER_METER),
    shader_define!(Float LEGACY_LIGHT_CULL_RANGE_MULTIPLIER),
    shader_define!(Float ADIABATIC_FLAME_TEMPERATURE_K),
    shader_define!(Float COMBUSTION_REACTION_RATE_PER_SECOND),
    shader_define!(Float COMBUSTION_RICH_SOOT_YIELD),
    shader_define!(Float COMBUSTION_LEAN_SOOT_YIELD),
    shader_define!(Float COMBUSTION_SOOT_OXIDATION_RATE_PER_SECOND),
    shader_define!(Vec3 COMBUSTION_SOOT_SINGLE_SCATTER_ALBEDO_RGB),
    shader_define!(Float COMBUSTION_LOCAL_LIGHT_PHASE_FORWARD_G),
    shader_define!(Float COMBUSTION_LOCAL_LIGHT_PHASE_BACKWARD_G),
    shader_define!(Float COMBUSTION_LOCAL_LIGHT_PHASE_MIX),
    shader_define!(Float COMBUSTION_MULTISCATTER_OCTAVE1_WEIGHT),
    shader_define!(Float COMBUSTION_MULTISCATTER_OCTAVE2_WEIGHT),
    shader_define!(Float COMBUSTION_SOOT_OXIDATION_START_TEMPERATURE_K),
    shader_define!(Float COMBUSTION_SOOT_OXIDATION_FULL_TEMPERATURE_K),
    shader_define!(Float EXPLOSION_EXPANSION_TIME_SECONDS),
    shader_define!(Float EXPLOSION_IMPULSE_DURATION_SECONDS),
    shader_define!(Float NUCLEAR_EXPLOSION_EXPANSION_SCALE),
    shader_define!(Float NUCLEAR_EXPLOSION_BUOYANCY_SCALE),
    shader_define!(Float NUCLEAR_EXPLOSION_SMOKE_MASS_SCALE),
    shader_define!(Float COMBUSTION_OVERPRESSURE_DISSIPATION_PER_SECOND),
    shader_define!(Float COMBUSTION_MAX_PRESSURE_ACCELERATION_MPS2),
    shader_define!(Float COMBUSTION_MAX_DILUTION_RATE_PER_SECOND),
    shader_define!(Float COMBUSTION_VORTICITY_CONFINEMENT_SPEED_MPS),
    shader_define!(Float COMBUSTION_MAX_VORTICITY_ACCELERATION_MPS2),
    shader_define!(Float COMBUSTION_TURBULENCE_COARSE_EDDY_SCALE_METERS),
    shader_define!(Float COMBUSTION_TURBULENCE_DETAIL_EDDY_SCALE_METERS),
    shader_define!(Float COMBUSTION_TURBULENCE_COARSE_RISE_SPEED_MPS),
    shader_define!(Float COMBUSTION_TURBULENCE_DETAIL_RISE_SPEED_MPS),
    shader_define!(Float COMBUSTION_AEROSOL_DISSIPATION_PER_SECOND),
    shader_define!(Float COMBUSTION_AEROSOL_LIFT_ACCELERATION_MPS2),
    shader_define!(Float COMBUSTION_AEROSOL_LIFT_EXTINCTION_SCALE),
    shader_define!(Float COMBUSTION_COOLED_AEROSOL_SOURCE_TEMPERATURE_K),
    shader_define!(Float COMBUSTION_EXPLOSION_SMOKE_EXTINCTION_SCALE),
    shader_define!(Float COMBUSTION_FUEL_VAPOUR_REMOVAL_PER_SECOND),
    shader_define!(Float COMBUSTION_RADIANCE_REMOVAL_PER_SECOND),
    shader_define!(Float COMBUSTION_THERMAL_COOLING_PER_SECOND),
    shader_define!(Float COMBUSTION_THERMAL_BUOYANCY_ACCELERATION_MPS2),
    shader_define!(Float COMBUSTION_VELOCITY_DAMPING_PER_SECOND),
    shader_define!(Float COMBUSTION_REACTION_HEAT_RESPONSE),
    shader_define!(Float FLAME_FUEL_BOUNDARY_HEIGHT_FRACTION),
    shader_define!(Float FLAME_REACTION_ZONE_HEIGHT_FRACTION),
    shader_define!(Float FLAME_REACTION_ZONE_FADE_START_FRACTION),
    shader_define!(Float FLAME_SOURCE_LATERAL_SPEED_MPS),
    shader_define!(Float FLAME_SOURCE_VELOCITY_RESPONSE_PER_SECOND),
    shader_define!(Float COMBUSTION_BFECC_ERROR_CORRECTION_STRENGTH),
    shader_define!(Float COMBUSTION_BFECC_ERROR_TRACE_THRESHOLD),
    HeaderLine::Blank,
    HeaderLine::Comment("Transported-combustion light-moment reduction"),
    shader_define!(Uint COMBUSTION_LIGHT_GRID_X),
    shader_define!(Uint COMBUSTION_LIGHT_GRID_Y),
    shader_define!(Uint COMBUSTION_LIGHT_GRID_Z),
    shader_define!(Uint COMBUSTION_LIGHT_GRID_COUNT),
    shader_define!(Float COMBUSTION_LIGHT_HALF_EXTENT_XZ_METERS),
    shader_define!(Float COMBUSTION_LIGHT_HALF_EXTENT_Y_METERS),
    shader_define!(Float COMBUSTION_LIGHT_FIXED_SCALE),
    shader_define!(Float COMBUSTION_LIGHT_VOLUME_FIXED_SCALE),
    HeaderLine::Blank,
    shader_define!(Float SHADOW_FADE_START),
    shader_define!(Float SHADOW_FADE_END),
    shader_define!(Float DIRECTIONAL_SHADOW_TRACE_DISTANCE),
    HeaderLine::Blank,
    HeaderLine::Comment("Glass / IOR ray budget"),
    shader_define!(Uint GLASS_RAY_BUDGET),
    shader_define!(Uint GI_VISIBLE_LIGHT_CAP),
    HeaderLine::Comment("Mesh-ID attachment (R32_UINT): bit 31 is the ALPHA_BLEND_NO_HISTORY"),
    HeaderLine::Comment("flag; set, the low 31 bits are an alpha draw index, not a stable ID."),
    shader_define!(Uint MESH_ID_NO_HISTORY_BIT),
    shader_define!(Uint MESH_ID_STABLE_MASK),
    HeaderLine::Comment("#3879 — GpuRayBudget shader-side ceilings. Each must be >= the"),
    HeaderLine::Comment("matching tier-3 value in AdaptiveRayBudget::settings_for_tier,"),
    HeaderLine::Comment("which now reads these same constants. Unsuffixed - cast at use."),
    shader_define!(UintPlain MAX_DIRECT_SHADOW_SAMPLES),
    shader_define!(UintPlain MAX_PATH_SEGMENTS),
    shader_define!(UintPlain MAX_SHADED_HITS),
    shader_define!(UintPlain MAX_FROXEL_LIGHTS),
    shader_define!(UintPlain MAX_RAY_QUALITY_TIER),
    shader_define!(UintPlain MAX_REFRACT_PASSTHRUS),
    shader_define!(Uint GLASS_RAY_COST),
    HeaderLine::Blank,
    HeaderLine::Comment("One-bounce GI light cap (shadow ray per light at a bounce hit)"),
    shader_define!(Uint GI_HIT_LIGHT_CAP),
    shader_define!(Float GI_SAMPLE_LUMINANCE_CLAMP),
    HeaderLine::Blank,
    HeaderLine::Comment("Caustic accumulation"),
    shader_define!(Float CAUSTIC_FIXED_SCALE),
    HeaderLine::Blank,
    HeaderLine::Comment("Compute workgroup sizes (bloom, volumetrics, SSAO, TAA)."),
    HeaderLine::Comment("Used in layout(local_size_x = WORKGROUP_X, ...) — no `u` suffix."),
    shader_define!(UintPlain WORKGROUP_X),
    shader_define!(UintPlain WORKGROUP_Y),
    shader_define!(UintPlain WORKGROUP_Z),
    HeaderLine::Blank,
    HeaderLine::Comment("Clustered light culling thread count (no `u` suffix — used in"),
    HeaderLine::Comment("`layout(local_size_x = THREADS_PER_CLUSTER)`)."),
    shader_define!(UintPlain THREADS_PER_CLUSTER),
    HeaderLine::Blank,
    HeaderLine::Comment("Bloom + volumetrics tunables (floats — always decimal)."),
    shader_define!(Float BLOOM_INTENSITY),
    shader_define!(Float BLOOM_THRESHOLD),
    shader_define!(Float BLOOM_KNEE),
    shader_define!(Float CLOUD_LAYER_BOTTOM),
    shader_define!(Float CLOUD_LAYER_TOP),
    shader_define!(Float CLOUD_PLANET_RADIUS),
    shader_define!(UintPlain CLOUD_CHEAP_SAMPLES_ZENITH),
    shader_define!(UintPlain CLOUD_CHEAP_SAMPLES_HORIZON),
    shader_define!(UintPlain CLOUD_MAX_MARCH_ITERATIONS),
    shader_define!(UintPlain CLOUD_LIGHT_STEPS),
    shader_define!(Float CLOUD_PHASE_G0),
    shader_define!(Float CLOUD_PHASE_G1),
    shader_define!(Float CLOUD_PHASE_BLEND),
    shader_define!(UintPlain CLOUD_MS_OCTAVES),
    shader_define!(Float CLOUD_MS_SCATTERING_FALLOFF),
    shader_define!(Float CLOUD_MS_EXTINCTION_FALLOFF),
    shader_define!(Float CLOUD_MS_ECCENTRICITY_FALLOFF),
    shader_define!(Float CLOUD_EXTINCTION_PER_METER),
    shader_define!(Float CLOUD_UPPER_PLANE_ALTITUDE),
    shader_define!(Vec3 ATMOSPHERE_RAYLEIGH_EXTINCTION),
    shader_define!(Float ATMOSPHERE_RAYLEIGH_SCALE_HEIGHT),
    shader_define!(Float ATMOSPHERE_MIE_EXTINCTION),
    shader_define!(Float ATMOSPHERE_MIE_SCALE_HEIGHT),
    shader_define!(Float CLOUD_SHADOW_MAP_SIZE as f32),
    shader_define!(Float CLOUD_SHADOW_MAP_EXTENT),
    shader_define!(UintPlain CLOUD_SHADOW_STEPS),
    shader_define!(Float CLOUD_OVERCAST_ONSET),
    shader_define!(Float CLOUD_MOOD_CHROMA_WEIGHT),
    shader_define!(Float CLOUD_STRATIFORM_START),
    shader_define!(Float CLOUD_STRATIFORM_FULL),
    shader_define!(Float CLOUD_OVERCAST_OPTICAL_DEPTH),
    shader_define!(Float CLOUD_WEATHER_SLICE_W),
    shader_define!(Float VOLUME_FAR),
    HeaderLine::Blank,
    HeaderLine::Comment("Per-instance flag bits (`GpuInstance.flags`, lower 16 bits)."),
    HeaderLine::Comment("Authoritative values: scene_buffer/constants.rs. #1190."),
    shader_define!(Uint INSTANCE_FLAG_NON_UNIFORM_SCALE),
    shader_define!(Uint INSTANCE_FLAG_ALPHA_BLEND),
    shader_define!(Uint INSTANCE_FLAG_CAUSTIC_SOURCE),
    shader_define!(Uint INSTANCE_FLAG_TERRAIN_SPLAT),
    shader_define!(Uint INSTANCE_RENDER_LAYER_SHIFT),
    shader_define!(Uint INSTANCE_RENDER_LAYER_MASK),
    shader_define!(Uint INSTANCE_FLAG_FLAT_SHADING),
    shader_define!(Uint INSTANCE_FLAG_DIFFUSE_ALPHA),
    shader_define!(Uint INSTANCE_FLAG_LOD_BLOCK),
    shader_define!(Uint INSTANCE_FLAG_MOVED_THIS_FRAME),
    shader_define!(Uint INSTANCE_TERRAIN_TILE_SHIFT),
    shader_define!(Uint INSTANCE_TERRAIN_TILE_MASK),
    HeaderLine::Comment("LAND splat geometry (#5113): channel count + lanes per vertex weight word."),
    shader_define!(Uint TERRAIN_SPLAT_LAYERS),
    shader_define!(Uint TERRAIN_SPLAT_LANES_PER_WORD),
    HeaderLine::Blank,
    HeaderLine::Comment("Per-material flag bits (`GpuMaterial.materialFlags`)."),
    HeaderLine::Comment("Authoritative values: vulkan/material.rs `material_flag::*`. #1190."),
    shader_define!(Uint MAT_FLAG_VERTEX_COLOR_EMISSIVE),
    shader_define!(Uint MAT_FLAG_EFFECT_SOFT),
    shader_define!(Uint MAT_FLAG_EFFECT_PALETTE_COLOR),
    shader_define!(Uint MAT_FLAG_EFFECT_PALETTE_ALPHA),
    shader_define!(Uint MAT_FLAG_EFFECT_LIT),
    shader_define!(Uint MAT_FLAG_PBR_BSDF),
    shader_define!(Uint MAT_FLAG_TRANSLUCENCY),
    shader_define!(Uint MAT_FLAG_MODEL_SPACE_NORMALS),
    shader_define!(Uint MAT_FLAG_TRANSLUCENCY_THICK_OBJECT),
    shader_define!(Uint MAT_FLAG_TRANSLUCENCY_MIX_ALBEDO),
    shader_define!(Uint MAT_FLAG_THIN_GLASS),
    shader_define!(Uint MAT_FLAG_SOFT_LIGHTING),
    shader_define!(Uint MAT_FLAG_RIM_LIGHTING),
    shader_define!(Uint MAT_FLAG_BACK_LIGHTING),
    shader_define!(Uint MAT_FLAG_MSN_HAS_AUTHORED_Z),
    shader_define!(Uint MAT_FLAG_EFFECT_LI_SHIFT),
    HeaderLine::Blank,
    HeaderLine::Comment("Normal-alpha-as-spec marker bit (OR'd into `GpuMaterial.glossMapIndex`)."),
    shader_define!(Uint NORMAL_ALPHA_SPEC_BIT),
    HeaderLine::Blank,
    HeaderLine::Comment("Alpha-channel-height marker bit (OR'd into `GpuMaterial.parallaxMapIndex`)."),
    shader_define!(Uint PARALLAX_ALPHA_HEIGHT_BIT),
    HeaderLine::Blank,
    HeaderLine::Comment("Tint-alpha-weight marker bit (OR'd into `GpuMaterial.tintMapIndex`)."),
    shader_define!(Uint TINT_ALPHA_WEIGHT_BIT),
    HeaderLine::Blank,
    HeaderLine::Comment("Water motion-kind enum (matches `byroredux::cell_loader::water::WaterKind`)."),
    shader_define!(Uint WATER_CALM),
    shader_define!(Uint WATER_RIVER),
    shader_define!(Uint WATER_RAPIDS),
    shader_define!(Uint WATER_WATERFALL),
    shader_define!(Uint WATER_LAVA),
    shader_define!(Float DEFAULT_WATER_WAVE_AMPLITUDE),
    shader_define!(Float DEFAULT_WATER_WAVE_FREQUENCY),
    shader_define!(Float WATER_COLUMN_ABSORPTION_SHAPE),
    shader_define!(Float WATER_FLOW_MAP_CYCLE_SECONDS),
    HeaderLine::Comment("`water.vert` two-wave displacement — shared with the CPU crest sampler (#5582)."),
    shader_define!(Float WATER_WAVE_DEFAULT_SCROLL_A),
    shader_define!(Float WATER_WAVE_DEFAULT_SCROLL_B),
    shader_define!(Float WATER_WAVE_RATE_MIN),
    shader_define!(Float WATER_WAVE_RATE_MAX),
    shader_define!(Float WATER_WAVE_MIX_A),
    shader_define!(Float WATER_WAVE_MIX_B),
    shader_define!(Float WATER_WAVE_PHASE_B_RATE),
    shader_define!(Float WATER_WAVE_AMPLITUDE_MAX),
    shader_define!(Float WATER_WAVE_SPATIAL_FLOOR_A),
    shader_define!(Float WATER_WAVE_SPATIAL_FLOOR_B),
    shader_define!(Vec2 WATER_WAVE_FALLBACK_DIR_A),
    shader_define!(Vec2 WATER_WAVE_FALLBACK_DIR_B),
    HeaderLine::Blank,
    HeaderLine::Comment("BGEM v21+ glass neutral pivots — `triangle.frag` divides the"),
    HeaderLine::Comment("authored scalars by these (#3459)."),
    shader_define!(Float DEFAULT_GLASS_BLUR_SCALE),
    shader_define!(Float DEFAULT_GLASS_REFRACTION_SCALE),
    HeaderLine::Blank,
    HeaderLine::Comment("Local fog-volume clustering (M55/Session 62)."),
    shader_define!(Uint FOG_VOLUME_CLUSTER_DIM),
    shader_define!(Uint MAX_FOG_VOLUMES_PER_CLUSTER),
    shader_define!(Uint MAX_FOG_PORTALS_PER_CLUSTER),
    shader_define!(Uint MAX_COMPOSITE_SKY_APERTURES),
    shader_define!(Float FOG_APERTURE_RIM_PROBE_RADIUS_BU),
    shader_define!(Float FOG_APERTURE_RIM_NORMAL_DOT_MIN),
    shader_define!(Float FOG_APERTURE_RIM_PLANE_TOLERANCE_BU),
    shader_define!(Float FOG_VOLUME_PROFILE_HOMOGENEOUS),
    shader_define!(Float FOG_VOLUME_PROFILE_SMOKE),
    shader_define!(Float FOG_VOLUME_PROFILE_FLAME),
    shader_define!(Float FOG_VOLUME_PROFILE_EXPLOSION),
    shader_define!(Float FOG_VOLUME_PROFILE_EXPLOSION_OIL),
    shader_define!(Float FOG_VOLUME_PROFILE_EXPLOSION_NUCLEAR),
    shader_define!(Float FOG_VOLUME_PROFILE_LIGHT_SHAFT),
    HeaderLine::Blank,
    HeaderLine::Comment("Structured renderer correctness views (`GpuCamera.renderDebug.x`)."),
    shader_define!(Uint RENDER_DEBUG_FINAL),
    shader_define!(Uint RENDER_DEBUG_SHADOW_VISIBILITY),
    shader_define!(Uint RENDER_DEBUG_SELECTED_LIGHT),
    shader_define!(Uint RENDER_DEBUG_DIRECT_ONLY),
    shader_define!(Uint RENDER_DEBUG_INDIRECT_ONLY),
    shader_define!(Uint RENDER_DEBUG_MATERIAL_LOBE),
    shader_define!(Uint RENDER_DEBUG_COMPOSITE_TERM),
    shader_define!(Uint RENDER_DEBUG_RT_LOD),
    shader_define!(Uint RENDER_DEBUG_VOLUMETRIC_TERM),
    shader_define!(Uint RENDER_DEBUG_MATERIAL_ROLE),
    shader_define!(Uint RENDER_DEBUG_WATER_TERM),
    shader_define!(Uint RENDER_DEBUG_WATER_NORMAL),
    shader_define!(Uint RENDER_DEBUG_TERRAIN_LOD),
    shader_define!(Uint RENDER_DEBUG_WATER_REFL),
    shader_define!(Uint RENDER_DEBUG_FACING_RATIO),
    shader_define!(Uint RENDER_DEBUG_RESTIR_LIGHT),
    shader_define!(Uint RENDER_DEBUG_RESTIR_HISTORY),
    shader_define!(Uint RENDER_DEBUG_MODE_MAX),
    shader_define!(Uint RENDER_DEBUG_LEGACY_FLAGS),
    HeaderLine::Blank,
    HeaderLine::Comment("Debug-viz bit flags (set via console for renderer bisects)."),
    shader_define!(Uint DBG_BYPASS_POM),
    shader_define!(Uint DBG_BYPASS_DETAIL),
    shader_define!(Uint DBG_VIZ_NORMALS),
    shader_define!(Uint DBG_VIZ_TANGENT),
    shader_define!(Uint DBG_BYPASS_NORMAL_MAP),
    shader_define!(Uint DBG_RESERVED_20),
    shader_define!(Uint DBG_VIZ_RENDER_LAYER),
    shader_define!(Uint DBG_VIZ_GLASS_PASSTHRU),
    shader_define!(Uint DBG_DISABLE_SPECULAR_AA),
    shader_define!(Uint DBG_VIZ_AO),
    shader_define!(Uint DBG_BYPASS_VERTEX_COLOR),
    shader_define!(Uint DBG_DISABLE_AO),
    shader_define!(Uint DBG_LEGACY_LIGHT_ATTEN),
    shader_define!(Uint DBG_DISABLE_MULTISCATTER),
    shader_define!(Uint DBG_DISABLE_ATROUS),
    shader_define!(Uint DBG_DISABLE_RESTIR),
    shader_define!(Uint DBG_DISABLE_SPATIAL),
    shader_define!(Uint DBG_VIZ_MOTION),
    shader_define!(Uint DBG_DISABLE_TEMPORAL),
    shader_define!(Uint DBG_VIZ_RAW_INDIRECT),
    shader_define!(Uint DBG_VIZ_MATERIAL_STATE),
    shader_define!(Uint DBG_VIZ_GI_BOUNCE),
    shader_define!(Uint DBG_VIZ_FSR_TEMPORAL),
    shader_define!(Uint DBG_VIZ_NONFINITE),
    shader_define!(Uint DBG_VIZ_SHADOW_OFFSET),
    shader_define!(Uint DBG_VIZ_NORMAL_DIVERGENCE),
    shader_define!(Uint DBG_VIZ_DIRECT),
    shader_define!(Uint DBG_DISABLE_DIRECT_SHADOWS),
    shader_define!(Uint DBG_DISABLE_GI_RAYS),
    shader_define!(Uint DBG_DISABLE_REFLECTION_GLASS_RAYS),
    shader_define!(Uint DBG_DISABLE_ALL_MAIN_RAYS),
    shader_define!(Uint DBG_VIZ_SELECTED_LIGHT),
    shader_define!(Uint DBG_VIZ_MATERIAL_LOBES),
    shader_define!(Uint DBG_VIZ_RT_LOD),
    shader_define!(Uint DBG_VIZ_SHADOW_VISIBILITY),
    HeaderLine::Blank,
    HeaderLine::DbgVizRequiresRawOutput,
    HeaderLine::Blank,
    HeaderLine::Comment("Main-pass ray-query decomposition."),
    shader_define!(Uint RT_ABLATION_DIRECT_SHADOW),
    shader_define!(Uint RT_ABLATION_GI),
    shader_define!(Uint RT_ABLATION_REFLECTION_GLASS),
    shader_define!(Uint RT_ABLATION_ALL_RAYS),
    shader_overridable!(Uint RT_COMPILE_ABLATION_MASK),
    HeaderLine::Blank,
    HeaderLine::Comment("#1799 / PERF-D5-NEW-01 — compile-time gate for the legacy 16-slot"),
    HeaderLine::Comment("WRS reservoir arrays. 0 (default): preprocessed out of"),
    HeaderLine::Comment("triangle.frag entirely. 1: restores the pre-fix always-compiled"),
    HeaderLine::Comment("behavior for A/B against ReSTIR (requires a shader recompile)."),
    shader_define!(UintPlain ENABLE_LEGACY_WRS),
    HeaderLine::Blank,
];
