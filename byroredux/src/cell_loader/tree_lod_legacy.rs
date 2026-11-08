//! Distant **tree** LOD for Oblivion and Fallout 3 (#6638, DLOD-TREE-001 to -007).
//!
//! Two families author distant trees in a container [`super::tree_lod`] does not read:
//!
//! * **Oblivion** — the `distantlod\<ws>_<x>_<y>.lod` placement groups whose base is a TREE record (82
//!   bases, 98 661 placements: most of all `.lod` placements). [`super::placement_lod`] skips them, because
//!   a TREE's `MODL` is a bare `.spt` name no `_far.nif` exists for.
//! * **Fallout 3** — `meshes\landscape\lod\<ws>\trees\<ws>.level<L>.x<X>.y<Y>.dtl` files
//!   ([`super::tree_lod::parse_dtl`]), each a list of `{position, scale, base FormID}` records.
//!
//! They are drawn differently, because the data differs:
//!
//! * **Oblivion** tree groups translate into the representation `tree_lod.rs` draws (DLOD-TREE-006): one mesh
//!   of crossed upright alpha-tested cards per LOD cell ([`super::tree_lod::build_tree_lod_mesh`]),
//!   `IsLodTerrain`. A TREE is an `.spt` with no geometry, but it has a billboard texture and size.
//! * **Fallout 3**'s `.dtl` records do not name TREE bases: they name placed references whose bases are STAT
//!   records with NIF tree models (`landscape\trees\treedead03.nif`, 99.99 % of the 32,275 vanilla trees),
//!   and the archives ship an authored `<model>_lod_flat.nif` for each. Those are drawn as authored,
//!   instanced like `_far.nif` placements ([`super::placement_lod::spawn_placement_groups`]); there is no
//!   billboard texture to pack and no card shape to guess. This departs from PRD Q3's crossed cards for
//!   Fallout 3, which Q3 itself anticipated ("V5 may still show those titles drew them another way").
//!
//! ## Texturing the Oblivion cards: one runtime atlas per worldspace
//!
//! Skyrim ships a `treelod` atlas and a `.lst` giving each species' size and rect. Oblivion does not: a tree
//! is its TREE base record, whose `BNAM` is the billboard size and whose `.spt` stem names the billboard
//! texture, `textures\trees\billboards\<stem>.dds` (census: all 82 Oblivion TREE bases pair one-to-one with a
//! billboard by stem and none by `ICON`, V2). The billboards are packed once per streaming state into a
//! single RGBA atlas ([`pack_billboards`]) so a LOD file stays one mesh and one draw.
//!
//! ## What is an interpretation
//!
//! No document describes Oblivion's distant-tree shape (V5), so its crossed cards, the billboard texture's
//! size and the alpha cutoff are **an interpretation, not the engine's draw** (DLOD-TREE-010). Fallout 3's
//! `_lod_flat.nif` meshes are authored data; only the reach is an interpretation. The reaches are the games'
//! own settings, with the radius-or-diameter unit unsettled (V1, V10): Oblivion's `uGridDistantTreeRange`
//! (15, UESP) and Fallout 3's `[TerrainManager] fTreeLoadDistance` (40000 BU, line 26 of the shipped
//! `VeryHigh.ini`, so 10 cells). Both reaches sit inside the default full-detail radius, so at the default
//! `--radius` a ring draws only what lies beyond it (DLOD-BAND-007, Q7).

use std::collections::{HashMap, HashSet};

use byroredux_core::ecs::World;
use byroredux_menuxml::tex::Rgba8;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_renderer::VulkanContext;

use crate::asset_provider::TextureProvider;

use super::lod_bands::{cells_from_bu, quad_min_chebyshev};
use super::lod_support::{
    classify_lod_attempt, release_lod_gpu_resources, retain_resident, LodAttemptVerdict,
    LodReconcileInput, LodWorkBudget,
};
use super::object_lod::{unload_object_lod_block, ObjectLodBlock};
use super::placement_lod::{
    parse_placement_lod, placement_lod_cell_path, placement_lod_cells_in_radius, spawn_placement_groups,
    unload_placement_lod_block, Placement, PlacementGroup, PlacementLodBlock,
};
use super::tree_lod::{
    build_tree_lod_mesh, parse_dtl, spawn_tree_lod_mesh, DtlTree, TreeLodInstance, TreeLodSpecies,
};

