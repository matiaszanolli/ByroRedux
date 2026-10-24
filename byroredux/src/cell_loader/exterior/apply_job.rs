//! The streaming exterior cell apply job (split out of `exterior.rs`, #6271).

use super::*;

/// Resumable main-thread application of one exterior cell.
///
/// Terrain and water are installed during construction as one
/// guaranteed-progress unit. FO4 precombined hashes and the remaining placed
/// REFRs then advance cooperatively under the shared frame deadline.
pub(crate) struct ExteriorCellApplyJob {
    gx: i32,
    gy: i32,
    cell_root: EntityId,
    terrain_center: Option<Vec3>,
    precombined: Option<super::super::precombined::PrecombinedSpawnJob>,
    /// #5484 — hashes whose `_oc.nif` bake drew geometry (`None` until
    /// the precombined phase completes). Keys
    /// `CellData::absorbed_ref_bakes` at the reference gate.
    baked_hashes: Option<Vec<u32>>,
    /// [`effective_absorbed_refs`](super::super::precombined::effective_absorbed_refs)
    /// of `baked_hashes`, built when the own-refs phase first needs it: the
    /// inputs are fixed once the precombined phase completes (#6127).
    absorbed: Option<HashSet<u32>>,
    references: Option<Box<ReferenceLoadJob>>,
    /// The cell's own refs' result centre, once they have all spawned.
    own_refs_center: Option<Vec3>,
    /// #6076 — the worldspace persistent CELL's refs this tile owns
    /// ([`persistent_refs_for_tile`]), spawned after its own and stamped on
    /// its root so they unload with it.
    persistent_refs: Vec<byroredux_plugin::esm::cell::PlacedRef>,
    persistent_promoted: bool,
    persistent_references: Option<Box<ReferenceLoadJob>>,
}

// Boxing `Pending` would allocate on every cooperative frame slice. The
// in-flight job is intentionally the large variant; it owns resumable cursors
// and bounded timing state across yields.
#[allow(clippy::large_enum_variant)]
pub(crate) enum ExteriorCellApplyProgress {
    Pending(ExteriorCellApplyJob),
    Complete(OneCellLoadInfo),
}

impl ExteriorCellApplyJob {
    /// Reclaim every ECS/GPU object produced by an unfinished apply.
    pub(crate) fn cancel(mut self, world: &mut World, ctx: &mut VulkanContext) {
        if let Some(references) = self.references.take() {
            references.cancel(world);
        }
        if let Some(references) = self.persistent_references.take() {
            references.cancel(world);
        }
        super::super::unload_cell(world, ctx, self.cell_root);
    }
}


