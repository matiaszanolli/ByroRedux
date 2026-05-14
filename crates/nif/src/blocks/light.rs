//! NiLight hierarchy — per-mesh dynamic light sources.
//!
//! Scene graph:
//!
//!   NiDynamicEffect (abstract) → NiLight (abstract) → NiAmbientLight
//!                                                    → NiDirectionalLight
//!                                                    → NiPointLight → NiSpotLight
//!
//! Wire layout (up to and including Skyrim — BSVER < 130):
//!
//!   NiAVObject base
//!   [NiDynamicEffect] switch_state:u8 (since 10.1.0.106)
//!                     num_affected_nodes:u32 (since 10.1.0.0)
//!                     affected_nodes:u32[n] (ptr hashes)
//!   [NiLight]         dimmer:f32
//!                     ambient_color:color3
//!                     diffuse_color:color3
//!                     specular_color:color3
//!   [NiPointLight]    constant_attenuation:f32
//!                     linear_attenuation:f32
//!                     quadratic_attenuation:f32
//!   [NiSpotLight]     outer_spot_angle:f32
//!                     inner_spot_angle:f32 (since 20.2.0.5)
//!                     exponent:f32
//!
//! FO4 (BSVER >= 130) reparents NiLight directly onto NiAVObject and
//! drops the NiDynamicEffect plumbing entirely — both `Switch State`
//! and the `Affected Nodes` list carry `vercond="#NI_BS_LT_FO4#"` in
//! nif.xml line 3499-3504. The base parser below honours that gate via
//! `stream.bsver() < 130`. Pre-#721 the parser unconditionally read the
//! NiDynamicEffect tail on every NIF version >= 10.1.0.x, demoting 681
//! mesh-embedded lights across FO4 / FO76 / Starfield Meshes archives
//! through the outer `block_size`-driven recovery path.

use super::base::NiAVObjectData;
use super::traits::{HasAVObject, HasObjectNET};
use super::NiObject;
use crate::stream::NifStream;
use crate::types::{BlockRef, NiColor, NiTransform};
use crate::version::NifVersion;
use std::any::Any;
use std::io;

/// Shared `NiDynamicEffect + NiLight` base fields, held by every light subtype.
#[derive(Debug, Clone)]
pub struct NiLightBase {
    pub av: NiAVObjectData,
    /// NiDynamicEffect: applied to the scene when true.
    pub switch_state: bool,
    /// NiDynamicEffect: subtree hashes this light affects. We keep them
    /// as raw u32 because Gamebryo stores Ptr-typed fields as hashes.
    pub affected_nodes: Vec<u32>,
    /// NiLight: overall scale on all light contributions (0..1 typical).
    pub dimmer: f32,
    pub ambient_color: NiColor,
    pub diffuse_color: NiColor,
    pub specular_color: NiColor,
}

impl NiLightBase {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let av = NiAVObjectData::parse(stream)?;

        // NiDynamicEffect base — present only when bsver < 130 (FO4
        // reparents NiLight directly onto NiAVObject, dropping both
        // fields per nif.xml `vercond="#NI_BS_LT_FO4#"`). Pre-#721 the
        // parser checked only the NIF version gate, so on FO4 / FO76 /
        // Starfield (NIF 20.2.0.7, bsver >= 130) it consumed 5+ bytes
        // of NiLight color data as `switch_state + affected_nodes`,
        // throwing every mesh-embedded light through `block_size`
        // recovery as NiUnknown.
        let pre_fo4 = stream.bsver() < 130;
        let switch_state = if pre_fo4 && stream.version() >= NifVersion(0x0A01006A) {
            stream.read_u8()? != 0
        } else {
            true
        };

        let affected_nodes = if pre_fo4 && stream.version() >= NifVersion::V10_1_0_0 {
            // #981 — bulk-read affected-nodes u32 array.
            let count = stream.read_u32_le()? as usize;
            stream.read_u32_array(count)?
        } else {
            Vec::new()
        };

        // NiLight scalar fields.
        let dimmer = stream.read_f32_le()?;
        let ambient_color = stream.read_ni_color()?;
        let diffuse_color = stream.read_ni_color()?;
        let specular_color = stream.read_ni_color()?;

        Ok(Self {
            av,
            switch_state,
            affected_nodes,
            dimmer,
            ambient_color,
            diffuse_color,
            specular_color,
        })
    }

    fn name_opt(&self) -> Option<&str> {
        self.av.net.name.as_deref()
    }
    fn name_arc_opt(&self) -> Option<&std::sync::Arc<str>> {
        self.av.net.name.as_ref()
    }
    fn transform_ref(&self) -> &NiTransform {
        &self.av.transform
    }
}

