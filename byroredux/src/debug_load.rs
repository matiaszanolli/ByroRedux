//! Drain the [`PendingDebugLoadSlot`] and dispatch each queued load
//! op to the existing loader primitives.
//!
//! The debug-server can only enqueue ops (it holds `&World`, not the
//! `&mut World + &mut VulkanContext + Provider`s that the actual
//! loaders require). This module consumes the queue between frames,
//! where the App holds full mutable access. Mirrors the deferred-
//! execution shape of [`crate::cell_loader::step_cell_transition`] —
//! same pattern, separate slot.
//!
//! NIF loading falls through the existing `load_nif_bytes` entry: try
//! a loose file relative to the startup asset roots first, then walk every
//! `--bsa` CLI arg the engine was launched with. The per-request `bsas`
//! field on cell requests is honoured by synthesising a one-shot args list
//! and reusing the same `build_texture_provider` / `build_material_provider`
//! helpers boot-time uses.
//!
//! Every client-supplied file path — the NIF, and a cell load's ESM,
//! masters and archives — is confined to the directories of the engine's
//! startup `--esm` / `--master` / `--bsa` args ([`confine_to_roots`]): the
//! debug server is unauthenticated, so it must not be able to aim the
//! untrusted-input parsers at an arbitrary file (#4752, #5165).

use byroredux_core::ecs::debug_load::{PendingDebugLoad, PendingDebugLoadSlot};
use byroredux_core::ecs::{Resource, World};
use byroredux_renderer::VulkanContext;

use crate::asset_provider::{build_material_provider, build_texture_provider};
use crate::cell_loader;
use crate::streaming;
use crate::streaming_helpers::{drain_streaming_state, SVGF_TAA_STREAMING_RECOVERY_FRAMES};

/// Drain every queued load op. Always returns `Ok(loads_processed)`
/// even when individual loads fail — failures are logged with the
/// label so the operator's console output points at the cause. The
/// engine never aborts on a debug-load error.
pub fn execute_pending_debug_loads(
    world: &mut World,
    ctx: &mut VulkanContext,
    streaming: &mut Option<streaming::WorldStreamingState>,
) -> usize {
    let loads = {
        let Some(mut slot) = world.try_resource_mut::<PendingDebugLoadSlot>() else {
            return 0;
        };
        slot.drain()
    };
    if loads.is_empty() {
        return 0;
    }

    let count = loads.len();
    for load in loads {
        match load {
            PendingDebugLoad::Nif { path, label } => {
                exec_load_nif(world, ctx, &path, label.as_deref());
            }
            PendingDebugLoad::InteriorCell {
                esm,
                cell,
                masters,
                bsas,
                textures_bsas,
            } => {
                let source = match ConfinedLoadSource::resolve(
                    &esm,
                    &masters,
                    &bsas,
                    &textures_bsas,
                    &startup_asset_roots(),
                ) {
                    Ok(source) => source,
                    Err(error) => {
                        log::error!("debug load interior cell '{cell}' rejected: {error}");
                        continue;
                    }
                };
                exec_load_interior(world, ctx, streaming, source.borrow(), &cell);
            }
            PendingDebugLoad::ExteriorCell {
                esm,
                grid_x,
                grid_y,
                radius,
                worldspace,
                masters,
                bsas,
                textures_bsas,
            } => {
                let source = match ConfinedLoadSource::resolve(
                    &esm,
                    &masters,
                    &bsas,
                    &textures_bsas,
                    &startup_asset_roots(),
                ) {
                    Ok(source) => source,
                    Err(error) => {
                        log::error!("debug load exterior ({grid_x},{grid_y}) rejected: {error}");
                        continue;
                    }
                };
                exec_load_exterior(
                    world,
                    ctx,
                    streaming,
                    source.borrow(),
                    DebugExteriorTarget {
                        grid_x,
                        grid_y,
                        radius,
                        worldspace: worldspace.as_deref(),
                    },
                );
            }
        }
    }
    count
}

