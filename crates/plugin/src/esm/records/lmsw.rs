//! LMSW (Layered Material Swap) and MTPT (Material Path) — Starfield's
//! replacement for FO4's MSWP material swaps (#5757).
//!
//! Starfield ships no `MSWP` GRUP. A REFR selects swaps through `XLMS`, a
//! list of LMSW FormIDs. Both records are an `EDID` plus a BGS reflection
//! blob in `REFL` (the same container WTHS uses, decoded by
//! [`parse_bgs_reflection_with_remap`]); no vanilla record of either type
//! carries an `RFDP`/`RDIF` diff. The decoded root classes, read off
//! `Starfield.esm` with `dump_wths_refl --type LMSW|MTPT`:
//!
//! ```text
//! BGSLayeredMaterialSwap
//!   Keywords        : List<Ref>                        // not modeled
//!   Entries         : List<BGSLayeredMaterialSwap::Entry>
//!   SwapsCollection : List<Ref>                        // nested LMSWs
//! BGSLayeredMaterialSwap::Entry
//!   Key                  : String     // source .mat path, inline …
//!   KeyMaterialPathForm  : Ref        // … or an MTPT naming it (0 = none)
//!   OverrideMaterialPath : String     // the replacement .mat path
//!   Properties           : BSBind::Snapshot           // not modeled
//! BGSMaterialPathForm (MTPT)
//!   Material : String
//! ```
//!
//! Paths are bare — relative to `Data\Materials\`, like FO4's `BNAM` —
//! and the consumer canonicalizes them against the NIF's
//! `Materials\…` spelling. An entry's `Properties` snapshot (bound
//! material-property overrides) and the record keywords have no
//! consumer and are not decoded into the record.

use super::common::read_zstring;
use super::reflection::parse_bgs_reflection_with_remap;
use super::weather_settings_instance::{ReflectionObject, ReflectionValue};
use crate::esm::reader::{FormIdRemap, SubRecord};

/// One `BGSLayeredMaterialSwap::Entry`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayeredMaterialSwapEntry {
    /// `Key` — the source material path, inline. Empty when the entry
    /// names its source through [`Self::key_material_path_form`].
    pub key: String,
    /// `KeyMaterialPathForm` — an MTPT whose `Material` is the source
    /// path. Load-order remapped; `None` when authored as 0.
    pub key_material_path_form: Option<u32>,
    /// `OverrideMaterialPath` — the replacement material path.
    pub override_material_path: String,
}

/// A parsed LMSW record. Stored on
/// [`crate::esm::cell::EsmCellIndex::layered_material_swaps`].
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LayeredMaterialSwapRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// The record's own swap entries, in authored order.
    pub entries: Vec<LayeredMaterialSwapEntry>,
    /// `SwapsCollection` — further LMSW FormIDs this record bundles
    /// (`Cydonia_Pipe_GreywithOffWhite01_Collection_01` is two of them
    /// and no entries of its own). Load-order remapped.
    pub swaps_collection: Vec<u32>,
}

/// Parse an LMSW record. A record without a decodable `REFL` root keeps
/// its editor ID and no swaps.
pub fn parse_lmsw(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<FormIdRemap>,
) -> LayeredMaterialSwapRecord {
    let mut record = LayeredMaterialSwapRecord {
        form_id,
        ..LayeredMaterialSwapRecord::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),
            b"REFL" => {
                let Some(root) = parse_bgs_reflection_with_remap(&sub.data, remap).root_instance
                else {
                    continue;
                };
                if root.class != "BGSLayeredMaterialSwap" {
                    continue;
                }
                record.entries = list_items(&root, "Entries")
                    .filter_map(ReflectionValue::as_object)
                    .map(parse_entry)
                    .collect();
                record.swaps_collection = list_items(&root, "SwapsCollection")
                    .filter_map(ReflectionValue::as_form_id)
                    .filter(|&id| id != 0)
                    .collect();
            }
            _ => {}
        }
    }
    record
}

fn parse_entry(entry: &ReflectionObject) -> LayeredMaterialSwapEntry {
    let string = |name| {
        entry
            .field(name)
            .and_then(ReflectionValue::as_string)
            .unwrap_or_default()
            .to_string()
    };
    LayeredMaterialSwapEntry {
        key: string("Key"),
        key_material_path_form: entry
            .field("KeyMaterialPathForm")
            .and_then(ReflectionValue::as_form_id)
            .filter(|&id| id != 0),
        override_material_path: string("OverrideMaterialPath"),
    }
}

fn list_items<'a>(
    object: &'a ReflectionObject,
    name: &str,
) -> impl Iterator<Item = &'a ReflectionValue> {
    let items: &[ReflectionValue] = match object.field(name) {
        Some(ReflectionValue::List(items)) => items,
        _ => &[],
    };
    items.iter()
}

