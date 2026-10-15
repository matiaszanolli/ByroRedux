//! Distant **tree** LOD — Skyrim's baked `.btt` billboards (#4913).
//!
//! Vanilla Skyrim `.bto` macro-meshes ([`super::object_lod`]) bake no trees.
//! The tree tier is its own family on the same quad grid: 386 level-4 `.btt`
//! files under `meshes\terrain\<ws>\trees\`, one `<ws>.lst` species list per
//! worldspace beside them, and the `textures\terrain\<ws>\trees\
//! <ws>treelod.dds` atlas the lists index into. FO4 and FO76 bake their trees
//! into the `.bto`, so this ring is Skyrim-only ([`tree_lod_supported`]).
//!
//! ## Formats (census-verified, see [`parse_btt`] / [`parse_tree_lst`])
//!
//! * `.btt` — `u32` group count, then per group a `u32` tree-type index, a
//!   `u32` count and that many 32-byte records: `f32` x, y, z (world, Z-up),
//!   `f32` rotation (radians), `f32` scale, `u32` REFR FormID, two more `u32`s
//!   (the first counts groups, the second is unexplained and unused here).
//! * `.lst` — `u32` entry count, then 32-byte entries: `u32` index, `f32`
//!   world width and height, `f32` u_min, v_min, u_max, v_max (the species'
//!   rect in the atlas), `u32`.
//!
//! The `.lst` is the **only** source of a billboard's world size and atlas
//! rect; a `.btt` record names only its species, so a worldspace without a
//! `.lst` draws no distant trees.
//!
//! ## What is drawn
//!
//! Each tree becomes **two crossed vertical quads** (an X in plan) sized and
//! textured from its species entry, standing on the record's base position.
//! All of a quad file's trees share one mesh, so the cost is one entity and
//! one global-SSBO range per `.btt` — the same residency shape as a `.bto`
//! quad ([`super::object_lod::ObjectLodBlock`]), drawn by the ordinary static
//! pass with alpha test and no BLAS ([`IsLodTerrain`]).
//!
//! **This is an interpretation, not the engine's draw.** The engine's tree
//! LOD billboards face the camera; here the quads are static. A crossed pair
//! needs no vertex-shader billboarding and keeps motion vectors honest (a
//! camera-facing quad would have to special-case its previous-frame
//! transform), at the price of a visible plan-view X on a very close
//! approach. The record's rotation orients the cross. The alpha cutoff
//! ([`TREE_LOD_ALPHA_CUTOFF`]) is likewise not sourced.
//!
//! ## Which quads
//!
//! The worldspace's own `.btt` files, selected **by name**: the archive name
//! tables are scanned once per streaming state for
//! `meshes\terrain\<ws>\trees\<ws>.4.<x>.<y>.btt`, and a quad is drawn when
//! its nearest cell is within `fTreeLoadDistance` ([`tree_load_cells`], 75000
//! BU at Ultra) of the player and its footprint touches no resident
//! full-detail cell — the full trees own that ground, exactly the rule the
//! `.bto` ring follows (EXAL §5.2). A resident quad survives one cell past
//! the load radius so a boundary crossing cannot flap it.
//!
//! Selecting by name rather than deriving a lattice from the worldspace's
//! `NAM0` is deliberate: the baked-LOD lattice origin is an open problem for
//! the other rings (#2586 — Tamriel's `.btr`/`.bto` sit on a lattice `NAM0`
//! does not give), and a tree quad's own name is the authoritative statement
//! of where it stands.

use std::collections::HashSet;

use byroredux_core::ecs::components::RenderLayer;
use byroredux_core::ecs::{GlobalTransform, MeshHandle, TextureHandle, Transform, World, WorldBound};
use byroredux_core::math::{Quat, Vec3};
use byroredux_renderer::{Vertex, VulkanContext};

use crate::asset_provider::{resolve_texture, TextureProvider};
use crate::components::{IsLodTerrain, TwoSided};

use super::exterior::ExteriorWorldContext;
use super::lod_bands::{quad_min_chebyshev, tree_load_cells, LOD_BAND_HYSTERESIS_CELLS};
use super::lod_support::{
    release_lod_gpu_resources, retain_resident, LodReconcileInput, LodWorkBudget,
};
use super::object_lod::{
    tree_lod_archive_path, tree_lod_atlas_path, tree_lod_list_path, tree_lod_supported,
    unload_object_lod_block, ObjectLodBlock,
};

/// The only level the vanilla corpus ships `.btt` files at.
const TREE_LOD_LEVEL: i32 = 4;

