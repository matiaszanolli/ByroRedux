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
//! The objects form the same graph the database stores: the root (the object
//! with no `ID`) names layers (`LayerID`), a layer its material
//! (`MaterialID`), a material its texture set (`TextureSetID`). Other objects
//! hang off the root too (blenders, UV streams, LOD materials with settings
//! of their own). Opacity, roughness, metalness and the alpha settings are
//! read along that graph, as
//! [`byroredux_sfmaterial::MaterialIndex::lookup`] reads them (SMM-SRC-002,
//! #7421, #7422); the other properties still merge every object.
//! A `Parent` naming another object in the file is inherited. Where the
//! root's parent is a `.mat` path the database holds, the alpha settings the
//! file leaves unauthored come from it ([`database_parent`]); no other
//! property, and no database resource id, is followed outside the file.
//!
//! Stage A contract:
//! * a loose `.mat` file that the archives actually carry **wins over the
//!   CDB** when it decodes — the loose file is the authored source;
//! * a decodable file merges its recognized components (`Decoded`);
//! * a file with no recognized component, or no JSON at all, does not
//!   pre-empt the CDB: the caller falls through to the compiled lookup
//!   (#5396 — it used to return recognition-only and hide the CDB row).

use byroredux_sfmaterial::{CdbMaterial, CdbSlotSet};

/// Where a loose `.mat` payload got to.
#[derive(Debug, PartialEq)]
pub(crate) enum LooseMatStage {
    /// JSON decoded; recognized components captured below.
    Decoded,
    /// JSON with the component layout, but no component this decoder
    /// recognizes.
    Undecodable,
}

/// The database material the loose file is a diff against, when the database
/// holds it: what its inherited alpha settings come from.
pub(super) fn database_parent(
    provider: &mut super::MaterialProvider,
    bytes: &[u8],
) -> Option<CdbMaterial> {
    provider.lookup_cdb_material(&loose_parent_path(bytes)?)
}

/// The `.mat` path the loose material is a diff against: its root's `Parent`,
/// when that names a material file.
fn loose_parent_path(bytes: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let objects = value.as_object()?.get("Objects")?.as_array()?;
    let parent = LooseGraph::new(objects)
        .roots()
        .find_map(|root| root.get("Parent")?.as_str().map(str::to_owned))?;
    parent.to_ascii_lowercase().ends_with(".mat").then_some(parent)
}

