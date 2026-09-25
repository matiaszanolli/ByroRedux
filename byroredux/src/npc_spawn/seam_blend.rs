//! Spawn-time skin seam blending for runtime-assembled legacy NPCs
//! (Oblivion / Fallout 3 / New Vegas).
//!
//! Those games build an actor from separate skinned NIFs — head, body or
//! outfit, hands — whose cuts are visible in the shipped game: the pieces
//! are not welded on clothed actors (measured in bind pose on FO3: a hand's
//! wrist edge sits 0.4–2.4 units from the outfit's arm skin, the head's neck
//! edge ~0.4–1.5 units from it; only the naked `upperbody.nif` welds
//! exactly), each piece has its own skin texture, and each carries its own
//! normals. Skyrim+ hides this with Creation-Kit-baked per-NPC FaceGen
//! heads and tint textures, which these games do not ship.
//!
//! This pass improves on the legacy look at the one point where every piece
//! is known — actor spawn — and only on the part being spawned (head,
//! hands), so the shared body / outfit imports stay cached and untouched:
//!
//! - **Match.** The part's open-edge vertices, in skeleton bind space, are
//!   matched to *skin* vertices on the actor's other body / outfit meshes
//!   within [`SEAM_MATCH_RADIUS`]. Skin is recognised by its texture living
//!   under `textures\characters\` (all three games); armour and clothing
//!   live elsewhere, so collars and sleeves are never blended into.
//! - **Normals.** Each matched edge vertex takes the neighbours' mean normal,
//!   so lighting is continuous across the cut.
//! - **Tone.** Both textures are sampled around the matched UVs; the
//!   neighbour / own ratio (smoothed along the seam) is baked into the
//!   part's vertex colours — which the lit path multiplies into albedo —
//!   fading back to 1 over [`SEAM_FADE_DISTANCE`], so both sides meet at the
//!   same skin tone while the textures stay as authored.
//!
//! [`SEAM_MATCH_RADIUS`] and [`SEAM_FADE_DISTANCE`] are engine tuning, set by
//! eye on FO3 Moriarty's Saloon; the legacy games have no equivalent value.

use byroredux_core::math::{Mat4, Vec3};
use byroredux_nif::import::ImportedMesh;
use std::collections::HashMap;

mod vertex_grid;
use vertex_grid::VertexGrid;

/// Bind-space distance within which a neighbour skin vertex counts as the
/// other side of an edge vertex's cut. Sized to the widest measured FO3 gap:
/// a wrist edge sits 2.0–2.4 units from the sleeve's skin edge on
/// `wastelandclothing01\outfitm.nif` (0.39–0.61 on the Pip-Boy arm, exact on
/// the naked `upperbody.nif`).
pub(crate) const SEAM_MATCH_RADIUS: f32 = 2.5;
/// Bind-space distance over which the tone correction fades back to 1.
pub(crate) const SEAM_FADE_DISTANCE: f32 = 4.0;
/// Radius along the seam over which per-vertex tone ratios are averaged, so
/// texture detail at a single sample point cannot blotch the correction.
const SEAM_SMOOTH_RADIUS: f32 = 3.0;
/// Per-channel clamp on the tone ratio — a guard against a mis-matched
/// sample (a dark eyebrow texel, a tattoo), not a tuning value.
const TONE_RATIO_RANGE: (f32, f32) = (0.5, 2.0);
/// Mip width the tone samples are taken at.
pub(crate) const TONE_SAMPLE_TEXTURE_WIDTH: u32 = 256;

/// Whether a diffuse texture path is actor skin in Oblivion / FO3 / FNV.
pub(crate) fn is_skin_texture(path: &str) -> bool {
    let lower = path.to_ascii_lowercase().replace('/', "\\");
    let lower = lower.strip_prefix("textures\\").unwrap_or(&lower);
    lower.starts_with("characters\\")
}

/// One neighbouring skin mesh of the actor, in skeleton bind space.
#[derive(Debug, Clone)]
pub(crate) struct NeighborSkin {
    /// Lower-cased NIF path it came from; a part never matches its own file.
    pub source: String,
    pub texture: String,
    pub positions: Vec<Vec3>,
    vertex_grid: VertexGrid,
    pub normals: Vec<Vec3>,
    pub uvs: Vec<[f32; 2]>,
}

