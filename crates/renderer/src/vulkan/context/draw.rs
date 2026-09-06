//! Frame recording and submission — the per-frame hot path.

use super::super::descriptors::memory_barrier;
use super::super::frame_upscaler::FsrFrameParameters;
use super::super::material::GpuMaterial;
use super::super::pipeline::PipelineKey;
use super::super::presentation::ImageSpaceModifierView;
use super::super::scene_buffer::{
    self, MATERIAL_KIND_GLASS, MATERIAL_KIND_MULTI_LAYER_PARALLAX, MAX_INDIRECT_DRAWS,
};
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::super::upscaling::fsr_camera_parameters;
use super::super::water::WaterDrawCommand;
use super::assemble_camera_and_lights::CameraAssemblyOutput;
use super::begin_frame_recording::BeginFrameOutput;
use super::build_and_upload_instances::BuildInstancesOutput;
use super::{DofView, DrawCommand, FrameTimings, SkyParams, VulkanContext};
use anyhow::{Context, Result};
use ash::vk;
use byroredux_core::ecs::storage::EntityId;
use std::time::Instant;

/// Shirley concentric disk mapping — maps the unit square [0,1)² uniformly
/// onto the unit disk. Returns `(u, v)` in `[-1, 1]²` with `u²+v² ≤ 1`.
///
/// Used for DOF aperture disk sampling: scaling the result by the lens
/// aperture radius and adding to the camera position gives a uniform
/// distribution of ray origins across the aperture disk.
fn concentric_disk_sample(a: f32, b: f32) -> (f32, f32) {
    // Map [0,1]² → [-1,1]²
    let a = a * 2.0 - 1.0;
    let b = b * 2.0 - 1.0;
    if a == 0.0 && b == 0.0 {
        return (0.0, 0.0);
    }
    let (r, theta) = if a.abs() > b.abs() {
        (a, std::f32::consts::FRAC_PI_4 * (b / a))
    } else {
        (
            b,
            std::f32::consts::FRAC_PI_2 - std::f32::consts::FRAC_PI_4 * (a / b),
        )
    };
    (r * theta.cos(), r * theta.sin())
}

/// Halton low-discrepancy sequence value at `index` (1-indexed) for `base`.
/// Returns a value in [0, 1).
fn halton(mut index: u32, base: u32) -> f32 {
    let mut result = 0.0_f32;
    let mut f = 1.0 / base as f32;
    while index > 0 {
        result += f * (index % base) as f32;
        index /= base;
        f /= base as f32;
    }
    result
}

/// Pure camera-cut decision (#2159), pulled out of `draw_frame` so it's
/// testable without standing up a `VulkanContext` — mirrors the
/// `is_slow_frame`/`scratch_should_shrink` style pure-predicate convention
/// used elsewhere in the renderer.
///
/// Two independent, origin/speed-robust signals: an absolute position jump
/// (`camera_delta`, in world units — unaffected by the render-origin's
/// camera-relative snapping) and an angular forward-vector flip
/// (`cam_forward_dot`, the dot product of this frame's and last frame's
/// unit forward vectors — unaffected by translation speed). Either alone
/// indicates a teleport/cutscene snap that bypassed the cell-transition
/// reset hooks; ordinary locomotion (any speed) and render-origin grid
/// crossings (zero real camera motion) trip neither.
///
/// Replaces a raw `view_proj` element-wise diff, which — because
/// `view_proj` is camera-relative to a render origin that itself snaps on
/// a grid crossing — misfired on both ordinary walk/run speeds and every
/// grid crossing, permanently defeating #1489's origin correction and
/// forcing TAA/SVGF/FSR into a reset loop while the player simply moved.
pub(super) fn is_camera_cut(frame_counter: u32, camera_delta: f32, cam_forward_dot: f32) -> bool {
    frame_counter > 0 && (camera_delta > 256.0 || cam_forward_dot < 0.0)
}

/// The three frame-over-frame camera signals `draw_frame` derives before
/// asking [`is_camera_cut`] for a verdict (#2197, extracted from
/// `draw_frame`).
pub(super) struct CameraFrameDeltas {
    /// Absolute world-space distance the camera moved since last frame.
    pub(super) camera_delta: f32,
    /// Dot product of this frame's and last frame's unit forward vectors.
    pub(super) cam_forward_dot: f32,
    /// Largest element-wise `|Δ|` between this and last frame's view-proj.
    /// Diagnostic only — deliberately NOT a cut signal, see below.
    pub(super) vp_max_abs_delta: f32,
}

/// Derive the frame-over-frame camera signals for cut detection.
///
/// #2159: the VP-matrix limb used to BE the cut signal — a raw element-wise
/// diff against `prev_view_proj`, which is camera-RELATIVE to the PREVIOUS
/// frame's render origin, so a plain 4096-unit grid crossing (zero real
/// camera motion) produced a huge, meaningless delta; the same raw diff also
/// tripped on ordinary walk/run speeds (a 6 units/frame forward move alone
/// crossed the old 0.75 threshold by 8x). Both false-positives defeated
/// #1489's origin correction on exactly the frame it exists for, and forced
/// TAA/SVGF/FSR into a permanent reset loop while the player was simply
/// moving.
///
/// It was replaced by two signals that are each robust on their own: an
/// absolute position jump (unaffected by origin-relativity) and an angular
/// forward-vector flip (unaffected by translation speed) — a real
/// teleport/cutscene snap reorients the camera far more than any continuous
/// turn does in one frame. `vp_max_abs_delta` survives only as a per-frame
/// debug-log field; keeping it here (rather than at the log site) documents
/// that its exclusion from [`is_camera_cut`] is deliberate.
pub(super) fn camera_frame_deltas(
    camera_pos: [f32; 3],
    prev_camera_position: [f32; 3],
    cam_forward: [f32; 3],
    prev_cam_forward: [f32; 3],
    view_proj: &[f32; 16],
    prev_view_proj: &[f32; 16],
) -> CameraFrameDeltas {
    use byroredux_core::math::Vec3;
    CameraFrameDeltas {
        camera_delta: Vec3::from_array(camera_pos).distance(Vec3::from_array(prev_camera_position)),
        cam_forward_dot: Vec3::from_array(cam_forward).dot(Vec3::from_array(prev_cam_forward)),
        vp_max_abs_delta: view_proj
            .iter()
            .zip(prev_view_proj.iter())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max),
    }
}

/// Whether a draw should read/write the per-frame rigid motion-history map
/// (#2160). The map is keyed on `DrawCommand::entity_id`, but particle
/// draws synthesize that field as `entity ^ i` — a sort tiebreaker, not a
/// real identity — which routinely aliases a real static-mesh entity's ID
/// and would otherwise corrupt that entity's motion vector. Alpha-blend
/// draws (particles included) get no temporal benefit from motion-history
/// reuse anyway, so they're excluded regardless of ID collisions; skinned
/// draws (`bone_offset != 0`) already have their own per-entity skin-pool
/// history and never used this map.
pub(super) fn uses_rigid_motion_history(bone_offset: u32, alpha_blend: bool) -> bool {
    bone_offset == 0 && !alpha_blend
}

/// Resolve the `GpuInstance.skinned_vertex_address` value for a draw
/// (REN-2026-07-28-02 / #2219). Pure decision function, extracted from the
/// live `vkGetBufferDeviceAddress` call site so the branch logic is
/// unit-testable without a device: rigid draws (`bone_offset == 0`) always
/// get `0` regardless of `slot_address` (defensive — a stray populated
/// slot must never leak into a rigid instance's field), and a skinned draw
/// with no registered `SkinSlot` yet (first-sight frame, or a
/// pool-exhaustion fallback) also gets `0`, falling back to the bind-pose
/// hit-normal path `ray_hit.glsl` already had before this fix.
#[inline]
pub(super) fn skinned_vertex_address_for_draw(
    bone_offset: u32,
    slot_address: Option<vk::DeviceAddress>,
) -> vk::DeviceAddress {
    if bone_offset == 0 {
        return 0;
    }
    slot_address.unwrap_or(0)
}

/// Whether a registered `SkinSlot` may back this draw's
/// `skinnedVertexAddress` (#2402 / CHAIN2-D2-03).
///
/// The slot's output buffer holds exactly `slot_vertex_count` positions,
/// and `ray_hit.glsl` indexes it through the *live* mesh's index buffer
/// via `GL_EXT_buffer_reference` — a raw device address with no
/// descriptor range check. So the slot may only be published when it was
/// sized for the very mesh being drawn.
///
/// The reconciliation that normally destroys and recreates a stale slot
/// lives in `record_skinned_blas_refit`'s dispatch loop, which `continue`s
/// past any draw whose mesh is `!rt_capable`. A skinned entity remapped
/// from an RT-capable mesh to a non-RT-capable one (a skinned
/// effect-shader proxy or decal — M41 equip/outfit swap, cell reload)
/// therefore keeps its old slot until the LRU sweep reaps it up to
/// `MAX_FRAMES_IN_FLIGHT + 1` frames later. If the new mesh has more
/// vertices, those frames read past the end of the allocation: garbage
/// normals at best, a GPU page fault at worst.
///
/// Equality rather than `>=`: a larger slot is in-bounds but still holds
/// a *different* mesh's skinned positions, which reconstructs hit normals
/// from the wrong geometry. Either way the caller falls back to `0`, and
/// `ray_hit.glsl`'s `skinnedVertexAddress != 0` branch takes the bind-pose
/// path it used before #2219 — the same fallback a first-sight frame or a
/// pool-exhaustion miss already gets.
#[inline]
pub(super) fn skin_slot_backs_mesh(slot_vertex_count: u32, mesh_vertex_count: u32) -> bool {
    slot_vertex_count == mesh_vertex_count
}

/// #3231 — the `MorphSlot` sibling of `skin_slot_backs_mesh`. Same
/// hazard, same fix: a `MorphSlot` is created once at spawn time for a
/// specific mesh (vertex_count AND target_count both fixed then) and
/// never resized, so a slot surviving a mesh remap (mod swap, cell
/// reload reusing an EntityId before this slot's own despawn cleanup
/// runs) must not be published for the new mesh — the shader indexes
/// `deltas.data[target * vertexCount + localVertex]` through a raw
/// `buffer_reference` with no range check, so an oversized `vertexCount`
/// (from `GpuInstance`, always the LIVE mesh's count) against an
/// undersized delta buffer reads out of bounds.
///
/// `pub(super)`: also called from `skinned_blas_refit.rs` (a sibling
/// module under `context/`) to gate the same slot before it's read by
/// the `skin_vertices.comp` dispatch, not just the raster `GpuInstance`
/// upload below.
#[inline]
pub(super) fn morph_slot_backs_mesh(
    slot_vertex_count: u32,
    slot_target_count: u32,
    mesh_vertex_count: u32,
) -> bool {
    slot_vertex_count == mesh_vertex_count && slot_target_count > 0
}

/// #3231 — resolve the three `GpuInstance` morph fields for a draw.
/// Pure decision function mirroring `skinned_vertex_address_for_draw`:
/// unit-testable without a device. `None` addresses (no slot yet, or a
/// slot that failed the mesh-backing check) fall back to the all-zero
/// "no morph data" triple — the shader's own `morphDeltaAddress != 0`
/// gate then never dereferences a stale/absent buffer.
///
/// `pub(super)`: see `morph_slot_backs_mesh` above — same cross-module
/// caller.
#[inline]
pub(super) fn morph_gpu_fields_for_draw(
    slot: Option<(vk::DeviceAddress, vk::DeviceAddress, u32)>,
) -> (vk::DeviceAddress, vk::DeviceAddress, u32) {
    slot.unwrap_or((0, 0, 0))
}

#[cfg(test)]
mod morph_gpu_fields_tests {
    use super::{morph_gpu_fields_for_draw, morph_slot_backs_mesh};

    #[test]
    fn no_slot_is_all_zero() {
        assert_eq!(morph_gpu_fields_for_draw(None), (0, 0, 0));
    }

    #[test]
    fn live_slot_publishes_its_fields() {
        assert_eq!(
            morph_gpu_fields_for_draw(Some((0xDEAD_BEEF, 0xFEED_FACE, 12))),
            (0xDEAD_BEEF, 0xFEED_FACE, 12)
        );
    }

    #[test]
    fn slot_sized_for_this_mesh_backs_it() {
        assert!(morph_slot_backs_mesh(1_024, 8, 1_024));
    }

    #[test]
    fn undersized_slot_is_rejected() {
        assert!(!morph_slot_backs_mesh(512, 8, 1_024));
    }

    #[test]
    fn oversized_slot_is_also_rejected() {
        // Equality, not `>=` — same reasoning as skin_slot_backs_mesh:
        // a larger slot is in-bounds but holds a DIFFERENT mesh's
        // deltas, which would blend the wrong shape onto this one.
        assert!(!morph_slot_backs_mesh(2_048, 8, 1_024));
    }

    /// A slot with zero targets (shouldn't exist in practice --
    /// `attach_animation_sinks` only creates slots with usable morph
    /// data -- but defend against it anyway, since `morphTargetCount
    /// == 0` combined with a nonzero address would let the shader's
    /// `!= 0` gate fire and then iterate a zero-length loop, which is
    /// harmless but signals a slot that should never have been
    /// created).
    #[test]
    fn zero_target_slot_is_rejected() {
        assert!(!morph_slot_backs_mesh(1_024, 0, 1_024));
    }
}

#[cfg(test)]
mod skinned_vertex_address_tests {
    use super::skinned_vertex_address_for_draw;

    #[test]
    fn rigid_draw_never_carries_an_address() {
        assert_eq!(skinned_vertex_address_for_draw(0, Some(0xDEAD_BEEF)), 0);
        assert_eq!(skinned_vertex_address_for_draw(0, None), 0);
    }

    #[test]
    fn skinned_draw_with_a_slot_carries_its_address() {
        assert_eq!(
            skinned_vertex_address_for_draw(64, Some(0xDEAD_BEEF)),
            0xDEAD_BEEF
        );
    }

    #[test]
    fn skinned_draw_without_a_slot_yet_falls_back_to_zero() {
        assert_eq!(skinned_vertex_address_for_draw(64, None), 0);
    }

    // #2402 / CHAIN2-D2-03 — the capacity gate applied at the call site
    // before the device-address query. The hazard it closes (a stale slot
    // sized for a smaller mesh, published after a remap onto a
    // non-RT-capable mesh that the refit path's reconciliation skips) is
    // a raw `buffer_reference` overread, invisible to both `cargo test`
    // and the validation layer — so the decision logic is what gets
    // pinned here, in the same style as `skinned_vertex_address_for_draw`
    // above.
    use super::skin_slot_backs_mesh;

    #[test]
    fn slot_sized_for_this_mesh_backs_it() {
        assert!(skin_slot_backs_mesh(1_024, 1_024));
    }

    #[test]
    fn undersized_slot_is_rejected() {
        // The dangerous direction: the live mesh has more vertices than
        // the slot was allocated for, so the shader would index past the
        // end of the allocation through a raw device address.
        assert!(!skin_slot_backs_mesh(512, 1_024));
    }

    #[test]
    fn oversized_slot_is_also_rejected() {
        // In-bounds but wrong geometry — the slot holds a different
        // mesh's skinned positions, which reconstructs hit normals from
        // the wrong surface. Bind-pose is the better fallback.
        assert!(!skin_slot_backs_mesh(2_048, 1_024));
    }
}

/// TAA sub-pixel jitter via Halton(2,3) sequence, in NDC. Each frame shifts
/// the projection by a different sub-pixel offset so temporal blending
/// reconstructs a super-sampled result; the vertex shader applies it AFTER
/// motion-vector computation so reprojection stays jitter-free.
///
/// Period 16 (#1093 / REN-D11-002). **Correction 2026-08-31**: this
/// comment previously claimed Halton(2)/Halton(3) have "natural periods"
/// of 2/3 with an LCM of 6 — Halton sequences are aperiodic (the radical
/// inverse is injective on the index), so there is no such period to take.
/// It also claimed `% 8` never reached "the 9th Halton(3) sample ≈ 0.889";
/// `halton(9, 3) = 1/27 ≈ 0.037` is what index 9 actually gives, and
/// `0.889 = halton(8, 3)` — reached by `% 8` (index range `1..=8`). The
/// sample `% 8` actually omits is index 9's `1/27`; `% 8`'s Y set is in
/// fact perfectly stratified to ninths, and `% 16` adds four 27ths that
/// are *not* aligned to that grid, so it is not obviously more uniform.
/// The real motivation for 16 over 8 is not re-derived here — treat this
/// as an open question rather than re-quote the retracted rationale above.
///
/// Returns `(0.0, 0.0)` (no jitter) whenever `taa_present` is false OR
/// `taa_failed` is true (#1932 / TAA-D13-01) — a permanent TAA failure must
/// fall back to a stable pinhole image, not a jittered-but-unresolved one.
///
/// SIGN CONVENTION (#2772 / REN-D13-05): Y is negated to agree with
/// [`super::super::upscaling::fsr_pixel_jitter_to_ndc`]'s Vulkan-NDC-vs-
/// SDK-render-pixel flip, so `GpuCamera.jitter.y` carries the SAME sign
/// meaning regardless of which upscaler is active. `clip.xy += jitter.xy *
/// clip.w` in triangle.vert/water.vert is sign-agnostic (a jittered sample
/// is undone by neighborhood-clamp + motion-vector reprojection in TAA/SVGF
/// resolve, never by re-reading the sign back out), so this had no
/// rendering effect either way — the only shader-side reader that cares
/// about the sign is `triangle.frag`'s `DBG_VIZ_FSR_TEMPORAL` debug view,
/// which hard-codes the FSR convention and was silently wrong under TAA
/// before this fix.
pub(super) fn taa_jitter(
    taa_present: bool,
    taa_failed: bool,
    frame_counter: u32,
    width: f32,
    height: f32,
) -> (f32, f32) {
    if taa_present && !taa_failed {
        let idx = (frame_counter % 16) + 1; // 1-indexed
        let hx = halton(idx, 2);
        let hy = halton(idx, 3);
        // Map [0,1] → [-0.5, 0.5] pixels, then to NDC. Y negated — see the
        // SIGN CONVENTION doc above.
        ((hx - 0.5) * 2.0 / width, -(hy - 0.5) * 2.0 / height)
    } else {
        (0.0, 0.0)
    }
}

