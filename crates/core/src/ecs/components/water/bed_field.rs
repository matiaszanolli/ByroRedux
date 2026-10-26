//! The bed under a water plane: how deep the water is, and which way the
//! shore lies.
//!
//! WATAL's two motion features — waves that feel the shore and a river whose
//! current follows its channel — both need the same two numbers at a point:
//! the depth of water over the bed, and the horizontal direction in which the
//! bed rises (toward the nearest shore). The LAND heightfield under the plane
//! is the only authored description of the bed, so it is sampled once into a
//! [`WaterBedField`] that the renderer bakes into the water mesh's vertices
//! (`cell_loader/water.rs`) and that physics reads for the same position. One
//! sampler, so the visible wave and the buoyancy crest cannot disagree about
//! how deep the water is.

use std::sync::Arc;

use crate::ecs::sparse_set::SparseSetStorage;
use crate::ecs::storage::Component;

/// Water depth and shoreward direction at one point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WaterBedSample {
    /// Surface height minus bed height, world units, never negative (a point
    /// where the bed is above the surface reads 0).
    pub depth: f32,
    /// Unit horizontal direction in which the bed rises (the depth
    /// decreases fastest) — toward the shore. `[0, 0]` where the bed is flat.
    pub uphill: [f32; 2],
}

/// A regular grid of water depths under one plane, in the renderer's
/// horizontal X/Z axes. Immutable once built; clones share the samples.
#[derive(Debug, Clone)]
pub struct WaterBedField {
    /// Renderer X/Z of the grid's minimum corner (sample `[0][0]`).
    origin: [f32; 2],
    /// World units between neighbouring samples.
    spacing: f32,
    /// Samples per axis (at least 2).
    side: usize,
    /// `side * side` signed depths (surface minus bed, negative where the bed
    /// is above the surface), row-major with rows running along +Z. Kept
    /// signed so interpolation crosses the waterline correctly; `sample`
    /// clamps the interpolated result at zero.
    depth: Arc<[f32]>,
}

impl Component for WaterBedField {
    type Storage = SparseSetStorage<Self>;
}

/// Depths whose gradient is below this (world units of depth per world unit
/// of distance) read as a flat bed with no shoreward direction.
const FLAT_BED_GRADIENT: f32 = 1.0e-4;

impl WaterBedField {
    /// Build a field. `depth` is the signed surface-minus-bed depth,
    /// row-major, `side * side` long, rows along +Z. Returns `None` for an unusable grid (too small, wrong length, a
    /// non-finite spacing or sample).
    pub fn new(origin: [f32; 2], spacing: f32, side: usize, depth: Vec<f32>) -> Option<Self> {
        if side < 2
            || depth.len() != side * side
            || !spacing.is_finite()
            || spacing <= 0.0
            || !origin.iter().all(|v| v.is_finite())
            || !depth.iter().all(|d| d.is_finite())
        {
            return None;
        }
        Some(Self {
            origin,
            spacing,
            side,
            depth: depth.into(),
        })
    }

    /// Depth and shoreward direction at `(x, z)`, bilinearly interpolated;
    /// `None` outside the grid.
    pub fn sample(&self, x: f32, z: f32) -> Option<WaterBedSample> {
        let fx = (x - self.origin[0]) / self.spacing;
        let fz = (z - self.origin[1]) / self.spacing;
        let max = (self.side - 1) as f32;
        if !(fx.is_finite() && fz.is_finite()) || fx < 0.0 || fz < 0.0 || fx > max || fz > max {
            return None;
        }
        // The last row/column belongs to the previous cell so every query
        // inside the grid has four corners.
        let i = (fx.floor() as usize).min(self.side - 2);
        let j = (fz.floor() as usize).min(self.side - 2);
        let (tx, tz) = (fx - i as f32, fz - j as f32);
        let at = |jj: usize, ii: usize| self.depth[jj * self.side + ii];
        let (d00, d10, d01, d11) = (at(j, i), at(j, i + 1), at(j + 1, i), at(j + 1, i + 1));
        let depth = ((d00 * (1.0 - tx) + d10 * tx) * (1.0 - tz)
            + (d01 * (1.0 - tx) + d11 * tx) * tz)
            .max(0.0);
        let grad_x = ((d10 - d00) * (1.0 - tz) + (d11 - d01) * tz) / self.spacing;
        let grad_z = ((d01 - d00) * (1.0 - tx) + (d11 - d10) * tx) / self.spacing;
        let len = (grad_x * grad_x + grad_z * grad_z).sqrt();
        let uphill = if len > FLAT_BED_GRADIENT {
            // Depth increases away from the shore, so the shore is the way
            // the depth falls.
            [-grad_x / len, -grad_z / len]
        } else {
            [0.0, 0.0]
        };
        Some(WaterBedSample { depth, uphill })
    }

