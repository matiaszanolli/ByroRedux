//! LMC-PERF-003 — a pool of archives answers in one index probe what scanning every archive answers, whatever the pool's
//! size, and opening a pool costs the archives' entries, not their pairs.
//!
//! The scan is the reference: [`naive_locate`] is what `extract_first` did, the last-listed archive that holds the path.
//! The index is a hint, so the tests also break it on purpose and check the answer survives.

use super::super::*;
use super::write_ba2;
use std::time::Instant;

/// A loose archive tier holding `files` (archive-style relative paths) under a fresh directory.
fn loose(files: &[&str]) -> (tempfile::TempDir, Archive) {
    let dir = tempfile::tempdir().unwrap();
    for file in files {
        let path = dir.path().join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, file.as_bytes()).unwrap();
    }
    let archive = Archive::open_loose(dir.path()).expect("loose assets present");
    (dir, archive)
}

/// What the pool did before the index: the last archive that holds `path`.
fn naive_locate(archives: &[Archive], path: &str) -> Option<usize> {
    archives.iter().rposition(|archive| archive.contains(path))
}

/// The pool holds the same answer the scan gives, for every path an archive holds and for spellings of it a caller could
/// use: other case, forward slashes, a leading separator; and for paths nothing holds.
#[test]
fn a_pool_locates_what_a_scan_of_its_archives_locates() {
    let (_a, base) = loose(&["textures/base/a.dds", "textures/base/b.dds", "meshes/m.nif", "textures/both.dds"]);
    let (_b, middle) = loose(&["textures/base/b.dds", "textures/mid/c.dds", "textures/both.dds"]);
    let (_c, top) = loose(&["textures/both.dds", "meshes/m.nif", "meshes/new.nif"]);
    let archives = vec![base, middle, top];
    let pool = ArchiveSet::from(archives.clone());

    let mut paths: Vec<String> = Vec::new();
    for archive in &archives {
        for key in archive.all_paths() {
            paths.push(key.to_owned());
            paths.push(key.to_uppercase());
            paths.push(key.replace('\\', "/"));
            paths.push(format!("\\{key}"));
            paths.push(format!("/{}", key.replace('\\', "/")));
        }
    }
    paths.extend(["textures\\absent.dds", "", "\\", "meshes\\", "textures\\base"].map(str::to_owned));
    for path in &paths {
        assert_eq!(pool.locate(path), naive_locate(&archives, path), "{path:?}");
    }
    assert_eq!(pool.locate("textures\\both.dds"), Some(2), "the last-listed archive wins");
    assert_eq!(pool.locate("textures\\base\\a.dds"), Some(0));
    assert_eq!(pool.locate("textures\\base\\b.dds"), Some(1));
    assert!(pool.contains("TEXTURES/MID/C.DDS") && !pool.contains("textures\\mid\\d.dds"));
    assert_eq!(pool.winner("meshes\\m.nif").map(|archive| archive.path()), Some(archives[2].path()));
}

/// An empty pool answers nothing and the answer follows a push: the index is built again for the archive added on top.
#[test]
fn pushing_an_archive_changes_the_answer() {
    let (_a, base) = loose(&["textures/x.dds"]);
    let (_b, over) = loose(&["textures/x.dds", "textures/y.dds"]);
    let mut pool = ArchiveSet::new();
    assert_eq!(pool.locate("textures\\x.dds"), None);
    pool.push(base);
    assert_eq!(pool.locate("textures\\x.dds"), Some(0));
    assert_eq!(pool.locate("textures\\y.dds"), None);
    pool.push(over);
    assert_eq!(pool.locate("textures\\x.dds"), Some(1), "the archive pushed last wins");
    assert_eq!(pool.locate("textures\\y.dds"), Some(1));
}

/// A hint that names the wrong archive, as a hash collision would, costs a scan and not a wrong answer: neither a
/// different archive that does not hold the path nor a position past the pool is served.
#[test]
fn a_wrong_hint_costs_a_scan_not_a_wrong_answer() {
    let (_a, base) = loose(&["textures/x.dds", "textures/only_base.dds"]);
    let (_b, other) = loose(&["textures/unrelated.dds"]);
    let pool = ArchiveSet::from(vec![base, other]);
    assert_eq!(pool.locate("textures\\only_base.dds"), Some(0));
    let wrong = pool.with_poisoned_index("textures\\only_base.dds", 1);
    assert_eq!(wrong.locate("textures\\only_base.dds"), Some(0), "archive 1 does not hold it: scan finds archive 0");
    let beyond = pool.with_poisoned_index("textures\\only_base.dds", 99);
    assert_eq!(beyond.locate("textures\\only_base.dds"), Some(0), "a position past the pool is not trusted either");
    assert_eq!(wrong.extract_first("textures\\only_base.dds").as_deref(), Some(&b"textures/only_base.dds"[..]));
}

