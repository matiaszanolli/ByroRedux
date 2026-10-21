//! #5757 — `EsmCellIndex::layered_material_swap_entries`, the flattening
//! of a REFR's `XLMS` list into the cell loader's swap entries.

use super::super::*;
use crate::esm::records::{LayeredMaterialSwapEntry, LayeredMaterialSwapRecord};

fn entry(key: &str, mtpt: Option<u32>, target: &str) -> LayeredMaterialSwapEntry {
    LayeredMaterialSwapEntry {
        key: key.into(),
        key_material_path_form: mtpt,
        override_material_path: target.into(),
    }
}

fn record(
    form_id: u32,
    entries: Vec<LayeredMaterialSwapEntry>,
    swaps_collection: Vec<u32>,
) -> LayeredMaterialSwapRecord {
    LayeredMaterialSwapRecord {
        form_id,
        editor_id: format!("Swap{form_id:X}"),
        entries,
        swaps_collection,
    }
}

fn pairs(index: &EsmCellIndex, forms: &[u32]) -> Vec<(String, String)> {
    index
        .layered_material_swap_entries(forms)
        .into_iter()
        .map(|e| {
            assert!(e.color_remap_index.is_none(), "LMSW carries no CNAM row");
            (e.source, e.target)
        })
        .collect()
}

fn index_with(records: Vec<LayeredMaterialSwapRecord>) -> EsmCellIndex {
    let mut index = EsmCellIndex::default();
    for r in records {
        index.layered_material_swaps.insert(r.form_id, r);
    }
    index
}

#[test]
fn inline_key_and_mtpt_sources_both_resolve() {
    let mut index = index_with(vec![record(
        0x10,
        vec![
            entry(r"a\inline.mat", None, r"a\inline_blue.mat"),
            entry("", Some(0x99), r"a\viaform_blue.mat"),
        ],
        vec![],
    )]);
    index
        .material_path_forms
        .insert(0x99, r"a\viaform.mat".into());
    assert_eq!(
        pairs(&index, &[0x10]),
        vec![
            (r"a\inline.mat".into(), r"a\inline_blue.mat".into()),
            (r"a\viaform.mat".into(), r"a\viaform_blue.mat".into()),
        ]
    );
}

/// An entry with neither a key nor a resolvable MTPT names no source
/// and is dropped; an unknown LMSW contributes nothing.
#[test]
fn sourceless_entries_and_unknown_forms_contribute_nothing() {
    let index = index_with(vec![record(
        0x10,
        vec![
            entry("", None, r"a\x.mat"),
            entry("", Some(0x404), r"a\y.mat"),
        ],
        vec![],
    )]);
    assert!(pairs(&index, &[0x10, 0xDEAD]).is_empty());
}

/// A collection contributes its members' entries (recursively, in
/// order) before its own, and the XLMS list keeps its order, so a later
/// listed swap overrides an earlier one in the later-wins consumer.
#[test]
fn collections_flatten_depth_first_in_authored_order() {
    let index = index_with(vec![
        record(0x1, vec![entry(r"m\a.mat", None, r"m\a1.mat")], vec![]),
        record(0x2, vec![entry(r"m\b.mat", None, r"m\b2.mat")], vec![0x1]),
        record(
            0x3,
            vec![entry(r"m\c.mat", None, r"m\c3.mat")],
            vec![0x2, 0x1],
        ),
        record(0x4, vec![entry(r"m\a.mat", None, r"m\a4.mat")], vec![]),
    ]);
    assert_eq!(
        pairs(&index, &[0x3, 0x4]),
        vec![
            (r"m\a.mat".into(), r"m\a1.mat".into()),
            (r"m\b.mat".into(), r"m\b2.mat".into()),
            (r"m\a.mat".into(), r"m\a1.mat".into()),
            (r"m\c.mat".into(), r"m\c3.mat".into()),
            (r"m\a.mat".into(), r"m\a4.mat".into()),
        ]
    );
}

/// A collection cycle (hostile or mis-authored data) is cut where it
/// closes instead of recursing without bound.
#[test]
fn a_collection_cycle_is_cut() {
    let index = index_with(vec![
        record(0x1, vec![entry(r"m\a.mat", None, r"m\a1.mat")], vec![0x2]),
        record(0x2, vec![entry(r"m\b.mat", None, r"m\b2.mat")], vec![0x1]),
    ]);
    assert_eq!(
        pairs(&index, &[0x1]),
        vec![
            (r"m\b.mat".into(), r"m\b2.mat".into()),
            (r"m\a.mat".into(), r"m\a1.mat".into()),
        ]
    );
}

/// `merge_from` carries both maps (its exhaustive destructuring makes a
/// dropped map a compile error; this pins the extend).
#[test]
fn merge_from_carries_layered_swaps_and_material_path_forms() {
    let mut base = EsmCellIndex::default();
    let mut plugin = index_with(vec![record(0x10, vec![], vec![])]);
    plugin.material_path_forms.insert(0x99, r"a\b.mat".into());
    base.merge_from(plugin);
    assert!(base.layered_material_swaps.contains_key(&0x10));
    assert_eq!(
        base.material_path_forms.get(&0x99).map(String::as_str),
        Some(r"a\b.mat")
    );
}