/// Resolve NIF bytes via loose-file or CLI-BSA search, then call the
/// existing `load_nif_bytes` import path. No new resolver code — the
/// search is the same one `scene::load_nif_from_args` runs at boot,
/// inlined here so a debug load doesn't need to round-trip through
/// the args parser.
fn exec_load_nif(world: &mut World, ctx: &mut VulkanContext, path: &str, label: Option<&str>) {
    let display_label = label.unwrap_or(path);
    let bytes = match resolve_nif_bytes(path) {
        Some(b) => b,
        None => {
            log::error!(
                "debug load NIF '{}': not found as loose file or in any --bsa archive",
                path,
            );
            return;
        }
    };

    let args: Vec<String> = crate::cli_args::effective_args();
    let tex_provider = build_texture_provider(&args);
    let mut mat_provider = build_material_provider(&args);

    let (count, root) = crate::scene::load_nif_bytes(
        world,
        ctx,
        &bytes,
        display_label,
        &tex_provider,
        Some(&mut mat_provider),
    );
    log::info!(
        "debug load NIF '{}': {} entities (root={:?})",
        display_label,
        count,
        root,
    );
    // SVGF / TAA accumulators carry per-pixel history that's no
    // longer correlated with the freshly-spawned mesh — flush the
    // recovery window so the first N frames don't smear motion
    // vectors against history pixels that belonged to the old
    // scene.
    ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES);
}

/// Try `path` as a loose file first; on a miss, scan every `--bsa`
/// CLI arg for a hit.
fn resolve_nif_bytes(path: &str) -> Option<Vec<u8>> {
    if let Some(bytes) = confine_to_roots(path, &startup_asset_roots())
        .and_then(|candidate| std::fs::read(candidate).ok())
    {
        return Some(bytes);
    }
    let args: Vec<String> = crate::cli_args::effective_args();
    for window in args.windows(2) {
        if window[0] != "--bsa" {
            continue;
        }
        let archive_path = &window[1];
        match byroredux_bsa::BsaArchive::open(archive_path) {
            Ok(archive) => {
                if let Ok(data) = archive.extract(path) {
                    log::info!(
                        "debug load NIF '{}': resolved via '{}' ({} bytes)",
                        path,
                        archive_path,
                        data.len()
                    );
                    return Some(data);
                }
            }
            Err(e) => {
                log::warn!(
                    "debug load NIF '{}': failed to open --bsa '{}': {}",
                    path,
                    archive_path,
                    e
                );
            }
        }
    }
    None
}

/// The directories a debug load may read loose files from: the parent of
/// every plugin / archive path the engine was started with. A debug client
/// is unauthenticated (any local process on a debug build), so it names
/// files *relative* to these roots and never chooses an arbitrary path.
fn startup_asset_roots() -> Vec<std::path::PathBuf> {
    let args: Vec<String> = crate::cli_args::effective_args();
    args.windows(2)
        .filter(|pair| {
            matches!(
                pair[0].as_str(),
                "--esm"
                    | "--master"
                    | "--bsa"
                    | "--textures-bsa"
                    | "--scripts-bsa"
                    | "--sounds-bsa"
                    | "--materials-bsa"
            )
        })
        .filter_map(|pair| std::path::Path::new(&pair[1]).parent()?.canonicalize().ok())
        .collect()
}

/// Resolve a client-supplied path under `roots`: it must be relative, made
/// only of normal components (no `..`, no root or prefix), and canonicalize
/// to an existing file still inside the root it was joined to (so a symlink
/// cannot step out). The first root that holds it wins. #4752 / #5165.
fn confine_to_roots(requested: &str, roots: &[std::path::PathBuf]) -> Option<std::path::PathBuf> {
    use std::path::{Component, Path};
    let requested = Path::new(requested);
    if !requested.is_relative()
        || !requested
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    roots.iter().find_map(|root| {
        root.join(requested)
            .canonicalize()
            .ok()
            .filter(|candidate| candidate.starts_with(root) && candidate.is_file())
    })
}

