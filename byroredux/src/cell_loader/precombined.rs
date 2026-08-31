//! FO4+ PreCombined Mesh loader (#1188).
//!
//! Bethesda's CK / GECK bakes individual architecture STAT placements
//! (walls, floors, ceilings, ductwork, etc.) into a single
//! `meshes\precombined\<cell_formid:08x>_<hash:08x>_oc.nif` file per
//! cell-tile. Those individual REFRs are then **absorbed** — the cell
//! record's REFR list still carries them, but with the XPRI sub-record
//! flagging them as precombined. The runtime spawns the combined NIF
//! instead.
//!
//! **Current state** (M49 — complete): this loader reads each `_oc.nif`
//! from the asset BSA chain (typically `Fallout4 - MeshesExtra.ba2`), then
//! resolves the vertex / triangle data from the `<Plugin> - Geometry.csg` blob
//! its own `BSPackedGeomObject::filename_hash` names, via `CsgArchive` (one big
//! zlib-compressed PSG keyed by filename hash + offset). Note that the naming
//! plugin is not always the cell's owner — a plugin re-baking a master-owned
//! cell keeps the master's filename but moves the geometry into its own blob
//! (#2369). Meshes are decoded to Y-up, spawned at cell-local identity, and
//! tagged as `RenderLayer::Architecture`. LOD is
//! selected by triangle count (finest LOD only, per `fo4-csg-format.md:138-142`).
//! Absorption gate in [`super::load::load_cell_with_masters`] (conditional on
//! spawn count) honors the cell's `absorbed_refs` list, suppressing per-REFR
//! rendering of baked REFRs.
//!
//! Deferred sub-items (M49 Stage B):
//! - Collision — the sibling file is `<cell_formid:08x>_physics.nif`
//!   (verified against real `Fallout4 - MeshesExtra.ba2` data, 2026-08-23 —
//!   NOT `_precomb.nif`, which appears nowhere in the archive). Every
//!   sampled instance (16/16) decodes to exactly `NiNode` + `NiExtraData` +
//!   `bhkNPCollisionObject` + `bhkPhysicsSystem`, and `bhkPhysicsSystem` is
//!   a raw undecoded Havok-serialised (HKX-like) blob
//!   (`BhkSystemBinary`, `crates/nif/src/blocks/collision/
//!   collision_object.rs:121-151`) — the same blocker that already blocks
//!   general FO4+ physics/ragdoll work. There is no authored convex-hull
//!   data extractable with `crates/nif`'s *existing* parsers; a Havok
//!   NP-physics binary decoder is greenfield format work, not a small
//!   addition. FO4 architecture today gets synthesized trimesh colliders
//!   via fallback in `spawn.rs`, spawned as separate MeshHandle-free ghost
//!   entities so they stay out of BLAS/TLAS. See EX-14/15 item C4 (#2369).
//! - Visibility / `.uvd` occlusion data — previs PVS keyed to visibility groups.
//!   Currently no occlusion-volume or CPU coarse-cull system exists.

use byroredux_bsa::CsgArchive;
use byroredux_core::ecs::components::{PrecombinedMesh, RenderLayer};
use byroredux_core::ecs::World;
use byroredux_core::math::{Quat, Vec3};
use byroredux_core::string::StringPool;
use byroredux_nif::import::precombine::{decode_shared_geom_object, psg_vertex_stride};
use byroredux_nif::import::{ImportedMesh, MeshResolver};
use byroredux_nif::scene::NifScene;
use byroredux_plugin::esm::cell::CellData;
use byroredux_renderer::vulkan::context::VulkanContext;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::nif_import_registry::{CachedNifImport, NifImportRegistry};
use super::spawn::spawn_placed_instances;
use super::FrameTimeBudget;
use crate::asset_provider::{MaterialProvider, TextureProvider};

/// Resolve the effective absorbed-REFR set for a cell load's per-REFR pass.
///
/// When the precombine actually spawned geometry (`pc_spawned > 0`), the
/// cell's `absorbed_refs` list suppresses per-REFR rendering of the baked
/// REFRs (the combined NIF already carries them). When nothing spawned,
/// those XPRI-flagged REFRs are the only carrier of the architecture and
/// must load normally, so the effective set is empty — the same fallback
/// real Bethesda games take under `bUseCombinedObjects=0` (#1188).
///
/// Shared by the interior (`load.rs`) and exterior (`exterior.rs`) loaders
/// so the gate cannot drift between them (TD2-104 / #2063). The empty set
/// is a process-lifetime singleton, so the borrow is valid for any caller.
pub(crate) fn absorbed_refs_or_empty(
    absorbed_refs: &std::collections::HashSet<u32>,
    pc_spawned: usize,
) -> &std::collections::HashSet<u32> {
    static EMPTY_ABSORBED: std::sync::OnceLock<std::collections::HashSet<u32>> =
        std::sync::OnceLock::new();
    if pc_spawned > 0 {
        absorbed_refs
    } else {
        EMPTY_ABSORBED.get_or_init(std::collections::HashSet::new)
    }
}

/// Resumable cursor over one cell's precombined hashes.
///
/// Each hash is one atomic parse/decode/upload/BLAS unit. Keeping the CSG
/// handle and ownership resolution here avoids reopening the large shared-
/// geometry archive on every frame; the outer cursor yields between hashes.
pub(super) struct PrecombinedSpawnJob {
    form_id: u32,
    total_hashes: usize,
    next_hash: usize,
    spawned: usize,
    misses: usize,
    owning_subdir: Option<String>,
    /// Boxed so the resumable job (and the `Pending` variant that carries it
    /// between frames) stays cheap to move.
    csg: Box<CsgRouting>,
    csg_open: Duration,
    timed_hashes: usize,
    max_prepare: Duration,
    max_spawn: Duration,
    max_spawn_cpu: Duration,
    max_blas: Duration,
    max_total: Duration,
    max_total_hash: u32,
}

/// Which `<Plugin> - Geometry.csg` each `BSPackedGeomObject::filename_hash`
/// resolves to, for one cell's load (#2369).
#[derive(Default)]
struct CsgRouting {
    /// `filename_hash` → the plugin whose companion blob answers to it, for
    /// every plugin in the load order. Names are hashed, not probed, so an
    /// entry means "this plugin could own that blob", not that a `.csg` is on
    /// disk; the probe happens on first use and is remembered in `open`. Only
    /// an empty load order leaves this empty, which short-circuits the
    /// shared-geometry path back to the ordinary NIF import.
    paths: std::collections::HashMap<u32, String>,
    /// Lazily opened blobs keyed by the same hash. `None` records a probe
    /// that found no companion CSG, so it isn't retried per hash.
    open: std::collections::HashMap<u32, Option<Arc<CsgArchive>>>,
}

pub(super) enum PrecombinedSpawnProgress {
    Pending(PrecombinedSpawnJob),
    Complete { spawned: usize, misses: usize },
}