#[cfg(test)]
mod taa_jitter_tests {
    use super::taa_jitter;

    /// No TAA pipeline at all (disabled build / init failure before the
    /// `Option` is ever populated) — always the stable pinhole offset.
    #[test]
    fn no_taa_present_is_unjittered() {
        assert_eq!(taa_jitter(false, false, 7, 1920.0, 1080.0), (0.0, 0.0));
    }

    /// #1932 / TAA-D13-01 — the regression this issue is about: once
    /// `taa_failed` latches, jitter must stop even though `taa.is_some()`
    /// stays true (the `Option` isn't torn down on failure, only bypassed).
    /// Pre-fix this returned a nonzero offset, matching the un-failed case.
    #[test]
    fn taa_failed_is_unjittered_even_with_pipeline_present() {
        assert_eq!(taa_jitter(true, true, 7, 1920.0, 1080.0), (0.0, 0.0));
    }

    /// The normal case still jitters, and does so identically regardless
    /// of the (irrelevant when un-failed) taa_failed plumbing path taken
    /// to reach here — i.e. this isn't a trivial "always zero" fix.
    #[test]
    fn taa_present_and_not_failed_jitters_nonzero() {
        let (jx, jy) = taa_jitter(true, false, 7, 1920.0, 1080.0);
        assert!(
            jx != 0.0 || jy != 0.0,
            "expected a nonzero Halton jitter offset"
        );
    }

    /// #2772 / REN-D13-05 — TAA and FSR must negate Y the same way when
    /// writing `GpuCamera.jitter.y`, so a shader reading it back (e.g.
    /// `DBG_VIZ_FSR_TEMPORAL`) doesn't need to know which upscaler
    /// produced the value. TAA negates its raw Halton offset; FSR negates
    /// the SDK's raw pixel-space offset — both must flip sign the same
    /// direction relative to their own "positive raw offset" case.
    #[test]
    fn taa_and_fsr_negate_jitter_y_the_same_way() {
        use super::super::super::upscaling::fsr_pixel_jitter_to_ndc;
        use super::halton;

        let frame_counter = 7;
        let idx = (frame_counter % 16) + 1;
        let raw_halton_y = halton(idx, 3);
        let (_, taa_jy) = taa_jitter(true, false, frame_counter, 1920.0, 1080.0);
        assert_eq!(
            taa_jy.is_sign_negative(),
            (raw_halton_y - 0.5).is_sign_positive(),
            "taa_jitter's Y component must be the negation of the raw \
             (halton_y - 0.5) offset"
        );

        let fsr_ndc = fsr_pixel_jitter_to_ndc(
            [0.0, 3.0],
            ash::vk::Extent2D {
                width: 1920,
                height: 1080,
            },
        );
        assert!(
            fsr_ndc[1].is_sign_negative(),
            "fsr_pixel_jitter_to_ndc must negate a positive raw pixel-Y \
             offset — same direction taa_jitter negates its own positive \
             raw offset above"
        );
    }
}

#[cfg(test)]
mod camera_cut_tests {
    use super::is_camera_cut;
    use byroredux_core::math::Vec3;

    /// #2159 regression guard — the exact false-positive that defeated
    /// #1489's origin correction: no real teleport, just a render-origin
    /// grid crossing (which by construction carries zero camera position
    /// jump and zero forward-vector change) plus ordinary fast-run
    /// locomotion (well over the old raw-matrix-diff threshold). Must NOT
    /// be classified as a cut.
    #[test]
    fn grid_crossing_plus_fast_run_is_not_a_cut() {
        // A grid crossing changes the render origin, not the camera's
        // absolute position or facing — both signals stay at "no change".
        let camera_delta = 0.0; // no teleport
        let cam_forward_dot = 1.0; // identical facing frame-to-frame
        assert!(!is_camera_cut(10, camera_delta, cam_forward_dot));

        // Sanity: even at the high end of documented run speed (400 u/s /
        // 60fps ≈ 6.7 u/frame), the position-delta signal alone (were it
        // still driven by translation) would stay far under the 256 u
        // teleport threshold — confirms the fix path, not just a
        // degenerate zero-motion case.
        let fast_run_frame_delta = 6.7;
        assert!(!is_camera_cut(10, fast_run_frame_delta, cam_forward_dot));
    }

    /// A same-frame ~180° reorientation (e.g. a scripted camera snap) must
    /// still be caught even with zero position change.
    #[test]
    fn same_position_but_reversed_facing_is_a_cut() {
        let forward_before = Vec3::new(0.0, 0.0, -1.0);
        let forward_after = Vec3::new(0.0, 0.0, 1.0);
        let cam_forward_dot = forward_before.dot(forward_after);
        assert!(is_camera_cut(10, 0.0, cam_forward_dot));
    }

    /// An absolute-position teleport past the 256-unit threshold is still
    /// caught regardless of facing.
    #[test]
    fn large_position_jump_is_a_cut() {
        assert!(is_camera_cut(10, 500.0, 1.0));
    }

    /// Frame 0 never counts as a cut (nothing to compare against yet).
    #[test]
    fn first_frame_is_never_a_cut() {
        assert!(!is_camera_cut(0, 10_000.0, -1.0));
    }

    /// #2197 — the same #2159 grid-crossing false-positive, now pinned end
    /// to end through the extracted derivation: a render-origin crossing
    /// makes `view_proj` jump wildly (the old cut signal) while the camera
    /// has neither moved nor turned, and the verdict must still be "no cut".
    #[test]
    fn deltas_from_a_grid_crossing_do_not_reach_the_cut_verdict() {
        let vp = [0.0_f32; 16];
        let mut prev_vp = [0.0_f32; 16];
        prev_vp[12] = 4096.0; // origin re-base: huge matrix delta, no motion
        let d = super::camera_frame_deltas(
            [10.0, 20.0, 30.0],
            [10.0, 20.0, 30.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            &vp,
            &prev_vp,
        );
        assert_eq!(d.camera_delta, 0.0);
        assert_eq!(d.cam_forward_dot, 1.0);
        assert_eq!(
            d.vp_max_abs_delta, 4096.0,
            "the diagnostic limb still reports the crossing"
        );
        assert!(
            !is_camera_cut(10, d.camera_delta, d.cam_forward_dot),
            "vp_max_abs_delta must stay diagnostic-only — feeding it back \
             into the verdict is exactly the #2159 regression"
        );
    }

    /// A real teleport produces a position delta past the threshold, and
    /// the derivation feeds that straight into a cut verdict.
    #[test]
    fn deltas_from_a_teleport_reach_the_cut_verdict() {
        let vp = [0.0_f32; 16];
        let d = super::camera_frame_deltas(
            [1000.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            &vp,
            &vp,
        );
        assert_eq!(d.camera_delta, 1000.0);
        assert!(is_camera_cut(10, d.camera_delta, d.cam_forward_dot));
    }
}

#[cfg(test)]
mod rigid_motion_history_tests {
    use super::uses_rigid_motion_history;

    /// #2160 regression guard: a rigid, opaque draw is exactly the case
    /// the map exists for.
    #[test]
    fn rigid_opaque_draw_uses_history() {
        assert!(uses_rigid_motion_history(0, false));
    }

    /// #2160 regression guard: particles are `bone_offset: 0` +
    /// `alpha_blend: true` and synthesize a colliding `entity_id`
    /// (`entity ^ i`) — they must be excluded from the map so that alias
    /// can never corrupt a real entity's motion vector.
    #[test]
    fn alpha_blend_draw_never_uses_history_even_at_bone_offset_zero() {
        assert!(!uses_rigid_motion_history(0, true));
    }

    /// Skinned draws have their own per-entity skin-pool history and never
    /// touched this map, regardless of blend mode.
    #[test]
    fn skinned_draw_never_uses_rigid_history() {
        assert!(!uses_rigid_motion_history(1, false));
        assert!(!uses_rigid_motion_history(1, true));
    }
}

/// Minimum focal distance for the DOF path. A zero or near-zero `focus_dist`
/// collapses the look-at eye→center vector onto the (perpendicular) aperture
/// offset, producing a sideways view basis — or NaN when the aperture disk
/// sample is also ~0 (eye ≈ center). Below this floor the frame is treated as
/// a pinhole instead. See #1525.
const DOF_MIN_FOCUS_DIST: f32 = 1.0e-3;

/// Build the per-frame depth-of-field view-projection.
///
/// Applies a Halton(5,7) concentric-disk sample to the camera position each
/// frame and points the jittered eye at a fixed focal point. TAA accumulates
/// the per-frame shifts into a spatially-varying bokeh blur: surfaces at
/// `focus_dist` project to identical NDC every frame (zero apparent motion →
/// full temporal weight → sharp); surfaces at other depths pick up a
/// frame-to-frame parallax proportional to their defocus (non-zero motion →
/// reduced TAA weight → blur). Bases 5 and 7 are coprime to the TAA bases
/// (2 and 3) so the 32-frame DOF period interleaves cleanly with the 16-frame
/// TAA period without correlated low-discrepancy gaps.
///
/// Returns `(view_proj, eye_pos)`. The matrix is camera-relative to
/// `render_origin` (so the DOF path stays camera-relative like the pinhole
/// path); the returned eye position stays ABSOLUTE for the shader's view-dir
/// math. Falls back to the pinhole `(*pinhole_vp, camera_pos)` when DOF is
/// disabled (`aperture <= 0.0`) or the focal distance is degenerate
/// (`<= DOF_MIN_FOCUS_DIST`, #1525) — the latter guards against the
/// sideways/NaN look-at the unbounded path would otherwise build.
pub(super) fn dof_effective_view_proj(
    dof: &DofView,
    frame_counter: u32,
    camera_pos: [f32; 3],
    render_origin: byroredux_core::math::Vec3,
    pinhole_vp: &[f32; 16],
) -> ([f32; 16], [f32; 3]) {
    use byroredux_core::math::{Mat4, Vec3};
    if dof.aperture <= 0.0 || dof.focus_dist <= DOF_MIN_FOCUS_DIST {
        return (*pinhole_vp, camera_pos);
    }
    let idx = (frame_counter % 32) + 1;
    let (disk_u, disk_v) = concentric_disk_sample(halton(idx, 5), halton(idx, 7));
    let lens_u = disk_u * dof.aperture;
    let lens_v = disk_v * dof.aperture;

    let pos = Vec3::from_array(camera_pos);
    let right = Vec3::from_array(dof.cam_right);
    let up = Vec3::from_array(dof.cam_up);
    let fwd = Vec3::from_array(dof.cam_forward);

    // Jitter the camera position on the aperture disk (absolute).
    let jittered_eye = pos + lens_u * right + lens_v * up;
    // All rays converge at the focal plane (absolute).
    let focal_pt = pos + dof.focus_dist * fwd;

    let jittered_view =
        Mat4::look_at_rh(jittered_eye - render_origin, focal_pt - render_origin, up);
    let proj = Mat4::from_cols_array(&dof.proj_mat);
    let jvp = (proj * jittered_view).to_cols_array();
    (jvp, jittered_eye.to_array())
}

/// FSR-vs-DOF interaction gate (#2197, extracted from `draw_frame`).
///
/// The initial FSR validation path is pinhole-only: combining the independent
/// Halton(5,7) lens sequence with FSR's own projection jitter would violate
/// the motion/reprojection contract before that contract has been validated.
/// Every other authored DOF field is preserved for the future
/// output-resolution implementation — only `aperture` is forced to zero, and
/// only while FSR is active.
pub(super) fn fsr_gated_dof(dof: DofView, fsr_active: bool) -> DofView {
    if fsr_active {
        DofView {
            aperture: 0.0,
            ..dof
        }
    } else {
        dof
    }
}

/// Assemble this frame's [`FsrFrameParameters`] (#2197, extracted from
/// `draw_frame` alongside `dof_effective_view_proj` / `fsr_gated_dof`).
///
/// `Ok(None)` is the ordinary "FSR is not the active upscaler" result —
/// `fsr_jitter_pixel` is `None` for every non-FSR path, so there is nothing
/// to describe. `Ok(Some(..))` carries the jitter, the reset flag, the frame
/// delta, and the authored perspective parameters FSR reconstructs depth
/// from; those come from `active_dof` (i.e. post-`fsr_gated_dof`) so the
/// values FSR is told about are the values the projection actually used.
///
/// `Err` means FSR is active but the camera cannot be described to it —
/// [`fsr_camera_parameters`] rejects a non-finite / non-perspective
/// near-far-fov triple. That is fatal for the frame rather than something to
/// paper over: FSR would reconstruct against garbage. The caller owns the
/// recovery (recreating the frame's `image_available` semaphore before
/// returning), which is why this stays a pure function returning `Result`.
///
/// Note the camera-cut override is NOT applied here — `draw_frame` sets
/// `reset = true` on the returned parameters after `is_camera_cut` fires,
/// since that decision also drives `signal_temporal_discontinuity` and the
/// `prev_view_proj` substitution.
pub(super) fn build_fsr_frame_parameters(
    active_dof: &DofView,
    fsr_jitter_pixel: Option<[f32; 2]>,
    fsr_reset_pending: bool,
    frame_time_delta_ms: f32,
) -> anyhow::Result<Option<FsrFrameParameters>> {
    let Some(jitter_offset) = fsr_jitter_pixel else {
        return Ok(None);
    };
    let Some(camera) = fsr_camera_parameters(
        active_dof.camera_near,
        active_dof.camera_far,
        active_dof.camera_fov_y,
    ) else {
        anyhow::bail!("FSR requires a finite perspective projection");
    };
    Ok(Some(FsrFrameParameters {
        jitter_offset,
        reset: fsr_reset_pending,
        frame_time_delta_ms,
        camera_near: camera.near,
        camera_far: camera.far,
        camera_fov_angle_vertical: camera.fov_y_radians,
    }))
}

/// Inputs for [`build_composite_params`] (#2255 / TD1-NEW-02). A named-field
/// struct rather than positional arguments deliberately — this bundle is
/// mostly same-typed `f32`s (`fog_near`/`fog_far`/`fog_clip`/`fog_power`/
/// `fog_extinction_per_meter`/`fog_single_scatter_albedo`/
/// `fog_height_reference`), and a positional call site could silently
/// transpose two of them without a type error.
pub(super) struct CompositeParamsInputs<'a> {
    pub(super) fog_color: [f32; 3],
    pub(super) fog_near: f32,
    pub(super) fog_far: f32,
    pub(super) fog_extinction_per_meter: f32,
    pub(super) fog_single_scatter_albedo: f32,
    pub(super) fog_clip: f32,
    pub(super) fog_power: f32,
    pub(super) fog_height_reference: f32,
    pub(super) sky_params: &'a SkyParams,
    pub(super) render_debug_flags: u32,
    pub(super) render_debug_mode: u32,
    pub(super) frame_counter: u32,
    pub(super) volume_far_distance: f32,
    /// Froxel grid depth (slice count) — #2470, `volumetrics::extent().depth`.
    /// Threaded into `sky_horizon.w` so the composite shader can remap
    /// `hybridSliceCoordinate`'s normalized depth onto the `sampler3D`
    /// texel-center grid before the `volumetricFroxel` tap.
    pub(super) froxel_slice_count: f32,
    pub(super) camera_pos: [f32; 3],
    pub(super) render_origin: byroredux_core::math::Vec3,
    pub(super) inv_vp_arr: [[f32; 4]; 4],
    pub(super) underwater: [f32; 4],
    /// Whether `water_caustic_accum` is genuinely live this session — see
    /// `CompositeParams::caustic_flags` (#2508).
    pub(super) water_caustic_active: bool,
}

/// Assemble this frame's `CompositeParams` (#2255 / TD1-NEW-02, extracted
/// from `draw_frame` alongside `build_fsr_frame_parameters` — same
/// rationale: pure data assembly with no borrow-checker reason to stay
/// inline). The caller still owns the `composite.upload_params` call and
/// its error handling; only the field-by-field construction moved here.
pub(super) fn build_composite_params(
    inputs: CompositeParamsInputs<'_>,
) -> super::super::composite::CompositeParams {
    let CompositeParamsInputs {
        fog_color,
        fog_near,
        fog_far,
        fog_extinction_per_meter,
        fog_single_scatter_albedo,
        fog_clip,
        fog_power,
        fog_height_reference,
        sky_params,
        render_debug_flags,
        render_debug_mode,
        frame_counter,
        volume_far_distance,
        froxel_slice_count,
        camera_pos,
        render_origin,
        inv_vp_arr,
        underwater,
        water_caustic_active,
    } = inputs;
    super::super::composite::CompositeParams {
        fog_color: [
            fog_color[0],
            fog_color[1],
            fog_color[2],
            if fog_extinction_per_meter > 0.0 {
                1.0
            } else {
                0.0
            },
        ],
        // Preserve the legacy curve inputs for diagnostics and an
        // explicit compatibility path. Runtime composition evaluates
        // only the engine-native physical medium.
        fog_params: [fog_near, fog_far, fog_clip, fog_power],
        depth_params: [
            if sky_params.is_exterior { 1.0 } else { 0.0 },
            // Categorical debug views must bypass fog, caustics, bloom and
            // dither in the composite pass. Bitcast the same flag word the
            // camera UBO supplies to triangle.frag; no numeric conversion.
            f32::from_bits(render_debug_flags),
            // Structured mode duplicated from `GpuCamera.render_debug.x`;
            // composite has its own UBO and does not declare CameraUBO.
            f32::from_bits(render_debug_mode),
            (frame_counter & 0x00ff_ffff) as f32,
        ],
        volume_params: [
            volume_far_distance,
            super::super::volumetrics::LINEAR_DEPTH,
            super::super::volumetrics::LINEAR_SLICE_FRACTION,
            1.0 / 1024.0,
        ],
        height_fog_params: [
            fog_extinction_per_meter.max(0.0) / super::super::volumetrics::WORLD_UNITS_PER_METER,
            super::super::volumetrics::DEFAULT_SCALE_HEIGHT_METERS
                * super::super::volumetrics::WORLD_UNITS_PER_METER,
            fog_single_scatter_albedo.clamp(0.0, 1.0),
            if sky_params.is_exterior && fog_extinction_per_meter > 0.0 {
                1.0
            } else {
                0.0
            },
        ],
        sky_zenith: [
            sky_params.zenith_color[0],
            sky_params.zenith_color[1],
            sky_params.zenith_color[2],
            sky_params.sun_size,
        ],
        sky_horizon: [
            sky_params.horizon_color[0],
            sky_params.horizon_color[1],
            sky_params.horizon_color[2],
            froxel_slice_count,
        ],
        // #541 — WTHR `SKY_LOWER` group. Pre-fix the
        // shader faked this as `sky_horizon * 0.3`,
        // dropping the authored colour entirely.
        sky_lower: [
            sky_params.lower_color[0],
            sky_params.lower_color[1],
            sky_params.lower_color[2],
            0.0,
        ],
        sun_dir: [
            sky_params.sun_direction[0],
            sky_params.sun_direction[1],
            sky_params.sun_direction[2],
            sky_params.sun_intensity,
        ],
        sun_color: [
            sky_params.sun_color[0],
            sky_params.sun_color[1],
            sky_params.sun_color[2],
            // #478 — pack the CLMT FNAM sun sprite handle
            // into the previously-unused w slot via
            // `from_bits`. The shader reinterprets with
            // `floatBitsToUint`; `0` keeps the procedural
            // disc (pre-fix behaviour).
            f32::from_bits(sky_params.sun_texture_index),
        ],
        cloud_params: [
            sky_params.cloud_scroll[0],
            sky_params.cloud_scroll[1],
            sky_params.cloud_tile_scale,
            f32::from_bits(sky_params.cloud_texture_index),
        ],
        cloud_params_1: [
            sky_params.cloud_scroll_1[0],
            sky_params.cloud_scroll_1[1],
            sky_params.cloud_tile_scale_1,
            f32::from_bits(sky_params.cloud_texture_index_1),
        ],
        cloud_params_2: [
            sky_params.cloud_scroll_2[0],
            sky_params.cloud_scroll_2[1],
            sky_params.cloud_tile_scale_2,
            f32::from_bits(sky_params.cloud_texture_index_2),
        ],
        cloud_params_3: [
            sky_params.cloud_scroll_3[0],
            sky_params.cloud_scroll_3[1],
            sky_params.cloud_tile_scale_3,
            f32::from_bits(sky_params.cloud_texture_index_3),
        ],
        weather_params: [
            sky_params.weather.precipitation[0],
            sky_params.weather.precipitation[1],
            sky_params.weather.thunder_frequency,
            sky_params.weather_time_seconds,
        ],
        weather_wind: [
            sky_params.weather.wind_direction[0],
            sky_params.weather.wind_speed,
            sky_params.weather.wind_direction[1],
            0.0,
        ],
        weather_lightning: [
            sky_params.weather.lightning_color[0],
            sky_params.weather.lightning_color[1],
            sky_params.weather.lightning_color[2],
            sky_params.weather.moon_glare,
        ],
        weather_sky: [
            sky_params.weather.stars_color[0],
            sky_params.weather.stars_color[1],
            sky_params.weather.stars_color[2],
            sky_params.weather.sun_glare,
        ],
        weather_aurora: [
            sky_params.weather.aurora_intensity,
            if sky_params.weather.aurora_follows_sun {
                1.0
            } else {
                0.0
            },
            // Broad procedural-cloud occupancy. Reuses the first reserved
            // lane so CompositeParams keeps its established 480-byte ABI.
            sky_params.weather.cloud_coverage,
            0.0,
        ],
        cloud_tint_0: sky_params.weather.cloud_tints[0],
        cloud_tint_1: sky_params.weather.cloud_tints[1],
        cloud_tint_2: sky_params.weather.cloud_tints[2],
        cloud_tint_3: sky_params.weather.cloud_tints[3],
        // #428 — composite-pass fog needs the camera origin to
        // compute per-pixel world-space distance from a depth
        // sample.
        // #markarth-precision — `inv_view_proj` is the camera-RELATIVE
        // inverse, so composite reconstructs world in relative space.
        // It uses that as `length(worldPos - camera_pos)` (fog
        // distance) + view directions (`screen_to_world_dir` subtracts
        // `camera_pos` from the unprojected far point, #1490), all
        // origin-invariant differences, so supply the camera position
        // in the SAME relative space.
        //
        // REN-D16-01 / #2225 — `w` (previously unused padding) now
        // carries the height-fog reference altitude in the same
        // render-origin-relative space, consumed by
        // `heightFogOpticalDepth`'s `baseHeight` parameter instead of
        // `camera_pos.y` (which made beyond-grid fog follow the
        // camera's own eye height vertically instead of the ground).
        camera_pos: [
            camera_pos[0] - render_origin.x,
            camera_pos[1] - render_origin.y,
            camera_pos[2] - render_origin.z,
            fog_height_reference - render_origin.y,
        ],
        inv_view_proj: inv_vp_arr,
        underwater,
        caustic_flags: [if water_caustic_active { 1.0 } else { 0.0 }, 0.0, 0.0, 0.0],
    }
}

#[cfg(test)]
mod composite_params_tests {
    use super::{build_composite_params, CompositeParamsInputs, SkyParams};

