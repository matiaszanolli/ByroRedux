use std::num::NonZeroU64;

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

    /// Smallest envelope containing both `self` and `other`.
    pub fn union(self, other: Self) -> Self {
        Self {
            min: Vec3::from_array(self.min)
                .min(Vec3::from_array(other.min))
                .to_array(),
            max: Vec3::from_array(self.max)
                .max(Vec3::from_array(other.max))
                .to_array(),
        }
    }

    /// The same envelope moved by `offset`.
    pub fn translated(self, offset: [f32; 3]) -> Self {
        let offset = Vec3::from_array(offset);
        Self {
            min: (Vec3::from_array(self.min) + offset).to_array(),
            max: (Vec3::from_array(self.max) + offset).to_array(),
        }
    }
}

/// Translation that stands `incoming` on the Studio floor (`y = 0`), centred
/// on the room's depth axis, beside everything already `occupied`.
///
/// The gallery grows along +X: the first asset is centred on the origin and
/// each later one starts a proportional gap past the current right edge. The
/// asset's own lowest point lands exactly on the floor so contact shadows and
/// ambient occlusion are evaluated against real contact, not a floating gap.
pub fn gallery_offset(occupied: Option<AssetBounds>, incoming: AssetBounds) -> [f32; 3] {
    let center = incoming.center();
    let x = match occupied {
        None => -center.x,
        Some(occupied) => {
            let span = occupied
                .size()
                .max_element()
                .max(incoming.size().max_element());
            occupied.max[0] + (span * 0.15).max(0.01) - incoming.min[0]
        }
    };
    [x, -incoming.min[1], -center.z]
}

/// One page of a host asset listing filtered by a case-insensitive query.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogPage {
    pub matches: Vec<String>,
    pub total_matches: usize,
}

/// Keep the paths containing every whitespace-separated term of `query`
/// (case-insensitive, `/` and `\` interchangeable), capped at `limit`.
/// `total_matches` still counts every hit so a UI can say how many it hid.
pub fn filter_catalog(paths: &[String], query: &str, limit: usize) -> CatalogPage {
    let terms: Vec<String> = query
        .split_whitespace()
        .map(|term| term.to_ascii_lowercase().replace('/', "\\"))
        .collect();
    let mut page = CatalogPage::default();
    for path in paths {
        let folded = path.to_ascii_lowercase().replace('/', "\\");
        if terms.iter().all(|term| folded.contains(term.as_str())) {
            if page.matches.len() < limit {
                page.matches.push(path.clone());
            }
            page.total_matches += 1;
        }
    }
    page
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
    /// The floor sits exactly at the lowest point of `bounds`: an asset
    /// resting on the floor must actually touch it, or contact shadows and
    /// AO are judged against a gap no game ever shows.
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
        let floor_y = min.y;
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

/// Stable, document-local identity of one asset placed in the Studio room.
/// Like [`ObjectId`] it is unrelated to host ECS IDs, and zero is reserved.
#[derive(Debug, Clone, Copy, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AssetId(NonZeroU64);

impl AssetId {
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// One installed title a host can import assets from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogGame {
    /// Host profile key, passed back in [`StudioCommand::BrowseCatalog`] and
    /// [`StudioCommand::AddAsset`].
    pub key: String,
    pub name: String,
}

/// The host's current asset browser state.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CatalogView {
    pub games: Vec<CatalogGame>,
    /// Key of the game being browsed, once one has been opened.
    pub game: Option<String>,
    pub filter: String,
    pub page: CatalogPage,
    /// Importable assets in the browsed game before filtering.
    pub total_assets: usize,
}

/// One asset placed in the room, with the objects it contributed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacedAssetSnapshot {
    pub id: AssetId,
    pub game: String,
    pub path: String,
    pub object_count: usize,
}

/// UI-facing immutable document projection.
#[derive(Debug, Clone, PartialEq)]
pub struct StudioSnapshot {
    pub source_label: String,
    pub revision: u64,
    pub selected: Option<ObjectId>,
    pub objects: Vec<ObjectSnapshot>,
    pub assets: Vec<PlacedAssetSnapshot>,
    pub catalog: CatalogView,
    /// Outcome of the most recent gallery operation (errors included).
    pub status: Option<String>,
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
    /// Open `game`'s archives (first use only) and list the assets matching
    /// `filter`.
    BrowseCatalog {
        game: String,
        filter: String,
    },
    /// Import one asset beside the ones already placed; the room refits.
    AddAsset {
        game: String,
        path: String,
    },
    RemoveAsset(AssetId),
    ClearAssets,
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
        assert_eq!(fit.floor_y, bounds.min[1], "the asset must touch the floor");
        assert!(fit.floor_y + fit.height > bounds.max[1]);
        assert!(fit.half_width > 4.0);
        assert!(fit.camera_position[2] > fit.center[2] + fit.half_depth);
    }

    #[test]
    fn gallery_grounds_the_first_asset_and_lines_up_the_rest_along_x() {
        let crate_box = AssetBounds {
            min: [10.0, -5.0, 2.0],
            max: [30.0, 15.0, 6.0],
        };
        let first = crate_box.translated(gallery_offset(None, crate_box));
        assert_eq!(first.min[1], 0.0, "lowest vertex sits on the floor");
        assert_eq!(first.center().x, 0.0);
        assert_eq!(first.center().z, 0.0);

        let bottle = AssetBounds {
            min: [-1.0, 3.0, -1.0],
            max: [1.0, 9.0, 1.0],
        };
        let second = bottle.translated(gallery_offset(Some(first), bottle));
        assert_eq!(second.min[1], 0.0);
        assert!(
            second.min[0] > first.max[0],
            "a later asset never overlaps an earlier one"
        );
        assert_eq!(second.center().z, 0.0);
    }

    #[test]
    fn union_contains_both_envelopes() {
        let a = AssetBounds {
            min: [0.0, 0.0, 0.0],
            max: [1.0, 1.0, 1.0],
        };
        let b = AssetBounds {
            min: [-2.0, 0.5, 0.25],
            max: [0.5, 3.0, 0.75],
        };
        let union = a.union(b);
        assert_eq!(union.min, [-2.0, 0.0, 0.0]);
        assert_eq!(union.max, [1.0, 3.0, 1.0]);
    }

    #[test]
    fn catalog_filter_requires_every_term_and_counts_hidden_hits() {
        let paths: Vec<String> = [
            r"meshes\clutter\bottle01.nif",
            r"meshes\clutter\bottle02.nif",
            r"meshes\armor\iron\cuirass.nif",
            r"meshes\clutter\plate01.nif",
        ]
        .map(str::to_owned)
        .to_vec();
        let page = filter_catalog(&paths, "CLUTTER bottle", 1);
        assert_eq!(
            page.matches,
            vec![r"meshes\clutter\bottle01.nif".to_owned()]
        );
        assert_eq!(page.total_matches, 2);
        assert_eq!(
            filter_catalog(&paths, "clutter/plate", 10).matches,
            vec![r"meshes\clutter\plate01.nif".to_owned()],
            "forward slashes match archive backslashes"
        );
        assert_eq!(filter_catalog(&paths, "", 10).total_matches, paths.len());
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
