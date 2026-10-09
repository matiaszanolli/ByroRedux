//! #4277 — the loose `.mat` JSON resolver (Stage A of #762, the
//! deliverable that issue closed without building).
//!
//! Starfield material paths captured by the NIF stopcond end in `.mat`,
//! and the shipped answer is the compiled Component Database
//! (`materialsbeta.cdb`, handled by [`super::cdb`]). The *loose* form —
//! a `.mat` JSON file sitting in an archive — is the authored source the
//! CDB compiles from. Vanilla ships none; this install's Creation
//! archives carry 20 (`qog-pawnshop`, `sp2_factionrequisitionkiosks`,
//! `starfieldresourcerevival`, `avontechshipyards` — `ba2_grep`,
//! 2026-10-09).
//!
//! #5396 — the layout, read off those installed files (the first decoder
//! was written for an invented one and decoded none of them):
//!
//! ```text
//! { "Filename": …, "Import": [ … ], "Version": 1, "Summary": { … },
//!   "Objects": [ { "Parent": …, "ID": …, "Edges": [ … ],
//!       "Components": [ { "Type": "BSMaterial::MRTextureFile", "Index": 0,
//!                         "Data": { "FileName": "Data\\Textures\\…\\x_color.dds" } },
//!                       { "Type": "BSMaterial::TextureReplacement", "Index": 7,
//!                         "Data": { "Enabled": "true",
//!                                   "Color": { "Type": "BSMaterial::Color",
//!                                     "Data": { "Value": { "Type": "XMFLOAT4",
//!                                       "Data": { "x": "1.000000", "y": …, "z": …, "w": … } } } } } },
//!                       { "Type": "BSMaterial::AlphaSettingsComponent", "Index": 0,
//!                         "Data": { "AlphaTestThreshold": "0.5", "HasOpacity": "true" } } ] } ] }
//! ```
//!
//! Components live at `Objects[].Components[]`, their properties under
//! `Data`; scalars and bools are **strings**; a texture path carries a
//! `Data\` prefix. A loose file is a diff against its `Parent` material:
//! a property it does not author is inherited, so nothing absent is
//! filled with a default — an enabled replacement with no `Color` (two of
//! the four samples) contributes no colour.
//!
//! Stage A contract:
//! * a loose `.mat` file that the archives actually carry **wins over the
//!   CDB** when it decodes — the loose file is the authored source;
//! * a decodable file merges its recognized components (`Decoded`);
//! * a file with no recognized component, or no JSON at all, does not
//!   pre-empt the CDB: the caller falls through to the compiled lookup
//!   (#5396 — it used to return recognition-only and hide the CDB row).

use byroredux_sfmaterial::CdbMaterial;

/// Where a loose `.mat` payload got to.
#[derive(Debug, PartialEq)]
pub(crate) enum LooseMatStage {
    /// JSON decoded; recognized components captured below.
    Decoded,
    /// JSON with the component layout, but no component this decoder
    /// recognizes.
    Undecodable,
}

