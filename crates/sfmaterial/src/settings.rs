//! #5525 — the settings-component capture both [`crate::CdbMaterial`]
//! producers share.
//!
//! A `CdbMaterial` is filled from two decoders of the same authored data: the
//! compiled Component Database (`MaterialIndex`, typed [`Value`]s) and the
//! loose `.mat` JSON (the engine's `parse_loose_mat`, string-typed values).
//! 9bb6b7dce extended only the compiled one, so the loose decoder dropped
//! `BlendingMode` and the emissive settings the compiled one captured, and
//! the two would have kept drifting every time one gained a capture.
//!
//! The per-component rule — which class carries which field, and where the
//! nested `Settings` struct sits — is stated here once. Each decoder only
//! says how to read a typed field out of its own representation
//! ([`FieldSource`]).
//!
//! Covered: `AlphaSettingsComponent`, `EffectSettingsComponent` and
//! `EmissiveSettingsComponent`. `TranslucencySettingsComponent` stays with
//! each decoder: the compiled arm reads its inline `Settings` (#5673), and no
//! installed loose file authors the component, so its loose layout is
//! unverified.

/// Typed field access over one component's fields — a compiled object
/// instance or a loose `.mat` `Data` map. `Copy` so a nested struct is just
/// another source of the same kind.
pub trait FieldSource: Copy {
    fn bool_field(self, name: &str) -> Option<bool>;
    fn f32_field(self, name: &str) -> Option<f32>;
    fn string_field(self, name: &str) -> Option<String>;
    /// The fields of a nested struct field (`Settings`, an inline object
    /// in the compiled form and a `{"Type", "Data"}` wrapper in the loose one).
    fn nested(self, name: &str) -> Option<Self>;
    /// An `XMFLOAT4` colour reached through the wrapper field `name`
    /// (`Color`, `EmissiveTint`): `<name>.Value.{x,y,z,w}`, every channel
    /// authored.
    fn color4_field(self, name: &str) -> Option<[f32; 4]>;
}

/// The fields one settings component authors. Every field is `None` when the
/// component does not author it — a diff against its parent, so nothing
/// absent is filled with a default.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct SettingsCapture {
    /// `AlphaSettingsComponent.AlphaTestThreshold`.
    pub alpha_test_threshold: Option<f32>,
    /// `AlphaSettingsComponent.HasOpacity`.
    pub has_opacity: Option<bool>,
    /// `EffectSettingsComponent.IsGlass`.
    pub is_glass: Option<bool>,
    /// `EffectSettingsComponent.BlendingMode` (#5277).
    pub blending_mode: Option<String>,
    /// `EmissiveSettingsComponent.Enabled` (#5283).
    pub emissive_enabled: Option<bool>,
    /// `EmissiveSettingsComponent.Settings.EmissiveTint`, linear RGB.
    pub emissive_tint: Option<[f32; 3]>,
    /// `EmissiveSettingsComponent.Settings.LuminousEmittance`.
    pub luminous_emittance: Option<f32>,
}

impl SettingsCapture {
    /// Whether the component authored none of the captured fields.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Fill the fields of `out` this capture authors and `out` does not yet
    /// have: first writer wins, the rule the compiled lookup applies down an
    /// object's `Parent` chain.
    pub fn fill_unset(&self, out: &mut crate::CdbMaterial) {
        fn fill<T: Clone>(slot: &mut Option<T>, value: &Option<T>) {
            if slot.is_none() {
                *slot = value.clone();
            }
        }
        fill(&mut out.alpha_test_threshold, &self.alpha_test_threshold);
        fill(&mut out.has_opacity, &self.has_opacity);
        fill(&mut out.is_glass, &self.is_glass);
        fill(&mut out.blending_mode, &self.blending_mode);
        fill(&mut out.emissive_enabled, &self.emissive_enabled);
        fill(&mut out.emissive_tint, &self.emissive_tint);
        fill(&mut out.luminous_emittance, &self.luminous_emittance);
    }
}

/// Capture the settings `class` (`BSMaterial::EffectSettingsComponent`, or
/// the short name) authors in `fields`. `None` when `class` is not one of
/// the covered settings components.
pub fn capture_settings<F: FieldSource>(class: &str, fields: F) -> Option<SettingsCapture> {
    let mut capture = SettingsCapture::default();
    match class.rsplit("::").next().unwrap_or(class) {
        "AlphaSettingsComponent" => {
            capture.alpha_test_threshold = fields.f32_field("AlphaTestThreshold");
            capture.has_opacity = fields.bool_field("HasOpacity");
        }
        "EffectSettingsComponent" => {
            capture.is_glass = fields.bool_field("IsGlass");
            // #5277 — the authored blend-state enum string, the only live
            // CDB blend signal (the alpha side's `Blender` edge has zero
            // top-level instances in the base CDB).
            capture.blending_mode = fields.string_field("BlendingMode");
        }
        "EmissiveSettingsComponent" => {
            capture.emissive_enabled = fields.bool_field("Enabled");
            // #5283 — `Settings` is the nested `EmittanceSettings` struct.
            if let Some(settings) = fields.nested("Settings") {
                capture.emissive_tint = settings
                    .color4_field("EmissiveTint")
                    .map(|c| [c[0], c[1], c[2]]);
                capture.luminous_emittance = settings.f32_field("LuminousEmittance");
            }
        }
        _ => return None,
    }
    Some(capture)
}

impl FieldSource for &crate::ObjectInstance {
    fn bool_field(self, name: &str) -> Option<bool> {
        match self.fields.get(name) {
            Some(crate::Value::Bool(v)) => Some(*v),
            _ => None,
        }
    }

