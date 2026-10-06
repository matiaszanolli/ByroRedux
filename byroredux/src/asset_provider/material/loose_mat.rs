//! #4277 — the loose `.mat` JSON resolver (Stage A of #762, the
//! deliverable that issue closed without building).
//!
//! Starfield material paths captured by the NIF stopcond end in `.mat`,
//! and the shipped answer is the compiled Component Database
//! (`materialsbeta.cdb`, handled by [`super::cdb`]). The *loose* form —
//! a `.mat` JSON file sitting in an archive — is the authored source the
//! CDB compiles from. The census cited in `cdb.rs`'s doc found only 20
//! loose `.mat` across the full 129-archive + Creation corpus, all
//! third-party, and a re-scan of this install's 129 archives with
//! `ba2_grep` finds **zero** — so no vanilla sample exists to derive the
//! JSON dialect from.
//!
//! What CAN be sourced without a sample: the component type names and
//! property names, because the compiled CDB stores them verbatim in its
//! string tables (see `crates/sfmaterial/src/index.rs`'s per-field docs —
//! `MRTextureFile`, `TextureReplacement`, `MaterialParamFloat`,
//! `AlphaSettingsComponent.AlphaTestThreshold`/`HasOpacity`,
//! `EffectSettingsComponent.IsGlass`,
//! `TranslucencySettings.UseSSS`/`TransmissiveScale`, `Components.Index`,
//! `Color`, `Enabled`, `Value`). The decode below matches those spellings
//! (plus case-insensitive tolerance for hand-edited mods) and ignores
//! everything it does not recognize — a partial decode fills only the
//! components it understood, and a non-JSON payload degrades to
//! recognition-only rather than a guess.
//!
//! Stage A contract:
//! * a loose `.mat` file that the archives actually carry **wins over the
//!   CDB** — the loose file is the authored source;
//! * a decodable file merges its recognized components (`Merged`);
//! * an undecodable-but-present file still routes as an authored external
//!   material (`is_pbr` + `ImportedTextureSource::Mat` provenance,
//!   `PresenceOnly`) instead of falling through to the CDB key hash;
//! * everything is parked on a real sample: when one lands, fix the
//!   spellings here against it.

use byroredux_sfmaterial::CdbMaterial;

/// Where a loose `.mat` payload got to.
#[derive(Debug, PartialEq)]
pub(crate) enum LooseMatStage {
    /// JSON decoded; recognized components captured below.
    Decoded,
    /// Present in the archives but not decodable as the component JSON —
    /// recognition-only (PBR routing + provenance, no authored field).
    Undecodable,
}

/// Decode a loose `.mat` payload into the [`CdbMaterial`] shape
/// `apply_cdb_material` already consumes. Returns `None` when the bytes
/// are not JSON at all.
pub(crate) fn parse_loose_mat(bytes: &[u8]) -> Option<(LooseMatStage, CdbMaterial)> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let components = find_array(&value, &["Components", "components"])?;
    let mut out = CdbMaterial::default();
    let mut recognized = false;
    for component in components.iter().filter_map(|v| v.as_object()) {
        let type_name = component
            .iter()
            .find(|(k, _)| k.starts_with('$') || k.eq_ignore_ascii_case("Type"))
            .and_then(|(_, v)| v.as_str())
            .unwrap_or_default();
        let get = |name: &str| {
            component
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(name))
                .map(|(_, v)| v)
        };
        let index = get("Index")
            .and_then(|v| v.as_u64())
            .map(|v| v as u8);
        if type_name.contains("MRTextureFile") || type_name.contains("TextureFile") {
            if let (Some(slot), Some(file)) = (index, get("File").or_else(|| get("Path"))) {
                if let Some(file) = file.as_str() {
                    if !file.is_empty() {
                        out.textures.push((slot, file.replace('/', "\\")));
                        recognized = true;
                    }
                }
            }
        } else if type_name.contains("TextureReplacement") {
            let enabled = get("Enabled").and_then(|v| v.as_bool());
            if enabled != Some(false) {
                if let (Some(slot), Some(color)) = (index, get("Color")) {
                    let rgba = decode_rgba(color);
                    out.flat_color_slots.push((slot, rgba));
                    if color_is_authored(color) {
                        recognized = true;
                    }
                }
            }
        } else if type_name.contains("MaterialParamFloat") {
            if let (Some(idx), Some(value)) = (index, get("Value").or_else(|| get("Float"))) {
                if let Some(value) = value.as_f64() {
                    out.param_floats.push((idx, value as f32));
                    recognized = true;
                }
            }
        } else if type_name.contains("AlphaSettings") {
            if let Some(v) = get("AlphaTestThreshold").and_then(|v| v.as_f64()) {
                out.alpha_test_threshold = Some(v as f32);
                recognized = true;
            }
            if let Some(v) = get("HasOpacity").and_then(|v| v.as_bool()) {
                out.has_opacity = Some(v);
                recognized = true;
            }
        } else if type_name.contains("EffectSettings") {
            if let Some(v) = get("IsGlass").and_then(|v| v.as_bool()) {
                out.is_glass = Some(v);
                recognized = true;
            }
        } else if type_name.contains("TranslucencySettings") {
            if let Some(v) = get("UseSSS").and_then(|v| v.as_bool()) {
                out.use_sss = Some(v);
                recognized = true;
            }
            if let Some(v) = get("TransmissiveScale").and_then(|v| v.as_f64()) {
                out.transmissive_scale = Some(v as f32);
                recognized = true;
            }
        }
        // Unrecognized component types are ignored — a partial decode is
        // strictly better than declining the file.
    }
    Some((
        if recognized {
            LooseMatStage::Decoded
        } else {
            LooseMatStage::Undecodable
        },
        out,
    ))
}

