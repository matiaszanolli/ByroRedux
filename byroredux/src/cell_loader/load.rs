//! Interior cell loading + lighting resolution.
//!
//! `load_cell_with_masters` is the interior entry point — drives the
//! REFR walk, BLAS build, water plane, lighting resolution, and
//! cell-root stamping. The exterior entry point lives in
//! [`super::exterior`] and shares this module's `stamp_cell_root`
//! helper. `CellLoadResult` is the shape returned to the engine
//! caller.

use byroredux_core::ecs::storage::EntityId;
use byroredux_core::ecs::{CellRoot, World};
use byroredux_core::math::Vec3;
use byroredux_plugin::esm;
use byroredux_renderer::VulkanContext;

use crate::asset_provider::{MaterialProvider, TextureProvider};
use crate::components::CellRootIndex;

use super::load_order::parse_record_indexes_in_load_order;
use super::references::load_references;
use super::water;

/// Result of loading a cell.
#[allow(dead_code)]
pub struct CellLoadResult {
    pub cell_name: String,
    pub entity_count: usize,
    /// Number of **mesh-bearing entities** spawned by this cell load —
    /// i.e. the count of `world.insert(entity, MeshHandle(...))` calls
    /// in `spawn_placed_instances` for this cell's references. Stable
    /// across repeat loads of the same cell (unlike the NIF-parse
    /// cache, which reports 0 on a second load even though the cell
    /// still spawns all its entities) and matches the per-cell
    /// draw-count. Useful as a telemetry baseline. See #477
    /// (FNV-3-L2). Bench-pinned counts for specific cells live in
    /// `docs/audits/` — don't pin a number here, since dispatch
    /// generation drift over time will desync it (see #822).
    pub mesh_count: usize,
    /// Bounding box center of all placed objects (Y-up, for camera positioning).
    pub center: Vec3,
    /// Interior cell lighting (ambient + directional).
    pub lighting: Option<byroredux_plugin::esm::cell::CellLighting>,
    /// Owner token for every entity this load produced. Pass to
    /// [`unload_cell`] to tear the cell down (despawn entities + free
    /// mesh/BLAS/texture resources). See #372.
    pub cell_root: EntityId,
    // Pre-#860 this struct also carried `weather: Option<WeatherRecord>`
    // and `climate: Option<ClimateRecord>` fields. The producer
    // (`load_cell_with_masters`) is the interior-only entry point and
    // unconditionally emitted `None` for both, and no consumer ever
    // read them — exterior weather flows through
    // `apply_worldspace_weather()` reading `wctx.default_weather`
    // directly off the [`ExteriorWorldContext`]. Re-added at the
    // point a real consumer (e.g. interior scripted-weather override)
    // appears, with a populated producer to match.
}

pub(crate) fn stamp_cell_root(
    world: &mut World,
    cell_root: EntityId,
    first: EntityId,
    last: EntityId,
) {
    world.insert(cell_root, CellRoot(cell_root));
    for eid in first..last {
        // `insert` is overwrite-safe; every spawned entity in
        // `first..last` gets a `CellRoot` row regardless of whether
        // it received any other components. The unload path filters
        // `CellRoot` storage by `cell_root`, so this stamp is what
        // makes the entity reachable from `unload_cell` (post-#791,
        // also via the `CellRootIndex` populated below).
        world.insert(eid, CellRoot(cell_root));
    }
    // Populate the inverted index. Production always registers the
    // resource at App init (`main.rs:258`); test fixtures that drive
    // stamp_cell_root through reduced setups may not. Skip silently in
    // that case — `unload_cell` will also skip and fall through to an
    // empty victim set, which is the same observable behaviour the
    // unload path had pre-#791 for cells whose query found no rows.
    if let Some(mut idx) = world.try_resource_mut::<CellRootIndex>() {
        let entry = idx.map.entry(cell_root).or_insert_with(Vec::new);
        let span = last.saturating_sub(first) as usize;
        entry.reserve(span + 1);
        // `extend` over a known-size `Copy` range lets the compiler
        // inline as a typed memcpy and elide per-push bounds checks
        // — same final layout as the prior per-eid push loop. #885.
        entry.extend(first..last);
        entry.push(cell_root);
    }
}