/// A present entry that fails to read falls through to the next archive down and is named once, as before (#4658): the
/// pool's index names the corrupt override, the read passes over it.
#[test]
fn a_corrupt_override_falls_through_in_a_pool() {
    let dir = tempfile::tempdir().unwrap();
    let entry = r"textures\override.dds";
    let (base_path, over_path) = (dir.path().join("base.ba2"), dir.path().join("over.ba2"));
    write_ba2(&base_path, &[(entry, b"vanilla bytes", false)]);
    write_ba2(&over_path, &[(entry, b"not a zlib stream", true)]);
    let base = Archive::open(&base_path.to_string_lossy()).expect("open base");
    let over = Archive::open(&over_path.to_string_lossy()).expect("open override");
    let pool = ArchiveSet::from(vec![base, over]);

    assert_eq!(pool.locate(entry), Some(1), "the corrupt override is the winner the index names");
    assert!(pool.extract_first(entry).as_deref() == Some(&b"vanilla bytes"[..]), "and the read passes over it");
    assert!(pool.extract_first(r"textures\absent.dds").is_none());
    assert_eq!(pool.declared_size(entry), Some(17), "the winner's own index entry reports its size, whatever its bytes hold");
}

/// Provider rebuilds repeat their pools (#5288): the same archives in the same order read one index, and a different
/// order does not.
#[test]
fn a_rebuilt_pool_reads_the_memoised_index() {
    let (_a, one) = loose(&["textures/one.dds"]);
    let (_b, two) = loose(&["textures/two.dds"]);
    let first = ArchiveSet::from(vec![one.clone(), two.clone()]);
    assert!(first.contains("textures\\one.dds"));
    let rebuilt = ArchiveSet::from(vec![one.clone(), two.clone()]);
    assert!(first.shares_index_with(&rebuilt), "the same ordered archives share their index");
    assert!(first.shares_index_with(&first.clone()), "a clone shares it");
    let reordered = ArchiveSet::from(vec![two, one]);
    assert!(!first.shares_index_with(&reordered), "another order is another pool");
    assert_eq!(reordered.locate("textures\\one.dds"), Some(1));
}

/// The shadow count of each archive opened onto a pool is the count the pairwise probe gave, whether the earlier
/// archives were counted one by one or only caught up when a later count asked.
#[test]
fn the_running_shadow_count_equals_the_pairwise_probe() {
    let (_a, a) = loose(&["meshes/m/1.nif", "meshes/m/2.nif", "meshes/m/3.nif"]);
    let (_b, b) = loose(&["meshes/m/2.nif", "meshes/m/4.nif"]);
    let (_c, c) = loose(&["meshes/m/1.nif", "meshes/m/4.nif", "meshes/m/5.nif"]);
    let (_d, d) = loose(&["meshes/m/9.nif"]);
    let archives = [a, b, c, d];
    let pairwise = |index: usize| -> usize {
        archives[index].all_paths().into_iter().filter(|path| archives[..index].iter().any(|earlier| earlier.contains(path))).count()
    };
    // Every archive counted as it is pushed.
    let mut pool = ArchiveSet::new();
    for (index, archive) in archives.iter().enumerate() {
        assert_eq!(pool.count_shadowed(archive), pairwise(index), "archive {index}, counted as pushed");
        pool.push(archive.clone());
    }
    // Pushes the memo would have answered, so the running index is not touched, then a count that needs it.
    let mut lazy = ArchiveSet::new();
    for archive in &archives[..3] {
        lazy.push(archive.clone());
    }
    assert_eq!(lazy.count_shadowed(&archives[3]), pairwise(3));
    lazy.push(archives[3].clone());
    assert_eq!(lazy.locate("meshes\\m\\1.nif"), Some(2));
}

/// A stored key is already normalized, so hashing it as stored is hashing its lookup form: the index is built from the
/// archives' own keys without normalizing each again. Loose, which normalizes the keys it builds, is the case at hand.
#[test]
fn archives_store_their_keys_normalized() {
    let (_a, archive) = loose(&["Textures/Mixed/Case.DDS", "meshes/a/b.nif"]);
    for key in archive.all_paths() {
        assert_eq!(byroredux_bsa::normalize_path(key), key);
    }
}