impl PrecombinedSpawnJob {
    pub(super) fn new(
        cell: &CellData,
        plugin_path: &str,
        load_order_paths: &[&str],
    ) -> Option<Self> {
        if cell.precombined_mesh_hashes.is_empty() {
            return None;
        }
        let (owning_path, owning_subdir) =
            resolve_precombine_owner(cell.form_id, load_order_paths, plugin_path);
        Some(Self {
            form_id: cell.form_id,
            total_hashes: cell.precombined_mesh_hashes.len(),
            next_hash: 0,
            spawned: 0,
            misses: 0,
            owning_subdir,
            csg: Box::new(CsgRouting {
                paths: csg_paths_by_name_hash(load_order_paths, owning_path),
                ..Default::default()
            }),
            csg_open: Duration::ZERO,
            timed_hashes: 0,
            max_prepare: Duration::ZERO,
            max_spawn: Duration::ZERO,
            max_spawn_cpu: Duration::ZERO,
            max_blas: Duration::ZERO,
            max_total: Duration::ZERO,
            max_total_hash: 0,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn advance(
        mut self,
        cell: &CellData,
        cell_origin: Vec3,
        world: &mut World,
        ctx: &mut VulkanContext,
        tex_provider: &TextureProvider,
        mut mat_provider: Option<&mut MaterialProvider>,
        budget: &mut FrameTimeBudget,
    ) -> PrecombinedSpawnProgress {
        debug_assert_eq!(cell.form_id, self.form_id);
        debug_assert_eq!(cell.precombined_mesh_hashes.len(), self.total_hashes);

        while self.next_hash < self.total_hashes {
            if budget.should_yield() {
                return PrecombinedSpawnProgress::Pending(self);
            }
            let hash = cell.precombined_mesh_hashes[self.next_hash];
            let path = precombine_oc_nif_path(self.form_id, hash, self.owning_subdir.as_deref());
            let hash_started = Instant::now();

            // Check the process-lifetime cache first; precombined NIFs are
            // typically unique-per-cell so the hit-rate is near zero on
            // cold loads but we still want the path through the LRU so
            // the `_oc.nif` survives a brief un/reload (e.g. interior →
            // re-enter same cell).
            let cached: Option<Arc<CachedNifImport>> = {
                let reg = world.resource::<NifImportRegistry>();
                reg.get(&path).and_then(|opt| opt.clone())
            };

            let cached = if let Some(c) = cached {
                // #1217 / D2 FIND-3 — cache-hit on a zero-mesh entry surfaces
                // post-mortem visibility for the CSG-deferred fallback. The
                // first cache MISS fires the zero-contribution warn in
                // `parse_and_import_nif` (#1215); subsequent cells re-using
                // the same `_oc.nif` path hit this branch and skip the
                // warn site. Without this debug line an operator only
                // sees the first occurrence per process.
                if c.meshes.is_empty() && c.collisions.is_empty() && c.lights.is_empty() {
                    log::debug!(
                        "PreCombined cache hit on zero-mesh entry: '{}' \
                     (cell {:08X}) — CSG-deferred fallback",
                        path,
                        self.form_id,
                    );
                }
                c
            } else {
                // Cache miss — extract + parse + import + commit. Use the
                // same `parse_and_import_nif` path that loose REFR meshes
                // use (BGSM merge + collision extraction + animation
                // capture) so precombines benefit from the same texture /
                // material plumbing.
                let bytes = match tex_provider.extract_mesh(&path) {
                    Some(b) => b,
                    None => {
                        if self.misses < 3 {
                            // Surface the first 3 misses at WARN so an
                            // operator can verify the path shape. Default
                            // log level may suppress debug!. The bulk
                            // miss count is logged at the end of this fn.
                            log::warn!(
                                "PreCombined miss: '{}' not found in mesh archives \
                             (cell {:08X}, hash {:08x})",
                                path,
                                self.form_id,
                                hash,
                            );
                        }
                        self.misses += 1;
                        self.next_hash += 1;
                        budget.complete_unit();
                        continue;
                    }
                };
                // M49 — shared-variant precombines store their geometry in the
                // companion `.csg`, which the standard walk-based import skips
                // (it produces zero meshes). When the CSG resolved, decode the
                // packed-combined objects directly into spawnable meshes. Falls
                // through to the standard import path when the CSG is absent or
                // the `_oc.nif` carries no shared geometry (baked variant /
                // non-precombine content).
                let csg_parsed: Option<Arc<CachedNifImport>> = (!self.csg.paths.is_empty())
                    .then(|| byroredux_nif::parse_nif(&bytes))
                    .and_then(|parsed| match parsed {
                        Ok(scene) => {
                            // Which blob this `_oc.nif` reads from is stated by
                            // the geometry objects themselves, not by whoever
                            // owns the cell (#2369). Opening the CSG is its own
                            // cooperative unit — on a cold session inflating a
                            // 240 MB-class blob's chunk table can cross a
                            // deadline by itself.
                            let csgs = self.open_csgs_for(
                                &byroredux_nif::import::precombine::precombine_csg_filename_hashes(
                                    &scene,
                                ),
                                mat_provider.as_deref_mut(),
                                budget,
                            );
                            if csgs.is_empty() {
                                return None;
                            }
                            let (meshes, geometry_dedup) = {
                                let mut pool = world.resource_mut::<StringPool>();
                                let (mut meshes, geometry_dedup) = build_precombine_meshes(
                                    &scene,
                                    &|h| csgs.get(&h).cloned(),
                                    &mut pool,
                                );
                                // Apply BGSM material flags (two_sided / decal /
                                // alpha_test) to the CSG-decoded meshes. FO4
                                // authors these in the `.bgsm`, not the NIF, so
                                // `precombine_material_from_shape` (NIF-only)
                                // can't see them. The REFR and fallback
                                // (`parse_and_import_nif`) paths run this merge; the
                                // shared-precombine CSG path did not — leaving
                                // precombine foliage/decals with no alpha-test
                                // (opaque-black cards clipping through walls) and no
                                // two-sided / decal routing. `merge_external_material`
                                // no-ops for meshes without a `material_path`.
                                //
                                // BUT do NOT take the BGSM alpha-BLEND on this path.
                                // FO4 authors the "Standard" blend mode (function=1,
                                // src=6, dst=7) identically on transparent lab glass
                                // AND opaque metal architecture (institutemetal01a,
                                // flatmetalpanelsdetails01); `merge_external_material`
                                // turns any function>0 into `has_alpha`, so applying
                                // it here made the whole precombined Institute shell
                                // alpha-blend against its diffuse alpha (specular
                                // data on lit metal, not opacity) → see-through walls
                                // (#1619 follow-up; the efd3c41b regression). Keep
                                // the merge's other flags but restore the pre-merge
                                // (NIF-shape) alpha-blend state so opaque precombine
                                // architecture stays opaque.
                                if let Some(provider) = mat_provider.as_deref_mut() {
                                    for mesh in &mut meshes {
                                        let blend = (
                                            mesh.material.has_alpha,
                                            mesh.material.src_blend_mode,
                                            mesh.material.dst_blend_mode,
                                        );
                                        // #2709 (SF-D9-03) — outcome discarded
                                        // deliberately; this path already
                                        // selectively reverts part of the merge
                                        // (the blend restore below) and has no
                                        // per-cell material tally to feed.
                                        let _ = crate::asset_provider::merge_external_material(
                                            &mut mesh.material,
                                            provider,
                                            &mut pool,
                                        );
                                        (
                                            mesh.material.has_alpha,
                                            mesh.material.src_blend_mode,
                                            mesh.material.dst_blend_mode,
                                        ) = blend;
                                    }
                                }
                                (meshes, geometry_dedup)
                            };
                            (!meshes.is_empty())
                                .then(|| Arc::new(geometry_only_cached(meshes, geometry_dedup)))
                        }
                        Err(e) => {
                            log::warn!(
                                "PreCombined CSG parse failed: '{path}' (cell {:08X}): {e}",
                                self.form_id
                            );
                            None
                        }
                    });
                let parsed = match csg_parsed {
                    Some(c) => Some(c),
                    None => {
                        let mut pool = world.resource_mut::<byroredux_core::string::StringPool>();
                        super::references::parse_and_import_nif_pub(
                            &bytes,
                            &path,
                            mat_provider.as_deref_mut(),
                            &mut pool,
                            Some(tex_provider as &dyn MeshResolver),
                        )
                    }
                };
                // Commit to registry so a re-load of this cell hits the cache.
                let freed = {
                    let mut reg = world.resource_mut::<NifImportRegistry>();
                    reg.insert(path.clone(), parsed.clone())
                };
                // #2524 / PERF-D3-NEW-01 — release any LRU-evicted clip
                // handles. This insert's own entry never owns a clip
                // handle, but the 2048-cap sweep it can trigger may evict
                // a DIFFERENT, animated cache entry registered via one of
                // the other four `NifImportRegistry::insert` call sites —
                // whichever victim the sweep picks, if it owned a clip
                // handle, that handle must be released here or it leaks
                // (the exact #863 bug class, reintroduced at this newer
                // call site).
                if !freed.is_empty() {
                    let mut clip_reg =
                        world.resource_mut::<byroredux_core::animation::AnimationClipRegistry>();
                    for h in freed {
                        clip_reg.release(h);
                    }
                }
                match parsed {
                    Some(c) => c,
                    None => {
                        log::warn!(
                            "PreCombined parse failed: '{}' (cell {:08X})",
                            path,
                            self.form_id,
                        );
                        self.misses += 1;
                        self.next_hash += 1;
                        budget.complete_unit();
                        continue;
                    }
                }
            };
            let prepare_elapsed = hash_started.elapsed();

            let spawn_started = Instant::now();
            let first_entity = world.next_entity_id();
            let (_placement_root, count, spawn_timings) = spawn_placed_instances(
                world,
                ctx,
                &cached,
                tex_provider,
                cell_origin,
                Quat::IDENTITY,
                1.0,
                None,
                0,
                0,
                // #2439 (NIFAL-D2-01) — precombined architecture carries no
                // LIGH data (`light_data: None` above), so these are unused
                // defaults, matching the `0, 0` animation/shadow flags above.
                byroredux_core::ecs::LightKind::Point,
                [0.0, 0.0, 0.0],
                0.0,
                None,
                None,
                RenderLayer::Architecture,
                Some(&path),
                None,
                None,
                None,
                mat_provider.as_deref_mut(),
            );
            // #2369 (EX-15) — stamp the entities this call just spawned as
            // precombine-owned, distinct from ordinary per-REFR
            // architecture, so `world.owners` can track them as their own
            // reclaim class instead of folding them into the generic
            // `cell_root_rows` count. `CellRoot` itself is stamped by the
            // caller (`exterior.rs` / `spawn_precombined_meshes`) once this
            // job either yields or completes; this narrower marker is
            // independent of that and safe to apply per-hash.
            let last_entity = world.next_entity_id();
            for eid in first_entity..last_entity {
                world.insert(eid, PrecombinedMesh);
            }
            let spawn_elapsed = spawn_started.elapsed();
            let total_elapsed = hash_started.elapsed();
            self.timed_hashes += 1;
            self.max_prepare = self.max_prepare.max(prepare_elapsed);
            self.max_spawn = self.max_spawn.max(spawn_elapsed);
            self.max_spawn_cpu = self.max_spawn_cpu.max(spawn_timings.cpu_upload);
            self.max_blas = self.max_blas.max(spawn_timings.blas);
            if total_elapsed > self.max_total {
                self.max_total = total_elapsed;
                self.max_total_hash = hash;
            }
            self.spawned += count;
            self.next_hash += 1;
            budget.complete_unit();
        }

        if self.misses > 0 {
            log::info!(
                "  PreCombined: {} hashes — {} entities spawned, {} misses (#1188)",
                self.total_hashes,
                self.spawned,
                self.misses,
            );
        } else {
            log::info!(
                "  PreCombined: {} hashes — {} entities spawned (#1188)",
                self.total_hashes,
                self.spawned,
            );
        }
        log::info!(
            "precombine_timing: cell={:08X} timed_hashes={} csg_open_ms={:.2} hash_max_ms={:.2} \
             hash={:08x} prepare_max_ms={:.2} spawn_total_max_ms={:.2} \
             spawn_cpu_max_ms={:.2} blas_max_ms={:.2}",
            self.form_id,
            self.timed_hashes,
            self.csg_open.as_secs_f64() * 1000.0,
            self.max_total.as_secs_f64() * 1000.0,
            self.max_total_hash,
            self.max_prepare.as_secs_f64() * 1000.0,
            self.max_spawn.as_secs_f64() * 1000.0,
            self.max_spawn_cpu.as_secs_f64() * 1000.0,
            self.max_blas.as_secs_f64() * 1000.0,
        );

        PrecombinedSpawnProgress::Complete {
            spawned: self.spawned,
            misses: self.misses,
        }
    }

    /// Open (once per job) every `<Plugin> - Geometry.csg` named by
    /// `filename_hashes`, returning the subset that resolved.
    ///
    /// Each first open counts as a cooperative unit so a cold session's
    /// chunk-table read shows up against the frame budget rather than
    /// disappearing into the hash that happened to trigger it.
    fn open_csgs_for(
        &mut self,
        filename_hashes: &[u32],
        mut mat_provider: Option<&mut MaterialProvider>,
        budget: &mut FrameTimeBudget,
    ) -> std::collections::HashMap<u32, Arc<CsgArchive>> {
        let mut out = std::collections::HashMap::new();
        for &name_hash in filename_hashes {
            if !self.csg.open.contains_key(&name_hash) {
                let opened = match self.csg.paths.get(&name_hash) {
                    Some(plugin_path) => {
                        let started = Instant::now();
                        let opened = match mat_provider.as_deref_mut() {
                            Some(mp) => mp.geometry_csg(plugin_path),
                            None => open_geometry_csg(plugin_path).map(Arc::new),
                        };
                        self.csg_open += started.elapsed();
                        budget.complete_unit();
                        opened
                    }
                    None => {
                        // A blob named by no loaded plugin — a mod shipping its
                        // own `.csg` that wasn't passed on the command line, or
                        // content from a game we don't have. Failing closed
                        // sends the cell down the per-REFR fallback instead of
                        // reading another plugin's PSG space at these offsets,
                        // which decodes as garbage rather than as nothing.
                        log::debug!(
                            "PreCombined: cell {:08X} names CSG {name_hash:08x}, \
                             which no loaded plugin answers to — falling back to REFRs",
                            self.form_id,
                        );
                        None
                    }
                };
                self.csg.open.insert(name_hash, opened);
            }
            if let Some(Some(csg)) = self.csg.open.get(&name_hash) {
                out.insert(name_hash, csg.clone());
            }
        }
        out
    }
}

/// Index the load order by the `BSPackedGeomObject::filename_hash` each
/// plugin's companion `<Plugin> - Geometry.csg` answers to (#2369).
///
/// `owner_path` is the cell's owning plugin, included so a single-plugin load
/// whose path never appears in `load_order_paths` still resolves. Earlier
/// entries win a hash collision, matching load-order precedence.
fn csg_paths_by_name_hash(
    load_order_paths: &[&str],
    owner_path: &str,
) -> std::collections::HashMap<u32, String> {
    let mut out = std::collections::HashMap::new();
    for path in load_order_paths.iter().copied().chain([owner_path]) {
        if let Some(h) = byroredux_bsa::csg_name_hash(path) {
            out.entry(h).or_insert_with(|| path.to_owned());
        }
    }
    out
}

/// Spawn all precombined `_oc.nif` files without a frame deadline.
///
/// Interior and initial bulk loads retain their synchronous behaviour; the
/// exterior streaming path owns a [`PrecombinedSpawnJob`] directly and yields
/// between hashes, imported meshes, and bounded BLAS batches.
#[allow(clippy::too_many_arguments)]
pub(super) fn spawn_precombined_meshes(
    cell: &CellData,
    cell_origin: Vec3,
    world: &mut World,
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    mut mat_provider: Option<&mut MaterialProvider>,
    plugin_path: &str,
    load_order_paths: &[&str],
) -> (usize, usize) {
    let Some(mut job) = PrecombinedSpawnJob::new(cell, plugin_path, load_order_paths) else {
        return (0, 0);
    };
    let mut budget = FrameTimeBudget::unlimited();
    loop {
        match job.advance(
            cell,
            cell_origin,
            world,
            ctx,
            tex_provider,
            mat_provider.as_deref_mut(),
            &mut budget,
        ) {
            PrecombinedSpawnProgress::Complete { spawned, misses } => {
                return (spawned, misses);
            }
            PrecombinedSpawnProgress::Pending(next) => {
                // Only finite exterior budgets apply the BLAS mesh-count cap;
                // this branch remains defensive for future cursor phases.
                job = next;
            }
        }
    }
}

/// On-disk archive path for a cell's precombined `_oc.nif`.
///
/// Bethesda keys precombine filenames by the cell's lower 24 form-id bits
/// with the mod-index byte forced to `00`, and namespaces DLC bakes under a
/// `<owning-plugin>.esm\` subdirectory while base-game (`Fallout4.esm`)
/// bakes live at the `meshes\precombined\` root. Verified against the real
/// `Fallout4 - MeshesExtra.ba2` (120 k names, all at root) and
/// `DLCCoast - Main.ba2` / `DLCRobot - Main.ba2` (own cells under
/// `dlccoast.esm\` / `dlcrobot.esm\`; their base-game-cell overrides at the
/// root). The cell loader holds the *remapped* (global-load-order)
/// `cell.form_id`, whose top byte is the owner's global slot; masking it off
/// recovers the baked filename, and `owning_subdir` (the owner's lowercased
/// basename, or `None` for the base master) restores the namespace. For the
/// base game both reduce to the pre-fix path. #1590.
fn precombine_oc_nif_path(form_id: u32, hash: u32, owning_subdir: Option<&str>) -> String {
    let local = form_id & 0x00FF_FFFF;
    match owning_subdir {
        Some(sub) => format!("meshes\\precombined\\{sub}\\{local:08x}_{hash:08x}_oc.nif"),
        None => format!("meshes\\precombined\\{local:08x}_{hash:08x}_oc.nif"),
    }
}

/// Resolve, from a cell's remapped form id, the path of the plugin that
/// *owns* it and the `meshes\precombined\` subdirectory its baked `_oc.nif`
/// lives under. The owner is the load-order plugin at the form-id mod-index
/// byte; `load_order_paths` is the correctly-cased, load-order-aligned plugin
/// list. The base master (index 0, `Fallout4.esm`) bakes at the root (`None`
/// subdir); every other owner namespaces under its lowercased basename. When
/// the index is out of range (the standalone-form-id artifact the remap
/// passes through unchanged) we fall back to the active plugin + root. #1590.
///
/// The **filename** is all this decides. Which `<Plugin> - Geometry.csg` the
/// geometry then comes out of is a separate question, answered by the
/// `BSPackedGeomObject::filename_hash` each object carries — see
/// [`csg_paths_by_name_hash`]. The two diverge on the override-rebake case: a
/// plugin re-baking a master-owned cell keeps the master's root filename (so
/// `owning_subdir` stays `None`) while storing the new geometry in its own
/// blob. Measured on the installed FO4 set, the DLCs alone re-bake ~460
/// `Fallout4.esm`-owned cells that way (#2369).
fn resolve_precombine_owner<'a>(
    form_id: u32,
    load_order_paths: &[&'a str],
    fallback: &'a str,
) -> (&'a str, Option<String>) {
    let idx = (form_id >> 24) as usize;
    match load_order_paths.get(idx) {
        Some(&path) if idx == 0 => (path, None),
        Some(&path) => (path, Some(super::load_order::plugin_basename_lc(path))),
        None => (fallback, None),
    }
}

/// Open the `<Plugin> - Geometry.csg` blob that sits next to `plugin_path`
/// in the Data directory (M49). The caller passes the cell's *owning*
/// plugin path (resolved via [`resolve_precombine_owner`]) so the CSG named for
/// the cell's master (`Fallout4 - Geometry.csg`, `DLCCoast - Geometry.csg`,
/// …) is opened rather than the last-loaded plugin's. Returns `None` when
/// the plugin has no companion CSG (non-FO4 content, or a plugin that
/// authored no shared precombines) — the caller then falls back to per-REFR
/// rendering.
pub(crate) fn open_geometry_csg(plugin_path: &str) -> Option<CsgArchive> {
    let p = Path::new(plugin_path);
    let dir = p.parent()?;
    let stem = p.file_stem()?.to_str()?;
    let csg_path = dir.join(format!("{stem} - Geometry.csg"));
    if !csg_path.is_file() {
        return None;
    }
    match CsgArchive::open(&csg_path) {
        Ok(a) => {
            log::info!(
                "PreCombined: opened CSG '{}' ({} objects, {} chunks)",
                csg_path.display(),
                a.num_objects(),
                a.num_chunks(),
            );
            Some(a)
        }
        Err(e) => {
            log::warn!(
                "PreCombined: failed to open CSG '{}': {e}",
                csg_path.display()
            );
            None
        }
    }
}

/// Resolve every `BSPackedCombinedSharedGeomDataExtra` object in a
/// precombined `_oc.nif` scene, producing one spawnable [`ImportedMesh`] per
/// placed instance (M49). Pure (no GPU / ECS) so it is unit-testable against
/// real data without a Vulkan device.
///
/// `resolve_csg` maps a `BSPackedGeomObject::filename_hash` to the blob that
/// answers to it. Objects naming a blob the caller couldn't open are skipped
/// rather than read out of whichever CSG happened to be open — a `data_offset`
/// is only meaningful in its own PSG space, so the wrong blob yields garbage,
/// not an error (#2369).
///
/// Each object's geometry is decoded once and cloned per
/// `BSPackedGeomDataCombined` instance transform. Objects whose CSG slice
/// is missing or fails to decode are skipped with a debug log rather than
/// aborting the whole bake. The Baked variant
/// (`BSPackedCombinedGeomDataExtra`, geometry inline) is not vanilla and
/// is left for a follow-up.
pub(super) fn build_precombine_meshes(
    scene: &NifScene,
    resolve_csg: &dyn Fn(u32) -> Option<Arc<CsgArchive>>,
    pool: &mut StringPool,
) -> (Vec<ImportedMesh>, Vec<u32>) {
    let mut meshes = Vec::new();
    // #3510 — `geometry_dedup[i]` is the index of the mesh whose geometry
    // mesh `i` shares. Every instance of one object points at that object's
    // first mesh, so the N byte-identical clones below collapse to one
    // upload and one BLAS while each keeps its own entity and `Transform`.
    let mut geometry_dedup: Vec<u32> = Vec::new();
    // `collect_precombine_geom_refs` pairs each shared-geometry object with
    // the material the owning shape's shader/alpha properties resolve to
    // (M49 texturing) — so precombines render with their real diffuse /
    // normal / alpha-test instead of the untextured placeholder.
    for geom in byroredux_nif::import::precombine::collect_precombine_geom_refs(scene, pool) {
        if geom.num_verts == 0 {
            continue;
        }
        let stride = psg_vertex_stride(geom.vertex_desc);
        // The 3 LODs are alternative triangulations of the SAME surface
        // (nif.xml: "switch a geometry at a specified distance"), stored
        // back-to-back as `[LOD0][LOD1][LOD2]` in one index buffer.
        // Rendering more than one z-fights — pick the finest (highest
        // triangle count); LOD index is NOT a reliable detail order (some
        // objects ship lod0 ≫ lod2, others lod0 ≪ lod2). The chosen LOD's
        // triangles start at its index-unit offset / 3.
        let (lod_count, lod_off_idx) = (0..3)
            .map(|i| (geom.lod_counts[i], geom.lod_offsets[i]))
            .max_by_key(|&(c, _)| c)
            .unwrap();
        let lod_count = lod_count as usize;
        if lod_count == 0 {
            continue;
        }
        let tri_start = (lod_off_idx / 3) as usize;
        let need = geom.num_verts * stride + (tri_start + lod_count) * 6;
        let Some(csg) = resolve_csg(geom.filename_hash) else {
            log::debug!(
                "PreCombined: object names CSG {:08x}, which is not open — skipped",
                geom.filename_hash,
            );
            continue;
        };
        let psg = match csg.read_psg(geom.data_offset as u64, need) {
            Ok(b) => b,
            Err(e) => {
                log::debug!(
                    "PreCombined: CSG read at offset {} failed: {e}",
                    geom.data_offset
                );
                continue;
            }
        };
        let decoded = match decode_shared_geom_object(
            &psg,
            geom.vertex_desc,
            geom.num_verts,
            tri_start,
            lod_count,
        ) {
            Ok(g) => g,
            Err(e) => {
                log::debug!(
                    "PreCombined: decode at offset {} failed: {e}",
                    geom.data_offset
                );
                continue;
            }
        };
        // One placed instance per combined transform, each carrying the
        // resolved material. Objects with no combined entries carry no
        // placement (an unplaced merge) and contribute nothing.
        // The first instance of this object is its own representative;
        // every later one points back at it. `into_imported_mesh` only sets
        // `translation`/`rotation`/`scale` — the vertex data it is handed is
        // the same decoded buffer each time — so they really are identical
        // geometry, which is the premise the dedup rests on.
        let representative = meshes.len() as u32;
        for inst in &geom.instances {
            let mesh = decoded
                .clone()
                .into_imported_mesh(&inst.transform, geom.material.clone());
            meshes.push(mesh);
            geometry_dedup.push(representative);
        }
    }
    debug_assert_eq!(meshes.len(), geometry_dedup.len());
    (meshes, geometry_dedup)
}

/// Wrap precombine-decoded meshes in a geometry-only [`CachedNifImport`]
/// (no collisions / lights / clips / particles) so the existing
/// [`spawn_placed_instances`] path uploads + spawns them.
fn geometry_only_cached(meshes: Vec<ImportedMesh>, geometry_dedup: Vec<u32>) -> CachedNifImport {
    CachedNifImport {
        meshes,
        geometry_dedup,
        collisions: Vec::new(),
        collision_authoring: Default::default(),
        lights: Vec::new(),
        particle_emitters: Vec::new(),
        embedded_clip: None,
        placement_root_billboard: None,
        speedtree_wind: None,
        bsx_flags: 0,
        root_flags: 0,
        flame_attach_offset: None,
        // Precombines are baked static architecture — no FO4 weapon-mod
        // connect points. #1594.
        attach_points: None,
        child_attach_connections: None,
        furniture: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_bsa::Ba2Archive;
    use std::path::PathBuf;

    /// #1590 (b) — the baked `_oc.nif` path drops the form-id mod-index byte
    /// and namespaces DLC bakes under a `<plugin>.esm\` subdir. Verified
    /// against the real `Fallout4 - MeshesExtra.ba2` (root) and
    /// `DLCCoast - Main.ba2` / `DLCRobot - Main.ba2` (own cells under
    /// `dlccoast.esm\` / `dlcrobot.esm\`, e.g. `…\dlccoast.esm\00000da8_…`).
    #[test]
    fn oc_nif_path_base_game_stays_at_root() {
        // Base master (no subdir) — unchanged from the pre-fix format!().
        assert_eq!(
            precombine_oc_nif_path(0x0000_e2db, 0x02be_5e11, None),
            "meshes\\precombined\\0000e2db_02be5e11_oc.nif"
        );
    }

    #[test]
    fn oc_nif_path_dlc_uses_subdir_and_zeroes_mod_byte() {
        // A DLCCoast cell remapped to global slot 2: the pre-fix code emitted
        // `meshes\precombined\020034a6_…` (wrong byte, no subdir) and missed.
        // The baked path is `…\dlccoast.esm\000034a6_…`.
        assert_eq!(
            precombine_oc_nif_path(0x0200_34a6, 0xb831_aac9, Some("dlccoast.esm")),
            "meshes\\precombined\\dlccoast.esm\\000034a6_b831aac9_oc.nif"
        );
    }

    /// #1590 (a) — the CSG + subdir follow the cell's owning plugin (form-id
    /// mod-index byte → load order), not the last-loaded `--esm`.
    #[test]
    fn resolve_precombine_owner_follows_form_id_mod_index() {
        let paths = [
            "/Data/Fallout4.esm",
            "/Data/DLCRobot.esm",
            "/Data/DLCCoast.esm",
        ];
        let fallback = "/Data/DLCCoast.esm";
        // A base-game cell (mod index 0) resolves Fallout4 + root even when a
        // DLC is the active plugin — pre-fix opened the active plugin's CSG.
        assert_eq!(
            resolve_precombine_owner(0x0000_e2db, &paths, fallback),
            ("/Data/Fallout4.esm", None)
        );
        // A DLCRobot-owned cell at global slot 1 → its CSG + `dlcrobot.esm\`.
        assert_eq!(
            resolve_precombine_owner(0x0100_2345, &paths, fallback),
            ("/Data/DLCRobot.esm", Some("dlcrobot.esm".to_string()))
        );
        // Out-of-range mod index (standalone-artifact form) → fallback + root.
        assert_eq!(
            resolve_precombine_owner(0x7f00_0001, &paths, fallback),
            (fallback, None)
        );
        // Single-plugin load: one-element slice, owner is slot 0 → root.
        assert_eq!(
            resolve_precombine_owner(0x0000_1234, &["/Data/Fallout4.esm"], "/Data/Fallout4.esm"),
            ("/Data/Fallout4.esm", None)
        );
    }

    /// #2369 — every plugin in the load order contributes its own CSG name
    /// hash, so a re-bake can be traced back to the plugin that made it even
    /// when the cell belongs to a master.
    #[test]
    fn csg_paths_index_every_plugin_in_the_load_order() {
        let paths = ["/Data/Fallout4.esm", "/Data/DLCCoast.esm"];
        let table = csg_paths_by_name_hash(&paths, paths[0]);
        assert_eq!(
            table.get(&0xddf1_9a67).map(String::as_str),
            Some("/Data/Fallout4.esm"),
        );
        assert_eq!(
            table.get(&0x2088_054d).map(String::as_str),
            Some("/Data/DLCCoast.esm"),
            "a DLC that re-bakes a master-owned cell must still be reachable"
        );
    }

    /// A plugin passed as the active `--esm` but absent from the load-order
    /// slice still needs its own blob reachable — otherwise a single-plugin
    /// load resolves nothing.
    #[test]
    fn csg_paths_include_the_owning_plugin() {
        let table = csg_paths_by_name_hash(&[], "/Data/DLCRobot.esm");
        assert_eq!(
            table.get(&0x3a1b_90b8).map(String::as_str),
            Some("/Data/DLCRobot.esm"),
        );
    }

    /// Test resolver over a single blob, keyed by the real BSCRC32 of the
    /// plugin that owns it — so a decode that reaches the CSG proves the
    /// `_oc.nif` really does name *that* plugin's geometry (#2369).
    fn one_csg(plugin_path: &str, csg: CsgArchive) -> impl Fn(u32) -> Option<Arc<CsgArchive>> {
        let want = byroredux_bsa::csg_name_hash(plugin_path).expect("plugin stem");
        let csg = Arc::new(csg);
        move |h| (h == want).then(|| csg.clone())
    }

    fn fo4_data_dir() -> Option<PathBuf> {
        if let Ok(v) = std::env::var("BYROREDUX_FO4_DATA") {
            let p = PathBuf::from(&v);
            if p.is_dir() {
                return Some(p);
            }
        }
        let p = PathBuf::from("/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data");
        p.is_dir().then_some(p)
    }

    /// Real-data, Vulkan-free regression for the M49 spawn path's decode
    /// half: a vanilla FO4 `_oc.nif` + `Fallout4 - Geometry.csg` must
    /// yield non-empty, index-valid meshes. Gated on `BYROREDUX_FO4_DATA`:
    /// `cargo test -p byroredux -- --ignored build_precombine_meshes`.
    #[test]
    #[ignore]
    fn build_precombine_meshes_decodes_real_oc_nif() {
        let Some(data) = fo4_data_dir() else {
            eprintln!("Skipping: BYROREDUX_FO4_DATA not set and default path missing");
            return;
        };
        let ba2 = match Ba2Archive::open(data.join("Fallout4 - MeshesExtra.ba2")) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Skipping: open MeshesExtra.ba2: {e}");
                return;
            }
        };
        let csg = match CsgArchive::open(data.join("Fallout4 - Geometry.csg")) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping: open Geometry.csg: {e}");
                return;
            }
        };

        let bytes = ba2
            .extract("meshes\\precombined\\0000e2db_02be5e11_oc.nif")
            .expect("extract _oc.nif");
        let scene = byroredux_nif::parse_nif(&bytes).expect("parse _oc.nif");
        let mut pool = StringPool::new();
        let resolve = one_csg(data.join("Fallout4.esm").to_str().unwrap(), csg);
        let (meshes, _dedup) = build_precombine_meshes(&scene, &resolve, &mut pool);

        assert!(
            !meshes.is_empty(),
            "shared precombine must decode at least one mesh from the CSG"
        );
        let mut textured = 0usize;
        for m in &meshes {
            assert!(!m.positions.is_empty(), "mesh has vertices");
            assert!(!m.indices.is_empty(), "mesh has indices");
            assert_eq!(m.normals.len(), m.positions.len(), "normal per vertex");
            let max_idx = m.indices.iter().copied().max().unwrap();
            assert!(
                (max_idx as usize) < m.positions.len(),
                "index {max_idx} in range for {} verts",
                m.positions.len()
            );
            if m.material.textures.base_color.is_some() {
                textured += 1;
            }
        }
        // M49 texturing: this object's shape resolves a real diffuse path
        // (Landscape/Rocks/CoastCliff01Wet_d.dds), so the mesh must carry it.
        assert!(
            textured > 0,
            "precombine meshes must resolve a diffuse texture from the owning shape"
        );
        eprintln!(
            "build_precombine_meshes: decoded {} mesh(es), {textured} textured",
            meshes.len()
        );
    }