    /// Regression for #2255 (TD1-NEW-02): pin `build_composite_params`'
    /// field mapping now that it's a standalone, directly-testable
    /// function (extracted from `draw_frame`). Distinct values per input
    /// so a transposed pair of same-typed fields (the exact risk a
    /// positional-argument version of this function would have carried)
    /// shows up as a wrong value at a specific index, not a silently
    /// passing test.
    #[test]
    fn maps_fog_sky_and_camera_fields_without_transposition() {
        let sky_params = SkyParams {
            zenith_color: [0.1, 0.2, 0.3],
            horizon_color: [0.4, 0.5, 0.6],
            is_exterior: true,
            sun_size: 0.9998,
            ..SkyParams::default()
        };
        let params = build_composite_params(CompositeParamsInputs {
            fog_color: [0.7, 0.8, 0.9],
            fog_near: 100.0,
            fog_far: 900.0,
            fog_extinction_per_meter: 0.05,
            fog_single_scatter_albedo: 0.6,
            fog_clip: 111.0,
            fog_power: 222.0,
            fog_height_reference: 50.0,
            sky_params: &sky_params,
            render_debug_flags: crate::shader_constants::DBG_VIZ_SHADOW_VISIBILITY,
            render_debug_mode: crate::shader_constants::RENDER_DEBUG_SHADOW_VISIBILITY,
            frame_counter: 42,
            volume_far_distance: 4096.0,
            froxel_slice_count: 64.0,
            camera_pos: [10.0, 20.0, 30.0],
            render_origin: byroredux_core::math::Vec3::new(1.0, 2.0, 3.0),
            inv_vp_arr: [[0.0; 4]; 4],
            underwater: [0.1, 0.2, 0.3, 0.4],
            water_caustic_active: true,
        });

        // `fog_params` carries near/far/clip/power in that exact order —
        // the four fields most at risk of a positional transposition.
        assert_eq!(params.fog_params, [100.0, 900.0, 111.0, 222.0]);
        // fog_color.w is the extinction-enabled flag, not extinction itself.
        assert_eq!(params.fog_color, [0.7, 0.8, 0.9, 1.0]);
        assert_eq!(params.depth_params[0], 1.0, "is_exterior must map through");
        assert_eq!(
            params.depth_params[1].to_bits(),
            crate::shader_constants::DBG_VIZ_SHADOW_VISIBILITY,
            "render_debug_flags must map through without numeric conversion"
        );
        assert_eq!(
            params.depth_params[2].to_bits(),
            crate::shader_constants::RENDER_DEBUG_SHADOW_VISIBILITY,
            "render_debug_mode must map through without numeric conversion"
        );
        assert_eq!(params.volume_params[0], 4096.0);
        assert_eq!(params.sky_zenith, [0.1, 0.2, 0.3, 0.9998]);
        assert_eq!(params.sky_horizon, [0.4, 0.5, 0.6, 64.0]);
        // Camera position and height-fog reference must both be
        // render-origin-relative (#markarth-precision / #2225).
        assert_eq!(
            params.camera_pos,
            [10.0 - 1.0, 20.0 - 2.0, 30.0 - 3.0, 50.0 - 2.0]
        );
        assert_eq!(params.underwater, [0.1, 0.2, 0.3, 0.4]);
        assert_eq!(
            params.caustic_flags[0], 1.0,
            "water_caustic_active must map through to caustic_flags.x"
        );
    }

    /// Regression for #2508: `water_caustic_active: false` must map to
    /// `caustic_flags.x == 0.0` — the fallback state `composite.frag`
    /// gates the water-caustic sum on, so it doesn't double-count the
    /// glass caustic contribution when `waterCausticTex` is aliased to
    /// `causticTex`.
    #[test]
    fn water_caustic_inactive_maps_to_zero_caustic_flag() {
        let sky_params = SkyParams::default();
        let params = build_composite_params(CompositeParamsInputs {
            fog_color: [0.0; 3],
            fog_near: 0.0,
            fog_far: 0.0,
            fog_extinction_per_meter: 0.0,
            fog_single_scatter_albedo: 0.0,
            fog_clip: 0.0,
            fog_power: 0.0,
            fog_height_reference: 0.0,
            sky_params: &sky_params,
            render_debug_flags: 0,
            render_debug_mode: crate::shader_constants::RENDER_DEBUG_FINAL,
            frame_counter: 0,
            volume_far_distance: 0.0,
            froxel_slice_count: 0.0,
            camera_pos: [0.0; 3],
            render_origin: byroredux_core::math::Vec3::ZERO,
            inv_vp_arr: [[0.0; 4]; 4],
            underwater: [0.0; 4],
            water_caustic_active: false,
        });
        assert_eq!(params.caustic_flags[0], 0.0);
    }
}

#[cfg(test)]
mod fsr_frame_parameter_tests {
    use super::{build_fsr_frame_parameters, fsr_gated_dof, DofView};

    /// #2197 — FSR active forces the pinhole path, but must not disturb any
    /// other authored DOF field (they still feed the future
    /// output-resolution DOF implementation).
    #[test]
    fn fsr_zeroes_aperture_and_preserves_every_other_dof_field() {
        let dof = DofView {
            aperture: 2.5,
            focus_dist: 137.0,
            ..DofView::default()
        };
        let gated = fsr_gated_dof(dof, true);
        assert_eq!(gated.aperture, 0.0);
        assert_eq!(gated.focus_dist, 137.0);
        assert_eq!(gated.camera_near, dof.camera_near);
        assert_eq!(gated.camera_far, dof.camera_far);
        assert_eq!(gated.camera_fov_y, dof.camera_fov_y);
        assert_eq!(gated.proj_mat, dof.proj_mat);
    }

    /// With FSR inactive the DOF view passes through untouched — a
    /// non-zero aperture must still reach `dof_effective_view_proj`.
    #[test]
    fn no_fsr_leaves_dof_untouched() {
        let dof = DofView {
            aperture: 2.5,
            ..DofView::default()
        };
        assert_eq!(fsr_gated_dof(dof, false).aperture, 2.5);
    }

    /// No jitter ⇒ FSR is not the active upscaler ⇒ no frame parameters,
    /// and specifically not an error: every non-FSR path takes this branch
    /// every frame.
    #[test]
    fn absent_jitter_yields_no_parameters() {
        let params = build_fsr_frame_parameters(&DofView::default(), None, false, 16.6)
            .expect("no-jitter path must not error");
        assert!(params.is_none());
    }

    /// The reset flag propagates verbatim from the pending-reset input, and
    /// the camera triple is sourced from the (already FSR-gated) DofView.
    #[test]
    fn reset_and_camera_parameters_propagate() {
        let dof = DofView {
            camera_near: 0.5,
            camera_far: 4096.0,
            camera_fov_y: 1.0,
            ..DofView::default()
        };
        let params = build_fsr_frame_parameters(&dof, Some([0.25, -0.5]), true, 8.0)
            .expect("finite perspective must succeed")
            .expect("jitter present ⇒ parameters present");
        assert!(params.reset, "pending reset must reach FSR");
        assert_eq!(params.jitter_offset, [0.25, -0.5]);
        assert_eq!(params.frame_time_delta_ms, 8.0);
        assert_eq!(params.camera_near, 0.5);
        assert_eq!(params.camera_far, 4096.0);
        assert_eq!(params.camera_fov_angle_vertical, 1.0);
    }

    /// No pending reset ⇒ `reset` is false. `draw_frame` is what later ORs
    /// in the camera-cut override (`is_camera_cut`), so this function must
    /// not invent one.
    #[test]
    fn without_pending_reset_the_flag_stays_clear() {
        let params = build_fsr_frame_parameters(&DofView::default(), Some([0.0, 0.0]), false, 16.6)
            .expect("finite perspective must succeed")
            .expect("jitter present ⇒ parameters present");
        assert!(!params.reset);
    }

    /// A camera FSR cannot reconstruct from is fatal for the frame, not a
    /// silently-dropped upscale. `far <= near` is the degenerate case
    /// `fsr_camera_parameters` rejects.
    #[test]
    fn degenerate_perspective_is_an_error() {
        let dof = DofView {
            camera_near: 10.0,
            camera_far: 1.0,
            ..DofView::default()
        };
        assert!(build_fsr_frame_parameters(&dof, Some([0.0, 0.0]), false, 16.6).is_err());
    }

