//! Bind-space buckets for seam neighbor lookups.

use byroredux_core::math::Vec3;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub(super) struct VertexGrid {
    buckets: HashMap<[i64; 3], Vec<usize>>,
    width: f64,
}

impl VertexGrid {
    pub(super) fn new(positions: &[Vec3], radius: f32) -> Self {
        let mut grid = Self {
            buckets: HashMap::new(),
            width: f64::from(radius),
        };
        for (index, &position) in positions.iter().enumerate() {
            if position.is_finite() {
                grid.buckets
                    .entry(grid.cell(position))
                    .or_default()
                    .push(index);
            }
        }
        grid
    }

    fn cell(&self, position: Vec3) -> [i64; 3] {
        position
            .to_array()
            .map(|p| (f64::from(p) / self.width).floor() as i64)
    }

    pub(super) fn candidates(&self, position: Vec3, out: &mut Vec<usize>) {
        out.clear();
        if !position.is_finite() {
            return;
        }
        let cell = self.cell(position);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for dz in -1..=1 {
                    let (Some(x), Some(y), Some(z)) = (
                        cell[0].checked_add(dx),
                        cell[1].checked_add(dy),
                        cell[2].checked_add(dz),
                    ) else {
                        continue;
                    };
                    if let Some(indices) = self.buckets.get(&[x, y, z]) {
                        out.extend_from_slice(indices);
                    }
                }
            }
        }
        // Preserve the original source-order accumulation and tie behavior.
        out.sort_unstable();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidates_cover_radius_neighbors_at_negative_coordinates_and_bucket_edges() {
        let positions: Vec<_> = (-100..100)
            .flat_map(|x| [-2.5, 0.0, 2.5].map(|y| Vec3::new(x as f32 * 1.25, y, 0.0)))
            .collect();
        let grid = VertexGrid::new(&positions, 2.5);
        let mut candidates = Vec::new();
        for x in -110..110 {
            let query = Vec3::new(x as f32 * 1.25, -0.01, 0.0);
            grid.candidates(query, &mut candidates);
            let expected: Vec<_> = positions
                .iter()
                .enumerate()
                .filter(|(_, p)| (**p - query).length_squared() <= 6.25)
                .map(|(i, _)| i)
                .collect();
            let actual: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|&i| (positions[i] - query).length_squared() <= 6.25)
                .collect();
            assert_eq!(actual, expected);
            assert!(candidates.len() < positions.len() / 10);
        }
        grid.candidates(Vec3::splat(f32::NAN), &mut candidates);
        assert!(candidates.is_empty());
    }
}
