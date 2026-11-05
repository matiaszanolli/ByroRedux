//! #6691 — which FO3 / FNV models carry a dismemberment-cap sub-mesh, and which
//! of those can a placed reference reach through the cell path?
//!
//! Both spawn paths hide caps (`is_hidden_dismemberment_cap`) because there is no
//! dismemberment system yet. This records why the cell path needs that gate at all.
//! It walks the shipped data the way the cell loader does: every NIF in the game's
//! mesh archives through the same import, then every placed reference through the
//! same load-order parse and the same `record_index.actor` gate that sends actors
//! to the NPC path.
//!
//! Before placed armor took its world model, FNV reached caps through 52 ARMO bases
//! (125 references, 42 models) and FO3 through 57 (171 references, 40 models), all
//! because `StaticObject.model_path` was the biped `MODL`. Now no shipped reference
//! reaches one, which is what these tests assert. The gate stays for content this
//! census cannot see: a mod's placed armor with no `MOD2` draws its worn mesh.
//!
//!   BYROREDUX_FNV_DATA=<Data> cargo test -p byroredux --bin byroredux \
//!       fnv_dismemberment_cap_census -- --ignored --nocapture
//!   BYROREDUX_FO3_DATA=<Data> cargo test -p byroredux --bin byroredux \
//!       fo3_dismemberment_cap_census -- --ignored --nocapture

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;

use rayon::prelude::*;

use crate::cell_loader::nif_import_registry::canonical_model_path_key;

/// One NIF that holds at least one cap sub-mesh.
struct CapModel {
    caps: Vec<String>,
    total_meshes: usize,
}

/// What the archive scan found, keyed by canonical model path. A later archive
/// overrides an earlier one, as it does at load time.
#[derive(Default)]
struct ArchiveScan {
    archives: Vec<String>,
    nifs: usize,
    parse_failures: usize,
    paths: BTreeSet<String>,
    cap_models: BTreeMap<String, CapModel>,
}

fn scan_archive(path: &Path, scan: &mut ArchiveScan) {
    let archive = byroredux_bsa::BsaArchive::open(path)
        .unwrap_or_else(|e| panic!("open {}: {e}", path.display()));
    let nifs: Vec<&str> = archive
        .list_files()
        .into_iter()
        .filter(|file| file.ends_with(".nif"))
        .collect();
    let results: Vec<(String, Option<Option<CapModel>>)> = nifs
        .par_iter()
        .map(|file| {
            let key = canonical_model_path_key(file);
            let Some(scene) = archive
                .extract(file)
                .ok()
                .and_then(|bytes| byroredux_nif::parse_nif(&bytes).ok())
            else {
                return (key, None);
            };
            let mut pool = byroredux_core::string::StringPool::new();
            // The import `parse_and_import_nif` runs for a placed model.
            let (meshes, _) = byroredux_nif::import::import_nif_with_collision_and_resolver(
                &scene, &mut pool, None,
            );
            let caps: Vec<String> = meshes
                .iter()
                .filter(|mesh| mesh.is_dismemberment_cap())
                .map(|mesh| mesh.name.as_deref().unwrap_or("<unnamed>").to_string())
                .collect();
            let found = (!caps.is_empty()).then_some(CapModel {
                caps,
                total_meshes: meshes.len(),
            });
            (key, Some(found))
        })
        .collect();

    scan.archives.push(format!(
        "{} ({} NIFs)",
        path.file_name().unwrap_or_default().to_string_lossy(),
        results.len()
    ));
    scan.nifs += results.len();
    for (key, outcome) in results {
        scan.paths.insert(key.clone());
        match outcome {
            None => scan.parse_failures += 1,
            Some(found) => {
                scan.cap_models.remove(&key);
                if let Some(found) = found {
                    scan.cap_models.insert(key, found);
                }
            }
        }
    }
}

