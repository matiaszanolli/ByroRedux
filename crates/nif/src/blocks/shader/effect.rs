//! `BSEffectShaderProperty` — the Skyrim+ effect/VFX shader.
//!
//! Split out of the single `blocks/shader.rs` under #4339, which had
//! re-crossed 2000 production lines holding four block families behind one
//! shared head. The axis is the block family, not the game: every growth
//! commit touched exactly one family, while a per-game split would have
//! scattered one struct's parsers across five files.

use super::lighting::LuminanceParams;
use super::{is_material_reference, parse_skyrim_shader_base, read_starfield_tail};
use crate::blocks::base::NiObjectNETData;
use crate::blocks::NiObject;
use crate::stream::NifStream;
use std::io;

/// BSEffectShaderProperty — Skyrim+ effect/VFX shader.
///
/// Unlike BSLightingShaderProperty, this shader embeds a source texture
/// filename as a sized string rather than referencing a BSShaderTextureSet.
#[derive(Debug)]
pub struct BSEffectShaderProperty {
    pub net: NiObjectNETData,
    /// True if stopcond short-circuit fired: BSVER >= 155 and Name is a non-empty
    /// BGEM file path. Other fields are at defaults.
    pub material_reference: bool,
    pub shader_flags_1: u32,
    pub shader_flags_2: u32,
    /// CRC32-hashed shader flag list (BSVER >= 132).
    pub sf1_crcs: Vec<u32>,
    /// Second CRC32-hashed shader flag list (BSVER >= 152).
    pub sf2_crcs: Vec<u32>,
    pub uv_offset: [f32; 2],
    pub uv_scale: [f32; 2],
    pub source_texture: String,
    pub texture_clamp_mode: u8,
    pub lighting_influence: u8,
    pub env_map_min_lod: u8,
    pub falloff_start_angle: f32,
    pub falloff_stop_angle: f32,
    pub falloff_start_opacity: f32,
    pub falloff_stop_opacity: f32,
    /// FO76+ refraction power (BSVER >= 155 — fixed in #746 to
    /// also pick up the Starfield 168/172 streams).
    pub refraction_power: f32,
    /// Base color (Color4) — multiplicative diffuse tint applied on
    /// top of the source texture sample. Pre-#166 this was called
    /// `emissive_color`, a holdover from an early nif.xml misread
    /// that conflated BSLightingShader's emissive slot with
    /// BSEffect's base-color slot. Per nif.xml `BSEffectShaderProperty`,
    /// byte offsets align with `emissive_color` — this is a
    /// semantic-name fix only, not a parse layout change.
    /// Downstream consumers in `import/material.rs` and
    /// `import/mesh.rs` still map it into [`MaterialInfo::emissive_color`]
    /// because the effect shader's visible "glow" is driven by
    /// `base_color * base_color_scale` with the current fragment
    /// shader path — a proper diffuse-tint remapping is downstream
    /// work once the effect shader gets its own render path.
    pub base_color: [f32; 4],
    /// Base color scale — scalar multiplier for `base_color`.
    /// Renamed from `emissive_multiple` alongside `base_color` (#166).
    pub base_color_scale: f32,
    pub soft_falloff_depth: f32,
    pub greyscale_texture: String,
    /// Environment map texture path (FO4+ only, BSVER >= 130).
    pub env_map_texture: String,
    /// Normal texture path (FO4+ only, BSVER >= 130).
    pub normal_texture: String,
    /// Environment mask texture path (FO4+ only, BSVER >= 130).
    pub env_mask_texture: String,
    /// Environment map scale (FO4+ only, BSVER >= 130).
    pub env_map_scale: f32,
    /// FO76+ reflectance texture. nif.xml scopes this to BSVER == 155; the parser
    /// reads it at BSVER >= 155 on corpus evidence (#3396).
    pub reflectance_texture: String,
    /// FO76+ lighting texture. nif.xml scopes this to BSVER == 155; the parser
    /// reads it at BSVER >= 155 on corpus evidence (#3396).
    pub lighting_texture: String,
    /// FO76+ emittance color. nif.xml scopes this to BSVER == 155; the parser
    /// reads it at BSVER >= 155 on corpus evidence (#3396).
    pub emittance_color: [f32; 3],
    /// FO76+ emit gradient texture. nif.xml scopes this to BSVER == 155; the parser
    /// reads it at BSVER >= 155 on corpus evidence (#3396).
    pub emit_gradient_texture: String,
    /// FO76+ luminance params. nif.xml scopes this to BSVER == 155; the
    /// parser reads it at BSVER >= 155 on corpus evidence (#3396).
    pub luminance: Option<LuminanceParams>,
    /// #1881 — opaque trailing Starfield bytes between the parser's stop
    /// point and `block_size`, captured via [`read_starfield_tail`] (the
    /// same treatment #1606 gave the `BSLightingShaderProperty` sibling).
    /// Every retail-Starfield `BSEffectShaderProperty` carries a ~32-byte
    /// undocumented tail beyond the FO76 fields; capturing it keeps
    /// `consumed == block_size` instead of leaning on drift recovery.
    /// Empty for the material-reference stub and every non-Starfield variant.
    pub starfield_tail: Vec<u8>,
}

