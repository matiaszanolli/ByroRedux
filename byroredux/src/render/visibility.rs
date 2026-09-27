//! Game-neutral visibility classification for extracted render candidates.
//!
//! The input is deliberately an owned, compact value rather than an ECS
//! query handle. That gives later extraction stages a safe unit to batch or
//! evaluate in parallel without retaining component-storage locks. Missing
//! or not-yet-computed bounds remain conservative: they are treated as
//! raster-visible, and the existing TLAS policy decides ray membership.

use byroredux_core::ecs::WorldBound;
use byroredux_core::math::Vec3;

use super::camera::FrustumPlanes;
use super::static_meshes::{tlas_exclusion, TlasExclusion};

/// Spatial and semantic facts needed to decide a draw's frame visibility.
/// All fields are canonical renderer inputs and contain no per-game tags.
#[derive(Clone, Copy)]
pub(super) struct VisibilityCandidate {
    pub world_bound: Option<WorldBound>,
    pub is_lod: bool,
    pub is_decal_mesh: bool,
    pub material_kind: u32,
}

/// Test the camera-raster predicate before fetching ray-only metadata.
///
/// Kept separate from [`ray_exclusion`] so a frustum-rejected draw can avoid
/// the additional ECS lookups needed only to decide TLAS membership.
#[inline]
pub(super) fn raster_visible(
    world_bound: Option<&WorldBound>,
    is_cover_template: bool,
    frustum: &FrustumPlanes,
    disable_frustum_culling: bool,
) -> bool {
    disable_frustum_culling
        || is_cover_template
        || match world_bound {
            Some(bound) if bound.radius > 0.0 => {
                frustum.contains_sphere(bound.center, bound.radius)
            }
            _ => true,
        }
}

/// Evaluate TLAS membership from an owned candidate snapshot.
///
/// This deliberately does not use camera-frustum rejection: off-frustum
/// candidates may still occlude or reflect for visible pixels. Later
/// occlusion work can refine the candidate without coupling that policy to a
/// specific game's record format.
#[inline]
pub(super) fn ray_exclusion(
    candidate: VisibilityCandidate,
    camera_position: Vec3,
) -> Option<TlasExclusion> {
    tlas_exclusion(
        candidate.is_lod,
        candidate.world_bound.as_ref(),
        camera_position,
        candidate.is_decal_mesh,
        candidate.material_kind,
    )
}