/// Alpha-test cutoff for the billboard cards, in `[0, 1]`. **Unsourced**: the
/// engine's tree-LOD shader is not available, so this is the conventional
/// half-coverage cutoff.
const TREE_LOD_ALPHA_CUTOFF: f32 = 0.5;

/// Bytes of one `.btt` tree record and of one `.lst` entry.
const RECORD_BYTES: usize = 32;

/// A malformed `.btt` / `.lst`. Both are untrusted archive input: every count
/// is checked against the bytes actually present before anything is
/// allocated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TreeLodError {
    /// The header or a group header ran past the end of the file.
    Truncated { needed: usize, have: usize },
    /// A declared count needs more bytes than the file has.
    CountExceedsData { count: u32, available: usize },
}

impl std::fmt::Display for TreeLodError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Truncated { needed, have } => {
                write!(f, "truncated: needs {needed} bytes, has {have}")
            }
            Self::CountExceedsData { count, available } => {
                write!(f, "declared count {count} exceeds the {available} bytes left")
            }
        }
    }
}

/// #5818 — the checked LE read goes through `core::bytes::le_u32_at`
/// (the #5596 boundary): its slice bound cannot overflow the way an
/// unchecked `at + 4` can when `at` comes from a file-declared count
/// near `usize::MAX`.
fn u32_at(bytes: &[u8], at: usize) -> Result<u32, TreeLodError> {
    byroredux_core::bytes::le_u32_at(bytes, at).ok_or(TreeLodError::Truncated {
        needed: at.saturating_add(4),
        have: bytes.len(),
    })
}

fn f32_at(bytes: &[u8], at: usize) -> Result<f32, TreeLodError> {
    byroredux_core::bytes::le_f32_at(bytes, at).ok_or(TreeLodError::Truncated {
        needed: at.saturating_add(4),
        have: bytes.len(),
    })
}

/// One `.lst` entry: a tree species' billboard.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TreeLodSpecies {
    /// The tree-type index `.btt` groups name.
    pub(crate) index: u32,
    /// World width and height of the billboard at scale 1, in units.
    pub(crate) width: f32,
    pub(crate) height: f32,
    /// The species' rect in the atlas: `[u_min, v_min, u_max, v_max]`.
    pub(crate) uv: [f32; 4],
}

/// Parse a worldspace's `.lst` species list.
pub(crate) fn parse_tree_lst(bytes: &[u8]) -> Result<Vec<TreeLodSpecies>, TreeLodError> {
    let count = u32_at(bytes, 0)?;
    let available = bytes.len() - 4;
    if (count as usize).saturating_mul(RECORD_BYTES) > available {
        return Err(TreeLodError::CountExceedsData { count, available });
    }
    (0..count as usize)
        .map(|i| {
            let at = 4 + i * RECORD_BYTES;
            Ok(TreeLodSpecies {
                index: u32_at(bytes, at)?,
                width: f32_at(bytes, at + 4)?,
                height: f32_at(bytes, at + 8)?,
                uv: [
                    f32_at(bytes, at + 12)?,
                    f32_at(bytes, at + 16)?,
                    f32_at(bytes, at + 20)?,
                    f32_at(bytes, at + 24)?,
                ],
            })
        })
        .collect()
}

/// One baked tree: where it stands and which species it is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TreeLodInstance {
    /// Key into the worldspace's species list.
    pub(crate) species: u32,
    /// World position of the tree's base, Z-up (Bethesda units).
    pub(crate) position: [f32; 3],
    /// Yaw in radians.
    pub(crate) rotation: f32,
    pub(crate) scale: f32,
    /// The placed REFR the tree stands in for (#3307's per-object id).
    pub(crate) ref_form_id: u32,
}

/// What one `.btt` held: its trees, and how many bytes followed the last
/// declared group. The six Solstheim files with trailing bytes (#5186) are
/// tolerated rather than refused — their declared groups parse exactly.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedBtt {
    pub(crate) trees: Vec<TreeLodInstance>,
    pub(crate) trailing_bytes: usize,
}

/// Parse one `.btt` quad file.
pub(crate) fn parse_btt(bytes: &[u8]) -> Result<ParsedBtt, TreeLodError> {
    let groups = u32_at(bytes, 0)?;
    let mut at = 4usize;
    let mut trees = Vec::new();
    for _ in 0..groups {
        let species = u32_at(bytes, at)?;
        let count = u32_at(bytes, at + 4)?;
        at += 8;
        let available = bytes.len().saturating_sub(at);
        if (count as usize).saturating_mul(RECORD_BYTES) > available {
            return Err(TreeLodError::CountExceedsData { count, available });
        }
        trees.reserve(count as usize);
        for _ in 0..count {
            trees.push(TreeLodInstance {
                species,
                position: [f32_at(bytes, at)?, f32_at(bytes, at + 4)?, f32_at(bytes, at + 8)?],
                rotation: f32_at(bytes, at + 12)?,
                scale: f32_at(bytes, at + 16)?,
                ref_form_id: u32_at(bytes, at + 20)?,
            });
            at += RECORD_BYTES;
        }
    }
    Ok(ParsedBtt {
        trees,
        trailing_bytes: bytes.len() - at,
    })
}