/// A debug cell load's plugin + archive paths after [`confine_to_roots`].
///
/// #5165 — `LoadInteriorCell` / `LoadExteriorCell` used to hand the client's
/// `esm`, `masters`, `bsas` and `textures_bsas` strings straight to the ESM
/// and archive parsers, so any local process could point that untrusted-input
/// code at any file the user can read. Only `LoadNif` was confined (#4752).
/// Every path now goes through the same rule, and one rejected path rejects
/// the whole load.
#[derive(Debug, PartialEq)]
struct ConfinedLoadSource {
    esm: String,
    masters: Vec<String>,
    bsas: Vec<String>,
    textures_bsas: Vec<String>,
}

impl ConfinedLoadSource {
    fn resolve(
        esm: &str,
        masters: &[String],
        bsas: &[String],
        textures_bsas: &[String],
        roots: &[std::path::PathBuf],
    ) -> Result<Self, String> {
        let confine = |requested: &str| {
            confine_to_roots(requested, roots)
                .and_then(|path| path.to_str().map(str::to_owned))
                .ok_or_else(|| {
                    format!(
                        "'{requested}' is not a relative path to an existing file under the \
                         engine's startup --esm/--master/--bsa directories"
                    )
                })
        };
        let confine_all = |paths: &[String]| -> Result<Vec<String>, String> {
            paths.iter().map(|path| confine(path)).collect()
        };
        Ok(Self {
            esm: confine(esm)?,
            masters: confine_all(masters)?,
            bsas: confine_all(bsas)?,
            textures_bsas: confine_all(textures_bsas)?,
        })
    }

    fn borrow(&self) -> DebugLoadSource<'_> {
        DebugLoadSource {
            esm: &self.esm,
            masters: &self.masters,
            bsas: &self.bsas,
            textures_bsas: &self.textures_bsas,
        }
    }
}

/// Shared plugin + archive source for a debug cell load: the ESM, its master
/// chain, and the mesh / texture BSA lists used to synthesize a provider.
/// Grouped so both [`exec_load_interior`] and [`exec_load_exterior`] stay
/// under the argument-count limit.
struct DebugLoadSource<'a> {
    esm: &'a str,
    masters: &'a [String],
    bsas: &'a [String],
    textures_bsas: &'a [String],
}

/// Archive-set signature of the most recent debug `cell.load` request.
/// Owned (not borrowed) so it survives past the request that created it,
/// for comparison against the *next* request.
///
/// FNV-D1-02 / #2078: `NifImportRegistry` caches parsed NIF scenes keyed
/// only by lowercased model path — no field records which archive set
/// resolved that path, and (by design, for the normal single-launch CLI
/// path) nothing ever clears it mid-process. The debug `cell.load`
/// console command breaks that assumption: it can synthesize an
/// arbitrary `--bsa`/`--esm`/`--master` set per request against the same
/// `World`. Comparing this signature against the previous request lets
/// [`invalidate_nif_cache_on_archive_change`] wipe the registry exactly
/// when the archive set actually changed — leaving the common case
/// (repeated debug loads against the same archive set) fully cached.
#[derive(Clone, PartialEq, Eq)]
struct DebugLoadArchiveSet {
    esm: String,
    masters: Vec<String>,
    bsas: Vec<String>,
    textures_bsas: Vec<String>,
}

impl Resource for DebugLoadArchiveSet {}

impl DebugLoadArchiveSet {
    fn from_source(source: &DebugLoadSource) -> Self {
        Self {
            esm: source.esm.to_string(),
            masters: source.masters.to_vec(),
            bsas: source.bsas.to_vec(),
            textures_bsas: source.textures_bsas.to_vec(),
        }
    }
}