/// Decode a loose `.mat` payload into the [`CdbMaterial`] shape
/// `apply_cdb_material` already consumes. `parent` is the file's
/// [`database_parent`]. Returns `None`
/// when the bytes are not JSON or carry no `Objects` array.
pub(crate) fn parse_loose_mat(
    bytes: &[u8],
    parent: Option<&CdbMaterial>,
) -> Option<(LooseMatStage, CdbMaterial)> {
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
                if json_bool(data.get("Enabled")) != Some(false)
                    && let (Some(slot), Some(rgba)) =
                        (index, data.get("Color").and_then(xmfloat4))
                    {
                        out.flat_color_slots.push((slot, rgba));
                        recognized = true;
                    }
            }
            "MaterialParamFloat" => {
                if let (Some(idx), Some(value)) = (index, json_f32(data.get("Value"))) {
                    out.param_floats.push((idx, value));
                    recognized = true;
                }
            }
            // #5525 — the alpha, effect (IsGlass, BlendingMode) and emissive
            // settings components are read by the same `capture_settings` the
            // compiled CDB decoder uses, so the two producers of a
            // `CdbMaterial` cannot drift per component. The alpha settings
            // are read along the layer graph, below.
            "EffectSettingsComponent" | "EmissiveSettingsComponent" => {
                if let Some(capture) = byroredux_sfmaterial::capture_settings(type_name, LooseFields(data))
                    && !capture.is_empty() {
                        capture.fill_unset(&mut out);
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
    recognized |= apply_layer_graph(&LooseGraph::new(objects), parent, &mut out);
    Some((
        if recognized {
            LooseMatStage::Decoded
        } else {
            LooseMatStage::Undecodable
        },
        out,
    ))
}

type JsonMap = serde_json::Map<String, serde_json::Value>;

/// The alpha settings, the opacity layer and the one-layer slots, read as the database lookup
/// reads them: settings from the root and then each layer's material, nearest
/// authored value first; opacity from the one layer `OpacitySourceLayer`
/// names. A setting the file leaves unauthored is its parent's: a loose file
/// is a diff. Returns whether the file itself authors any alpha setting.
fn apply_layer_graph(
    graph: &LooseGraph,
    parent: Option<&CdbMaterial>,
    out: &mut CdbMaterial,
) -> bool {
    let roots: Vec<&JsonMap> = graph.roots().collect();
    let mut layer_refs: Vec<(u8, &str)> = Vec::new();
    for &root in &roots {
        for (index, id) in graph.references(root, "LayerID") {
            if !layer_refs.iter().any(|(n, _)| *n == index) {
                layer_refs.push((index, id));
            }
        }
    }
    layer_refs.sort_unstable_by_key(|(n, _)| *n);

    let mut materials: Vec<(u8, &JsonMap)> = Vec::new();
    for (number, layer) in layer_refs {
        let Some(layer) = graph.object(layer) else {
            continue;
        };
        if let Some(material) = graph.first_reference(layer, "MaterialID") {
            materials.push((number, material));
        }
    }

    let mut found_alpha = false;
    let settings_order = roots.iter().copied().chain(materials.iter().map(|(_, m)| *m));
    for object in settings_order {
        for data in graph.chain(object).flat_map(|o| components(o, "AlphaSettingsComponent")) {
            if let Some(capture) =
                byroredux_sfmaterial::capture_settings("AlphaSettingsComponent", LooseFields(data.1))
                && !capture.is_empty()
            {
                capture.fill_unset(out);
                found_alpha = true;
            }
        }
    }

    out.opacity_source_layer = roots
        .iter()
        .flat_map(|&root| graph.chain(root))
        .flat_map(|o| components(o, "AlphaSettingsComponent"))
        .find_map(|(_, data)| data.get("OpacitySourceLayer")?.as_str().map(str::to_owned));
    if let Some(parent) = parent {
        out.has_opacity = out.has_opacity.or(parent.has_opacity);
        out.alpha_test_threshold = out.alpha_test_threshold.or(parent.alpha_test_threshold);
        out.opacity_source_layer = out
            .opacity_source_layer
            .take()
            .or_else(|| parent.opacity_source_layer.clone());
    }

    let layers: Vec<(u8, CdbSlotSet)> = materials
        .iter()
        .map(|&(number, material)| {
            let set = graph
                .first_reference(material, "TextureSetID")
                .map(|set| graph.slot_set(set))
                .unwrap_or_default();
            (number, set)
        })
        .collect();
    // SMM-SRC-010 — the root-named set, then the numbered layers.
    let root_sets: Vec<CdbSlotSet> = roots
        .iter()
        .filter_map(|&root| graph.first_reference(root, "TextureSetID"))
        .map(|set| graph.slot_set(set))
        .collect();
    out.one_layer = byroredux_sfmaterial::one_layer_slots(
        root_sets.iter().chain(layers.iter().map(|(_, set)| set)),
    );
    out.opacity_layer =
        byroredux_sfmaterial::opacity_layer_of(out.opacity_source_layer.as_deref(), layers);
    found_alpha
}

/// `object`'s components of one type, as (`Index`, `Data`).
fn components<'a>(
    object: &'a JsonMap,
    kind: &'a str,
) -> impl Iterator<Item = (Option<u8>, &'a JsonMap)> + 'a {
    object
        .get("Components")
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter_map(move |component| {
            let component = component.as_object()?;
            let name = component.get("Type")?.as_str()?;
            if name.rsplit("::").next() != Some(kind) {
                return None;
            }
            let index = component
                .get("Index")
                .and_then(|v| v.as_u64())
                .and_then(|v| u8::try_from(v).ok());
            Some((index, component.get("Data")?.as_object()?))
        })
}

/// A loose file's objects, addressable by `ID`.
struct LooseGraph<'a> {
    objects: Vec<&'a JsonMap>,
    by_id: std::collections::HashMap<&'a str, &'a JsonMap>,
}

impl<'a> LooseGraph<'a> {
    fn new(objects: &'a [serde_json::Value]) -> Self {
        let objects: Vec<&JsonMap> = objects.iter().filter_map(|o| o.as_object()).collect();
        let by_id = objects
            .iter()
            .filter_map(|o| Some((o.get("ID")?.as_str()?, *o)))
            .collect();
        Self { objects, by_id }
    }