/// The crossed-card geometry of one quad's trees, in renderer (Y-up) world
/// space.
#[derive(Debug, Default)]
pub(crate) struct TreeLodMesh {
    pub(crate) vertices: Vec<Vertex>,
    pub(crate) indices: Vec<u32>,
    /// Trees whose species the `.lst` does not list, or whose size is
    /// unusable — dropped rather than drawn at a guessed size.
    pub(crate) skipped: usize,
}

/// Build one quad's mesh: for every tree with a usable species entry, two
/// vertical cards crossing at its base, `width × scale` wide and
/// `height × scale` tall, textured from the species' atlas rect.
///
/// Positions convert Z-up → Y-up (`(x, y, z) → (x, z, -y)`). Card A runs
/// along the Z-up direction `(cos φ, sin φ)` for the record's yaw φ, card B
/// perpendicular. The image's top edge takes `v_min` (DDS rows run top
/// first).
pub(crate) fn build_tree_lod_mesh(trees: &[TreeLodInstance], species: &[TreeLodSpecies]) -> TreeLodMesh {
    let mut mesh = TreeLodMesh::default();
    mesh.vertices.reserve(trees.len() * 8);
    mesh.indices.reserve(trees.len() * 12);
    for tree in trees {
        let Some(entry) = species.iter().find(|s| s.index == tree.species) else {
            mesh.skipped += 1;
            continue;
        };
        let scale = if tree.scale.is_finite() && tree.scale > 0.0 { tree.scale } else { 1.0 };
        let (half_w, height) = (entry.width * scale * 0.5, entry.height * scale);
        let finite = tree.position.iter().chain([&tree.rotation, &half_w, &height]).all(|v| v.is_finite());
        if !finite || half_w <= 0.0 || height <= 0.0 {
            mesh.skipped += 1;
            continue;
        }
        let [x, y, z] = tree.position;
        let base = Vec3::new(x, z, -y);
        let (sin, cos) = tree.rotation.sin_cos();
        // Z-up plan directions (cos, sin) and (-sin, cos) → Y-up XZ.
        for dir in [Vec3::new(cos, 0.0, -sin), Vec3::new(sin, 0.0, cos)] {
            let first = mesh.vertices.len() as u32;
            let [u0, v0, u1, v1] = entry.uv;
            for (side, top, u, v) in [
                (-1.0, false, u0, v1),
                (1.0, false, u1, v1),
                (1.0, true, u1, v0),
                (-1.0, true, u0, v0),
            ] {
                let p = base + dir * (side * half_w) + Vec3::Y * if top { height } else { 0.0 };
                // Foliage lights as soft ground-facing material, not as a
                // flat card with a face normal.
                mesh.vertices
                    .push(Vertex::new_rgba(p.to_array(), [1.0; 4], [0.0, 1.0, 0.0], [u, v]));
            }
            mesh.indices
                .extend_from_slice(&[first, first + 1, first + 2, first, first + 2, first + 3]);
        }
    }
    mesh
}

/// One archive file name as a tree quad: `Some((worldspace, qx, qy))` for
/// `meshes\terrain\<ws>\trees\<ws>.4.<x>.<y>.btt` (any case, either
/// separator), `None` for every other name.
pub(crate) fn parse_tree_quad_name(path: &str) -> Option<(String, i32, i32)> {
    let p = path.to_ascii_lowercase().replace('/', "\\");
    let rest = p.strip_prefix("meshes\\terrain\\")?;
    let (world, tail) = rest.split_once("\\trees\\")?;
    let stem = tail.strip_suffix(".btt")?;
    let fields = stem.strip_prefix(world)?.strip_prefix('.')?;
    let mut parts = fields.split('.');
    let level = parts.next()?.parse::<i32>().ok()?;
    let qx = parts.next()?.parse::<i32>().ok()?;
    let qy = parts.next()?.parse::<i32>().ok()?;
    (level == TREE_LOD_LEVEL && parts.next().is_none()).then(|| (world.to_owned(), qx, qy))
}