/// Clear [`cell_loader::NifImportRegistry`] when `source`'s archive set
/// differs from the previous debug load's — see [`DebugLoadArchiveSet`].
/// No-op (and no clear) on the very first debug load of a session or on
/// a repeat load against the same archive set, so the common case keeps
/// its cache warm.
fn invalidate_nif_cache_on_archive_change(world: &mut World, source: &DebugLoadSource) {
    let next = DebugLoadArchiveSet::from_source(source);
    let changed = world
        .try_resource::<DebugLoadArchiveSet>()
        .map(|prev| *prev != next)
        .unwrap_or(false); // first debug load this session — nothing to invalidate
    if changed {
        log::info!(
            "debug load: archive set changed from the previous debug load — clearing NifImportRegistry \
             (FNV-D1-02 / #2078, avoids stale cross-load model reuse)",
        );
        world
            .resource_mut::<cell_loader::NifImportRegistry>()
            .clear();
    }
    world.insert_resource(next);
}

/// Exterior grid target for a debug load: the center cell, stream radius, and
/// optional worldspace editor-id override.
struct DebugExteriorTarget<'a> {
    grid_x: i32,
    grid_y: i32,
    radius: u8,
    worldspace: Option<&'a str>,
}

fn exec_load_interior(
    world: &mut World,
    ctx: &mut VulkanContext,
    streaming: &mut Option<streaming::WorldStreamingState>,
    source: DebugLoadSource,
    cell: &str,
) {
    invalidate_nif_cache_on_archive_change(world, &source);
    let DebugLoadSource {
        esm,
        masters,
        bsas,
        textures_bsas,
    } = source;
    // #5056 sibling — a debug load replaces the session just like a save
    // load: drop cinematic/tether rows so the teardown despawns a live
    // convoy instead of retaining it as ghosts.
    cell_loader::purge_cinematic_retention_state(world);
    if streaming.is_some() {
        drain_streaming_state(world, ctx, streaming);
    }
    let synth_args = synth_provider_args(bsas, textures_bsas);
    let tex_provider = build_texture_provider(&synth_args);
    let mut mat_provider = build_material_provider(&synth_args);

    cell_loader::unload_current_interior(world, ctx);
    match cell_loader::load_cell_with_masters(
        masters,
        esm,
        cell,
        world,
        ctx,
        &tex_provider,
        Some(&mut mat_provider),
    ) {
        Ok(result) => {
            log::info!(
                "debug load interior cell '{}': spawned {} entities at ({:.1},{:.1},{:.1})",
                cell,
                result.entity_count,
                result.center.x,
                result.center.y,
                result.center.z,
            );
            // #1340 — apply the loaded interior's lighting, same as the
            // startup `--cell` and door-walk transition paths. Without it
            // the debug-loaded interior keeps the previous cell's
            // `CellLightingRes` (stale ambient/fog + leaked exterior sun).
            // Always called (not gated on `Some`) so a cell with no
            // `XCLL`/resolvable `LTMP` still gets the engine-default
            // interior fallback rather than a stale carry-over (FNV-D1-01).
            cell_loader::apply_interior_cell_lighting(
                world,
                result.lighting.as_ref(),
                result.show_sky,
                result.image_space,
            );
            // EX-16 item 1 (#2372) — same always-insert reasoning as
            // lighting above (see the comment there).
            world.insert_resource(result.region_ambient);
            ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES);
            // Update the LoadedPluginSet so a subsequent
            // `door.teleport` from inside the debug-loaded cell
            // dispatches against the right masters + esm.
            world.insert_resource(cell_loader::LoadedPluginSet {
                masters: masters.to_vec(),
                esm_path: esm.to_string(),
            });
        }
        Err(e) => {
            log::error!("debug load interior cell '{}' FAILED: {:#}", cell, e);
        }
    }
}

