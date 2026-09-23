//! Exterior LAND heightmap → terrain mesh conversion.
//!
//! Each exterior cell's `LAND` record carries a 33×33 vertex grid spanning
//! 4096×4096 Bethesda units (128-unit vertex spacing) plus up to 8 texture
//! splat layers per UESP's LAND format spec. This module turns that
//! authoring data into a GPU mesh + ECS entity, including per-vertex splat
//! weights packed into 2×RGBA8 attributes for the fragment shader.
//!
//! Coordinate conversion (Bethesda Z-up → renderer Y-up):
//!   `world_x = grid_x * 4096 + col * 128`     → X
//!   `world_z = heights[row][col]`             → Y (up)
//!   `world_y = grid_y * 4096 + row * 128`     → −Z (negate for Y-up)
//!
//! See `#470` for the splat-layer landing.

use std::collections::HashMap;

use byroredux_core::ecs::{GlobalTransform, MeshHandle, TextureHandle, Transform, World};
use byroredux_core::math::coord::{
    zup_to_yup_pos, EXTERIOR_CELL_UNITS, LAND_GRID_VERTS, LAND_VERTEX_SPACING,
};
use byroredux_core::math::{Quat, Vec3};
use byroredux_plugin::esm;
use byroredux_plugin::esm::cell::TextureSet;
use byroredux_renderer::vulkan::scene_buffer::GpuTerrainTile;
use byroredux_renderer::vulkan::GpuUploadCtx;
use byroredux_renderer::{Vertex, VulkanContext};

use crate::asset_provider::{
    derive_present_normal_map_path, resolve_linear_texture, resolve_texture, TextureProvider,
};
use crate::components::{MaterialTextureHandles, TerrainTileSlot};
use byroredux_nif::import::MaterialTextureSet;

/// Number of times a LAND diffuse/splat texture tiles across one exterior
/// cell edge. Bethesda's LAND format carries no per-texture tiling field, so
/// this is a fixed engine constant: the diffuse repeats `2 × quad-textures-
/// per-side` times per cell. Per openmw's ESM4 (Oblivion+) terrain
/// (`Storage::getTextureTileCount` → `2 * ESM4::Land::sQuadTexturePerSide`,
/// with `sQuadTexturePerSide = 6`) that is **12** — 2 quadrants per cell side
/// × 6 texture tiles per quadrant. Pre-fix the UV ran 0→1 across the whole
/// 4096-BU cell (≈1 texel / 16 BU), so every exterior surface read as a
/// blurry gray average regardless of mip level; the texture has to tile so
/// near terrain shows real detail. `Lod` terrain reuses this same factor
/// (`terrain_lod`) so the seam at the full-detail boundary tiles identically.
pub(super) const LAND_TEXTURE_TILES_PER_CELL: f32 = 12.0;

/// Resolved terrain splat layers for one cell — up to 8 cell-global layers,
/// each with its bindless texture handle and the per-quadrant alpha grids
/// contributed by every quadrant that painted that LTEX. Produced by
/// [`build_cell_splat_layers`] and consumed by the vertex packer in
/// [`spawn_terrain_mesh`]. See #470.
#[derive(Default)]
pub(super) struct CellSplatLayers {
    /// 0–8 entries sorted by ascending `layer_sort_key`, then by
    /// `ltex_form_id` for deterministic tiebreak.
    layers: Vec<CellSplatLayer>,
}

pub(super) struct CellSplatLayer {
    /// Source LTEX FormID. Retained when its diffuse fails to resolve because
    /// `LTEX.GNAM` vegetation belongs to layer identity, not texture upload.
    pub ltex_form_id: Option<u32>,
    /// §3's `cover_affinity` for this layer, resolved from its `LTEX` name by
    /// the keyword table (#4054). A layer does not *enable* ground cover, it
    /// *weights* it — which is what makes the vegetation boundary stop
    /// coinciding with the texture boundary.
    pub cover_affinity: f32,
    /// Bindless diffuse handle (resolved via LTEX → TXST → TX00).
    /// 0 means the texture failed to load; fragment shader skips (index 0
    /// is the fallback checkerboard).
    pub diffuse_index: u32,
    /// Optional TX01 tangent-space normal paired with the diffuse layer.
    pub normal_index: u32,
    /// Optional TX07 specular-colour map paired with the layer.
    pub specular_index: u32,
    /// Per-quadrant contribution. `[SW, SE, NW, NE]` — `None` means the
    /// quadrant didn't paint this LTEX. Each `Some` is a 17×17 alpha grid.
    pub per_quadrant_alpha: [Option<Vec<f32>>; 4],
}

/// Per-quadrant alpha grids for one LTEX. `[SW, SE, NW, NE]` matches
/// `CellSplatLayer::per_quadrant_alpha`; `None` means that quadrant
/// didn't paint this LTEX.
type PerQuadrantAlpha = [Option<Vec<f32>>; 4];

/// Collect cell-global splat layers from the 4 quadrants. Dedup by
/// `ltex_form_id`; take the minimum `layer` field as the sort key so seam
/// vertices across quadrants resolve to the same cell-global layer. Caps
/// at 8 per UESP's LAND format spec; excess is dropped with a warning.
pub(super) fn build_cell_splat_layers(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    landscape_textures: &HashMap<u32, String>,
    landscape_texture_sets: &HashMap<u32, TextureSet>,
    land: &esm::cell::LandscapeData,
    canonical_base_ltex: Option<u32>,
    default_land: Option<DefaultLandTexture>,
) -> CellSplatLayers {
    use std::collections::hash_map::Entry;

    let mut by_ltex: HashMap<u32, (u16, PerQuadrantAlpha)> = HashMap::new();
    for (q_idx, q) in land.quadrants.iter().enumerate() {
        for l in &q.layers {
            let Some(ref alpha) = l.alpha else {
                // Malformed ATXT without VTXT — nothing to paint. #470.
                log::debug!(
                    "Terrain quadrant {}: ATXT LTEX {:08X} layer {} has no VTXT; skipped",
                    q_idx,
                    l.ltex_form_id,
                    l.layer
                );
                continue;
            };
            match by_ltex.entry(l.ltex_form_id) {
                Entry::Vacant(v) => {
                    let mut slots: [Option<Vec<f32>>; 4] = Default::default();
                    slots[q_idx] = Some(alpha.clone());
                    v.insert((l.layer, slots));
                }
                Entry::Occupied(mut o) => {
                    let (min_layer, slots) = o.get_mut();
                    if l.layer < *min_layer {
                        *min_layer = l.layer;
                    }
                    // Merge into the same quadrant slot — rare; one LTEX
                    // per quadrant is the vanilla pattern.
                    if let Some(existing) = slots[q_idx].as_mut() {
                        for (dst, src) in existing.iter_mut().zip(alpha.iter()) {
                            *dst = dst.max(*src);
                        }
                    } else {
                        slots[q_idx] = Some(alpha.clone());
                    }
                }
            }
        }
    }

    // Sort by (layer_sort_key, ltex_form_id) for deterministic order.
    let mut sorted: Vec<(u32, u16, PerQuadrantAlpha)> = by_ltex
        .into_iter()
        .map(|(ltex, (layer, slots))| (ltex, layer, slots))
        .collect();
    sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

    // A LAND cell has one mesh but four independently-authored BTXT bases.
    // The material's base texture remains the canonical (first) BTXT; every
    // other base becomes a low-priority splat layer, painted only in its own
    // quadrant. This keeps the floor continuous when a cell crosses dirt,
    // grass, rock, or snow instead of flattening all four quadrants to the
    // first texture we happened to find. The edge is feathered by one LAND
    // vertex (128 BU) so the authored quadrant boundary does not become a
    // hard, camera-visible checkerboard seam.
    let base_transitions = build_base_transition_layers(land, canonical_base_ltex);

    // Bethesda's authoring tool caps at 8 per UESP, but Skyrim
    // routinely ships cells with 9-12 layers and modded content
    // (TTW, Project Nevada, DLC merges) goes higher. The 8-cap is
    // a real shader-side limit — `vertex.rs::Vertex` packs splat
    // weights as 2× RGBA8 = 8 channels per vertex.
    //
    // Pre-fix this dropped the highest `layer` field values, but
    // `layer` is just authoring order — not visual importance. A
    // tiny trim decal authored last would survive while a dominant
    // ground texture authored first got dropped if the budget was
    // hit. Coverage-aware policy picks the 8 layers with the most
    // painted area across all quadrants, then re-sorts for
    // deterministic GPU ordering. #470.
    // At most three non-canonical bases exist, leaving at least five lanes
    // for actual ATXT/VTXT painting. Base transitions go first so later
    // authored masks retain their intended visual precedence.
    let authored_budget = 8 - base_transitions.len();
    if sorted.len() > authored_budget {
        let drop_count = sorted.len() - authored_budget;
        log::warn!(
            "Terrain cell has {} authored splat layers plus {} BTXT transitions; capping authored layers at {} (dropping {} with smallest total coverage). #470",
            sorted.len(),
            base_transitions.len(),
            authored_budget,
            drop_count,
        );
        select_top_by_coverage(&mut sorted, authored_budget);
    }

    let mut layers = Vec::with_capacity(base_transitions.len() + sorted.len());
    for (base_ltex, per_quadrant_alpha) in base_transitions {
        layers.push(resolve_cell_splat_layer(
            ctx,
            tex_provider,
            landscape_textures,
            landscape_texture_sets,
            base_ltex,
            default_land,
            per_quadrant_alpha,
        ));
    }
    for (ltex, _layer_key, per_quadrant_alpha) in sorted {
        layers.push(resolve_cell_splat_layer(
            ctx,
            tex_provider,
            landscape_textures,
            landscape_texture_sets,
            Some(ltex),
            default_land,
            per_quadrant_alpha,
        ));
    }

    CellSplatLayers { layers }
}

