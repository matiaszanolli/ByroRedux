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
use crate::components::{CellLightingRes, CellRootIndex};

use super::load_order::parse_record_indexes_in_load_order;
use super::references::load_references;
use super::water;

/// Result of loading a cell.
pub struct CellLoadResult {
    pub cell_name: String,
    pub entity_count: usize,
    /// Chosen spawn point (Y-up), used for initial camera/player
    /// positioning. Prefers the cell's first door's own placement — a
    /// guaranteed walkable threshold — over the bounding-box centroid of
    /// every placed REFR, which has no such guarantee and could land inside
    /// a wall, a stairwell void, or outside the interior shell entirely for
    /// L-shaped/multi-wing cells. See `references::load_references`.
    pub center: Vec3,
    /// Interior cell lighting (ambient + directional).
    pub lighting: Option<byroredux_plugin::esm::cell::CellLighting>,
}

/// Apply a freshly-loaded interior cell's [`CellLighting`](esm::cell::CellLighting)
/// to the renderer's `CellLightingRes`. Shared by *every* interior-load
/// entry point — the startup `--cell` path (`scene.rs`), the M40 door-walk
/// transition ([`super::load_interior_cell`]), and the `cell.load` debug
/// command (`debug_load.rs`) — so they cannot drift.
///
/// Pre-#1340 only the startup path applied it, so an interior reached at
/// runtime rendered with the *previous* cell's `CellLightingRes`: wrong
/// ambient/fog, exterior clear color, and the directional sun leaking into
/// a sealed interior — the exact failure #1282 gated on `is_interior`.
///
/// Routes the authored XCLL Euler angles through `euler_zup_to_quat_yup`
/// (the CW-convention helper REFR placements use), then applies the Y-up
/// quaternion to Gamebryo's `NiDirectionalLight` model direction `(1,0,0)`
/// (2.3 `NiDirectionalLight.h`: "The model direction of the light is
/// (1,0,0)"; the Z-up → Y-up swap leaves +X invariant). `is_interior` is
/// always `true` — `load_cell_with_masters` only loads interior cells, and
/// the flag makes `CellLightingRes` skip the directional as a scene light
/// to prevent wall light leakage. The 9 extended XCLL fields (`fog_clip`,
/// `directional_ambient`, …) are propagated by `from_cell_lighting` (#861).
pub(crate) fn apply_interior_cell_lighting(world: &mut World, lighting: &esm::cell::CellLighting) {
    let (rx, ry) = (
        lighting.directional_rotation[0],
        lighting.directional_rotation[1],
    );
    let quat = super::euler_zup_to_quat_yup(rx, ry, 0.0);
    let dir_v = quat * Vec3::new(1.0, 0.0, 0.0);
    let dir = [dir_v.x, dir_v.y, dir_v.z];
    world.insert_resource(CellLightingRes::from_cell_lighting(lighting, dir, true));
    log::info!(
        "Cell lighting: ambient={:?} directional={:?} dir={:?} fog={:?} near={:.0} far={:.0}",
        lighting.ambient,
        lighting.directional_color,
        dir,
        lighting.fog_color,
        lighting.fog_near,
        lighting.fog_far,
    );
}

