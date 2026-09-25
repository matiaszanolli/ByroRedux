//! Immutable XZ broad phase for placed water triangles (#4797).
//!
//! Use a compact cell-to-triangle list. Very large triangles live in a
//! separate list so an overlapping mesh cannot multiply its index storage
//! by the entire grid size. Exact surface/layer selection stays in water.rs.

use super::WATER_SURFACE_EDGE_EPSILON;

type Triangle = [[f32; 3]; 3];

#[derive(Debug)]
pub(super) struct SurfaceGrid {
    min: [f64; 2],
    max: [f64; 2],
    inverse_cell_size: [f64; 2],
    dimensions: [usize; 2],
    offsets: Box<[usize]>,
    indices: Box<[usize]>,
    large: Box<[usize]>,
}

fn bounds(triangle: &Triangle) -> Option<([f64; 2], [f64; 2])> {
    if !triangle.iter().flatten().all(|v| v.is_finite()) {
        return None;
    }
    let mut min = [f64::INFINITY; 2];
    let mut max = [f64::NEG_INFINITY; 2];
    for point in triangle {
        for (axis, component) in [0, 2].into_iter().enumerate() {
            min[axis] = min[axis].min(f64::from(point[component]));
            max[axis] = max[axis].max(f64::from(point[component]));
        }
    }
    for axis in 0..2 {
        // With three barycentric weights >= -epsilon, the furthest point
        // can extend 2*epsilon times the triangle extent past its AABB.
        // Include roundoff at the world-coordinate scale as well. Compute
        // the bounds in f64 so padding is not lost at large placements.
        let padding = 2.0 * f64::from(WATER_SURFACE_EDGE_EPSILON) * (max[axis] - min[axis])
            + 8.0 * f64::from(f32::EPSILON) * min[axis].abs().max(max[axis].abs()).max(1.0);
        min[axis] -= padding;
        max[axis] += padding;
    }
    Some((min, max))
}

impl SurfaceGrid {
    pub(super) fn new(triangles: &[Triangle]) -> Option<Self> {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for triangle in triangles {
            if let Some((lo, hi)) = bounds(triangle) {
                for axis in 0..2 {
                    min[axis] = min[axis].min(lo[axis]);
                    max[axis] = max[axis].max(hi[axis]);
                }
            }
        }
        if !min.iter().all(|v| v.is_finite()) {
            return None;
        }
        const MAX_SIDE: usize = 32;
        const MAX_CELLS_PER_TRIANGLE: usize = 16;
        let target_cells = (triangles.len() / 8).clamp(1, MAX_SIDE * MAX_SIDE);
        let width = max[0] - min[0];
        let depth = max[1] - min[1];
        let x_cells =
            ((target_cells as f64 * width / depth).sqrt().ceil() as usize).clamp(1, MAX_SIDE);
        let z_cells = target_cells.div_ceil(x_cells).clamp(1, MAX_SIDE);
        let dimensions = [x_cells, z_cells];
        let inverse_cell_size = [x_cells as f64 / width, z_cells as f64 / depth];
        let mut grid = Self {
            min,
            max,
            inverse_cell_size,
            dimensions,
            offsets: Box::default(),
            indices: Box::default(),
            large: Box::default(),
        };
        let mut cells = vec![Vec::new(); x_cells * z_cells];
        let mut large = Vec::new();
        for (index, triangle) in triangles.iter().enumerate() {
            let Some((lo, hi)) = bounds(triangle) else {
                continue;
            };
            let first = grid.cell_coords(lo);
            let last = grid.cell_coords(hi);
            let covered = (last[0] - first[0] + 1) * (last[1] - first[1] + 1);
            if covered > MAX_CELLS_PER_TRIANGLE {
                large.push(index);
                continue;
            }
            for z in first[1]..=last[1] {
                for x in first[0]..=last[0] {
                    cells[z * x_cells + x].push(index);
                }
            }
        }
        let mut offsets = Vec::with_capacity(cells.len() + 1);
        let mut indices = Vec::new();
        offsets.push(0);
        for cell in cells {
            indices.extend(cell);
            offsets.push(indices.len());
        }
        grid.offsets = offsets.into_boxed_slice();
        grid.indices = indices.into_boxed_slice();
        grid.large = large.into_boxed_slice();
        Some(grid)
    }

    fn cell_coords(&self, point: [f64; 2]) -> [usize; 2] {
        std::array::from_fn(|axis| {
            (((point[axis] - self.min[axis]) * self.inverse_cell_size[axis]) as usize)
                .min(self.dimensions[axis] - 1)
        })
    }

    pub(super) fn candidates(&self, x: f32, z: f32) -> Option<&[usize]> {
        let point = [f64::from(x), f64::from(z)];
        if (0..2).any(|axis| point[axis] < self.min[axis] || point[axis] > self.max[axis]) {
            return None;
        }
        let [x, z] = self.cell_coords(point);
        let cell = z * self.dimensions[0] + x;
        Some(&self.indices[self.offsets[cell]..self.offsets[cell + 1]])
    }

    pub(super) fn large_triangles(&self) -> &[usize] {
        &self.large
    }
}