/// Oblivion's `[General] uGridDistantTreeRange`: 15 in the shipped `Oblivion_default.ini` (UESP, *Oblivion
/// Mod:Oblivion.ini*: "the visible distance on trees").
pub(crate) const OBLIVION_GRID_DISTANT_TREE_RANGE: i32 = 15;

/// The tree ring's reach in cells from the player. The INI value is read as a **diameter** in cells, like the
/// same file's `uGridsToLoad=5` (a 5x5 loaded grid): 15 across is a radius of 7. The unit has no second
/// source (V1), so this is the owner's ruling on the PRD's risk, taken 2026-11-08. It sits inside the default
/// 12-cell full-detail radius, so at the default `--radius` the ring draws nothing (DLOD-BAND-007, Q7).
/// Pinned by `legacy_tree_ranges_are_the_documented_ini_readings`.
pub(crate) const OBLIVION_TREE_RANGE_CELLS: i32 = OBLIVION_GRID_DISTANT_TREE_RANGE / 2;

/// Fallout 3's `[TerrainManager] fTreeLoadDistance` in BU (40000 in both the FO3 and FNV `VeryHigh.ini`, line
/// 26). What the key bounds on these games is undocumented (V10); this reads it as the `.dtl` ring's reach,
/// an interpretation. Pinned by `legacy_tree_ranges_are_the_documented_ini_readings`.
const FALLOUT3_TREE_LOAD_BU: f32 = 40_000.0;

/// Edge of one billboard's tile in the packed atlas, in texels. Billboards are stretched to the tile and the
/// card geometry (the TREE `BNAM` size) restores their aspect.
const ATLAS_TILE: u32 = 128;

/// The largest atlas side the packer will build.
const ATLAS_MAX_SIDE: u32 = 4096;

/// The two legacy tree containers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacyTreeFamily {
    /// Oblivion: TREE groups of the per-cell `distantlod\*.lod` placement files.
    OblivionLod,
    /// Fallout 3: the per-quad `.dtl` tree files, drawn as their authored `_lod_flat.nif` meshes. New Vegas
    /// shares `GameKind::Fallout3NV`; its archives hold no `.dtl`, so its ring finds nothing and spawns
    /// nothing (DLOD-TREE-009).
    FalloutDtl,
}

/// Which legacy tree container `game` authors, if any.
pub(crate) fn legacy_tree_family(game: GameKind) -> Option<LegacyTreeFamily> {
    match game {
        GameKind::Oblivion => Some(LegacyTreeFamily::OblivionLod),
        GameKind::Fallout3NV => Some(LegacyTreeFamily::FalloutDtl),
        _ => None,
    }
}

/// How many cells out the legacy tree ring reaches, `None` where `game` has no legacy tree family.
pub(crate) fn legacy_tree_range_cells(game: GameKind) -> Option<i32> {
    match legacy_tree_family(game)? {
        LegacyTreeFamily::OblivionLod => Some(OBLIVION_TREE_RANGE_CELLS),
        LegacyTreeFamily::FalloutDtl => Some(cells_from_bu(FALLOUT3_TREE_LOAD_BU)),
    }
}

// ── Billboard texture and atlas ────────────────────────────────────────────────────────────────────

/// The billboard texture a TREE base is drawn with at distance: `textures\trees\billboards\<stem>.dds`, where
/// `stem` is the `.spt` model's file stem. `None` for a model that is not an `.spt`.
pub(crate) fn billboard_texture_path(model_path: &str) -> Option<String> {
    let file = model_path.rsplit(['\\', '/']).next()?;
    let (stem, ext) = file.rsplit_once('.')?;
    (ext.eq_ignore_ascii_case("spt") && !stem.is_empty())
        .then(|| format!("textures\\trees\\billboards\\{}.dds", stem.to_ascii_lowercase()))
}