/// Surface parsed `GLOB` runtime values as the `Globals` `World` resource,
/// but only when it isn't already present (#1668, #1865 / SCR-D6-NEW-03).
///
/// Shared by every cell-load entry point (this module's interior path and
/// [`super::exterior::load_one_exterior_cell`]) so they can't drift back
/// into disagreement. Rebuilding unconditionally on every load would
/// silently discard any runtime `Globals::set` mutation the moment another
/// cell loads afterward — dormant today (no production writer exists yet)
/// but a landmine for the pending `SetGlobalValue` Papyrus writer.
pub(crate) fn ensure_globals_resource(
    world: &mut World,
    records: &std::collections::HashMap<u32, byroredux_plugin::esm::records::GlobalRecord>,
) {
    if world
        .try_resource::<byroredux_scripting::globals::Globals>()
        .is_none()
    {
        world.insert_resource(byroredux_scripting::globals::Globals::from_records(records));
    }
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
/// SAVE-D6-02 — non-destructive pre-flight for a live load.
///
/// Parses the plugin set and confirms `cell_editor_id` resolves, WITHOUT
/// mutating any world or GPU state. Mirrors the first two phases of
/// [`load_cell_with_masters`] (parse → cell lookup); those are exactly the
/// two phases the live `load <slot>` failure modes hit (missing/corrupt
/// ESM, renamed/absent cell editor id), and both are non-destructive in the
/// full loader too. The caller runs this *before* tearing down the running
/// cell, so a reload that can't succeed keeps the current cell instead of
/// stranding the player in an empty world.
///
/// The full [`load_cell_with_masters`] re-parses (a few seconds, paid once
/// per user-initiated load) — cheap insurance against an unrecoverable
/// session. Returns `Ok(())` when the cell is loadable.
pub fn validate_cell_loadable(
    masters: &[String],
    esm_path: &str,
    cell_editor_id: &str,
) -> anyhow::Result<()> {
    let plugin_paths: Vec<&str> = masters
        .iter()
        .map(|s| s.as_str())
        .chain(std::iter::once(esm_path))
        .collect();
    let (index, _load_order) = parse_record_indexes_in_load_order(&plugin_paths)?;
    let cell_key = cell_editor_id.to_ascii_lowercase();
    anyhow::ensure!(
        index.cells.cells.contains_key(&cell_key),
        "Cell '{}' not found among {} interior cells in the saved plugin set",
        cell_editor_id,
        index.cells.cells.len(),
    );
    Ok(())
}

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
    mut mat_provider: Option<&mut MaterialProvider>,
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
        // Phase 20.2 — when the requested cell doesn't exist,
        // filter the suggestion list to cells whose editor ID
        // contains the requested name as a substring (case-
        // insensitive). Turns "cell not found" into a
        // self-diagnostic: a typo or off-by-one suffix shows
        // every close match in the error message instead of
        // a random 20-cell sample that's rarely close to what
        // the user wanted. Falls back to the random sample only
        // when no substring match exists.
        let needle = cell_key.as_str();
        let matches: Vec<&str> = index
            .cells
            .cells
            .values()
            .filter(|c| c.editor_id.to_ascii_lowercase().contains(needle))
            .take(20)
            .map(|c| c.editor_id.as_str())
            .collect();
        let (label, examples) = if matches.is_empty() {
            // Also try a 4-char prefix match — handles cases
            // where the user got the prefix right but the
            // suffix wrong (e.g. `InstAdvSys01` when the cell
            // is `InstSRBLab02`).
            let prefix_len = needle.len().min(4);
            let prefix = &needle[..prefix_len];
            let prefix_matches: Vec<&str> = index
                .cells
                .cells
                .values()
                .filter(|c| c.editor_id.to_ascii_lowercase().starts_with(prefix))
                .take(20)
                .map(|c| c.editor_id.as_str())
                .collect();
            if prefix_matches.is_empty() {
                let any: Vec<&str> = index
                    .cells
                    .cells
                    .values()
                    .take(20)
                    .map(|c| c.editor_id.as_str())
                    .collect();
                ("first 20 cells", any)
            } else {
                ("cells matching prefix", prefix_matches)
            }
        } else {
            ("cells containing substring", matches)
        };
        anyhow::anyhow!(
            "Cell '{}' not found. {} interior cells available. {} ({}): {:?}",
            cell_editor_id,
            index.cells.cells.len(),
            label,
            examples.len(),
            examples,
        )
    })?;

    log::info!(
        "Loading cell '{}' (form {:08X}): {} placed references",
        cell.editor_id,
        cell.form_id,
        cell.references.len(),
    );

    // 3a. FO4+ PreCombined Mesh spawn (#1188). Run BEFORE REFR
    // loading so the spawn count decides whether `cell.absorbed_refs`
    // is honored. The shared-variant `_oc.nif` files are resolved via
    // `Fallout4 - Geometry.csg` (M49 complete). If spawn succeeds,
    // the absorption gate suppresses per-REFR rendering of the original
    // architecture (which is flagged XPRI). Empty on non-FO4 cells
    // or when CSG resolution fails — fallback via the conditional
    // gate in load_cell_with_masters.
    let (pc_spawned, _pc_misses) = super::precombined::spawn_precombined_meshes(
        cell,
        // Interior cells: cell origin IS the world origin, so the
        // bake's cell-local coords already are world coords. #1222.
        Vec3::ZERO,
        world,
        ctx,
        tex_provider,
        mat_provider.as_deref_mut(),
        // M49 — the active plugin provides the Data dir + CSG fallback; the
        // owning plugin (per cell form-id) selects the actual
        // `<Plugin> - Geometry.csg` and the `_oc.nif` path. #1590.
        esm_path,
        &plugin_paths,
    );

    // 3b. Load placed references. Honor `cell.absorbed_refs` only
    // when the precombined spawn produced at least one entity — the
    // XPRI list marks REFRs whose geometry is supposed to come from
    // the precombine. With no precombine actually rendered, those
    // REFRs are the only carrier of the architecture and must load
    // normally (real Bethesda games take the same fallback path when
    // `bUseCombinedObjects=0`).
    static EMPTY_ABSORBED: std::sync::OnceLock<std::collections::HashSet<u32>> =
        std::sync::OnceLock::new();
    let absorbed = if pc_spawned > 0 {
        &cell.absorbed_refs
    } else {
        EMPTY_ABSORBED.get_or_init(std::collections::HashSet::new)
    };
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
        absorbed,
    );

    // 3a′. M47.2 keystone — populate the quest-stage fragment table from
    // the merged index's QUST VMAD fragment bindings (Skyrim+). Decompiles
    // each quest's `QF_` script once and lowers its bound `Fragment_N`
    // bodies to canonical effects the `quest_fragment_dispatch_system`
    // applies when a stage advances. No-op without `--scripts-bsa` (no
    // `.pex` to resolve) or on pre-Papyrus games (empty `fragments`).
    crate::asset_provider::populate_quest_fragments(world, &index);

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
    // don't show up in the CellRoot storage — fine. The returned root
    // entity is only consumed by the interior-unload path; today no
    // caller exercises it (interior cells loaded at startup live
    // until process exit) so it's discarded here. Re-add the field
    // when a real interior-unload consumer materialises.
    let last_entity = world.next_entity_id();
    let cell_root = world.spawn();
    stamp_cell_root(world, cell_root, first_entity, last_entity);

    // Capture the cell's editor_id BEFORE the `index.cells` move below
    // — `cell` borrows from `index.cells.cells`, so the borrow has to
    // end before the move consumes the parent map.
    let cell_name = cell.editor_id.clone();
    let entity_count = result.entity_count;
    let center = result.center;

    // #1668 — surface GLOB runtime values so CTDA "Use Global" comparands
    // resolve. Keyed in global load-order space (EsmIndex remaps record
    // FormIDs at parse), matching the comparand's remapped space. Built
    // before the `index.cells` move below — `globals` is a sibling field.
    ensure_globals_resource(world, &index.globals);

    // M40 Phase 2 Stage 1 — surface the parsed cell index as a World
    // resource so `&World` readers (door.teleport console command,
    // future F-key activate system) can resolve XTEL destination
    // FormIDs back to their parent cells without re-parsing the ESM.
    // Replaces any prior load's index wholesale.
    world.insert_resource(super::LoadedCellIndex(std::sync::Arc::new(index.cells)));

    // M40 Phase 2 Stage 3 — record the just-spawned cell root so the
    // transition orchestrator can unload it on the next swap. Cleared
    // by `transition::execute_pending` before each load; the exterior
    // streaming entry points leave this as `None` (they track their
    // own `state.loaded` map). Insert wholesale on every interior load
    // so a transition from one interior to another updates the tracker
    // even when the resource was already present from the prior load.
    world.insert_resource(super::CurrentCellRoot(Some(cell_root)));

    // M45.1 — record the cell identity + plugin set so a save taken here
    // is self-describing: `load` re-issues this exact interior load before
    // applying saved deltas. Replaces any prior load's context wholesale.
    world.insert_resource(super::CurrentCellContext {
        cell_editor_id: cell_editor_id.to_string(),
        esm_path: esm_path.to_string(),
        masters: masters.to_vec(),
    });

    Ok(CellLoadResult {
        cell_name,
        entity_count,
        center,
        lighting: resolved_lighting,
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
        // SF volumetric height-fog fields ride on the inline 108-byte
        // XCLL, not the LGTM template stub (#1293).
        starfield: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// SAVE-D6-02 — the live-load pre-flight must FAIL (not panic) when the
    /// saved plugin set can't be read, so the drain catches it BEFORE tearing
    /// down the running cell and the player isn't stranded in the void. A
    /// missing ESM is the cheapest reproduction of the "corrupt/missing ESM"
    /// failure mode and needs no game data.
    #[test]
    fn validate_cell_loadable_errors_on_missing_esm() {
        let err = validate_cell_loadable(&[], "/nonexistent/Missing.esm", "AnyCell")
            .expect_err("a missing ESM must fail the pre-flight");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("Missing.esm"),
            "error should name the unreadable plugin: {msg}"
        );
    }

    /// #1865 / SCR-D6-NEW-03 — `ensure_globals_resource` must guard on
    /// `is_none()`, mirroring `exterior.rs`'s exterior-streaming guard,
    /// so a runtime `Globals::set` mutation survives a second cell load
    /// (e.g. an interior-to-interior door transition) rather than being
    /// silently reset back to the ESM-parsed default. Pre-fix, `load.rs`'s
    /// interior path called `insert_resource` unconditionally.
    #[test]
    fn ensure_globals_resource_preserves_runtime_mutation_across_reload() {
        use byroredux_plugin::esm::records::global::SettingValue;
        use byroredux_plugin::esm::records::GlobalRecord;
        use byroredux_scripting::globals::Globals;
        use std::collections::HashMap;

        let mut records = HashMap::new();
        records.insert(
            0x1000,
            GlobalRecord {
                form_id: 0x1000,
                editor_id: "GameHour".to_string(),
                value: SettingValue::Float(8.0),
            },
        );

        let mut world = World::new();

        // First "load": no resource present yet, so it's built from records.
        ensure_globals_resource(&mut world, &records);
        assert_eq!(world.resource::<Globals>().get(0x1000), Some(8.0));

        // A Papyrus SetGlobalValue-style runtime write.
        world.resource_mut::<Globals>().set(0x1000, 23.5);
        assert_eq!(world.resource::<Globals>().get(0x1000), Some(23.5));

        // Second "load" (simulates an interior-to-interior transition) with
        // the SAME static records — must NOT clobber the runtime mutation.
        ensure_globals_resource(&mut world, &records);
        assert_eq!(
            world.resource::<Globals>().get(0x1000),
            Some(23.5),
            "a second cell load must preserve the runtime Globals::set mutation, \
             not reset it back to the ESM-parsed default"
        );
    }

    /// Minimal FNV-shape interior lighting (no Skyrim+ / Starfield tail).
    fn interior_lighting() -> esm::cell::CellLighting {
        esm::cell::CellLighting {
            ambient: [0.10, 0.10, 0.12],
            directional_color: [1.0, 0.95, 0.80],
            directional_rotation: [0.0, 0.0],
            fog_color: [0.50, 0.45, 0.30],
            fog_near: 100.0,
            fog_far: 8000.0,
            directional_fade: None,
            fog_clip: None,
            fog_power: None,
            fog_far_color: None,
            fog_max: None,
            light_fade_begin: None,
            light_fade_end: None,
            directional_ambient: None,
            specular_color: None,
            specular_alpha: None,
            fresnel_power: None,
            starfield: None,
        }
    }

    /// Regression for #1340 / D3-04 — the shared interior-lighting apply
    /// helper must install a `CellLightingRes` with `is_interior == true`.
    /// Pre-fix, two of the three interior-load entry points (the door-walk
    /// transition + the `cell.load` debug command) skipped this entirely,
    /// so a runtime-loaded interior kept the *previous* cell's resource —
    /// wrong fog/ambient and the exterior directional sun leaking into a
    /// sealed interior (the gate #1282 added keys on `is_interior`).
    /// Routing all three callers through this one helper is the structural
    /// fix; this pins the helper's `is_interior == true` contract.
    #[test]
    fn apply_interior_cell_lighting_inserts_interior_resource() {
        let mut world = World::new();
        // Fresh world == "no previous cell lighting present".
        assert!(world.try_resource::<CellLightingRes>().is_none());

        apply_interior_cell_lighting(&mut world, &interior_lighting());

        let res = world
            .try_resource::<CellLightingRes>()
            .expect("apply_interior_cell_lighting must insert CellLightingRes");
        assert!(
            res.is_interior,
            "interior lighting must set is_interior=true so the directional \
             sun is gated out of the sealed cell (#1282 / #1340)"
        );
        assert_eq!(res.ambient, [0.10, 0.10, 0.12], "ambient must propagate");
        assert_eq!(res.fog_far, 8000.0, "fog_far must propagate");
    }
}