/// Parse an MTPT record into its `Material` path. `None` when the
/// record carries no decodable `BGSMaterialPathForm` root or an empty
/// path.
pub fn parse_mtpt(subs: &[SubRecord], remap: &Option<FormIdRemap>) -> Option<String> {
    let blob = subs.iter().find(|s| s.sub_type == *b"REFL")?;
    let root = parse_bgs_reflection_with_remap(&blob.data, remap).root_instance?;
    if root.class != "BGSMaterialPathForm" {
        return None;
    }
    root.field("Material")
        .and_then(ReflectionValue::as_string)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

    // `REFL` payloads lifted verbatim from vanilla `Starfield.esm` — the
    // reflection grammar is only grounded against real data (xEdit ships
    // no reader for it), so the fixtures are the real bytes, not a
    // hand-built approximation.

    /// `IndustrialKit_Swap_BlueA_IndIntMetalTrimGrey03` (0x1A0953): one
    /// entry whose source is an MTPT form (2,301 Cydonia REFRs).
    const LMSW_MTPT_KEYED: &str = concat!(
        "4245544808000000040000000b000000535452549f0000004247534c6179657265644d6174657269616c53776170004b",
        "6579776f72647300456e7472696573005377617073436f6c6c656374696f6e004247534c6179657265644d6174657269",
        "616c537761703a3a456e747279004b6579004b65794d6174657269616c50617468466f726d004f766572726964654d61",
        "74657269616c5061746800425342696e643a3a536e617073686f740050726f7065727469657300545950450400000003",
        "000000434c4153300000000000000000000000000003001700000003ffffff380110002000000003ffffff4801100028",
        "00000003ffffff58011000434c4153180000008300000000000000000001002000000003ffffff08009001434c41533c",
        "0000003800000000000000000004005600000002ffffff000008005a00000005ffffff080008006e00000002ffffff10",
        "0008009400000083000000180098014f424a5404000000000000004c4953541800000005ffffff020000000dffffffd2",
        "9c1f000dffffff54bf32004c4953545400000038000000010000000100000dffffff37a51f003f004172636869746563",
        "747572655c496e647573747269616c4b69745c496e746572696f72735c496e64496e744d6574616c5472696d426c7565",
        "30332e6d6174004c4953540800000001ffffff000000004c4953540800000001ffffff00000000",
    );

    /// `indintmetaltilegrey01_To_cydonia_paintedmetal02_offwhite01_Swap`
    /// (0x249791): one entry with an inline `Key`.
    const LMSW_INLINE_KEY: &str = concat!(
        "4245544808000000040000000b000000535452549f0000004247534c6179657265644d6174657269616c53776170004b",
        "6579776f72647300456e7472696573005377617073436f6c6c656374696f6e004247534c6179657265644d6174657269",
        "616c537761703a3a456e747279004b6579004b65794d6174657269616c50617468466f726d004f766572726964654d61",
        "74657269616c5061746800425342696e643a3a536e617073686f740050726f7065727469657300545950450400000003",
        "000000434c4153300000000000000000000000000003001700000003ffffff380110002000000003ffffff4801100028",
        "00000003ffffff58011000434c4153180000008300000000000000000001002000000003ffffff08009001434c41533c",
        "0000003800000000000000000004005600000002ffffff000008005a00000005ffffff080008006e00000002ffffff10",
        "0008009400000083000000180098014f424a5404000000000000004c4953541000000005ffffff010000000dffffff54",
        "bf32004c4953549300000038000000010000003f004172636869746563747572655c496e647573747269616c4b69745c",
        "496e746572696f72735c496e64496e744d6574616c54696c654772657930312e6d6174000dffffff0000000040004172",
        "636869746563747572655c436974795c4379646f6e69615c4379646f6e69615f5061696e7465646d6574616c30325f4f",
        "6666576869746530312e6d6174004c4953540800000001ffffff000000004c4953540800000001ffffff00000000",
    );

    /// `Cydonia_Pipe_GreywithOffWhite01_Collection_01` (0x149FA2): no
    /// entries, two `SwapsCollection` members.
    const LMSW_COLLECTION: &str = concat!(
        "4245544808000000040000000800000053545254380000004247534c6179657265644d6174657269616c53776170004b",
        "6579776f72647300456e7472696573005377617073436f6c6c656374696f6e00545950450400000001000000434c4153",
        "300000000000000000000000000003001700000003ffffff380110002000000003ffffff480110002800000003ffffff",
        "580110004f424a5404000000000000004c4953541000000005ffffff010000000dffffff54bf32004c49535408000000",
        "01ffffff000000004c4953541800000005ffffff020000000dffffffbe9614000dffffffa7bd3400",
    );

    /// MTPT `IndIntMetalTrimGrey03` (0x1FA537).
    const MTPT_TRIM_GREY03: &str = concat!(
        "42455448080000000400000005000000535452541d0000004247534d6174657269616c50617468466f726d004d617465",
        "7269616c00545950450400000001000000434c4153180000000000000000000000000001001400000002ffffff180108",
        "004f424a5445000000000000003f004172636869746563747572655c496e647573747269616c4b69745c496e74657269",
        "6f72735c496e64496e744d6574616c5472696d4772657930332e6d617400",
    );

    fn bytes(hex: &str) -> Vec<u8> {
        (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect()
    }

    fn lmsw(form_id: u32, editor_id: &str, refl: &str) -> LayeredMaterialSwapRecord {
        let mut edid = editor_id.as_bytes().to_vec();
        edid.push(0);
        parse_lmsw(
            form_id,
            &[sub(b"EDID", edid), sub(b"REFL", bytes(refl))],
            &None,
        )
    }

    #[test]
    fn mtpt_keyed_entry_decodes_its_source_form_and_override() {
        let rec = lmsw(
            0x1A_0953,
            "IndustrialKit_Swap_BlueA_IndIntMetalTrimGrey03",
            LMSW_MTPT_KEYED,
        );
        assert_eq!(
            rec.editor_id,
            "IndustrialKit_Swap_BlueA_IndIntMetalTrimGrey03"
        );
        assert_eq!(
            rec.entries,
            vec![LayeredMaterialSwapEntry {
                key: String::new(),
                key_material_path_form: Some(0x1F_A537),
                override_material_path:
                    r"Architecture\IndustrialKit\Interiors\IndIntMetalTrimBlue03.mat".into(),
            }]
        );
        assert!(rec.swaps_collection.is_empty());
    }

    #[test]
    fn inline_key_entry_decodes_its_source_path() {
        let rec = lmsw(
            0x24_9791,
            "indintmetaltilegrey01_To_cydonia_paintedmetal02_offwhite01_Swap",
            LMSW_INLINE_KEY,
        );
        assert_eq!(
            rec.entries,
            vec![LayeredMaterialSwapEntry {
                key: r"Architecture\IndustrialKit\Interiors\IndIntMetalTileGrey01.mat".into(),
                key_material_path_form: None,
                override_material_path:
                    r"Architecture\City\Cydonia\Cydonia_Paintedmetal02_OffWhite01.mat".into(),
            }],
            "a 0 KeyMaterialPathForm is no form"
        );
    }

    #[test]
    fn collection_decodes_its_member_swaps() {
        let rec = lmsw(
            0x14_9FA2,
            "Cydonia_Pipe_GreywithOffWhite01_Collection_01",
            LMSW_COLLECTION,
        );
        assert!(rec.entries.is_empty());
        assert_eq!(rec.swaps_collection, vec![0x14_96BE, 0x34_BDA7]);
    }

    #[test]
    fn mtpt_decodes_its_material_path() {
        assert_eq!(
            parse_mtpt(&[sub(b"REFL", bytes(MTPT_TRIM_GREY03))], &None).as_deref(),
            Some(r"Architecture\IndustrialKit\Interiors\IndIntMetalTrimGrey03.mat")
        );
    }

    /// A blob whose root is another class (here an LMSW handed to the
    /// MTPT reader, and the reverse) yields nothing rather than a guess.
    #[test]
    fn a_foreign_root_class_decodes_to_nothing() {
        assert_eq!(
            parse_mtpt(&[sub(b"REFL", bytes(LMSW_INLINE_KEY))], &None),
            None
        );
        let rec = lmsw(0x1F_A537, "IndIntMetalTrimGrey03", MTPT_TRIM_GREY03);
        assert!(rec.entries.is_empty() && rec.swaps_collection.is_empty());
    }

    /// The Ref leaves are load-order remapped at decode time: mod-index
    /// `0x00` (the plugin's first master) lands in that master's global
    /// slot.
    #[test]
    fn ref_leaves_are_remapped() {
        use crate::esm::reader::GlobalSlot;
        let remap = Some(FormIdRemap {
            plugin_slot: GlobalSlot::Regular(0x02),
            master_slots: vec![GlobalSlot::Regular(0x01)],
            scheme: Default::default(),
        });
        let rec = parse_lmsw(0x14_9FA2, &[sub(b"REFL", bytes(LMSW_COLLECTION))], &remap);
        assert_eq!(rec.swaps_collection, vec![0x0114_96BE, 0x0134_BDA7]);
    }
}