/// Everything a part's seam pass needs, resolved before the part loads (the
/// pre-spawn hook runs while the loader holds the world).
#[derive(Debug, Clone, Default)]
pub(crate) struct SeamContext {
    /// Skeleton bone bind transforms relative to the placement root, keyed
    /// by lower-cased bone name.
    pub bone_binds: HashMap<String, Mat4>,
    pub neighbors: Vec<NeighborSkin>,
    /// Resolved diffuse texture per `(lower-cased NIF path, mesh index)`.
    pub textures: HashMap<(String, usize), String>,
}

mod tone;
pub(crate) use tone::ToneSampler;

/// Per-vertex bind-space skinning matrix and position of a skinned mesh, or
/// `None` when the mesh is unskinned or its skin data is incomplete.
pub(crate) fn bind_space(
    mesh: &ImportedMesh,
    bone_binds: &HashMap<String, Mat4>,
) -> Option<(Vec<Vec3>, Vec<Mat4>)> {
    let skin = mesh.skin.as_ref()?;
    if skin.vertex_bone_indices.len() != mesh.positions.len()
        || skin.vertex_bone_weights.len() != mesh.positions.len()
    {
        return None;
    }
    // palette[i] = bone_bind × bind_inverse — the renderer's own skinning
    // composition (`SkinnedMesh::compute_palette_into`) at bind pose.
    let palette: Vec<Option<Mat4>> = skin
        .bones
        .iter()
        .map(|bone| {
            let bind = bone_binds.get(&bone.name.to_ascii_lowercase())?;
            Some(*bind * Mat4::from_cols_array_2d(&bone.bind_inverse))
        })
        .collect();
    let mut positions = Vec::with_capacity(mesh.positions.len());
    let mut matrices = Vec::with_capacity(mesh.positions.len());
    for (i, position) in mesh.positions.iter().enumerate() {
        let mut matrix = Mat4::ZERO;
        let mut total = 0.0;
        for k in 0..4 {
            let weight = skin.vertex_bone_weights[i][k];
            if weight <= 0.0 {
                continue;
            }
            let bone = palette
                .get(skin.vertex_bone_indices[i][k] as usize)
                .copied()
                .flatten()?;
            matrix += bone * weight;
            total += weight;
        }
        if total <= f32::EPSILON {
            return None;
        }
        let matrix = matrix * (1.0 / total);
        positions.push(matrix.transform_point3(Vec3::from_array(*position)));
        matrices.push(matrix);
    }
    Some((positions, matrices))
}

/// A skin neighbour built from one imported mesh, or `None` when it is not
/// usable (unskinned, a dismemberment cap, no normals / UVs).
pub(crate) fn neighbor_from_mesh(
    mesh: &ImportedMesh,
    source: &str,
    texture: &str,
    bone_binds: &HashMap<String, Mat4>,
) -> Option<NeighborSkin> {
    if mesh.is_dismemberment_cap()
        || !is_skin_texture(texture)
        || mesh.normals.len() != mesh.positions.len()
        || mesh.uvs.len() != mesh.positions.len()
    {
        return None;
    }
    let (positions, matrices) = bind_space(mesh, bone_binds)?;
    let normals = mesh
        .normals
        .iter()
        .zip(&matrices)
        .map(|(n, m)| {
            m.inverse()
                .transpose()
                .transform_vector3(Vec3::from_array(*n))
                .normalize_or_zero()
        })
        .collect();
    Some(NeighborSkin {
        source: source.to_ascii_lowercase(),
        texture: texture.to_owned(),
        vertex_grid: VertexGrid::new(&positions, SEAM_MATCH_RADIUS),
        positions,
        normals,
        uvs: mesh.uvs.clone(),
    })
}