    /// The material's own objects: those with no `ID`.
    fn roots(&self) -> impl Iterator<Item = &'a JsonMap> + '_ {
        self.objects.iter().copied().filter(|o| !o.contains_key("ID"))
    }

    fn object(&self, id: &str) -> Option<&'a JsonMap> {
        self.by_id.get(id).copied()
    }

    /// `object`, then each `Parent` that is another object in this file.
    /// Bounded by the object count, so a cyclic file ends.
    fn chain(&self, object: &'a JsonMap) -> impl Iterator<Item = &'a JsonMap> + '_ {
        std::iter::successors(Some(object), move |o| self.object(o.get("Parent")?.as_str()?))
            .take(self.objects.len())
    }

    /// The `(Index, ID)` references of one type along `object`'s chain,
    /// nearest first.
    fn references(&self, object: &'a JsonMap, kind: &'static str) -> Vec<(u8, &'a str)> {
        self.chain(object)
            .flat_map(|o| components(o, kind))
            .filter_map(|(index, data)| Some((index?, data.get("ID")?.as_str()?)))
            .collect()
    }

    fn first_reference(&self, object: &'a JsonMap, kind: &'static str) -> Option<&'a JsonMap> {
        self.references(object, kind)
            .into_iter()
            .find_map(|(_, id)| self.object(id))
    }

    /// What a texture set supplies with its in-file `Parent` chain, nearest
    /// first; a disabled replacement switches an inherited one off.
    fn slot_set(&self, set: &'a JsonMap) -> CdbSlotSet {
        let mut out = CdbSlotSet::default();
        let mut replaced: Vec<u8> = Vec::new();
        for object in self.chain(set) {
            for (slot, data) in components(object, "MRTextureFile") {
                let Some(slot) = slot else { continue };
                let file = data.get("FileName").and_then(|v| v.as_str()).map(texture_path);
                if let Some(file) = file.filter(|f| !f.is_empty())
                    && out.texture(slot).is_none()
                {
                    out.textures.push((slot, file));
                }
            }
            for (slot, data) in components(object, "TextureReplacement") {
                let Some(slot) = slot else { continue };
                if replaced.contains(&slot) {
                    continue;
                }
                match (json_bool(data.get("Enabled")), data.get("Color").and_then(xmfloat4)) {
                    (Some(false), _) => replaced.push(slot),
                    (_, Some(rgba)) => {
                        replaced.push(slot);
                        out.flat_color_slots.push((slot, rgba));
                    }
                    (_, None) => {}
                }
            }
        }
        out
    }
}

/// A loose component's `Data` map as a [`byroredux_sfmaterial::FieldSource`]:
/// string-typed scalars, and nested structs as `{"Type", "Data"}` wrappers
/// (read off the installed Creation files, #5396).
#[derive(Clone, Copy)]
struct LooseFields<'a>(&'a serde_json::Map<String, serde_json::Value>);

