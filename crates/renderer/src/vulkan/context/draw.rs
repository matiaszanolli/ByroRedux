//! Frame recording and submission — the per-frame hot path.

use super::super::descriptors::memory_barrier;
use super::super::frame_upscaler::FsrFrameParameters;
use super::super::material::GpuMaterial;
use super::super::pipeline::PipelineKey;
use super::super::presentation::ImageSpaceModifierView;
use super::super::scene_buffer::{
    self, GpuInstance, GpuTerrainTile, INSTANCE_FLAG_ALPHA_BLEND, INSTANCE_FLAG_CAUSTIC_SOURCE,
    INSTANCE_FLAG_DIFFUSE_ALPHA, INSTANCE_FLAG_FLAT_SHADING, INSTANCE_FLAG_NON_UNIFORM_SCALE,
    INSTANCE_FLAG_TERRAIN_SPLAT, INSTANCE_RENDER_LAYER_MASK, INSTANCE_RENDER_LAYER_SHIFT,
    INSTANCE_TERRAIN_TILE_MASK, INSTANCE_TERRAIN_TILE_SHIFT, MATERIAL_KIND_GLASS,
};
use super::super::sync::MAX_FRAMES_IN_FLIGHT;
use super::super::upscaling::fsr_camera_parameters;
use super::super::water::WaterDrawCommand;
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
fn is_camera_cut(frame_counter: u32, camera_delta: f32, cam_forward_dot: f32) -> bool {
    frame_counter > 0 && (camera_delta > 256.0 || cam_forward_dot < 0.0)
}