/// One decoded billboard going into the atlas.
pub(crate) struct BillboardSource {
    /// The TREE base FormID; it becomes the species index.
    pub(crate) form_id: u32,
    /// World size of the card at scale 1 (the TREE `BNAM`).
    pub(crate) width: f32,
    pub(crate) height: f32,
    pub(crate) image: Rgba8,
}

/// The packed billboards: an RGBA image and one species entry per source.
pub(crate) struct PackedBillboards {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) pixels: Vec<u8>,
    pub(crate) species: Vec<TreeLodSpecies>,
}

/// Pack `sources` into a grid of [`ATLAS_TILE`]-sized tiles, row-major in source order. Each billboard is
/// resampled (nearest, texel-centre) to its tile; a species' rect is the tile in `[0, 1]` atlas coordinates
/// (`v_min` is the tile's top edge, matching [`super::tree_lod::build_tree_lod_mesh`]). `None` when there is
/// nothing to pack or the grid would exceed [`ATLAS_MAX_SIDE`].
pub(crate) fn pack_billboards(sources: &[BillboardSource]) -> Option<PackedBillboards> {
    if sources.is_empty() {
        return None;
    }
    let cols = (sources.len() as f64).sqrt().ceil() as u32;
    let rows = (sources.len() as u32).div_ceil(cols);
    let (width, height) = (cols * ATLAS_TILE, rows * ATLAS_TILE);
    if width > ATLAS_MAX_SIDE || height > ATLAS_MAX_SIDE {
        return None;
    }
    let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
    let mut species = Vec::with_capacity(sources.len());
    for (i, source) in sources.iter().enumerate() {
        let (col, row) = (i as u32 % cols, i as u32 / cols);
        let (x0, y0) = (col * ATLAS_TILE, row * ATLAS_TILE);
        let image = &source.image;
        for ty in 0..ATLAS_TILE {
            let sy = (((ty as f32 + 0.5) / ATLAS_TILE as f32) * image.height as f32) as i64;
            for tx in 0..ATLAS_TILE {
                let sx = (((tx as f32 + 0.5) / ATLAS_TILE as f32) * image.width as f32) as i64;
                let at = (((y0 + ty) as usize) * width as usize + (x0 + tx) as usize) * 4;
                pixels[at..at + 4].copy_from_slice(&image.pixel(sx, sy));
            }
        }
        species.push(TreeLodSpecies {
            index: source.form_id,
            width: source.width,
            height: source.height,
            uv: [
                x0 as f32 / width as f32,
                y0 as f32 / height as f32,
                (x0 + ATLAS_TILE) as f32 / width as f32,
                (y0 + ATLAS_TILE) as f32 / height as f32,
            ],
        });
    }
    Some(PackedBillboards { width, height, pixels, species })
}

/// Decode every usable TREE billboard the loaded archives hold: an `.spt` model with a positive finite `BNAM`
/// size and a decodable `trees\billboards\<stem>.dds`. Sorted by FormID so the atlas is deterministic.
fn gather_billboards(
    trees: &HashMap<u32, byroredux_plugin::esm::records::TreeRecord>,
    tex_provider: &TextureProvider,
) -> Vec<BillboardSource> {
    let mut forms: Vec<u32> = trees.keys().copied().collect();
    forms.sort_unstable();
    let mut out = Vec::new();
    for form_id in forms {
        let tree = &trees[&form_id];
        let Some((width, height)) = tree.billboard_size else { continue };
        if !(width.is_finite() && height.is_finite() && width > 0.0 && height > 0.0) {
            continue;
        }
        let Some(path) = billboard_texture_path(&tree.model_path) else { continue };
        let Some(bytes) = tex_provider.extract(&path) else { continue };
        let Some(image) = Rgba8::decode_dds(&bytes) else {
            log::debug!("Tree-LOD billboard '{path}' is not a decodable DDS");
            continue;
        };
        out.push(BillboardSource { form_id, width, height, image });
    }
    out
}

// ── Instances ──────────────────────────────────────────────────────────────────────────────────────