    /// A non-finite fov reaches the same rejection — pinning that the guard
    /// covers NaN/inf, not just ordering.
    #[test]
    fn non_finite_fov_is_an_error() {
        let dof = DofView {
            camera_fov_y: f32::NAN,
            ..DofView::default()
        };
        assert!(build_fsr_frame_parameters(&dof, Some([0.0, 0.0]), false, 16.6).is_err());
    }
}

/// Return `true` when `cmd` represents a real refractive surface that the
/// caustic compute pass (`caustic_splat.comp`) should splat from. The CPU
/// gate produces `INSTANCE_FLAG_CAUSTIC_SOURCE` on the `GpuInstance.flags`
/// word. The mesh-ID attachment preserves a live instance index only for
/// alpha-blended pixels, and the compute pass spends its bounded light/ray
/// budget per flagged pixel, so this gate has to stay tight.
///
/// Accepted refractive signals:
///   * `material_kind == MATERIAL_KIND_GLASS` — engine-classified glass
///     from `render::build_render_data` (low metal + low roughness + not a
///     decal). See #515 / #706.
///   * Skyrim+ `MultiLayerParallax` (kind 11) with a non-zero inner-layer
///     refraction scale — real two-layer refractive surface.
///
/// Rejected (pre-#922 false positives the old `alpha_blend &&
/// metalness < 0.3` gate caught): hair (HairTint, kind 6), foliage (kind 0
/// alpha-test cutouts), particle billboards (kind 0, emissive), decals
/// (`is_decal` excluded by the glass classifier), `BSEffectShaderProperty`
/// FX cards (kind 101 — MATERIAL_KIND_EFFECT_SHADER).
pub(super) fn is_caustic_source(cmd: &DrawCommand) -> bool {
    cmd.alpha_blend && is_refractive_glass(cmd)
}

/// Whether a draw is a refractive glass-family surface.
///
/// The shared material classifier behind two independent consumers, both
/// of which need "real glass, not a blended billboard":
///   * [`is_caustic_source`] — gates `INSTANCE_FLAG_CAUSTIC_SOURCE`.
///   * [`needs_two_sided_blend_split`] — gates the two-pass
///     FRONT-then-BACK cull split (#1804 / #2165).
///
/// Kept as one function deliberately: when the classifier drifts (a new
/// refractive `material_kind`, say), both consumers must move together —
/// they are asking the same question about the same authored material.
///
/// Accepted refractive signals:
///   * `material_kind == MATERIAL_KIND_GLASS` — engine-classified glass
///     from `render::build_render_data` (alpha-blend + low metal + low
///     roughness + not a decal). See #515 / #706.
///   * Skyrim+ `MultiLayerParallax` (kind 11) with a non-zero inner-layer
///     refraction scale — real two-layer refractive surface.
pub(super) fn is_refractive_glass(cmd: &DrawCommand) -> bool {
    if cmd.material_kind == MATERIAL_KIND_GLASS {
        return true;
    }
    if cmd.material_kind == MATERIAL_KIND_MULTI_LAYER_PARALLAX
        && cmd.multi_layer_refraction_scale > 0.0
    {
        return true;
    }
    false
}

/// D6-04 / #1811 — advance `VulkanContext::clean_skin_frames` for one
/// frame. Any dirty signal (a pose changed, or a first-sight
/// `bind_inverses` upload is pending) resets the streak to `0`;
/// otherwise it grows by one. Extracted as a pure function so the
/// counter arithmetic is unit-testable without a live `VulkanContext`.
pub(super) fn next_clean_skin_frames(current: u32, skin_state_dirty: bool) -> u32 {
    if skin_state_dirty {
        0
    } else {
        current.saturating_add(1)
    }
}

/// D6-04 / #1811 — `true` once `clean_skin_frames` has grown past
/// `MAX_FRAMES_IN_FLIGHT`, meaning every per-frame `bone_world` buffer
/// copy has already seen today's (unchanged) content at least once. At
/// that point the bone_world upload, its device copy, and the
/// `skin_palette.comp` dispatch are all redundant until the next dirty
/// frame. Mirrors the `MAX_FRAMES_IN_FLIGHT + 1` safety margin used by
/// `SkinSlotPool::sweep`'s `min_idle` threshold.
pub(super) fn should_skip_skin_gpu_refresh(clean_skin_frames: u32) -> bool {
    clean_skin_frames > MAX_FRAMES_IN_FLIGHT as u32
}

/// A batch of instances sharing the same mesh + pipeline state.
/// Drawn with a single `cmd_draw_indexed` call.
///
/// `pub(super)` so the enclosing `VulkanContext` can hold a reusable
/// `Vec<DrawBatch>` scratch buffer as a field and amortize allocations
/// across frames. See issue #243.
pub(super) struct DrawBatch {
    pub mesh_handle: u32,
    /// Pipeline selector. `Opaque` uses the single prebuilt opaque
    /// pipeline; `Blended { src, dst }` resolves through the lazy
    /// blend pipeline cache on `VulkanContext`. See #392 / #930.
    pub pipeline_key: PipelineKey,
    /// Two-sided / cull-disabled rendering. Drives per-batch
    /// `cmd_set_cull_mode(NONE)` (was a separate pipeline pre-#930).
    /// MUST be part of the merge key so adjacent draws with different
    /// cull state don't fold into one batch.
    pub two_sided: bool,
    /// Content-class layer driving the depth-bias ladder
    /// (Architecture / Clutter / Actor / Decal). Replaces the previous
    /// `is_decal` + per-frame `needs_depth_bias` derivation from
    /// commits 0f13ff5 / ee3cb13 — `RenderLayer::Decal` subsumes both.
    /// Set per-DrawCommand at cell-load time from the REFR's base
    /// record type, with the alpha-test / NIF-decal-flag escalation
    /// rule already applied. Bias values come from
    /// `byroredux_core::ecs::components::RenderLayer::depth_bias`.
    pub render_layer: byroredux_core::ecs::components::RenderLayer,
    pub first_instance: u32,
    pub instance_count: u32,
    pub index_count: u32,
    /// Offset into the global index buffer (in indices). Used with the
    /// global geometry SSBO as `first_index` in `cmd_draw_indexed`. #294.
    pub global_index_offset: u32,
    /// Offset into the global vertex buffer (in vertices). Used with the
    /// global geometry SSBO as `vertex_offset` in `cmd_draw_indexed`. #294.
    pub global_vertex_offset: i32,
    /// `NiZBufferProperty.z_test` — fed to `vkCmdSetDepthTestEnable`
    /// before the batch (extended dynamic state, Vulkan 1.3 core).
    /// Batched into the merge key so consecutive draws sharing depth
    /// state pay zero state-change cost. See #398.
    pub z_test: bool,
    /// `NiZBufferProperty.z_write` — fed to `vkCmdSetDepthWriteEnable`.
    pub z_write: bool,
    /// `NiZBufferProperty.z_function` — fed to `vkCmdSetDepthCompareOp`
    /// (Gamebryo `TestFunction` enum mapped to `vk::CompareOp`).
    pub z_function: u8,
    /// Whether this batch's material is refractive glass
    /// ([`is_refractive_glass`]) — the ONLY population that needs the
    /// two-pass back-then-front cull split.
    ///
    /// Carried explicitly rather than re-derived from depth state.
    /// #1804 originally used `z_write` as the "order-dependent glass"
    /// proxy; `883f57cd` then dropped that limb outright (correctly —
    /// FO4 BGEM glass is commonly authored `z_write == false`), which
    /// re-broadened the split to every two-sided blended batch and put
    /// particle FX back on the 2-direct-draw path #1804 had removed
    /// them from (#2165). The material kind is the real signal; depth
    /// state never was.
    ///
    /// MUST be part of the merge key ([`group_state`]): a non-glass
    /// leader would otherwise swallow a glass batch into its indirect
    /// group and silently drop that batch's split.
    pub order_dependent_glass: bool,
}

/// Indirect-merge key for [`DrawBatch`] (#1581 / F1). Two batches may fold
/// into one `cmd_draw_indexed_indirect` call ONLY when their `group_state`
/// is equal — the key captures every dynamic state the draw loop sets once
/// from the group leader: the pipeline + depth-bias layer, the `two_sided`
/// cull mode (`cmd_set_cull_mode` NONE vs BACK, #930), and the extended-
/// dynamic depth state (`z_test`/`z_write`/`z_function`, #398). Omitting any
/// of these let a single-sided / `z_write=1` leader's state bleed across a
/// boundary onto two-sided cutouts or `z_write=0` halos in the same
/// `(pipeline, layer)` run. The opaque sort already clusters identical state
/// (two_sided + packed depth sort before mesh), so this only splits at
/// genuine state boundaries — no instancing loss within a homogeneous run.
pub(super) fn group_state(
    b: &DrawBatch,
) -> (
    PipelineKey,
    byroredux_core::ecs::components::RenderLayer,
    bool,
    bool,
    bool,
    u8,
    bool,
) {
    (
        b.pipeline_key,
        b.render_layer,
        b.two_sided,
        b.z_test,
        b.z_write,
        b.z_function,
        // #2165 — split-eligibility must split the group too. The
        // gather loop admits a batch on `group_state` equality alone,
        // so without this limb a non-glass two-sided blend leader would
        // absorb a following glass batch and rasterize it in one
        // CULL_NONE indirect draw, losing the back-then-front ordering.
        b.order_dependent_glass,
    )
}

/// Whether a batch needs the two-pass (FRONT-cull then BACK-cull)
/// two-sided alpha-blend split (#1804 / D2-NEW-03).
///
/// The split establishes stable back-face-before-front-face compositing
/// for order-dependent glass. It is required even when depth writes are
/// disabled: FO4 BGEM glass commonly uses `z_write == false`, and a single
/// CULL_NONE draw otherwise interleaves the dome's front/back triangles in
/// mesh index order. TAA jitter then exposes a different blend winner and
/// produces crawling blocks/cross-hatch.
///
/// Eligibility is the material kind ([`DrawBatch::order_dependent_glass`],
/// set at batch formation from [`is_refractive_glass`]), NOT depth state.
/// Both earlier spellings were wrong in opposite directions: `z_write` as
/// a glass proxy (#1804) excluded the FO4 BGEM glass that motivated the
/// split, and dropping the limb entirely (`883f57cd`) re-included every
/// two-sided blended particle batch (#2165). Particle billboards are
/// front-facing by construction, so their FRONT-cull pass rasterizes zero
/// fragments — pure wasted vertex work, plus the batch falls out of
/// indirect grouping into two direct draws.
///
/// # This predicate is structurally dormant for engine-classified glass
///
/// PERF-D2-02 / #2691 — the dormancy repeatedly rediscovered as an empirical
/// observation ("`blended && two_sided == 0` on every measured cell") is
/// actually a **guarantee**, and the guarantee lives upstream:
/// `byroredux::render::static_meshes::collect_static_mesh_draws` — the only
/// producer of glass `DrawCommand`s — unconditionally clears `two_sided` for
/// `MATERIAL_KIND_GLASS` before the command is built. So `b.two_sided` is
/// false for every engine-classified glass batch *by construction*, and the
/// `material_kind == MATERIAL_KIND_GLASS` arm of [`is_refractive_glass`] can
/// never satisfy this predicate.
///
/// The only population that can reach it is an alpha-blended, two-sided,
/// kind-11 (MultiLayerParallax) draw with `multi_layer_refraction_scale > 0`
/// — a rare Skyrim+ authoring case. The other two `DrawCommand` producers are
/// excluded too: `render::particles::emit_particles` hardcodes
/// `MATERIAL_KIND_EFFECT_SHADER` (rejected by `is_refractive_glass`, which is
/// #2165 working as intended), and `render::water::reemit_water_planes` only
/// flips `is_water` on an already-emitted command, which `skip_batch` keeps
/// out of batch formation.
///
/// Consequences worth stating rather than re-deriving:
/// * The #1804/#2237 glass compositing artifact has **two** mitigations, and
///   for engine-classified glass the live one is the single-sided override,
///   not this split — it removes back faces entirely, at the documented cost
///   of glass interiors not rendering.
/// * Changes here are runtime no-ops on all currently-tested content, so
///   batch-count movement must not be attributed to them (cf. #2215, where
///   the depth-primary alpha-over sort was the real cause).
pub(super) fn needs_two_sided_blend_split(b: &DrawBatch) -> bool {
    let is_blend = matches!(b.pipeline_key, PipelineKey::Blended { .. });
    is_blend && b.two_sided && b.order_dependent_glass
}

/// Whether the geometry pass should dispatch via `cmd_draw_indexed_indirect`
/// this frame, or fall back to direct `cmd_draw_indexed` per batch.
///
/// All three inputs must hold: the global vertex/index buffer must be bound
/// (`global_bound` — indirect groups index into it by offset, so there's no
/// per-mesh fallback for it), the device must support multi-draw-indirect,
/// and this frame's indirect-buffer upload must have actually succeeded
/// (`indirect_upload_ok`). The third condition is #2504 /
/// D12-2026-08-07-02: without it, a failed `upload_indirect_draws` (rare —
/// requires a mapped-slice or flush failure) still left the draw loop
/// issuing `cmd_draw_indexed_indirect` over a buffer holding a stale or
/// (on a slot's first use) uninitialized command list — the GPU fetches
/// and executes `index_count`/`vertex_offset` from it, so that's a
/// page-fault/TDR risk, not a misrender.
///
/// #2751 / REN-D12-2026-08-12-01 adds the fourth: the batch count must fit
/// the indirect buffer. `upload_indirect_draws` clamps its write to
/// [`MAX_INDIRECT_DRAWS`] and warns — the RP-1 "log and continue" policy —
/// but the draw loop walked the *unclamped* `batches` slice and derived
/// `byte_offset = i * stride` from it, so an overflowing frame recorded a
/// call whose `offset + drawCount × stride` ran past the allocation
/// (`indirect_buffers[frame]` is sized exactly `MAX_INDIRECT_DRAWS`
/// commands). That violates VUID-vkCmdDrawIndexedIndirect-offset-00556 and
/// has the GPU fetch `indexCount`/`vertexOffset`/`firstInstance` from
/// unallocated memory — device-lost class, the same failure #2504 closed on
/// the upload-failure axis and left open on the overflow axis.
///
/// Rejecting the whole frame rather than clamping the loop is the safer of
/// the two fixes the finding offers: the direct-draw fallback reads no
/// indirect buffer at all, so every batch still renders. Clamping the loop
/// would instead silently drop the tail beyond the ceiling, compounding the
/// producer's own silent drop. Reachability is a deep tail — it needs
/// >262 144 post-merge rasterized batches in one frame, ~20× the densest
///
/// cell this codebase's own comments cite — which is why this is
/// defence-in-depth at an already-declared lossy ceiling rather than a live
/// spec violation.
pub(super) fn should_use_indirect_draws(
    global_bound: bool,
    multi_draw_indirect_supported: bool,
    indirect_upload_ok: bool,
    batch_count: usize,
) -> bool {
    global_bound
        && multi_draw_indirect_supported
        && indirect_upload_ok
        && batch_count <= MAX_INDIRECT_DRAWS
}

/// All per-frame inputs consumed by [`VulkanContext::draw_frame`].
///
/// Groups the (formerly 22) loose `draw_frame` arguments into one struct so
/// the call stays under the argument-count lint. Construction is mechanical:
/// every field is exactly the argument it replaces, in the same order.
pub struct FrameInputs<'a> {
    /// Clear color (RGBA) for the main render pass.
    pub clear_color: [f32; 4],
    /// Combined view-projection matrix as column-major `[f32; 16]`.
    pub view_proj: &'a [f32; 16],
    /// Per-object draw commands (mesh handle + model matrix + flags).
    pub draw_commands: &'a [DrawCommand],
    /// Scene lights for this frame.
    pub lights: &'a [scene_buffer::GpuLight],
    /// Spatially bounded authored participating-medium primitives.
    pub fog_volumes: &'a [super::super::volumetrics::GpuFogVolume],
    /// M29.5/M29.6 — per-frame bone-world matrices for the GPU palette
    /// compute pass (`skin_palette.comp`). `bone_world[i]` is the per-slot
    /// raw world transform sourced from `GlobalTransform`; indexed by
    /// `skin_slot_id × MAX_BONES_PER_MESH` via the `SkinSlotPool`. The
    /// matching `bind_inverses` for each slot live in the persistent SSBO
    /// and are uploaded first-sight via `bind_inverse_pending_uploads`.
    pub bone_world: &'a [[[f32; 4]; 4]],
    /// Sparse `bone_world` offsets keyed by skinned entity. Used to narrow
    /// the staging/device transfer to dirty slots while retaining the dense
    /// shader input layout.
    pub skin_offsets: &'a rustc_hash::FxHashMap<EntityId, u32>,
    /// M29.6 — first-sight `bind_inverses` uploads to schedule this frame.
    /// Each entry is `(slot_id, per-mesh bind_inverses)`; the renderer writes
    /// them into the persistent SSBO at the slot's offset before dispatching
    /// `skin_palette.comp`. Empty on frames with no fresh skinned-mesh
    /// first-sight (the steady-state case).
    pub bind_inverse_pending_uploads: &'a [(u32, Vec<[[f32; 4]; 4]>)],
    /// Per-frame materials.
    pub materials: &'a [GpuMaterial],
    /// Scene-level feature bit collected from loaded material sources. When
    /// set, the post-geometry pass must refresh the depth history image for
    /// soft-particle fade. This is deliberately not recomputed from the draw
    /// list at the copy site: a newly appearing effect is already represented
    /// before this frame is recorded.
    pub has_effect_soft_material: bool,
    /// Camera world position.
    pub camera_pos: [f32; 3],
    /// Cell-grid-snapped render origin (`scene_buffer::snap_render_origin`
    /// applied to the same un-jittered camera position used to build the
    /// relative `view_proj`). Computed once by `render::camera::assemble_
    /// camera` in the binary and threaded here so this consumer and that
    /// one can't independently compute — and potentially disagree on —
    /// the origin (#2043 / PERF-D9-04).
    pub render_origin: [f32; 3],
    /// Ambient light color.
    pub ambient_color: [f32; 3],
    /// Linear fog color.
    pub fog_color: [f32; 3],
    /// Fog near distance.
    pub fog_near: f32,
    /// Fog far distance.
    pub fog_far: f32,
    /// Engine-native extinction coefficient σ_t in inverse metres.
    pub fog_extinction_per_meter: f32,
    /// Single-scatter albedo ρ = σ_s / σ_t.
    pub fog_single_scatter_albedo: f32,
    /// Nubis-style procedural density coverage in `[0, 1]`.
    pub fog_coverage: f32,
    /// XCLL FNV+ cubic-fog clip distance retained for diagnostics and
    /// explicit legacy compatibility.
    ///
    /// **Currently unconsumed** (#1926, #1927 / REN-D8-01, REN-D8-02):
    /// `composite.frag` parsed and mixed this curve inside the
    /// aerial-perspective fog fallback, but that branch was gated
    /// `is_exterior`-only — meaningless for the FNV interiors (Doc
    /// Mitchell's House, Goodsprings Source Pump) the curve was authored
    /// for, and it mixed toward sky-haze rather than `fog_color` in any
    /// case. #1926 removed that dead branch entirely once
    /// `VOLUMETRIC_OUTPUT_CONSUMED` made it permanently unreachable.
    /// `fog_clip`/`fog_power` are still parsed from XCLL and uploaded,
    /// reserved for a future interior-scoped composite branch that mixes
    /// toward `fog_color` — not resurrected as-is.
    pub fog_clip: f32,
    /// XCLL FNV+ cubic-fog falloff exponent. `0.0` disables the curve.
    /// See the `fog_clip` doc for current unconsumed status.
    pub fog_power: f32,
    /// World-space Y anchor for height-fog density (REN-D16-01 / #2225) —
    /// a downward ray-cast from the camera against static collision, or
    /// the camera's own Y as a fallback when no ground is found below.
    /// Threaded through to `VolumetricsParams`/`CompositeParams` in place
    /// of the camera's raw Y, which pre-fix made fog density follow the
    /// player vertically instead of thinning with real altitude.
    pub fog_height_reference: f32,
    /// Shared atmospheric wind for volumetric advection. `[x, z, base_speed,
    /// gust_amplitude]`, with the speeds in renderer world units per second;
    /// the shader applies height shear and keeps this external to local
    /// combustion velocity.
    pub wind_params: [f32; 4],
    /// `x = gust_frequency` in cycles per second. Remaining lanes are spare
    /// std140-compatible slots for future atmospheric shear controls.
    pub wind_gust: [f32; 4],
    /// Optional UI overlay texture handle.
    pub ui_texture_handle: Option<u32>,
    /// Sky / weather parameters.
    pub sky_params: &'a SkyParams,
    /// Depth-of-field lens parameters. `dof.aperture == 0.0` = pinhole camera
    /// (no DOF jitter). When non-zero, the camera position is displaced each
    /// frame by a Halton(5,7)-sampled concentric disk of radius `aperture`;
    /// TAA accumulates the samples into a spatially-varying bokeh blur.
    pub dof: DofView,
    /// CPU frame delta supplied to temporal reconstruction, in milliseconds.
    pub frame_time_delta_ms: f32,
    /// Optional per-frame GPU timing sink.
    pub timings: Option<&'a mut FrameTimings>,
    /// Water-surface draws for this frame. Each entry must match a
    /// `DrawCommand` with `is_water=true` that supplies the corresponding
    /// `GpuInstance` SSBO slot. Empty slice = no water rendering this frame.
    pub water_commands: &'a [WaterDrawCommand],
    /// `xyz` = deep_color tint to blend the scene toward; `w` = camera depth
    /// below the water surface in world units. `[0, 0, 0, 0]` disables
    /// underwater FX.
    pub underwater: [f32; 4],
    /// Timed Bethesda IMAD lens and color-grade channels.
    pub image_space_modifier: ImageSpaceModifierView,
    /// #1195 / PERF-DIM7-01 — per-frame dirty set for the skin compute
    /// dispatch + skinned-BLAS refit gate. Entities NOT in this set whose
    /// slots already have `has_populated_output = true` AND a live BLAS skip
    /// both compute dispatch and refit. First-sight (no populated output yet)
    /// ignores the set and always dispatches. Paired with #1196.
    pub pose_dirty: &'a rustc_hash::FxHashSet<EntityId>,
}