    fn f32_field(self, name: &str) -> Option<f32> {
        match self.fields.get(name) {
            Some(crate::Value::Float(v)) => Some(*v),
            _ => None,
        }
    }

    fn string_field(self, name: &str) -> Option<String> {
        match self.fields.get(name) {
            Some(crate::Value::String(v)) => Some(v.clone()),
            _ => None,
        }
    }

    fn nested(self, name: &str) -> Option<Self> {
        match self.fields.get(name) {
            Some(crate::Value::Object(inner)) => Some(inner),
            _ => None,
        }
    }

    fn color4_field(self, name: &str) -> Option<[f32; 4]> {
        let wrapper = self.nested(name)?;
        let inner = wrapper.nested("Value")?;
        Some([
            inner.f32_field("x")?,
            inner.f32_field("y")?,
            inner.f32_field("z")?,
            inner.f32_field("w")?,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ObjectInstance, Value};
    use std::collections::BTreeMap;

    fn object(class: &str, fields: Vec<(&str, Value)>) -> ObjectInstance {
        ObjectInstance {
            class_name: class.to_string(),
            type_id: 0,
            fields: fields
                .into_iter()
                .map(|(name, value)| (name.to_string(), value))
                .collect::<BTreeMap<_, _>>(),
        }
    }

    fn color(x: f32, y: f32, z: f32, w: f32) -> Value {
        let channels = object(
            "XMFLOAT4",
            vec![
                ("x", Value::Float(x)),
                ("y", Value::Float(y)),
                ("z", Value::Float(z)),
                ("w", Value::Float(w)),
            ],
        );
        Value::Object(object(
            "BSMaterial::Color",
            vec![("Value", Value::Object(channels))],
        ))
    }

    #[test]
    fn effect_alpha_and_emissive_components_are_captured_from_typed_values() {
        let effect = object(
            "BSMaterial::EffectSettingsComponent",
            vec![
                ("IsGlass", Value::Bool(true)),
                ("BlendingMode", Value::String("Additive".into())),
            ],
        );
        let captured = capture_settings("BSMaterial::EffectSettingsComponent", &effect).unwrap();
        assert_eq!(captured.is_glass, Some(true));
        assert_eq!(captured.blending_mode.as_deref(), Some("Additive"));

        let alpha = object(
            "BSMaterial::AlphaSettingsComponent",
            vec![
                ("AlphaTestThreshold", Value::Float(0.5)),
                ("HasOpacity", Value::Bool(false)),
            ],
        );
        let captured = capture_settings("AlphaSettingsComponent", &alpha).unwrap();
        assert_eq!(
            (captured.alpha_test_threshold, captured.has_opacity),
            (Some(0.5), Some(false))
        );

        let emittance = object(
            "BSMaterial::EmittanceSettings",
            vec![
                ("EmissiveTint", color(1.0, 0.8, 0.6, 1.0)),
                ("LuminousEmittance", Value::Float(432.0)),
            ],
        );
        let emissive = object(
            "BSMaterial::EmissiveSettingsComponent",
            vec![
                ("Enabled", Value::Bool(true)),
                ("Settings", Value::Object(emittance)),
            ],
        );
        let captured = capture_settings("BSMaterial::EmissiveSettingsComponent", &emissive).unwrap();
        assert_eq!(captured.emissive_enabled, Some(true));
        assert_eq!(captured.emissive_tint, Some([1.0, 0.8, 0.6]));
        assert_eq!(captured.luminous_emittance, Some(432.0));
    }

    /// A diff authors only what it changes: the real pawnshop emissive
    /// components carry `Settings` with `AdaptiveEmittance` and nothing the
    /// capture reads, and must capture nothing rather than defaults.
    #[test]
    fn an_absent_field_stays_absent() {
        let emittance = object(
            "BSMaterial::EmittanceSettings",
            vec![("AdaptiveEmittance", Value::Bool(true))],
        );
        let emissive = object(
            "BSMaterial::EmissiveSettingsComponent",
            vec![("Settings", Value::Object(emittance))],
        );
        let captured = capture_settings("EmissiveSettingsComponent", &emissive).unwrap();
        assert!(captured.is_empty(), "{captured:?}");
    }

    #[test]
    fn a_tint_missing_a_channel_is_not_completed() {
        let mut tint = object("BSMaterial::Color", vec![]);
        tint.fields.insert(
            "Value".into(),
            Value::Object(object("XMFLOAT4", vec![("x", Value::Float(1.0)), ("y", Value::Float(0.0))])),
        );
        let emittance = object("BSMaterial::EmittanceSettings", vec![("EmissiveTint", Value::Object(tint))]);
        let emissive = object(
            "BSMaterial::EmissiveSettingsComponent",
            vec![("Settings", Value::Object(emittance))],
        );
        let captured = capture_settings("EmissiveSettingsComponent", &emissive).unwrap();
        assert_eq!(captured.emissive_tint, None);
    }

    #[test]
    fn other_classes_are_not_settings_components() {
        let any = object("BSMaterial::MRTextureFile", vec![]);
        assert!(capture_settings("BSMaterial::MRTextureFile", &any).is_none());
        assert!(capture_settings("BSMaterial::TranslucencySettingsComponent", &any).is_none());
    }

    #[test]
    fn fill_unset_is_first_writer_wins() {
        let mut out = crate::CdbMaterial { is_glass: Some(false), ..Default::default() };
        SettingsCapture {
            is_glass: Some(true),
            blending_mode: Some("AlphaBlend".into()),
            ..Default::default()
        }
        .fill_unset(&mut out);
        assert_eq!(out.is_glass, Some(false), "an already-set field is kept");
        assert_eq!(out.blending_mode.as_deref(), Some("AlphaBlend"));
    }
}