/// Oblivion: the placements of every `.lod` group whose base is a TREE (`is_tree`), as card instances. The
/// group's base FormID is the species; the Z euler is the yaw (cards are a symmetric cross, so its sign
/// convention only matters modulo 90 degrees).
pub(crate) fn oblivion_tree_instances(
    groups: &[PlacementGroup],
    is_tree: impl Fn(u32) -> bool,
) -> Vec<TreeLodInstance> {
    groups
        .iter()
        .filter(|group| is_tree(group.base_form_id))
        .flat_map(|group| {
            group.placements.iter().map(|placement| TreeLodInstance {
                species: group.base_form_id,
                position: placement.position,
                rotation: placement.rotation[2],
                scale: placement.scale,
                ref_form_id: 0,
            })
        })
        .collect()
}

/// Fallout 3: a `.dtl`'s trees as [`PlacementGroup`]s, one per STAT base, for the instanced flat-mesh path.
/// The record names the placed REFR the tree stands in for; `base_of_ref` resolves it to the base (whose
/// `landscape\trees\*.nif` model has the authored `_lod_flat.nif`). A tree whose reference does not resolve
/// is dropped rather than drawn as a guessed species. Groups are ordered by base FormID, so the result is
/// deterministic; the chunk's `treetypes.lst` index is not needed to draw.
pub(crate) fn dtl_placement_groups(
    trees: &[DtlTree],
    base_of_ref: impl Fn(u32) -> Option<u32>,
) -> Vec<PlacementGroup> {
    let mut by_base: std::collections::BTreeMap<u32, Vec<Placement>> = Default::default();
    for tree in trees {
        let Some(base) = base_of_ref(tree.ref_form_id) else { continue };
        by_base.entry(base).or_default().push(Placement {
            position: tree.position,
            rotation: [0.0, 0.0, tree.rotation],
            scale: tree.scale,
        });
    }
    by_base
        .into_iter()
        .map(|(base_form_id, placements)| PlacementGroup { base_form_id, placements })
        .collect()
}

/// The authored distant mesh of a Fallout 3 tree model: `landscape\trees\treedead03.nif` ships
/// `treedead03_lod_flat.nif` beside it (census: 8 of the 8 wasteland species the `.dtl` files place have one).
/// `None` for a model that is not a `.nif`.
pub(crate) fn lod_flat_nif_path(model_path: &str) -> Option<String> {
    let lower = model_path.to_ascii_lowercase().replace('/', "\\");
    let stem = lower.strip_suffix(".nif")?;
    let flat = format!("{stem}_lod_flat.nif");
    Some(if flat.starts_with("meshes\\") { flat } else { format!("meshes\\{flat}") })
}

/// One archive file name as a Fallout 3 tree quad: `Some((worldspace, level, x, y))` for
/// `meshes\landscape\lod\<ws>\trees\<ws>.level<L>.x<X>.y<Y>.dtl` (any case, either separator).
pub(crate) fn parse_dtl_name(path: &str) -> Option<(String, i32, i32, i32)> {
    let p = path.to_ascii_lowercase().replace('/', "\\");
    let rest = p.strip_prefix("meshes\\landscape\\lod\\")?;
    let (world, tail) = rest.split_once("\\trees\\")?;
    let stem = tail.strip_suffix(".dtl")?;
    let fields = stem.strip_prefix(world)?.strip_prefix('.')?;
    let mut parts = fields.split('.');
    let level = parts.next()?.strip_prefix("level")?.parse::<i32>().ok()?;
    let qx = parts.next()?.strip_prefix('x')?.parse::<i32>().ok()?;
    let qy = parts.next()?.strip_prefix('y')?.parse::<i32>().ok()?;
    parts.next().is_none().then(|| (world.to_owned(), level, qx, qy))
}

/// Whether any resident full-detail cell lies inside the level-`level` quad at `(qx, qy)`: the full trees own
/// that ground, so the file is not drawn (DLOD-TREE-007).
fn footprint_has_resident_cell(qx: i32, qy: i32, level: i32, resident: &HashSet<(i32, i32)>) -> bool {
    resident
        .iter()
        .any(|&(gx, gy)| gx >= qx && gx < qx + level && gy >= qy && gy < qy + level)
}