/// Load an interior cell with explicit master plugins.
///
/// `masters` is an ordered list of master ESM paths (base game first,
/// then any required DLC masters); `esm_path` is the main plugin
/// being loaded (DLC or mod). Each plugin's FormIDs are remapped to
/// global load-order indices before being merged into a single cell
/// index, so cross-plugin REFRs (e.g. a Dawnguard interior placing a
/// Skyrim.esm STAT) resolve correctly.
///
/// Pre-#561 the cell loader only accepted a single ESM and silently
/// rendered empty interiors when REFRs pointed into a missing master.
/// This entry point closes the audit's SK-D6-01 gap by threading
/// `parse_esm_with_load_order` through the cell-loader pipeline.
///
/// On unresolved REFR `base_form_id` lookups, the warning summary now
/// names the missing plugin so the failure mode is diagnosable
/// instead of silent. See M46.0 / #561.
#[tracing::instrument(
    name = "load_cell_with_masters",
    skip_all,
    fields(esm = esm_path, cell = cell_editor_id, master_count = masters.len()),
)]
pub fn load_cell_with_masters(
    masters: &[String],
    esm_path: &str,
    cell_editor_id: &str,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mat_provider: Option<&mut MaterialProvider>,
) -> anyhow::Result<CellLoadResult> {
    // Mark the high-water entity id before loading. Everything spawned
    // by this load (including the designated cell_root at the end) gets
    // CellRoot stamped on it for later unload. See #372.
    let first_entity = world.next_entity_id();

    // 1. Parse the ESM(s) into a single merged cell index. Empty
    //    `masters` list reduces to single-plugin behaviour (FormIDs
    //    pass through unchanged via the remap's self-reference path).
    let plugin_paths: Vec<&str> = masters
        .iter()
        .map(|s| s.as_str())
        .chain(std::iter::once(esm_path))
        .collect();
    // SK-D6-02 / #566 — use the full-record parser so the LGTM
    // lighting-template fallback can resolve through
    // `EsmIndex.lighting_templates`. Pre-#566 this path only loaded the
    // cell index, which couldn't see LGTM records and silently dropped
    // the XCLL-absent fallback. The cost is bounded: ~1 s extra to
    // parse the surrounding categories on FNV / Skyrim, paid once per
    // cell load.
    let (index, load_order) = parse_record_indexes_in_load_order(&plugin_paths)?;

    // 2. Find the cell.
    let cell_key = cell_editor_id.to_ascii_lowercase();
    let cell = index.cells.cells.get(&cell_key).ok_or_else(|| {
        // List available cells for debugging.
        let available: Vec<&str> = index
            .cells
            .cells
            .values()
            .take(20)
            .map(|c| c.editor_id.as_str())
            .collect();
        anyhow::anyhow!(
            "Cell '{}' not found. {} interior cells available. Examples: {:?}",
            cell_editor_id,
            index.cells.cells.len(),
            available,
        )
    })?;

    log::info!(
        "Loading cell '{}' (form {:08X}): {} placed references",
        cell.editor_id,
        cell.form_id,
        cell.references.len(),
    );

    // 3. Load placed references.
    let result = load_references(
        &cell.references,
        &index.cells,
        &index,
        &index.npcs,
        &index.races,
        index.game,
        world,
        ctx,
        tex_provider,
        mat_provider,
        &cell.editor_id,
        &load_order,
    );

    // 3a. Interior water plane from XCLW / XCWT — flooded ruins,
    // sewers, named indoor pools. The cell parser captured the
    // height directly; the water material comes from the global
    // WATR record table.
    if let Some(water_height) = cell.water_height {
        let mut _blas_dummy: Vec<(u32, u32, u32)> = Vec::new();
        let _ = water::spawn_water_plane(
            world,
            ctx,
            tex_provider,
            &index.waters,
            water_height,
            cell.water_type_form,
            // Interior cells use a local-origin frame; the cell's
            // reference bounds are not yet aggregated at this site
            // (the cell loader runs reference loading before bounds
            // collection lands). For MVP we centre the plane on the
            // world origin — references in flooded interiors are
            // typically authored around the origin too. Improving
            // the centroid is a separate audit-pass once the cell
            // root's WorldBound aggregation is plumbed through.
            (0.0, 0.0),
            water::default_interior_half_extent(),
            &mut _blas_dummy,
        );
    }

    // SK-D6-02 / #566 — LGTM lighting-template fallback. Vanilla
    // Skyrim ships interior cells (Solitude inn cluster, Dragonsreach
    // throne room, Markarth cells) that omit XCLL and rely on this
    // template chain. Pre-#566 the LTMP FormID was unparsed, so the
    // fallback never fired and these cells rendered with the engine
    // default ambient.
    let resolved_lighting = resolve_cell_lighting(cell, &index);
    log::info!("Cell lighting: {:?}", resolved_lighting);

    // Reserve a dedicated root entity and stamp CellRoot on every
    // entity in [first_entity, last_entity). The stamp is sparse-set
    // backed, so entities that never received any component simply
    // don't show up in the CellRoot storage — fine.
    let last_entity = world.next_entity_id();
    let cell_root = world.spawn();
    stamp_cell_root(world, cell_root, first_entity, last_entity);

    Ok(CellLoadResult {
        cell_name: cell.editor_id.clone(),
        entity_count: result.entity_count,
        mesh_count: result.mesh_count,
        center: result.center,
        lighting: resolved_lighting,
        cell_root,
    })
}