impl BSEffectShaderProperty {
    fn material_reference_stub(net: NiObjectNETData) -> Self {
        Self {
            net,
            material_reference: true,
            shader_flags_1: 0,
            shader_flags_2: 0,
            sf1_crcs: Vec::new(),
            sf2_crcs: Vec::new(),
            uv_offset: [0.0, 0.0],
            uv_scale: [1.0, 1.0],
            source_texture: String::new(),
            texture_clamp_mode: 3,
            lighting_influence: 0,
            env_map_min_lod: 0,
            falloff_start_angle: 1.0,
            falloff_stop_angle: 1.0,
            falloff_start_opacity: 0.0,
            falloff_stop_opacity: 0.0,
            refraction_power: 0.0,
            base_color: [1.0, 1.0, 1.0, 1.0],
            base_color_scale: 1.0,
            soft_falloff_depth: 100.0,
            greyscale_texture: String::new(),
            env_map_texture: String::new(),
            normal_texture: String::new(),
            env_mask_texture: String::new(),
            env_map_scale: 1.0,
            reflectance_texture: String::new(),
            lighting_texture: String::new(),
            emittance_color: [0.0, 0.0, 0.0],
            emit_gradient_texture: String::new(),
            luminance: None,
            starfield_tail: Vec::new(),
        }
    }
}