    /// #2369 — the override-rebake case, against real data.
    ///
    /// Far Harbor re-bakes `Fallout4.esm`-owned Commonwealth cells: the new
    /// `_oc.nif` keeps the root `meshes\precombined\` name (so the cell's
    /// form-id mod byte still says `Fallout4.esm`) but its geometry lives in
    /// `DLCCoast - Geometry.csg`. Routing by the cell owner decodes **zero**
    /// meshes for these; routing by the object's own `filename_hash` decodes
    /// them. Both halves are asserted so the test fails if either the wrong
    /// blob starts "working" or the right one stops.
    ///
    /// Gated on the installed FO4 data:
    /// `cargo test -p byroredux -- --ignored dlc_rebake`.
    #[test]
    #[ignore]
    fn dlc_rebake_of_a_master_owned_cell_decodes_from_the_dlc_csg() {
        let Some(data) = fo4_data_dir() else {
            eprintln!("Skipping: BYROREDUX_FO4_DATA not set and default path missing");
            return;
        };
        let (Ok(ba2), Ok(dlc_csg), Ok(base_csg)) = (
            Ba2Archive::open(data.join("DLCCoast - Main.ba2")),
            CsgArchive::open(data.join("DLCCoast - Geometry.csg")),
            CsgArchive::open(data.join("Fallout4 - Geometry.csg")),
        ) else {
            eprintln!("Skipping: Far Harbour archives not installed");
            return;
        };
        let coast_hash = byroredux_bsa::csg_name_hash("DLCCoast.esm").unwrap();

        // A ROOT-level (i.e. Fallout4.esm-owned cell) precombine shipped by
        // Far Harbour whose objects name Far Harbour's own blob.
        let rebake = ba2
            .list_files()
            .into_iter()
            .filter(|n| {
                n.starts_with("meshes\\precombined\\")
                    && n.ends_with("_oc.nif")
                    && n.matches('\\').count() == 2
            })
            .find(|n| {
                ba2.extract(n)
                    .ok()
                    .and_then(|b| byroredux_nif::parse_nif(&b).ok())
                    .is_some_and(|s| {
                        byroredux_nif::import::precombine::precombine_csg_filename_hashes(&s)
                            == [coast_hash]
                    })
            })
            .expect("Far Harbour re-bakes at least one master-owned cell");

        let bytes = ba2.extract(rebake).expect("extract re-baked _oc.nif");
        let scene = byroredux_nif::parse_nif(&bytes).expect("parse re-baked _oc.nif");

        let mut pool = StringPool::new();
        let base = Arc::new(base_csg);
        let wrong_blob = |_| Some(base.clone());
        let (wrong, _dedup) = build_precombine_meshes(&scene, &wrong_blob, &mut pool);
        assert!(
            wrong.is_empty(),
            "'{rebake}' must not decode out of Fallout4 - Geometry.csg — \
             that is the #2369 bug, and a PSG offset read against the wrong \
             blob is garbage rather than an error"
        );

        let mut pool = StringPool::new();
        let resolve = one_csg(data.join("DLCCoast.esm").to_str().unwrap(), dlc_csg);
        let (right, _dedup) = build_precombine_meshes(&scene, &resolve, &mut pool);
        assert!(
            !right.is_empty(),
            "'{rebake}' decodes against the blob its own objects name"
        );
        eprintln!("dlc re-bake '{rebake}' → {} mesh(es)", right.len());
    }

