use super::*;
use crate::form_id::LocalFormId;
use std::collections::HashMap;

/// A regular-slots-only load order: slot `i` is `plugins[i]`.
struct Order(Vec<PluginId>);

impl FormIdTranslator for Order {
    fn pair(&self, global: u32) -> Option<FormIdPair> {
        let plugin = *self.0.get((global >> 24) as usize)?;
        Some(FormIdPair { plugin, local: LocalFormId(global & 0x00FF_FFFF) })
    }
    fn global(&self, pair: FormIdPair) -> Option<u32> {
        let slot = self.0.iter().position(|plugin| *plugin == pair.plugin)?;
        Some(((slot as u32) << 24) | pair.local.0)
    }
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct Saved {
    #[serde(with = "crate::stable_form_id")]
    item: u32,
    #[serde(with = "crate::stable_form_id::option")]
    equipped: Option<u32>,
    #[serde(with = "crate::stable_form_id::vec")]
    perks: Vec<u32>,
    #[serde(with = "crate::stable_form_id::map")]
    values: HashMap<u32, f32>,
    count: u32,
}

fn sample() -> Saved {
    Saved {
        item: 0x0200_0800,
        equipped: Some(0x0100_0801),
        perks: vec![0x0000_0010, 0x0100_0802, 0x0200_0803],
        values: HashMap::from([(0x0000_0020, 1.0), (0x0100_0804, 2.0)]),
        count: 0x0100_0000,
    }
}

/// A save made with `ModA.esp` at slot 1 and `ModB.esp` at slot 2 loads into an order where `ModA.esp` is gone and
/// `ModB.esp` moved to slot 1: ModB's forms follow it, ModA's read as absent, the game's are unchanged, and a plain
/// `u32` that is not a FormID is left alone.
#[test]
fn a_save_reads_back_under_a_changed_load_order() {
    let (skyrim, mod_a, mod_b) =
        (PluginId::from_filename("Skyrim.esm"), PluginId::from_filename("ModA.esp"), PluginId::from_filename("ModB.esp"));
    let (json, saved) = with_translator(Rc::new(Order(vec![skyrim, mod_a, mod_b])), || serde_json::to_value(sample()).unwrap());
    assert_eq!(saved.translated, 7);
    assert!(json["item"].is_object(), "a stable identity, not a number: {json}");
    assert_eq!(json["count"], 0x0100_0000, "an unmarked field stays a number");

    let (loaded, report) = with_translator(Rc::new(Order(vec![skyrim, mod_b])), || serde_json::from_value::<Saved>(json).unwrap());
    assert_eq!(loaded.item, 0x0100_0800, "ModB's form follows ModB to slot 1");
    assert_eq!(loaded.equipped, None, "ModA's form is absent");
    assert_eq!(loaded.perks, vec![0x0000_0010, 0x0100_0803]);
    assert_eq!(loaded.values, HashMap::from([(0x0000_0020, 1.0)]));
    assert_eq!(loaded.count, 0x0100_0000);
    assert_eq!(report.absent, BTreeMap::from([(mod_a, 3)]));
}

/// Without a translator the adapters write and read plain numbers, so a unit test's round trip is unchanged; a stable
/// identity read with none installed (a typed preflight) is the null form.
#[test]
fn without_a_translator_the_form_is_the_plain_number() {
    let json = serde_json::to_value(sample()).unwrap();
    assert_eq!(json["item"], 0x0200_0800);
    assert_eq!(serde_json::from_value::<Saved>(json).unwrap(), sample());

    let skyrim = PluginId::from_filename("Skyrim.esm");
    let (json, _) = with_translator(Rc::new(Order(vec![skyrim])), || serde_json::to_value(sample()).unwrap());
    let read = serde_json::from_value::<Saved>(json).unwrap();
    assert_eq!(
        read.perks,
        vec![0x0100_0802, 0x0200_0803],
        "the game's form was written as a pair and reads as absent with no translator; the forms no plugin of that \
         order owns were written as numbers and read back unchanged"
    );
}

/// FormID 0 and a FormID no plugin of the load order owns (an engine-made reference) stay numbers.
#[test]
fn a_form_no_plugin_owns_is_written_as_its_number() {
    let skyrim = PluginId::from_filename("Skyrim.esm");
    let (json, report) = with_translator(Rc::new(Order(vec![skyrim])), || {
        serde_json::to_value(Saved { item: 0, equipped: Some(0xFF00_0001), perks: vec![], values: HashMap::new(), count: 0 })
            .unwrap()
    });
    assert_eq!((json["item"].clone(), json["equipped"].clone()), (serde_json::json!(0), serde_json::json!(0xFF00_0001u32)));
    assert_eq!(report.translated, 0);
}