impl BSEffectShaderProperty {
    /// Parse a `BSEffectShaderProperty` block. Equivalent to
    /// `parse_with_size(stream, None)` — no Starfield tail capture, so any
    /// trailing bytes fall to the outer `block_size` drift recovery as before.
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Self::parse_with_size(stream, None)
    }

    /// As [`parse`](Self::parse), but with the block's declared `block_size`
    /// so the undocumented Starfield trailing tail (#1881) is captured
    /// opaquely up to the block boundary — mirroring #1606 on the
    /// `BSLightingShaderProperty` sibling. The dispatcher passes
    /// `Some(block_size)`; the legacy `parse(stream)` entry passes `None`.
    pub fn parse_with_size(stream: &mut NifStream, block_size: Option<u32>) -> io::Result<Self> {
        let block_start = stream.position();
        let bsver = stream.bsver();
        let mut me = Self::parse_inner(stream, bsver)?;
        me.starfield_tail = read_starfield_tail(stream, block_start, block_size, bsver)?;
        Ok(me)
    }

    fn parse_inner(stream: &mut NifStream, bsver: u32) -> io::Result<Self> {
        let net = NiObjectNETData::parse(stream)?;

        // FO76+ stopcond: Name is an external `.bgem` / `.mat` material-file
        // reference (sibling of the BSLightingShaderProperty gate above).
        // #1510 — Starfield (bsver >= 172) material references are
        // suffix-less content-hash paths (`<hash>\<hash>`) that
        // `is_material_reference` misses; a Starfield full-body block
        // instead carries an EMPTY name, so `!name.is_empty()` is the
        // correct stub discriminator there. FO76 (152..171) keeps the
        // suffix-aware test so editor labels with no path suffix continue
        // through to the full body parse — see #749 / SF-D3-01. This must
        // stay in lockstep with `BSLightingShaderProperty::parse_fo76_plus`.
        if bsver >= crate::version::bsver::FO76 {
            if let Some(name) = net.name.as_deref() {
                let is_ref = if bsver >= crate::version::bsver::STARFIELD {
                    !name.is_empty()
                } else {
                    is_material_reference(name)
                };
                if is_ref {
                    return Ok(Self::material_reference_stub(net));
                }
            }
        }

        // Shared Skyrim+ head — shader flags 1/2, the BSVER >= 132 CRC
        // arrays, then UV offset/scale. `parse_skyrim_shader_base` owns the
        // gap-band gate (`FO4_SHADER_GAP` = 131 carries neither encoding,
        // #409) and the #981 bulk CRC read for every block with this prefix
        // (#3845). BSEffectShaderProperty is the other block type with the
        // real gap band, so `has_gap_band = true` (#4151).
        let (shader_flags_1, shader_flags_2, sf1_crcs, sf2_crcs, uv_offset, uv_scale) =
            parse_skyrim_shader_base(stream, true)?;

        // Source texture as sized string (NOT a texture set reference).
        let source_texture = stream.read_sized_string()?;

        // 4 bytes packed: texture_clamp_mode(u8), lighting_influence(u8),
        // env_map_min_lod(u8), unused(u8).
        let texture_clamp_mode = stream.read_u8()?;
        let lighting_influence = stream.read_u8()?;
        let env_map_min_lod = stream.read_u8()?;
        let _unused = stream.read_u8()?;

        let falloff_start_angle = stream.read_f32_le()?;
        let falloff_stop_angle = stream.read_f32_le()?;
        let falloff_start_opacity = stream.read_f32_le()?;
        let falloff_stop_opacity = stream.read_f32_le()?;

        // FO76+ refraction power. The gate is `>=` on **corpus evidence**,
        // not on nif.xml: pre-#746 the parser used `==` and every Starfield
        // (`bsver = 172`) BSEffect block under-read by 4 B, drifting the rest
        // of the block.
        //
        // #3396 — #746's commit body justified this by claiming nif.xml gates
        // the field on `BSVER #GTE# 155`. That citation is false. nif.xml gates
        // it on `#BS_F76#`, which both copies in the tree define as an
        // equality (`docs/legacy/nif.xml:29`, `/mnt/data/src/reference/nifxml/
        // nif.xml:29`: `<verexpr token="#BS_F76#" string="(#BSVER# #EQ# 155)">`
        // — "Fallout 76 stream 155 only"). Four of the seven sites #746
        // widened on that premise have since been re-narrowed for Starfield on
        // corpus evidence (#1510, #2622); this one and the trailing block below
        // are the two that measurement still supports. Keep the `>=`, but cite
        // the measurement — not nif.xml — as its authority.
        let refraction_power = if bsver >= crate::version::bsver::FO76 {
            stream.read_f32_le()?
        } else {
            0.0
        };

        // Per nif.xml `BSEffectShaderProperty`, these fields are
        // Base Color (Color4) + Base Color Scale (float) — NOT
        // emissive. BSEffect's visible "glow" comes from the base
        // color multiplied by the base-color-scale tint over the
        // source texture. Pre-#166 these were named emissive_* and
        // material.rs folded them into MaterialInfo.emissive_*;
        // byte layout identical so downstream behavior unchanged.
        let base_color = [
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
            stream.read_f32_le()?,
        ];
        let base_color_scale = stream.read_f32_le()?;

        // Soft falloff depth — present in all versions.
        let soft_falloff_depth = stream.read_f32_le()?;

        // Greyscale texture — sized string, present in all versions.
        let greyscale_texture = stream.read_sized_string()?;

        // FO4+ additional textures (BSVER >= 130).
        //
        // #4250 — `env_map_scale`'s not-present-on-Skyrim placeholder was
        // `0.0`, but the field's neutral "no scale authored" value is `1.0`
        // everywhere downstream: `BsEffectShaderData::default()`
        // (`import/material/mod.rs`) and the canonical
        // `Material::env_map_scale` (`core/ecs/components/material.rs`)
        // both declare `1.0`. `dedicated_shader.rs::apply_bs_effect_shader`
        // copies this field straight through to both without
        // reconciliation, so the old `0.0` silently overrode those
        // structs' own declared defaults for every Skyrim
        // BSEffectShaderProperty. `1.0` here matches the neutral
        // multiplier those two structs already agree on.
        let (env_map_texture, normal_texture, env_mask_texture, env_map_scale) =
            if bsver >= crate::version::bsver::FALLOUT4 {
                let env = stream.read_sized_string()?;
                let norm = stream.read_sized_string()?;
                let mask = stream.read_sized_string()?;
                let scale = stream.read_f32_le()?;
                (env, norm, mask, scale)
            } else {
                (String::new(), String::new(), String::new(), 1.0)
            };

        // FO76+ trailing fields. Same value-gate regression as
        // `refraction_power` and the BLSP tail: pre-#746 the parser used `==`
        // and Starfield (`bsver = 172`) BSEffect blocks under-read by ≥40 B +
        // 4 sized strings, so the `>=` is corpus-driven.
        //
        // #3396 — nif.xml does NOT gate these on `#GTE# 155`, contrary to
        // #746's stated premise; `#BS_F76#` is an equality token (see the
        // note on `refraction_power` above). The six fields carrying
        // `vercond="#BS_F76#"` under `<niobject name="BSEffectShaderProperty">`
        // are exactly Refraction Power, Reflectance Texture, Lighting Texture,
        // Emittance Color, Emit Gradient Texture and Luminance — the six read
        // here. Measurement, not the spec, is why they are read past 155.
        let mut reflectance_texture = String::new();
        let mut lighting_texture = String::new();
        let mut emittance_color = [0.0f32; 3];
        let mut emit_gradient_texture = String::new();
        let mut luminance = None;
        if bsver >= crate::version::bsver::FO76 {
            reflectance_texture = stream.read_sized_string()?;
            lighting_texture = stream.read_sized_string()?;
            emittance_color = [
                stream.read_f32_le()?,
                stream.read_f32_le()?,
                stream.read_f32_le()?,
            ];
            emit_gradient_texture = stream.read_sized_string()?;
            luminance = Some(LuminanceParams {
                lum_emittance: stream.read_f32_le()?,
                exposure_offset: stream.read_f32_le()?,
                final_exposure_min: stream.read_f32_le()?,
                final_exposure_max: stream.read_f32_le()?,
            });
        }

        Ok(Self {
            net,
            material_reference: false,
            shader_flags_1,
            shader_flags_2,
            sf1_crcs,
            sf2_crcs,
            uv_offset,
            uv_scale,
            source_texture,
            texture_clamp_mode,
            lighting_influence,
            env_map_min_lod,
            falloff_start_angle,
            falloff_stop_angle,
            falloff_start_opacity,
            falloff_stop_opacity,
            refraction_power,
            base_color,
            base_color_scale,
            soft_falloff_depth,
            greyscale_texture,
            env_map_texture,
            normal_texture,
            env_mask_texture,
            env_map_scale,
            reflectance_texture,
            lighting_texture,
            emittance_color,
            emit_gradient_texture,
            luminance,
            // Set by `parse_with_size` after the body; `parse_inner` leaves it
            // empty (the `None`-block_size path keeps no tail).
            starfield_tail: Vec::new(),
        })
    }
}

impl NiObject for BSEffectShaderProperty {
    fn block_type_name(&self) -> &'static str {
        "BSEffectShaderProperty"
    }
    fn as_any(&self) -> &dyn ::std::any::Any {
        self
    }
    fn opaque_tail_len(&self) -> Option<usize> {
        Some(self.starfield_tail.len())
    }
}