    /// Switchboard transform diagnostic. The packed record's authored
    /// bounding sphere lets us compare matrix interpretations without a
    /// renderer or a subjective screenshot. Kept ignored because it needs
    /// the installed FO4 archives.
    #[test]
    #[ignore]
    fn switchboard_precombine_transforms_match_authored_bounds() {
        let Some(data) = fo4_data_dir() else {
            eprintln!("Skipping: no FO4 data dir");
            return;
        };
        let ba2 = Ba2Archive::open(data.join("Fallout4 - MeshesExtra.ba2"))
            .expect("open MeshesExtra.ba2");
        let csg =
            CsgArchive::open(data.join("Fallout4 - Geometry.csg")).expect("open Geometry.csg");
        let paths: Vec<_> = ba2
            .list_files()
            .into_iter()
            .filter(|name| {
                name.starts_with("meshes\\precombined\\000b42e4_") && name.ends_with("_oc.nif")
            })
            .collect();
        assert!(!paths.is_empty(), "Switchboard precombine NIFs present");

        fn transpose(t: &byroredux_nif::types::NiTransform) -> byroredux_nif::types::NiTransform {
            let mut out = *t;
            for row in 0..3 {
                for col in 0..3 {
                    out.rotation.rows[row][col] = t.rotation.rows[col][row];
                }
            }
            out
        }

        fn bound_error(mesh: &ImportedMesh, authored: [f32; 4]) -> (f32, f32) {
            let q = Quat::from_xyzw(
                mesh.rotation[0],
                mesh.rotation[1],
                mesh.rotation[2],
                mesh.rotation[3],
            );
            let local = Vec3::from_array(mesh.local_bound_center) * mesh.scale;
            let placed = Vec3::from_array(mesh.translation) + q * local;
            let authored_center = Vec3::new(authored[0], authored[2], -authored[1]);
            let center_error = placed.distance(authored_center);
            let radius_error = (mesh.local_bound_radius * mesh.scale.abs() - authored[3]).abs();
            (center_error, radius_error)
        }

        fn sphere_overrun(mesh: &ImportedMesh, authored: [f32; 4]) -> f32 {
            let q = Quat::from_xyzw(
                mesh.rotation[0],
                mesh.rotation[1],
                mesh.rotation[2],
                mesh.rotation[3],
            );
            let translation = Vec3::from_array(mesh.translation);
            let center = Vec3::new(authored[0], authored[2], -authored[1]);
            let furthest = mesh
                .positions
                .iter()
                .map(|&p| {
                    let placed = translation + q * (Vec3::from_array(p) * mesh.scale);
                    placed.distance(center)
                })
                .fold(0.0f32, f32::max);
            (furthest - authored[3]).max(0.0)
        }

        let mut current = Vec::new();
        let mut transposed = Vec::new();
        let mut radius = Vec::new();
        let mut current_overrun = Vec::new();
        let mut transposed_overrun = Vec::new();
        let mut objects = 0usize;
        let mut instances = 0usize;
        let mut pool = StringPool::new();
        for path in &paths {
            let bytes = ba2.extract(path).expect("extract Switchboard _oc.nif");
            let scene = byroredux_nif::parse_nif(&bytes).expect("parse Switchboard _oc.nif");
            for geom in
                byroredux_nif::import::precombine::collect_precombine_geom_refs(&scene, &mut pool)
            {
                objects += 1;
                if geom.num_verts == 0 {
                    continue;
                }
                let (lod_count, lod_off_idx) = (0..3)
                    .map(|i| (geom.lod_counts[i], geom.lod_offsets[i]))
                    .max_by_key(|&(count, _)| count)
                    .unwrap();
                if lod_count == 0 {
                    continue;
                }
                let stride = psg_vertex_stride(geom.vertex_desc);
                let tri_start = (lod_off_idx / 3) as usize;
                let need = geom.num_verts * stride + (tri_start + lod_count as usize) * 6;
                let psg = csg
                    .read_psg(geom.data_offset as u64, need)
                    .expect("read Switchboard PSG");
                let decoded = decode_shared_geom_object(
                    &psg,
                    geom.vertex_desc,
                    geom.num_verts,
                    tri_start,
                    lod_count as usize,
                )
                .expect("decode Switchboard PSG");
                for inst in &geom.instances {
                    instances += 1;
                    let regular = decoded
                        .clone()
                        .into_imported_mesh(&inst.transform, geom.material.clone());
                    let flipped = decoded
                        .clone()
                        .into_imported_mesh(&transpose(&inst.transform), geom.material.clone());
                    let (ce, re) = bound_error(&regular, inst.bounding_sphere);
                    current.push(ce);
                    radius.push(re);
                    transposed.push(bound_error(&flipped, inst.bounding_sphere).0);
                    current_overrun.push(sphere_overrun(&regular, inst.bounding_sphere));
                    transposed_overrun.push(sphere_overrun(&flipped, inst.bounding_sphere));
                }
            }
        }
        current.sort_by(f32::total_cmp);
        transposed.sort_by(f32::total_cmp);
        radius.sort_by(f32::total_cmp);
        current_overrun.sort_by(f32::total_cmp);
        transposed_overrun.sort_by(f32::total_cmp);
        let percentile =
            |values: &[f32], p: f32| values[((values.len() - 1) as f32 * p).round() as usize];
        eprintln!(
            "Switchboard: {} NIFs, {objects} objects, {instances} instances; \
             center error current p50={:.3} p90={:.3} max={:.3}; \
             transpose p50={:.3} p90={:.3} max={:.3}; \
             radius error p50={:.3} p90={:.3} max={:.3}; \
             sphere overrun current p50={:.3} p90={:.3} max={:.3}; \
             transpose p50={:.3} p90={:.3} max={:.3}",
            paths.len(),
            percentile(&current, 0.5),
            percentile(&current, 0.9),
            current.last().copied().unwrap_or_default(),
            percentile(&transposed, 0.5),
            percentile(&transposed, 0.9),
            transposed.last().copied().unwrap_or_default(),
            percentile(&radius, 0.5),
            percentile(&radius, 0.9),
            radius.last().copied().unwrap_or_default(),
            percentile(&current_overrun, 0.5),
            percentile(&current_overrun, 0.9),
            current_overrun.last().copied().unwrap_or_default(),
            percentile(&transposed_overrun, 0.5),
            percentile(&transposed_overrun, 0.9),
            transposed_overrun.last().copied().unwrap_or_default(),
        );
        assert!(
            percentile(&current_overrun, 0.9) < 0.05,
            "current packed transforms must keep decoded geometry within authored spheres"
        );
        assert!(
            percentile(&transposed_overrun, 0.9) > 50.0,
            "regression fixture must distinguish the incorrect matrix transpose"
        );
    }

