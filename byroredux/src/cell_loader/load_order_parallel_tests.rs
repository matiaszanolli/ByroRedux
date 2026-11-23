//! #3813 — the parallel per-plugin record walk must produce exactly the
//! merged `EsmIndex` of the inline schedule it replaced.
//!
//! `EsmIndex` has no `PartialEq` (hundreds of record types, and float fields
//! where `NaN != NaN`), and its `Debug` text cannot be compared verbatim:
//! every `HashMap` / `HashSet` prints in its own randomly seeded order.
//! [`digest`] hashes that text with the entries of every `{…}` block made
//! order-independent — map, set and struct entries alike (a struct entry
//! carries its field name, so sorting loses nothing) — while `[…]` and `(…)`
//! keep their order. Every field reachable through `Debug` is covered,
//! including ones added later, and only one index is held at a time.

use super::tests::{build_strings_file, build_tes4_with_masters, build_weap, wrap_group};
use super::{parse_record_indexes_in_load_order_with_archive, ArchiveStringSource, PluginWalk};
use byroredux_plugin::esm;
use esm::structural_digest::structural_digest as digest;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

const DLC_COUNT: u32 = 11;
const PLAIN_DLC: u32 = 5;
const MASTER_WEAP: u32 = 0x0000_1000;

fn dlc_own_weap(dlc: u32) -> u32 {
    (dlc << 24) | (0x2000 + dlc)
}

fn write_chain_plugin(
    dir: &Path,
    stem: &str,
    masters: &[&str],
    weapons: &[(u32, u32, String)],
    localized: bool,
    archive: &mut HashMap<String, Vec<u8>>,
) -> PathBuf {
    let mut records = Vec::new();
    for (form_id, lstring, text) in weapons {
        let full = if localized {
            lstring.to_le_bytes().to_vec()
        } else {
            format!("{text}\0").into_bytes()
        };
        records.extend(build_weap(*form_id, &full));
    }
    let mut bytes = build_tes4_with_masters(if localized { 0x80 } else { 0 }, masters);
    bytes.extend(wrap_group(b"WEAP", &records));
    let path = dir.join(format!("{stem}.esm"));
    std::fs::write(&path, bytes).unwrap();
    if localized {
        let entries: Vec<(u32, &str)> = weapons
            .iter()
            .map(|(_, lstring, text)| (*lstring, text.as_str()))
            .collect();
        archive.insert(
            format!(r"strings\{stem}_english.STRINGS"),
            build_strings_file(&entries),
        );
    }
    path
}

/// A 12-plugin chain: a localized master, then DLCs that each override the
/// master's WEAP and add one of their own, every localized plugin resolving
/// the same lstring ids to its own text through archive-sourced tables, and
/// one non-localized DLC in the middle.
#[test]
fn parallel_walk_matches_the_inline_walk_on_a_localized_master_chain() {
    let dir = tempfile::tempdir().unwrap();
    let mut archive = HashMap::new();
    let mut paths = vec![write_chain_plugin(
        dir.path(),
        "Base",
        &[],
        &[(MASTER_WEAP, 1, "Base Blade".into())],
        true,
        &mut archive,
    )];
    for dlc in 1..=DLC_COUNT {
        paths.push(write_chain_plugin(
            dir.path(),
            &format!("Dlc{dlc:02}"),
            &["Base.esm"],
            &[
                (MASTER_WEAP, 1, format!("Dlc{dlc:02} Override")),
                (0x0100_2000 + dlc, 2, format!("Dlc{dlc:02} Own")),
            ],
            dlc != PLAIN_DLC,
            &mut archive,
        ));
    }
    let paths: Vec<&str> = paths.iter().map(|p| p.to_str().unwrap()).collect();
    let walk = |schedule| {
        let (index, order) = parse_record_indexes_in_load_order_with_archive(
            &paths,
            |_, relative| archive.get(relative).cloned(),
            schedule,
            byroredux_plugin::esm::records::list_merge::MergePolicy::default(),
        )
        .unwrap();
        let digest = digest(&(&index, &order[..]));
        (index, digest)
    };

    // Pin the inline reference itself, so two equally wrong walks can't agree.
    let (inline, expected) = walk(PluginWalk::Inline);
    assert_eq!(
        inline.items[&MASTER_WEAP].common.full_name,
        format!("Dlc{DLC_COUNT:02} Override"),
        "the last plugin in load order wins the override"
    );
    for dlc in 1..=DLC_COUNT {
        assert_eq!(
            inline.items[&dlc_own_weap(dlc)].common.full_name,
            format!("Dlc{dlc:02} Own"),
            "each plugin resolves its lstrings against its own tables"
        );
    }

    // #6426 — the merged index carries the load order's file names
    // (lower-cased, in order). `CharacterRulesProfile::level_cap_raise`
    // keys on them (#6227); without this line New Vegas silently falls
    // back to a cap of 30 with every test still green.
    let expected_files: Vec<String> = paths
        .iter()
        .map(|p| {
            std::path::Path::new(p)
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .to_lowercase()
        })
        .collect();
    assert_eq!(inline.plugin_files, expected_files, "plugin_files = the load order");

    // Repeat to vary which pool thread walks which plugin.
    for run in 0..8 {
        let (parallel, actual) = walk(PluginWalk::Parallel);
        assert_eq!(
            actual,
            expected,
            "run {run}: the parallel walk's index differs from the inline walk's \
             (items: {:?})",
            parallel
                .items
                .iter()
                .map(|(id, item)| (format!("{id:#010x}"), item.common.full_name.as_str()))
                .collect::<std::collections::BTreeMap<_, _>>()
        );
    }
}