impl VulkanContext {
    /// Publish CPU-staged morph weights after the dual-fence wait has proven
    /// no prior submission can still read their mapped buffers (#3244).
    pub(super) fn flush_pending_morph_weights(&mut self) -> Result<()> {
        for (&entity, slot) in &mut self.morph_slots {
            slot.flush_pending_weights(&self.device)
                .with_context(|| format!("flush MorphSlot weights for entity {entity}"))?;
        }
        Ok(())
    }

    /// Whether FSR is not merely the *selected* upscaler mode but is
    /// actually dispatching this frame (#2518).
    ///
    /// `self.fsr_temporal.is_some()` answers a different question: it stays
    /// `Some` for the whole of `UpscalerMode::Fsr3(..)`, including when the
    /// FSR context never got created or `dispatch_failure` has latched. In
    /// those states the frame falls back to an unjittered native blit, so
    /// every "is FSR's projection jitter in play?" decision — the camera
    /// jitter itself and the DOF gate that exists to avoid conflicting with
    /// it — must key on this, not on mode selection. Sharing one accessor
    /// is what keeps the two from drifting apart again.
    ///
    /// #3632 — also `false` whenever `record_upscale_pass` is about to pass
    /// `force_native_blit: true` into `FrameUpscaler::record` (any render-
    /// debug view that requires raw, unreconstructed output). That path
    /// bridges straight to a native blit exactly like a missing context or a
    /// latched `dispatch_failure` does — the frame is never reconstructed —
    /// so it must fall out of this predicate the same way, or the jitter and
    /// DOF gates above apply FSR's sub-pixel offset to a frame nothing ever
    /// resolves it back out of. `self.render_debug_flags` / `render_debug_
    /// mode` are frame-stable (only a console command changes them, never
    /// mid-frame), so evaluating the same predicate again in
    /// `record_upscale_pass` cannot disagree with this one.
    pub(super) fn is_fsr_dispatch_active(&self) -> bool {
        self.frame_upscaler
            .as_ref()
            .is_some_and(|upscaler| upscaler.is_fsr_dispatch_active())
            && !crate::shader_constants::render_debug_requires_raw_output(
                self.render_debug_flags,
                self.render_debug_mode.shader_value(),
            )
    }

    pub fn draw_frame(&mut self, inputs: FrameInputs) -> Result<bool> {
        let FrameInputs {
            clear_color,
            view_proj,
            draw_commands,
            lights,
            fog_volumes,
            bone_world,
            skin_offsets,
            bind_inverse_pending_uploads,
            materials,
            has_effect_soft_material,
            camera_pos,
            render_origin: input_render_origin,
            ambient_color,
            fog_color,
            fog_near,
            fog_far,
            fog_extinction_per_meter,
            fog_single_scatter_albedo,
            fog_coverage,
            fog_clip,
            fog_power,
            fog_height_reference,
            wind_params,
            wind_gust,
            ui_texture_handle,
            sky_params,
            dof,
            frame_time_delta_ms,
            timings,
            water_commands,
            underwater,
            image_space_modifier,
            pose_dirty,
        } = inputs;
        // #1796 / D6-02 — reset before either early-return guard below so
        // a bailed frame reads `false`; see the field doc on `skin_dispatch_ran`.
        self.skin_dispatch_ran = false;
        // #3569 / D9-01 — reset alongside `skin_dispatch_ran`: this frame's
        // upload hasn't happened yet, so any stale `true` from a previous
        // frame's failure must not leak into this frame's rollback check.
        self.bind_inverse_upload_failed = false;
        // #2112 / D6-01 — same reasoning as `skin_dispatch_ran` above: reset
        // before the early-return guard so a bailed frame reads zero
        // instead of retaining the previous frame's counters. Frame without
        // a skinned section (no RT, no bone buffer) also reads zero.
        // Section-local increments below populate `last_skin_coverage_frame`;
        // `fill_skin_coverage_stats` snapshots it after `Scheduler::run`.
        self.last_skin_coverage_frame = super::super::skin_compute::SkinCoverageFrame::default();
        // Reset per-frame draw-call counts. Populated after the batch
        // merge (`batch_count`) and inside the indirect-grouping draw
        // loop below (`indirect_call_count`). Read by the app's stats
        // wiring after `draw_frame` returns to populate `DebugStats`.
        // #1258 / PERF-D3-NEW-03.
        self.last_draw_call_stats = super::DrawCallStats::default();
        // #1211 / REN-SAFETY — skip the frame when the main framebuffers
        // Vec is empty. `recreate_swapchain` destroys framebuffers up
        // front and only rebuilds them at the end (`resize.rs:564`);
        // any `?`-propagated failure between those two points leaves
        // the Vec at `len == 0`. The app-level caller logs the recreate
        // error and queues `event_loop.exit()`, but exit is queued —
        // the next `RedrawRequested` already in flight would index
        // `framebuffers[frame]` and panic.
        //
        // Return BEFORE `acquire_next_image` so `image_available[frame]`
        // is not left signal-pending without a paired wait. `Ok(false)`
        // (not `Ok(true)`) avoids a recreate-retry loop when the
        // underlying surface is still invalid — recovery rides the
        // next `Resized` / focus event instead.
        if self.framebuffers.is_empty() {
            return Ok(false);
        }

        let mut armed_selected_ray_probe_generation = None;
        let volumetric_time_seconds = self.volumetric_time_seconds;
        // Use a local to avoid borrow complexity; copy out at end.
        let mut t = FrameTimings::default();

        // #3282 / TD1-2026-08-24-01 — `draw_frame` was split into 5
        // sibling-file phases (mirroring the existing `record_geometry_pass`
        // / `record_post_passes` extraction pattern) to shrink this
        // function. Pure code motion: every barrier, dispatch, and
        // recording order below is unchanged from the pre-split function —
        // see each phase's own module doc for its exact scope.
        let Some((frame, img, suboptimal)) = self.sync_and_acquire_frame(&mut t)? else {
            return Ok(true);
        };

        let BeginFrameOutput {
            cmd,
            clear_values,
            instance_map,
            tlas_t0,
        } = self.begin_frame_recording(frame, draw_commands, clear_color, sky_params)?;

        // Main framebuffer is now per-frame-in-flight (not per-swapchain-image).
        // Each frame slot has its own HDR color image, so no read-after-write
        // hazard across overlapping frames.
        let render_pass_begin = vk::RenderPassBeginInfo::default()
            .render_pass(self.render_pass)
            .framebuffer(self.framebuffers[frame])
            .render_area(vk::Rect2D {
                offset: vk::Offset2D { x: 0, y: 0 },
                extent: self.frame_extents.render,
            })
            .clear_values(&clear_values);

        let CameraAssemblyOutput {
            frame_lights,
            camera_cut,
            camera_static,
            effective_vp,
            pvp,
            inv_vp_arr,
            render_origin,
            previous_camera_position,
            fsr_frame,
        } = self.assemble_camera_and_lights(
            frame,
            lights,
            fog_volumes,
            view_proj,
            camera_pos,
            input_render_origin,
            ambient_color,
            fog_color,
            fog_near,
            fog_far,
            fog_extinction_per_meter,
            sky_params,
            dof,
            frame_time_delta_ms,
        )?;
        let vp = &effective_vp;

        self.dispatch_skin_and_cluster(
            cmd,
            frame,
            draw_commands,
            bone_world,
            skin_offsets,
            bind_inverse_pending_uploads,
            pose_dirty,
            &instance_map,
            tlas_t0,
            &mut t,
        );

        let lights = frame_lights.as_slice();
        let BuildInstancesOutput {
            gpu_instances,
            previous_models,
            mut current_rigid_models,
            batches,
            ui_instance_idx,
            caustic_history_valid,
        } = self.build_and_upload_instances(
            cmd,
            frame,
            draw_commands,
            render_origin,
            camera_cut,
            camera_static,
            pose_dirty,
            lights,
            &instance_map,
            ui_texture_handle,
            materials,
            fog_color,
            fog_near,
            fog_far,
            fog_extinction_per_meter,
            fog_single_scatter_albedo,
            fog_clip,
            fog_power,
            fog_height_reference,
            sky_params,
            camera_pos,
            inv_vp_arr,
            underwater,
            water_commands,
            &mut armed_selected_ray_probe_generation,
            &mut t,
        );

        // #3837 — hand the lights Vec back to its field as soon as the borrow
        // above (`lights`) is dead, rather than ~400 lines later at the tail.
        // `assemble_camera_and_lights` vacated the field with `mem::take`, and
        // three `return Err` sites (`end_command_buffer`, `reset_fences`,
        // submit — the same recovery paths #910 hardened, so reachable in
        // practice on swapchain churn) sat between there and the old restore
        // point. On any of them the taken Vec dropped and next frame regrew
        // from zero. Restoring here removes the window structurally instead of
        // adding a restore to each site, so a future early return added below
        // cannot reintroduce it.
        //
        // The tail still owns the shrink policy; it now reads the length from
        // the field.
        //
        // SIBLING (#3837): `gpu_instances_scratch` and `previous_models_scratch`
        // are taken by `build_and_upload_instances` and restored at the same
        // tail, so they had the identical window. Neither is read between the
        // destructure above and that tail, so both come back here too.
        // `batches_scratch` is the same case but has one more use
        // (`record_geometry_pass`), so it is restored just after it.
        self.frame_lights_scratch = frame_lights;
        self.gpu_instances_scratch = gpu_instances;
        self.previous_models_scratch = previous_models;

        let cmd_t0 = Instant::now();
        self.record_geometry_pass(
            cmd,
            frame,
            &render_pass_begin,
            &batches,
            draw_commands,
            water_commands,
        );
        // #3837 — last use of `batches`; hand it back before the three
        // `return Err` sites below (see the sibling note above).
        self.batches_scratch = batches;
        // SAFETY: tail of the per-frame command buffer — depth-history
        // snapshot, post/denoise/composite chain, egui overlay, screenshot
        // copy, and `end_command_buffer`. Each call documents its own
        // recording-order contract; this is the same single `unsafe` scope
        // `draw_frame` opened before the geometry pass was extracted (#1748).
        unsafe {
            // Publish the bounded fragment-shader probe record to the host.
            // The matching CPU read occurs only after this slot's fence wait
            // on its next use and performs a non-coherent invalidate first.
            memory_barrier(
                &self.device,
                cmd,
                vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::AccessFlags::SHADER_WRITE,
                vk::PipelineStageFlags::HOST,
                vk::AccessFlags::HOST_READ,
            );

            // Soft-particle depth fade: when the scene-level material bit is
            // set, snapshot this frame's opaque depth into the sampleable
            // history image so effect-shader FX can feather their alpha
            // against the geometry behind them. The transparent FX wrote no
            // depth (z_write off), so the depth buffer here holds opaque-only
            // depth. The helper restores depth to READ_ONLY afterwards so
            // SSAO / SVGF / composite read it unchanged. When the bit is
            // clear, both images remain in their normal read-only layouts and
            // the full-resolution copy plus its barriers are omitted. See
            // `crates/renderer/shaders/triangle.frag` (soft-fade block).
            if has_effect_soft_material {
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_depth_history_copy_start(&self.device, cmd, frame);
                }
                self.copy_depth_to_history(cmd);
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_depth_history_copy_end(&self.device, cmd, frame);
                }
            }
            // #3308 — the render pass leaves the depth image in
            // DEPTH_STENCIL_READ_ONLY_OPTIMAL. A history copy, when enabled
            // above, restores that same layout before this helper; when the
            // copy is skipped, the layout is already the precondition
            // `depth_capture_record_copy` requires and restores.
            // SAFETY: `cmd` is recording outside any render pass here (same
            // contract `copy_depth_to_history` on the line above relies on),
            // and the depth image is in DEPTH_STENCIL_READ_ONLY_OPTIMAL.
            // Already inside this function's enclosing `unsafe` block.
            self.depth_capture_record_copy(cmd);

            // #1255 / Phase C of #1210 — sequence water.frag's
            // imageAtomicAdd writes (FRAGMENT_SHADER WRITE during the
            // main pass) so composite's FRAGMENT_SHADER READ in the
            // composite pass sees them. Render-pass-end is implicit
            // sync for color-attachment writes; descriptor-image
            // atomic writes need an explicit barrier. Skipped when
            // the accumulator failed init.
            self.record_post_passes(
                cmd,
                frame,
                img,
                caustic_history_valid,
                camera_pos,
                render_origin,
                vp,
                &pvp,
                inv_vp_arr,
                previous_camera_position,
                self.frame_counter,
                volumetric_time_seconds,
                sky_params,
                fog_color,
                fog_far,
                fog_extinction_per_meter,
                fog_single_scatter_albedo,
                fog_coverage,
                fog_height_reference,
                wind_params,
                wind_gust,
                fog_volumes,
                fsr_frame,
                underwater,
                image_space_modifier,
                ui_instance_idx,
            );

            // Debug-UI overlay (Phase 4 of the debug-UI plan).
            // The presentation pass (FSR 3.1 tail, `presentation.rs`) —
            // not composite — already wrote the swapchain image and left
            // it in PRESENT_SRC_KHR; the egui RP keeps that layout
            // via loadOp=LOAD + matching initial/final layouts, so
            // the only thing this needs is a fresh begin/end inside
            // the same command buffer. Skipped unless both
            // `init_egui` ran AND a frame was submitted via
            // `submit_egui_frame` this iteration.
            if let Some(pass) = self.egui_pass.as_mut() {
                if let Some((egui_ctx, output)) = self.egui_pending_output.take() {
                    // Pass the queue Mutex by reference: `dispatch` locks it
                    // only around the internal `set_textures` upload, not
                    // across tessellate + cmd_draw (which just record into
                    // `cmd`). CONC-D1-01 (#1713) — the pre-fix code held this
                    // guard across the entire dispatch call.
                    if let Err(e) = pass.dispatch(
                        crate::vulkan::egui_pass::EguiDispatchCtx {
                            device: &self.device,
                            cmd,
                            queue: &self.graphics_queue,
                            upload_command_pool: self.transfer_pool,
                        },
                        img as u32,
                        &egui_ctx,
                        output,
                    ) {
                        log::error!("egui overlay dispatch failed: {e:#}");
                    }
                }
            }