// ── The ring ───────────────────────────────────────────────────────────────────────────────────────

struct LegacyAtlas {
    /// The registered atlas texture; owned by the ring (one reference), released on drain.
    handle: u32,
    species: Vec<TreeLodSpecies>,
}

/// One resident unit of the ring: Oblivion's single card mesh, or Fallout 3's instanced flat meshes.
pub(crate) enum LegacyTreeBlock {
    Cards(ObjectLodBlock),
    Flat(PlacementLodBlock),
}

impl LegacyTreeBlock {
    /// Whether the unit drew anything (an empty block is a "nothing here" sentinel).
    fn drew(&self) -> bool {
        match self {
            Self::Cards(block) => !block.entities.is_empty(),
            Self::Flat(block) => !block.entities.is_empty(),
        }
    }

    fn sentinel() -> Self {
        Self::Cards(ObjectLodBlock::empty())
    }
}

/// Free one resident unit's meshes, textures and entities.
pub(crate) fn unload_legacy_tree_block(world: &mut World, ctx: &mut VulkanContext, block: &LegacyTreeBlock) {
    match block {
        LegacyTreeBlock::Cards(block) => unload_object_lod_block(world, ctx, block),
        LegacyTreeBlock::Flat(block) => unload_placement_lod_block(world, ctx, block),
    }
}

/// The legacy tree ring's streaming state, one per worldspace session.
#[derive(Default)]
pub(crate) struct LegacyTreeRing {
    /// Resident units by `(level, x, y)`: an Oblivion `.lod` cell is `(1, cx, cy)`, a Fallout 3 `.dtl` quad
    /// its authored level and south-west cell. An empty block is a sentinel for a unit that held no drawable
    /// tree, so it is not re-read every reconcile.
    pub(crate) blocks: HashMap<(i32, i32, i32), LegacyTreeBlock>,
    /// Oblivion's packed billboard atlas; the outer `None` means "not built yet", the inner `None` "this
    /// worldspace has no usable billboard" (nothing draws). Fallout 3 builds none.
    atlas: Option<Option<LegacyAtlas>>,
    /// Fallout 3: the worldspace's `.dtl` files, scanned from the archive name tables once.
    dtl_files: Option<Vec<(i32, i32, i32, String)>>,
}

impl LegacyTreeRing {
    /// The resident units that drew something, as `(level, x, y)` footprint keys.
    pub(crate) fn resident_keys(&self) -> Vec<(i32, i32, i32)> {
        self.blocks.iter().filter(|(_, block)| block.drew()).map(|(&key, _)| key).collect()
    }

    /// Forget everything cached and hand back the resident blocks and the atlas handle for the caller to
    /// release (worldspace teardown).
    pub(crate) fn drain(&mut self) -> (Vec<LegacyTreeBlock>, Option<u32>) {
        self.dtl_files = None;
        let atlas = self.atlas.take().flatten().map(|a| a.handle);
        (self.blocks.drain().map(|(_, block)| block).collect(), atlas)
    }
}

/// Build the worldspace's billboard atlas and register it as a texture. `None` when nothing usable packs.
fn build_atlas(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    trees: &HashMap<u32, byroredux_plugin::esm::records::TreeRecord>,
) -> Option<LegacyAtlas> {
    let sources = gather_billboards(trees, tex_provider);
    let packed = pack_billboards(&sources)?;
    let upload_ctx = byroredux_renderer::upload_ctx!(ctx);
    match ctx
        .texture_registry
        .register_rgba(upload_ctx, packed.width, packed.height, &packed.pixels)
    {
        Ok(handle) => {
            log::info!(
                "Tree-LOD billboard atlas: {} species in {}x{} texels",
                packed.species.len(),
                packed.width,
                packed.height
            );
            Some(LegacyAtlas { handle, species: packed.species })
        }
        Err(e) => {
            log::warn!("Tree-LOD billboard atlas upload failed: {e}");
            None
        }
    }
}