/// The acceptance oracle across real multi-master load orders: localized
/// Skyrim SE and Fallout 4 (archive-sourced string tables), and the
/// non-localized Fallout 3 / New Vegas DLC chains. Walks each order inline,
/// then in parallel, one index in memory at a time, and prints both wall
/// times.
///
/// ```text
/// cargo test --release -p byroredux --bin byroredux \
///     parallel_walk_matches_the_inline_walk_on_real_load_orders -- --ignored --nocapture
/// ```
#[test]
#[ignore = "needs installed game data; walks each vanilla load order twice"]
fn parallel_walk_matches_the_inline_walk_on_real_load_orders() {
    use esm::test_paths;
    let load_orders: [(&str, PathBuf, &[&str]); 4] = [
        (
            "Fallout New Vegas",
            test_paths::fnv_data_dir(),
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
        ),
        (
            "Fallout 3",
            test_paths::fo3_data_dir(),
            &[
                "Fallout3.esm",
                "Anchorage.esm",
                "ThePitt.esm",
                "BrokenSteel.esm",
                "PointLookout.esm",
                "Zeta.esm",
            ],
        ),
        (
            "Skyrim SE",
            test_paths::skyrim_se_data_dir(),
            &[
                "Skyrim.esm",
                "Update.esm",
                "Dawnguard.esm",
                "HearthFires.esm",
                "Dragonborn.esm",
            ],
        ),
        (
            "Fallout 4",
            test_paths::fo4_data_dir(),
            &[
                "Fallout4.esm",
                "DLCRobot.esm",
                "DLCworkshop01.esm",
                "DLCCoast.esm",
                "DLCworkshop02.esm",
                "DLCworkshop03.esm",
                "DLCNukaWorld.esm",
                "DLCUltraHighResolution.esm",
            ],
        ),
    ];

    let mut compared = 0;
    for (game, data, plugins) in &load_orders {
        let paths: Vec<PathBuf> = plugins.iter().map(|p| data.join(p)).collect();
        if let Some(missing) = paths.iter().find(|p| !p.is_file()) {
            eprintln!("[{game}] skipping: {} is missing", missing.display());
            continue;
        }
        let paths: Vec<&str> = paths.iter().map(|p| p.to_str().unwrap()).collect();
        // Warm the page cache so neither schedule pays the cold read.
        for path in &paths {
            std::fs::read(path).unwrap();
        }
        let walk = |schedule| {
            let mut archive_source = ArchiveStringSource::default();
            let started = Instant::now();
            let (index, order) = parse_record_indexes_in_load_order_with_archive(
                &paths,
                |plugin, relative| archive_source.read(plugin, relative),
                schedule,
                byroredux_plugin::esm::records::list_merge::MergePolicy::default(),
            )
            .unwrap();
            let secs = started.elapsed().as_secs_f64();
            (digest(&(&index, &order[..])), index.total(), secs)
        };

        let (inline, records, inline_secs) = walk(PluginWalk::Inline);
        let (parallel, _, parallel_secs) = walk(PluginWalk::Parallel);
        eprintln!(
            "[{game}] {} plugins, {records} records: inline {inline_secs:.2} s, \
             parallel {parallel_secs:.2} s",
            paths.len(),
        );
        if parallel != inline {
            let (again, _, _) = walk(PluginWalk::Inline);
            panic!(
                "[{game}] parallel walk digest {parallel:#018x} != inline {inline:#018x}; \
                 a second inline walk gave {again:#018x}{}",
                if again == inline {
                    ""
                } else {
                    " — the inline walk is itself nondeterministic"
                }
            );
        }
        compared += 1;
    }
    if std::env::var("BYROREDUX_REQUIRE_GAME_DATA").is_ok_and(|v| v != "0") {
        assert!(
            compared > 0,
            "BYROREDUX_REQUIRE_GAME_DATA is set, but no load order had all its plugins"
        );
    }
}