            // Screenshot capture: copy swapchain image to staging buffer
            // if requested. Must happen after composite (image has content)
            // and before end_command_buffer (still recording).
            let swapchain_image = self.swapchain_state.images[img];
            self.screenshot_record_copy(cmd, swapchain_image);

            if let Err(e) = self
                .device
                .end_command_buffer(cmd)
                .context("end_command_buffer")
            {
                // Drop out of the inner `unsafe { ... }` block — we
                // can't call `&mut self` recovery while a closure-style
                // recovery is held; do it in the outer scope below.
                // The `?`-replacement here mirrors the other 5 sites:
                // see #910 / REN-D5-NEW-01 (acquire-signal leak).
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                return Err(e);
            }
        }
        t.cmd_record_ns = cmd_t0.elapsed().as_nanos() as u64;

        // Submit.
        let submit_t0 = Instant::now();
        let wait_semaphores = [self.frame_sync.image_available[frame]];
        let wait_stages = [vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
        // render_finished is PER SWAPCHAIN IMAGE. Re-using the same
        // semaphore on a per-frame-in-flight cycle (the pre-revert #906
        // pattern) trips VUID-vkQueueSubmit-pSignalSemaphores-00067
        // whenever swapchain_image_count > MAX_FRAMES_IN_FLIGHT: the
        // slot's submit re-signals `render_finished[slot]` while a
        // prior present on a different image is still tracking the
        // same handle. Per-image keys off the acquire boundary —
        // `acquire_next_image` returning `image_index` guarantees the
        // prior present of that image (and its semaphore consumption)
        // has completed. See `sync::FrameSync` doc for the full
        // rationale + the Khronos issue 2007 MAILBOX-discard
        // clarification that made this safe again.
        let signal_semaphores = [self.frame_sync.render_finished[img]];
        let command_buffers_to_submit = [cmd];

        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(&wait_semaphores)
            .wait_dst_stage_mask(&wait_stages)
            .command_buffers(&command_buffers_to_submit)
            .signal_semaphores(&signal_semaphores);

        // #952 / REN-D1-NEW-04 — `reset_fences` lands HERE, immediately
        // before `queue_submit`. The Vulkan spec only requires the
        // fence to be unsignaled at the moment of submit; resetting
        // any earlier opens a deadlock window if a `?`-propagated
        // error fires between the reset and the submit (was ~2200
        // lines pre-fix, see the moved-from comment higher up).
        // SAFETY: `in_flight[frame]` is live and (per the spec) need only be unsignaled at submit time; resetting it here, immediately before `queue_submit` re-signals it, leaves no deadlock window. On reset failure the fence stays SIGNALED (so next frame's wait won't hang) and we clear the pending acquire signal.
        unsafe {
            if let Err(e) = self
                .device
                .reset_fences(&[self.frame_sync.in_flight[frame]])
                .context("reset_fences")
            {
                // Pre-submit failure: the fence is still in its prior
                // SIGNALED state (the reset is what would have moved it
                // — and just errored), so the next frame's wait won't
                // hang. The acquired `image_available[frame]` slot
                // stays signal-pending though, so mirror the submit-
                // failure recovery to clear it.
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                return Err(e);
            }
        }

        // SAFETY: queue access is serialized by `graphics_queue`'s Mutex held across the call (VUID-vkQueueSubmit-queue-00893); `cmd` was just closed by `end_command_buffer`, `image_available[frame]` is the wait semaphore and `in_flight[frame]` (just reset) is the signal fence. `cmd` is not re-recorded until that fence is next waited on. On failure both the acquire signal and the fence are recreated before propagating.
        unsafe {
            // Bind the MutexGuard, deref inside the call — `*self
            // .graphics_queue.lock()` would release the guard end-of-
            // statement (vk::Queue is Copy) before `queue_submit` ran,
            // defeating VUID-vkQueueSubmit-queue-00893 the Mutex was
            // added to enforce. Mirrors the present-queue site below.
            // See CONC-D2-NEW-01 (audit 2026-05-16).
            let queue = self
                .graphics_queue
                .lock()
                .expect("graphics queue lock poisoned");
            if let Err(e) = self
                .device
                .queue_submit(*queue, &[submit_info], self.frame_sync.in_flight[frame])
                .context("queue_submit")
            {
                // Submit failed — `image_available[frame]` was never
                // consumed by the (would-be) wait, so it stays signal-
                // pending. Recover before propagating so the next
                // acquire on this slot doesn't trip
                // VUID-vkAcquireNextImageKHR-semaphore-01779.
                // #910 / REN-D5-NEW-01.
                drop(queue);
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                // #952 / REN-D1-NEW-04 — the reset_fences just above
                // succeeded, so `in_flight[frame]` is UNSIGNALED with
                // no pending submit (this one just failed). Recreate
                // it as SIGNALED so the next frame's
                // `wait_for_fences(..., u64::MAX)` doesn't block forever.
                let _ = self
                    .frame_sync
                    .recreate_in_flight_for_frame(&self.device, frame);
                return Err(e);
            }
            drop(queue);
        }

        // #2715 (CONC-D7-UI-01) — `queue_submit` above just created a new
        // pending submission against `bindless_sets[frame]`, so
        // `TextureRegistry::apply_descriptor_write`'s immediate-write fast
        // path may no longer target this slot until the next `begin_frame`
        // (post fence-wait) call re-confirms it idle.
        self.texture_registry.note_frame_submitted(frame);
        if self
            .pending_selected_ray_probe
            .is_some_and(|request| Some(request.generation) == armed_selected_ray_probe_generation)
        {
            self.pending_selected_ray_probe = None;
        }

        // #917 / REN-D10-NEW-03 — advance SVGF + TAA `frames_since_
        // creation` counters now that `queue_submit` returned success.
        // Each pipeline self-gates on its `dispatched_this_frame` flag
        // set during recording, so a skipped dispatch (svgf_failed
        // latch, missing pipeline, upload_params failure) is a no-op
        // here. Pre-fix the counters advanced at record time, meaning a
        // record-time / submit-time failure between them and submit
        // success would leave the counter advanced without the
        // corresponding GPU write — the next frame would assume valid
        // history that wasn't actually written.
        if let Some(ref mut svgf) = self.svgf {
            svgf.mark_frame_completed();
        }
        if let Some(ref mut taa) = self.taa {
            taa.mark_frame_completed();
        }
        if let Some(ref mut volumetrics) = self.volumetrics {
            volumetrics.mark_frame_completed();
        }
        self.volumetric_time_seconds += frame_time_delta_ms.max(0.0) * 0.001;
        if self
            .frame_upscaler
            .as_mut()
            .is_some_and(|upscaler| upscaler.take_submitted_dispatch())
        {
            self.fsr_temporal
                .as_mut()
                .expect("submitted FSR dispatch requires temporal state")
                .mark_dispatch_completed();
        }
        // Object-transform history follows successful GPU submission, not
        // command recording or presentation. This mirrors TAA/SVGF history:
        // a failed submit cannot advance the source frame motion reprojects.
        std::mem::swap(&mut self.previous_rigid_models, &mut current_rigid_models);
        current_rigid_models.clear();
        self.current_rigid_models_scratch = current_rigid_models;
        // #2486 / D5-01 — same shrink policy the two scratch Vecs get at the
        // bottom of this function. Both maps are `clear()`-then-`reserve(
        // draw_commands.len())`, so without this their capacity is the session
        // high-water mark rather than the working set, and one large-exterior
        // peak stays resident through the walk back into a small interior.
        // `previous_rigid_models` post-swap holds this frame's entries, which
        // is the working set for both.
        let working_rigid = self.previous_rigid_models.len();
        super::super::acceleration::shrink_map_scratch_if_oversized(
            &mut self.previous_rigid_models,
            working_rigid,
            512,
        );
        super::super::acceleration::shrink_map_scratch_if_oversized(
            &mut self.current_rigid_models_scratch,
            working_rigid,
            512,
        );

        // Present.
        let swapchains = [self.swapchain_state.swapchain];
        let image_indices = [img as u32];
        let present_info = vk::PresentInfoKHR::default()
            .wait_semaphores(&signal_semaphores)
            .swapchains(&swapchains)
            .image_indices(&image_indices);

        // SAFETY: present-queue access is serialized by `present_queue`'s Mutex held across the call; `render_finished[img]` (signaled by the submit above) is the present wait semaphore, and `swapchain` + `image_index` are the live acquired image. The OUT_OF_DATE arm degrades to `suboptimal=true` instead of touching stale state.
        let present_suboptimal = unsafe {
            let pq = self
                .present_queue
                .lock()
                .expect("present queue lock poisoned");
            match self
                .swapchain_state
                .swapchain_loader
                .queue_present(*pq, &present_info)
            {
                Ok(suboptimal) => suboptimal,
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => true,
                Err(e) => anyhow::bail!("queue_present: {:?}", e),
            }
        };

        t.submit_present_ns = submit_t0.elapsed().as_nanos() as u64;
        if let Some(out) = timings {
            *out = t;
        }

        self.current_frame = (self.current_frame + 1) % MAX_FRAMES_IN_FLIGHT;
        self.frame_counter = self.frame_counter.wrapping_add(1);

        // Restore the scratch buffers to the context so their capacity
        // amortizes across frames (#243), then shrink them back toward
        // the working set after a past peak frame. Same policy as the
        // `tlas_instances_scratch` in #504 — scratch Vecs behave as
        // "grow fast, shrink on pressure": working-set × 2 keeps a
        // slack band against frame-to-frame variance, and the 512
        // floor avoids reallocations on common-case small scenes.
        // #3837 — all four scratch Vecs are restored above, each right after
        // its last use, so none of them is vacated across the error paths
        // between here and there. The shrink policy below is unchanged; it
        // just reads its working-set lengths from the fields now.
        let working_instances = self.gpu_instances_scratch.len();
        let working_lights = self.frame_lights_scratch.len();
        let working_previous = self.previous_models_scratch.len();
        let working_batches = self.batches_scratch.len();
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.gpu_instances_scratch,
            working_instances,
            512,
        );
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.frame_lights_scratch,
            working_lights,
            128,
        );
        // #2486 / D5-01 — `previous_models_scratch` was restored here but
        // never shrunk, so it pinned its peak (~16 MB at `MAX_INSTANCES`) for
        // the session. It grows one entry per instance, so its own `len()` is
        // the working set.
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.previous_models_scratch,
            working_previous,
            512,
        );
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.batches_scratch,
            working_batches,
            512,
        );

        // #645 / MEM-2-3 — TLAS instance buffer mirrored shrink. The
        // slot we just incremented to (`current_frame` after the line
        // above) is the one whose previous frame work signalled at
        // the start of this frame, so its instance / staging /
        // device-local buffers are GPU-idle at this point and safe to
        // tear down. The slot we just SUBMITTED on (the one before
        // the increment) stays in flight and is left alone.
        //
        // SAFETY: see precondition on
        // `AccelerationManager::shrink_tlas_to_fit` — caller must
        // ensure no in-flight command buffer references the target
        // slot. The `current_frame_after_increment` slot's fence was
        // waited on at the start of this frame's recording (the
        // standard MAX_FRAMES_IN_FLIGHT alternation), so by the time
        // we reach this line its previous use has completed by
        // construction. Same justification used by `#504` for the
        // CPU-side scratch shrink above.
        if let Some(accel) = self.accel_manager.as_mut() {
            if let Some(allocator) = self.allocator.as_ref() {
                let slot_to_shrink = self.current_frame;
                unsafe {
                    // SAFETY: `accel`, `device` and `allocator` are live; the
                    // shrink runs on this frame slot after its prior GPU use
                    // completed (the caller's fence wait), so the freed TLAS
                    // scratch/buffers are not referenced by an in-flight build.
                    accel.shrink_tlas_to_fit(
                        slot_to_shrink,
                        working_instances as u32,
                        &self.device,
                        allocator,
                    );
                    // #682 / MEM-2-7 — TLAS build scratch shrink. Same
                    // safety justification as `shrink_tlas_to_fit`
                    // above (the slot's previous use completed before
                    // this frame's recording began). Since #2929,
                    // `shrink_tlas_to_fit` no longer destroys the slot (it
                    // sets a pending-shrink flag folded in by
                    // `ensure_tlas_state`), so this call's ordering relative
                    // to it is no longer load-bearing — the "tlas[slot] is
                    // None" arm this used to chase in one tick is reached
                    // only by a fresh/never-rebuilt slot now, independent of
                    // call order.
                    accel.shrink_tlas_scratch_to_fit(slot_to_shrink, &self.device, allocator);
                }
            }
        }

        Ok(suboptimal || present_suboptimal)
    }
}

// #3632 — `VulkanContext::is_fsr_dispatch_active` needs a live Vulkan device
// to exercise end-to-end (its inputs are `self.frame_upscaler` and the
// render-debug fields), so — matching this file's `composite_params_tests`
// / this crate's `fsr_startup_failure_promotes_to_taa_tests` pattern — this
// pins the fix at the source level: the raw-output predicate must be AND-ed
// into the upscaler check, never OR-ed or left independent, so it can only
// narrow `true` to `false` and never manufacture a `true` the upscaler
// check didn't already produce.
#[cfg(test)]
mod is_fsr_dispatch_active_tests {
    fn production_src() -> &'static str {
        include_str!("draw.rs")
    }

    #[test]
    fn folds_the_raw_output_debug_predicate_into_the_fsr_dispatch_check() {
        let src = production_src();
        let fn_start = src
            .find("pub(super) fn is_fsr_dispatch_active(&self) -> bool {")
            .expect("is_fsr_dispatch_active must still exist with this signature");
        let fn_end = src[fn_start..]
            .find("\n    pub fn draw_frame(")
            .map(|offset| fn_start + offset)
            .expect("is_fsr_dispatch_active must be immediately followed by draw_frame");
        let body = &src[fn_start..fn_end];

        let upscaler_check_pos = body
            .find("is_some_and(|upscaler| upscaler.is_fsr_dispatch_active())")
            .expect("must still start from FrameUpscaler's own dispatch-active check");
        let raw_output_pos = body.find("render_debug_requires_raw_output(").expect(
            "#3632 — the accessor must also gate on force_native_debug's raw-output \
             predicate, or a debug view that bridges straight to a native blit is left \
             jittered by a jitter gate that thinks FSR is still reconstructing",
        );
        assert!(
            upscaler_check_pos < raw_output_pos,
            "the upscaler dispatch check must come first, matching the doc comment's framing"
        );
        assert!(
            body[upscaler_check_pos..raw_output_pos].contains("&&")
                && body[upscaler_check_pos..raw_output_pos].contains('!'),
            "the raw-output predicate must be AND-ed in as a negated (suppressing) \
             condition, never OR-ed — it can only turn `true` into `false`, never \
             manufacture a `true` the upscaler check didn't already produce"
        );
    }
}

/// Rebase an absolute column-major model matrix into the current camera-relative
/// render-origin space. Current and previous rigid transforms use the same
/// origin so [`origin_corrected_prev_view_proj`] can project both coherently.
pub(super) fn rebase_model_matrix(
    model: &[f32; 16],
    render_origin: byroredux_core::math::Vec3,
) -> scene_buffer::GpuPreviousModel {
    [
        [model[0], model[1], model[2], model[3]],
        [model[4], model[5], model[6], model[7]],
        [model[8], model[9], model[10], model[11]],
        [
            model[12] - render_origin.x,
            model[13] - render_origin.y,
            model[14] - render_origin.z,
            model[15],
        ],
    ]
}

/// #1489 / REN2-04 — re-express last frame's camera-relative view-projection
/// (built against render origin `prev_origin` = O₁) in the CURRENT frame's
/// render-origin space (O₂). Geometry uploaded this frame is rebased by O₂,
/// so the previous-frame matrix must satisfy
/// `M·(x_abs − O₂) = prev_vp·(x_abs − O₁)` for every world point — i.e.
/// `M = prev_vp · translation(O₂ − O₁)`. This is exact (a pure translation
/// composition), so motion vectors stay valid across 4096-unit grid
/// crossings; without it the jump frame produced full-screen garbage motion
/// vectors (TAA aliasing flash + SVGF full-frame history drop).
pub(super) fn origin_corrected_prev_view_proj(
    prev_vp: &[f32; 16],
    prev_origin: [f32; 3],
    cur_origin: [f32; 3],
) -> [f32; 16] {
    let delta = byroredux_core::math::Vec3::from_array(cur_origin)
        - byroredux_core::math::Vec3::from_array(prev_origin);
    if delta == byroredux_core::math::Vec3::ZERO {
        // Hot path: the origin only moves on cell-grid crossings.
        return *prev_vp;
    }
    (byroredux_core::math::Mat4::from_cols_array(prev_vp)
        * byroredux_core::math::Mat4::from_translation(delta))
    .to_cols_array()
}

#[cfg(test)]
mod prev_view_proj_origin_tests {
    use super::{origin_corrected_prev_view_proj, rebase_model_matrix};
    use byroredux_core::math::{Mat4, Vec3, Vec4};