/// The `Components` array, whichever key spelling it landed under.
fn find_array<'v>(
    value: &'v serde_json::Value,
    keys: &[&str],
) -> Option<&'v Vec<serde_json::Value>> {
    let object = value.as_object()?;
    keys.iter()
        .find_map(|k| object.get(*k))
        .or_else(|| {
            object
                .iter()
                .find(|(k, _)| keys.iter().any(|want| k.eq_ignore_ascii_case(want)))
                .map(|(_, v)| v)
        })
        .and_then(|v| v.as_array())
}

fn decode_rgba(color: &serde_json::Value) -> [f32; 4] {
    match color {
        serde_json::Value::Array(items) => {
            let channel = |i: usize| {
                items
                    .get(i)
                    .and_then(|v| v.as_f64())
                    .map(|v| v as f32)
                    .unwrap_or(1.0)
            };
            [channel(0), channel(1), channel(2), channel(3)]
        }
        // A hex string (`"#RRGGBB"` / `"RRGGBBAA"`) is the other common
        // Bethesda JSON colour spelling; tolerated, not yet sourced.
        serde_json::Value::String(s) => {
            let hex = s.trim_start_matches('#');
            let channel = |i: usize| {
                u8::from_str_radix(hex.get(i..i + 2).unwrap_or("FF"), 16).unwrap_or(255) as f32
                    / 255.0
            };
            if hex.len() >= 6 {
                [channel(0), channel(2), channel(4), channel(6.min(hex.len().saturating_sub(2)))]
            } else {
                [1.0; 4]
            }
        }
        _ => [1.0; 4],
    }
}

/// A `Color` that actually carries a value (not `null` / empty).
fn color_is_authored(color: &serde_json::Value) -> bool {
    !color.is_null()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_the_cited_component_spellings() {
        let json = br#"{
            "Components": [
                {"$type": "MRTextureFile", "Index": 0, "File": "textures/foo/bar_d.dds"},
                {"$type": "MRTextureFile", "Index": 3, "File": "textures/foo/bar_r.dds"},
                {"$type": "MaterialParamFloat", "Index": 1, "Value": 0.25},
                {"$type": "AlphaSettingsComponent", "AlphaTestThreshold": 0.5, "HasOpacity": true},
                {"$type": "EffectSettingsComponent", "IsGlass": true},
                {"$type": "TranslucencySettings", "UseSSS": true, "TransmissiveScale": 0.8},
                {"$type": "TextureReplacement", "Index": 4, "Color": [0.25, 0.5, 0.75, 1.0], "Enabled": true},
                {"$type": "SomethingUnknownToUs", "Whatever": 1}
            ]
        }"#;
        let (stage, mat) = parse_loose_mat(json).expect("decodes");
        assert_eq!(stage, LooseMatStage::Decoded);
        assert!(mat
            .textures
            .contains(&(0, "textures\\foo\\bar_d.dds".to_owned())));
        assert!(mat.textures.contains(&(3, "textures\\foo\\bar_r.dds".to_owned())));
        assert_eq!(mat.param_floats, vec![(1, 0.25)]);
        assert_eq!(mat.alpha_test_threshold, Some(0.5));
        assert_eq!(mat.has_opacity, Some(true));
        assert_eq!(mat.is_glass, Some(true));
        assert_eq!(mat.use_sss, Some(true));
        assert_eq!(mat.transmissive_scale, Some(0.8));
        assert_eq!(mat.flat_color_slots.len(), 1);
        assert_eq!(mat.flat_color_slots[0].0, 4);
    }

    #[test]
    fn disabled_texture_replacement_is_skipped() {
        let json = br#"{"components": [
            {"$type": "TextureReplacement", "Index": 0, "Color": [1,1,1,1], "Enabled": false}
        ]}"#;
        let (_, mat) = parse_loose_mat(json).expect("lowercase components key tolerated");
        assert!(mat.flat_color_slots.is_empty());
    }

    #[test]
    fn non_json_degrades_to_undecodable_none() {
        assert!(parse_loose_mat(b"\x00\x01binary garbage").is_none());
        // JSON without a Components array is not the dialect we know.
        assert!(parse_loose_mat(br#"{"hello": "world"}"#).is_none());
    }

    #[test]
    fn recognized_nothing_is_undecodable_stage_not_a_failure() {
        let json = br#"{"Components": [{"$type": "FutureComponent"}]}"#;
        let (stage, _) = parse_loose_mat(json).expect("parses");
        assert_eq!(stage, LooseMatStage::Undecodable);
    }
}