/// `None` is the executable's default LAND texture: both an absent BTXT and
/// BTXT form ID zero mean that same built-in material.
fn normalize_base_ltex(ltex: Option<u32>) -> Option<u32> {
    match ltex {
        Some(0) | None => None,
        Some(ltex) => Some(ltex),
    }
}

/// Make one 17×17 alpha grid for a non-canonical BTXT quadrant. Shared edges
/// against a different base get half weight; interpolation then gives each
/// side one LAND-vertex of overlap rather than a binary texture cut.
pub(super) fn base_transition_alpha(q_idx: usize, bases: &[Option<u32>; 4]) -> Vec<f32> {
    let mut alpha = vec![1.0; 17 * 17];
    let base = bases[q_idx];
    let (feather_top, feather_bottom, feather_left, feather_right) = match q_idx {
        0 => (false, bases[2] != base, false, bases[1] != base),
        1 => (false, bases[3] != base, bases[0] != base, false),
        2 => (bases[0] != base, false, false, bases[3] != base),
        3 => (bases[1] != base, false, bases[2] != base, false),
        _ => unreachable!("LAND has exactly four quadrants"),
    };
    for row in 0..17 {
        for col in 0..17 {
            if (feather_top && row == 0)
                || (feather_bottom && row == 16)
                || (feather_left && col == 0)
                || (feather_right && col == 16)
            {
                alpha[row * 17 + col] = 0.5;
            }
        }
    }
    alpha
}

/// Unique non-canonical BTXTs and the quadrants in which they replace the
/// cell base. Deterministic `BTreeMap` ordering keeps vertex lane assignment
/// stable across runs.
fn build_base_transition_layers(
    land: &esm::cell::LandscapeData,
    canonical_base_ltex: Option<u32>,
) -> Vec<(Option<u32>, PerQuadrantAlpha)> {
    let mut bases = [None; 4];
    let mut present = [false; 4];
    for (q_idx, quadrant) in land.quadrants.iter().take(4).enumerate() {
        bases[q_idx] = normalize_base_ltex(quadrant.base);
        present[q_idx] = true;
    }
    base_transition_layers_for_bases(&bases, &present, normalize_base_ltex(canonical_base_ltex))
}

/// Pure base-transition planner, split from LAND traversal so its treatment
/// of absent quadrants and deterministic lane order remains regression-testable
/// without a parsed plugin record. Inputs are normalized base identities:
/// `None` means the executable default land texture.
pub(super) fn base_transition_layers_for_bases(
    bases: &[Option<u32>; 4],
    present: &[bool; 4],
    canonical_base_ltex: Option<u32>,
) -> Vec<(Option<u32>, PerQuadrantAlpha)> {
    use std::collections::BTreeMap;

    let mut transitions: BTreeMap<Option<u32>, PerQuadrantAlpha> = BTreeMap::new();
    for (q_idx, &base_ltex) in bases.iter().enumerate() {
        if !present[q_idx] || base_ltex == canonical_base_ltex {
            continue;
        }
        let slots = transitions.entry(base_ltex).or_default();
        slots[q_idx] = Some(base_transition_alpha(q_idx, bases));
    }
    transitions.into_iter().collect()
}

fn resolve_cell_splat_layer(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    landscape_textures: &HashMap<u32, String>,
    landscape_texture_sets: &HashMap<u32, TextureSet>,
    ltex: Option<u32>,
    default_land: Option<DefaultLandTexture>,
    per_quadrant_alpha: PerQuadrantAlpha,
) -> CellSplatLayer {
    let texture_path = ltex
        .and_then(|id| landscape_textures.get(&id).map(String::as_str))
        .or_else(|| {
            ltex.is_none()
                .then_some(default_land.map(|land| land.diffuse))
                .flatten()
        });
    let diffuse_index = match texture_path {
        Some(path) => resolve_texture(ctx, tex_provider, Some(path)),
        None => {
            if let Some(ltex) = ltex {
                log::debug!(
                    "Terrain splat: LTEX {ltex:08X} not in landscape_textures map; skipping layer"
                );
            }
            0
        }
    };
    let texture_set = ltex.and_then(|id| landscape_texture_sets.get(&id));
    let normal_path = if let Some(default_land) = ltex.is_none().then_some(default_land).flatten() {
        default_land.normal.map(str::to_string)
    } else {
        terrain_layer_normal_path(
            tex_provider,
            texture_set.and_then(|set| set.normal.as_deref()),
            texture_path,
        )
    };
    let specular_path = texture_set
        .and_then(|set| set.specular.as_deref())
        .or_else(|| {
            ltex.is_none()
                .then_some(default_land.and_then(|land| land.specular))
                .flatten()
        });
    CellSplatLayer {
        ltex_form_id: ltex,
        cover_affinity: crate::groundcover_translate::layer_affinity(texture_path.unwrap_or("")),
        diffuse_index,
        normal_index: resolve_optional_terrain_texture(ctx, tex_provider, normal_path.as_deref()),
        specular_index: resolve_optional_terrain_texture(ctx, tex_provider, specular_path),
        per_quadrant_alpha,
    }
}

/// Resolve the authored `LTEX.GNAM` grass association in GPU splat-lane
/// order.
///
/// The terrain shader sees only the texture/weight lanes, while the future
/// authored-card tier needs the originating vegetation form. Keeping this as
/// a pure mapping makes that order explicit and testable without a Vulkan
/// context or an on-disk archive.
///
/// #4642 — `GNAM` is an *array*: 151 of 184 grass-bearing vanilla LTEXs
/// author 2–4 grasses, so each lane carries the full authored species
/// list in authored order (empty = no `LTEX.GNAM` link, or the
/// executable's default land texture). The authored-card consumer tier
/// (#4413) picks its species mix per lane from this list instead of the
/// single last-wins grass the pre-fix map kept.
pub(super) fn authored_grass_for_splat_layers(
    layers: &[CellSplatLayer],
    landscape_grasses: &HashMap<u32, Vec<u32>>,
) -> [Vec<u32>; 8] {
    let mut out: [Vec<u32>; 8] = Default::default();
    for (slot, layer) in out.iter_mut().zip(layers.iter()) {
        if let Some(grasses) = layer
            .ltex_form_id
            .and_then(|ltex_id| landscape_grasses.get(&ltex_id))
        {
            *slot = grasses.clone();
        }
    }
    out
}

/// Optional LAND material roles use handle 0 when absent or unresolved. The
/// diagnostic checkerboard is useful for a missing diffuse surface, but a
/// missing normal/specular contribution must be treated as "no contribution"
/// or it perturbs lighting with magenta placeholder data.
///
/// Both roles this serves — TX01 normal and TX07 specular — are data
/// textures, and upload linear exactly as the same slots do on placed meshes
/// (`map_secondary_texture_handles`). Through the sRGB diffuse path a flat
/// tangent-space texel of 128/255 decoded to ≈0.22 before `* 2 - 1`, tilting
/// every layer normal; every Skyrim (67/67) and FNV (88/88) LTEX texture set
/// authors TX01, and Fallout 4 authors TX07 on 74 of 105
/// (`plugin/examples/landscape_txst_census.rs`).
fn resolve_optional_terrain_texture(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    path: Option<&str>,
) -> u32 {
    let Some(path) = path else {
        return 0;
    };
    let handle = resolve_linear_texture(ctx, tex_provider, Some(path));
    if handle == ctx.texture_registry.fallback() {
        0
    } else {
        handle
    }
}

/// A terrain layer's tangent-space normal: the texture set's authored TX01,
/// else the diffuse's present `_n` sibling. Oblivion's LTEX carries only a
/// diffuse `ICON` and ships every layer normal under that convention
/// (`terrainwetsand02_n.dds` beside `terrainwetsand02.dds`); the sibling is
/// only used when an archive actually holds it (#3551), so games that author
/// normals explicitly are unaffected.
fn terrain_layer_normal_path(
    tex_provider: &TextureProvider,
    authored: Option<&str>,
    diffuse: Option<&str>,
) -> Option<String> {
    authored
        .map(str::to_string)
        .or_else(|| diffuse.and_then(|d| derive_present_normal_map_path(tex_provider, d)))
}