    /// Build a plausible camera-relative view-projection for an eye near
    /// the origin (the post-#markarth-precision convention).
    fn sample_vp(eye_rel: Vec3) -> Mat4 {
        let proj = Mat4::perspective_rh(60f32.to_radians(), 16.0 / 9.0, 0.1, 300_000.0);
        proj * Mat4::look_at_rh(eye_rel, eye_rel + Vec3::new(0.3, -0.1, -1.0), Vec3::Y)
    }

    /// Identity case: no grid crossing → the matrix passes through
    /// untouched (bitwise, not just numerically).
    #[test]
    fn unchanged_origin_returns_matrix_verbatim() {
        let vp = sample_vp(Vec3::new(1000.0, 200.0, 3000.0)).to_cols_array();
        let o = [-176_128.0, 0.0, 8192.0];
        assert_eq!(origin_corrected_prev_view_proj(&vp, o, o), vp);
    }

    /// Grid-crossing case (#1489 / REN2-04): for points rebased by the
    /// CURRENT origin O₂, the corrected matrix must reproduce what the
    /// previous-frame matrix produced for the same ABSOLUTE point rebased
    /// by ITS origin O₁ — `M·(x − O₂) = prev_vp·(x − O₁)`. Uses
    /// Markarth-scale coordinates where the pre-fix ΔO error was the
    /// full 4096-unit snap.
    #[test]
    fn corrected_matrix_matches_prev_origin_projection() {
        let o1 = Vec3::new(-176_128.0, 0.0, 8192.0);
        let o2 = Vec3::new(-180_224.0, 4096.0, 8192.0); // crossed in -X and +Y
        let prev_vp = sample_vp(Vec3::new(310.5, 140.0, 2007.25));
        let corrected = Mat4::from_cols_array(&origin_corrected_prev_view_proj(
            &prev_vp.to_cols_array(),
            o1.to_array(),
            o2.to_array(),
        ));
        for x_abs in [
            Vec3::new(-176_500.0, 350.0, 9000.0),
            Vec3::new(-179_800.0, 4200.0, 7500.0),
            Vec3::new(-177_000.0, 0.0, 8192.0),
        ] {
            let want = prev_vp * Vec4::from((x_abs - o1, 1.0));
            let got = corrected * Vec4::from((x_abs - o2, 1.0));
            for i in 0..4 {
                assert!(
                    (want[i] - got[i]).abs() <= 1e-2 * want[i].abs().max(1.0),
                    "clip component {i} diverged: want {want:?}, got {got:?} for {x_abs:?}"
                );
            }
        }
    }

    #[test]
    fn current_and_previous_rigid_models_share_current_render_origin() {
        let origin = Vec3::new(4096.0, -8192.0, 12_288.0);
        let current = Mat4::from_translation(Vec3::new(4106.0, -8172.0, 12_318.0));
        let previous = Mat4::from_translation(Vec3::new(4104.0, -8172.0, 12_318.0));

        let current_rebased =
            Mat4::from_cols_array_2d(&rebase_model_matrix(&current.to_cols_array(), origin));
        let previous_rebased =
            Mat4::from_cols_array_2d(&rebase_model_matrix(&previous.to_cols_array(), origin));
        assert_eq!(
            current_rebased.w_axis.truncate(),
            Vec3::new(10.0, 20.0, 30.0)
        );
        assert_eq!(
            previous_rebased.w_axis.truncate(),
            Vec3::new(8.0, 20.0, 30.0)
        );
    }
}

#[cfg(test)]
mod rigid_motion_contract_tests {
    use super::super::super::upscaling::engine_motion_to_fsr_pixels;
    use ash::vk;
    use byroredux_core::math::{Mat4, Vec3, Vec4};

    fn uv(clip: Vec4) -> [f32; 2] {
        let ndc = clip.truncate() / clip.w;
        [ndc.x * 0.5 + 0.5, ndc.y * 0.5 + 0.5]
    }

    #[test]
    fn stationary_rigid_vertex_has_zero_engine_and_fsr_motion() {
        let point = Vec4::new(0.25, -0.5, 0.0, 1.0);
        let model = Mat4::from_translation(Vec3::new(0.1, 0.2, 0.0));
        let current_uv = uv(model * point);
        let previous_uv = uv(model * point);
        let engine = [
            current_uv[0] - previous_uv[0],
            current_uv[1] - previous_uv[1],
        ];
        assert_eq!(engine, [0.0, 0.0]);
        assert_eq!(
            engine_motion_to_fsr_pixels(
                engine,
                vk::Extent2D {
                    width: 1920,
                    height: 1080,
                },
            ),
            [0.0, 0.0]
        );
    }

    #[test]
    fn moving_rigid_vertex_converts_to_previous_minus_current_pixels() {
        let point = Vec4::new(0.0, 0.0, 0.0, 1.0);
        let previous_uv = uv(Mat4::IDENTITY * point);
        let current_uv = uv(Mat4::from_translation(Vec3::new(0.02, -0.04, 0.0)) * point);
        let engine = [
            current_uv[0] - previous_uv[0],
            current_uv[1] - previous_uv[1],
        ];
        let fsr = engine_motion_to_fsr_pixels(
            engine,
            vk::Extent2D {
                width: 1000,
                height: 500,
            },
        );
        assert!((engine[0] - 0.01).abs() < 1.0e-6);
        assert!((engine[1] + 0.02).abs() < 1.0e-6);
        assert!((fsr[0] + 10.0).abs() < 1.0e-4);
        assert!((fsr[1] - 10.0).abs() < 1.0e-4);
    }
}

#[cfg(test)]
mod dof_view_proj_tests {
    use super::{dof_effective_view_proj, DofView, DOF_MIN_FOCUS_DIST};
    use byroredux_core::math::{Mat4, Vec3};

    fn pinhole() -> [f32; 16] {
        Mat4::perspective_rh(60f32.to_radians(), 16.0 / 9.0, 0.1, 300_000.0).to_cols_array()
    }

    fn dof_view(aperture: f32, focus_dist: f32) -> DofView {
        DofView {
            aperture,
            focus_dist,
            cam_right: [1.0, 0.0, 0.0],
            cam_up: [0.0, 1.0, 0.0],
            cam_forward: [0.0, 0.0, -1.0],
            proj_mat: pinhole(),
            camera_near: 0.1,
            camera_far: 300_000.0,
            camera_fov_y: 60f32.to_radians(),
        }
    }

    /// #1525 — a degenerate `focus_dist` must never yield a NaN/Inf view-proj.
    /// Pre-fix, `aperture > 0` with `focus_dist = 0` collapsed the look-at
    /// eye→center vector onto the perpendicular lens offset (sideways view, or
    /// NaN when the disk sample was also ~0). The guard falls back to pinhole.
    /// Sweeps the frame counter so the disk-center sample (frame 0 → idx 1) is
    /// covered.
    #[test]
    fn zero_focus_dist_falls_back_to_pinhole_and_stays_finite() {
        let pin = pinhole();
        let cam = [1000.0, 200.0, 3000.0];
        for fc in 0..64u32 {
            let (vp, eye) = dof_effective_view_proj(&dof_view(0.5, 0.0), fc, cam, Vec3::ZERO, &pin);
            assert!(
                vp.iter().all(|x| x.is_finite()),
                "frame {fc}: non-finite vp {vp:?}"
            );
            assert!(
                eye.iter().all(|x| x.is_finite()),
                "frame {fc}: non-finite eye {eye:?}"
            );
            assert_eq!(
                vp, pin,
                "frame {fc}: degenerate focus_dist must use the pinhole matrix"
            );
            assert_eq!(
                eye, cam,
                "frame {fc}: degenerate focus_dist must keep the un-jittered eye"
            );
        }
    }

    /// `aperture <= 0` is a pinhole camera — inputs pass straight through.
    #[test]
    fn zero_aperture_is_pinhole() {
        let pin = pinhole();
        let cam = [10.0, 20.0, 30.0];
        let (vp, eye) = dof_effective_view_proj(&dof_view(0.0, 20.0), 7, cam, Vec3::ZERO, &pin);
        assert_eq!(vp, pin);
        assert_eq!(eye, cam);
    }

    /// A valid aperture + focal distance jitters the eye on the aperture disk
    /// (perpendicular to forward) and produces a finite, non-pinhole matrix.
    #[test]
    fn valid_dof_jitters_and_stays_finite() {
        let pin = pinhole();
        let cam = [0.0, 0.0, 0.0];
        // frame 3 → idx 4 → a non-center disk sample, so the eye actually moves.
        let (vp, eye) = dof_effective_view_proj(&dof_view(0.5, 20.0), 3, cam, Vec3::ZERO, &pin);
        assert!(vp.iter().all(|x| x.is_finite()));
        assert!(eye.iter().all(|x| x.is_finite()));
        assert_ne!(vp, pin, "valid DOF must not equal the pinhole matrix");
        assert!(
            eye[2].abs() < 1e-6,
            "jitter stays in the right/up plane (z unchanged)"
        );
        assert!(
            eye[0] != 0.0 || eye[1] != 0.0,
            "eye should move on the aperture disk"
        );
    }

    /// The guard threshold is a real positive floor, so exact-zero and
    /// tiny-positive focus distances both fall back to pinhole.
    #[test]
    fn guard_threshold_is_positive_floor() {
        const {
            assert!(DOF_MIN_FOCUS_DIST > 0.0);
        }
        let pin = pinhole();
        let cam = [0.0, 0.0, 0.0];
        let (vp, _) = dof_effective_view_proj(
            &dof_view(0.5, DOF_MIN_FOCUS_DIST * 0.5),
            3,
            cam,
            Vec3::ZERO,
            &pin,
        );
        assert_eq!(
            vp, pin,
            "focus_dist below the floor must fall back to pinhole"
        );
    }
}

#[cfg(test)]
mod is_caustic_source_tests {
    use super::*;

    /// Minimal `DrawCommand` builder for the caustic-gate unit tests.
    /// Fields irrelevant to `is_caustic_source` get zero/default values
    /// — the gate only consults `material_kind` and
    /// `multi_layer_refraction_scale`.
    fn cmd(material_kind: u32, multi_layer_refraction_scale: f32) -> DrawCommand {
        DrawCommand {
            mesh_handle: 0,
            texture_handle: 0,
            model_matrix: [0.0; 16],
            alpha_blend: true,
            src_blend: 6,
            dst_blend: 7,
            two_sided: false,
            no_sorter: false,
            wireframe: false,
            flat_shading: false,
            is_decal: false,
            render_layer: byroredux_core::ecs::components::RenderLayer::Architecture,
            bone_offset: 0,
            normal_map_index: 0,
            dark_map_index: 0,
            glow_map_index: 0,
            detail_map_index: 0,
            gloss_map_index: 0,
            parallax_map_index: 0,
            parallax_height_scale: 0.0,
            parallax_max_passes: 0.0,
            env_map_index: 0,
            env_mask_index: 0,
            alpha_threshold: 0.0,
            alpha_test_func: 0,
            roughness: 0.5,
            metalness: 0.0,
            ior: 1.5, // #1248
            glass_fresnel_color: [1.0; 3],
            glass_refraction_scale:
                byroredux_core::ecs::components::material::DEFAULT_GLASS_REFRACTION_SCALE,
            glass_blur_scale: byroredux_core::ecs::components::material::DEFAULT_GLASS_BLUR_SCALE,
            glass_blur_scale_factor: 1.0,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            fresnel_power: 5.0,
            grayscale_to_palette_scale: 1.0,
            subsurface: 0.0, // #1249
            sheen: 0.0,
            sheen_tint: 0.0,
            anisotropic: 0.0, // #1250
            emissive_mult: 0.0,
            emissive_color: [0.0; 3],
            specular_strength: 0.0,
            specular_color: [0.0; 3],
            diffuse_color: [1.0; 3],
            ambient_color: [1.0; 3],
            vertex_offset: 0,
            index_offset: 0,
            vertex_count: 0,
            sort_depth: 0,
            in_tlas: true,
            in_raster: true,
            avg_albedo: [0.0; 3],
            material_kind,
            z_test: true,
            z_write: true,
            z_function: 3,
            terrain_tile_index: None,
            entity_id: 0,
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
            material_alpha: 1.0,
            skin_tint_rgba: [0.0; 4],
            hair_tint_rgb: [0.0; 3],
            multi_layer_envmap_strength: 0.0,
            eye_left_center: [0.0; 3],
            eye_cubemap_scale: 0.0,
            eye_right_center: [0.0; 3],
            multi_layer_inner_thickness: 0.0,
            multi_layer_refraction_scale,
            multi_layer_inner_scale: [0.0; 2],
            sparkle_rgba: [0.0; 4],
            effect_falloff: [0.0; 5],
            material_id: 0,
            vertex_color_emissive: false,
            effect_shader_flags: 0,
            greyscale_lut_index: 0,
            supplemental_texture_indices: [0; 16],
            translucency_subsurface_color: [0.0; 3],
            translucency_transmissive_scale: 0.0,
            translucency_turbulence: 0.0,
            shader_color: [0.0; 3],
            shader_float: 0.0,
            is_water: false,
        }
    }

    #[test]
    fn glass_material_is_caustic_source() {
        // MATERIAL_KIND_GLASS = 100: engine-classified refractive surface.
        assert!(is_caustic_source(&cmd(MATERIAL_KIND_GLASS, 0.0)));
    }

    #[test]
    fn multi_layer_parallax_with_refraction_is_caustic_source() {
        // Skyrim+ BSLightingShaderProperty MultiLayerParallax variant
        // with non-zero refraction scale — real two-layer refraction.
        assert!(is_caustic_source(&cmd(
            MATERIAL_KIND_MULTI_LAYER_PARALLAX,
            0.3
        )));
    }

    #[test]
    fn opaque_refractive_materials_are_not_caustic_sources() {
        // Opaque mesh-ID pixels carry a stable surface ID rather than the
        // live instance index caustic_splat.comp requires. Neither material
        // classification alone may opt such a draw into the compute pass.
        for mut draw in [
            cmd(MATERIAL_KIND_GLASS, 0.0),
            cmd(MATERIAL_KIND_MULTI_LAYER_PARALLAX, 0.3),
        ] {
            draw.alpha_blend = false;
            assert!(!is_caustic_source(&draw));
        }
    }

    #[test]
    fn multi_layer_parallax_without_refraction_is_not_caustic() {
        // Kind 11 with zero refraction scale = parallax but no refraction.
        assert!(!is_caustic_source(&cmd(
            MATERIAL_KIND_MULTI_LAYER_PARALLAX,
            0.0
        )));
    }

    #[test]
    fn default_lit_alpha_blend_is_not_caustic_source() {
        // material_kind=0 covers foliage alpha-test cutouts and particle
        // billboards. Pre-#922 the old `alpha_blend && metalness < 0.3`
        // gate fired here and burned `max_lights` TLAS ray queries per
        // foliage pixel on exterior cells.
        assert!(!is_caustic_source(&cmd(0, 0.0)));
    }

    #[test]
    fn hair_tint_is_not_caustic_source() {
        // material_kind=6 = HairTint (Skyrim+). Pre-#922 false positive.
        assert!(!is_caustic_source(&cmd(6, 0.0)));
    }

    #[test]
    fn effect_shader_is_not_caustic_source() {
        // MATERIAL_KIND_EFFECT_SHADER (101): BSEffectShaderProperty FX
        // cards — fire planes, magic auras, decals. Emissive add, no
        // refraction. Pre-#922 false positive on every alpha-blend FX.
        assert!(!is_caustic_source(&cmd(
            scene_buffer::MATERIAL_KIND_EFFECT_SHADER,
            0.0
        )));
    }

    #[test]
    fn skin_tint_is_not_caustic_source() {
        // material_kind=5 = SkinTint. Bethesda character skin meshes.
        // Pre-#922 false positive on the alpha-blend body slot.
        assert!(!is_caustic_source(&cmd(5, 0.0)));
    }
}

/// Regression for #1211 / REN-SAFETY. `draw_frame` must early-return
/// when `self.framebuffers` is empty (the state left behind when
/// `recreate_swapchain` fails partway). Without the guard the first
/// indexing access at the `RenderPassBeginInfo::framebuffer(...)` site
/// panics with `index out of bounds`, taking the process down on
/// surface-lost events that are normal Vulkan (window minimize,
/// monitor disconnect, compositor restart, NVIDIA driver mismatch
/// falling back to RADV).
///
/// Live unit test against a mocked `VulkanContext` is impractical —
/// 70+ Vulkan-loader fields with no safe defaults. Static source
/// assertion mirrors the precedent set by
/// `resize.rs::old_image_views_destroyed_between_new_swapchain_creation_and_old_destroy`
/// (#654 ordering check).
#[cfg(test)]
mod framebuffers_empty_guard_tests {
    #[test]
    fn draw_frame_guards_on_empty_framebuffers_before_acquire() {
        let src = include_str!("draw.rs");

        // The guard text — must be present somewhere in the file.
        let guard_pos = src
            .find("if self.framebuffers.is_empty() {")
            .expect("draw_frame must guard on empty framebuffers (#1211)");

        // The fence-wait + acquire happen inside `draw_frame` and
        // must come AFTER the guard. We anchor on `wait_for_fences`
        // (the first fallible Vulkan call in `draw_frame`) and
        // `acquire_next_image` (the call that signals
        // `image_available[frame]` — the semaphore that would leak
        // if we early-return after acquire). Both must appear after
        // the guard.
        let wait_pos = src
            .find(".wait_for_fences(")
            .expect("draw_frame should call wait_for_fences");
        let acquire_pos = src
            .find(".acquire_next_image(")
            .expect("draw_frame should call acquire_next_image");

        assert!(
            guard_pos < wait_pos,
            "framebuffers.is_empty() guard must come BEFORE \
             wait_for_fences — no point waiting for a frame we're \
             about to skip. (#1211)"
        );
        assert!(
            guard_pos < acquire_pos,
            "framebuffers.is_empty() guard must come BEFORE \
             acquire_next_image — otherwise the image_available \
             semaphore is left signal-pending without a paired wait, \
             tripping VUID-vkAcquireNextImageKHR-semaphore-01779 on \
             the next acquire. (#1211)"
        );
    }
}