fn exec_load_exterior(
    world: &mut World,
    ctx: &mut VulkanContext,
    streaming: &mut Option<streaming::WorldStreamingState>,
    source: DebugLoadSource,
    target: DebugExteriorTarget,
) {
    invalidate_nif_cache_on_archive_change(world, &source);
    let DebugLoadSource {
        esm,
        masters,
        bsas,
        textures_bsas,
    } = source;
    let DebugExteriorTarget {
        grid_x,
        grid_y,
        radius,
        worldspace,
    } = target;
    // Radius cap is 1..=12 (matches the CLI `parse_exterior_radius` max) —
    // clamp the wire value here so a bogus `0` doesn't trip the assertion in
    // `build_exterior_world_context` and a runaway `200` doesn't try to stream
    // 40K cells.
    let clamped_radius = (radius as i32).clamp(1, 12);
    if clamped_radius != radius as i32 {
        log::warn!(
            "debug load exterior: radius {} clamped to {}",
            radius,
            clamped_radius
        );
    }

    // #5056 sibling — same pre-teardown purge as the interior debug load.
    cell_loader::purge_cinematic_retention_state(world);
    // Tear down anything currently loaded — interior cell, exterior
    // streaming state — same teardown sequence the cell-transition
    // orchestrator runs.
    cell_loader::unload_current_interior(world, ctx);
    if streaming.is_some() {
        drain_streaming_state(world, ctx, streaming);
    }

    let synth_args = synth_provider_args(bsas, textures_bsas);
    let tex_provider = build_texture_provider(&synth_args);
    let mat_provider = build_material_provider(&synth_args);

    let state = match crate::scene::begin_exterior_streaming(
        world,
        ctx,
        tex_provider,
        mat_provider,
        masters,
        esm,
        worldspace,
        (grid_x, grid_y),
        clamped_radius,
        crate::scene::ExteriorBootstrapMode::ForegroundFirst,
    ) {
        Ok((state, _cam_center)) => state,
        Err(e) => {
            log::error!(
                "debug load exterior '{}' ({},{}): build context FAILED: {:#}",
                esm,
                grid_x,
                grid_y,
                e,
            );
            return;
        }
    };
    *streaming = Some(state);
    ctx.signal_temporal_discontinuity(SVGF_TAA_STREAMING_RECOVERY_FRAMES);

    // Same `LoadedPluginSet` refresh as the interior path — a future
    // `door.teleport` from an exterior REFR needs the right
    // (masters, esm) tuple to land on a valid destination.
    world.insert_resource(cell_loader::LoadedPluginSet {
        masters: masters.to_vec(),
        esm_path: esm.to_string(),
    });

    log::info!(
        "debug load exterior '{}' ({},{}) radius={}: streaming initialised",
        esm,
        grid_x,
        grid_y,
        clamped_radius,
    );
}