/// Stream the Oblivion `.lod` tree cells / Fallout 3 `.dtl` quads around the player.
///
/// Returns `true` when every desired unit is resident or known missing. Reclaims are immediate; entering
/// units consume [`LodWorkBudget`] units and upload through the `Prop` admission tier (DLOD-PERF-001/-002).
pub(crate) fn stream_legacy_tree_lod_blocks(
    world: &mut World,
    ctx: &mut VulkanContext,
    input: &LodReconcileInput<'_>,
    ring: &mut LegacyTreeRing,
    budget: &mut LodWorkBudget,
) -> bool {
    let wctx = input.wctx;
    let game = wctx.record_index.game;
    let (Some(family), Some(range)) = (legacy_tree_family(game), legacy_tree_range_cells(game)) else {
        return true;
    };
    let tex_provider = input.tex_provider;
    // Oblivion's cards need the billboard atlas; Fallout 3 draws authored meshes and needs none.
    let (atlas_handle, species) = if family == LegacyTreeFamily::OblivionLod {
        if ring.atlas.is_none() {
            ring.atlas = Some(build_atlas(ctx, tex_provider, &wctx.record_index.trees));
        }
        let Some(Some(atlas)) = ring.atlas.as_ref() else {
            return true;
        };
        (atlas.handle, atlas.species.clone())
    } else {
        (0, Vec::new())
    };
    let player = input.player_grid;
    let resident_full = input.resident_full_cells;

    // The units the ring wants: Oblivion cells beyond the full-detail ring, Fallout 3 quads whose footprint
    // touches no resident cell.
    let desired: Vec<(i32, i32, i32)> = match family {
        LegacyTreeFamily::OblivionLod => placement_lod_cells_in_radius(player, resident_full, range)
            .into_iter()
            .map(|(cx, cy)| (1, cx, cy))
            .collect(),
        LegacyTreeFamily::FalloutDtl => {
            let ws = wctx.lod_worldspace_key().to_ascii_lowercase();
            let files = ring.dtl_files.get_or_insert_with(|| {
                let mut files: Vec<(i32, i32, i32, String)> = tex_provider
                    .all_archive_names()
                    .filter_map(|name| parse_dtl_name(name).map(|(w, l, x, y)| (w, l, x, y, name.to_owned())))
                    .filter(|(w, ..)| *w == ws)
                    .map(|(_, l, x, y, name)| (l, x, y, name))
                    .collect();
                files.sort_unstable();
                files.dedup_by(|a, b| (a.0, a.1, a.2) == (b.0, b.1, b.2));
                files
            });
            files
                .iter()
                .filter(|&&(level, qx, qy, _)| {
                    quad_min_chebyshev(qx, qy, level, player) <= range
                        && !footprint_has_resident_cell(qx, qy, level, resident_full)
                })
                .map(|&(level, qx, qy, _)| (level, qx, qy))
                .collect()
        }
    };
    let keep: HashSet<(i32, i32, i32)> = desired.iter().copied().collect();

    let mut unloaded = 0usize;
    unloaded += retain_resident(&mut ring.blocks, &keep, |block| {
        if block.drew() {
            unload_legacy_tree_block(world, ctx, block);
            true
        } else {
            false
        }
    });

    let mut candidates: Vec<(i32, i32, i32)> =
        desired.into_iter().filter(|key| !ring.blocks.contains_key(key)).collect();
    candidates.sort_unstable_by_key(|&(level, x, y)| (quad_min_chebyshev(x, y, level, player), level, x, y));
    let mut spawned = 0usize;
    let mut starved = false;
    for (index, &key) in candidates.iter().enumerate() {
        if !ctx.mesh_registry.prop_geometry_admission_open() {
            budget.note_held_back(candidates.len() - index);
            starved = true;
            break;
        }
        if !budget.try_take() {
            break;
        }
        let block = match spawn_legacy_tree_unit(world, ctx, input, family, key, atlas_handle, &species) {
            Some(block) => block,
            None => match classify_lod_attempt(false, ctx.mesh_registry.prop_geometry_admission_open()) {
                LodAttemptVerdict::Miss => LegacyTreeBlock::sentinel(),
                // #6092 — pool refusal, not a missing unit: no sentinel.
                LodAttemptVerdict::HeldBack => {
                    budget.note_held_back(candidates.len() - index);
                    starved = true;
                    break;
                }
                LodAttemptVerdict::Keep => unreachable!("no block was produced"),
            },
        };
        if block.drew() {
            spawned += 1;
        }
        ring.blocks.insert(key, block);
    }

    let complete = starved || candidates.iter().all(|key| ring.blocks.contains_key(key));
    if complete && spawned + unloaded > 0 {
        log::info!(
            "Legacy tree-LOD ring @cell ({},{}): +{} units, -{} unloaded ({} tracked)",
            player.0,
            player.1,
            spawned,
            unloaded,
            ring.blocks.len(),
        );
    }
    complete
}