/// Real data (ignored): across every archive of each installed game's main pools, in the order the engine lists them, the
/// index agrees with the scan for every entry and for spellings of it. Reports the pool sizes.
#[test]
#[ignore = "needs game archives on disk"]
fn the_index_agrees_with_the_scan_on_real_archives() {
    use byroredux_plugin::esm::test_paths::{fnv_data_dir, fo4_data_dir, skyrim_se_data_dir};
    let pools: [(&str, std::path::PathBuf, &[&str]); 3] = [
        ("FNV", fnv_data_dir(), &["Fallout - Meshes.bsa", "Fallout - Textures.bsa", "Fallout - Textures2.bsa", "DeadMoney - Main.bsa"]),
        ("Skyrim SE", skyrim_se_data_dir(), &["Skyrim - Meshes0.bsa", "Skyrim - Meshes1.bsa", "Skyrim - Textures0.bsa", "Dawnguard.bsa", "Update.bsa"]),
        ("FO4", fo4_data_dir(), &["Fallout4 - Meshes.ba2", "Fallout4 - MeshesExtra.ba2", "DLCRobot - Main.ba2", "DLCCoast - Main.ba2"]),
    ];
    let mut checked_any = false;
    for (game, dir, names) in pools {
        let archives: Vec<Archive> = names.iter().filter_map(|name| Archive::open(&dir.join(name).to_string_lossy()).ok()).collect();
        if archives.len() < 2 {
            continue;
        }
        checked_any = true;
        let pool = ArchiveSet::from(archives.clone());
        let mut checked = 0usize;
        for archive in &archives {
            for key in archive.all_paths() {
                for path in [key.to_owned(), key.to_uppercase(), key.replace('\\', "/")] {
                    assert_eq!(pool.locate(&path), naive_locate(&archives, &path), "{game}: {path:?}");
                    checked += 1;
                }
            }
        }
        for absent in ["textures\\no\\such\\file.dds", "\\meshes\\x.nif", "meshes/"] {
            assert_eq!(pool.locate(absent), naive_locate(&archives, absent), "{game}: {absent:?}");
        }
        eprintln!("{game}: {} archives, {checked} lookups agree with the scan", archives.len());
    }
    if !checked_any {
        byroredux_plugin::esm::test_paths::require_or_skip("two archives of one installed game");
    }
}

/// `count` BA2s of `entries` each in `dir`: archive 0 holds the base assets, every later one its own, and a tenth of each
/// later one's entries are base assets again, so each archive shadows something.
fn write_pool(dir: &std::path::Path, count: usize, entries: usize) -> Vec<std::path::PathBuf> {
    (0..count)
        .map(|k| {
            let names: Vec<String> = (0..entries)
                .map(|j| if k == 0 || j % 10 == 0 { format!("textures\\base\\f{j:05}.dds") } else { format!("textures\\a{k:04}\\f{j:05}.dds") })
                .collect();
            let entries: Vec<(&str, &[u8], bool)> = names.iter().map(|name| (name.as_str(), &b"0123456789abcdef"[..], false)).collect();
            let path = dir.join(format!("p{k:04}.ba2"));
            write_ba2(&path, &entries);
            path
        })
        .collect()
}

/// Time a lookup of an asset only the first archive holds, and opening the pool with its shadow counts, as the archive
/// count grows. A lookup that probes every archive grows with the count; the index does not. Opening was quadratic.
#[test]
#[ignore = "a timing measurement: opens pools of up to 400 archives of 5,000 entries"]
fn lookup_and_open_cost_do_not_grow_with_the_archive_count() {
    const ENTRIES: usize = 5000;
    let dir = tempfile::tempdir().unwrap();
    eprintln!("  archives   µs/lookup (extract)   open seconds");
    let mut rows = Vec::new();
    for count in [10usize, 50, 100, 200, 400] {
        let paths = write_pool(dir.path(), count, ENTRIES);
        let opened = Instant::now();
        let mut pool = ArchiveSet::new();
        for path in &paths {
            let archive = Archive::open(&path.to_string_lossy()).expect("open");
            let _ = pool.count_shadowed(&archive);
            pool.push(archive);
        }
        let open_seconds = opened.elapsed().as_secs_f64();
        let _ = pool.locate("textures\\base\\f00000.dds"); // build the index outside the timed loop
        let probes = 20_000usize;
        let started = Instant::now();
        let mut hits = 0;
        for i in 0..probes {
            let path = format!("Textures/Base/F{:05}.DDS", (i * 7) % ENTRIES);
            hits += usize::from(pool.extract_first(&path).is_some());
        }
        let micros = started.elapsed().as_secs_f64() * 1e6 / probes as f64;
        assert_eq!(hits, probes, "every base asset is found");
        eprintln!("  {count:8} {micros:21.2} {open_seconds:14.3}");
        rows.push((count, micros, open_seconds));
        for path in &paths {
            let _ = std::fs::remove_file(path);
        }
    }
    let (first, last) = (rows[0], rows[rows.len() - 1]);
    assert!(last.1 <= first.1 * 3.0, "a lookup cost {:.2} µs at {} archives and {:.2} µs at {}: it grows with the archive count", first.1, first.0, last.1, last.0);
    let (mid, _) = (rows[1], ());
    // Linear opening grows 8-fold from 50 to 400 archives; the pairwise probe grew 80-fold.
    assert!(last.2 <= mid.2 * 16.0, "opening took {:.3} s at {} archives and {:.3} s at {}: more than linear", mid.2, mid.0, last.2, last.0);
}