/// In-place coverage-aware selection of the top `max_layers` splat layers from
/// `sorted`. Computes total painted alpha across all quadrants per
/// layer, keeps the highest-coverage layers, then re-sorts those survivors
/// by `(layer, ltex_form_id)` so the GPU vertex-attribute layer-index
/// ordering stays deterministic across runs.
///
/// Pure function — no Vulkan, no allocator — so it's unit-testable
/// without a real cell. #470.
///
/// Precondition: `sorted.len() > max_layers` (called only when the cap is
/// exceeded; the no-op case is gated at the call site).
fn select_top_by_coverage(sorted: &mut Vec<(u32, u16, PerQuadrantAlpha)>, max_layers: usize) {
    // Coverage = sum of alpha values across all painted quadrants.
    // f64 accumulator handles the worst case (4 quadrants × 17×17 =
    // 1156 floats × 1.0 = 1156.0) without precision drift even when
    // many cells stack up across a session.
    sorted.sort_by(|a, b| {
        let ca = total_coverage(&a.2);
        let cb = total_coverage(&b.2);
        // Descending coverage. The parser gates VTXT opacity to finite
        // [0, 1] at the decode choke point (#4484), so NaN cannot reach
        // this comparator any more — but `total_cmp` keeps the order a
        // true total (and `sort_by` panic-free) even if a future decode
        // path regresses that gate: defense in depth, per #4484.
        cb.total_cmp(&ca)
    });
    sorted.truncate(max_layers);
    // Re-sort by (layer, ltex) so the shader's per-layer-index access
    // pattern stays consistent with the no-cap path — pre-fix every
    // caller assumed `(layer ascending, ltex ascending)` ordering.
    sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
}

/// Sum of alpha values across all painted quadrants for one layer.
/// Higher = more painted area = more visually important. Used by the
/// coverage-aware splat cap to drop the least-impactful layers when
/// a cell exceeds the 8-channel vertex budget. #470.
fn total_coverage(per_quadrant_alpha: &PerQuadrantAlpha) -> f64 {
    per_quadrant_alpha
        .iter()
        .filter_map(|q| q.as_ref())
        .flat_map(|q| q.iter())
        .map(|&v| v as f64)
        .sum()
}

/// Map a global 33×33 `(row, col)` to the list of contributing
/// `(quadrant_index, local_row_in_17, local_col_in_17)` tuples. Most
/// vertices belong to exactly one quadrant; edges belong to two, corners
/// to four. Sentinel `0xFF` in slot 0 of `q` means "unused" — caller
/// checks `q < 4` to decide whether to sample.
pub(super) fn quadrant_samples_for_vertex(row: usize, col: usize) -> [(u8, u8, u8); 4] {
    let mut out = [(0xFFu8, 0u8, 0u8); 4];
    let mut n = 0;
    // SW (0): rows [0..=16], cols [0..=16].
    if row <= 16 && col <= 16 {
        out[n] = (0, row as u8, col as u8);
        n += 1;
    }
    // SE (1): rows [0..=16], cols [16..=32]. Local col = col-16.
    if row <= 16 && col >= 16 {
        out[n] = (1, row as u8, (col - 16) as u8);
        n += 1;
    }
    // NW (2): rows [16..=32], cols [0..=16]. Local row = row-16.
    if row >= 16 && col <= 16 {
        out[n] = (2, (row - 16) as u8, col as u8);
        n += 1;
    }
    // NE (3): rows [16..=32], cols [16..=32].
    if row >= 16 && col >= 16 {
        out[n] = (3, (row - 16) as u8, (col - 16) as u8);
        n += 1;
    }
    let _ = n;
    out
}

/// Sample one splat weight for a global vertex by taking the max across
/// every contributing quadrant's alpha grid. Absent quadrants contribute
/// 0. Returns a u8 ready to pack into the vertex attribute.
pub(super) fn splat_weight_for_vertex(layer: &CellSplatLayer, row: usize, col: usize) -> u8 {
    let samples = quadrant_samples_for_vertex(row, col);
    let mut best = 0.0_f32;
    for (q, lr, lc) in samples {
        if q >= 4 {
            continue;
        }
        let Some(ref alpha) = layer.per_quadrant_alpha[q as usize] else {
            continue;
        };
        let local_idx = (lr as usize) * 17 + (lc as usize);
        if local_idx < alpha.len() {
            best = best.max(alpha[local_idx]);
        }
    }
    (best.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Generate a terrain mesh from LAND heightmap data and spawn it as an
/// entity. The mesh participates in the global geometry SSBO so RT
/// reflection / GI rays sample the right vertex data — using
/// `upload_scene_mesh` (not plain `upload`) is mandatory; see #371.
#[allow(clippy::too_many_arguments)]
/// Which splat-layer texture indices [`release_splat_layer_textures`] drops:
/// skips `0` (LTEX not resolved → never acquired) and the registry
/// `fallback` slot (shared placeholder, never per-cell refcounted) — the
/// same skip rule the unload-side `free_terrain_tile` → `push_tex_drop`
/// sweep uses. Pure so the release set is unit-testable without a
/// `VulkanContext`. (#1343)
fn splat_indices_to_release(indices: &[u32], fallback: u32) -> Vec<u32> {
    indices
        .iter()
        .copied()
        .filter(|&i| i != 0 && i != fallback)
        .collect()
}

/// Release the per-layer splat texture refcounts acquired by
/// [`build_cell_splat_layers`]. Called only on `spawn_terrain_mesh`'s
/// early-return paths (no allocator / mesh-upload failure) — the success
/// path hands these handles to a `TerrainTileSlot` whose `free_terrain_tile`
/// drops them on cell unload, so calling this there would double-release.
/// (#1343)
fn release_splat_layer_textures(ctx: &mut VulkanContext, indices: &[u32]) {
    let fallback = ctx.texture_registry.fallback();
    for idx in splat_indices_to_release(indices, fallback) {
        ctx.texture_registry.drop_texture(&ctx.device, idx);
    }
}

// ── LAND value-plausibility guards (EX-10/11 item 5, #2371) ─────────────
//
// Corpus-derived, not guessed: a throwaway probe over real
// Oblivion.esm/Skyrim.esm/FalloutNV.esm LAND data (~83M height samples,
// ~82M VNML samples across the three games) found zero non-finite
// heights and a raw VNML magnitude range of exactly 0.7501–1.4254 —
// identical to four decimal places across all three independently
// authored games, strongly suggesting it's the achievable range of the
// byte-quantization grid for "mostly upward" terrain normals rather than
// an incidental property of any one game's content. Both thresholds
// below sit well outside that measured real-data range so vanilla
// content never trips them, while still catching genuinely malformed or
// adversarial input.

/// Fallback height when a LAND vertex's decoded value is non-finite
/// (NaN/Inf). `parse_land_record`'s VHGT delta-decode starts from a raw
/// `f32` `base_offset` read directly off the wire; a corrupt or
/// adversarial sub-record whose first 4 bytes happen to encode a
/// NaN/Inf bit pattern propagates that through the entire row's delta
/// chain. Never observed in real content, but a NaN/Inf world-space
/// vertex position would poison the mesh's AABB, BLAS build, and
/// collision trimesh — worth clamping even though vanilla content never
/// exercises this path.
const LAND_HEIGHT_FALLBACK: f32 = 0.0;

/// Below this raw (pre-renormalize) VNML magnitude, a decoded normal is
/// treated as degenerate rather than legitimately near-flat authored
/// terrain. Real data never drops below 0.7501 raw magnitude; `0.5`
/// leaves a wide safety margin while still catching genuinely degenerate
/// data — most notably the exact-zero vector (`(128,128,128)` bytes),
/// which the existing `.max(0.001)` renormalize floor already prevents
/// from exploding into a NaN/Inf normal but does not surface as a
/// diagnostic.
const VNML_DEGENERATE_RAW_MAGNITUDE: f32 = 0.5;

/// Sanitize one decoded LAND height sample. Returns the value to use plus
/// whether a fallback was substituted (for the caller's per-cell summary).
fn sanitize_land_height(raw: f32) -> (f32, bool) {
    if raw.is_finite() {
        (raw, false)
    } else {
        (LAND_HEIGHT_FALLBACK, true)
    }
}

/// Mean blade height across the installed ground-cover palette, world units —
/// §12.5's canopy slab thickness for the terrain receiver (#4057).
///
/// Zero when no palette is installed. That is not a fallback thickness, it is
/// the disable: `GpuTerrainTile::canopy_height == 0` makes `triangle.frag`
/// skip the whole canopy-shadow block, which is the correct answer for an
/// interior, a synthetic test cell, or a worldspace whose palette never
/// resolved. Inventing a thickness there would paint a shadow under grass
/// that does not exist.
///
/// The *mean* of each species' height midpoint rather than a max: the slab is
/// a bulk property of the sward, and one tall outlier species in the palette
/// should not deepen the shadow everywhere the short ones grow. Species are
/// unweighted because the per-climate selection weights govern how often a
/// species is *drawn*, not how much of the canopy it is — and a chunk's actual
/// mix is not knowable from here.
fn mean_palette_blade_height(world: &World) -> f32 {
    use byroredux_core::ecs::components::groundcover::GroundCoverPalette;
    let Some(palette) = world.try_resource::<GroundCoverPalette>() else {
        return 0.0;
    };
    if palette.species.is_empty() {
        return 0.0;
    }
    let total: f32 = palette
        .species
        .iter()
        .map(|s| (s.height_range.0 + s.height_range.1) * 0.5)
        .sum();
    let mean = total / palette.species.len() as f32;
    if mean.is_finite() && mean > 0.0 {
        mean
    } else {
        0.0
    }
}

/// Raw (pre-renormalize) magnitude of a decoded VNML sample.
fn vnml_raw_magnitude(nx: f32, ny: f32, nz: f32) -> f32 {
    (nx * nx + ny * ny + nz * nz).sqrt()
}

/// Decode one LAND `VNML` byte triple into a renderer-space (Y-up) unit
/// normal, plus whether the raw sample was degenerate.
///
/// **The bytes are signed `i8`, `127 = +1`.** #4059: this read
/// `(byte - 128) / 127` — unsigned centred at 128 — from the day the terrain
/// path landed until `637b6526`. Under that decode flat ground, authored
/// `(0, 0, 127)`, came out as `(-1.008, -1.008, -0.008)` and normalised to a
/// vector lying almost exactly in the horizontal plane, so *every* exterior
/// terrain vertex normal in the engine pointed sideways.
///
/// It survived because nothing that consumed the normal failed loudly: a
/// sideways normal on flat ground is a plausible shading input, not a crash,
/// and the magnitude guard below is satisfied by both decodes (the wrong one
/// yields 1.425 for flat ground, comfortably above the floor). It measured
/// that the data was not degenerate — which was true — and not that it was
/// being read the right way up.
///
/// So this is a named function rather than three lines inside a 200-line
/// loop: the *direction* is what needs a test, and a test cannot reach an
/// expression buried in a mesh builder. See
/// `flat_ground_vnml_decodes_to_world_up`.
fn decode_vnml_normal(nml: [u8; 3]) -> ([f32; 3], bool) {
    let nx = (nml[0] as i8) as f32 / 127.0;
    let ny = (nml[1] as i8) as f32 / 127.0;
    let nz = (nml[2] as i8) as f32 / 127.0;
    let degenerate = vnml_raw_magnitude(nx, ny, nz) < VNML_DEGENERATE_RAW_MAGNITUDE;
    // Bethesda Z-up -> Y-up via the canonical helper; per-component normalise
    // commutes with the axis swap (#1753).
    let len = (nx * nx + nz * nz + ny * ny).sqrt().max(0.001);
    (zup_to_yup_pos([nx / len, ny / len, nz / len]), degenerate)
}

/// Renderer-side borrows shared by terrain spawning: the Vulkan context,
/// texture provider, the cell's landscape-texture lookup, and the BLAS spec
/// sink the caller batches builds through. Grouped to keep
/// [`spawn_terrain_mesh`]'s argument count in check.
pub(super) struct TerrainSpawnCtx<'a> {
    pub ctx: &'a mut VulkanContext,
    pub tex_provider: &'a TextureProvider,
    pub landscape_textures: &'a HashMap<u32, String>,
    pub landscape_texture_sets: &'a HashMap<u32, TextureSet>,
    /// LTEX.GNAM associations keyed by the LAND layer's LTEX form ID.
    pub landscape_grasses: &'a HashMap<u32, Vec<u32>>,
    pub blas_specs: &'a mut Vec<(u32, u32, u32)>,
    /// Y-up water-plane height for this cell, or `None` when it has none.
    ///
    /// #4054 — resolved by the caller (it already computes exactly this for
    /// the water plane it spawns a few lines later) and threaded in so the
    /// ground-cover `moisture` term has a single insertion site alongside the
    /// affinity table, rather than a second component written from a second
    /// place that could disagree about which cell it belongs to.
    pub water_y: Option<f32>,
    /// Selects the engine's built-in default land texture
    /// ([`DefaultLandTexture::for_game`]) for BTXT-less / BTXT-0 quadrants.
    pub game: esm::reader::GameKind,
}

/// The texture a game's engine paints where LAND authors no base texture
/// (no BTXT, or BTXT form 0). It is not in any plugin: each executable
/// hardcodes it, so this is the one per-game terrain fact that has to live
/// in code. Read from the shipped executables' string tables:
///
/// | Game | Executable string(s) | Archive |
/// |---|---|---|
/// | Oblivion | `Default.DDS` via `%s\Landscape\%s` | Textures - Compressed |
/// | FO3 / FNV | `DirtWasteland01.dds`, `_N` | Textures / Textures2 |
/// | Skyrim | `Dirt02.dds`, `Dirt02_N.dds` (`sDefaultLandDiffuseTexture`) | Textures5 |
/// | FO4 | `Ground\CommonwealthDefault01_d/_n/_s.dds` | Textures1 |
///
/// FO76 and Starfield ship no LAND records, so they have no entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct DefaultLandTexture {
    pub diffuse: &'static str,
    pub normal: Option<&'static str>,
    pub specular: Option<&'static str>,
}