    /// #1590 — end-to-end DLC regression. A DLCCoast-owned cell loaded with
    /// `Fallout4.esm` as master gets a *remapped* form id (top byte = global
    /// slot 1). `resolve_precombine_owner` + `precombine_oc_nif_path` must
    /// reproduce the real on-disk `meshes\precombined\dlccoast.esm\<low24>_…`
    /// path (pre-fix the loader emitted `…\01……` with no subdir and missed),
    /// and the geometry must decode against `DLCCoast - Geometry.csg` — the
    /// owning plugin's CSG, not the active plugin's. Gated on real data.
    #[test]
    #[ignore]
    fn dlc_precombine_path_and_csg_resolve_end_to_end() {
        let Some(data) = fo4_data_dir() else {
            eprintln!("Skipping: no FO4 data dir");
            return;
        };
        let ba2 = match Ba2Archive::open(data.join("DLCCoast - Main.ba2")) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("Skipping: open DLCCoast - Main.ba2: {e}");
                return;
            }
        };
        let csg = match CsgArchive::open(data.join("DLCCoast - Geometry.csg")) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("Skipping: open DLCCoast - Geometry.csg: {e}");
                return;
            }
        };

        // Pick a real DLC-owned (subdir) precombine and parse its baked
        // <low24>_<hash> off the filename.
        let on_disk = ba2
            .list_files()
            .into_iter()
            .find(|n| n.contains("\\dlccoast.esm\\") && n.ends_with("_oc.nif"))
            .expect("a DLCCoast-owned precombine name")
            .to_string();
        let stem = on_disk
            .rsplit('\\')
            .next()
            .unwrap()
            .trim_end_matches("_oc.nif");
        let (low24_hex, hash_hex) = stem.split_once('_').expect("<low24>_<hash>");
        let low24 = u32::from_str_radix(low24_hex, 16).unwrap();
        let hash = u32::from_str_radix(hash_hex, 16).unwrap();
        assert_eq!(low24 >> 24, 0, "on-disk form id has a zeroed mod byte");

        // Simulate the load `--master Fallout4.esm --esm DLCCoast.esm`: the
        // cell record's form id is remapped to DLCCoast's global slot (1).
        let f4 = data.join("Fallout4.esm");
        let coast = data.join("DLCCoast.esm");
        let load_order: [&str; 2] = [f4.to_str().unwrap(), coast.to_str().unwrap()];
        let remapped_form_id = (1u32 << 24) | low24;

        let (owning_path, subdir) =
            resolve_precombine_owner(remapped_form_id, &load_order, load_order[1]);
        assert_eq!(
            owning_path, load_order[1],
            "owner is DLCCoast, not the master"
        );
        assert_eq!(subdir.as_deref(), Some("dlccoast.esm"));

        let built = precombine_oc_nif_path(remapped_form_id, hash, subdir.as_deref());
        assert_eq!(
            built, on_disk,
            "reconstructed path matches the on-disk name"
        );

        // And the geometry decodes against the OWNING plugin's CSG.
        let bytes = ba2.extract(&built).expect("extract via reconstructed path");
        let scene = byroredux_nif::parse_nif(&bytes).expect("parse _oc.nif");
        let mut pool = StringPool::new();
        let resolve = one_csg(coast.to_str().unwrap(), csg);
        let (meshes, _dedup) = build_precombine_meshes(&scene, &resolve, &mut pool);
        assert!(
            !meshes.is_empty(),
            "DLC precombine decodes against its own DLCCoast - Geometry.csg"
        );
        eprintln!(
            "dlc path '{built}' → {} mesh(es) from DLCCoast CSG",
            meshes.len()
        );
    }
}