/// The three frame-over-frame camera signals `draw_frame` derives before
/// asking [`is_camera_cut`] for a verdict (#2197, extracted from
/// `draw_frame`).
struct CameraFrameDeltas {
    /// Absolute world-space distance the camera moved since last frame.
    camera_delta: f32,
    /// Dot product of this frame's and last frame's unit forward vectors.
    cam_forward_dot: f32,
    /// Largest element-wise `|Δ|` between this and last frame's view-proj.
    /// Diagnostic only — deliberately NOT a cut signal, see below.
    vp_max_abs_delta: f32,
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
fn camera_frame_deltas(
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
fn uses_rigid_motion_history(bone_offset: u32, alpha_blend: bool) -> bool {
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
fn skinned_vertex_address_for_draw(
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
fn skin_slot_backs_mesh(slot_vertex_count: u32, mesh_vertex_count: u32) -> bool {
    slot_vertex_count == mesh_vertex_count
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
/// Period 16 (#1093 / REN-D11-002): Halton(2) natural period is 2, Halton(3)
/// natural period is 3, LCM = 6. Using 16 (nearest power-of-2 ≥ 6) avoids the
/// asymmetric Y-coverage gap that `% 8` caused.
///
/// Returns `(0.0, 0.0)` (no jitter) whenever `taa_present` is false OR
/// `taa_failed` is true (#1932 / TAA-D13-01) — a permanent TAA failure must
/// fall back to a stable pinhole image, not a jittered-but-unresolved one.
fn taa_jitter(
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
        // Map [0,1] → [-0.5, 0.5] pixels, then to NDC.
        ((hx - 0.5) * 2.0 / width, (hy - 0.5) * 2.0 / height)
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
fn dof_effective_view_proj(
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
fn fsr_gated_dof(dof: DofView, fsr_active: bool) -> DofView {
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
fn build_fsr_frame_parameters(
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
struct CompositeParamsInputs<'a> {
    fog_color: [f32; 3],
    fog_near: f32,
    fog_far: f32,
    fog_extinction_per_meter: f32,
    fog_single_scatter_albedo: f32,
    fog_clip: f32,
    fog_power: f32,
    fog_height_reference: f32,
    sky_params: &'a SkyParams,
    exposure_value: f32,
    frame_counter: u32,
    volume_far_distance: f32,
    /// Froxel grid depth (slice count) — #2470, `volumetrics::extent().depth`.
    /// Threaded into `sky_horizon.w` so the composite shader can remap
    /// `hybridSliceCoordinate`'s normalized depth onto the `sampler3D`
    /// texel-center grid before the `volumetricFroxel` tap.
    froxel_slice_count: f32,
    camera_pos: [f32; 3],
    render_origin: byroredux_core::math::Vec3,
    inv_vp_arr: [[f32; 4]; 4],
    underwater: [f32; 4],
    /// Whether `water_caustic_accum` is genuinely live this session — see
    /// `CompositeParams::caustic_flags` (#2508).
    water_caustic_active: bool,
}

/// Assemble this frame's `CompositeParams` (#2255 / TD1-NEW-02, extracted
/// from `draw_frame` alongside `build_fsr_frame_parameters` — same
/// rationale: pure data assembly with no borrow-checker reason to stay
/// inline). The caller still owns the `composite.upload_params` call and
/// its error handling; only the field-by-field construction moved here.
fn build_composite_params(
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
        exposure_value,
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
            // Exposure — vestigial in the composite UBO. `composite.frag`
            // does not read `depth_params.y`; the live exposure consumer
            // is `presentation.frag`'s `exposure` push constant, sourced
            // from this same shared exposure producer separately in
            // `record_presentation_pass`. Kept here for UBO layout
            // parity (default Bethesda-era HDR target; promote to WTHR
            // field #743). Falls back to the const when the 1x1
            // resource failed to allocate.
            exposure_value,
            // #1013 — host-side mirror of the volumetric-output gate.
            // Vestigial: #1926 removed the `composite.frag` branch that
            // used to read this slot, so `vol.a`/`vol.rgb` consumption
            // there is now unconditional. The value is still pinned to
            // the host const so a future flip of
            // `VOLUMETRIC_OUTPUT_CONSUMED` stays a single-line change on
            // the host side — but the shader-side off-path no longer
            // exists; it now relies entirely on the neutral clear
            // (`volumetrics.rs::initialize_layouts`, rgb=0/a=1) leaving
            // `vol.a` == 1 and `vol.rgb` == 0 when the volumetric
            // dispatches are skipped.
            if super::super::volumetrics::VOLUMETRIC_OUTPUT_CONSUMED {
                1.0
            } else {
                0.0
            },
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
            exposure_value: 1.5,
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
            params.depth_params[1], 1.5,
            "exposure_value must map through"
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
            exposure_value: 0.0,
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
/// word; the compute pass burns `max_lights` TLAS ray queries per flagged
/// pixel, so the gate has to stay tight.
///
/// Accepted refractive signals:
///   * `material_kind == MATERIAL_KIND_GLASS` — engine-classified glass
///     from `render::build_render_data` (alpha-blend + low metal + low
///     roughness + not a decal). See #515 / #706.
///   * Skyrim+ `MultiLayerParallax` (kind 11) with a non-zero inner-layer
///     refraction scale — real two-layer refractive surface.
///
/// Rejected (pre-#922 false positives the old `alpha_blend &&
/// metalness < 0.3` gate caught): hair (HairTint, kind 6), foliage (kind 0
/// alpha-test cutouts), particle billboards (kind 0, emissive), decals
/// (`is_decal` excluded by the glass classifier), `BSEffectShaderProperty`
/// FX cards (kind 101 — MATERIAL_KIND_EFFECT_SHADER).
fn is_caustic_source(cmd: &DrawCommand) -> bool {
    is_refractive_glass(cmd)
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
fn is_refractive_glass(cmd: &DrawCommand) -> bool {
    if cmd.material_kind == MATERIAL_KIND_GLASS {
        return true;
    }
    const MATERIAL_KIND_MULTI_LAYER_PARALLAX: u32 = 11;
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
fn next_clean_skin_frames(current: u32, skin_state_dirty: bool) -> u32 {
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
fn should_skip_skin_gpu_refresh(clean_skin_frames: u32) -> bool {
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
pub(super) fn should_use_indirect_draws(
    global_bound: bool,
    multi_draw_indirect_supported: bool,
    indirect_upload_ok: bool,
) -> bool {
    global_bound && multi_draw_indirect_supported && indirect_upload_ok
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
    /// M29.6 — first-sight `bind_inverses` uploads to schedule this frame.
    /// Each entry is `(slot_id, per-mesh bind_inverses)`; the renderer writes
    /// them into the persistent SSBO at the slot's offset before dispatching
    /// `skin_palette.comp`. Empty on frames with no fresh skinned-mesh
    /// first-sight (the steady-state case).
    pub bind_inverse_pending_uploads: &'a [(u32, Vec<[[f32; 4]; 4]>)],
    /// Per-frame materials.
    pub materials: &'a [GpuMaterial],
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
    pub pose_dirty: &'a std::collections::HashSet<EntityId>,
}

impl VulkanContext {
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
    fn is_fsr_dispatch_active(&self) -> bool {
        self.frame_upscaler
            .as_ref()
            .is_some_and(|upscaler| upscaler.is_fsr_dispatch_active())
    }

    pub fn draw_frame(&mut self, inputs: FrameInputs) -> Result<bool> {
        let FrameInputs {
            clear_color,
            view_proj,
            draw_commands,
            lights,
            fog_volumes,
            bone_world,
            bind_inverse_pending_uploads,
            materials,
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

        let frame = self.current_frame;
        let volumetric_time_seconds = self.volumetric_time_seconds;
        // Use a local to avoid borrow complexity; copy out at end.
        let mut t = FrameTimings::default();
        // #1197 / PERF-DIM7-03 — reset per-frame descriptor-writes
        // counters on both skin compute pipelines. The dispatch
        // bodies bump these only when they actually call
        // `vkUpdateDescriptorSets`; steady state stays at 0.
        if let Some(ref p) = self.skin_compute {
            p.reset_descriptor_writes_counter();
        }
        if let Some(ref p) = self.skin_palette {
            p.reset_descriptor_writes_counter();
        }

        // Wait for this frame-in-flight slot AND the previous slot to be
        // available. SVGF's temporal pass reads the previous slot's G-buffer
        // images (mesh_id, motion, raw_indirect) — without waiting on the
        // other slot's fence, a read-after-write hazard exists when the GPU
        // hasn't finished the other slot's render pass. See #282.
        //
        // Cost: zero in practice — the GPU is rarely more than 1 frame
        // behind the CPU, so the other fence is almost always signaled.
        let fence_t0 = Instant::now();
        // SAFETY: `in_flight[frame]` and `in_flight[prev]` are live fences; both were signal-targets of prior `queue_submit`s (or created pre-signaled), so the wait cannot deadlock. This frame's `cmd` is not re-recorded until this wait returns, so the GPU is done with the prior recording.
        unsafe {
            let prev = (frame + 1) % super::super::sync::MAX_FRAMES_IN_FLIGHT;
            self.device
                .wait_for_fences(
                    &[
                        self.frame_sync.in_flight[frame],
                        self.frame_sync.in_flight[prev],
                    ],
                    true,
                    u64::MAX,
                )
                .context("wait_for_fences")?;
        }
        t.fence_wait_ns = fence_t0.elapsed().as_nanos() as u64;

        // EX-05 / #2736 — harvest this slot's image-health counters from the
        // *prior* use of the slot, then zero them for the frame about to be
        // recorded. The fence wait above proves submission completed
        // (device-side access scope only) — it does NOT by itself prove the
        // GPU write is host-visible; that additionally requires the memory
        // to be host-coherent (or an explicit invalidate). See
        // `collect_image_health`'s doc comment for why that holds here.
        // #2740 (REN-D4-04).
        self.collect_image_health(frame);

        // #1194 — read this slot's TIMESTAMP results (from the prior
        // cycle's use of this slot), then reset the pool for the
        // upcoming frame. The fence wait above proves the prior
        // submission for this slot is complete, so query results
        // are guaranteed available — no host stall here. First-cycle
        // reads return zero (active_bits never set yet); steady-state
        // reads are one MAX_FRAMES_IN_FLIGHT cycle behind, which is
        // fine for per-pass instrumentation.
        if let Some(ref mut timers) = self.gpu_timers {
            timers.read_and_reset(&self.device, frame);
        }

        // If a screenshot was captured last frame, the GPU is done — read it back.
        self.screenshot_finish_readback();

        // Acquire next swapchain image. Bracketed (Phase 9) so a
        // FIFO-present-mode block waiting for the next image is
        // surfaced in `CpuFrameTimings.acquire_ms` rather than
        // disappearing into the gap between fence_wait and
        // cmd_record. The acquire itself blocks until the image
        // is available; on most desktop drivers + Wayland/X11
        // compositors this is also where vsync ends up.
        let acquire_t0 = Instant::now();
        // SAFETY: swapchain + loader are live; `image_available[frame]` is an unsignaled binary semaphore (its prior signal was consumed by last cycle's submit wait on this slot) so acquiring into it is legal. The OUT_OF_DATE arm bails before the semaphore is depended on.
        let (image_index, suboptimal) = unsafe {
            match self.swapchain_state.swapchain_loader.acquire_next_image(
                self.swapchain_state.swapchain,
                u64::MAX,
                self.frame_sync.image_available[frame],
                vk::Fence::null(),
            ) {
                Ok((idx, suboptimal)) => (idx, suboptimal),
                Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => return Ok(true),
                Err(e) => anyhow::bail!("acquire_next_image: {:?}", e),
            }
        };
        t.acquire_ns = acquire_t0.elapsed().as_nanos() as u64;

        let img = image_index as usize;

        // From here through `queue_submit` below, `image_available[frame]`
        // is signal-pending (set by the acquire above) and any `?`-
        // propagated error would leak the signal into the next acquire,
        // tripping VUID-vkAcquireNextImageKHR-semaphore-01779. Each
        // fallible call between this point and the submit recovers via
        // `recreate_image_available_for_frame` — sibling to the
        // `in_flight` fence recovery already wired through
        // `recreate_for_swapchain` (#908). See #910 / REN-D5-NEW-01.

        // If this swapchain image is still in use by a different frame, wait.
        let image_fence = self.frame_sync.images_in_flight[img];
        if image_fence != vk::Fence::null() && image_fence != self.frame_sync.in_flight[frame] {
            // SAFETY: `image_fence` is a live fence belonging to whichever frame last used this swapchain image; it was a `queue_submit` signal-target, so the wait terminates. Guarantees that image's prior frame finished before we reuse it. On error we clear the pending acquire signal before propagating.
            unsafe {
                if let Err(e) = self
                    .device
                    .wait_for_fences(&[image_fence], true, u64::MAX)
                    .context("wait for image fence")
                {
                    let _ = self
                        .frame_sync
                        .recreate_image_available_for_frame(&self.device, frame);
                    return Err(e);
                }
            }
        }
        self.frame_sync.images_in_flight[img] = self.frame_sync.in_flight[frame];

        // #952 / REN-D1-NEW-04 — `reset_fences` MOVED to immediately
        // before `queue_submit`. Pre-fix this ran here, then ~2200
        // lines of `?`-propagated fallible work followed before the
        // submit re-signaled the fence. Any error in that window left
        // the fence UNSIGNALED with no pending submit, and the next
        // frame's both-slots `wait_for_fences(..., u64::MAX)` at
        // lines 174-183 blocked forever — logical deadlock matching
        // the resize-path window closed by #908. Reorder narrows the
        // window to a single fallible call; the submit-failure error
        // arm below additionally recreates the fence to cover that
        // residual case.

        // Deferred-destroy tick. Runs AFTER `wait_for_fences` so every
        // resource whose countdown reaches zero this frame is
        // guaranteed unreferenced by any in-flight command buffer.
        // Pre-#418 this ran at the TOP of `draw_frame`, before the
        // fence wait — `AccelerationManager::tick_deferred_destroy`
        // (and the `mesh_registry` / `texture_registry` siblings, all
        // three destroy GPU resources) could free a BLAS / buffer /
        // image the previous frame's TLAS or blit was still reading.
        // Latent because `MAX_FRAMES_IN_FLIGHT`-conservative countdowns
        // kept the window from ever closing, but a policy change that
        // shortened the countdown would have turned this into a
        // sync2-validated use-after-free.
        //
        // `texture_registry.begin_frame` advances the internal frame
        // counter that the tick compares against — must run BEFORE the
        // tick so the counter reflects "this frame" during the
        // deferred-destroy decision.
        self.texture_registry.begin_frame(&self.device, frame);
        if let Some(ref alloc) = self.allocator {
            self.mesh_registry
                .tick_deferred_destroy(&self.device, alloc);
            self.texture_registry
                .tick_deferred_destroy(&self.device, alloc);
            if let Some(ref mut accel) = self.accel_manager {
                accel.tick_deferred_destroy(&self.device, alloc);
            }
        }

        // Re-point the RT-shading global-geometry descriptor (bindings 8/9)
        // to the CURRENT global SSBO for THIS frame-in-flight, every frame.
        // The global vertex/index SSBO is reallocated to a brand-new
        // `VkBuffer` whenever cell-stream growth marks geometry dirty
        // (`MeshRegistry::rebuild_geometry_ssbo`), but the binding was
        // written only ONCE at scene setup (`scene.rs::setup_scene`). Without
        // this per-frame refresh the descriptor keeps naming the OLD buffer,
        // which `rebuild_geometry_ssbo` defers to the destroy queue and
        // `tick_deferred_destroy` (just above) frees `MAX_FRAMES_IN_FLIGHT`
        // frames later — at which point the next RT hit-fetch
        // (`getHitUV` / `getHitTriNormal`, bindings 8/9, on the
        // reflection / refraction / GI paths) dereferences freed device
        // memory → GPU page fault → ~TDR → `VK_ERROR_DEVICE_LOST`. The
        // raster path never hit this because it re-fetches the buffer fresh
        // each frame (`cmd_bind_vertex_buffers` below); only the once-bound
        // RT descriptor dangled. Mirrors `write_tlas` (binding 2, re-pointed
        // every frame): safe because `in_flight[frame]` was just waited on,
        // so this frame's descriptor set is idle. See WATAL §0 device-loss
        // hunt. (bindings 8/9 are PARTIALLY_BOUND, so the None case — no
        // geometry yet / headless — leaves them validly unbound.)
        if let (Some(vb), Some(ib)) = (
            self.mesh_registry.global_vertex_buffer.as_ref(),
            self.mesh_registry.global_index_buffer.as_ref(),
        ) {
            self.scene_buffers.write_geometry_buffers(
                &self.device,
                frame,
                vb.buffer,
                vb.size,
                ib.buffer,
                ib.size,
            );
        }

        // Record command buffer. Indexed by frame-in-flight (not swapchain
        // image) so the fence and command buffer share the same slot — #259.
        // Safe because in_flight[frame] was just waited on, guaranteeing
        // the GPU has finished with this cmd buffer's previous recording.
        let cmd = self.command_buffers[frame];
        // SAFETY: `cmd` is `command_buffers[frame]`, whose fence `in_flight[frame]` was just waited on above, so the GPU has finished its previous recording and the buffer is safe to reset. On error we clear the pending acquire signal before propagating.
        unsafe {
            if let Err(e) = self
                .device
                .reset_command_buffer(cmd, vk::CommandBufferResetFlags::empty())
                .context("reset_command_buffer")
            {
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                return Err(e);
            }
        }

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        // SAFETY: `cmd` was just reset (above) and is in the initial state; it is recorded by this thread only, so beginning recording with ONE_TIME_SUBMIT is valid. On error we clear the pending acquire signal before propagating.
        unsafe {
            if let Err(e) = self
                .device
                .begin_command_buffer(cmd, &begin_info)
                .context("begin_command_buffer")
            {
                let _ = self
                    .frame_sync
                    .recreate_image_available_for_frame(&self.device, frame);
                return Err(e);
            }
        }

        // 8 color attachments + depth. Order must match the render pass:
        //   0 HDR, 1 normal, 2 motion, 3 mesh_id, 4 raw_indirect, 5 albedo,
        //   6 fsr_reactive, 7 fsr_transparency, 8 depth.
        let zero_f = vk::ClearValue {
            color: vk::ClearColorValue {
                float32: [0.0, 0.0, 0.0, 0.0],
            },
        };
        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: clear_color,
                },
            },
            zero_f, // normal
            zero_f, // motion
            vk::ClearValue {
                // Mesh ID: 0 reserved for background (shader writes id + 1).
                color: vk::ClearColorValue {
                    uint32: [0, 0, 0, 0],
                },
            },
            zero_f, // raw_indirect (background: no light)
            zero_f, // albedo (background: no color)
            // Both FSR masks clear to zero — "fully described by depth and
            // motion" — and only transparent draws MAX-blend a value in.
            zero_f, // fsr_reactive
            zero_f, // fsr_transparency
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

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

        // Pre-compute the shared `draw_idx → ssbo_idx` map once so the
        // TLAS `instance_custom_index` values stay in lockstep with the
        // compacted SSBO positions regardless of which filter rejects a
        // draw_cmd. Before #419 the TLAS path used the raw enumerate
        // index while the SSBO builder used `gpu_instances.len()` —
        // identical only while `mesh_registry.get()` never returned None
        // for a submitted command. A single evicted mesh would shift
        // every subsequent SSBO entry by one while TLAS custom indices
        // stayed put, producing silently-wrong material/transform reads
        // on every RT hit downstream (shadows / reflections / GI /
        // caustics / primary-hit fallback in `triangle.frag`). See
        // `crates/renderer/src/vulkan/acceleration.rs::build_tlas` and
        // the SSBO builder below — both must honour this map.
        let tlas_t0 = Instant::now();
        let instance_map: Vec<Option<u32>> = super::super::acceleration::build_instance_map(
            draw_commands.len(),
            super::super::scene_buffer::MAX_INSTANCES,
            |i| {
                self.mesh_registry
                    .get(draw_commands[i].mesh_handle)
                    .is_some()
            },
        );
        // M29 Phase 2: TLAS build moved to AFTER bone upload + skin
        // chain (compute dispatch + BLAS refit) so the TLAS sees this
        // frame's skinned poses with zero lag. instance_map computed
        // here stays valid through the move — it's a pure function of
        // draw_commands + mesh_registry state.

        // Upload scene data (lights + camera) BEFORE the render pass begins.
        self.scene_buffers
            .upload_lights(&self.device, frame, lights)
            .unwrap_or_else(|e| log::warn!("Failed to upload lights: {e}"));
        // `tlas_written[frame]` lags one frame per FIF slot — on the
        // first frame each slot gets a successful TLAS, this still reads
        // `false` because `write_tlas` runs later in `draw_frame` (see
        // the `patch_camera_rt_flag` site post-TLAS-build). The first-
        // frame fallback to `rt_flag = 0.0` is corrected in-place after
        // `write_tlas` flips the bit, so frame 0 still gets RT-enabled
        // shading at GPU-submit time. See #1227 / REN-D8-NEW-21.
        let rt_flag =
            if self.device_caps.ray_query_supported && self.scene_buffers.tlas_written[frame] {
                1.0
            } else {
                0.0
            };

        // TAA sub-pixel jitter via Halton(2,3) sequence. Each frame shifts
        // the projection by a different sub-pixel offset in NDC so that
        // temporal blending reconstructs a super-sampled result. The offset
        // is applied in the vertex shader AFTER motion vector computation so
        // reprojection is jitter-free.
        //
        // Period 16 (#1093 / REN-D11-002): Halton(2) natural period is 2,
        // Halton(3) natural period is 3, LCM = 6. Using 16 (nearest power-
        // of-2 ≥ 6) avoids the asymmetric Y-coverage gap that % 8 caused
        // (the 9th Halton(3) sample ≈ 0.889 was never reached with % 8).
        // #1932 / TAA-D13-01 — gate on `!self.taa_failed` too, matching the
        // dispatch gate above and `upload_params`. Without it, a permanent
        // TAA failure would leave composite reading raw un-resolved HDR
        // (per #479's fallback) while geometry kept rendering with a
        // per-frame Halton sub-pixel offset — full-frame shimmer instead of
        // a stable pinhole fallback image.
        let (jx, jy, fsr_jitter_pixel, fsr_reset_pending) = match self.renderer_config.upscaler {
            super::super::upscaling::UpscalerMode::Taa => {
                let (jx, jy) = taa_jitter(
                    self.taa.is_some(),
                    self.taa_failed,
                    self.frame_counter,
                    self.frame_extents.render.width as f32,
                    self.frame_extents.render.height as f32,
                );
                (jx, jy, None, false)
            }
            super::super::upscaling::UpscalerMode::Fsr3(_) => {
                if !self.is_fsr_dispatch_active() {
                    (0.0, 0.0, None, false)
                } else {
                    let fsr = self
                        .fsr_temporal
                        .as_ref()
                        .expect("FSR mode must own temporal state");
                    let sample = fsr.current();
                    (
                        sample.ndc[0],
                        sample.ndc[1],
                        Some(sample.pixel),
                        fsr.reset_pending(),
                    )
                }
            }
        };

        // Camera-relative render origin (#markarth-precision). Computed
        // ONCE by `render::camera::assemble_camera` (the same un-jittered
        // camera position it used to build the RELATIVE `view_proj`) and
        // threaded in via `FrameInputs::render_origin` (#2043 / PERF-D9-04)
        // — this consumer no longer recomputes `snap_render_origin`
        // independently, so the rebased per-instance models below and the
        // uploaded matrices are structurally guaranteed to agree on the
        // origin rather than relying on both call sites happening to be
        // passed the same value. Uploaded `view_proj` / `inv_view_proj` are
        // relative; the vertex shader reconstructs the absolute world
        // position as `worldPos_rel + renderOrigin`. Passes that
        // reconstruct world from an inverse VP either add the origin back
        // where absolute space is required (cluster_cull, caustic_splat,
        // volumetrics_inject) or stay fully relative with a relative
        // camera position (ssao, composite — origin-invariant differences
        // only). See `GpuCamera::render_origin` (#1492).
        let render_origin = byroredux_core::math::Vec3::from_array(input_render_origin);
        // DOF aperture-disk jitter, or the pinhole pass-through. The bokeh
        // rationale and the #1525 degenerate-`focus_dist` guard live in
        // `dof_effective_view_proj`.
        // FSR forces the pinhole path — rationale in `fsr_gated_dof`.
        //
        // #2518 — gate on FSR actually *dispatching*, not merely on FSR
        // mode being selected. `fsr_temporal` is `Some` for the whole of
        // `UpscalerMode::Fsr3(..)`, including when the FSR context never got
        // created or `dispatch_failure` has latched. In those states the
        // frame runs completely unjittered on the native blit (the jitter
        // gate above sets `fsr_jitter_pixel = None` and `jx/jy = 0.0` on
        // exactly this predicate), so the stated rationale — that the
        // independent Halton(5,7) lens sequence would conflict with FSR's
        // own projection jitter — does not apply, yet authored DOF was
        // still being silently dropped. Both facts now come from one
        // predicate so they cannot diverge again.
        let active_dof = fsr_gated_dof(dof, self.is_fsr_dispatch_active());
        let (effective_vp, effective_cam_pos) = dof_effective_view_proj(
            &active_dof,
            self.frame_counter,
            camera_pos,
            render_origin,
            view_proj,
        );
        let vp = &effective_vp;
        let mut fsr_frame = match build_fsr_frame_parameters(
            &active_dof,
            fsr_jitter_pixel,
            fsr_reset_pending,
            frame_time_delta_ms,
        ) {
            Ok(params) => params,
            Err(e) => {
                let _ = unsafe {
                    // SAFETY: this early-return happens after the swapchain
                    // image acquire but before any batch is submitted this
                    // frame (the `return` below aborts before `queue_submit`),
                    // `frame < MAX_FRAMES_IN_FLIGHT` per the caller's frame
                    // index, and `self.device` is the same device that
                    // allocated the existing semaphore.
                    self.frame_sync
                        .recreate_image_available_for_frame(&self.device, frame)
                };
                return Err(e);
            }
        };
        // Automatic camera-cut detection catches debug teleports and scripted
        // snaps that do not flow through the cell-transition reset hooks.
        // Signal derivation + rationale in `camera_frame_deltas`.
        let previous_camera_position = self.prev_camera_position;
        let CameraFrameDeltas {
            camera_delta,
            cam_forward_dot,
            vp_max_abs_delta,
        } = camera_frame_deltas(
            camera_pos,
            self.prev_camera_position,
            active_dof.cam_forward,
            self.prev_cam_forward,
            vp,
            &self.prev_view_proj,
        );
        let camera_cut = is_camera_cut(self.frame_counter, camera_delta, cam_forward_dot);
        if camera_cut {
            self.signal_temporal_discontinuity(8);
            if let Some(ref mut frame) = fsr_frame {
                frame.reset = true;
            }
        }
        // #1489 / REN2-04 — `prev_view_proj` is relative to LAST frame's
        // render origin O₁; this frame's geometry (per-instance models, bone
        // palettes) is rebased by the CURRENT origin O₂. On a 4096-grid
        // crossing the two differ and every motion vector would be off by
        // ΔO — a one-frame full-screen TAA flash + SVGF history drop per
        // crossing. Right-multiplying by `translation(O₂ − O₁)` makes the
        // uploaded matrix consume current-origin positions exactly:
        // `M·(x − O₂) = prev_vp·(x − O₁)`. Off the jump frame ΔO = 0 and
        // the correction is the identity.
        let pvp = if camera_cut {
            // Reset history and emit zero velocity on the cut frame. Keeping
            // the old matrix here would feed extreme motion into the
            // disocclusion filters even though their history was flushed.
            *vp
        } else {
            origin_corrected_prev_view_proj(
                &self.prev_view_proj,
                self.prev_render_origin,
                [render_origin.x, render_origin.y, render_origin.z],
            )
        };
        // Precompute inverse(viewProj) once on the CPU so shaders
        // (cluster culling, SSAO) can read it directly from the UBO
        // instead of computing a ~100 ALU-op matrix inverse per invocation.
        let vp_mat = byroredux_core::math::Mat4::from_cols_array(vp);
        let inv_vp = vp_mat.inverse();
        let inv_vp_cols = inv_vp.to_cols_array();
        let inv_vp_arr = [
            [
                inv_vp_cols[0],
                inv_vp_cols[1],
                inv_vp_cols[2],
                inv_vp_cols[3],
            ],
            [
                inv_vp_cols[4],
                inv_vp_cols[5],
                inv_vp_cols[6],
                inv_vp_cols[7],
            ],
            [
                inv_vp_cols[8],
                inv_vp_cols[9],
                inv_vp_cols[10],
                inv_vp_cols[11],
            ],
            [
                inv_vp_cols[12],
                inv_vp_cols[13],
                inv_vp_cols[14],
                inv_vp_cols[15],
            ],
        ];
        // Camera-static detection for progressive temporal accumulation.
        // The view-proj here is jitter-free (TAA sub-pixel jitter is applied
        // later in the vertex shader), so a matrix unchanged frame-to-frame
        // means a parked camera. Computed BEFORE the camera UBO is built so
        // the flag can ride `dof_params.w` into triangle.frag's GI-seed
        // decorrelation, and BEFORE `prev_view_proj` is overwritten below.
        let camera_static = vp
            .iter()
            .zip(self.prev_view_proj.iter())
            .all(|(a, b)| (a - b).abs() < 1.0e-6);
        let camera = scene_buffer::GpuCamera {
            view_proj: [
                [vp[0], vp[1], vp[2], vp[3]],
                [vp[4], vp[5], vp[6], vp[7]],
                [vp[8], vp[9], vp[10], vp[11]],
                [vp[12], vp[13], vp[14], vp[15]],
            ],
            prev_view_proj: [
                [pvp[0], pvp[1], pvp[2], pvp[3]],
                [pvp[4], pvp[5], pvp[6], pvp[7]],
                [pvp[8], pvp[9], pvp[10], pvp[11]],
                [pvp[12], pvp[13], pvp[14], pvp[15]],
            ],
            inv_view_proj: inv_vp_arr,
            // w = monotonic frame counter for temporal jitter seed in
            // shadow rays. Masked to the bottom 24 bits before the
            // `u32 → f32` cast so consecutive frames remain
            // distinguishable for the full uptime of the process:
            // f32's mantissa stops resolving ±1 increments above 2^24,
            // so a raw cast at frame 16_777_217 would map to the same
            // `cameraPos.w` as frame 16_777_216 and the RT noise
            // patterns (reservoir streaming, shadow / reflection /
            // refraction jitter, GI hemisphere) would freeze. Wrap at
            // 2^24 instead — the noise pattern repeats every ~3.2 days
            // at 60 FPS (acceptable; TAA accumulation absorbs the
            // discontinuity). See #1161 / REN-D9-NEW-08.
            position: [
                effective_cam_pos[0],
                effective_cam_pos[1],
                effective_cam_pos[2],
                (self.frame_counter & 0xFFFFFF) as f32,
            ],
            flags: [
                rt_flag,
                ambient_color[0],
                ambient_color[1],
                ambient_color[2],
            ],
            screen: [
                self.frame_extents.render.width as f32,
                self.frame_extents.render.height as f32,
                fog_near,
                fog_far,
            ],
            fog: [
                fog_color[0],
                fog_color[1],
                fog_color[2],
                if fog_extinction_per_meter > 0.0 {
                    1.0
                } else {
                    0.0
                }, // fog enabled flag
            ],
            // jitter[2] carries the debug-bypass bitmask for the
            // fragment shader (see `parse_render_debug_flags_env` and
            // `triangle.frag`'s `floatBitsToUint(jitter.z)` branches).
            // Zero-bits → free no-op; non-zero → debug paths active.
            //
            // jitter[3] carries the per-frame `is_exterior` flag
            // (#1125 / REN-D9-NEW-01). 1.0 = exterior cell (real TOD-
            // driven SkyParamsRes loaded), 0.0 = interior cell (or no
            // exterior load yet — `SkyParamsRes` absent so
            // `build_sky_params` returned `SkyParams::default()` with
            // clear-noon-blue zenith). The shader uses this to gate
            // `skyTint`-blended fallbacks in `traceReflection` /
            // refraction miss so sealed interiors don't bleed
            // daylight tint into glass refractions.
            jitter: [
                jx,
                jy,
                // REND-#1451 — OR the runtime legacy-attenuation toggle
                // (console-driven via LightTuning) onto the env-set
                // debug bitmask so both paths reach the shader's
                // `DBG_LEGACY_LIGHT_ATTEN` branch.
                f32::from_bits(
                    self.render_debug_flags
                        | if self.light_atten_legacy {
                            crate::shader_constants::DBG_LEGACY_LIGHT_ATTEN
                        } else {
                            0
                        },
                ),
                if sky_params.is_exterior { 1.0 } else { 0.0 },
            ],
            // #925 / REN-D15-NEW-03 — mirror the composite's
            // `sky_zenith.xyz` here so triangle.frag's window-portal
            // escape transmits a sky tint matching whatever
            // `compute_sky` paints behind the world. Same source of
            // truth → same TOD/weather cross-fade behaviour at no
            // extra upload cost.
            //
            // w = sun_angular_radius (rad). Plumbed from SkyParams so
            // PCSS-lite directional-shadow disk jitter in triangle.frag
            // is tunable per-cell / per-TOD without a shader recompile.
            // See #1023 / REN-D20-NEW-01.
            sky_tint: [
                sky_params.zenith_color[0],
                sky_params.zenith_color[1],
                sky_params.zenith_color[2],
                sky_params.sun_angular_radius,
            ],
            // #1210 — sun direction + intensity, plumbed for water.frag's
            // caustic synthesis (shadow ray to sun → refract on miss).
            // SkyParams.sun_direction is already unit-length and in
            // world space. w carries authored intensity so the caustic
            // splat scales with TOD / weather (dawn / dusk = dimmer
            // caustics, noon = peak).
            sun_direction: [
                sky_params.sun_direction[0],
                sky_params.sun_direction[1],
                sky_params.sun_direction[2],
                sky_params.sun_intensity,
            ],
            // x = aperture half-radius (0.0 → pinhole, DOF jitter skipped),
            // y = focal distance.
            // z = REND-#1451 point/spot attenuation knee fraction,
            // consumed by `pointSpotAtten` in triangle.frag (0 → shader
            // default 0.5). Live-tunable via the `light.atten` console
            // command for the controlled bench.
            // w = camera_static flag (1.0 = parked). triangle.frag reads it
            // to advance the GI noise seed every frame when parked, so the
            // dark indirect-lit floor converges ~4× faster (TARGET 1).
            dof_params: [
                active_dof.aperture,
                active_dof.focus_dist,
                self.light_atten_knee,
                if camera_static { 1.0 } else { 0.0 },
            ],
            // #markarth-precision — camera-relative render origin in xyz.
            // Vertex/deferred shaders add this back to recover the absolute
            // world position from the relative `view_proj` space.
            //
            // `w` is NOT padding (REN-LOW L-10 / #2164): it carries the
            // FSR one-frame-reset flag, read by `triangle.frag`'s FSR-reset
            // debug view. Any shader that treats this as a free slot will
            // fight that consumer — same trap as #1928's
            // `VolumetricsParams.render_origin.w`.
            render_origin: [
                render_origin.x,
                render_origin.y,
                render_origin.z,
                if fsr_reset_pending { 1.0 } else { 0.0 },
            ],
        };
        self.scene_buffers
            .upload_camera(&self.device, frame, &camera)
            .unwrap_or_else(|e| log::warn!("Failed to upload camera: {e}"));
        // #993 — upload the per-TOD-lerped 6-axis directional ambient
        // cube (Skyrim WTHR.DALC). When the cell carries no DALC
        // (FNV / FO3 / Oblivion), `sky_params.dalc_cube` is `None`;
        // we upload a disabled cube so the fragment shader stays on
        // its AMBIENT_AO_FLOOR fallback path. The `flags.x` field is
        // the runtime gate the shader reads.
        let dalc_gpu = if let Some(cube) = sky_params.dalc_cube {
            super::super::scene_buffer::GpuDalcCube {
                pos_x: [cube.pos_x[0], cube.pos_x[1], cube.pos_x[2], 0.0],
                neg_x: [cube.neg_x[0], cube.neg_x[1], cube.neg_x[2], 0.0],
                pos_y: [cube.pos_y[0], cube.pos_y[1], cube.pos_y[2], 0.0],
                neg_y: [cube.neg_y[0], cube.neg_y[1], cube.neg_y[2], 0.0],
                pos_z: [cube.pos_z[0], cube.pos_z[1], cube.pos_z[2], 0.0],
                neg_z: [cube.neg_z[0], cube.neg_z[1], cube.neg_z[2], 0.0],
                specular_fresnel: [
                    cube.specular[0],
                    cube.specular[1],
                    cube.specular[2],
                    cube.fresnel_power,
                ],
                flags: [1.0, 0.0, 0.0, 0.0],
            }
        } else {
            super::super::scene_buffer::GpuDalcCube::default()
        };
        self.scene_buffers
            .upload_dalc(&self.device, frame, &dalc_gpu)
            .unwrap_or_else(|e| log::warn!("Failed to upload DALC cube: {e}"));
        // `camera_static` was computed above (before the camera UBO was
        // built) so the flag could ride `dof_params.w` into triangle.frag's
        // GI-seed decorrelation; it is reused here for the SVGF / TAA /
        // caustic param uploads. Store this frame's viewProj as next frame's
        // "previous" for motion vectors — together with the origin it was
        // built against, so next frame's upload can origin-correct it
        // (#1489 / REN2-04).
        // #2171 — capture the origin delta BEFORE `prev_render_origin` is
        // overwritten on the next line. The trace below used to subtract
        // the field after the assignment, so it printed exactly zero every
        // frame — actively arguing "no origin crossing happened" on
        // precisely the frames the ghosting investigation was looking at.
        let origin_delta = [
            render_origin.x - self.prev_render_origin[0],
            render_origin.y - self.prev_render_origin[1],
            render_origin.z - self.prev_render_origin[2],
        ];

        self.prev_view_proj = *vp;
        self.prev_camera_position = camera_pos;
        self.prev_render_origin = [render_origin.x, render_origin.y, render_origin.z];
        self.prev_cam_forward = active_dof.cam_forward;

        // #1874 diagnostic — ghosted diagonal double-image investigation.
        // Cheap, stateless (uses only locals already computed above) trace
        // of the exact values Dim 10 reasoned about statically: the
        // render-origin/view-proj delta this frame carries and whether a
        // discontinuity-recovery window is active. Enable via
        // `RUST_LOG=byroredux_renderer::vulkan::context::draw=trace` to
        // correlate a live repro's cell-transition frame against these
        // numbers instead of guessing from static analysis alone. Safe to
        // leave in — trace level, zero new state, filtered out by default.
        log::trace!(
            "camera frame={} static={} svgf_recovery_frames={} render_origin_delta=({:.3},{:.3},{:.3}) vp_max_abs_delta={:.6}",
            self.frame_counter,
            camera_static,
            self.svgf_recovery_frames,
            origin_delta[0],
            origin_delta[1],
            origin_delta[2],
            vp_max_abs_delta,
        );

        // D6-04 / #1811 — track how many consecutive frames had no
        // skinned-pose change and no pending first-sight bind_inverses
        // upload. Any dirty signal resets the streak so the forthcoming
        // upload/copy/dispatch trio (below) always runs at least once
        // per change, and for the next `MAX_FRAMES_IN_FLIGHT` frames
        // after that so every per-frame `bone_world` buffer copy sees
        // the fresh value at least once (same safety margin as the
        // `MAX_FRAMES_IN_FLIGHT + 1` sweep threshold in
        // `SkinSlotPool::sweep` / `build_skinned_palettes`).
        let skin_state_dirty = !pose_dirty.is_empty() || !bind_inverse_pending_uploads.is_empty();
        self.clean_skin_frames = next_clean_skin_frames(self.clean_skin_frames, skin_state_dirty);
        let skip_skin_gpu_refresh = should_skip_skin_gpu_refresh(self.clean_skin_frames);

        // M29.5/M29.6 — upload bone_world (per-frame) and any pending
        // first-sight bind_inverses (write-once persistent SSBO). The
        // skin_palette dispatch below reads both:
        //   - bone_world from the per-frame DEVICE_LOCAL pair
        //   - bind_inverses from the persistent DEVICE_LOCAL SSBO
        // and writes the existing palette SSBO that raster +
        // skin_vertices.comp consume.
        //
        // D6-04 / #1811 — skipped entirely once `skip_skin_gpu_refresh`
        // is true: every live frame-in-flight buffer already holds
        // today's (unchanged) bone_world content, so the staging
        // memcpy + device copy would just rewrite identical bytes.
        if !skip_skin_gpu_refresh {
            if !bone_world.is_empty() {
                self.scene_buffers
                    .upload_bone_worlds(&self.device, frame, bone_world)
                    .unwrap_or_else(|e| log::warn!("Failed to upload bone_world: {e}"));
            }
            self.scene_buffers
                .record_bone_world_copy(&self.device, cmd, frame);
        }

        // M29.6 — drain pending bind_inverses first-sight uploads.
        // Two-stage: write into HOST_VISIBLE staging, then record
        // per-slot cmd_copy_buffer regions into the persistent SSBO,
        // followed by a single TRANSFER → COMPUTE_SHADER barrier.
        // No-op when the pending list is empty (steady-state).
        let pending_capped = if !bind_inverse_pending_uploads.is_empty() {
            self.scene_buffers
                .upload_pending_bind_inverses(&self.device, bind_inverse_pending_uploads)
                .unwrap_or_else(|e| {
                    log::warn!("Failed to upload pending bind_inverses: {e}");
                    0
                })
        } else {
            0
        };
        if pending_capped > 0 {
            let pending_slots: Vec<u32> = bind_inverse_pending_uploads
                .iter()
                .take(pending_capped)
                .map(|(s, _)| *s)
                .collect();
            self.scene_buffers.record_pending_bind_inverse_copies(
                &self.device,
                cmd,
                &pending_slots,
                pending_capped,
            );
        }

        // M29.5/M29.6 — dispatch the palette-build compute pass.
        // Writes the existing `bone_device_buffers[frame]` SSBO that
        // raster (`triangle.vert:147-204` inline-skinning, set 1
        // binding 3 + binding 12) and `skin_vertices.comp` (set 0
        // binding 1 in SkinComputePipeline) read. Emits the
        // COMPUTE_SHADER_WRITE → (COMPUTE_SHADER_READ | VERTEX_SHADER_READ)
        // barrier on the palette buffer after the dispatch so both
        // downstream consumers see well-defined data.
        if let Some(ref mut skin_palette) = self.skin_palette {
            let bone_byte_size = self.scene_buffers.bone_input_upload_bytes(frame);
            // Each palette slot is one mat4 = 64 B. Skip the dispatch
            // entirely when there are no skinned bones this frame —
            // the palette buffer retains its prior contents (slot 0
            // identity from a previous frame's write, or zero on
            // frame 0), so any raster sampling at `bone_offset = 0`
            // either reads identity (post-warm) or garbage that
            // never gets shaded (no entity points there).
            let bone_count =
                (bone_byte_size as usize / std::mem::size_of::<[[f32; 4]; 4]>()) as u32;
            // D6-04 / #1811 — also skip once `skip_skin_gpu_refresh` is
            // true: the palette buffer already holds the correct output
            // for today's (unchanged) bone_world + bind_inverses, so a
            // full-range recompute would just rewrite identical data.
            if bone_count > 0 && !skip_skin_gpu_refresh {
                let bone_world_buf = self.scene_buffers.bone_world_buffers()[frame].buffer;
                let bind_inverse_buf = self.scene_buffers.bind_inverses_persistent().buffer;
                let bind_inverse_size = self.scene_buffers.bone_buffer_size();
                let palette_buf = self.scene_buffers.bone_buffers()[frame].buffer;
                let palette_size = self.scene_buffers.bone_buffer_size();
                // SAFETY: `cmd` is recording (begin_command_buffer succeeded above); the bone-world / bind-inverse / palette buffers are live SSBOs for this frame and `bone_count > 0`. The COMPUTE_SHADER_WRITE -> SHADER_READ buffer barrier afterward sequences the palette write before its compute + vertex consumers; no concurrent recording of this buffer.
                unsafe {
                    skin_palette.dispatch(
                        &self.device,
                        cmd,
                        frame,
                        super::super::skin_compute::PaletteDispatchBuffers {
                            bone_world_buffer: bone_world_buf,
                            bone_world_buffer_size: bone_byte_size,
                            bind_inverse_buffer: bind_inverse_buf,
                            bind_inverse_buffer_size: bind_inverse_size,
                            palette_buffer: palette_buf,
                            palette_buffer_size: palette_size,
                        },
                        super::super::skin_compute::SkinPalettePushConstants { bone_count },
                    );
                    // COMPUTE_SHADER_WRITE → SHADER_READ barrier on the
                    // palette buffer covers both downstream consumers:
                    // `skin_vertices.comp` (compute read in this same
                    // command buffer below) and `triangle.vert` (vertex
                    // read during the raster pass).
                    let palette_barrier = vk::BufferMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::SHADER_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .buffer(palette_buf)
                        .offset(0)
                        .size(palette_size);
                    self.device.cmd_pipeline_barrier(
                        cmd,
                        vk::PipelineStageFlags::COMPUTE_SHADER,
                        vk::PipelineStageFlags::COMPUTE_SHADER
                            | vk::PipelineStageFlags::VERTEX_SHADER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[palette_barrier],
                        &[],
                    );
                }
            }
        }

        self.record_skinned_blas_refit(cmd, frame, draw_commands, pose_dirty);

        // ── TLAS build (relocated from top of frame) ─────────────────
        // Picks up just-refit per-skinned-entity BLAS via the
        // `bone_offset != 0` override in `build_tlas`. Static draws
        // continue using the per-mesh `blas_entries` table.
        // SAFETY: `cmd` is recording; `accel` and `alloc` are live. `build_tlas` records the TLAS build into `cmd` over this frame's just-refit BLAS; the following AS_BUILD_WRITE -> FRAGMENT|COMPUTE READ barrier sequences it before the ray-query consumers. `write_tlas` / `patch_camera_rt_flag` touch this frame's descriptor + UBO, idle by the fence wait.
        unsafe {
            if let Some(ref mut accel) = self.accel_manager {
                if let Some(alloc) = self.allocator.as_ref() {
                    if let Some(ref mut timers) = self.gpu_timers {
                        timers.cmd_tlas_build_start(&self.device, cmd, frame);
                    }
                    if let Err(e) = accel.build_tlas(
                        &self.device,
                        alloc,
                        cmd,
                        draw_commands,
                        &instance_map,
                        frame,
                    ) {
                        log::warn!("TLAS build failed: {e}");
                        // #2673 / CONC-D1-NEW-01 — defence in depth for
                        // the warn-only policy above. `tlas_written` is
                        // otherwise a one-way latch, so a slot that ever
                        // had a TLAS keeps `rt_flag = 1.0` forever and
                        // every RT path (shadows, reflections, GI, water
                        // refraction) keeps ray-querying binding 2 on a
                        // frame whose build never landed. Re-point the
                        // binding at whatever AS the manager still owns
                        // (post-#2673 a failed resize keeps the previous
                        // one alive), then clear the latch and drop
                        // `rt_flag` so this frame degrades to non-RT
                        // shading instead. The next successful build
                        // re-latches and re-patches it to 1.0 via the
                        // `first_tlas_this_slot` path below.
                        if let Some(stale_handle) = accel.tlas_handle(frame) {
                            self.scene_buffers
                                .write_tlas(&self.device, frame, stale_handle);
                        }
                        // Ordered after `write_tlas`, which latches the
                        // flag `true` as a side effect.
                        self.scene_buffers.tlas_written[frame] = false;
                        if let Err(e) =
                            self.scene_buffers
                                .patch_camera_rt_flag(&self.device, frame, 0.0)
                        {
                            log::warn!("Failed to clear rt_flag after TLAS build failure: {e}");
                        }
                    } else {
                        if let Some(ref mut timers) = self.gpu_timers {
                            timers.cmd_tlas_build_end(&self.device, cmd, frame);
                        }
                        // Memory barrier: TLAS build → ray-query consumers
                        // (FRAGMENT_SHADER for main render pass +
                        // COMPUTE_SHADER for caustic_splat.comp). See
                        // #415 for the COMPUTE_SHADER widening.
                        // AS_BUILD_KHR → FRAGMENT_SHADER|COMPUTE_SHADER
                        memory_barrier(
                            &self.device,
                            cmd,
                            vk::PipelineStageFlags::ACCELERATION_STRUCTURE_BUILD_KHR,
                            vk::AccessFlags::ACCELERATION_STRUCTURE_WRITE_KHR,
                            vk::PipelineStageFlags::FRAGMENT_SHADER
                                | vk::PipelineStageFlags::COMPUTE_SHADER,
                            vk::AccessFlags::ACCELERATION_STRUCTURE_READ_KHR,
                        );
                        if let Some(tlas_handle) = accel.tlas_handle(frame) {
                            // Capture whether this is the first time the
                            // TLAS lands for this FIF slot — `write_tlas`
                            // flips `tlas_written[frame] = true`, but
                            // we want to know if it WAS false before.
                            let first_tlas_this_slot = !self.scene_buffers.tlas_written[frame];
                            self.scene_buffers
                                .write_tlas(&self.device, frame, tlas_handle);
                            // #1227 / REN-D8-NEW-21 — earlier in this
                            // frame `rt_flag` was uploaded as 0.0 because
                            // `tlas_written[frame]` was still false at
                            // camera-UBO upload time. Now that the TLAS
                            // exists and the descriptor is wired, patch
                            // `flags[0]` to 1.0 in-place so the upcoming
                            // render pass sees RT enabled on this very
                            // frame. Without this, frame 0 + frame 1
                            // (one per FIF slot) render with RT shading
                            // off and TAA dissolves the flash across
                            // ~5 frames on every cell-load. Only fires
                            // on RT-capable hardware AND only on the
                            // slot's first valid-TLAS frame — steady
                            // state pays nothing.
                            if first_tlas_this_slot && self.device_caps.ray_query_supported {
                                if let Err(e) = self.scene_buffers.patch_camera_rt_flag(
                                    &self.device,
                                    frame,
                                    1.0,
                                ) {
                                    log::warn!("Failed to patch rt_flag post-TLAS: {e}");
                                }
                            }
                        }
                        // #1792 — `pending_bytes = 0`: no in-flight batch
                        // context at this per-frame call site.
                        accel.evict_unused_blas(&self.device, alloc, 0);
                    }
                }
            }
        }
        t.tlas_build_ns = tlas_t0.elapsed().as_nanos() as u64;

        // ── Cluster light culling (compute dispatch) ─────────────────
        //
        // Runs after light + camera uploads, before the render pass.
        // The compute shader reads lights/camera and writes cluster SSBOs
        // that the fragment shader reads during the render pass.
        // SAFETY: `cmd` is recording; `cc` (cluster-cull pipeline) and its per-frame cluster SSBOs are live. The leading HOST_WRITE -> COMPUTE barrier makes the host-written light/camera buffers visible before `dispatch`; the trailing COMPUTE_WRITE -> FRAGMENT_READ barrier sequences the cluster SSBO outputs before the render pass reads them.
        unsafe {
            if let Some(ref cc) = self.cluster_cull {
                // Barrier: host writes to light/camera SSBOs must be visible
                // to the compute shader before dispatch. Required by Vulkan
                // spec even for HOST_COHERENT memory. Instance data is NOT
                // uploaded yet — it is built and uploaded after this dispatch.
                // HOST → COMPUTE_SHADER (light/camera UBO flush)
                memory_barrier(
                    &self.device,
                    cmd,
                    vk::PipelineStageFlags::HOST,
                    vk::AccessFlags::HOST_WRITE,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_READ | vk::AccessFlags::UNIFORM_READ,
                );

                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_cluster_cull_start(&self.device, cmd, frame);
                }
                cc.dispatch(&self.device, cmd, frame);
                if let Some(ref mut timers) = self.gpu_timers {
                    timers.cmd_cluster_cull_end(&self.device, cmd, frame);
                }
                // Barrier: compute writes → fragment reads on cluster SSBOs.
                // COMPUTE_SHADER → FRAGMENT_SHADER (cluster SSBO outputs)
                memory_barrier(
                    &self.device,
                    cmd,
                    vk::PipelineStageFlags::COMPUTE_SHADER,
                    vk::AccessFlags::SHADER_WRITE,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::AccessFlags::SHADER_READ,
                );
            }
        }

        // ── Build instance SSBO + draw batches ────────────────────────
        //
        // Each DrawCommand becomes one GpuInstance in the SSBO. Consecutive
        // commands with the same (pipeline_key, render_layer, mesh_handle) are
        // merged into a single instanced draw call.
        //
        // The two working vectors are held on `self` as scratch buffers
        // (`gpu_instances_scratch`, `batches_scratch`). `mem::take` moves
        // them out so the rest of draw_frame can continue borrowing other
        // fields of `self` without fighting the borrow checker; at the
        // bottom of the function they are moved back, amortizing their
        // capacity across frames. Error-path early returns lose the
        // amortization for one frame only — acceptable since the draw
        // has already failed. See issue #243.
        let ssbo_t0 = Instant::now();
        let mut gpu_instances: Vec<GpuInstance> = std::mem::take(&mut self.gpu_instances_scratch);
        gpu_instances.clear();
        gpu_instances.reserve(draw_commands.len() + 1); // +1 for optional UI quad
        let mut previous_models = std::mem::take(&mut self.previous_models_scratch);
        previous_models.clear();
        previous_models.reserve(draw_commands.len() + 1);
        let mut current_rigid_models = std::mem::take(&mut self.current_rigid_models_scratch);
        current_rigid_models.clear();
        current_rigid_models.reserve(draw_commands.len());
        let mut batches: Vec<DrawBatch> = std::mem::take(&mut self.batches_scratch);
        batches.clear();
        batches.reserve(draw_commands.len());

        // Sort contract for draw_commands is owned by render.rs
        // `build_render_data`. The per-field cluster order is covered
        // by the unit test `render::sort_key_clusters_by_alpha_decal_twosided`
        // (#500 D3-M2). A duplicate debug_assert here drifted out of
        // sync with the real key and was removed rather than kept in
        // lockstep across two crates.
        for draw_cmd in draw_commands {
            let Some(mesh) = self.mesh_registry.get(draw_cmd.mesh_handle) else {
                continue;
            };

            let instance_idx = gpu_instances.len() as u32;
            let m = &draw_cmd.model_matrix;
            let skip_batch = !draw_cmd.in_raster || draw_cmd.is_water;
            let current_model = rebase_model_matrix(m, render_origin);
            let uses_rigid_history =
                uses_rigid_motion_history(draw_cmd.bone_offset, draw_cmd.alpha_blend);
            let previous_source = if uses_rigid_history && !camera_cut {
                self.previous_rigid_models
                    .get(&draw_cmd.entity_id)
                    .unwrap_or(m)
            } else {
                m
            };
            previous_models.push(rebase_model_matrix(previous_source, render_origin));
            if uses_rigid_history {
                current_rigid_models.insert(draw_cmd.entity_id, *m);
            }

            // #1260 / PERF-D3-NEW-05 — flag-bit assembly is rasterizer-
            // only state. The non-uniform-scale dot products feed the
            // vertex shader's inverse-transpose path (triangle.vert
            // line 175); ALPHA_BLEND / FLAT_SHADING / TERRAIN_SPLAT /
            // RENDER_LAYER are all read only by the rasterized fragment
            // shader (`inst.flags & ...` at triangle.frag:1011 / 1074 /
            // 1119 / 1231 / 1728); CAUSTIC_SOURCE is gated by the
            // meshId G-buffer (caustic_splat.comp:170-172), which only
            // contains pixels for in-frustum rasterized geometry. The
            // RT hit paths read `hitInst.vertexOffset / indexOffset /
            // materialId / avgAlbedo* / textureIndex` (triangle.frag:
            // 438 / 543 / 2981 / 2147) but NEVER `hitInst.flags`.
            // Therefore off-frustum + water entries can ship `flags=0`
            // and skip the entire assembly block — the SSBO slot still
            // serves the RT contract (#516) via model+mesh refs +
            // material_id + avg_albedo, which are written
            // unconditionally below.
            let flags = if skip_batch {
                0u32
            } else {
                // Detect non-uniform scale from the model matrix column
                // lengths. If the 3 column vectors of the upper-3x3
                // have different lengths, the vertex shader must use
                // inverse-transpose for normals. Otherwise it can skip
                // the expensive inverse (~40 ALU ops). Three dot
                // products is trivial compared to the per-vertex savings.
                let col0_sq = m[0] * m[0] + m[1] * m[1] + m[2] * m[2];
                let col1_sq = m[4] * m[4] + m[5] * m[5] + m[6] * m[6];
                let col2_sq = m[8] * m[8] + m[9] * m[9] + m[10] * m[10];
                let has_non_uniform_scale = {
                    let tol = 0.001;
                    (col0_sq - col1_sq).abs() > tol || (col0_sq - col2_sq).abs() > tol
                };
                // Per-instance flags — see INSTANCE_FLAG_* constants in
                // scene_buffer.rs. CPU-side assembly must stay in
                // lockstep with the fragment shader's `flags & N` checks.
                //   bit 0 = non-uniform scale
                //   bit 1 = NiAlphaProperty blend bit
                //   bit 2 = caustic source — real refractive surface
                //           (#922 / REN-D13-NEW-01). Gate matches the
                //           upstream glass classification in
                //           `render::build_render_data` (#515 / #706):
                //           engine-classified `MATERIAL_KIND_GLASS`
                //           (alpha-blend + low metal + low roughness +
                //           not a decal) OR Skyrim+ `MultiLayerParallax`
                //           (kind 11) with a non-zero inner-layer
                //           refraction scale.
                //   bit 3 = terrain splat (set in cell_loader for LAND
                //           entities, #470).
                let mut f = if has_non_uniform_scale {
                    INSTANCE_FLAG_NON_UNIFORM_SCALE
                } else {
                    0u32
                };
                if draw_cmd.alpha_blend {
                    f |= INSTANCE_FLAG_ALPHA_BLEND;
                    // #1653 — tells the fragment shader the diffuse carries
                    // a GENUINE authored alpha channel. When clear (BC1 and
                    // other alpha-less formats) the shader pins texColor.a
                    // to 1.0 unless an alpha test is active, so a BC1
                    // 3-colour block's index-3 texel (a==0 in opaque
                    // regions, an RGB-fidelity encoder choice) can't leak
                    // transparency into the discard / decalWeight /
                    // finalAlpha paths on a pure-blend mesh. BC1 decodes as
                    // BC1_RGBA so its 1-bit punch-through still drives
                    // alpha-test cutouts (2aac5351). `handle_has_alpha` is
                    // false for BC1_RGBA (`format_has_alpha` excludes it)
                    // and true for BC2/BC3/BC7/RGBA, so the FNV picture/
                    // table blend keeps its authored alpha. Cheap cached
                    // lookup (same map as the gi_albedo mean below), gated
                    // on alpha_blend so the opaque majority pays nothing.
                    if self
                        .texture_registry
                        .handle_has_alpha(draw_cmd.texture_handle)
                    {
                        f |= INSTANCE_FLAG_DIFFUSE_ALPHA;
                    }
                }
                if is_caustic_source(draw_cmd) {
                    f |= INSTANCE_FLAG_CAUSTIC_SOURCE;
                }
                if let Some(tile_idx) = draw_cmd.terrain_tile_index {
                    f |= INSTANCE_FLAG_TERRAIN_SPLAT;
                    f |= (tile_idx & INSTANCE_TERRAIN_TILE_MASK) << INSTANCE_TERRAIN_TILE_SHIFT;
                }
                // #869 — NiShadeProperty.flags==0 flat-shading:
                // fragment shader replaces interpolated normal with
                // the per-face derivative when this bit is set.
                if draw_cmd.flat_shading {
                    f |= INSTANCE_FLAG_FLAT_SHADING;
                }
                // #renderlayer — pack the 2-bit layer discriminant
                // into bits 4..5 for the fragment shader's debug-viz
                // branch (BYROREDUX_RENDER_DEBUG=0x40 tints fragments
                // by layer).
                f |= (draw_cmd.render_layer as u32 & INSTANCE_RENDER_LAYER_MASK)
                    << INSTANCE_RENDER_LAYER_SHIFT;
                f
            };

            // R1 Phase 6 — `GpuInstance` carries only per-DRAW data
            // now: model + mesh refs + bone_offset + flags +
            // material_id + caustic-source avg_albedo. Every
            // per-material field reads through `materials[material_id]`
            // in the fragment shader.
            //
            // #1628 — fold the diffuse texture's texel-mean into the GI
            // bounce albedo. `draw_cmd.avg_albedo` is the material tint
            // (diffuse_color); multiplying it by the texture's average
            // texel colour gives the true surface mean a textured wall
            // bleeds into the one-bounce GI, instead of the flat tint.
            // The mean is computed once at DDS upload and cached per
            // handle, so this is a cheap lookup + multiply. Untextured /
            // normal-map / BC7 handles return `None` and keep the tint.
            let gi_albedo = match self
                .texture_registry
                .handle_avg_rgb(draw_cmd.texture_handle)
            {
                Some(mean) => [
                    draw_cmd.avg_albedo[0] * mean[0],
                    draw_cmd.avg_albedo[1] * mean[1],
                    draw_cmd.avg_albedo[2] * mean[2],
                ],
                None => draw_cmd.avg_albedo,
            };
            // REN-2026-07-28-02 / #2219 — skinned instances' secondary-ray
            // hit-normal reconstruction needs the deformed (post-skin)
            // vertex positions, not the bind-pose global vertex SSBO
            // `getHitTriNormal` otherwise reads unconditionally. Look up
            // this entity's SkinSlot (populated earlier this frame by the
            // skin-dispatch chain) and query its output buffer's GPU
            // address; `ray_hit.glsl` dereferences it via
            // GL_EXT_buffer_reference for `boneOffset != 0` instances
            // instead of the bind-pose path. Zero for rigid draws and for
            // skinned draws with no slot yet (first-sight frame, or a
            // pool-exhaustion fallback) — the shader's own `boneOffset !=
            // 0` branch means a stray zero address is never dereferenced
            // by a rigid draw, and a skinned draw with no slot yet already
            // has no primed skinned BLAS this frame either, so falling
            // back to the bind-pose hit-normal path is consistent with
            // what the rest of the RT pipeline does for that draw.
            let slot_address = (draw_cmd.bone_offset != 0)
                .then(|| self.skin_slots.get(&draw_cmd.entity_id))
                .flatten()
                // #2402 — the slot must have been sized for THIS mesh. The
                // refit path's capacity reconciliation skips non-RT-capable
                // meshes entirely, so a remap onto one leaves a stale slot
                // live for a few frames; publishing its address would have
                // the fragment shader index a raw device address with the
                // new mesh's (possibly larger) index range.
                .filter(|slot| skin_slot_backs_mesh(slot.vertex_count(), mesh.vertex_count))
                .map(|slot| {
                    // SAFETY: `slot.output_buffer.buffer` is a live
                    // DEVICE_LOCAL buffer created with
                    // SHADER_DEVICE_ADDRESS usage (see
                    // `SkinComputePipeline::create_slot`).
                    unsafe {
                        self.device.get_buffer_device_address(
                            &vk::BufferDeviceAddressInfo::default()
                                .buffer(slot.output_buffer.buffer),
                        )
                    }
                });
            let skinned_vertex_address =
                skinned_vertex_address_for_draw(draw_cmd.bone_offset, slot_address);
            gpu_instances.push(GpuInstance {
                // #markarth-precision — rebase the model translation by the
                // camera-relative render origin so `model * pos` stays near 0
                // in the shader (full f32 precision; large worldspace offsets
                // like MarkarthWorld's ~-176000 otherwise quantize fine detail
                // into spikes). The shader adds render_origin back for the
                // absolute world position. Columns 0-2 (rotation/scale) are
                // unchanged; only the translation column (m[12..14]) shifts.
                model: current_model,
                texture_index: draw_cmd.texture_handle,
                bone_offset: draw_cmd.bone_offset,
                vertex_offset: mesh.global_vertex_offset,
                index_offset: mesh.global_index_offset,
                vertex_count: mesh.vertex_count,
                flags,
                material_id: draw_cmd.material_id,
                // Reuse the layout's former padding lane for per-material IOR.
                // caustic_splat.comp names this offset `ior`; other shaders
                // keep treating it as padding, so the std430 ABI is unchanged.
                ior: draw_cmd.ior,
                avg_albedo_r: gi_albedo[0],
                avg_albedo_g: gi_albedo[1],
                avg_albedo_b: gi_albedo[2],
                // Stable across per-frame sort/batch changes. Zero remains
                // reserved for synthetic/default instances.
                surface_id: draw_cmd.entity_id.wrapping_add(1),
                skinned_vertex_address,
                _reserved: [0; 2],
            });

            // Frustum-culled draws still need an SSBO entry so RT hit
            // shaders that land on their TLAS instance read the right
            // material / transform (#516).
            //
            // REN-LOW L-8 / #2164 — "transform" needs a caveat. Since the
            // render-origin rebase, `GpuInstance.model` is render-origin
            // *relative*, while the TLAS an RT hit arrives through is
            // absolute. Rotation and scale are therefore usable from a hit
            // shader; translation is NOT. The only current RT reader
            // (`raytrace.glsl::getHitTriNormal`) is translation-invariant,
            // so nothing is wrong today — but a future hit-position
            // reconstruction built on `.model[3]` would land `renderOrigin`
            // (up to ~176k units on MarkarthWorld) from the true hit. Add
            // `+ renderOrigin.xyz` if you ever need absolute position here.
            //
            // Skip batch formation — they
            // have no rasterized pixels this frame. Breaking the batch
            // chain here also avoids accidentally extending a previous
            // batch across a gap in the SSBO layout (`first_instance +
            // instance_count` would point past an off-screen draw).
            //
            // Water surfaces are also skipped here: their `GpuInstance`
            // SSBO slot is populated (so the water pipeline's vertex
            // shader can read the model matrix via `gl_InstanceIndex`),
            // but they render through the dedicated water pipeline in
            // a separate pass below — not through the triangle / blend
            // pipeline batches.
            if skip_batch {
                continue;
            }

            // Two-sided is NOT a key axis (#930) — both opaque and
            // blended pipelines declare CULL_MODE as dynamic state, so
            // two-sided rendering uses per-draw `cmd_set_cull_mode`
            // not a separate pipeline. Wireframe IS a key axis (#869)
            // because `polygon_mode` is static pipeline state — LINE
            // and FILL each need their own pipeline.
            let order_dependent_glass = is_refractive_glass(draw_cmd);
            let pipeline_key = if draw_cmd.alpha_blend {
                PipelineKey::Blended {
                    src: draw_cmd.src_blend,
                    dst: draw_cmd.dst_blend,
                    wireframe: draw_cmd.wireframe,
                    preserve_opaque_gbuffer: order_dependent_glass,
                }
            } else {
                PipelineKey::Opaque {
                    wireframe: draw_cmd.wireframe,
                }
            };

            // Extend the current batch if this draw shares the same
            // state AND is contiguous in the SSBO (no culled draws in
            // the gap). The contiguity check is new with #516 — before
            // the in_raster split the SSBO idx always advanced 1:1
            // with the batch-eligible iterations, so contiguity was
            // implicit. Now an off-screen draw pushes an SSBO entry
            // but skips batch formation, so the next rasterized draw
            // might land at a non-contiguous `instance_idx`.
            // #renderlayer — depth bias is selected from the per-layer
            // ladder via `DrawCommand::render_layer`. `RenderLayer::Decal`
            // subsumes both the legacy `is_decal` and `needs_depth_bias`
            // bits — alpha-tested rugs / posters / fences and true
            // NIF-flagged decals all carry `render_layer == Decal` set
            // at cell-load time.
            let render_layer = draw_cmd.render_layer;

            // #2165 — split-eligibility is a material property, resolved
            // once here at emit time. Part of the batch merge key: a
            // glass draw and a particle draw that happen to agree on
            // every pipeline/depth axis must not fold together, or the
            // merged batch would take one population's path for both.
            if let Some(batch) = batches.last_mut() {
                if batch.mesh_handle == draw_cmd.mesh_handle
                    && batch.pipeline_key == pipeline_key
                    && batch.two_sided == draw_cmd.two_sided
                    && batch.render_layer == render_layer
                    && batch.z_test == draw_cmd.z_test
                    && batch.z_write == draw_cmd.z_write
                    && batch.z_function == draw_cmd.z_function
                    && batch.order_dependent_glass == order_dependent_glass
                    && batch.first_instance + batch.instance_count == instance_idx
                {
                    batch.instance_count += 1;
                    continue;
                }
            }

            // Start a new batch.
            batches.push(DrawBatch {
                mesh_handle: draw_cmd.mesh_handle,
                pipeline_key,
                two_sided: draw_cmd.two_sided,
                render_layer,
                first_instance: instance_idx,
                instance_count: 1,
                index_count: mesh.index_count,
                global_index_offset: mesh.global_index_offset,
                global_vertex_offset: mesh.global_vertex_offset as i32,
                z_test: draw_cmd.z_test,
                z_write: draw_cmd.z_write,
                z_function: draw_cmd.z_function,
                order_dependent_glass,
            });
        }

        // Append UI instance (if needed) BEFORE the bulk upload so it's
        // included in the single flush. Avoids the need for a separate raw
        // pointer write + flush that was missing on non-coherent memory (#189).
        let ui_instance_idx =
            if let (Some(ui_tex), Some(_)) = (ui_texture_handle, self.ui_quad_handle) {
                let idx = gpu_instances.len() as u32;
                let instance = GpuInstance {
                    texture_index: ui_tex,
                    ..GpuInstance::default()
                };
                previous_models.push(instance.model);
                gpu_instances.push(instance);
                Some(idx)
            } else {
                None
            };

        // #647 / RP-1 — guard against `gl_InstanceIndex` outrunning
        // the `MAX_INSTANCES` SSBO allocation. Post-#992 the mesh_id
        // G-buffer is `R32_UINT` (bit 31 = ALPHA_BLEND_NO_HISTORY,
        // bits 0..30 = id + 1, ceiling 0x7FFFFFFF), and `MAX_INSTANCES`
        // is sized at `0x40000` (262144) to absorb dense Skyrim/FO4
        // city cells (~50K REFRs) with ~5× headroom. The SSBO is
        // sized to `MAX_INSTANCES`, so writes past that index would
        // overrun the GPU-side allocation. `upload_instances` clamps to
        // MAX_INSTANCES in release; we log and continue rather than
        // panicking inside an active command-buffer recording (#956 /
        // REN-D5-NEW-05 — a debug_assert! at this site leaks the
        // in-flight cmd buffer on unwind).
        if gpu_instances.len() > super::super::scene_buffer::MAX_INSTANCES {
            static ONCE: std::sync::Once = std::sync::Once::new();
            ONCE.call_once(|| {
                log::error!(
                    "RP-1: visible instance count {} exceeds MAX_INSTANCES ({}). \
                     Instances past the cap are silently dropped. \
                     Bump MAX_INSTANCES or partition draws.",
                    gpu_instances.len(),
                    super::super::scene_buffer::MAX_INSTANCES,
                );
            });
        }
        // Upload all instance data (scene + UI) to the SSBO in one flush.
        if !gpu_instances.is_empty() {
            debug_assert_eq!(gpu_instances.len(), previous_models.len());
            self.scene_buffers
                .upload_instances(&self.device, frame, &gpu_instances)
                .unwrap_or_else(|e| log::warn!("Failed to upload instances: {e}"));
            self.scene_buffers
                .upload_previous_models(&self.device, frame, &previous_models)
                .unwrap_or_else(|e| log::warn!("Failed to upload previous models: {e}"));
        }

        // R1 Phase 4 — upload the deduplicated material table. The
        // fragment shader reads `materials[instance.materialId]` for
        // migrated fields (Phase 4: roughness; Phases 5–6: the rest).
        // Empty table means no draws → no material reads, so the
        // upload is skipped harmlessly.
        if !materials.is_empty() {
            self.scene_buffers
                .upload_materials(&self.device, frame, materials)
                .unwrap_or_else(|e| log::warn!("Failed to upload materials: {e}"));
        }

        // Feed the last retired main-pass timestamp into the hysteretic ray
        // allocator, then upload a fresh per-frame counter + loop limits.
        // Timer brackets are conservative upper bounds and cannot safely be
        // summed; use the slower controlled pass as the quality signal.
        let measured_lighting_ms = self
            .gpu_timers
            .as_ref()
            .map(|timers| {
                let snapshot = timers.last_snapshot();
                snapshot.main_render_ms.max(snapshot.volumetrics_ms)
            })
            .filter(|ms| *ms > 0.0);
        self.scene_buffers
            .reset_ray_budget(&self.device, frame, measured_lighting_ms)
            .unwrap_or_else(|e| log::warn!("Failed to upload adaptive ray budget: {e}"));

        // Reupload the terrain tile SSBO when cell load mutated it.
        // The slab is static until the next cell transition — #497
        // moved it to a single DEVICE_LOCAL buffer uploaded via a
        // transient staging copy, so one upload per dirty transition
        // is enough. The scratch Vec lives on self so its 32 KB
        // capacity amortizes across cell loads — `mem::take` moves it
        // out so the fill can run while `&self.scene_buffers` consumes
        // the slice. #496.
        let mut tile_scratch: Vec<GpuTerrainTile> = std::mem::take(&mut self.terrain_tile_scratch);
        if self.fill_terrain_tile_scratch_if_dirty(&mut tile_scratch) {
            let allocator = self.allocator.as_ref().expect("allocator missing");
            self.scene_buffers
                .upload_terrain_tiles(
                    &self.device,
                    allocator,
                    &self.graphics_queue,
                    self.transfer_pool,
                    &tile_scratch,
                )
                .unwrap_or_else(|e| log::warn!("Failed to upload terrain tiles: {e}"));
        }
        self.terrain_tile_scratch = tile_scratch;

        // Build + upload indirect-draw commands for this frame (#309).
        // One `VkDrawIndexedIndirectCommand` per DrawBatch, laid out in
        // the same order as `batches` so the draw loop can reference a
        // contiguous range of the buffer for each pipeline group.
        // Populated regardless of `device_caps.multi_draw_indirect_supported`
        // — the upload is ~N × 20 B for small N, and this keeps the
        // indirect path always ready when it is enabled.
        if !batches.is_empty() && self.device_caps.multi_draw_indirect_supported {
            let indirect_scratch = &mut self.indirect_draws_scratch;
            indirect_scratch.clear();
            indirect_scratch.extend(batches.iter().map(|b| vk::DrawIndexedIndirectCommand {
                index_count: b.index_count,
                instance_count: b.instance_count,
                first_index: b.global_index_offset,
                vertex_offset: b.global_vertex_offset,
                first_instance: b.first_instance,
            }));
            // #2504 / D12-2026-08-07-02 — unlike the neighbouring data-SSBO
            // uploads above (stale content there only misrenders), the
            // indirect buffer's contents are fetched and executed by the
            // GPU. A failed upload must force the direct-draw fallback for
            // this frame in `record_geometry_pass` (`use_indirect` reads
            // `indirect_upload_ok`) rather than let `cmd_draw_indexed_
            // indirect` read stale or uninitialized commands.
            self.indirect_upload_ok = self
                .scene_buffers
                .upload_indirect_draws(&self.device, frame, indirect_scratch)
                .map_err(|e| {
                    log::warn!(
                        "Failed to upload indirect draws: {e} — falling back to direct draws this frame"
                    )
                })
                .is_ok();
        }
        t.ssbo_build_ns = ssbo_t0.elapsed().as_nanos() as u64;

        // Pre-populate the blend pipeline cache for any new (src, dst)
        // combos this frame. Resolved up-front because the hot draw
        // loop only takes `&self.device` for `cmd_bind_pipeline` and
        // can't reborrow `&mut self` to lazy-create. After this loop
        // every `PipelineKey::Blended` has a corresponding cache entry.
        // See #392 / #930 (two-sided dropped from key).
        // #1259 / PERF-D3-NEW-04 — pre-fix this loop did
        // `blend_pipeline_cache.contains_key` per batch (M = blended
        // batch count, typically 300-500 on a Skyrim exterior). After
        // the first few cell-load frames every (src, dst, wireframe)
        // combo is cached and the per-batch lookup always hits —
        // O(M) wasted work per frame in steady state.
        //
        // Two-stage swap: collect distinct keys into the persistent
        // `blend_seen_scratch` HashSet (O(M) inserts, but on a
        // typically-tiny set — the same 3-5 distinct combos repeat
        // across hundreds of batches), then walk the small set once.
        // The subset check after the walk also lets us skip the
        // creation pass entirely when every seen key is cached —
        // the common steady-state path.
        self.blend_seen_scratch.clear();
        for batch in &batches {
            if let PipelineKey::Blended {
                src,
                dst,
                wireframe,
                preserve_opaque_gbuffer,
            } = batch.pipeline_key
            {
                // Normalize cache key against the device-cap gate so a
                // disabled-wireframe device hits the same slot it would
                // for a regular opaque blend. Matches the gate in
                // `get_or_create_blend_pipeline`. #869.
                let wireframe = wireframe && self.device_caps.fill_mode_non_solid_supported;
                self.blend_seen_scratch
                    .insert((src, dst, wireframe, preserve_opaque_gbuffer));
            }
        }
        // Skip the creation pass when every seen key is already cached
        // (the steady-state fast path — after warmup, no new pipeline
        // creation needed).
        let all_cached = self
            .blend_seen_scratch
            .iter()
            .all(|key| self.blend_pipeline_cache.contains_key(key));
        if !all_cached {
            // Collect missing keys into a local Vec so we can release
            // the borrow on `blend_seen_scratch` before calling
            // `get_or_create_blend_pipeline` (which takes `&mut self`
            // and would re-borrow scratch via the cache field).
            let missing: Vec<(u8, u8, bool, bool)> = self
                .blend_seen_scratch
                .iter()
                .filter(|key| !self.blend_pipeline_cache.contains_key(key))
                .copied()
                .collect();
            for (src, dst, wireframe, preserve_opaque_gbuffer) in missing {
                if let Err(e) =
                    self.get_or_create_blend_pipeline(src, dst, wireframe, preserve_opaque_gbuffer)
                {
                    log::error!(
                        "Failed to create blend pipeline (src={src}, dst={dst}, \
                         preserve_opaque_gbuffer={preserve_opaque_gbuffer}): {e}; \
                         draws using this combo will fall back to opaque pipeline"
                    );
                }
            }
        }

        // Upload composite params (fog + sky) up-front so the bulk host
        // barrier below covers this UBO's HOST_WRITE too (#909 /
        // REN-D1-NEW-03). All inputs are available from `draw_frame`'s
        // parameters; the composite pass itself runs much later, after
        // the render pass + SVGF / TAA / SSAO / Bloom, but the barrier
        // doesn't care when the consumer runs as long as it's been
        // emitted before the consumer.
        if let Some(ref mut composite) = self.composite {
            let composite_params = build_composite_params(CompositeParamsInputs {
                fog_color,
                fog_near,
                fog_far,
                fog_extinction_per_meter,
                fog_single_scatter_albedo,
                fog_clip,
                fog_power,
                fog_height_reference,
                sky_params,
                exposure_value: self
                    .exposure
                    .as_ref()
                    .map_or(super::super::exposure::DEFAULT_EXPOSURE, |e| e.value()),
                frame_counter: self.frame_counter,
                volume_far_distance: self
                    .volumetrics
                    .as_ref()
                    .map_or(super::super::volumetrics::DEFAULT_VOLUME_FAR, |volume| {
                        volume.far_distance_world()
                    }),
                froxel_slice_count: self
                    .volumetrics
                    .as_ref()
                    .map_or(1.0, |volume| volume.extent().depth as f32),
                camera_pos,
                render_origin,
                inv_vp_arr,
                underwater,
                water_caustic_active: self.water_caustic_accum.is_some(),
            });
            if let Err(e) = composite.upload_params(&self.device, frame, &composite_params) {
                log::warn!("composite upload_params failed: {e}");
            }
        }

        // SVGF temporal params UBO — uploaded BEFORE the bulk barrier
        // below so its HOST_WRITE → UNIFORM_READ at COMPUTE_SHADER fold
        // into the same execution dependency the bulk barrier already
        // emits for composite. Mirrors the composite-UBO fold from
        // #909 / REN-D1-NEW-03. See #961 / REN-D10-NEW-04. The α state
        // machine is host-side and depends on `svgf_recovery_frames`
        // (advanced at end-of-tick); it does NOT depend on anything
        // produced by the render pass below.
        if !self.svgf_failed {
            if let Some(ref mut svgf) = self.svgf {
                let (alpha_color, alpha_moments, next_frames) =
                    crate::vulkan::svgf::next_svgf_temporal_alpha(self.svgf_recovery_frames);
                self.svgf_recovery_frames = next_frames;
                // SAFETY: `svgf`'s host-visible param buffer for `frame` is live and not in use by an in-flight frame (the fence wait at frame start guarantees the prior use of this slot completed); the host write is made visible to the compute pass by the bulk HOST->COMPUTE barrier below.
                if let Err(e) = unsafe {
                    svgf.upload_params(
                        &self.device,
                        frame,
                        alpha_color,
                        alpha_moments,
                        camera_static,
                    )
                } {
                    log::warn!("svgf upload_params failed: {e}");
                }
            }
        }

        // TAA UBO — fold into the bulk barrier below (#1397 / NCPS-03).
        // upload_params writes the host-visible param_buffers[frame];
        // the HOST→COMPUTE dependency is covered by the bulk barrier's
        // dst_stage = COMPUTE_SHADER, so no per-dispatch barrier is needed.
        if !self.taa_failed {
            if let Some(ref mut taa) = self.taa {
                if let Err(e) = taa.upload_params(&self.device, frame) {
                    log::warn!("TAA upload_params failed: {e}");
                }
            }
        }

        // Bloom UBOs — #2037 / GPU-D5-01: every down/upsample param UBO
        // is a pure function of the (construction-time-fixed) mip
        // extents, so `BloomPipeline::new` writes them once and a
        // resize (which rebuilds the whole pipeline) re-enters that
        // same write. No per-frame upload needed here; only the
        // input_view descriptor update (which depends on the
        // render-pass HDR output) stays in dispatch().

        // Barrier: make the instance SSBO host write (and any remaining
        // light/camera/bone host writes) visible to the vertex + fragment
        // shaders in the upcoming render pass. Also covers all UBO host
        // writes uploaded above (composite, SVGF, TAA, bloom) — each
        // write completes before this barrier and the barrier's dst_stage
        // includes COMPUTE_SHADER, so every post-render-pass compute
        // consumer that had its UBO folded here needs no per-dispatch
        // HOST→COMPUTE barrier. Fold history: composite (#909 /
        // REN-D1-NEW-03), SVGF (#961 / REN-D10-NEW-04), TAA + bloom
        // (#1397 / NCPS-03). Required by Vulkan spec even for
        // HOST_COHERENT memory.
        // HOST → VERTEX|FRAGMENT|COMPUTE|DRAW_INDIRECT (instance SSBO + UBOs)
        // SAFETY: `cmd` is recording. This single HOST_WRITE -> VERTEX|FRAGMENT|COMPUTE|DRAW_INDIRECT barrier makes every host-written buffer this frame (instance SSBO + composite/SVGF/TAA/bloom UBOs) visible to its shader consumers before the render pass; required by spec even for HOST_COHERENT memory.
        unsafe {
            memory_barrier(
                &self.device,
                cmd,
                vk::PipelineStageFlags::HOST,
                vk::AccessFlags::HOST_WRITE,
                vk::PipelineStageFlags::VERTEX_SHADER
                    | vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COMPUTE_SHADER
                    | vk::PipelineStageFlags::DRAW_INDIRECT,
                vk::AccessFlags::SHADER_READ
                    | vk::AccessFlags::UNIFORM_READ
                    | vk::AccessFlags::INDIRECT_COMMAND_READ,
            );
        }

        // #1255 / Phase C of #1210 — clear the water-caustic
        // accumulator BEFORE the main render pass begins. water.frag
        // (the live Phase D/E consumer) atomic-adds into it during
        // the main pass; the post-render-pass barrier below
        // sequences those writes to the composite read.
        // Skipped when the accumulator failed init (None) — graceful
        // degrade matches the rest of the renderer's optional-pipeline
        // policy.
        if let Some(ref wca) = self.water_caustic_accum {
            // SAFETY: `cmd` is recording and outside the render pass; `wca` (water-caustic accumulator) and its per-frame buffer are live. The clear is recorded before the main pass that atomic-adds into it, and the post-pass barrier sequences those writes to the composite read.
            unsafe { wca.clear_pre_render_pass(&self.device, cmd, frame) };
        }

        let cmd_t0 = Instant::now();
        self.record_geometry_pass(
            cmd,
            frame,
            &render_pass_begin,
            &batches,
            draw_commands,
            water_commands,
            ui_instance_idx,
        );
        // SAFETY: tail of the per-frame command buffer — depth-history
        // snapshot, post/denoise/composite chain, egui overlay, screenshot
        // copy, and `end_command_buffer`. Each call documents its own
        // recording-order contract; this is the same single `unsafe` scope
        // `draw_frame` opened before the geometry pass was extracted (#1748).
        unsafe {
            // Soft-particle depth fade: snapshot this frame's opaque depth
            // into the sampleable history image so next frame's effect-shader
            // FX can feather their alpha against the geometry behind them.
            // The transparent FX wrote no depth (z_write off), so the depth
            // buffer here holds opaque-only depth. Restores depth to
            // READ_ONLY afterwards so SSAO / SVGF / composite read it
            // unchanged. See `crates/renderer/shaders/triangle.frag`
            // (MATERIAL_KIND_EFFECT_SHADER soft-fade block).
            self.copy_depth_to_history(cmd);

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
                camera_static,
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
                fog_volumes,
                fsr_frame,
                underwater,
                image_space_modifier,
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

        // Present.
        let swapchains = [self.swapchain_state.swapchain];
        let image_indices = [image_index];
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
        let working_instances = gpu_instances.len();
        let working_batches = batches.len();
        self.gpu_instances_scratch = gpu_instances;
        self.previous_models_scratch = previous_models;
        self.batches_scratch = batches;
        super::super::acceleration::shrink_scratch_if_oversized(
            &mut self.gpu_instances_scratch,
            working_instances,
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
                    // this frame's recording began). Order matters:
                    // run AFTER `shrink_tlas_to_fit` so a destroyed
                    // slot lets the scratch shrink hit its
                    // "tlas[slot] is None → drop scratch entirely"
                    // arm in one tick.
                    accel.shrink_tlas_scratch_to_fit(slot_to_shrink, &self.device, allocator);
                }
            }
        }

        Ok(suboptimal || present_suboptimal)
    }
}

/// Rebase an absolute column-major model matrix into the current camera-relative
/// render-origin space. Current and previous rigid transforms use the same
/// origin so [`origin_corrected_prev_view_proj`] can project both coherently.
fn rebase_model_matrix(
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
fn origin_corrected_prev_view_proj(
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
            ior: 1.5,        // #1248
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
            supplemental_texture_indices: [0; 12],
            translucency_subsurface_color: [0.0; 3],
            translucency_transmissive_scale: 0.0,
            translucency_turbulence: 0.0,
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
        assert!(is_caustic_source(&cmd(11, 0.3)));
    }

    #[test]
    fn multi_layer_parallax_without_refraction_is_not_caustic() {
        // Kind 11 with zero refraction scale = parallax but no refraction.
        assert!(!is_caustic_source(&cmd(11, 0.0)));
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

    /// All three prerequisites hold — the happy path that actually reaches
    /// `cmd_draw_indexed_indirect`.
    #[test]
    fn true_when_bound_supported_and_upload_succeeded() {
        assert!(should_use_indirect_draws(true, true, true));
    }

    /// The regression case: everything else says "go indirect" but this
    /// frame's upload failed. Must fall back to direct draws.
    #[test]
    fn false_when_upload_failed_even_if_otherwise_eligible() {
        assert!(!should_use_indirect_draws(true, true, false));
    }

    #[test]
    fn false_when_global_buffer_not_bound() {
        assert!(!should_use_indirect_draws(false, true, true));
    }

    #[test]
    fn false_when_device_lacks_multi_draw_indirect() {
        assert!(!should_use_indirect_draws(true, false, true));
    }
}