impl DefaultLandTexture {
    pub(super) fn for_game(game: esm::reader::GameKind) -> Option<Self> {
        use esm::reader::GameKind;
        let (diffuse, normal, specular) = match game {
            GameKind::Oblivion => (
                "textures\\landscape\\default.dds",
                Some("textures\\landscape\\default_n.dds"),
                None,
            ),
            GameKind::Fallout3NV => (
                "textures\\landscape\\dirtwasteland01.dds",
                Some("textures\\landscape\\dirtwasteland01_n.dds"),
                None,
            ),
            GameKind::Skyrim => (
                "textures\\landscape\\dirt02.dds",
                Some("textures\\landscape\\dirt02_n.dds"),
                None,
            ),
            GameKind::Fallout4 => (
                "textures\\landscape\\ground\\commonwealthdefault01_d.dds",
                Some("textures\\landscape\\ground\\commonwealthdefault01_n.dds"),
                Some("textures\\landscape\\ground\\commonwealthdefault01_s.dds"),
            ),
            GameKind::Fallout76 | GameKind::Starfield => return None,
        };
        Some(Self {
            diffuse,
            normal,
            specular,
        })
    }
}

pub(super) fn spawn_terrain_mesh(
    world: &mut World,
    spawn: TerrainSpawnCtx,
    grid_x: i32,
    grid_y: i32,
    land: &esm::cell::LandscapeData,
) -> Option<usize> {
    let TerrainSpawnCtx {
        ctx,
        tex_provider,
        landscape_textures,
        landscape_texture_sets,
        landscape_grasses,
        blas_specs,
        water_y,
        game,
    } = spawn;
    // #4052 — both promoted to `byroredux_core::math::coord` so the
    // ground-cover scatter shader reads the same numbers through
    // `shader_constants.glsl` instead of a second hand-typed copy.
    const GRID: usize = LAND_GRID_VERTS;
    const SPACING: f32 = LAND_VERTEX_SPACING; // 128.0

    let origin_x = grid_x as f32 * EXTERIOR_CELL_UNITS;
    let origin_y = grid_y as f32 * EXTERIOR_CELL_UNITS;

    // Collect cell-global splat layers before the vertex loop — we need
    // all 8 resolved before we can pack per-vertex weights. #470.
    let splat_layers = build_cell_splat_layers(
        ctx,
        tex_provider,
        landscape_textures,
        landscape_texture_sets,
        land,
        land.quadrants.iter().find_map(|q| q.base),
        DefaultLandTexture::for_game(game),
    );
    // #1343 — `build_cell_splat_layers` acquired (refcounted) one texture per
    // splat layer above, but those handles only reach an unload-droppable
    // owner at `allocate_terrain_tile` below. If we bail before that (no
    // allocator / mesh-upload failure), release them here so the refcount +
    // bindless slot don't leak. Snapshot the indices now so the release
    // doesn't re-borrow `splat_layers` (still needed by the vertex loop).
    let splat_tex_indices: Vec<u32> = splat_layers
        .layers
        .iter()
        .flat_map(|layer| {
            [
                layer.diffuse_index,
                layer.normal_index,
                layer.specular_index,
            ]
        })
        .collect();

    // Build vertices (33×33 = 1089).
    let mut vertices = Vec::with_capacity(GRID * GRID);
    // EX-10/11 item 5 (#2371) — counted, not logged per-vertex: a fully
    // corrupt file could otherwise flood the log with up to 1089 lines
    // per cell. One summary line after the loop instead.
    let mut nonfinite_heights = 0u32;
    let mut degenerate_normals = 0u32;
    for row in 0..GRID {
        for col in 0..GRID {
            let idx = row * GRID + col;

            // World-space position (Z-up → Y-up conversion via the
            // canonical helper, #1753).
            let bx = origin_x + col as f32 * SPACING;
            let by = origin_y + row as f32 * SPACING;
            let (bz, height_was_nonfinite) = sanitize_land_height(land.heights[idx]);
            if height_was_nonfinite {
                nonfinite_heights += 1;
            }
            let position = zup_to_yup_pos([bx, by, bz]);

            // Normal: signed `i8` VNML — see `decode_vnml_normal` (#4059).
            // The sign error this replaced was found by the ground-cover
            // scatter's per-factor telemetry, which reported the slope gate at
            // a hard 0.000 across 49,152 candidates on Whiterun tundra;
            // probing the stored vertex normal at the centre vertex of 48
            // cells gave `(0.768, -0.012, 0.694)`, the exact signature.
            let normal = if let Some(ref nml) = land.normals {
                let ni = idx * 3;
                let (n, degenerate) = decode_vnml_normal([nml[ni], nml[ni + 1], nml[ni + 2]]);
                if degenerate {
                    degenerate_normals += 1;
                }
                n
            } else {
                [0.0, 1.0, 0.0]
            };

            let color = if let Some(ref vc) = land.vertex_colors {
                let ci = idx * 3;
                [
                    vc[ci] as f32 / 255.0,
                    vc[ci + 1] as f32 / 255.0,
                    vc[ci + 2] as f32 / 255.0,
                ]
            } else {
                [1.0, 1.0, 1.0]
            };

            // Tile the diffuse/splat textures `LAND_TEXTURE_TILES_PER_CELL`
            // times across the cell (REPEAT sampler) so near terrain shows
            // real texel detail instead of one stretched texture. The
            // per-vertex splat WEIGHTS (splat0/splat1 below) are unaffected —
            // they're interpolated attributes, not UV-sampled.
            let uv = [
                col as f32 / 32.0 * LAND_TEXTURE_TILES_PER_CELL,
                (1.0 - row as f32 / 32.0) * LAND_TEXTURE_TILES_PER_CELL,
            ];

            // Pack up to 8 splat weights into 2× RGBA8 unorm (#470). The
            // layer budget is capped at 8 upstream (`build_cell_splat_layers`:
            // base transitions ≤ 4 + authored budget truncation) — the
            // assert pins that contract at the packer so a future budget
            // edit fails here instead of indexing out of bounds.
            debug_assert!(
                splat_layers.layers.len() <= 8,
                "splat packer received {} layers; the 2×RGBA8 budget is 8",
                splat_layers.layers.len()
            );
            let mut splat0 = [0u8; 4];
            let mut splat1 = [0u8; 4];
            for (i, layer) in splat_layers.layers.iter().enumerate() {
                let w = splat_weight_for_vertex(layer, row, col);
                if i < 4 {
                    splat0[i] = w;
                } else {
                    splat1[i - 4] = w;
                }
            }

            vertices.push(Vertex::new_terrain(
                position, color, normal, uv, splat0, splat1,
            ));
        }
    }
    if nonfinite_heights > 0 || degenerate_normals > 0 {
        log::warn!(
            "Cell ({grid_x},{grid_y}): LAND data anomalies — {nonfinite_heights} non-finite \
             height sample(s) clamped to {LAND_HEIGHT_FALLBACK}, {degenerate_normals} \
             degenerate VNML normal(s) (raw magnitude < {VNML_DEGENERATE_RAW_MAGNITUDE})"
        );
    }

    // Indices: 32×32 quads × 2 triangles. The Z-up → Y-up transform
    // negates Z, flipping winding — emit CW so it becomes CCW (Vulkan
    // front face) after the coordinate conversion.
    let mut indices = Vec::with_capacity(32 * 32 * 6);
    for row in 0..32u32 {
        for col in 0..32u32 {
            let tl = row * GRID as u32 + col;
            let tr = tl + 1;
            let bl = (row + 1) * GRID as u32 + col;
            let br = bl + 1;
            indices.push(tl);
            indices.push(tr);
            indices.push(bl);
            indices.push(tr);
            indices.push(br);
            indices.push(bl);
        }
    }

    if ctx.allocator.is_none() {
        // #1343 — release the splat-layer refcounts before bailing; no
        // `TerrainTileSlot` will be allocated to carry them to unload.
        release_splat_layer_textures(ctx, &splat_tex_indices);
        return None;
    }
    let upload_ctx = GpuUploadCtx {
        device: &ctx.device,
        allocator: ctx.allocator.as_ref().unwrap(), // non-None checked just above
        queue: &ctx.graphics_queue,
        command_pool: ctx.transfer_pool,
    };
    let mesh_handle = match ctx.mesh_registry.upload_scene_mesh(
        upload_ctx,
        &vertices,
        &indices,
        ctx.device_caps.ray_query_supported,
        None,
    ) {
        Ok(h) => h,
        Err(e) => {
            // #3406 — `{:#}` keeps anyhow's source chain.
            log::warn!(
                "Failed to upload terrain mesh ({},{}): {:#}",
                grid_x,
                grid_y,
                e
            );
            // #1343 — release the splat-layer refcounts before bailing.
            release_splat_layer_textures(ctx, &splat_tex_indices);
            return None;
        }
    };
    ctx.mesh_registry.note_mesh_provenance(
        mesh_handle,
        byroredux_renderer::MeshUploadSource::Terrain,
        false,
        Some(&format!("land({grid_x},{grid_y})")),
    );

    // Resolve terrain base texture: pick the first available BTXT from
    // any quadrant, resolve via LTEX → texture path. Per-quadrant BTXT
    // disagreement is handled best-effort — the chosen base wins on its
    // own quadrants and the ATXT splat layers paint the rest. See #470
    // (D7 follow-up).
    let base_ltex = land.quadrants.iter().find_map(|q| q.base);
    let default_land = DefaultLandTexture::for_game(game);
    let uses_default_land = matches!(base_ltex, Some(0) | None);
    // #2444 (MAT-D3-02) — the path is retained, not just the handle: it is
    // the classifier input for this tile's canonical `Material` below, so
    // landscape shades by the same rules as the statics standing on it.
    let base_texture_path: Option<&str> = match base_ltex {
        // BTXT with form ID 0 = the engine's built-in default land texture.
        Some(0) | None => default_land.map(|d| d.diffuse),
        Some(ltex_id) => match landscape_textures.get(&ltex_id) {
            Some(path) => Some(path.as_str()),
            None => {
                log::debug!(
                    "Terrain ({},{}): LTEX {:08X} not in landscape_textures map",
                    grid_x,
                    grid_y,
                    ltex_id,
                );
                None
            }
        },
    };
    let tex_handle = match base_texture_path {
        Some(path) => resolve_texture(ctx, tex_provider, Some(path)),
        None => 0,
    };
    let base_texture_set = base_ltex.and_then(|id| landscape_texture_sets.get(&id));
    let (base_normal_path, base_specular_path) = if uses_default_land {
        (
            default_land.and_then(|d| d.normal).map(str::to_string),
            default_land.and_then(|d| d.specular),
        )
    } else {
        (
            terrain_layer_normal_path(
                tex_provider,
                base_texture_set.and_then(|set| set.normal.as_deref()),
                base_texture_path,
            ),
            base_texture_set.and_then(|set| set.specular.as_deref()),
        )
    };
    let base_normal_index =
        resolve_optional_terrain_texture(ctx, tex_provider, base_normal_path.as_deref());
    let base_specular_index =
        resolve_optional_terrain_texture(ctx, tex_provider, base_specular_path);

    // Allocate a terrain tile slot only when the cell actually has splat
    // layers. BTXT-only cells skip this and render with the pre-#470
    // single-texture path for free. The slot is freed in `unload_cell`
    // via `VulkanContext::free_terrain_tile_slot`.
    // #4054 — the density field's two per-cell inputs. Resolved here because
    // this is the only place that has both: the resolved `LTEX` layer order
    // (which the shader's splat lanes are indexed by) and the caller's water
    // height. Unfilled affinity slots take the default rather than zero — an
    // unused layer must not read as a vegetation hole.
    let mut layer_affinity = [crate::groundcover_translate::DEFAULT_AFFINITY; 8];
    for (affinity, layer) in layer_affinity.iter_mut().zip(splat_layers.layers.iter()) {
        *affinity = layer.cover_affinity;
    }
    let authored_grass = authored_grass_for_splat_layers(&splat_layers.layers, landscape_grasses);
    let cover_water_y =
        water_y.unwrap_or(byroredux_core::ecs::components::groundcover::NO_WATER_HEIGHT);
    // #4057 — §12.5's canopy slab thickness for this tile: the resolved
    // palette's mean blade height. Zero when no palette is installed, which
    // disables the terrain half of the canopy shadow rather than guessing a
    // thickness for a sward that is not there.
    //
    // Read once at spawn rather than per frame because a palette resolves once
    // per worldspace entry (§7) and this record's whole job is to carry
    // per-cell constants; a per-frame path would be a second upload of a
    // number that cannot change while the cell is resident.
    let canopy_height = mean_palette_blade_height(world);
    let terrain_tile_index = if !splat_layers.layers.is_empty() {
        let mut diffuse_indices = [0u32; 8];
        let mut normal_indices = [0u32; 8];
        let mut specular_indices = [0u32; 8];
        for (i, layer) in splat_layers.layers.iter().enumerate() {
            diffuse_indices[i] = layer.diffuse_index;
            normal_indices[i] = layer.normal_index;
            specular_indices[i] = layer.specular_index;
        }
        ctx.allocate_terrain_tile(GpuTerrainTile {
            layer_diffuse_index: diffuse_indices,
            layer_normal_index: normal_indices,
            layer_specular_index: specular_indices,
            cover_affinity0: [
                layer_affinity[0],
                layer_affinity[1],
                layer_affinity[2],
                layer_affinity[3],
            ],
            cover_affinity1: [
                layer_affinity[4],
                layer_affinity[5],
                layer_affinity[6],
                layer_affinity[7],
            ],
            // The Y-up counterpart of the Bethesda Z-up row axis — the same
            // `(row 0, col 0)` vertex `TerrainCellOrigin` records below, and
            // the origin `byroSampleTerrain` inverts its grid mapping against.
            cell_origin_xz: [origin_x, -origin_y],
            water_y: cover_water_y,
            canopy_height,
            groundcover_detail_atlas: [0; 4],
        })
    } else {
        None
    };
    if !splat_layers.layers.is_empty() && terrain_tile_index.is_none() {
        log::warn!(
            "Terrain ({grid_x},{grid_y}): terrain material table is full; \
             rendering BTXT only"
        );
        release_splat_layer_textures(ctx, &splat_tex_indices);
    }

    // Queue BLAS build into the caller's batched-spec list — terrain
    // must be in the TLAS for RT shadows/GI, but we collapse N submits
    // into one batched build downstream of the loop. See #382.
    if ctx.device_caps.ray_query_supported {
        blas_specs.push((mesh_handle, vertices.len() as u32, indices.len() as u32));
    }

    let entity = world.spawn();
    world.insert(entity, Transform::IDENTITY);
    world.insert(entity, GlobalTransform::IDENTITY);
    world.insert(entity, MeshHandle(mesh_handle));
    if tex_handle != 0 {
        world.insert(entity, TextureHandle(tex_handle));
    }
    // #2444 (MAT-D3-02) — LAND tiles are drawn surfaces and therefore need a
    // canonical `Material` like every other draw. Without it these fell into
    // `render/static_meshes.rs`'s no-`Material` arm and rendered against
    // hardcoded literals (roughness 0.5) instead of the classifier value the
    // stone/dirt statics standing on the same ground get (0.85) — a visible
    // GGX mismatch at every ground-meets-architecture seam.
    world.insert(
        entity,
        crate::material_translate::translate_texture_only_material(
            base_texture_path.map(str::to_string),
        ),
    );
    if base_normal_index != 0 || base_specular_index != 0 {
        let textures = MaterialTextureSet {
            base_color: tex_handle,
            normal: base_normal_index,
            specular: base_specular_index,
            ..Default::default()
        };
        world.insert(
            entity,
            MaterialTextureHandles {
                textures,
                normal_has_alpha: ctx.texture_registry.handle_has_alpha(base_normal_index),
                // #4423 — synthetic paths bind no tint texture; see the field doc.
                tint_has_alpha: false,
                // #4444 — canonical defaults, not literals (#3073 doctrine:
                // a retune can't leave synthetic paths on the old numbers).
                parallax_height_scale:
                    byroredux_core::ecs::components::material::DEFAULT_PARALLAX_HEIGHT_SCALE,
                parallax_max_passes:
                    byroredux_core::ecs::components::material::DEFAULT_PARALLAX_MAX_PASSES,
            },
        );
    }
    if let Some(slot) = terrain_tile_index {
        world.insert(entity, TerrainTileSlot(slot));
    }
    // #4052 — the cell's Y-up origin, kept so a world-space point can find
    // the terrain instance covering it (`exal-groundcover.md` §11.1's
    // chunk-to-instance association). `origin_y` is the Bethesda Z-up row
    // axis, so its Y-up counterpart is `-origin_y`: this is the (row 0,
    // col 0) vertex, the largest Z in the cell, not the smallest.
    world.insert(
        entity,
        crate::components::TerrainCellOrigin {
            origin_xz: [origin_x, -origin_y],
        },
    );
    world.insert(
        entity,
        crate::components::TerrainCoverInputs {
            layer_affinity,
            water_y: cover_water_y,
            authored_grass,
        },
    );
    // #renderlayer — terrain LAND tiles ARE the architectural floor
    // everything else stacks on. Explicit Architecture (zero bias) so
    // the depth-bias ladder treats them as the canonical baseline,
    // not as defaulted-by-omission entities (which would also yield
    // Architecture but obscures the intent).
    world.insert(
        entity,
        byroredux_core::ecs::components::RenderLayer::Architecture,
    );

    // ...and being the floor, it is also collision. Terrain goes through the
    // exact same collider synthesis as every other static mesh — Gamebryo
    // treated exterior landscape as a separate physics subsystem from
    // interior `bhk` bodies, but that distinction buys us nothing and only
    // created a class of geometry that rendered without being solid.
    //
    // The tile's vertices are already world-space Y-up (`zup_to_yup_pos`
    // above) and the render entity sits at `Transform::IDENTITY`, so the
    // ghost takes an identity placement at unit scale and its collider
    // lands exactly on the drawn surface.
    let positions: Vec<[f32; 3]> = vertices.iter().map(|v| v.position).collect();
    if !crate::cell_loader::spawn::spawn_trimesh_collider_ghost(
        world,
        &positions,
        &indices,
        Vec3::ZERO,
        Quat::IDENTITY,
        1.0,
        None,
    ) {
        log::warn!(
            "Terrain ({},{}): collider synthesis produced no triangles — \
             tile renders but is not solid",
            grid_x,
            grid_y,
        );
    }

    log::debug!(
        "Terrain mesh ({},{}): {} verts, {} tris, height range {:.0}–{:.0}",
        grid_x,
        grid_y,
        vertices.len(),
        indices.len() / 3,
        land.heights.iter().cloned().fold(f32::INFINITY, f32::min),
        land.heights
            .iter()
            .cloned()
            .fold(f32::NEG_INFINITY, f32::max),
    );

    Some(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every Oblivion LTEX must resolve to a texture that ships. Oblivion's
    /// `ICON` is relative to the landscape folder; before that was applied at
    /// the parse boundary every Oblivion terrain layer missed its archive
    /// key and rendered the fallback checkerboard.
    ///
    /// ```sh
    /// cargo test -p byroredux --bin byroredux \
    ///     oblivion_ltex_paths_exist_in_vanilla_archives -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs vanilla Oblivion data on disk"]
    fn oblivion_ltex_paths_exist_in_vanilla_archives() {
        use byroredux_bsa::BsaArchive;
        use std::path::PathBuf;

        let dir = std::env::var("BYROREDUX_OBLIVION_DATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Oblivion/Data")
            });
        let esm_path = dir.join("Oblivion.esm");
        let archives = [
            dir.join("Oblivion - Textures - Compressed.bsa"),
            dir.join("DLCShiveringIsles - Textures.bsa"),
        ];
        if !esm_path.is_file() || !archives.iter().all(|a| a.is_file()) {
            eprintln!("skipping: Oblivion data not found under {}", dir.display());
            return;
        }
        let archives: Vec<BsaArchive> = archives
            .iter()
            .map(|a| BsaArchive::open(a).expect("open bsa"))
            .collect();
        let bytes = std::fs::read(&esm_path).expect("read Oblivion.esm");
        let index = esm::records::parse_esm(&bytes).expect("parse Oblivion.esm");
        let paths = &index.cells.landscape_textures;
        assert!(!paths.is_empty(), "Oblivion.esm yielded no LTEX paths");
        // Authored by vanilla LTEX records but shipped in no archive under
        // any landscape path (a basename search finds only the unrelated
        // `textures\rocks\chrock01.dds`) — data defects, not resolution bugs.
        const UNSHIPPED: [&str; 3] = [
            "landscape\\terrainanvilgrass01.dds",
            "landscape\\chrock01.dds",
            "landscape\\oblivion\\terrainhdoblivionevilsymbol01.dds",
        ];
        let mut missing: Vec<String> = paths
            .values()
            .filter(|path| {
                let key = format!("textures\\{path}");
                !archives.iter().any(|a| a.contains(&key))
            })
            .map(|path| path.to_ascii_lowercase())
            .collect();
        missing.sort();
        let mut expected: Vec<String> = UNSHIPPED.iter().map(|p| p.to_string()).collect();
        expected.sort();
        assert_eq!(
            missing,
            expected,
            "Oblivion LTEX paths missing from every vanilla archive changed (of {})",
            paths.len()
        );
        eprintln!(
            "verified {} Oblivion LTEX paths ({} known unshipped)",
            paths.len(),
            UNSHIPPED.len()
        );
    }

    /// Every LAND-carrying game gets a default land texture; the two games
    /// that ship no LAND records get none rather than a borrowed one.
    #[test]
    fn default_land_texture_covers_every_land_game() {
        use esm::reader::GameKind;
        for game in [
            GameKind::Oblivion,
            GameKind::Fallout3NV,
            GameKind::Skyrim,
            GameKind::Fallout4,
        ] {
            let tex = DefaultLandTexture::for_game(game)
                .unwrap_or_else(|| panic!("{game:?} has LAND but no default texture"));
            for path in [Some(tex.diffuse), tex.normal, tex.specular]
                .into_iter()
                .flatten()
            {
                assert!(
                    path.starts_with("textures\\landscape\\") && path.ends_with(".dds"),
                    "{game:?}: {path} is not a textures\\landscape DDS key"
                );
            }
        }
        assert_eq!(DefaultLandTexture::for_game(GameKind::Fallout76), None);
        assert_eq!(DefaultLandTexture::for_game(GameKind::Starfield), None);
    }

    /// Each default land texture must exist, by exact key, in its game's
    /// vanilla texture archives. The pre-fix constant
    /// (`textures\\landscape\\dirt02.dds` for every game) exists only in
    /// Skyrim, so every other game's BTXT-less terrain — including whole
    /// lake and sea beds — rendered the fallback checkerboard. A
    /// source-only test cannot know whether an archive key is real.
    ///
    /// Gated on game data; each game is skipped when its archives are not on
    /// disk. Run with:
    /// ```sh
    /// cargo test -p byroredux --bin byroredux \
    ///     default_land_textures_exist_in_vanilla_archives -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs vanilla game texture archives on disk"]
    fn default_land_textures_exist_in_vanilla_archives() {
        use byroredux_bsa::{Ba2Archive, BsaArchive};
        use esm::reader::GameKind;
        use std::path::PathBuf;

        const STEAM: &str = "/mnt/data/SteamLibrary/steamapps/common";
        let games: [(GameKind, &str, &str, &[&str]); 5] = [
            (
                GameKind::Oblivion,
                "BYROREDUX_OBLIVION_DATA",
                "Oblivion/Data",
                &["Oblivion - Textures - Compressed.bsa"],
            ),
            (
                GameKind::Fallout3NV,
                "BYROREDUX_FO3_DATA",
                "Fallout 3 goty/Data",
                &["Fallout - Textures.bsa"],
            ),
            (
                GameKind::Fallout3NV,
                "BYROREDUX_FNV_DATA",
                "Fallout New Vegas/Data",
                &["Fallout - Textures.bsa", "Fallout - Textures2.bsa"],
            ),
            (
                GameKind::Skyrim,
                "BYROREDUX_SKYRIMSE_DATA",
                "Skyrim Special Edition/Data",
                &[
                    "Skyrim - Textures0.bsa",
                    "Skyrim - Textures1.bsa",
                    "Skyrim - Textures2.bsa",
                    "Skyrim - Textures3.bsa",
                    "Skyrim - Textures4.bsa",
                    "Skyrim - Textures5.bsa",
                    "Skyrim - Textures6.bsa",
                    "Skyrim - Textures7.bsa",
                    "Skyrim - Textures8.bsa",
                ],
            ),
            (
                GameKind::Fallout4,
                "BYROREDUX_FO4_DATA",
                "Fallout 4/Data",
                &["Fallout4 - Textures1.ba2"],
            ),
        ];

        let mut checked = 0usize;
        for (game, env_var, default_dir, archives) in games {
            let dir = std::env::var(env_var)
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(STEAM).join(default_dir));
            let paths: Vec<PathBuf> = archives.iter().map(|a| dir.join(a)).collect();
            if !paths.iter().all(|p| p.is_file()) {
                eprintln!(
                    "skipping {env_var}: archives not found under {}",
                    dir.display()
                );
                continue;
            }
            let mut lookups: Vec<Box<dyn Fn(&str) -> bool>> = Vec::new();
            for path in &paths {
                if path
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("ba2"))
                {
                    let archive = Ba2Archive::open(path).expect("open ba2");
                    lookups.push(Box::new(move |key| archive.contains(key)));
                } else {
                    let archive = BsaArchive::open(path).expect("open bsa");
                    lookups.push(Box::new(move |key| archive.contains(key)));
                }
            }
            let tex = DefaultLandTexture::for_game(game).expect("LAND game");
            for key in [Some(tex.diffuse), tex.normal, tex.specular]
                .into_iter()
                .flatten()
            {
                assert!(
                    lookups.iter().any(|contains| contains(key)),
                    "{env_var}: default land texture {key} is not in {archives:?}"
                );
                checked += 1;
            }
        }
        eprintln!("verified {checked} default land texture keys against real archives");
    }

    /// #1343 / D3-02 — on a `spawn_terrain_mesh` early return (no allocator /
    /// mesh-upload failure) the acquired splat-layer textures must be
    /// released, but `0` (unresolved LTEX, never acquired) and the registry
    /// `fallback` slot must be skipped so we don't over-release a shared
    /// slot. Same skip rule as the unload-side `free_terrain_tile` sweep.
    #[test]
    fn splat_release_skips_zero_and_fallback() {
        let fallback = 99u32;
        // 8 layers: real handles, one unresolved (0), one fallback.
        let indices = [10u32, 0, 11, fallback, 12, 0, 13, fallback];
        let mut got = splat_indices_to_release(&indices, fallback);
        got.sort_unstable();
        assert_eq!(
            got,
            vec![10, 11, 12, 13],
            "only real, non-fallback splat handles are released"
        );
    }

    /// A BTXT-only cell (no ATXT splat layers) acquired nothing → releases
    /// nothing on an early return.
    #[test]
    fn splat_release_empty_is_empty() {
        assert!(splat_indices_to_release(&[], 99).is_empty());
        assert!(splat_indices_to_release(&[0, 0, 0, 0], 99).is_empty());
    }

    // ── LAND value-plausibility guards (EX-10/11 item 5, #2371) ─────────

    #[test]
    fn sanitize_land_height_passes_finite_values_through_unchanged() {
        assert_eq!(sanitize_land_height(1234.5), (1234.5, false));
        assert_eq!(sanitize_land_height(-8192.0), (-8192.0, false));
        assert_eq!(sanitize_land_height(0.0), (0.0, false));
    }

    #[test]
    fn sanitize_land_height_clamps_nan_and_infinity() {
        assert_eq!(sanitize_land_height(f32::NAN), (LAND_HEIGHT_FALLBACK, true));
        assert_eq!(
            sanitize_land_height(f32::INFINITY),
            (LAND_HEIGHT_FALLBACK, true)
        );
        assert_eq!(
            sanitize_land_height(f32::NEG_INFINITY),
            (LAND_HEIGHT_FALLBACK, true)
        );
    }

    #[test]
    fn vnml_raw_magnitude_matches_measured_real_corpus_range() {
        // Real Oblivion.esm/Skyrim.esm/FalloutNV.esm VNML data (~82M
        // samples total) never drops below 0.7501 raw magnitude — a
        // representative near-vertical authored normal should land
        // comfortably inside that range, well above the degenerate floor.
        let mag = vnml_raw_magnitude(0.0, 1.0, 0.05);
        assert!(mag > VNML_DEGENERATE_RAW_MAGNITUDE);
        assert!(mag >= 0.75, "expected near-unit magnitude, got {mag}");
    }

    #[test]
    fn vnml_raw_magnitude_flags_the_exact_zero_vector_as_degenerate() {
        // Byte triple (0, 0, 0) decodes to exactly (0.0, 0.0, 0.0) under the
        // signed reading — the one input the existing `.max(0.001)`
        // renormalize floor exists to survive without exploding into a
        // NaN/Inf normal. (Pre-#4059 this comment named (128,128,128), which
        // was the zero of the *unsigned* decode; under the signed one those
        // bytes are (-128,-128,-128), a full-magnitude sample.)
        let mag = vnml_raw_magnitude(0.0, 0.0, 0.0);
        assert_eq!(mag, 0.0);
        assert!(mag < VNML_DEGENERATE_RAW_MAGNITUDE);
        assert!(
            decode_vnml_normal([0, 0, 0]).1,
            "byte zero is the degenerate sample"
        );
        assert!(
            !decode_vnml_normal([128, 128, 128]).1,
            "(128,128,128) is the unsigned zero, not the signed one — it is a \
             full-magnitude sample and must not be reported as degenerate"
        );
    }

    /// #4059. The corpus magnitude guard above cannot see a sign error: the
    /// wrong decode gives flat ground a raw magnitude of 1.425, comfortably
    /// inside the measured range. Only the *direction* separates the two
    /// readings, so that is what this pins.
    #[test]
    fn flat_ground_vnml_decodes_to_world_up() {
        // Authored flat ground is (0, 0, 127): Z-up +Z at full scale.
        let (n, degenerate) = decode_vnml_normal([0, 0, 127]);
        assert!(!degenerate);
        assert!(
            n[1] > 0.999,
            "flat ground must decode to renderer +Y, got {n:?} — the unsigned \
             reading gives a near-horizontal normal here (#4059)"
        );
        // The unsigned decode, for the record: (0,0,127) -> (-1.008, -1.008,
        // -0.008), whose Y-up form has a Y component near zero.
        let unsigned = |b: u8| (f32::from(b) - 128.0) / 127.0;
        let (ux, uy, uz) = (unsigned(0), unsigned(0), unsigned(127));
        let ulen = (ux * ux + uy * uy + uz * uz).sqrt();
        let wrong = zup_to_yup_pos([ux / ulen, uy / ulen, uz / ulen]);
        assert!(
            wrong[1].abs() < 0.05,
            "the pre-#4059 decode is supposed to be the near-horizontal one; \
             got {wrong:?}"
        );
    }

    /// The other half of "signed": a negative authored component has to stay
    /// negative. A decode that is signed but scaled wrong would still pass the
    /// flat-ground test above.
    #[test]
    fn vnml_negative_components_survive_the_decode() {
        // (-127, 0, 0) in Z-up is fully -X, which the Y-up flip leaves on -X.
        let (n, _) = decode_vnml_normal([0x81, 0, 0]);
        assert!(n[0] < -0.999, "expected -X, got {n:?}");
        // A 45 degree slope: Z-up (0, -90, 90) is unit-ish and must keep both
        // signs through the axis swap rather than folding to a positive pair.
        let (slope, _) = decode_vnml_normal([0, 0xA6, 90]);
        assert!(slope[1] > 0.6, "up component lost: {slope:?}");
        assert!(slope[2].abs() > 0.6, "horizontal component lost: {slope:?}");
    }

    /// Build a layer tuple with one painted quadrant filled to a
    /// constant alpha value. `alpha = 0.0` produces a zero-coverage
    /// layer; `alpha = 1.0` produces full coverage in that quadrant.
    fn layer(
        ltex: u32,
        layer_field: u16,
        q_idx: usize,
        alpha: f32,
    ) -> (u32, u16, PerQuadrantAlpha) {
        let mut slots: PerQuadrantAlpha = Default::default();
        slots[q_idx] = Some(vec![alpha; 17 * 17]);
        (ltex, layer_field, slots)
    }

    #[test]
    fn coverage_drops_zero_paint_layers_first() {
        // 12 layers — 8 with full coverage (alpha=1.0) and 4 with
        // zero coverage (alpha=0.0). Coverage-aware policy should
        // drop the 4 zero-coverage layers and keep the 8 painted
        // ones, regardless of the `layer_field` (authoring order).
        //
        // Pre-fix this dropped by `layer_field` ascending — the 4
        // zero-coverage layers at field=8..11 would have been kept
        // and 4 of the painted layers at field=4..7 would have been
        // dropped, producing a visually-broken terrain cell.
        let mut sorted: Vec<(u32, u16, PerQuadrantAlpha)> = Vec::new();
        for i in 0..8u32 {
            // High-coverage layers, low `layer_field` values.
            sorted.push(layer(0xC000_0000 + i, i as u16, (i % 4) as usize, 1.0));
        }
        for i in 0..4u32 {
            // Zero-coverage layers, high `layer_field` values — pre-fix
            // these would have survived the truncation.
            sorted.push(layer(
                0xD000_0000 + i,
                (8 + i) as u16,
                (i % 4) as usize,
                0.0,
            ));
        }
        // Sort matches the call-site state at entry to select_top_by_coverage.
        sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        assert_eq!(sorted.len(), 12);

        select_top_by_coverage(&mut sorted, 8);

        assert_eq!(sorted.len(), 8);
        // Every survivor should be a high-coverage layer (LTEX 0xC0..).
        for (ltex, _, _) in &sorted {
            assert!(
                *ltex >= 0xC000_0000 && *ltex < 0xD000_0000,
                "zero-coverage layer 0x{:08X} survived the cap",
                ltex
            );
        }
    }

    #[test]
    fn coverage_keeps_dominant_layers_drops_trim() {
        // Realistic Skyrim pattern: 4 dominant ground textures
        // (grass, dirt, rock, snow) at high coverage + 6 trim
        // decorations (paths, decals, edge blends) at low coverage.
        // Total = 10 layers; 2 must drop. Expect both dropped to
        // be from the trim group.
        let mut sorted: Vec<(u32, u16, PerQuadrantAlpha)> = Vec::new();
        // 4 dominant: full coverage in one quadrant each.
        for i in 0..4u32 {
            sorted.push(layer(0xA000_0000 + i, i as u16, (i % 4) as usize, 1.0));
        }
        // 6 trim: 10% coverage in one quadrant each.
        for i in 0..6u32 {
            sorted.push(layer(
                0xB000_0000 + i,
                (4 + i) as u16,
                (i % 4) as usize,
                0.1,
            ));
        }
        sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
        assert_eq!(sorted.len(), 10);

        select_top_by_coverage(&mut sorted, 8);

        assert_eq!(sorted.len(), 8);
        // All 4 dominant layers must survive.
        let surviving_dominant = sorted
            .iter()
            .filter(|(ltex, _, _)| *ltex >= 0xA000_0000 && *ltex < 0xB000_0000)
            .count();
        assert_eq!(surviving_dominant, 4, "a dominant layer was dropped");
        // 4 of the 6 trim layers should survive (the policy is
        // order-insensitive within the trim group since they all
        // have identical coverage — any 4 is correct).
        let surviving_trim = sorted
            .iter()
            .filter(|(ltex, _, _)| *ltex >= 0xB000_0000 && *ltex < 0xC000_0000)
            .count();
        assert_eq!(surviving_trim, 4, "wrong number of trim layers survived");
    }

    #[test]
    fn output_is_resorted_by_layer_after_truncation() {
        // After the coverage-based selection, the survivors must be
        // re-sorted by (layer_field, ltex) for deterministic GPU
        // ordering — the shader's per-layer-index access pattern
        // expects this. Verifies the second sort runs.
        let mut sorted: Vec<(u32, u16, PerQuadrantAlpha)> = Vec::new();
        // 9 layers, distinct layer_field values 100..108, all full
        // coverage. The selection by coverage is a tie — every
        // layer has identical coverage — so the dropped one is
        // implementation-defined, but the survivors must be sorted
        // ascending by layer_field on output.
        for i in 0..9u16 {
            sorted.push(layer(0xE000_0000 + i as u32, 100 + i, 0, 1.0));
        }
        sorted.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

        select_top_by_coverage(&mut sorted, 8);

        assert_eq!(sorted.len(), 8);
        // Verify sorted ascending by layer_field.
        for w in sorted.windows(2) {
            assert!(
                w[0].1 < w[1].1 || (w[0].1 == w[1].1 && w[0].0 < w[1].0),
                "output not sorted by (layer_field, ltex_form_id)"
            );
        }
    }

    /// #4496 — the packer's `splat1[i - 4]` indexing is only safe because
    /// the layer budget is capped at 8: base transitions (at most one per
    /// quadrant slot, so ≤ 4) plus authored layers truncated to
    /// `8 - transitions.len()`. Pin both premises at the pure planner for
    /// the worst-case fully-mixed cell — four distinct BTXT bases, none
    /// canonical — so a future "fifth base transition" or budget edit
    /// fails here (and the packer's `debug_assert!`) instead of indexing
    /// out of bounds in every exterior cell.
    #[test]
    fn splat_layer_budget_never_exceeds_the_packer_bound() {
        let bases = [Some(0x100u32), Some(0x200), Some(0x300), Some(0x400)];
        let present = [true; 4];
        let transitions = base_transition_layers_for_bases(&bases, &present, Some(0xDEAD));
        assert_eq!(
            transitions.len(),
            4,
            "four distinct non-canonical bases must yield four transitions"
        );

        // The budget arithmetic exactly as `build_cell_splat_layers` runs
        // it, for arbitrarily many authored layers.
        let authored_budget = 8 - transitions.len();
        let capped = 20usize.min(authored_budget);
        assert!(
            transitions.len() + capped <= 8,
            "layer count {} exceeds the 2×RGBA8 packer budget of 8",
            transitions.len() + capped
        );
    }

    #[test]
    fn total_coverage_sums_across_quadrants() {
        // Layer painted in 3 of 4 quadrants — 0.5 alpha each, 17×17
        // cells per quadrant. Expected = 3 × 17 × 17 × 0.5 = 433.5.
        let mut slots: PerQuadrantAlpha = Default::default();
        slots[0] = Some(vec![0.5; 17 * 17]);
        slots[1] = Some(vec![0.5; 17 * 17]);
        slots[3] = Some(vec![0.5; 17 * 17]);
        // slots[2] = None — unpainted quadrant contributes zero.

        let cov = total_coverage(&slots);
        let expected = 3.0 * (17 * 17) as f64 * 0.5;
        assert!(
            (cov - expected).abs() < 1e-9,
            "expected {expected}, got {cov}"
        );
    }

    #[test]
    fn total_coverage_zero_for_empty_layer() {
        let slots: PerQuadrantAlpha = Default::default();
        assert_eq!(total_coverage(&slots), 0.0);
    }

    /// #4052 — `TerrainCellOrigin` must be the Y-up XZ of the (row 0, col 0)
    /// vertex, which means `-origin_y`, not `origin_y`.
    ///
    /// The sign is the one mistake in this mapping that still produces
    /// plausible terrain: a sampler handed `+origin_y` reads the cell's rows
    /// mirrored, and every consumer downstream (the ground-cover density
    /// field, the blade orientation) gets confidently wrong answers rather
    /// than an error. So this walks the same `zup_to_yup_pos` the vertex loop
    /// uses and asserts the component agrees with the vertex it claims to
    /// name.
    #[test]
    fn terrain_cell_origin_is_the_row_zero_vertex_in_yup() {
        for (grid_x, grid_y) in [(0, 0), (3, -7), (-12, 5), (41, 41)] {
            let origin_x = grid_x as f32 * EXTERIOR_CELL_UNITS;
            let origin_y = grid_y as f32 * EXTERIOR_CELL_UNITS;
            // The component, as `spawn_terrain_mesh` writes it.
            let component = [origin_x, -origin_y];
            // The vertex it claims to name: row 0, col 0, height irrelevant.
            let vertex = zup_to_yup_pos([origin_x, origin_y, 0.0]);
            assert_eq!(
                component,
                [vertex[0], vertex[2]],
                "TerrainCellOrigin disagrees with the (row 0, col 0) vertex at                  grid ({grid_x}, {grid_y})"
            );
            // …and it is the LARGEST z in the cell, since rows run toward -Z.
            let last_row = zup_to_yup_pos([
                origin_x,
                origin_y + (LAND_GRID_VERTS - 1) as f32 * LAND_VERTEX_SPACING,
                0.0,
            ]);
            assert!(
                component[1] > last_row[2],
                "row 0 must be the largest Z in the cell; rows advance toward -Z"
            );
        }
    }
}
