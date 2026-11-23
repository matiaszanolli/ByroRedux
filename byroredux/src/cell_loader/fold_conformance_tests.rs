//! Fold conformance dump (PRD legacy-mod-compatibility LMC-REC-002, -004, -007, -011). Folds a real load order with
//! the production fold and writes what it decided, every form keyed by (load-order position of the plugin that owns
//! its ID space, object id), for `scripts/lmc-fold-oracle.py` to diff against its own raw reading of the plugins. The
//! gate is `docs/smoke-tests/lmc-fold-conformance.sh`; run by hand with
//!
//! ```text
//! BYRO_FOLD_PLUGINS=<file: one plugin path per line> BYRO_FOLD_EXPECT=<oracle expectations> BYRO_FOLD_OUT=<dir> \
//!     <byroredux test binary> --ignored --exact cell_loader::load_order::fold_conformance_tests::dump_fold
//! ```
//!
//! Writes into `BYRO_FOLD_OUT`: `fold.json` (plugins, slots, fold time, the tombstones each record category still
//! holds), `cells.jsonl` (each merged cell's placements), `navmeshes.txt`, `record_types.txt` (every form the fold
//! keeps and its record type) and `winners.txt` (the fold's winner for each form the oracle asks about).

use super::parse_record_indexes_in_load_order;
use byroredux_plugin::esm::cell::CellData;
use byroredux_plugin::esm::reader::GlobalSlot;
use byroredux_plugin::esm::records::EsmIndex;
use std::collections::{HashMap, HashSet};
use std::io::Write as _;

/// (position, object id) ↔ global FormID, from the order's slots.
struct Keys {
    regular: HashMap<u8, usize>,
    light: HashMap<u16, usize>,
    medium: HashMap<u16, usize>,
    slots: Vec<GlobalSlot>,
}

impl Keys {
    fn key(&self, id: u32) -> (i64, u32) {
        let found = match (id >> 24) as u8 {
            0xFE => self.light.get(&(((id >> 12) & 0xFFF) as u16)).map(|&p| (p as i64, id & 0xFFF)),
            0xFD => self.medium.get(&(((id >> 16) & 0xFF) as u16)).map(|&p| (p as i64, id & 0xFFFF)),
            top => self.regular.get(&top).map(|&p| (p as i64, id & 0xFF_FFFF)),
        };
        found.unwrap_or((-1, id))
    }

    fn global(&self, position: usize, object: u32) -> Option<u32> {
        Some(self.slots.get(position)?.compose(object))
    }
}

