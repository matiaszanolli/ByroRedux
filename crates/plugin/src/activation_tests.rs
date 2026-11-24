use super::*;

fn plugin(name: &str, masters: &[&str]) -> ProfilePlugin {
    ProfilePlugin::new(name, masters.iter().copied())
}

fn names(profile: &[ProfilePlugin], order: &[usize]) -> Vec<String> {
    order.iter().map(|&p| profile[p].name.clone()).collect()
}

/// A profile already in masters-first order activates as given, with nothing to report.
#[test]
fn an_ordered_profile_activates_unchanged() {
    let profile = [
        plugin("FalloutNV.esm", &[]),
        plugin("DeadMoney.esm", &["FalloutNV.esm"]),
        plugin("HonestHearts.esm", &["FalloutNV.esm"]),
        plugin("MyMod.esp", &["FalloutNV.esm", "DeadMoney.esm"]),
    ];
    let activation = activate(&profile);
    assert_eq!(activation.order, vec![0, 1, 2, 3]);
    assert!(activation.is_unchanged());
    assert!(activation.report(&profile).is_empty());
}

/// LMC-ID-003 — a master listed after its dependent moves to just ahead of it, and only it moves: the independent
/// plugins keep their profile order, and so do two masters hoisted together.
#[test]
fn a_late_master_is_hoisted_ahead_of_its_first_dependent_and_nothing_else_moves() {
    let profile = [
        plugin("Skyrim.esm", &[]),
        plugin("Patch.esp", &["Skyrim.esm", "ModB.esp", "ModA.esp"]),
        plugin("Other.esp", &["Skyrim.esm"]),
        plugin("ModA.esp", &["Skyrim.esm"]),
        plugin("ModB.esp", &["Skyrim.esm"]),
    ];
    let activation = activate(&profile);
    assert_eq!(names(&profile, &activation.order), ["Skyrim.esm", "ModA.esp", "ModB.esp", "Patch.esp", "Other.esp"]);
    assert_eq!(activation.hoists, vec![Hoist { master: 3, ahead_of: 1 }, Hoist { master: 4, ahead_of: 1 }]);
    assert!(activation.disabled.is_empty());
    let report = activation.report(&profile);
    assert_eq!(report.len(), 2);
    assert!(report[0].contains("ModA.esp now loads ahead of Patch.esp"), "{report:?}");
}

/// A master's own late master comes along: the whole chain lands ahead of the first plugin that needs it, masters first.
#[test]
fn a_hoisted_master_brings_its_own_masters() {
    let profile = [
        plugin("Fallout3.esm", &[]),
        plugin("Top.esp", &["Fallout3.esm", "Mid.esm"]),
        plugin("Base.esm", &["Fallout3.esm"]),
        plugin("Mid.esm", &["Base.esm"]),
    ];
    let activation = activate(&profile);
    assert_eq!(names(&profile, &activation.order), ["Fallout3.esm", "Base.esm", "Mid.esm", "Top.esp"]);
    assert_eq!(activation.hoists, vec![Hoist { master: 2, ahead_of: 1 }, Hoist { master: 3, ahead_of: 1 }]);
}

/// LMC-ID-011 — a missing master disables its plugin and every transitive dependent, each reported with the chain back
/// to the missing master, and the rest of the profile still activates.
#[test]
fn a_missing_master_disables_its_plugin_and_every_dependent_and_the_rest_activates() {
    let profile = [
        plugin("FalloutNV.esm", &[]),
        plugin("NeedsDLC.esp", &["FalloutNV.esm", "LonesomeRoad.esm"]),
        plugin("Fine.esp", &["FalloutNV.esm"]),
        plugin("PatchForNeedsDLC.esp", &["FalloutNV.esm", "NeedsDLC.esp"]),
        plugin("PatchOfPatch.esp", &["PatchForNeedsDLC.esp"]),
    ];
    let activation = activate(&profile);
    assert_eq!(names(&profile, &activation.order), ["FalloutNV.esm", "Fine.esp"]);
    assert_eq!(
        activation.disabled,
        vec![
            Disabled { plugin: 1, cause: DisableCause::MissingMaster { master: "LonesomeRoad.esm".into() } },
            Disabled { plugin: 3, cause: DisableCause::DisabledMaster { master: 1 } },
            Disabled { plugin: 4, cause: DisableCause::DisabledMaster { master: 3 } },
        ]
    );
    let report = activation.report(&profile);
    assert!(report[0].contains("NeedsDLC.esp — its master LonesomeRoad.esm is not in the profile"), "{report:?}");
    assert!(
        report[2].contains("PatchOfPatch.esp — its master is disabled: PatchForNeedsDLC.esp <- NeedsDLC.esp (missing master LonesomeRoad.esm)"),
        "{report:?}"
    );
}