/// Every `.btt` quad of `worldspace_key` the open archives ship, sorted.
fn scan_tree_quads(tex_provider: &TextureProvider, worldspace_key: &str) -> Vec<(i32, i32)> {
    let mut quads: Vec<(i32, i32)> = tex_provider
        .all_archive_names()
        .filter_map(parse_tree_quad_name)
        .filter(|(world, _, _)| world == worldspace_key)
        .map(|(_, qx, qy)| (qx, qy))
        .collect();
    quads.sort_unstable();
    quads.dedup();
    quads
}

/// The quads of `all` whose nearest cell is within `range_cells` of
/// `player`, nearest first (ties break on coordinates, so the order is
/// deterministic).
pub(crate) fn tree_quads_in_range(
    all: &[(i32, i32)],
    player: (i32, i32),
    range_cells: i32,
) -> Vec<(i32, i32)> {
    let mut quads: Vec<(i32, i32, i32)> = all
        .iter()
        .map(|&(qx, qy)| (quad_min_chebyshev(qx, qy, TREE_LOD_LEVEL, player), qy, qx))
        .filter(|&(d, _, _)| d <= range_cells)
        .collect();
    quads.sort_unstable();
    quads.into_iter().map(|(_, qy, qx)| (qx, qy)).collect()
}

fn footprint_intersects_full_detail(qx: i32, qy: i32, resident: &HashSet<(i32, i32)>) -> bool {
    resident.iter().any(|&(gx, gy)| {
        gx >= qx && gx < qx + TREE_LOD_LEVEL && gy >= qy && gy < qy + TREE_LOD_LEVEL
    })
}

/// The tree ring's streaming state, one per worldspace session.
#[derive(Default)]
pub(crate) struct TreeLodRing {
    /// Resident quads by SW cell, an empty sentinel for a quad whose file
    /// failed to parse or held nothing drawable (so it is not re-read every
    /// reconcile).
    pub(crate) blocks: std::collections::HashMap<(i32, i32), ObjectLodBlock>,
    /// The worldspace's `.btt` quads, scanned from the archive name tables
    /// once per session; `None` until then.
    quads: Option<Vec<(i32, i32)>>,
    /// The worldspace's species list; the outer `None` means "not read yet",
    /// the inner `None` "this worldspace ships none" (nothing draws).
    species: Option<Option<Vec<TreeLodSpecies>>>,
}

impl TreeLodRing {
    /// Forget everything cached and hand back the resident blocks for the
    /// caller to unload (worldspace teardown).
    pub(crate) fn drain(&mut self) -> Vec<ObjectLodBlock> {
        self.quads = None;
        self.species = None;
        self.blocks.drain().map(|(_, block)| block).collect()
    }
}

fn load_species(tex_provider: &TextureProvider, wctx: &ExteriorWorldContext) -> Option<Vec<TreeLodSpecies>> {
    let path = tree_lod_list_path(wctx.lod_worldspace_key());
    let bytes = tex_provider.extract_mesh(&path)?;
    match parse_tree_lst(&bytes) {
        Ok(list) if !list.is_empty() => Some(list),
        Ok(_) => None,
        Err(e) => {
            log::warn!("Tree-LOD '{path}' parse failed: {e}");
            None
        }
    }
}

/// Stream the distant tree billboards around the player (Skyrim).
///
/// Returns `true` when every desired quad is resident or known missing.
/// Reclaims are immediate; entering quads consume [`LodWorkBudget`] units.
pub(crate) fn stream_tree_lod_blocks(
    world: &mut World,
    ctx: &mut VulkanContext,
    input: &LodReconcileInput<'_>,
    ring: &mut TreeLodRing,
    budget: &mut LodWorkBudget,
) -> bool {
    let wctx = input.wctx;
    let game = wctx.record_index.game;
    let (true, Some(range)) = (tree_lod_supported(game), tree_load_cells(game)) else {
        return true;
    };
    let tex_provider = input.tex_provider;
    let species = ring
        .species
        .get_or_insert_with(|| load_species(tex_provider, wctx))
        .clone();
    let Some(species) = species else {
        return true;
    };

    let lod_key = wctx.lod_worldspace_key().to_owned();
    let all_quads = ring
        .quads
        .get_or_insert_with(|| scan_tree_quads(tex_provider, &lod_key))
        .clone();
    let resident_full = input.resident_full_cells;
    let keep: HashSet<(i32, i32)> =
        tree_quads_in_range(&all_quads, input.player_grid, range + LOD_BAND_HYSTERESIS_CELLS)
            .into_iter()
            .filter(|&(qx, qy)| !footprint_intersects_full_detail(qx, qy, resident_full))
            .collect();

    let mut unloaded = 0usize;
    unloaded += retain_resident(&mut ring.blocks, &keep, |block| {
        if !block.entities.is_empty() {
            unload_object_lod_block(world, ctx, block);
            true
        } else {
            false
        }
    });

    let candidates: Vec<(i32, i32)> = tree_quads_in_range(&all_quads, input.player_grid, range)
        .into_iter()
        .filter(|key| keep.contains(key) && !ring.blocks.contains_key(key))
        .collect();
    let mut spawned = 0usize;
    for &(qx, qy) in &candidates {
        if !budget.try_take() {
            break;
        }
        let path = tree_lod_archive_path(&lod_key, qx, qy);
        let block = spawn_tree_lod_quad(world, ctx, tex_provider, &lod_key, &path, &species)
            .unwrap_or_else(ObjectLodBlock::empty);
        if !block.entities.is_empty() {
            spawned += 1;
        }
        ring.blocks.insert((qx, qy), block);
    }

    let complete = candidates.iter().all(|key| ring.blocks.contains_key(key));
    if complete && spawned + unloaded > 0 {
        log::info!(
            "Tree-LOD @cell ({},{}): +{} quads, -{} unloaded ({} tracked, {} cells deep)",
            input.player_grid.0,
            input.player_grid.1,
            spawned,
            unloaded,
            ring.blocks.len(),
            range,
        );
    }
    complete
}