#[test]
#[ignore = "fold conformance dump: needs BYRO_FOLD_PLUGINS, BYRO_FOLD_EXPECT and BYRO_FOLD_OUT (lmc-fold-conformance.sh)"]
fn dump_fold() {
    let env = |name: &str| std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    let list = std::fs::read_to_string(env("BYRO_FOLD_PLUGINS")).expect("BYRO_FOLD_PLUGINS");
    let expect = std::fs::read_to_string(env("BYRO_FOLD_EXPECT")).expect("BYRO_FOLD_EXPECT");
    let out = std::path::PathBuf::from(env("BYRO_FOLD_OUT"));
    let paths: Vec<&str> = list.lines().map(str::trim).filter(|line| !line.is_empty()).collect();

    let started = std::time::Instant::now();
    let (mut merged, order) = parse_record_indexes_in_load_order(&paths).expect("fold");
    let fold_s = started.elapsed().as_secs_f64();

    let mut keys = Keys { regular: HashMap::new(), light: HashMap::new(), medium: HashMap::new(), slots: order.slots().to_vec() };
    for (position, slot) in order.slots().iter().enumerate() {
        match *slot {
            GlobalSlot::Regular(b) => keys.regular.insert(b, position),
            GlobalSlot::Light(sub) => keys.light.insert(sub, position),
            GlobalSlot::Medium(sub) => keys.medium.insert(sub, position),
        };
    }
    let create = |name: &str| std::io::BufWriter::new(std::fs::File::create(out.join(name)).expect(name));

    // Placements per merged cell.
    let mut cells = create("cells.jsonl");
    let mut dump = |kind: &str, place: &str, cell: &CellData| {
        let (cp, co) = keys.key(cell.form_id);
        let refs: Vec<String> = cell
            .references
            .iter()
            .map(|r| {
                let (p, o) = keys.key(r.form_id);
                let (bp, bo) = keys.key(r.base_form_id);
                format!("[{p},{o},{bp},{bo},{:.2},{:.2},{:.2}]", r.position[0], r.position[1], r.position[2])
            })
            .collect();
        writeln!(cells, "{{\"kind\":\"{kind}\",\"where\":{place:?},\"cell\":[{cp},{co}],\"refs\":[{}]}}", refs.join(","))
            .unwrap();
    };
    for (name, cell) in &merged.cells.cells {
        dump("int", name, cell);
    }
    for (world, grid) in &merged.cells.exterior_cells {
        for ((x, y), cell) in grid {
            dump("ext", &format!("{world} {x},{y}"), cell);
        }
    }
    for (world, cell) in &merged.cells.worldspace_persistent_cells {
        dump("wp", world, cell);
    }
    cells.flush().unwrap();

    // Navmeshes: #1272 drains every cell's NAVMs into the flat map, each naming its parent CELL (typed form) or its
    // worldspace and grid (packed form).
    let mut navmeshes = create("navmeshes.txt");
    for navm in merged.navmeshes.values() {
        let (p, o) = keys.key(navm.form_id);
        let (cp, co) = navm.cell_form.map_or((-2, 0), |cell| keys.key(cell));
        writeln!(navmeshes, "{p} {o} {cp} {co}").unwrap();
    }
    navmeshes.flush().unwrap();

    // Every form the fold keeps, with its record type.
    let mut types = create("record_types.txt");
    for (&id, record_type) in &merged.record_types {
        let (p, o) = keys.key(id);
        writeln!(types, "{p} {o} {}", String::from_utf8_lossy(record_type)).unwrap();
    }
    types.flush().unwrap();

    // The oracle's questions: `W pos obj` asks the fold's winner of a form more than one plugin carried, or one a
    // plugin injected; `D pos obj` names a form whose last carrier deletes it.
    let mut winners = create("winners.txt");
    let mut deleted = HashSet::new();
    for line in expect.lines() {
        let mut parts = line.split_whitespace();
        let (Some(kind), Some(p), Some(o)) = (parts.next(), parts.next(), parts.next()) else { continue };
        let (Ok(p), Ok(o)) = (p.parse::<usize>(), u32::from_str_radix(o, 16)) else { continue };
        let Some(id) = keys.global(p, o) else { continue };
        match kind {
            "W" => {
                let winner = order.provenance().winner(id).map_or(p as i64, |w| w as i64);
                writeln!(winners, "{p} {o:x} {winner}").unwrap();
            }
            "D" => {
                deleted.insert(id);
            }
            _ => {}
        }
    }
    winners.flush().unwrap();

    // A tombstoned form must be gone from every record category: prune the merged index with the oracle's set and
    // count what each category still held.
    let before: Vec<(&str, usize)> = EsmIndex::categories().iter().map(|(name, count, _, _)| (*name, count(&merged))).collect();
    for (_, _, _, prune) in EsmIndex::categories() {
        prune(&mut merged, &deleted);
    }
    let survivors: Vec<String> = EsmIndex::categories()
        .iter()
        .zip(&before)
        .filter_map(|((_, count, _, _), (name, was))| {
            let left = was - count(&merged);
            (left > 0).then(|| format!("{name:?}:{left}"))
        })
        .collect();

    let names: Vec<String> = merged.plugin_files.iter().map(|name| format!("{name:?}")).collect();
    let slots: Vec<String> = order
        .slots()
        .iter()
        .map(|slot| match slot {
            GlobalSlot::Regular(b) => format!("\"R{b}\""),
            GlobalSlot::Light(sub) => format!("\"L{sub}\""),
            GlobalSlot::Medium(sub) => format!("\"M{sub}\""),
        })
        .collect();
    let mut fold = create("fold.json");
    writeln!(
        fold,
        "{{\"plugins\":[{}],\"slots\":[{}],\"fold_s\":{fold_s:.3},\"tombstones_kept\":{{{}}}}}",
        names.join(","),
        slots.join(","),
        survivors.join(",")
    )
    .unwrap();
    fold.flush().unwrap();
    eprintln!("fold conformance: {} plugins folded in {fold_s:.2} s -> {}", paths.len(), out.display());
}