/// LMC-ID-001 / -009 — masters match the profile by case-folded basename, whatever case or path either side uses.
#[test]
fn masters_match_by_case_folded_basename() {
    let profile = [
        plugin("/games/Skyrim/Data/Skyrim.esm", &[]),
        plugin("Data\\Update.esm", &["SKYRIM.ESM"]),
        plugin("dawnguard.esm", &["skyrim.esm", "update.ESM"]),
    ];
    let activation = activate(&profile);
    assert_eq!(activation.order, vec![0, 1, 2]);
    assert!(activation.is_unchanged());
}

/// LMC-ID-010 — the second entry with an identity the profile already has is left out and reported; its dependents
/// bind to the first.
#[test]
fn a_duplicate_identity_is_reported_and_the_first_kept() {
    let profile = [plugin("Oblivion.esm", &[]), plugin("Mod.esp", &["Oblivion.esm"]), plugin("data/OBLIVION.ESM", &[])];
    let activation = activate(&profile);
    assert_eq!(activation.order, vec![0, 1]);
    assert_eq!(
        activation.disabled,
        vec![Disabled { plugin: 2, cause: DisableCause::Duplicate { first: 0, content: DuplicateContent::Unknown } }]
    );
    assert!(activation.report(&profile)[0].contains("already lists it as Oblivion.esm"));
}

/// LMC-ID-010 — a duplicate is compared with the plugin it duplicates: one file listed twice, or a copy with the same
/// bytes, is a repeat; different bytes under one identity are reported as an identity collision. The earlier entry
/// loads either way.
#[test]
fn a_duplicate_with_different_content_is_an_identity_collision() {
    let tmp = tempfile::tempdir().unwrap();
    let write = |relative: &str, bytes: &[u8]| {
        let path = tmp.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    };
    let base = write("Data/Skyrim.esm", b"base");
    let patch = write("a/Patch.esp", b"patch v1");
    let copy = write("b/patch.ESP", b"patch v1");
    let other = write("c/Patch.esp", b"patch v2");
    let longer = write("d/Patch.esp", b"patch v2, longer");
    let profile = [
        plugin(&base, &[]),
        plugin(&patch, &["Skyrim.esm"]),
        plugin(&patch, &["Skyrim.esm"]),
        plugin(&copy, &["Skyrim.esm"]),
        plugin(&other, &["Skyrim.esm"]),
        plugin(&longer, &["Skyrim.esm"]),
    ];
    let mut activation = activate(&profile);
    activation.compare_duplicates(&profile, |a, b| same_file_content(Path::new(a), Path::new(b)).ok());
    assert_eq!(activation.order, vec![0, 1]);
    let content: Vec<_> = activation
        .disabled
        .iter()
        .map(|entry| match entry.cause {
            DisableCause::Duplicate { first: 1, content } => content,
            ref cause => panic!("{cause:?}"),
        })
        .collect();
    use DuplicateContent::{Different, Same};
    assert_eq!(content, [Same, Same, Different, Different]);
    let report = activation.report(&profile);
    assert!(report[0].ends_with("already lists it as Patch.esp, with the same content"), "{report:?}");
    assert!(report[2].contains(&format!("identity collision (LMC-ID-010): {other} has the identity of {patch}")), "{report:?}");

    let absent = [plugin("A.esp", &[]), plugin("x/a.esp", &[])];
    let mut unreadable = activate(&absent);
    unreadable.compare_duplicates(&absent, |a, b| same_file_content(Path::new(a), Path::new(b)).ok());
    assert!(matches!(unreadable.disabled[0].cause, DisableCause::Duplicate { content: DuplicateContent::Unknown, .. }));
}

/// A master cycle (no shipped content has one) disables its members and their dependents instead of looping.
#[test]
fn a_master_cycle_is_disabled_not_looped_on() {
    let profile = [
        plugin("Base.esm", &[]),
        plugin("A.esp", &["Base.esm", "B.esp"]),
        plugin("B.esp", &["A.esp"]),
        plugin("UsesA.esp", &["A.esp"]),
        plugin("Self.esp", &["Self.esp"]),
        plugin("Fine.esp", &["Base.esm"]),
    ];
    let activation = activate(&profile);
    assert_eq!(names(&profile, &activation.order), ["Base.esm", "Fine.esp"]);
    let causes: Vec<_> = activation.disabled.iter().map(|d| (d.plugin, d.cause.clone())).collect();
    assert_eq!(
        causes,
        vec![
            (1, DisableCause::MasterCycle { cycle: vec![1, 2, 1] }),
            (2, DisableCause::MasterCycle { cycle: vec![2, 1, 2] }),
            (3, DisableCause::DisabledMaster { master: 1 }),
            (4, DisableCause::MasterCycle { cycle: vec![4, 4] }),
        ]
    );
    assert!(activation.report(&profile)[0].contains("cycle: A.esp -> B.esp -> A.esp"));
}