/// Read one unit's trees and spawn it.
fn spawn_legacy_tree_unit(
    world: &mut World,
    ctx: &mut VulkanContext,
    input: &LodReconcileInput<'_>,
    family: LegacyTreeFamily,
    (level, x, y): (i32, i32, i32),
    atlas_handle: u32,
    species: &[TreeLodSpecies],
) -> Option<LegacyTreeBlock> {
    let wctx = input.wctx;
    let tex_provider = input.tex_provider;
    match family {
        LegacyTreeFamily::OblivionLod => {
            let path = placement_lod_cell_path(
                &wctx.worldspace_key,
                wctx.lod_worldspace_key(),
                x,
                y,
                |path| tex_provider.has_mesh_exact(path),
            )?;
            let bytes = tex_provider.extract_mesh_exact(&path)?;
            let groups = match parse_placement_lod(&bytes) {
                Ok(groups) => groups,
                Err(e) => {
                    log::warn!("Tree-LOD '{path}' parse failed: {e}");
                    return None;
                }
            };
            let trees = &wctx.record_index.trees;
            let instances = oblivion_tree_instances(&groups, |form| trees.contains_key(&form));
            let mesh = build_tree_lod_mesh(&instances, species);
            if mesh.indices.is_empty() {
                return None;
            }
            ctx.allocator.as_ref()?;
            // The ring owns the atlas reference, so the block carries none (texture_handle 0 is never
            // released).
            let name = format!("<tree-lod billboard atlas {}>", wctx.lod_worldspace_key());
            let mut block = spawn_tree_lod_mesh(world, ctx, &mesh, atlas_handle, Some(name), &path)?;
            block.texture_handle = 0;
            if mesh.skipped > 0 {
                log::debug!(
                    "Tree-LOD '{path}': {} trees drawn, {} skipped (no usable species)",
                    instances.len() - mesh.skipped,
                    mesh.skipped
                );
            }
            Some(LegacyTreeBlock::Cards(block))
        }
        LegacyTreeFamily::FalloutDtl => {
            let ws = wctx.lod_worldspace_key().to_ascii_lowercase();
            let path = format!("meshes\\landscape\\lod\\{ws}\\trees\\{ws}.level{level}.x{x}.y{y}.dtl");
            let bytes = tex_provider.extract_mesh_exact(&path)?;
            let trees = match parse_dtl(&bytes) {
                Ok(trees) => trees,
                Err(e) => {
                    log::warn!("Tree-LOD '{path}' parse failed: {e}");
                    return None;
                }
            };
            let cells = &wctx.record_index.cells;
            let groups = dtl_placement_groups(&trees, |refr| {
                cells.locate_refr(refr).map(|(_, placed)| placed.base_form_id)
            });
            spawn_placement_groups(world, ctx, tex_provider, wctx, &groups, &|model_path: &str| {
                let flat = lod_flat_nif_path(model_path)?;
                tex_provider.extract_mesh_exact(&flat).map(|bytes| (flat, bytes))
            })
            .map(LegacyTreeBlock::Flat)
        }
    }
}

/// Release the atlas the ring owned, after its blocks were unloaded (worldspace teardown).
pub(crate) fn release_legacy_tree_atlas(ctx: &mut VulkanContext, atlas: Option<u32>) {
    if let Some(handle) = atlas {
        release_lod_gpu_resources(ctx, &[], &[handle]);
    }
}

#[cfg(test)]
#[path = "tree_lod_legacy_tests.rs"]
mod tests;