/// Decode a loose `.mat` payload into the [`CdbMaterial`] shape
/// `apply_cdb_material` already consumes. Returns `None` when the bytes
/// are not JSON or carry no `Objects` array.
pub(crate) fn parse_loose_mat(bytes: &[u8]) -> Option<(LooseMatStage, CdbMaterial)> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let objects = value.as_object()?.get("Objects")?.as_array()?;
    let components = objects
        .iter()
        .filter_map(|object| object.get("Components")?.as_array())
        .flatten()
        .filter_map(|component| component.as_object());
    let mut out = CdbMaterial::default();
    let mut recognized = false;
    for component in components {
        let type_name = component
            .get("Type")
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let Some(data) = component.get("Data").and_then(|v| v.as_object()) else {
            continue;
        };
        // PAR-D3-2026-10-08-02 — an out-of-range slot is dropped, never
        // wrapped onto another slot.
        let index = component
            .get("Index")
            .and_then(|v| v.as_u64())
            .and_then(|v| u8::try_from(v).ok());
        match type_name.rsplit("::").next().unwrap_or_default() {
            "MRTextureFile" => {
                if let (Some(slot), Some(file)) =
                    (index, data.get("FileName").and_then(|v| v.as_str()))
                {
                    let file = texture_path(file);
                    if !file.is_empty() {
                        out.textures.push((slot, file));
                        recognized = true;
                    }
                }
            }
            "TextureReplacement" => {
                // `Enabled` absent counts as enabled — the CDB capture's
                // rule (`CdbMaterial::flat_color_slots`).
                if json_bool(data.get("Enabled")) != Some(false) {
                    if let (Some(slot), Some(rgba)) =
                        (index, data.get("Color").and_then(xmfloat4))
                    {
                        out.flat_color_slots.push((slot, rgba));
                        recognized = true;
                    }
                }
            }
            "MaterialParamFloat" => {
                if let (Some(idx), Some(value)) = (index, json_f32(data.get("Value"))) {
                    out.param_floats.push((idx, value));
                    recognized = true;
                }
            }
            "AlphaSettingsComponent" => {
                if let Some(v) = json_f32(data.get("AlphaTestThreshold")) {
                    out.alpha_test_threshold = Some(v);
                    recognized = true;
                }
                if let Some(v) = json_bool(data.get("HasOpacity")) {
                    out.has_opacity = Some(v);
                    recognized = true;
                }
            }
            "EffectSettingsComponent" => {
                if let Some(v) = json_bool(data.get("IsGlass")) {
                    out.is_glass = Some(v);
                    recognized = true;
                }
            }
            "TranslucencySettingsComponent" | "TranslucencySettings" => {
                if let Some(v) = json_bool(data.get("UseSSS")) {
                    out.use_sss = Some(v);
                    recognized = true;
                }
                if let Some(v) = json_f32(data.get("TransmissiveScale")) {
                    out.transmissive_scale = Some(v);
                    recognized = true;
                }
            }
            // Unrecognized component types are ignored — a partial decode
            // is strictly better than declining the file.
            _ => {}
        }
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

/// A texture path in the archive convention: backslash separators and
/// no `Data\` prefix (the loose file writes `Data\Textures\…`).
fn texture_path(file: &str) -> String {
    let file = file.replace('/', "\\");
    match file.get(..5) {
        Some(prefix) if prefix.eq_ignore_ascii_case("data\\") => file[5..].to_string(),
        _ => file,
    }
}

/// A bool authored as a JSON bool or as the string `"true"` / `"false"`
/// (the loose files' spelling).
fn json_bool(value: Option<&serde_json::Value>) -> Option<bool> {
    match value? {
        serde_json::Value::Bool(b) => Some(*b),
        serde_json::Value::String(s) if s.eq_ignore_ascii_case("true") => Some(true),
        serde_json::Value::String(s) if s.eq_ignore_ascii_case("false") => Some(false),
        _ => None,
    }
}

/// A finite float authored as a JSON number or a numeric string.
fn json_f32(value: Option<&serde_json::Value>) -> Option<f32> {
    let v = match value? {
        serde_json::Value::Number(n) => n.as_f64()? as f32,
        serde_json::Value::String(s) => s.trim().parse::<f32>().ok()?,
        _ => return None,
    };
    v.is_finite().then_some(v)
}

/// A `BSMaterial::Color`: `Data.Value.Data` holds the `XMFLOAT4`'s
/// `x`/`y`/`z`/`w`. All four channels must be authored — a missing one
/// is not invented.
fn xmfloat4(color: &serde_json::Value) -> Option<[f32; 4]> {
    let channels = color.get("Data")?.get("Value")?.get("Data")?;
    Some([
        json_f32(channels.get("x"))?,
        json_f32(channels.get("y"))?,
        json_f32(channels.get("z"))?,
        json_f32(channels.get("w"))?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #5396 — verbatim excerpts of installed Creation files (the
    /// `galacticpawnshopterminal_terminalcase.mat` texture-set object,
    /// `portablegreenhouse02.mat`'s alpha settings and
    /// `lasersight_white.mat`'s two replacements), in their real layout.
    const REAL_LAYOUT: &[u8] = br#"{
	"Filename" : "Materials\\QOG\\Pawnshop\\GalacticPawnShopTerminal_TerminalCase.mat",
	"Import" : [ "Data\\MATERIALS\\Layered\\ShaderModels\\ColorEmissive.mat" ],
	"Objects" :
	[
		{
			"Components" :
			[
				{ "Data" : { "Name" : "GalacticPawnShopTerminal_TerminalCase_TextureSet1" }, "Index" : 0, "Type" : "BSComponentDB::CTName" },
				{ "Data" : { "Enabled" : "false" }, "Index" : 7, "Type" : "BSMaterial::TextureReplacement" },
				{ "Data" : { "FileName" : "Data\\Textures\\QOG\\Pawnshop\\GalacticPawnShopTerminal\\TerminalCase_color.dds" }, "Index" : 0, "Type" : "BSMaterial::MRTextureFile", "Version" : 2 },
				{ "Data" : { "FileName" : "Data\\Textures\\QOG\\Pawnshop\\GalacticPawnShopTerminal\\TerminalCase_normal.dds" }, "Index" : 1, "Type" : "BSMaterial::MRTextureFile", "Version" : 2 },
				{ "Data" : { "FileName" : "Data\\Textures\\QOG\\Pawnshop\\GalacticPawnShopTerminal\\TerminalCase_emissive.dds" }, "Index" : 7, "Type" : "BSMaterial::MRTextureFile", "Version" : 2 }
			],
			"ID" : "res:CE0A3062:000625D2:A183B6ED",
			"Parent" : "res:68CD9647:0005AE81:A06346EA"
		},
		{
			"Components" :
			[
				{ "Data" : { "AlphaTestThreshold" : "0.5", "HasOpacity" : "true", "OpacitySourceLayer" : "MATERIAL_LAYER_1" }, "Index" : 0, "Type" : "BSMaterial::AlphaSettingsComponent" },
				{ "Data" : { "Enabled" : "true" }, "Index" : 2, "Type" : "BSMaterial::TextureReplacement" },
				{ "Data" : { "Color" : { "Data" : { "Value" : { "Data" : { "w" : "1.000000", "x" : "1.000000", "y" : "0.428672", "z" : "1.000000" }, "Type" : "XMFLOAT4" } }, "Type" : "BSMaterial::Color" }, "Enabled" : "true" }, "Index" : 3, "Type" : "BSMaterial::TextureReplacement" }
			]
		}
	],
	"Version" : 1
}"#;

    /// #5396 — the real layout decodes end to end: `Data\` stripped from
    /// texture paths, string scalars and bools parsed, the nested
    /// `XMFLOAT4` colour read, and an authored-disabled replacement or an
    /// enabled one with no `Color` contributes nothing.
    #[test]
    fn decodes_the_installed_loose_mat_layout() {
        let (stage, mat) = parse_loose_mat(REAL_LAYOUT).expect("decodes");
        assert_eq!(stage, LooseMatStage::Decoded);
        assert_eq!(
            mat.textures,
            vec![
                (0, "Textures\\QOG\\Pawnshop\\GalacticPawnShopTerminal\\TerminalCase_color.dds".to_owned()),
                (1, "Textures\\QOG\\Pawnshop\\GalacticPawnShopTerminal\\TerminalCase_normal.dds".to_owned()),
                (7, "Textures\\QOG\\Pawnshop\\GalacticPawnShopTerminal\\TerminalCase_emissive.dds".to_owned()),
            ]
        );
        assert_eq!(mat.alpha_test_threshold, Some(0.5));
        assert_eq!(mat.has_opacity, Some(true));
        assert_eq!(
            mat.flat_color_slots,
            vec![(3, [1.0, 0.428672, 1.0, 1.0])],
            "slot 7 is authored-disabled and slot 2 authors no colour"
        );
    }

    /// PAR-D3-2026-10-08-02 — an out-of-range `Index` is dropped, not
    /// wrapped onto slot 0.
    #[test]
    fn out_of_range_index_is_dropped_not_wrapped() {
        let json = br#"{"Objects": [{"Components": [
            {"Data": {"FileName": "Data\\Textures\\x.dds"}, "Index": 256, "Type": "BSMaterial::MRTextureFile"}
        ]}]}"#;
        let (stage, mat) = parse_loose_mat(json).expect("parses");
        assert_eq!(stage, LooseMatStage::Undecodable);
        assert!(mat.textures.is_empty());
    }

    /// A colour missing a channel is not completed with an invented value.
    #[test]
    fn partial_colour_is_not_completed() {
        let json = br#"{"Objects": [{"Components": [
            {"Data": {"Enabled": "true", "Color": {"Data": {"Value": {"Data": {"x": "1", "y": "0", "z": "0"}}}}},
             "Index": 0, "Type": "BSMaterial::TextureReplacement"}
        ]}]}"#;
        let (_, mat) = parse_loose_mat(json).expect("parses");
        assert!(mat.flat_color_slots.is_empty());
    }

    #[test]
    fn non_json_or_no_objects_is_none() {
        assert!(parse_loose_mat(b"\x00\x01binary garbage").is_none());
        assert!(parse_loose_mat(br#"{"hello": "world"}"#).is_none());
    }

    #[test]
    fn recognized_nothing_is_undecodable_stage_not_a_failure() {
        let json = br#"{"Objects": [{"Components": [{"Data": {"Name": "x"}, "Index": 0, "Type": "BSComponentDB::CTName"}]}]}"#;
        let (stage, _) = parse_loose_mat(json).expect("parses");
        assert_eq!(stage, LooseMatStage::Undecodable);
    }
}
