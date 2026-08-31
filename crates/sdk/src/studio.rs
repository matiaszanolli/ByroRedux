use byroredux_core::math::Vec3;

use crate::identity::ObjectId;

/// A finite world-space sphere contributed by a renderable asset object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoundSphere {
    pub center: [f32; 3],
    pub radius: f32,
}

/// World-space asset envelope used by Studio hosts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AssetBounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}

impl AssetBounds {
    pub fn from_spheres(spheres: impl IntoIterator<Item = BoundSphere>) -> Option<Self> {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        let mut found = false;
        for sphere in spheres {
            let center = Vec3::from_array(sphere.center);
            let radius = sphere.radius.abs();
            if !center.is_finite() || !radius.is_finite() {
                continue;
            }
            let extent = Vec3::splat(radius);
            min = min.min(center - extent);
            max = max.max(center + extent);
            found = true;
        }
        found.then_some(Self {
            min: min.to_array(),
            max: max.to_array(),
        })
    }

    pub fn center(self) -> Vec3 {
        (Vec3::from_array(self.min) + Vec3::from_array(self.max)) * 0.5
    }

    pub fn size(self) -> Vec3 {
        Vec3::from_array(self.max) - Vec3::from_array(self.min)
    }
}

/// Automatically sized open-front Cornell room and initial camera pose.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CornellFit {
    pub center: [f32; 3],
    pub half_width: f32,
    pub half_depth: f32,
    pub floor_y: f32,
    pub height: f32,
    pub wall_thickness: f32,
    pub camera_position: [f32; 3],
    pub camera_target: [f32; 3],
}

impl CornellFit {
    /// Fit a room with proportional breathing room and conservative minima.
    pub fn around(bounds: AssetBounds) -> Self {
        let supplied_min = Vec3::from_array(bounds.min);
        let supplied_max = Vec3::from_array(bounds.max);
        let (min, max) = if supplied_min.is_finite() && supplied_max.is_finite() {
            (
                supplied_min.min(supplied_max),
                supplied_min.max(supplied_max),
            )
        } else {
            (Vec3::splat(-0.5), Vec3::splat(0.5))
        };
        let center = (min + max) * 0.5;
        let size = (max - min).max(Vec3::splat(0.01));
        let span = size.max_element().max(1.0);
        let horizontal_padding = (span * 0.25).max(0.5);
        let vertical_padding = (span * 0.18).max(0.35);
        let half_width = (size.x * 0.5 + horizontal_padding).max(1.5);
        let half_depth = (size.z * 0.5 + horizontal_padding).max(1.5);
        let floor_y = min.y - vertical_padding;
        let height = (size.y + vertical_padding * 2.0).max(2.5);
        let target = Vec3::new(center.x, center.y, center.z);
        let camera_distance = (half_width.max(half_depth) * 2.35).max(span * 1.6);
        let camera = Vec3::new(center.x, center.y, center.z + camera_distance);
        Self {
            center: center.to_array(),
            half_width,
            half_depth,
            floor_y,
            height,
            wall_thickness: (span * 0.006).clamp(0.025, 4.0),
            camera_position: camera.to_array(),
            camera_target: target.to_array(),
        }
    }
}

/// Source identity retained by the Studio document independently of import IO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetSource {
    pub label: String,
}

/// UI-facing immutable document projection.
#[derive(Debug, Clone, PartialEq)]
pub struct StudioSnapshot {
    pub source_label: String,
    pub revision: u64,
    pub selected: Option<ObjectId>,
    pub objects: Vec<ObjectSnapshot>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectSnapshot {
    pub id: ObjectId,
    pub name: String,
    pub transform: TransformValue,
    pub material: Option<MaterialValue>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TransformValue {
    pub translation: [f32; 3],
    /// Intrinsic XYZ Euler angles in degrees, for human-facing editing.
    pub rotation_degrees: [f32; 3],
    pub scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialValue {
    pub diffuse_color: [f32; 3],
    pub metalness: f32,
    pub roughness: f32,
    pub alpha: f32,
    pub ior: f32,
}

/// Typed mutation protocol shared by GUI, CLI, automation, and trusted tools.
#[derive(Debug, Clone, PartialEq)]
pub enum StudioCommand {
    Select(Option<ObjectId>),
    PickFromView,
    SetTransform {
        object: ObjectId,
        value: TransformValue,
    },
    ResetTransform(ObjectId),
    SetMaterial {
        object: ObjectId,
        value: MaterialValue,
    },
    FrameSelection(ObjectId),
}

/// Return the nearest positive ray/sphere hit. Invalid spheres are ignored.
pub fn pick_spheres(
    origin: [f32; 3],
    direction: [f32; 3],
    spheres: impl IntoIterator<Item = (ObjectId, BoundSphere)>,
) -> Option<ObjectId> {
    let origin = Vec3::from_array(origin);
    let direction = Vec3::from_array(direction).normalize_or_zero();
    if !origin.is_finite() || direction == Vec3::ZERO {
        return None;
    }
    spheres
        .into_iter()
        .filter_map(|(object, sphere)| {
            let center = Vec3::from_array(sphere.center);
            let radius = sphere.radius.abs();
            if !center.is_finite() || !radius.is_finite() {
                return None;
            }
            let offset = center - origin;
            let projected = offset.dot(direction);
            let perpendicular_sq = offset.length_squared() - projected * projected;
            let radius_sq = radius * radius;
            if perpendicular_sq > radius_sq {
                return None;
            }
            let half_chord = (radius_sq - perpendicular_sq).sqrt();
            let near = projected - half_chord;
            let far = projected + half_chord;
            let distance = if near >= 0.0 { near } else { far };
            (distance >= 0.0).then_some((object, distance))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(object, _)| object)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounds_skip_non_finite_content() {
        let bounds = AssetBounds::from_spheres([
            BoundSphere {
                center: [1.0, 2.0, 3.0],
                radius: 2.0,
            },
            BoundSphere {
                center: [f32::NAN, 0.0, 0.0],
                radius: 1.0,
            },
        ])
        .unwrap();
        assert_eq!(bounds.min, [-1.0, 0.0, 1.0]);
        assert_eq!(bounds.max, [3.0, 4.0, 5.0]);
    }

    #[test]
    fn cornell_fit_contains_asset_and_places_camera_at_open_front() {
        let bounds = AssetBounds {
            min: [-2.0, 4.0, -1.0],
            max: [6.0, 10.0, 3.0],
        };
        let fit = CornellFit::around(bounds);
        assert!(fit.floor_y < bounds.min[1]);
        assert!(fit.floor_y + fit.height > bounds.max[1]);
        assert!(fit.half_width > 4.0);
        assert!(fit.camera_position[2] > fit.center[2] + fit.half_depth);
    }

    #[test]
    fn picking_returns_nearest_forward_sphere() {
        let hit = pick_spheres(
            [0.0; 3],
            [0.0, 0.0, -1.0],
            [
                (
                    ObjectId::new(9).unwrap(),
                    BoundSphere {
                        center: [0.0, 0.0, -8.0],
                        radius: 1.0,
                    },
                ),
                (
                    ObjectId::new(4).unwrap(),
                    BoundSphere {
                        center: [0.0, 0.0, -3.0],
                        radius: 0.5,
                    },
                ),
                (
                    ObjectId::new(2).unwrap(),
                    BoundSphere {
                        center: [0.0, 0.0, 2.0],
                        radius: 1.0,
                    },
                ),
            ],
        );
        assert_eq!(hit, ObjectId::new(4));
    }
}