impl ExteriorCellApplyJob {
    /// Install the non-reference phases and create the cell's reclaim root.
    ///
    /// This is deliberately one cooperative unit: the expensive high-cardinality
    /// phase is the REFR walk, while terrain/water/precombine preserve their
    /// existing atomic GPU submission boundaries.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn begin(
        wctx: &ExteriorWorldContext,
        gx: i32,
        gy: i32,
        world: &mut World,
        ctx: &mut VulkanContext,
        tex_provider: &TextureProvider,
        mat_provider: Option<&mut MaterialProvider>,
        terrain_blas_accumulator: Option<&mut Vec<(u32, u32, u32)>>,
        budget: &mut FrameTimeBudget,
    ) -> anyhow::Result<Option<Self>> {
        // #1668 — surface GLOB runtime values once per streaming session so
        // CTDA "Use Global" comparands resolve. The exterior path streams many
        // cells from one `record_index`; build the lean `Globals` map only when
        // it isn't already present rather than rebuilding it each cell.
        super::super::load::ensure_globals_resource(world, &wctx.record_index.globals);
        // #3010 / #3161 — the QF_ table used to be populated here, because
        // exterior streaming did not pass through the interior loader's
        // population hook. It now lives inside `populate_scene_runtime`
        // alongside its siblings, and every path that reaches this job first
        // builds its `WorldStreamingState` in `assemble_exterior_streaming`,
        // which calls that hook — so outdoor launches are covered without a
        // second call site to keep in sync. The `is_empty()` guard that
        // stood here was also wrong: it read only the `map` half of a
        // resource the walk fills in two independent halves.

        let index = &wctx.record_index.cells;
        let Some(cells_map) = index.exterior_cells.get(&wctx.worldspace_key) else {
            return Ok(None);
        };
        let Some(cell) = cells_map.get(&(gx, gy)) else {
            return Ok(None);
        };

        let cell_root = world.spawn();
        register_cell_root(world, cell_root);
        world.insert(cell_root, CellFormId(cell.form_id));
        stamp_exterior_cell(world, cell_root, wctx);
        super::super::note_player_entered_cell(world, cell.form_id, &wctx.record_index);
        let first_entity = world.next_entity_id();
        let has_land = cell.landscape.is_some();
        log::info!(
            "  Cell ({},{}) '{}': {} references{}",
            gx,
            gy,
            cell.editor_id,
            cell.references.len(),
            if has_land { " + LAND" } else { "" },
        );

        // Terrain mesh from LAND heightmap. Either accumulate the BLAS
        // spec for caller's batched build (bulk loader) or build it
        // ourselves (streaming).
        let mut local_blas: Vec<(u32, u32, u32)> = Vec::new();
        let blas_sink: &mut Vec<(u32, u32, u32)> = match terrain_blas_accumulator {
            Some(acc) => acc,
            None => &mut local_blas,
        };
        // #4054 — resolved before the terrain spawn (rather than only at the
        // water-plane spawn below) because the ground-cover `moisture` term
        // needs it on the terrain entity, and a cell resolves it once.
        let cell_water_y = resolved_exterior_water_height(
            cell.water_height_is_explicit,
            cell.water_height,
            wctx.default_water_height,
        );
        // Bethesda authors water height on its Z axis, and `zup_to_yup_pos`
        // maps Z-up Z onto Y-up Y unchanged — so this scalar is already the
        // Y-up height the density field compares blade heights against, with
        // no flip. (The plane's own placement still needs the full conversion;
        // that is `exterior_water_tile_transform` below.)
        if let Some(ref land) = cell.landscape {
            // #1855 — `spawn_terrain_mesh` already `log::warn!`s a mesh-upload
            // failure with its own (gx,gy) (the only realistic None cause in
            // production; the other — no GPU allocator — can't happen once
            // VulkanContext is constructed). This call-site line adds the
            // "cell had no terrain" correlation the per-cell log block above
            // otherwise lacks, so a partial-cell-render is diagnosable from
            // this cell's own log lines without cross-referencing the callee.
            if terrain::spawn_terrain_mesh(
                world,
                terrain::TerrainSpawnCtx {
                    ctx: &mut *ctx,
                    tex_provider,
                    landscape_textures: &index.landscape_textures,
                    landscape_texture_sets: &index.landscape_texture_sets,
                    mat_provider,
                    landscape_grasses: &index.landscape_grasses,
                    landscape_texture_names: &index.landscape_texture_names,
                    landscape_texture_havok: &index.landscape_texture_havok,
                    blas_specs: &mut *blas_sink,
                    water_y: cell_water_y,
                    game: wctx.record_index.game,
                    hidden_quadrants: cell.land_flags & 0x0F,
                },
                gx,
                gy,
                land,
            )
            .is_none()
            {
                log::warn!(
                    "  Cell ({gx},{gy}): terrain mesh spawn failed — no terrain will render"
                );
            }
        }
        // Water plane. A cell's explicit XCLW height wins; cells without one
        // inherit the resolved worldspace default (Oblivion Tamriel uses sea
        // level Z=0; FO3+ worlds use their DNAM water height). 98% of Oblivion
        // exterior cells have no XCLW and relied on this fallback — without it
        // every coastal/sea cell rendered dry
        // seabed (#1305 / OBL-D6-NEW-02). An authored XCLW no-water sentinel
        // suppresses that fallback; `water_height_is_explicit` preserves the
        // distinction between "no XCLW" and "explicitly dry".
        if let Some(water_height) = cell_water_y {
            // Exterior cell origin in Y-up world coords. The helper composes
            // grid-scale and the Z-up→Y-up flip; see TD3-202 / #1112.
            let (water_center, half) = exterior_water_tile_transform(gx, gy);
            // #1855 — `spawn_water_plane` already `log::warn!`s a mesh-upload
            // failure, but without cell coords (it doesn't take gx/gy). This
            // call-site line adds the cell correlation so a dry cell that
            // should have had water is diagnosable from this cell's own log
            // lines.
            if water::spawn_water_plane(
                world,
                ctx,
                tex_provider,
                &wctx.record_index.waters,
                water_height,
                cell.water_type_form.or(wctx.default_water_type_form),
                water_center,
                half,
                false,
                cell.landscape.as_ref(),
                wctx.record_index.game,
            )
            .is_none()
            {
                log::warn!("  Cell ({gx},{gy}): water plane spawn failed — no water will render");
            }
        }
        // Streaming path: submit our own BLAS build now (one mesh, one submit).
        if !local_blas.is_empty() {
            let built = ctx.build_blas_batched(&local_blas);
            log::info!(
                "  Cell ({},{}) terrain BLAS: {built}/{} tiles",
                gx,
                gy,
                local_blas.len(),
            );
        }

        // FO4+ precombined geometry is intentionally not spawned here. A
        // Commonwealth cell commonly carries 50–65 hashes; applying them as
        // one unit produced multi-second frames and let later boundary loads
        // supersede the active cell. Keep the ownership resolution on a
        // resumable cursor and advance one-or-more hashes per frame in
        // `advance` (#2376 / EX-07).
        let load_order_paths = wctx
            .plugin_paths
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let precombined = super::super::precombined::PrecombinedSpawnJob::new(
            cell,
            &wctx.plugin_path,
            &load_order_paths,
        );
        let baked_hashes = precombined.is_none().then(Vec::new);

        // Mid-cell terrain ground point — only meaningful for the
        // initial-load camera-positioning path used by the bulk loader.
        let terrain_center = cell.landscape.as_ref().map(|land| {
            let mid_height = land.heights[16 * 33 + 16];
            let world_x = gx as f32 * EXTERIOR_CELL_UNITS + 16.0 * 128.0;
            let world_y = gy as f32 * EXTERIOR_CELL_UNITS + 16.0 * 128.0;
            // Z-up → Y-up: (x, height, -y), plus 200 units above ground.
            Vec3::new(world_x, mid_height + 200.0, -world_y)
        });
        // EX-16 item 2 (#2372) — make this tile's NAVM tiles resident.
        // Must run before `last_entity` is captured below so the spawned
        // entities land inside the stamped range and get reclaimed on
        // unload for free.
        crate::components::spawn_navmesh_tiles(world, &cell.navmeshes);

        let last_entity = world.next_entity_id();
        stamp_cell_root_range(world, cell_root, first_entity, last_entity);
        budget.complete_unit();

        let persistent_refs = persistent_refs_for_tile(wctx, world, gx, gy)
            .into_iter()
            .cloned()
            .collect();
        Ok(Some(Self {
            gx,
            gy,
            cell_root,
            terrain_center,
            precombined,
            baked_hashes,
            absorbed: None,
            references: None,
            own_refs_center: None,
            persistent_refs,
            persistent_promoted: false,
            persistent_references: None,
        }))
    }

    /// Advance placed references until the frame budget expires.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn advance(
        mut self,
        wctx: &ExteriorWorldContext,
        world: &mut World,
        ctx: &mut VulkanContext,
        tex_provider: &TextureProvider,
        mut mat_provider: Option<&mut MaterialProvider>,
        budget: &mut FrameTimeBudget,
    ) -> ExteriorCellApplyProgress {
        let index = &wctx.record_index.cells;
        let cell = index
            .exterior_cells
            .get(&wctx.worldspace_key)
            .and_then(|cells| cells.get(&(self.gx, self.gy)))
            .expect("exterior cell existed when its apply job began");
        if self.baked_hashes.is_none() {
            let job = self
                .precombined
                .take()
                .expect("unfinished precombined apply retained its cursor");
            let first_entity = world.next_entity_id();
            match job.advance(
                cell,
                // The bake's instance transforms are world-absolute on
                // exteriors — the same frame as the exterior REFR `DATA`
                // positions, which spawn with no cell offset. Adding the
                // grid origin here would place every tile at ≈2× its world
                // position (#5228; 1ed8dc0bd assumed cell-local coords
                // before any CSG reader existed).
                Vec3::ZERO,
                world,
                ctx,
                tex_provider,
                mat_provider.as_deref_mut(),
                budget,
            ) {
                super::super::precombined::PrecombinedSpawnProgress::Pending(job) => {
                    self.precombined = Some(job);
                    stamp_cell_root_range(
                        world,
                        self.cell_root,
                        first_entity,
                        world.next_entity_id(),
                    );
                    flush_pending_cell_textures_on_yield(ctx);
                    return ExteriorCellApplyProgress::Pending(self);
                }
                super::super::precombined::PrecombinedSpawnProgress::Complete { baked_hashes, .. } => {
                    self.baked_hashes = Some(baked_hashes);
                    stamp_cell_root_range(
                        world,
                        self.cell_root,
                        first_entity,
                        world.next_entity_id(),
                    );
                }
            }
            if budget.should_yield() {
                flush_pending_cell_textures(ctx);
                return ExteriorCellApplyProgress::Pending(self);
            }
        }
        if self.own_refs_center.is_none() {
            let absorbed = &*self.absorbed.get_or_insert_with(|| {
                super::super::precombined::effective_absorbed_refs(
                    &cell.absorbed_ref_bakes,
                    self.baked_hashes
                        .as_deref()
                        .expect("precombined apply completed before reference loading"),
                )
            });
            let label = format!("exterior({},{})", self.gx, self.gy);
            let first_entity = world.next_entity_id();
            let progress = load_references_budgeted(
                &cell.references,
                index,
                &wctx.record_index,
                &wctx.record_index.races,
                wctx.record_index.game,
                world,
                ctx,
                tex_provider,
                mat_provider.as_deref_mut(),
                &label,
                wctx.load_order.as_ref(),
                absorbed,
                self.references.take(),
                budget,
            );
            let last_entity = world.next_entity_id();
            stamp_cell_root_range(world, self.cell_root, first_entity, last_entity);
            // #4699 — CELL `XOWN` for placements with no ownership of their own.
            crate::inventory::stamp_cell_ownership(
                world,
                cell.ownership.as_ref(),
                first_entity,
                last_entity,
            );

            match progress {
                ReferenceLoadProgress::Pending(references) => {
                    self.references = Some(references);
                    flush_pending_cell_textures_on_yield(ctx);
                    return ExteriorCellApplyProgress::Pending(self);
                }
                ReferenceLoadProgress::Complete(result) => {
                    self.own_refs_center = Some(self.terrain_center.unwrap_or(result.center));
                }
            }
        }

        // #6076 — then the persistent CELL's refs that stand in this tile.
        if !self.persistent_refs.is_empty() {
            if !self.persistent_promoted {
                promote_persistent_actor_stubs(world, wctx, &self.persistent_refs);
                self.persistent_promoted = true;
            }
            let label = format!("exterior({},{}) persistent", self.gx, self.gy);
            let first_entity = world.next_entity_id();
            let progress = load_references_budgeted(
                &self.persistent_refs,
                index,
                &wctx.record_index,
                &wctx.record_index.races,
                wctx.record_index.game,
                world,
                ctx,
                tex_provider,
                mat_provider,
                &label,
                wctx.load_order.as_ref(),
                &HashSet::new(),
                self.persistent_references.take(),
                budget,
            );
            stamp_cell_root_range(world, self.cell_root, first_entity, world.next_entity_id());
            if let ReferenceLoadProgress::Pending(references) = progress {
                self.persistent_references = Some(references);
                flush_pending_cell_textures_on_yield(ctx);
                return ExteriorCellApplyProgress::Pending(self);
            }
        }
        // #6102 — every reference of the tile has spawned: retire its pending
        // `OnReset` after the next pass marks the last slices' scripted refs.
        byroredux_scripting::obscript_state::cell_load_finished(world, self.cell_root);
        ExteriorCellApplyProgress::Complete(OneCellLoadInfo {
            cell_root: self.cell_root,
            center: self
                .own_refs_center
                .expect("the cell's own references completed first"),
        })
    }
}

// `default_water_for_worldspace` (+ its #1305 / OBL-D6-NEW-02 per-game
// default-water regressions) moved to the EXAL boundary in
// `crate::env_translate`; the tests moved with it.