/// An unreadable plugin is disabled with its dependents, and named with the error.
#[test]
fn an_unreadable_plugin_is_disabled_with_its_dependents() {
    let profile = [
        plugin("Skyrim.esm", &[]),
        ProfilePlugin::unreadable("Broken.esm", "does not start with a TES4 record"),
        plugin("UsesBroken.esp", &["Skyrim.esm", "Broken.esm"]),
    ];
    let activation = activate(&profile);
    assert_eq!(activation.order, vec![0]);
    let report = activation.report(&profile);
    assert!(report[0].contains("Broken.esm — its header could not be read: does not start with a TES4 record"), "{report:?}");
    assert!(report[1].contains("Broken.esm (unreadable)"), "{report:?}");
}

/// The activation is a function of the profile: shuffling which masters a plugin lists, or their case, changes nothing,
/// and every activated plugin follows all of its masters.
#[test]
fn every_activated_plugin_follows_its_masters_and_the_result_is_deterministic() {
    let profile = [
        plugin("Z.esp", &["M.esm", "A.esm"]),
        plugin("M.esm", &["A.esm"]),
        plugin("Q.esp", &[]),
        plugin("A.esm", &[]),
        plugin("Y.esp", &["a.esm", "z.esp"]),
    ];
    let activation = activate(&profile);
    let reordered = [
        plugin("Z.esp", &["a.ESM", "m.esm"]),
        plugin("M.esm", &["A.esm"]),
        plugin("Q.esp", &[]),
        plugin("A.esm", &[]),
        plugin("Y.esp", &["Z.esp", "A.esm"]),
    ];
    assert_eq!(activate(&reordered), activation);
    assert_eq!(names(&profile, &activation.order), ["A.esm", "M.esm", "Z.esp", "Q.esp", "Y.esp"]);
    for (index, &position) in activation.order.iter().enumerate() {
        for master in profile[position].masters.as_ref().unwrap() {
            let master = activation.order.iter().position(|&p| plugin_key(&profile[p].name) == plugin_key(master)).unwrap();
            assert!(master < index, "{} loads before its master", profile[position].name);
        }
    }
}

/// `read_plugin_header` reads the `TES4` record of a file and nothing past it; a file that is not a plugin becomes an
/// unreadable entry rather than an error.
#[test]
fn the_header_reader_reads_the_tes4_record_alone() {
    // A Tes5+ TES4 record: HEDR, then two MAST/DATA pairs, then a GRUP the reader must not need.
    fn sub(tag: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = tag.to_vec();
        out.extend_from_slice(&(data.len() as u16).to_le_bytes());
        out.extend_from_slice(data);
        out
    }
    let mut body = sub(b"HEDR", &[0x9A, 0x99, 0xD9, 0x3F, 0, 0, 0, 0, 0, 0, 0, 0]);
    body.extend(sub(b"MAST", b"Skyrim.esm\0"));
    body.extend(sub(b"DATA", &[0; 8]));
    body.extend(sub(b"MAST", b"Update.esm\0"));
    body.extend(sub(b"DATA", &[0; 8]));
    let mut file = b"TES4".to_vec();
    file.extend_from_slice(&(body.len() as u32).to_le_bytes());
    file.extend_from_slice(&[0; 16]);
    file.extend_from_slice(&body);
    file.extend_from_slice(b"GRUP this is not read");

    let dir = std::env::temp_dir().join(format!("byro-activation-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("Dawnguard.esm");
    std::fs::write(&path, &file).unwrap();
    let not_a_plugin = dir.join("notes.txt");
    std::fs::write(&not_a_plugin, b"hello").unwrap();

    let read = read_profile_plugin(path.to_str().unwrap());
    assert_eq!(read.masters, Some(vec!["Skyrim.esm".to_owned(), "Update.esm".to_owned()]));
    let bad = read_profile_plugin(not_a_plugin.to_str().unwrap());
    assert!(bad.masters.is_none());
    assert!(bad.unreadable.as_deref().unwrap().contains("does not start with a TES4 record"), "{bad:?}");
    let missing = read_profile_plugin(dir.join("absent.esp").to_str().unwrap());
    assert!(missing.unreadable.as_deref().unwrap().contains("open"), "{missing:?}");
    std::fs::remove_dir_all(&dir).ok();
}