/// One placed base form whose model carries a cap and which takes the cell path.
struct ReachableCap {
    record_type: String,
    editor_id: String,
    form_id: u32,
    refs: usize,
    model: String,
}

struct Census {
    reachable: Vec<ReachableCap>,
    cap_models: usize,
}

fn run_census(game: &str, data_dir: &Path, archives: &[&str], plugins: &[&str]) -> Census {
    let mut scan = ArchiveScan::default();
    for archive in archives {
        scan_archive(&data_dir.join(archive), &mut scan);
    }

    let plugin_paths: Vec<String> = plugins
        .iter()
        .map(|plugin| data_dir.join(plugin).to_string_lossy().into_owned())
        .collect();
    let plugin_refs: Vec<&str> = plugin_paths.iter().map(String::as_str).collect();
    let (index, _load_order) = crate::cell_loader::parse_record_indexes_in_load_order(&plugin_refs)
        .unwrap_or_else(|e| panic!("{game}: parse load order: {e:#}"));

    let cells = &index.cells;
    let all_cells = cells
        .cells
        .values()
        .chain(
            cells
                .exterior_cells
                .values()
                .flat_map(|world| world.values()),
        )
        .chain(cells.worldspace_persistent_cells.values());

    let (mut placed, mut actor_refs, mut no_model_refs, mut cell_path_refs) = (0, 0, 0, 0);
    let mut refs_by_base: HashMap<u32, usize> = HashMap::new();
    let mut placed_armor_by_cell: HashMap<&str, usize> = HashMap::new();
    for cell in all_cells {
        for placement in &cell.references {
            placed += 1;
            let base = placement.base_form_id;
            // The gate `advance_reference_load` applies before the static path.
            if index.actor(base).is_some() {
                actor_refs += 1;
            } else if cells
                .statics
                .get(&base)
                .is_some_and(|stat| !stat.model_path.is_empty())
            {
                cell_path_refs += 1;
                *refs_by_base.entry(base).or_default() += 1;
                if cells.statics[&base].record_type == byroredux_plugin::record::RecordType::ARMO {
                    *placed_armor_by_cell
                        .entry(cell.editor_id.as_str())
                        .or_default() += 1;
                }
            } else {
                no_model_refs += 1;
            }
        }
    }

    let mut models: BTreeSet<String> = BTreeSet::new();
    let mut by_type: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    let mut reachable = Vec::new();
    for (&base, &refs) in &refs_by_base {
        let stat = &cells.statics[&base];
        let model = canonical_model_path_key(&stat.model_path);
        let row = by_type
            .entry(format!("{:?}", stat.record_type))
            .or_default();
        row.0 += 1;
        row.1 += refs;
        if scan.cap_models.contains_key(&model) {
            reachable.push(ReachableCap {
                record_type: format!("{:?}", stat.record_type),
                editor_id: stat.editor_id.clone(),
                form_id: base,
                refs,
                model: model.clone(),
            });
        }
        models.insert(model);
    }
    reachable.sort_by(|a, b| {
        (&a.record_type, &a.model, a.form_id).cmp(&(&b.record_type, &b.model, b.form_id))
    });

    let in_archive = models
        .iter()
        .filter(|model| scan.paths.contains(*model))
        .count();
    let cap_submeshes: usize = scan.cap_models.values().map(|m| m.caps.len()).sum();
    println!("=== {game}");
    println!("plugins: {}", plugins.join(", "));
    println!("archives: {}", scan.archives.join(", "));
    println!(
        "archive NIFs: {} ({} failed to parse); cap-bearing NIFs: {}; cap sub-meshes: {}",
        scan.nifs,
        scan.parse_failures,
        scan.cap_models.len(),
        cap_submeshes
    );
    println!(
        "placed references: {placed}; actor path: {actor_refs}; no model: {no_model_refs}; \
         cell path: {cell_path_refs}"
    );
    println!(
        "cell-path bases: {}; distinct models: {} ({in_archive} in the scanned archives)",
        refs_by_base.len(),
        models.len()
    );
    println!("cell-path bases by record type (bases, refs): {by_type:?}");
    let mut armor_cells: Vec<(&str, usize)> = placed_armor_by_cell.into_iter().collect();
    armor_cells.sort_by_key(|(name, count)| (std::cmp::Reverse(*count), *name));
    armor_cells.truncate(8);
    println!("cells with the most placed ARMO: {armor_cells:?}");
    let reachable_models: BTreeSet<&str> = reachable.iter().map(|r| r.model.as_str()).collect();
    println!(
        "REACHABLE caps: {} bases, {} refs, {} models",
        reachable.len(),
        reachable.iter().map(|r| r.refs).sum::<usize>(),
        reachable_models.len()
    );
    for row in &reachable {
        let model = &scan.cap_models[&row.model];
        println!(
            "  {} {:08X} {:?} refs={} {} caps={:?} of {} meshes",
            row.record_type,
            row.form_id,
            row.editor_id,
            row.refs,
            row.model,
            model.caps,
            model.total_meshes
        );
    }

    Census {
        reachable,
        cap_models: scan.cap_models.len(),
    }
}