    /// World units between samples.
    pub fn spacing(&self) -> f32 {
        self.spacing
    }

    /// Samples per axis.
    pub fn side(&self) -> usize {
        self.side
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A beach: depth grows with +X from 0 at x=0 to 300 at x=300 (4 samples,
    /// spacing 100), constant in Z. The shore is toward -X.
    fn beach() -> WaterBedField {
        let mut depth = Vec::new();
        for _z in 0..4 {
            for x in 0..4 {
                depth.push(x as f32 * 100.0);
            }
        }
        WaterBedField::new([0.0, 0.0], 100.0, 4, depth).expect("valid grid")
    }

    #[test]
    fn depth_is_bilinear_and_the_shore_is_where_it_falls() {
        let field = beach();
        let s = field.sample(150.0, 50.0).expect("inside");
        assert!((s.depth - 150.0).abs() < 1e-3, "{s:?}");
        assert!((s.uphill[0] + 1.0).abs() < 1e-5 && s.uphill[1].abs() < 1e-5, "{s:?}");
    }

    #[test]
    fn the_grid_edge_is_inside_and_beyond_it_is_not() {
        let field = beach();
        assert!(field.sample(0.0, 0.0).is_some());
        assert!(field.sample(300.0, 300.0).is_some(), "the far corner is a sample");
        assert!(field.sample(-0.5, 10.0).is_none());
        assert!(field.sample(300.5, 10.0).is_none());
        assert!(field.sample(f32::NAN, 10.0).is_none());
    }

    #[test]
    fn a_flat_bed_has_no_shoreward_direction() {
        let flat = WaterBedField::new([0.0, 0.0], 100.0, 2, vec![50.0; 4]).unwrap();
        let s = flat.sample(40.0, 40.0).unwrap();
        assert_eq!(s.depth, 50.0);
        assert_eq!(s.uphill, [0.0, 0.0]);
    }

    #[test]
    fn unusable_grids_are_refused() {
        assert!(WaterBedField::new([0.0, 0.0], 100.0, 1, vec![0.0]).is_none());
        assert!(WaterBedField::new([0.0, 0.0], 100.0, 2, vec![0.0; 3]).is_none());
        assert!(WaterBedField::new([0.0, 0.0], 0.0, 2, vec![0.0; 4]).is_none());
        assert!(WaterBedField::new([0.0, 0.0], 100.0, 2, vec![f32::NAN; 4]).is_none());
        // A bed above the surface is a dry sample, never negative depth.
        let dry = WaterBedField::new([0.0, 0.0], 1.0, 2, vec![-5.0; 4]).unwrap();
        assert_eq!(dry.sample(0.5, 0.5).unwrap().depth, 0.0);
    }

    /// Interpolation crosses the waterline on the signed samples: a bed rising
    /// from 40 below to 10 above the surface is wet for the first 80% of the
    /// span, and its depth falls to zero exactly there, not at the dry sample.
    #[test]
    fn depth_reaches_zero_at_the_waterline_between_samples() {
        let field = WaterBedField::new([0.0, 0.0], 100.0, 2, vec![40.0, -10.0, 40.0, -10.0]).unwrap();
        assert!((field.sample(0.0, 0.0).unwrap().depth - 40.0).abs() < 1e-4);
        assert!(field.sample(80.0, 0.0).unwrap().depth.abs() < 1e-4, "waterline at 80%");
        assert_eq!(field.sample(90.0, 0.0).unwrap().depth, 0.0);
        // The slope is still read across the clamp, so the shore direction is
        // available on the dry side's edge.
        assert!(field.sample(90.0, 0.0).unwrap().uphill[0] > 0.9);
    }
}