/// #3510 — the geometry-dedup mapping `build_precombine_meshes` emits.
///
/// A precombine `_oc.nif` materialises one `ImportedMesh` per
/// `BSPackedGeomDataCombined` instance transform, all cloned from one
/// decoded shared geometry. Without the mapping each claimed a distinct
/// `(path, sub_mesh_index)` cache slot — N uploads, N BLAS builds, and no
/// instanced draw merging, measured at 1.37x VRAM overall and 4.6x on the
/// worst tile.
#[cfg(test)]
mod geometry_dedup_tests {
    use crate::cell_loader::spawn::mesh_instance::geometry_dedup_active;

    /// The mapping is only safe to index when it came from the same mesh
    /// list, and only useful when there is a cache entry to redirect to.
    #[test]
    fn dedup_requires_both_a_cache_key_and_a_matching_length() {
        assert!(geometry_dedup_active(true, 4, 4));
        // No cache key — nothing to acquire a shared handle through.
        assert!(!geometry_dedup_active(false, 4, 4));
        // A mapping from a different mesh list would pair instances with
        // unrelated geometry; not deduping is the safe outcome.
        assert!(!geometry_dedup_active(true, 3, 4));
        assert!(!geometry_dedup_active(true, 5, 4));
        // The ordinary path: every other importer leaves the vector empty.
        assert!(!geometry_dedup_active(true, 0, 4));
        // Degenerate but consistent — an empty import deduplicates nothing.
        assert!(geometry_dedup_active(true, 0, 0));
    }

    /// The shape `build_precombine_meshes` guarantees: contiguous runs, each
    /// pointing at the run's own first index, so an instance never redirects
    /// forward or across objects.
    #[test]
    fn representative_indices_are_self_referential_run_starts() {
        // Two objects: 3 instances then 2.
        let dedup: Vec<u32> = vec![0, 0, 0, 3, 3];
        for (i, &rep) in dedup.iter().enumerate() {
            assert!(
                (rep as usize) <= i,
                "instance {i} redirects forward to {rep} — a representative \
                 must already have been uploaded when its instances resolve"
            );
            assert_eq!(
                dedup[rep as usize], rep,
                "representative {rep} must point at itself, or the redirect \
                 chains instead of resolving in one hop"
            );
        }
        let unique = dedup.iter().collect::<std::collections::BTreeSet<_>>();
        assert_eq!(unique.len(), 2, "5 instances collapse to 2 uploads");
    }
}