/// Vertex indices on the mesh's open edges (edges used by one triangle),
/// with coincident vertices welded so UV / normal splits are not mistaken
/// for boundaries.
fn boundary_vertices(positions: &[Vec3], indices: &[u32]) -> Vec<usize> {
    let quantize = |v: Vec3| [v.x, v.y, v.z].map(|c| (c * 1000.0).round() as i64);
    let mut weld: HashMap<[i64; 3], usize> = HashMap::new();
    let welded: Vec<usize> = positions
        .iter()
        .map(|p| {
            let next = weld.len();
            *weld.entry(quantize(*p)).or_insert(next)
        })
        .collect();
    let mut edges: HashMap<(usize, usize), u32> = HashMap::new();
    for triangle in indices.chunks_exact(3) {
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let (Some(&a), Some(&b)) = (welded.get(a as usize), welded.get(b as usize)) else {
                continue;
            };
            *edges.entry((a.min(b), a.max(b))).or_default() += 1;
        }
    }
    let mut open = vec![false; weld.len()];
    for ((a, b), count) in edges {
        if count == 1 {
            open[a] = true;
            open[b] = true;
        }
    }
    (0..positions.len()).filter(|&i| open[welded[i]]).collect()
}

/// What one part's seam pass changed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SeamStats {
    pub matched_vertices: usize,
    pub toned_vertices: usize,
}