#[test]
#[ignore = "needs Fallout New Vegas game data on disk"]
fn fnv_dismemberment_cap_census() {
    let census = run_census(
        "Fallout New Vegas",
        &byroredux_plugin::esm::test_paths::fnv_data_dir(),
        &[
            "Fallout - Meshes.bsa",
            "Update.bsa",
            "DeadMoney - Main.bsa",
            "HonestHearts - Main.bsa",
            "OldWorldBlues - Main.bsa",
            "LonesomeRoad - Main.bsa",
            "GunRunnersArsenal - Main.bsa",
            "ClassicPack - Main.bsa",
            "MercenaryPack - Main.bsa",
            "TribalPack - Main.bsa",
            "CaravanPack - Main.bsa",
        ],
        &[
            "FalloutNV.esm",
            "DeadMoney.esm",
            "HonestHearts.esm",
            "OldWorldBlues.esm",
            "LonesomeRoad.esm",
            "GunRunnersArsenal.esm",
            "ClassicPack.esm",
            "MercenaryPack.esm",
            "TribalPack.esm",
            "CaravanPack.esm",
        ],
    );
    assert_no_placed_reference_reaches_a_cap(&census);
}

#[test]
#[ignore = "needs Fallout 3 game data on disk"]
fn fo3_dismemberment_cap_census() {
    let census = run_census(
        "Fallout 3",
        &byroredux_plugin::esm::test_paths::fo3_data_dir(),
        &[
            "Fallout - Meshes.bsa",
            "Anchorage - Main.bsa",
            "ThePitt - Main.bsa",
            "BrokenSteel - Main.bsa",
            "PointLookout - Main.bsa",
            "Zeta - Main.bsa",
        ],
        &[
            "Fallout3.esm",
            "Anchorage.esm",
            "ThePitt.esm",
            "BrokenSteel.esm",
            "PointLookout.esm",
            "Zeta.esm",
        ],
    );
    assert_no_placed_reference_reaches_a_cap(&census);
}

/// The detector works, and no shipped reference draws a model that has a cap. A row
/// here is a new way for caps to reach the cell path: the gate still hides them, but
/// the reason recorded above has changed.
fn assert_no_placed_reference_reaches_a_cap(census: &Census) {
    assert!(
        census.cap_models > 0,
        "the detector found no caps, so it proves nothing"
    );
    assert!(
        census.reachable.is_empty(),
        "{} placed bases reach a dismemberment cap, the first: {} {:08X} {:?} {}",
        census.reachable.len(),
        census.reachable[0].record_type,
        census.reachable[0].form_id,
        census.reachable[0].editor_id,
        census.reachable[0].model,
    );
}
