//! LMC-REC-010 — the checked-in decode table (`data/decoded_subrecords.toml`) says what the parsers decode today.
//!
//! The table is measured, not written ([`byroredux_plugin::esm::decode_probe`]), so it goes stale when a decoder is added or
//! removed. Both directions matter, and not equally:
//!
//! - a sub-record the parsers now decode but the table lacks is kept as opaque bytes and reported as "not decoded": a false
//!   claim, but nothing is lost;
//! - a sub-record the table lists that no parser decodes any more is *dropped*: the one failure that loses a mod's data.
//!
//! Each test re-derives one game's section from its vanilla plugins and compares it with the table. Opt in with
//! `--ignored`; a game that is not installed skips (or fails under the strict game-data lane, like the other real-data tests).

use byroredux_plugin::esm::decode_probe::{derive_section, ProbeLimits};
use byroredux_plugin::esm::opaque::{decoded_table, encode_tag};
use byroredux_plugin::esm::reader::GameKind;
use byroredux_plugin::esm::test_paths;

fn check(section: &str, game: GameKind) {
    let Some(derived) = derive_section(section, &ProbeLimits::default()).expect("probe the section's plugins") else {
        test_paths::require_or_skip(&format!("the {section} plugins of decode_probe::TABLE_SOURCES"));
        return;
    };
    let table = decoded_table().section(game).unwrap_or_else(|| panic!("data/decoded_subrecords.toml has no [{section}] section"));
    let mut stale = Vec::new();
    let mut lost = Vec::new();
    for record_type in derived.keys().chain(table.keys()).collect::<std::collections::BTreeSet<_>>() {
        let now = derived.get(record_type).cloned().unwrap_or_default();
        let listed = table.get(record_type).cloned().unwrap_or_default();
        for tag in now.difference(&listed) {
            stale.push(format!("{} {}", encode_tag(record_type), encode_tag(tag)));
        }
        for tag in listed.difference(&now) {
            lost.push(format!("{} {}", encode_tag(record_type), encode_tag(tag)));
        }
    }
    let how = "regenerate the file: cargo run --release -p byroredux-plugin --example inspect -- decoded-subrecords > crates/plugin/data/decoded_subrecords.toml";
    assert!(lost.is_empty(), "[{section}] the table lists sub-records no parser decodes any more, so they would be dropped, not kept: {lost:?} — {how}");
    assert!(stale.is_empty(), "[{section}] the parsers now decode sub-records the table does not list: {stale:?} — {how}");
}

#[test]
#[ignore = "re-derives the table from Oblivion's plugins"]
fn the_oblivion_section_is_current() {
    check("oblivion", GameKind::Oblivion);
}

#[test]
#[ignore = "re-derives the table from Fallout 3 and New Vegas's plugins"]
fn the_fallout3nv_section_is_current() {
    check("fallout3nv", GameKind::Fallout3NV);
}

#[test]
#[ignore = "re-derives the table from Skyrim SE's plugins"]
fn the_skyrim_section_is_current() {
    check("skyrim", GameKind::Skyrim);
}

#[test]
#[ignore = "re-derives the table from Fallout 4's plugins"]
fn the_fallout4_section_is_current() {
    check("fallout4", GameKind::Fallout4);
}

#[test]
#[ignore = "re-derives the table from Fallout 76's plugins"]
fn the_fallout76_section_is_current() {
    check("fallout76", GameKind::Fallout76);
}

#[test]
#[ignore = "re-derives the table from Starfield's plugins (about a minute)"]
fn the_starfield_section_is_current() {
    check("starfield", GameKind::Starfield);
}