/// Blend `mesh`'s cuts into the actor's neighbouring skin. `own_source` is
/// the part's lower-cased NIF path (its own file is never a neighbour);
/// `own_texture` its resolved diffuse, `None` to blend normals only.
/// `sample` returns the mean colour around a UV of a texture.
pub(crate) fn blend_part_seams(
    mesh: &mut ImportedMesh,
    own_source: &str,
    own_texture: Option<&str>,
    context: &SeamContext,
    sample: &mut dyn FnMut(&str, [f32; 2]) -> Option<[f32; 3]>,
) -> SeamStats {
    let mut stats = SeamStats::default();
    let Some((positions, matrices)) = bind_space(mesh, &context.bone_binds) else {
        log::debug!(
            "seam blend: '{}' mesh {:?} has no bind-space placement (unskinned or unbound bone)",
            own_source,
            mesh.name,
        );
        return stats;
    };
    let own_source = own_source.to_ascii_lowercase();
    let neighbors: Vec<&NeighborSkin> = context
        .neighbors
        .iter()
        .filter(|neighbor| neighbor.source != own_source)
        .collect();
    if neighbors.is_empty() {
        return stats;
    }

    // (vertex, mean neighbour normal, tone ratio if both sides sampled)
    let mut matched: Vec<(usize, Vec3, Option<[f32; 3]>)> = Vec::new();
    let mut candidates = Vec::new();
    for vertex in boundary_vertices(&positions, &mesh.indices) {
        let here = positions[vertex];
        let mut normal = Vec3::ZERO;
        let mut colour = [0.0f32; 3];
        let mut coloured = 0u32;
        let mut hits = 0u32;
        for neighbor in &neighbors {
            neighbor.vertex_grid.candidates(here, &mut candidates);
            for &j in &candidates {
                if (neighbor.positions[j] - here).length_squared() > SEAM_MATCH_RADIUS * SEAM_MATCH_RADIUS {
                    continue;
                }
                hits += 1;
                normal += neighbor.normals[j];
                if let Some(c) = sample(&neighbor.texture, neighbor.uvs[j]) {
                    for i in 0..3 {
                        colour[i] += c[i];
                    }
                    coloured += 1;
                }
            }
        }
        if hits == 0 {
            continue;
        }
        let own_colour = own_texture
            .zip(mesh.uvs.get(vertex).copied())
            .and_then(|(texture, uv)| sample(texture, uv));
        let ratio = own_colour.filter(|_| coloured > 0).map(|own| {
            [0, 1, 2].map(|i| {
                let theirs = colour[i] / coloured as f32;
                (theirs / own[i].max(1.0 / 255.0)).clamp(TONE_RATIO_RANGE.0, TONE_RATIO_RANGE.1)
            })
        });
        matched.push((vertex, normal.normalize_or_zero(), ratio));
    }
    stats.matched_vertices = matched.len();
    if matched.is_empty() {
        return stats;
    }

    // Normals: each matched edge vertex takes the other side's normal,
    // brought back into its own mesh space (normals map by the inverse
    // transpose, so back by the transpose).
    if mesh.normals.len() == mesh.positions.len() {
        for &(vertex, normal, _) in &matched {
            if normal != Vec3::ZERO {
                let local = matrices[vertex]
                    .transpose()
                    .transform_vector3(normal)
                    .normalize_or_zero();
                if local != Vec3::ZERO {
                    mesh.normals[vertex] = local.to_array();
                }
            }
        }
    }

    // Tone: smooth the ratios along the seam, then fade them inward.
    let toned: Vec<(Vec3, [f32; 3])> = matched
        .iter()
        .filter_map(|&(vertex, _, ratio)| ratio.map(|_| vertex))
        .map(|vertex| {
            let here = positions[vertex];
            let mut sum = [0.0f32; 3];
            let mut n = 0.0f32;
            for &(other, _, ratio) in &matched {
                if let Some(r) = ratio {
                    if (positions[other] - here).length() <= SEAM_SMOOTH_RADIUS {
                        for i in 0..3 {
                            sum[i] += r[i];
                        }
                        n += 1.0;
                    }
                }
            }
            (here, sum.map(|s| s / n))
        })
        .collect();
    if toned.is_empty() {
        return stats;
    }
    if mesh.colors.len() != mesh.positions.len() {
        mesh.colors = vec![[1.0; 4]; mesh.positions.len()];
    }
    for (vertex, position) in positions.iter().enumerate() {
        let Some((distance, ratio)) = toned
            .iter()
            .map(|(seam, ratio)| ((*seam - *position).length(), *ratio))
            .min_by(|a, b| a.0.total_cmp(&b.0))
        else {
            continue;
        };
        if distance >= SEAM_FADE_DISTANCE {
            continue;
        }
        let t = distance / SEAM_FADE_DISTANCE;
        let weight = 1.0 - t * t * (3.0 - 2.0 * t);
        let colour = &mut mesh.colors[vertex];
        for i in 0..3 {
            colour[i] *= 1.0 + (ratio[i] - 1.0) * weight;
        }
        stats.toned_vertices += 1;
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_nif::import::{ImportedBone, ImportedSkin};

    /// A skinned strip of two triangles bound 1:1 to `bone` at identity, so
    /// bind space equals mesh space.
    fn strip(positions: Vec<[f32; 3]>, uv_u: f32) -> ImportedMesh {
        let n = positions.len();
        let mut mesh = ImportedMesh::from_geometry(
            positions,
            Vec::new(),
            vec![[0.0, 0.0, 1.0]; n],
            Vec::new(),
            vec![[uv_u, 0.5]; n],
            vec![0, 1, 2, 1, 3, 2],
        );
        mesh.skin = Some(ImportedSkin {
            bones: vec![ImportedBone {
                name: "Bone".into(),
                bind_inverse: Mat4::IDENTITY.to_cols_array_2d(),
                bounding_sphere: [0.0; 4],
            }],
            vertex_bone_indices: vec![[0; 4]; n],
            vertex_bone_weights: vec![[1.0, 0.0, 0.0, 0.0]; n],
            ..Default::default()
        });
        mesh
    }

    fn context_with(neighbor: &ImportedMesh) -> SeamContext {
        let bone_binds = HashMap::from([("bone".to_string(), Mat4::IDENTITY)]);
        let mut n = neighbor_from_mesh(
            neighbor,
            "meshes\\body.nif",
            "Characters\\Female\\UpperBody.dds",
            &bone_binds,
        )
        .expect("skin neighbour");
        // Point the neighbour's normals sideways so the copy is observable.
        n.normals = vec![Vec3::X; n.normals.len()];
        SeamContext {
            bone_binds,
            neighbors: vec![n],
            textures: HashMap::new(),
        }
    }

    #[test]
    fn skin_is_recognised_by_its_characters_texture_root() {
        assert!(is_skin_texture("Characters\\Female\\HeadHuman.dds"));
        assert!(is_skin_texture(
            "textures\\characters\\male\\upperbodymale.dds"
        ));
        assert!(is_skin_texture(
            "Textures/Characters/Imperial/HeadHuman.dds"
        ));
        assert!(!is_skin_texture(
            "textures\\armor\\wastelandclothing01\\outfitf.dds"
        ));
        assert!(!is_skin_texture("textures\\gore\\meatcapgore01.dds"));
    }

    /// The part's cut edge (y = 0) sits 0.4 units from the neighbour's edge —
    /// the measured FO3 overlap — so it matches; its far edge (y = 10) does
    /// not. The matched vertices take the neighbour's normal and a tone
    /// ratio that fades to nothing by `SEAM_FADE_DISTANCE`.
    #[test]
    fn cut_edge_takes_the_neighbours_normal_and_tone_and_fades_inward() {
        let neighbor = strip(
            vec![
                [0.0, -0.4, 0.0],
                [1.0, -0.4, 0.0],
                [0.0, -5.0, 0.0],
                [1.0, -5.0, 0.0],
            ],
            0.9,
        );
        let context = context_with(&neighbor);
        let mut part = strip(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 10.0, 0.0],
                [1.0, 10.0, 0.0],
            ],
            0.1,
        );
        // Own texture samples 0.4 grey, the neighbour's 0.6 grey.
        let mut sample = |texture: &str, _uv: [f32; 2]| {
            Some(if texture.contains("HeadHuman") {
                [0.4; 3]
            } else {
                [0.6; 3]
            })
        };
        let stats = blend_part_seams(
            &mut part,
            "meshes\\head.nif",
            Some("Characters\\Female\\HeadHuman.dds"),
            &context,
            &mut sample,
        );
        assert_eq!(stats.matched_vertices, 2);
        // Seam vertices: neighbour normal, full 1.5× tone.
        assert_eq!(part.normals[0], [1.0, 0.0, 0.0]);
        assert!(
            (part.colors[0][0] - 1.5).abs() < 1.0e-5,
            "{:?}",
            part.colors[0]
        );
        // Far edge: untouched.
        assert_eq!(part.normals[2], [0.0, 0.0, 1.0]);
        assert_eq!(part.colors[2], [1.0; 4]);
    }

    #[test]
    fn own_file_clothing_and_distant_meshes_are_never_neighbours() {
        let neighbor = strip(
            vec![
                [0.0, -0.4, 0.0],
                [1.0, -0.4, 0.0],
                [0.0, -5.0, 0.0],
                [1.0, -5.0, 0.0],
            ],
            0.9,
        );
        let context = context_with(&neighbor);
        let mut part = strip(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 10.0, 0.0],
                [1.0, 10.0, 0.0],
            ],
            0.1,
        );
        let mut sample = |_: &str, _: [f32; 2]| Some([0.5; 3]);
        // Same source file as the neighbour → no match.
        assert_eq!(
            blend_part_seams(&mut part, "meshes\\body.nif", None, &context, &mut sample)
                .matched_vertices,
            0
        );
        // Clothing texture → not a skin neighbour at all.
        let bone_binds = HashMap::from([("bone".to_string(), Mat4::IDENTITY)]);
        assert!(neighbor_from_mesh(
            &neighbor,
            "meshes\\outfit.nif",
            "armor\\outfitf.dds",
            &bone_binds
        )
        .is_none());
        // Too far (3 units > SEAM_MATCH_RADIUS) → no match.
        let far = strip(
            vec![
                [0.0, -3.0, 0.0],
                [1.0, -3.0, 0.0],
                [0.0, -6.0, 0.0],
                [1.0, -6.0, 0.0],
            ],
            0.9,
        );
        let far_context = context_with(&far);
        let mut part = strip(
            vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [0.0, 10.0, 0.0],
                [1.0, 10.0, 0.0],
            ],
            0.1,
        );
        assert_eq!(
            blend_part_seams(
                &mut part,
                "meshes\\head.nif",
                None,
                &far_context,
                &mut sample
            )
            .matched_vertices,
            0
        );
    }
}