impl byroredux_sfmaterial::FieldSource for LooseFields<'_> {
    fn bool_field(self, name: &str) -> Option<bool> {
        json_bool(self.0.get(name))
    }

    fn f32_field(self, name: &str) -> Option<f32> {
        json_f32(self.0.get(name))
    }

    fn string_field(self, name: &str) -> Option<String> {
        self.0.get(name)?.as_str().map(str::to_owned)
    }

    fn nested(self, name: &str) -> Option<Self> {
        self.0.get(name)?.get("Data")?.as_object().map(Self)
    }

    fn color4_field(self, name: &str) -> Option<[f32; 4]> {
        xmfloat4(self.0.get(name)?)
    }
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
        let (stage, mat) = parse_loose_mat(REAL_LAYOUT, None).expect("decodes");
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
        let (stage, mat) = parse_loose_mat(json, None).expect("parses");
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
        let (_, mat) = parse_loose_mat(json, None).expect("parses");
        assert!(mat.flat_color_slots.is_empty());
    }

    /// #5525 — the effect and emissive settings the compiled decoder
    /// captures are captured from the loose form too, in the nested
    /// `{"Type", "Data"}` layout the installed files use (a real pawnshop
    /// `EmissiveSettingsComponent` wraps its `Settings` exactly so).
    #[test]
    fn effect_and_emissive_settings_are_captured_like_the_compiled_decoder() {
        let json = br#"{"Objects": [{"Components": [
            {"Data": {"IsGlass": "true", "BlendingMode": "Additive"}, "Index": 0, "Type": "BSMaterial::EffectSettingsComponent"},
            {"Data": {"Enabled": "true", "Settings": {"Type": "BSMaterial::EmittanceSettings", "Data": {
                "LuminousEmittance": "432.0",
                "EmissiveTint": {"Type": "BSMaterial::Color", "Data": {"Value": {"Type": "XMFLOAT4", "Data":
                    {"x": "1.0", "y": "0.8", "z": "0.6", "w": "1.0"}}}}}}},
             "Index": 0, "Type": "BSMaterial::EmissiveSettingsComponent"}
        ]}]}"#;
        let (stage, mat) = parse_loose_mat(json, None).expect("decodes");
        assert_eq!(stage, LooseMatStage::Decoded);
        assert_eq!(mat.is_glass, Some(true));
        assert_eq!(mat.blending_mode.as_deref(), Some("Additive"));
        assert_eq!(mat.emissive_enabled, Some(true));
        assert_eq!(mat.emissive_tint, Some([1.0, 0.8, 0.6]));
        assert_eq!(mat.luminous_emittance, Some(432.0));
    }

    /// The real installed emissive components (the 12 QOG pawnshop parts)
    /// author `Settings` with `AdaptiveEmittance` only — no `Enabled`, tint or
    /// luminance (they inherit from the `ColorEmissive` shader model they
    /// import). A diff authors only what it changes, so nothing is captured and
    /// the file is not claimed as decoded on that component's account.
    #[test]
    fn a_real_pawnshop_emissive_component_captures_nothing_and_invents_nothing() {
        let json = br#"{"Objects": [{"Components": [
            {"Data": {"Settings": {"Data": {"AdaptiveEmittance": "true", "EnableAdaptiveLimits": "true"},
                                   "Type": "BSMaterial::EmittanceSettings"}},
             "Index": 0, "Type": "BSMaterial::EmissiveSettingsComponent", "Version": 1}
        ]}]}"#;
        let (stage, mat) = parse_loose_mat(json, None).expect("parses");
        assert_eq!(stage, LooseMatStage::Undecodable);
        assert_eq!(mat.emissive_enabled, None);
        assert_eq!(mat.emissive_tint, None);
        assert_eq!(mat.luminous_emittance, None);
    }

    /// #5525 — the drift guard: one authored component, expressed as a typed
    /// CDB instance and as loose JSON, captures identically, because both go
    /// through `capture_settings`.
    #[test]
    fn the_compiled_and_loose_representations_capture_identically() {
        use byroredux_sfmaterial::{capture_settings, ObjectInstance, Value};
        use std::collections::BTreeMap;

        let compiled = ObjectInstance {
            class_name: "BSMaterial::EffectSettingsComponent".into(),
            type_id: 0,
            fields: BTreeMap::from([
                ("IsGlass".to_string(), Value::Bool(true)),
                ("BlendingMode".to_string(), Value::String("SourceSoftAdditive".into())),
            ]),
        };
        let loose: serde_json::Value = serde_json::json!({
            "IsGlass": "true",
            "BlendingMode": "SourceSoftAdditive",
        });
        let from_compiled = capture_settings("BSMaterial::EffectSettingsComponent", &compiled);
        let from_loose = capture_settings(
            "BSMaterial::EffectSettingsComponent",
            LooseFields(loose.as_object().unwrap()),
        );
        assert!(from_compiled.is_some());
        assert_eq!(from_compiled, from_loose);
    }

    #[test]
    fn non_json_or_no_objects_is_none() {
        assert!(parse_loose_mat(b"\x00\x01binary garbage", None).is_none());
        assert!(parse_loose_mat(br#"{"hello": "world"}"#, None).is_none());
    }

    #[test]
    fn recognized_nothing_is_undecodable_stage_not_a_failure() {
        let json = br#"{"Objects": [{"Components": [{"Data": {"Name": "x"}, "Index": 0, "Type": "BSComponentDB::CTName"}]}]}"#;
        let (stage, _) = parse_loose_mat(json, None).expect("parses");
        assert_eq!(stage, LooseMatStage::Undecodable);
    }
}