fn spawn_tree_lod_quad(
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    lod_worldspace_key: &str,
    path: &str,
    species: &[TreeLodSpecies],
) -> Option<ObjectLodBlock> {
    let bytes = tex_provider.extract_mesh(path)?;
    let parsed = match parse_btt(&bytes) {
        Ok(parsed) => parsed,
        Err(e) => {
            log::warn!("Tree-LOD '{path}' parse failed: {e}");
            return None;
        }
    };
    let mesh = build_tree_lod_mesh(&parsed.trees, species);
    if mesh.indices.is_empty() {
        return None;
    }
    ctx.allocator.as_ref()?;

    let atlas_path = tree_lod_atlas_path(lod_worldspace_key);
    let atlas = resolve_texture(ctx, tex_provider, Some(atlas_path.as_str()));
    let atlas = if atlas == ctx.texture_registry.fallback() { 0 } else { atlas };

    let handle = match ctx
        .mesh_registry
        .upload_scene_mesh_global_only(&mesh.vertices, &mesh.indices)
    {
        Ok(handle) => handle,
        Err(e) => {
            log::warn!("Tree-LOD '{path}' mesh upload failed: {e}");
            let textures: Vec<u32> = (atlas != 0).then_some(atlas).into_iter().collect();
            release_lod_gpu_resources(ctx, &[], &textures);
            return None;
        }
    };
    ctx.mesh_registry.note_mesh_provenance(
        handle,
        byroredux_renderer::MeshUploadSource::Lod,
        false,
        Some(path),
    );

    let positions: Vec<[f32; 3]> = mesh.vertices.iter().map(|v| v.position).collect();
    let (centre, radius) = super::lod_support::local_aabb_center_radius(&positions);

    let entity = world.spawn();
    world.insert(entity, Transform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
    world.insert(entity, GlobalTransform::new(Vec3::ZERO, Quat::IDENTITY, 1.0));
    world.insert(entity, MeshHandle(handle));
    if atlas != 0 {
        world.insert(entity, TextureHandle(atlas));
    }
    world.insert(entity, WorldBound::new(centre, radius));
    // The NIFAL boundary material: a texture-only population with no source
    // record, alpha-tested, drawn from both sides.
    let mut material = crate::material_translate::translate_texture_only_material(
        (atlas != 0).then(|| atlas_path.clone()),
    );
    material.alpha_test = true;
    material.alpha_threshold = TREE_LOD_ALPHA_CUTOFF;
    world.insert(entity, material);
    world.insert(entity, TwoSided);
    world.insert(entity, RenderLayer::Architecture);
    // No BLAS, lean static draw, out of the TLAS — shared with every LOD.
    world.insert(entity, IsLodTerrain);

    if parsed.trailing_bytes > 0 || mesh.skipped > 0 {
        log::debug!(
            "Tree-LOD '{path}': {} trees drawn, {} skipped (no usable species), {} trailing bytes",
            parsed.trees.len() - mesh.skipped,
            mesh.skipped,
            parsed.trailing_bytes,
        );
    }
    Some(ObjectLodBlock {
        entities: vec![entity],
        mesh_handles: vec![handle],
        texture_handle: atlas,
        extra_texture_handles: Vec::new(),
    })
}

#[cfg(test)]
#[path = "tree_lod_tests.rs"]
mod tests;