/// Regression for #1796 / D6-02. `skin_dispatch_ran` must be reset
/// `false` before both of `draw_frame`'s early-return guards (empty
/// framebuffers, `ERROR_OUT_OF_DATE_KHR`) and only flipped `true` once
/// `record_skinned_blas_refit` — the function that actually reads
/// `pose_dirty` and gates the skin compute dispatch — runs. A live
/// mocked `VulkanContext` test is impractical for the same reason as
/// `framebuffers_empty_guard_tests` above (70+ Vulkan-loader fields, no
/// safe defaults); a static source assertion pins the ordering instead.
#[cfg(test)]
mod skin_dispatch_ran_ordering_tests {
    #[test]
    fn skin_dispatch_ran_is_reset_before_both_early_return_guards() {
        let src = include_str!("draw.rs");

        let reset_pos = src
            .find("self.skin_dispatch_ran = false;")
            .expect("draw_frame must reset skin_dispatch_ran to false (#1796)");
        let fb_guard_pos = src
            .find("if self.framebuffers.is_empty() {")
            .expect("draw_frame must guard on empty framebuffers (#1211)");
        let oode_guard_pos = src
            .find("Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),")
            .expect("draw_frame must guard on ERROR_OUT_OF_DATE_KHR");
        // `record_skinned_blas_refit` (which sets the flag true) is
        // defined textually EARLIER in the file than `draw_frame` — so
        // the assertion anchors on draw_frame's *call site* for that
        // function, mirroring how the sibling test above anchors on the
        // `wait_for_fences` / `acquire_next_image` call sites rather
        // than callee bodies.
        let call_site_pos = src
            .find("self.record_skinned_blas_refit(")
            .expect("draw_frame must call record_skinned_blas_refit (#1796)");

        assert!(
            reset_pos < fb_guard_pos,
            "skin_dispatch_ran reset must come BEFORE the empty-framebuffers \
             guard, or that early return would leave the flag from the \
             previous frame's outcome instead of reporting its own. (#1796)"
        );
        assert!(
            reset_pos < oode_guard_pos,
            "skin_dispatch_ran reset must come BEFORE the \
             ERROR_OUT_OF_DATE_KHR guard, for the same reason. (#1796)"
        );
        assert!(
            fb_guard_pos < call_site_pos && oode_guard_pos < call_site_pos,
            "record_skinned_blas_refit (which sets skin_dispatch_ran true) \
             must be called AFTER both early-return guards — calling it any \
             earlier would defeat the rollback signal entirely. (#1796)"
        );
    }
}

/// Regression for #3569 / D9-01. `bind_inverse_upload_failed` must be
/// reset `false` in lockstep with `skin_dispatch_ran` — both guard the
/// same rollback check in `app_frame.rs`, and a stale `true` surviving
/// from a previous frame's failure would force an unnecessary requeue
/// every frame after.
#[cfg(test)]
mod bind_inverse_upload_failed_reset_tests {
    #[test]
    fn bind_inverse_upload_failed_is_reset_alongside_skin_dispatch_ran() {
        let src = include_str!("draw.rs");

        let skin_reset_pos = src
            .find("self.skin_dispatch_ran = false;")
            .expect("draw_frame must reset skin_dispatch_ran to false (#1796)");
        let upload_failed_reset_pos = src
            .find("self.bind_inverse_upload_failed = false;")
            .expect(
                "draw_frame must reset bind_inverse_upload_failed to false (#3569)",
            );
        let fb_guard_pos = src
            .find("if self.framebuffers.is_empty() {")
            .expect("draw_frame must guard on empty framebuffers (#1211)");

        assert!(
            skin_reset_pos < upload_failed_reset_pos,
            "bind_inverse_upload_failed reset must come right after the \
             skin_dispatch_ran reset it mirrors. (#3569)"
        );
        assert!(
            upload_failed_reset_pos < fb_guard_pos,
            "bind_inverse_upload_failed reset must come BEFORE the \
             empty-framebuffers guard — same reasoning as \
             skin_dispatch_ran: an early return must not leak the \
             previous frame's failure into this frame's check. (#3569)"
        );
    }
}

/// Regression for D6-04 / #1811. `next_clean_skin_frames` /
/// `should_skip_skin_gpu_refresh` gate the bone_world upload + device
/// copy + `skin_palette.comp` dispatch so they don't re-run every frame
/// once a scene's skinned poses have gone quiet. Both are pure, so
/// (unlike the rest of `draw_frame`) they're directly unit-testable.
#[cfg(test)]
mod clean_skin_frames_tests {
    use super::{next_clean_skin_frames, should_skip_skin_gpu_refresh};

    #[test]
    fn dirty_frame_resets_the_streak() {
        assert_eq!(next_clean_skin_frames(9, true), 0);
    }

    #[test]
    fn clean_frame_grows_the_streak() {
        assert_eq!(next_clean_skin_frames(0, false), 1);
        assert_eq!(next_clean_skin_frames(1, false), 2);
    }

    #[test]
    fn streak_saturates_instead_of_overflowing() {
        assert_eq!(next_clean_skin_frames(u32::MAX, false), u32::MAX);
    }

    #[test]
    fn refresh_is_not_skipped_within_max_frames_in_flight_of_a_dirty_frame() {
        // MAX_FRAMES_IN_FLIGHT == 2 (crates/renderer/src/vulkan/sync.rs).
        // A dirty frame itself (streak 0) and the next
        // MAX_FRAMES_IN_FLIGHT frames after it (streak 1, 2) must all
        // still refresh — every live frame-in-flight bone_world buffer
        // needs to see the fresh value at least once before it's safe
        // to stop.
        for streak in 0..=super::MAX_FRAMES_IN_FLIGHT as u32 {
            assert!(
                !should_skip_skin_gpu_refresh(streak),
                "streak {streak} must still refresh — not every frame-in-flight \
                 buffer has seen the current value yet"
            );
        }
    }

    #[test]
    fn refresh_is_skipped_once_every_buffer_has_seen_the_current_value() {
        let threshold = super::MAX_FRAMES_IN_FLIGHT as u32 + 1;
        assert!(
            should_skip_skin_gpu_refresh(threshold),
            "streak {threshold} must skip — every frame-in-flight buffer has \
             already been refreshed with the unchanged current value"
        );
        assert!(should_skip_skin_gpu_refresh(threshold + 5));
    }
}

#[cfg(test)]
mod group_state_tests {
    //! #1581 / F1 — the indirect-merge key must not let a group leader's
    //! cull (`two_sided`) or depth (`z_test`/`z_write`/`z_function`) state
    //! bleed across a state boundary onto the rest of a merged group.
    use super::*;
    use byroredux_core::ecs::components::RenderLayer;

    /// A baseline single-sided, depth-tested-and-written opaque batch.
    fn batch() -> DrawBatch {
        DrawBatch {
            mesh_handle: 1,
            pipeline_key: PipelineKey::Opaque { wireframe: false },
            two_sided: false,
            render_layer: RenderLayer::Clutter,
            first_instance: 0,
            instance_count: 1,
            index_count: 3,
            global_index_offset: 0,
            global_vertex_offset: 0,
            z_test: true,
            z_write: true,
            z_function: 3,
            order_dependent_glass: false,
        }
    }

    /// Two batches identical in state (only mesh differs) DO share a key —
    /// the homogeneous run still merges into one indirect call.
    #[test]
    fn same_state_different_mesh_merges() {
        let a = batch();
        let mut b = batch();
        b.mesh_handle = 99;
        b.first_instance = 1;
        assert_eq!(group_state(&a), group_state(&b));
    }

    /// A two_sided boundary must split the group: a CULL_NONE batch can't
    /// inherit a single-sided leader's CULL_BACK (lost back faces on fences
    /// / grates / foliage cards).
    #[test]
    fn two_sided_boundary_splits() {
        let single = batch();
        let mut two = batch();
        two.two_sided = true;
        assert_ne!(
            group_state(&single),
            group_state(&two),
            "two_sided must break the merge key",
        );
    }

    /// Each depth-state axis must split the group on its own — a `z_write=0`
    /// halo can't inherit a `z_write=1` leader's depth write, etc.
    #[test]
    fn depth_state_boundaries_split() {
        let base = batch();
        for mutate in [
            (|b: &mut DrawBatch| b.z_test = false) as fn(&mut DrawBatch),
            |b: &mut DrawBatch| b.z_write = false,
            |b: &mut DrawBatch| b.z_function = 7,
        ] {
            let mut other = batch();
            mutate(&mut other);
            assert_ne!(
                group_state(&base),
                group_state(&other),
                "a depth-state change must break the merge key",
            );
        }
    }

    /// Pipeline + render-layer (the original key axes) still split.
    #[test]
    fn pipeline_and_layer_still_split() {
        let base = batch();
        let mut blended = batch();
        blended.pipeline_key = PipelineKey::Blended {
            src: 10,
            dst: 6,
            wireframe: false,
            preserve_opaque_gbuffer: false,
        };
        assert_ne!(group_state(&base), group_state(&blended));

        let mut decal = batch();
        decal.render_layer = RenderLayer::Decal;
        assert_ne!(group_state(&base), group_state(&decal));
    }
}

#[cfg(test)]
mod needs_two_sided_blend_split_tests {
    //! #1804 / D2-NEW-03 / #2165 — the two-pass back-then-front cull split
    //! applies to two-sided *refractive glass* batches only.
    //!
    //! Both earlier predicates were wrong, in opposite directions, and both
    //! shipped green because the tests were written to match whatever the
    //! code did at the time. #1804 keyed on `z_write`, which excludes the
    //! FO4 BGEM glass (`z_write: false`) that motivated the split;
    //! `883f57cd` then dropped the limb entirely and re-included every
    //! two-sided blended particle batch — the exact population #1804 set
    //! out to exclude — while a same-named test asserted that as correct.
    //! The cases below are signed against the *material*, which is what the
    //! split has always actually been about.
    use super::*;
    use byroredux_core::ecs::components::RenderLayer;

    /// A two-sided blended batch. `order_dependent_glass` is the axis under
    /// test; `z_write` is varied only to prove it is NOT an input.
    fn blended_two_sided_batch(z_write: bool, order_dependent_glass: bool) -> DrawBatch {
        DrawBatch {
            mesh_handle: 1,
            pipeline_key: PipelineKey::Blended {
                src: 6,
                dst: 0,
                wireframe: false,
                preserve_opaque_gbuffer: order_dependent_glass,
            },
            two_sided: true,
            render_layer: RenderLayer::Clutter,
            first_instance: 0,
            instance_count: 1,
            index_count: 3,
            global_index_offset: 0,
            global_vertex_offset: 0,
            z_test: true,
            z_write,
            z_function: 3,
            order_dependent_glass,
        }
    }

    /// Depth-writing two-sided glass splits.
    #[test]
    fn splits_when_blended_two_sided_glass_and_z_write() {
        assert!(needs_two_sided_blend_split(&blended_two_sided_batch(
            true, true
        )));
    }

    /// Non-depth-writing two-sided glass must also split: this is the
    /// normal authored state for FO4 BGEM glass, and the case #1804's
    /// `z_write` proxy wrongly excluded.
    #[test]
    fn splits_when_glass_and_z_write_false() {
        assert!(needs_two_sided_blend_split(&blended_two_sided_batch(
            false, true
        )));
    }

    /// #2165 regression guard — the particle population. Two-sided,
    /// alpha-blended, `z_write: false`, non-glass: matches every limb the
    /// post-`883f57cd` predicate tested, and must NOT split. Billboards are
    /// front-facing by construction, so the FRONT-cull pass produces no
    /// camera-facing fragments; splitting buys nothing and costs a wasted
    /// vertex walk plus the batch's place in an indirect group.
    ///
    /// This is the assertion whose sign was inverted before #2165 (as
    /// `splits_when_z_write_false`) — which is why `cargo test` stayed
    /// green straight through the regression.
    #[test]
    fn does_not_split_two_sided_blended_particles() {
        assert!(!needs_two_sided_blend_split(&blended_two_sided_batch(
            false, false
        )));
    }

    /// The same population with `z_write: true` also stays unsplit — depth
    /// state is not an input to the predicate in either direction.
    #[test]
    fn does_not_split_non_glass_regardless_of_z_write() {
        assert!(!needs_two_sided_blend_split(&blended_two_sided_batch(
            true, false
        )));
    }

    /// Single-sided glass never splits — there are no back faces to order.
    #[test]
    fn does_not_split_when_not_two_sided() {
        let mut b = blended_two_sided_batch(true, true);
        b.two_sided = false;
        assert!(!needs_two_sided_blend_split(&b));
    }

    /// Opaque batches never split, even if (nonsensically) two-sided glass.
    #[test]
    fn does_not_split_when_opaque() {
        let mut b = blended_two_sided_batch(true, true);
        b.pipeline_key = PipelineKey::Opaque { wireframe: false };
        assert!(!needs_two_sided_blend_split(&b));
    }

    /// #2165 — the indirect gather loop admits a batch on `group_state`
    /// equality alone. If split-eligibility weren't in that key, a particle
    /// leader would absorb a following glass batch into its indirect group
    /// and rasterize it in a single CULL_NONE draw, silently losing the
    /// back-then-front ordering the split exists for.
    #[test]
    fn glass_and_particles_never_share_an_indirect_group() {
        let particles = blended_two_sided_batch(false, false);
        let glass = blended_two_sided_batch(false, true);
        assert_ne!(
            group_state(&particles),
            group_state(&glass),
            "identical pipeline + depth state, differing only in glass-ness — \
             the merge key must still split them"
        );
    }
}

#[cfg(test)]
mod should_use_indirect_draws_tests {
    //! #2504 / D12-2026-08-07-02 — a failed indirect-buffer upload must
    //! force the direct-draw fallback for that frame, not leave
    //! `cmd_draw_indexed_indirect` reading a stale/uninitialized buffer.
    use super::*;

    /// All prerequisites hold — the happy path that actually reaches
    /// `cmd_draw_indexed_indirect`.
    #[test]
    fn true_when_bound_supported_and_upload_succeeded() {
        assert!(should_use_indirect_draws(true, true, true, 1));
    }

    /// The regression case: everything else says "go indirect" but this
    /// frame's upload failed. Must fall back to direct draws.
    #[test]
    fn false_when_upload_failed_even_if_otherwise_eligible() {
        assert!(!should_use_indirect_draws(true, true, false, 1));
    }

    #[test]
    fn false_when_global_buffer_not_bound() {
        assert!(!should_use_indirect_draws(false, true, true, 1));
    }

    #[test]
    fn false_when_device_lacks_multi_draw_indirect() {
        assert!(!should_use_indirect_draws(true, false, true, 1));
    }

    /// #2751 / REN-D12-2026-08-12-01 — the batch count is the limb this
    /// predicate was missing. `indirect_buffers[frame]` holds exactly
    /// `MAX_INDIRECT_DRAWS` commands, and the draw loop derives its
    /// `byte_offset` from the *unclamped* batch index, so one batch past the
    /// ceiling is already a read past the allocation
    /// (VUID-vkCmdDrawIndexedIndirect-offset-00556) — not a misrender but a
    /// device-lost-class fetch of `indexCount`/`vertexOffset` from
    /// unallocated memory.
    #[test]
    fn false_when_batch_count_exceeds_the_indirect_buffer() {
        assert!(
            !should_use_indirect_draws(true, true, true, MAX_INDIRECT_DRAWS + 1),
            "one batch past the ceiling must reject the indirect path"
        );
        assert!(
            !should_use_indirect_draws(true, true, true, MAX_INDIRECT_DRAWS * 4),
            "a wildly overflowing frame must reject it too"
        );
    }

    /// Exactly `MAX_INDIRECT_DRAWS` batches is the last legal count, not the
    /// first illegal one: the loop's final `byte_offset + stride` lands
    /// exactly on the buffer end. An off-by-one here would silently drop the
    /// indirect path for a frame that fits.
    #[test]
    fn true_at_exactly_the_indirect_buffer_capacity() {
        assert!(should_use_indirect_draws(
            true,
            true,
            true,
            MAX_INDIRECT_DRAWS
        ));
    }

    /// An empty frame stays eligible — the draw loop simply records nothing.
    /// Rejecting here would be harmless but would misattribute the fallback
    /// in any future telemetry on this predicate.
    #[test]
    fn true_for_an_empty_batch_list() {
        assert!(should_use_indirect_draws(true, true, true, 0));
    }
}