/// Build a synthetic CLI-style args list from explicit BSA paths so
/// `build_texture_provider` / `build_material_provider` can be
/// reused without divergence. Each list expands to its respective
/// CLI flag (`--bsa` for mesh, `--textures-bsa` for textures). Empty
/// inputs produce an empty list — provider construction then yields
/// an empty provider, which is a valid no-op state.
fn synth_provider_args(bsas: &[String], textures_bsas: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(2 * (bsas.len() + textures_bsas.len()));
    for b in bsas {
        out.push("--bsa".to_string());
        out.push(b.clone());
    }
    for b in textures_bsas {
        out.push("--textures-bsa".to_string());
        out.push(b.clone());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cell_loader::NifImportRegistry;

    /// FNV-D1-02 / #2078 — two debug `cell.load` requests with different
    /// `--bsa` sets against the same `World` must not let the second
    /// request see the first request's cached NIF content: a model-path
    /// collision across the two archive sets would otherwise silently
    /// keep serving the first-loaded content.
    #[test]
    fn archive_set_change_clears_nif_registry() {
        let mut world = World::new();
        world.insert_resource(NifImportRegistry::new());

        // Seed the registry as if a prior debug load had already resolved
        // (or failed to resolve) this model path — either way, a cache
        // entry exists under it.
        let _ = world
            .resource_mut::<NifImportRegistry>()
            .insert("meshes\\armor\\test.nif".to_string(), None);
        assert_eq!(world.resource::<NifImportRegistry>().len(), 1);

        let no_masters: Vec<String> = Vec::new();
        let no_textures: Vec<String> = Vec::new();
        let vanilla_bsa = vec!["Vanilla.bsa".to_string()];
        let mod_bsa = vec!["Mod.bsa".to_string()];

        // First request establishes the baseline signature — no prior
        // signature to compare against, so nothing is cleared.
        invalidate_nif_cache_on_archive_change(
            &mut world,
            &DebugLoadSource {
                esm: "FalloutNV.esm",
                masters: &no_masters,
                bsas: &vanilla_bsa,
                textures_bsas: &no_textures,
            },
        );
        assert_eq!(
            world.resource::<NifImportRegistry>().len(),
            1,
            "first debug load must not clear an existing registry"
        );

        // Repeat request, same archive set — still no clear.
        invalidate_nif_cache_on_archive_change(
            &mut world,
            &DebugLoadSource {
                esm: "FalloutNV.esm",
                masters: &no_masters,
                bsas: &vanilla_bsa,
                textures_bsas: &no_textures,
            },
        );
        assert_eq!(
            world.resource::<NifImportRegistry>().len(),
            1,
            "an unchanged archive set must not clear the registry"
        );

        // Different `--bsa` — a mod's overriding archive. Must clear so a
        // subsequent lookup of `meshes\armor\test.nif` re-resolves against
        // the new archive set instead of reusing the first request's entry.
        invalidate_nif_cache_on_archive_change(
            &mut world,
            &DebugLoadSource {
                esm: "FalloutNV.esm",
                masters: &no_masters,
                bsas: &mod_bsa,
                textures_bsas: &no_textures,
            },
        );
        assert_eq!(
            world.resource::<NifImportRegistry>().len(),
            0,
            "a changed archive set must clear the registry"
        );
    }

    /// #5165 — a cell load's plugin and archive paths obey the same root
    /// rule as `LoadNif`: relative, no `..`, under a startup root.
    #[test]
    fn cell_load_paths_are_confined_to_startup_roots() {
        let data = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        for name in ["Game.esm", "Master.esm", "Meshes.bsa", "Textures.bsa"] {
            std::fs::write(data.path().join(name), b"x").unwrap();
        }
        std::fs::write(outside.path().join("secret.esm"), b"x").unwrap();
        let roots = vec![data.path().canonicalize().unwrap()];
        let names = |list: &[&str]| list.iter().map(|s| s.to_string()).collect::<Vec<_>>();

        let source = ConfinedLoadSource::resolve(
            "Game.esm",
            &names(&["Master.esm"]),
            &names(&["Meshes.bsa"]),
            &names(&["./Textures.bsa"]),
            &roots,
        )
        .expect("relative paths under a startup root load");
        assert_eq!(
            source.esm,
            roots[0].join("Game.esm").to_str().unwrap(),
            "the loader receives the canonical path"
        );
        assert_eq!(
            source.textures_bsas[0],
            roots[0].join("Textures.bsa").to_str().unwrap()
        );

        let absolute = outside.path().join("secret.esm");
        let escapes = [
            absolute.to_str().unwrap().to_string(),
            "../secret.esm".to_string(),
            "Missing.esm".to_string(),
        ];
        for esm in &escapes {
            assert!(
                ConfinedLoadSource::resolve(esm, &[], &[], &[], &roots).is_err(),
                "{esm} must be rejected"
            );
        }
        // One bad archive rejects the whole load.
        assert!(ConfinedLoadSource::resolve(
            "Game.esm",
            &[],
            &names(&["Meshes.bsa", absolute.to_str().unwrap()]),
            &[],
            &roots,
        )
        .is_err());
    }

    /// A symlink inside a root that points outside it does not escape.
    #[cfg(unix)]
    #[test]
    fn confinement_rejects_a_symlink_out_of_the_root() {
        let data = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret.esm"), b"x").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.esm"),
            data.path().join("link.esm"),
        )
        .unwrap();
        let roots = vec![data.path().canonicalize().unwrap()];
        assert_eq!(confine_to_roots("link.esm", &roots), None);
    }
}