/// Shared boilerplate: implement NiObject/HasObjectNET/HasAVObject for a
/// light type by delegating to its inner `NiLightBase`.
macro_rules! impl_light_bases {
    ($ty:ident, $name:literal, $base:ident) => {
        impl NiObject for $ty {
            fn block_type_name(&self) -> &'static str {
                $name
            }
            fn as_any(&self) -> &dyn Any {
                self
            }
            fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
                Some(self)
            }
            fn as_av_object(&self) -> Option<&dyn HasAVObject> {
                Some(self)
            }
        }
        impl HasObjectNET for $ty {
            fn name(&self) -> Option<&str> {
                self.$base.name_opt()
            }
            fn name_arc(&self) -> Option<&std::sync::Arc<str>> {
                self.$base.name_arc_opt()
            }
            fn extra_data_refs(&self) -> &[BlockRef] {
                &self.$base.av.net.extra_data_refs
            }
            fn controller_ref(&self) -> BlockRef {
                self.$base.av.net.controller_ref
            }
        }
        impl HasAVObject for $ty {
            fn flags(&self) -> u32 {
                self.$base.av.flags
            }
            fn transform(&self) -> &NiTransform {
                self.$base.transform_ref()
            }
            fn properties(&self) -> &[BlockRef] {
                &self.$base.av.properties
            }
            fn collision_ref(&self) -> BlockRef {
                self.$base.av.collision_ref
            }
        }
    };
}

// ── NiAmbientLight ─────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NiAmbientLight {
    pub base: NiLightBase,
}

impl_light_bases!(NiAmbientLight, "NiAmbientLight", base);

impl NiAmbientLight {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Ok(Self {
            base: NiLightBase::parse(stream)?,
        })
    }
}

// ── NiDirectionalLight ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NiDirectionalLight {
    pub base: NiLightBase,
}

impl_light_bases!(NiDirectionalLight, "NiDirectionalLight", base);

impl NiDirectionalLight {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Ok(Self {
            base: NiLightBase::parse(stream)?,
        })
    }
}

// ── NiPointLight ───────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NiPointLight {
    pub base: NiLightBase,
    pub constant_attenuation: f32,
    pub linear_attenuation: f32,
    pub quadratic_attenuation: f32,
}

impl_light_bases!(NiPointLight, "NiPointLight", base);

impl NiPointLight {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiLightBase::parse(stream)?;
        let constant_attenuation = stream.read_f32_le()?;
        let linear_attenuation = stream.read_f32_le()?;
        let quadratic_attenuation = stream.read_f32_le()?;
        Ok(Self {
            base,
            constant_attenuation,
            linear_attenuation,
            quadratic_attenuation,
        })
    }
}

// ── NiSpotLight ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct NiSpotLight {
    /// Inherits NiPointLight (which in turn wraps NiLightBase).
    pub point: NiPointLight,
    /// Outer cone angle in radians.
    pub outer_spot_angle: f32,
    /// Inner cone angle in radians (since 20.2.0.5 — zero for Oblivion).
    pub inner_spot_angle: f32,
    /// Falloff exponent within the cone.
    pub exponent: f32,
}

impl NiObject for NiSpotLight {
    fn block_type_name(&self) -> &'static str {
        "NiSpotLight"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_object_net(&self) -> Option<&dyn HasObjectNET> {
        Some(self)
    }
    fn as_av_object(&self) -> Option<&dyn HasAVObject> {
        Some(self)
    }
}

impl HasObjectNET for NiSpotLight {
    fn name(&self) -> Option<&str> {
        self.point.base.name_opt()
    }
    fn name_arc(&self) -> Option<&std::sync::Arc<str>> {
        self.point.base.name_arc_opt()
    }
    fn extra_data_refs(&self) -> &[BlockRef] {
        &self.point.base.av.net.extra_data_refs
    }
    fn controller_ref(&self) -> BlockRef {
        self.point.base.av.net.controller_ref
    }
}

impl HasAVObject for NiSpotLight {
    fn flags(&self) -> u32 {
        self.point.base.av.flags
    }
    fn transform(&self) -> &NiTransform {
        self.point.base.transform_ref()
    }
    fn properties(&self) -> &[BlockRef] {
        &self.point.base.av.properties
    }
    fn collision_ref(&self) -> BlockRef {
        self.point.base.av.collision_ref
    }
}

impl NiSpotLight {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let point = NiPointLight::parse(stream)?;
        let outer_spot_angle = stream.read_f32_le()?;
        let inner_spot_angle = if stream.version() >= NifVersion(0x14020005) {
            stream.read_f32_le()?
        } else {
            0.0
        };
        let exponent = stream.read_f32_le()?;
        Ok(Self {
            point,
            outer_spot_angle,
            inner_spot_angle,
            exponent,
        })
    }
}