/// Resolve a cell's lighting against the ESM index, applying the
/// XCLL → LTMP → engine-default fallback chain (SK-D6-02 / #566).
///
/// 1. **Explicit XCLL wins.** Every Skyrim+/FNV/FO3/Oblivion CELL that
///    authors `XCLL` returns its parsed `CellLighting` verbatim — the
///    template path is never consulted.
/// 2. **LGTM template synthesises a CellLighting.** When the cell has
///    no XCLL but its `LTMP` resolves through `index.lighting_templates`,
///    the LgtmRecord's ambient / directional / fog scalars project into
///    a fresh `CellLighting`. Fields the LGTM stub doesn't carry
///    (directional_rotation, ambient cube, specular) stay at their
///    pre-XCLL defaults — directional_rotation `[0, 0]` matches a
///    sun-from-+X cell origin and the Skyrim-extended optionals stay
///    `None` (the renderer falls back to legacy single-color ambient
///    when they're absent, the same path FO3/FNV cells take).
/// 3. **No XCLL and no resolvable LGTM** → `None` (engine default).
pub(crate) fn resolve_cell_lighting(
    cell: &esm::cell::CellData,
    index: &esm::records::EsmIndex,
) -> Option<esm::cell::CellLighting> {
    if let Some(lit) = cell.lighting.clone() {
        return Some(lit);
    }
    let template_form = cell.lighting_template_form?;
    let template = index.lighting_templates.get(&template_form)?;
    Some(esm::cell::CellLighting {
        ambient: template.ambient,
        directional_color: template.directional,
        // LGTM doesn't carry directional rotation. Sun-from-+X origin
        // is what FO3/FNV cells defaulted to before #379 added explicit
        // rotation parsing — same fallback shape here.
        directional_rotation: [0.0, 0.0],
        fog_color: template.fog_color,
        fog_near: template.fog_near,
        fog_far: template.fog_far,
        directional_fade: template.directional_fade,
        fog_clip: template.fog_clip,
        fog_power: template.fog_power,
        // Skyrim extended fields (ambient cube, specular, light fade,
        // fog far color) ride on the 92-byte XCLL only. The current
        // LgtmRecord stub doesn't extract them; future LGTM expansion
        // can fill these in without touching the fallback's call shape.
        fog_far_color: None,
        fog_max: None,
        light_fade_begin: None,
        light_fade_end: None,
        directional_ambient: None,
        specular_color: None,
        specular_alpha: None,
        fresnel_power: None,
    })
}
