//! NIF interpolator and keyframe data blocks.
//!
//! These blocks hold the actual animation keyframe data that controllers reference.
//! NiTransformInterpolator → NiTransformData (position/rotation/scale keys)
//! NiFloatInterpolator → NiFloatData (single-channel float keys)

use super::NiObject;
use crate::stream::NifStream;
use crate::types::{BlockRef, NiQuatTransform};
use std::any::Any;
use std::io;
use std::sync::Arc;

// ── Key types ─────────────────────────────────────────────────────────

/// Interpolation type for keyframe data (nif.xml KeyType enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum KeyType {
    Linear = 1,
    Quadratic = 2,
    Tbc = 3,
    XyzRotation = 4,
    /// Step/constant interpolation — value holds until next key. Used by NiBoolData.
    Constant = 5,
}

impl KeyType {
    pub fn from_u32(v: u32) -> io::Result<Self> {
        match v {
            1 => Ok(Self::Linear),
            2 => Ok(Self::Quadratic),
            3 => Ok(Self::Tbc),
            4 => Ok(Self::XyzRotation),
            5 => Ok(Self::Constant),
            _ => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown KeyType: {}", v),
            )),
        }
    }
}

/// A single float keyframe.
#[derive(Debug, Clone, Copy)]
pub struct FloatKey {
    pub time: f32,
    pub value: f32,
    /// Forward/backward tangents (Quadratic) or TBC params.
    pub tangent_forward: f32,
    pub tangent_backward: f32,
    pub tbc: Option<[f32; 3]>, // tension, bias, continuity
}

/// A single Vec3 keyframe.
#[derive(Debug, Clone, Copy)]
pub struct Vec3Key {
    pub time: f32,
    pub value: [f32; 3],
    pub tangent_forward: [f32; 3],
    pub tangent_backward: [f32; 3],
    pub tbc: Option<[f32; 3]>,
}

/// A single quaternion keyframe.
#[derive(Debug, Clone, Copy)]
pub struct QuatKey {
    pub time: f32,
    pub value: [f32; 4], // w, x, y, z
    pub tbc: Option<[f32; 3]>,
}

/// A single RGBA color keyframe. Used by NiColorData / NiColorInterpolator
/// for animated emissive, plasma glow, muzzle-flash fades, etc. (nif.xml
/// `KeyGroup<Color4>`).
#[derive(Debug, Clone, Copy)]
pub struct Color4Key {
    pub time: f32,
    pub value: [f32; 4], // r, g, b, a
    pub tangent_forward: [f32; 4],
    pub tangent_backward: [f32; 4],
    pub tbc: Option<[f32; 3]>,
}

/// A typed group of keys with a shared interpolation type.
#[derive(Debug, Clone)]
pub struct KeyGroup<K> {
    pub key_type: KeyType,
    pub keys: Vec<K>,
}

impl KeyGroup<FloatKey> {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_keys = stream.read_u32_le()?;
        if num_keys == 0 {
            return Ok(Self {
                key_type: KeyType::Linear,
                keys: Vec::new(),
            });
        }
        let key_type = KeyType::from_u32(stream.read_u32_le()?)?;
        let mut keys: Vec<FloatKey> = stream.allocate_vec(num_keys)?;
        for _ in 0..num_keys {
            let time = stream.read_f32_le()?;
            let value = stream.read_f32_le()?;
            let mut tangent_forward = 0.0;
            let mut tangent_backward = 0.0;
            let mut tbc = None;
            match key_type {
                KeyType::Linear | KeyType::Constant => {}
                KeyType::Quadratic => {
                    tangent_forward = stream.read_f32_le()?;
                    tangent_backward = stream.read_f32_le()?;
                }
                KeyType::Tbc => {
                    let t = stream.read_f32_le()?;
                    let b = stream.read_f32_le()?;
                    let c = stream.read_f32_le()?;
                    tbc = Some([t, b, c]);
                }
                KeyType::XyzRotation => {
                    // Not valid for float keys
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "XyzRotation key type in float key group",
                    ));
                }
            }
            keys.push(FloatKey {
                time,
                value,
                tangent_forward,
                tangent_backward,
                tbc,
            });
        }
        Ok(Self { key_type, keys })
    }
}

impl KeyGroup<Vec3Key> {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_keys = stream.read_u32_le()?;
        if num_keys == 0 {
            return Ok(Self {
                key_type: KeyType::Linear,
                keys: Vec::new(),
            });
        }
        let key_type = KeyType::from_u32(stream.read_u32_le()?)?;
        let mut keys: Vec<Vec3Key> = stream.allocate_vec(num_keys)?;
        for _ in 0..num_keys {
            let time = stream.read_f32_le()?;
            let x = stream.read_f32_le()?;
            let y = stream.read_f32_le()?;
            let z = stream.read_f32_le()?;
            let mut tangent_forward = [0.0; 3];
            let mut tangent_backward = [0.0; 3];
            let mut tbc = None;
            match key_type {
                KeyType::Linear | KeyType::Constant => {}
                KeyType::Quadratic => {
                    tangent_forward = [
                        stream.read_f32_le()?,
                        stream.read_f32_le()?,
                        stream.read_f32_le()?,
                    ];
                    tangent_backward = [
                        stream.read_f32_le()?,
                        stream.read_f32_le()?,
                        stream.read_f32_le()?,
                    ];
                }
                KeyType::Tbc => {
                    let t = stream.read_f32_le()?;
                    let b = stream.read_f32_le()?;
                    let c = stream.read_f32_le()?;
                    tbc = Some([t, b, c]);
                }
                KeyType::XyzRotation => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "XyzRotation key type in vec3 key group",
                    ));
                }
            }
            keys.push(Vec3Key {
                time,
                value: [x, y, z],
                tangent_forward,
                tangent_backward,
                tbc,
            });
        }
        Ok(Self { key_type, keys })
    }
}

impl KeyGroup<Color4Key> {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_keys = stream.read_u32_le()?;
        if num_keys == 0 {
            return Ok(Self {
                key_type: KeyType::Linear,
                keys: Vec::new(),
            });
        }
        let key_type = KeyType::from_u32(stream.read_u32_le()?)?;
        let mut keys: Vec<Color4Key> = stream.allocate_vec(num_keys)?;
        for _ in 0..num_keys {
            let time = stream.read_f32_le()?;
            let r = stream.read_f32_le()?;
            let g = stream.read_f32_le()?;
            let b = stream.read_f32_le()?;
            let a = stream.read_f32_le()?;
            let mut tangent_forward = [0.0; 4];
            let mut tangent_backward = [0.0; 4];
            let mut tbc = None;
            match key_type {
                KeyType::Linear | KeyType::Constant => {}
                KeyType::Quadratic => {
                    for slot in tangent_forward.iter_mut() {
                        *slot = stream.read_f32_le()?;
                    }
                    for slot in tangent_backward.iter_mut() {
                        *slot = stream.read_f32_le()?;
                    }
                }
                KeyType::Tbc => {
                    let t = stream.read_f32_le()?;
                    let b = stream.read_f32_le()?;
                    let c = stream.read_f32_le()?;
                    tbc = Some([t, b, c]);
                }
                KeyType::XyzRotation => {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "XyzRotation key type in color key group",
                    ));
                }
            }
            keys.push(Color4Key {
                time,
                value: [r, g, b, a],
                tangent_forward,
                tangent_backward,
                tbc,
            });
        }
        Ok(Self { key_type, keys })
    }
}

// ── NiTransformData (aka NiKeyframeData) ──────────────────────────────

/// Keyframe data for a transform animation: rotation, translation, and scale keys.
#[derive(Debug)]
pub struct NiTransformData {
    /// Quaternion rotation keys (Linear or TBC).
    pub rotation_type: Option<KeyType>,
    pub rotation_keys: Vec<QuatKey>,
    /// If rotation_type == XyzRotation, three separate float key groups for X, Y, Z euler angles.
    pub xyz_rotations: Option<[KeyGroup<FloatKey>; 3]>,
    /// Translation keys.
    pub translations: KeyGroup<Vec3Key>,
    /// Scale keys.
    pub scales: KeyGroup<FloatKey>,
}

impl NiObject for NiTransformData {
    fn block_type_name(&self) -> &'static str {
        "NiTransformData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTransformData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // Rotation keys
        let num_rotation_keys = stream.read_u32_le()?;
        let mut rotation_type = None;
        let mut rotation_keys = Vec::new();
        let mut xyz_rotations = None;

        if num_rotation_keys > 0 {
            let rt = KeyType::from_u32(stream.read_u32_le()?)?;
            rotation_type = Some(rt);

            if rt == KeyType::XyzRotation {
                // nif.xml: `Order` float present between RotationType and XYZ KeyGroups on
                // pre-Gamebryo NIFs (field cond="Rotation Type == 4" until="10.1.0.0").
                // `until=` is inclusive per the version.rs doctrine — present at
                // v <= 10.1.0.0.
                if stream.version() <= crate::version::NifVersion::V10_1_0_0 {
                    let _order = stream.read_f32_le()?;
                }
                // XYZ rotation: no quaternion keys, three float key groups instead
                let x_keys = KeyGroup::<FloatKey>::parse(stream)?;
                let y_keys = KeyGroup::<FloatKey>::parse(stream)?;
                let z_keys = KeyGroup::<FloatKey>::parse(stream)?;
                xyz_rotations = Some([x_keys, y_keys, z_keys]);
            } else {
                // Quaternion keys. Counts go through allocate_vec so a
                // corrupt 0xFFFFFFFF can't OOM before the inner reads fail.
                // See #764.
                rotation_keys = stream.allocate_vec::<QuatKey>(num_rotation_keys)?;
                for _ in 0..num_rotation_keys {
                    let time = stream.read_f32_le()?;
                    let w = stream.read_f32_le()?;
                    let x = stream.read_f32_le()?;
                    let y = stream.read_f32_le()?;
                    let z = stream.read_f32_le()?;
                    let tbc = if rt == KeyType::Tbc {
                        let t = stream.read_f32_le()?;
                        let b = stream.read_f32_le()?;
                        let c = stream.read_f32_le()?;
                        Some([t, b, c])
                    } else {
                        None
                    };
                    rotation_keys.push(QuatKey {
                        time,
                        value: [w, x, y, z],
                        tbc,
                    });
                }
            }
        }

        // Translation keys
        let translations = KeyGroup::<Vec3Key>::parse(stream)?;
        // Scale keys
        let scales = KeyGroup::<FloatKey>::parse(stream)?;

        Ok(Self {
            rotation_type,
            rotation_keys,
            xyz_rotations,
            translations,
            scales,
        })
    }
}

// ── NiTransformInterpolator ───────────────────────────────────────────

/// Interpolates a full transform (translation + rotation + scale).
/// References NiTransformData for the actual keyframes.
#[derive(Debug)]
pub struct NiTransformInterpolator {
    pub transform: NiQuatTransform,
    pub data_ref: BlockRef,
}

impl NiObject for NiTransformInterpolator {
    fn block_type_name(&self) -> &'static str {
        "NiTransformInterpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTransformInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let transform = stream.read_ni_quat_transform()?;
        let data_ref = stream.read_block_ref()?;
        Ok(Self {
            transform,
            data_ref,
        })
    }
}

// ── NiLookAtInterpolator ───────────────────────────────────────────────
//
// Rotates its parent so a chosen axis tracks a target NiNode. nif.xml
// line 4352. Replaces the deprecated `NiLookAtController` from 10.2
// onwards; Bethesda content from FO3+ uses this driving a plain
// `NiTransformController`. Pre-#NEW the dispatch table had no entry
// and 18 instances per FNV mesh sweep landed in NiUnknown — surfaced
// by the R3 per-block histogram.
//
// Wire layout (since 10.x; `Transform` field present `until="20.4.0.12"`,
// which covers every game we target — Oblivion 20.0.0.5, FO3/FNV/SkyrimLE
// 20.2.0.7, Skyrim SE/FO4/+ 20.2.0.7 — they're all <= 20.4.0.12):
//
//   Flags                  (LookAtFlags = u16, 2 B)
//   Look At                (Ptr → NiNode, 4 B)
//   Look At Name           (string — table index since 20.1.0.1)
//   Transform              (NiQuatTransform, 32 B until 20.4.0.12)
//   Interpolator: Trans    (Ref → NiPoint3Interpolator, 4 B)
//   Interpolator: Roll     (Ref → NiFloatInterpolator, 4 B)
//   Interpolator: Scale    (Ref → NiFloatInterpolator, 4 B)
//
// The TRS sub-interpolators each animate one channel of the local
// orientation that gets composed against the look-at solve at runtime;
// we parse them as plain refs so they land in `NifScene.blocks` for a
// later constraint-evaluation pass.

/// `LookAtFlags` — nif.xml line 4339. Three-bit ushort. Stored as the
/// raw u16 with named bit constants below so the parser does not pull
/// in the `bitflags` crate (none of the rest of the nif crate uses it).
pub mod look_at_flags {
    /// Flip the chosen axis (180° around the look direction).
    pub const LOOK_FLIP: u16 = 0x0001;
    /// Track with Y as the up axis instead of the default X.
    pub const LOOK_Y_AXIS: u16 = 0x0002;
    /// Track with Z as the up axis instead of the default X.
    pub const LOOK_Z_AXIS: u16 = 0x0004;
}

#[derive(Debug)]
pub struct NiLookAtInterpolator {
    /// Raw flag bits. Test against constants in [`look_at_flags`].
    pub flags: u16,
    pub look_at: BlockRef,
    pub look_at_name: Option<Arc<str>>,
    /// Pose transform — the static fall-back when the three sub-
    /// interpolators are null. Present on every game we target
    /// (`until="20.4.0.12"`).
    pub transform: NiQuatTransform,
    pub interp_translation: BlockRef,
    pub interp_roll: BlockRef,
    pub interp_scale: BlockRef,
}

impl NiObject for NiLookAtInterpolator {
    fn block_type_name(&self) -> &'static str {
        "NiLookAtInterpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiLookAtInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let flags = stream.read_u16_le()?;
        let look_at = stream.read_block_ref()?;
        let look_at_name = stream.read_string()?;
        let transform = stream.read_ni_quat_transform()?;
        let interp_translation = stream.read_block_ref()?;
        let interp_roll = stream.read_block_ref()?;
        let interp_scale = stream.read_block_ref()?;
        Ok(Self {
            flags,
            look_at,
            look_at_name,
            transform,
            interp_translation,
            interp_roll,
            interp_scale,
        })
    }
}

// ── NiFloatInterpolator ───────────────────────────────────────────────

/// Interpolates a single float value. References NiFloatData.
#[derive(Debug)]
pub struct NiFloatInterpolator {
    pub value: f32,
    pub data_ref: BlockRef,
}

impl NiObject for NiFloatInterpolator {
    fn block_type_name(&self) -> &'static str {
        "NiFloatInterpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiFloatInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let value = stream.read_f32_le()?;
        let data_ref = stream.read_block_ref()?;
        Ok(Self { value, data_ref })
    }
}

// ── NiFloatData ───────────────────────────────────────────────────────

/// A single channel of float keyframes.
#[derive(Debug)]
pub struct NiFloatData {
    pub keys: KeyGroup<FloatKey>,
}

impl NiObject for NiFloatData {
    fn block_type_name(&self) -> &'static str {
        "NiFloatData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiFloatData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let keys = KeyGroup::<FloatKey>::parse(stream)?;
        Ok(Self { keys })
    }
}

// ── NiPoint3Interpolator ─────────────────────────────────────────────

/// Interpolates a Vec3 value. References NiPosData.
#[derive(Debug)]
pub struct NiPoint3Interpolator {
    pub value: [f32; 3],
    pub data_ref: BlockRef,
}

impl NiObject for NiPoint3Interpolator {
    fn block_type_name(&self) -> &'static str {
        "NiPoint3Interpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiPoint3Interpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let x = stream.read_f32_le()?;
        let y = stream.read_f32_le()?;
        let z = stream.read_f32_le()?;
        let data_ref = stream.read_block_ref()?;
        Ok(Self {
            value: [x, y, z],
            data_ref,
        })
    }
}

// ── NiPosData ─────────────────────────────────────────────────────────

/// Vec3 keyframe data (used by NiPoint3Interpolator).
#[derive(Debug)]
pub struct NiPosData {
    pub keys: KeyGroup<Vec3Key>,
}

impl NiObject for NiPosData {
    fn block_type_name(&self) -> &'static str {
        "NiPosData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiPosData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let keys = KeyGroup::<Vec3Key>::parse(stream)?;
        Ok(Self { keys })
    }
}

// ── NiPathInterpolator ────────────────────────────────────────────────
//
// Spline-path interpolator for animated motion along a curve —
// `NiKeyBasedInterpolator` → `NiInterpolator` → `NiObject`. Door
// hinges, pendulum swings, wind-turbine blades, hot-air-balloon drift
// paths and similar environmental motion. nif.xml line 3270. Oblivion
// vanilla content ships a handful of these; without a parser the
// `block_sizes`-less Oblivion loader can't skip past the block and the
// rest of the NIF is truncated. See audit OBL-D5-H2 / #394.
//
// Wire layout (24 bytes, version-independent for Oblivion-target
// parsing):
//   flags(u16) + bank_dir(i32) + max_bank_angle(f32) + smoothing(f32)
//   + follow_axis(i16) + path_data(Ref) + percent_data(Ref)
// = 2 + 4 + 4 + 4 + 2 + 4 + 4 = 24 B

/// `NiPathInterpolator` — spline-path motion driver. See module doc
/// above.
#[derive(Debug)]
pub struct NiPathInterpolator {
    /// `PathFlags` bits: `CVDataNeedsUpdate` (bit 0), `CurveTypeOpen`
    /// (bit 1), and Bethesda extensions. Preserved verbatim.
    pub flags: u16,
    /// Direction of banking: `-1` = negative, `1` = positive.
    pub bank_dir: i32,
    /// Maximum bank angle in radians.
    pub max_bank_angle: f32,
    pub smoothing: f32,
    /// Axis the object aims along when following the path: 0=X, 1=Y,
    /// 2=Z. Out-of-range values are preserved untouched for the
    /// consumer to filter.
    pub follow_axis: i16,
    /// Reference to an `NiPosData` block carrying the spline control
    /// points.
    pub path_data_ref: BlockRef,
    /// Reference to an `NiFloatData` block carrying
    /// parametric-position-along-the-path keys.
    pub percent_data_ref: BlockRef,
}

impl NiObject for NiPathInterpolator {
    fn block_type_name(&self) -> &'static str {
        "NiPathInterpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiPathInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let flags = stream.read_u16_le()?;
        let bank_dir = stream.read_i32_le()?;
        let max_bank_angle = stream.read_f32_le()?;
        let smoothing = stream.read_f32_le()?;
        // No `read_i16_le` on NifStream today — follow_axis is 0/1/2
        // for X/Y/Z so the sign never matters; cast the u16 over.
        let follow_axis = stream.read_u16_le()? as i16;
        let path_data_ref = stream.read_block_ref()?;
        let percent_data_ref = stream.read_block_ref()?;
        Ok(Self {
            flags,
            bank_dir,
            max_bank_angle,
            smoothing,
            follow_axis,
            path_data_ref,
            percent_data_ref,
        })
    }
}

// ── NiColorInterpolator ───────────────────────────────────────────────

/// Interpolates an RGBA color value. References NiColorData.
///
/// Paired with BSEffectShaderPropertyColorController /
/// BSLightingShaderPropertyColorController (and historical
/// NiMaterialColorController targets). Pre-#431 this block landed as
/// NiUnknown because it wasn't in the dispatch table; downstream
/// animation extraction silently ran with a default color on every
/// animated emissive / plasma / muzzle-flash controller.
#[derive(Debug)]
pub struct NiColorInterpolator {
    /// Pose value used when `data_ref` doesn't resolve.
    pub value: [f32; 4],
    pub data_ref: BlockRef,
}

impl NiObject for NiColorInterpolator {
    fn block_type_name(&self) -> &'static str {
        "NiColorInterpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiColorInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let r = stream.read_f32_le()?;
        let g = stream.read_f32_le()?;
        let b = stream.read_f32_le()?;
        let a = stream.read_f32_le()?;
        let data_ref = stream.read_block_ref()?;
        Ok(Self {
            value: [r, g, b, a],
            data_ref,
        })
    }
}

// ── NiColorData ───────────────────────────────────────────────────────

/// RGBA keyframe data. Wrapped `KeyGroup<Color4>` — same layout shape as
/// `NiPosData` but four components instead of three.
#[derive(Debug)]
pub struct NiColorData {
    pub keys: KeyGroup<Color4Key>,
}

impl NiObject for NiColorData {
    fn block_type_name(&self) -> &'static str {
        "NiColorData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiColorData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let keys = KeyGroup::<Color4Key>::parse(stream)?;
        Ok(Self { keys })
    }
}

// ── NiTextKeyExtraData ────────────────────────────────────────────────

/// Text keys embedded in animation files — event markers like "start", "end", "hit".
#[derive(Debug)]
pub struct NiTextKeyExtraData {
    pub name: Option<Arc<str>>,
    pub text_keys: Vec<(f32, String)>,
}

impl NiObject for NiTextKeyExtraData {
    fn block_type_name(&self) -> &'static str {
        "NiTextKeyExtraData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiTextKeyExtraData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // #723 / NIF-D1-05 — pre-Gamebryo (v <= 4.2.2.0) NiExtraData
        // base format: `Next Extra Data: Ref` (linked-list head) +
        // `Num Bytes: uint` (since 4.0.0.0, until 4.2.2.0). Both `until=`
        // gates are inclusive per the version.rs doctrine — present at
        // v <= 4.2.2.0. The gap window (4.2.2.0 < v < 10.0.1.0) carries
        // neither prefix nor a Name field; v >= 10.0.1.0 inherits
        // NiObjectNET's Name.
        //
        // Pre-fix `read_string()` on a pre-Gamebryo NIF consumed the
        // `Next Extra Data` ref bytes as if they were a length-
        // prefixed inline string (read_string's pre-20.1.0.1 path),
        // then `num_text_keys` read consumed the `Num Bytes` payload
        // as if it were the key count. Cosmetic on shipped Bethesda
        // content (none in the pre-Gamebryo band); guards
        // pre-Gamebryo NetImmerse / Morrowind-era kf compat.
        let v = stream.version();
        if v <= crate::version::NifVersion(0x04020200) {
            let _next_extra_data_ref = stream.read_block_ref()?;
            if v >= crate::version::NifVersion(0x04000000) {
                let _num_bytes = stream.read_u32_le()?;
            }
        }
        // NiObjectNET::name (only since 10.0.1.0; pre-Gamebryo and gap
        // window have no name field).
        let name = if v >= crate::version::NifVersion(0x0A000100) {
            stream.read_string()?
        } else {
            None
        };
        // num_text_keys
        let num_text_keys = stream.read_u32_le()?;
        // #388: allocate_vec gates `count * size_of::<(f32, String)>` against
        // the stream budget before any capacity is reserved — a corrupt
        // u32 that used to OOM the process (135 GB abort on Oblivion
        // `upperclassdisplaycaseblue01.nif`) now returns `Err` cleanly.
        let mut text_keys: Vec<(f32, String)> = stream.allocate_vec(num_text_keys)?;
        for _ in 0..num_text_keys {
            let time = stream.read_f32_le()?;
            let text = stream
                .read_string()?
                .map(|s| s.to_string())
                .unwrap_or_default();
            text_keys.push((time, text));
        }
        Ok(Self { name, text_keys })
    }
}

// ── NiBoolInterpolator ────────────────────────────────────────────────

/// Discriminator for the two wire types that share the
/// [`NiBoolInterpolator`] Rust struct. `NiBoolTimelineInterpolator` has
/// the identical serialized layout (nif.xml line 3287 — no additional
/// fields beyond `NiBoolInterpolator`), but its gameplay semantics differ:
/// timelines guarantee no key is missed between two updates, so the
/// importer needs the wire-type tag to wire the right play-head policy.
/// Mirrors the [`super::tri_shape::BsTriShapeKind`] pattern — #548, #560.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoolInterpolatorKind {
    /// Plain `NiBoolInterpolator` — sample the last passed key.
    Plain,
    /// `NiBoolTimelineInterpolator` — second-most-common NiUnknown on
    /// Skyrim SE (6,796 blocks in vanilla). Wire layout identical to
    /// `NiBoolInterpolator`; the discriminator distinguishes the
    /// "don't miss a key between updates" semantics. #548.
    Timeline,
}

/// Interpolates a boolean value (visibility). References NiBoolData.
#[derive(Debug)]
pub struct NiBoolInterpolator {
    pub value: bool,
    pub data_ref: BlockRef,
    /// Wire-type discriminator. `NiBoolTimelineInterpolator` (nif.xml
    /// line 3287) shares this struct; the `kind` field is what lets
    /// downstream importers distinguish the two. See #548.
    pub kind: BoolInterpolatorKind,
}

impl NiObject for NiBoolInterpolator {
    fn block_type_name(&self) -> &'static str {
        // Static-string contract — dispatch on the wire discriminator
        // so downstream `block_type_name()` callers see the original
        // subclass. Consumers that need the timeline semantics should
        // match on `self.kind` instead.
        match self.kind {
            BoolInterpolatorKind::Plain => "NiBoolInterpolator",
            BoolInterpolatorKind::Timeline => "NiBoolTimelineInterpolator",
        }
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBoolInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Self::parse_with_kind(stream, BoolInterpolatorKind::Plain)
    }

    /// Parse `NiBoolTimelineInterpolator`. Per nif.xml line 3287 the
    /// subclass adds no fields over `NiBoolInterpolator`; the dispatch
    /// simply stamps the Timeline discriminator. 8,450 blocks across
    /// FO3 + FNV + Skyrim SE fell into `NiUnknown` pre-#548 because
    /// no dispatch arm existed.
    pub fn parse_timeline(stream: &mut NifStream) -> io::Result<Self> {
        Self::parse_with_kind(stream, BoolInterpolatorKind::Timeline)
    }

    fn parse_with_kind(stream: &mut NifStream, kind: BoolInterpolatorKind) -> io::Result<Self> {
        // nif.xml: NiBoolInterpolator.bool_value is type "bool" (1 byte),
        // NOT "NiBool" (version-dependent u32/u8). Always a single byte.
        let value = stream.read_byte_bool()?;
        let data_ref = stream.read_block_ref()?;
        Ok(Self {
            value,
            data_ref,
            kind,
        })
    }
}

// ── NiBoolData ────────────────────────────────────────────────────────

/// Boolean keyframe data (on/off visibility keys).
/// Stored as byte keys (0/1) with the same KeyGroup format as float keys.
#[derive(Debug)]
pub struct NiBoolData {
    pub keys: KeyGroup<FloatKey>,
}

impl NiObject for NiBoolData {
    fn block_type_name(&self) -> &'static str {
        "NiBoolData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBoolData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // Bool data uses byte keys (KeyType is always 1=Linear for bool).
        // Parsed the same as float key group but values are 0.0/1.0.
        let num_keys = stream.read_u32_le()?;
        if num_keys == 0 {
            return Ok(Self {
                keys: KeyGroup {
                    key_type: KeyType::Linear,
                    keys: Vec::new(),
                },
            });
        }
        let key_type = KeyType::from_u32(stream.read_u32_le()?)?;
        let mut keys: Vec<FloatKey> = stream.allocate_vec(num_keys)?;
        for _ in 0..num_keys {
            let time = stream.read_f32_le()?;
            // Bool keys store the value as a u8 (byte)
            let value = stream.read_u8()? as f32;
            keys.push(FloatKey {
                time,
                value,
                tangent_forward: 0.0,
                tangent_backward: 0.0,
                tbc: None,
            });
        }
        Ok(Self {
            keys: KeyGroup { key_type, keys },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::NifHeader;
    use crate::stream::NifStream;
    use crate::version::NifVersion;

    fn make_header_fnv() -> NifHeader {
        NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 11,
            user_version_2: 34,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: vec![Arc::from("TestName"), Arc::from("start")],
            max_string_length: 8,
            num_groups: 0,
        }
    }

    #[test]
    fn parse_transform_interpolator() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        // NiQuatTransform: translation (1, 2, 3)
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes());
        data.extend_from_slice(&3.0f32.to_le_bytes());
        // rotation: identity quat (w=1, x=0, y=0, z=0)
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        // scale: 1.0
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // data_ref: 5
        data.extend_from_slice(&5i32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let interp = NiTransformInterpolator::parse(&mut stream).unwrap();
        assert_eq!(interp.transform.translation.x, 1.0);
        assert_eq!(interp.transform.translation.y, 2.0);
        assert_eq!(interp.transform.translation.z, 3.0);
        assert_eq!(interp.transform.rotation, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(interp.transform.scale, 1.0);
        assert_eq!(interp.data_ref.index(), Some(5));
        // 3 + 4 + 1 = 8 floats (32 bytes) + 4 byte ref = 36 bytes
        assert_eq!(stream.position(), 36);
    }

    #[test]
    fn parse_transform_data_linear_rotation() {
        let header = make_header_fnv();
        let mut data = Vec::new();

        // 2 rotation keys, type=Linear(1)
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes()); // Linear

        // Key 0: time=0.0, quat=(1,0,0,0)
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());

        // Key 1: time=1.0, quat=(0,0,1,0)
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());

        // 0 translation keys
        data.extend_from_slice(&0u32.to_le_bytes());
        // 0 scale keys
        data.extend_from_slice(&0u32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let td = NiTransformData::parse(&mut stream).unwrap();

        assert_eq!(td.rotation_type, Some(KeyType::Linear));
        assert_eq!(td.rotation_keys.len(), 2);
        assert_eq!(td.rotation_keys[0].time, 0.0);
        assert_eq!(td.rotation_keys[0].value, [1.0, 0.0, 0.0, 0.0]);
        assert_eq!(td.rotation_keys[1].time, 1.0);
        assert_eq!(td.rotation_keys[1].value, [0.0, 0.0, 1.0, 0.0]);
        assert!(td.xyz_rotations.is_none());
        assert!(td.translations.keys.is_empty());
        assert!(td.scales.keys.is_empty());
    }

    #[test]
    fn parse_transform_data_with_translation_keys() {
        let header = make_header_fnv();
        let mut data = Vec::new();

        // 0 rotation keys
        data.extend_from_slice(&0u32.to_le_bytes());

        // 2 translation keys, type=Linear(1)
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes()); // Linear

        // Key 0: time=0.0, pos=(0,0,0)
        data.extend_from_slice(&0.0f32.to_le_bytes());
        for _ in 0..3 {
            data.extend_from_slice(&0.0f32.to_le_bytes());
        }

        // Key 1: time=1.0, pos=(10,20,30)
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&10.0f32.to_le_bytes());
        data.extend_from_slice(&20.0f32.to_le_bytes());
        data.extend_from_slice(&30.0f32.to_le_bytes());

        // 0 scale keys
        data.extend_from_slice(&0u32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let td = NiTransformData::parse(&mut stream).unwrap();

        assert!(td.rotation_keys.is_empty());
        assert_eq!(td.translations.keys.len(), 2);
        assert_eq!(td.translations.key_type, KeyType::Linear);
        assert_eq!(td.translations.keys[1].value, [10.0, 20.0, 30.0]);
    }

    #[test]
    fn parse_float_interpolator() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        data.extend_from_slice(&42.0f32.to_le_bytes()); // value
        data.extend_from_slice(&7i32.to_le_bytes()); // data_ref

        let mut stream = NifStream::new(&data, &header);
        let fi = NiFloatInterpolator::parse(&mut stream).unwrap();
        assert_eq!(fi.value, 42.0);
        assert_eq!(fi.data_ref.index(), Some(7));
    }

    #[test]
    fn parse_float_data_linear() {
        let header = make_header_fnv();
        let mut data = Vec::new();

        // 2 keys, Linear
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes()); // Linear

        data.extend_from_slice(&0.0f32.to_le_bytes()); // time
        data.extend_from_slice(&0.0f32.to_le_bytes()); // value

        data.extend_from_slice(&1.0f32.to_le_bytes()); // time
        data.extend_from_slice(&1.0f32.to_le_bytes()); // value

        let mut stream = NifStream::new(&data, &header);
        let fd = NiFloatData::parse(&mut stream).unwrap();
        assert_eq!(fd.keys.keys.len(), 2);
        assert_eq!(fd.keys.key_type, KeyType::Linear);
    }

    #[test]
    fn parse_text_key_extra_data() {
        let header = make_header_fnv();
        let mut data = Vec::new();

        // name: string table index 0 = "TestName"
        data.extend_from_slice(&0i32.to_le_bytes());
        // num_text_keys: 1
        data.extend_from_slice(&1u32.to_le_bytes());
        // key 0: time=0.0, text=string table index 1 = "start"
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1i32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let tk = NiTextKeyExtraData::parse(&mut stream).unwrap();
        assert_eq!(tk.name.as_deref(), Some("TestName"));
        assert_eq!(tk.text_keys.len(), 1);
        assert_eq!(tk.text_keys[0].0, 0.0);
        assert_eq!(tk.text_keys[0].1, "start");
    }

    #[test]
    fn parse_text_key_extra_data_rejects_malicious_count() {
        // Regression #388 / OBL-D5-C1: a corrupt/drifted u32 used to
        // OOM the process via `Vec::with_capacity(num_text_keys as usize)`.
        // The reproducer was Oblivion's `upperclassdisplaycaseblue01.nif`
        // — a 218 KB file claimed 4.24 G text keys, prompting a
        // 135 GB allocation that aborted the process. The new
        // `allocate_vec` bound rejects any count larger than the bytes
        // remaining in the stream.
        let header = make_header_fnv();
        let mut data = Vec::new();
        // name: empty string-table index
        data.extend_from_slice(&(-1i32).to_le_bytes());
        // num_text_keys = u32::MAX → must be rejected, not OOM
        data.extend_from_slice(&u32::MAX.to_le_bytes());
        // No payload follows; if the bound check were missing, parse
        // would call `Vec::with_capacity(u32::MAX as usize)` and
        // potentially abort the process before any read fails.

        let mut stream = NifStream::new(&data, &header);
        let result = NiTextKeyExtraData::parse(&mut stream);
        assert!(result.is_err(), "expected Err on malicious count");
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("only") && msg.contains("bytes remain"),
            "expected allocate_vec budget message, got: {msg}"
        );
    }

    #[test]
    fn parse_transform_data_empty() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        // 0 rotation, 0 translation, 0 scale
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let td = NiTransformData::parse(&mut stream).unwrap();
        assert!(td.rotation_keys.is_empty());
        assert!(td.translations.keys.is_empty());
        assert!(td.scales.keys.is_empty());
        assert_eq!(stream.position(), 12);
    }

    /// Regression: #436 — XYZ_ROTATION_KEY mode stores euler angles
    /// across three independent `KeyGroup<float>` blocks (one per axis).
    /// Per nif.xml `NiKeyframeData`, `Num Rotation Keys` MUST be 1 when
    /// rotation_type==4; the actual per-axis key counts live in the
    /// three KeyGroups. The audit claimed the parser read only the X
    /// channel and left Y/Z bytes in the stream — a stale observation;
    /// `interpolator.rs:224-229` already reads all three. This test
    /// pins that behavior so a future rewrite can't regress to the
    /// imagined bug without failing loudly.
    /// Regression for #431 — the canonical color-animation chain
    /// (`NiColorInterpolator` → `NiColorData`) must parse end-to-end
    /// instead of landing as `NiUnknown`. Covers the value default
    /// field, the data_ref, and two linear RGBA keys with distinct
    /// alpha so a future regression that drops the 4th component
    /// fails loudly.
    #[test]
    fn parse_color_interpolator_and_color_data() {
        let header = make_header_fnv();

        // NiColorInterpolator: value (r, g, b, a) + data_ref.
        let mut data = Vec::new();
        data.extend_from_slice(&0.5f32.to_le_bytes());
        data.extend_from_slice(&0.25f32.to_le_bytes());
        data.extend_from_slice(&0.125f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&9i32.to_le_bytes()); // data_ref

        let mut stream = NifStream::new(&data, &header);
        let ci = NiColorInterpolator::parse(&mut stream).unwrap();
        assert_eq!(ci.value, [0.5, 0.25, 0.125, 1.0]);
        assert_eq!(ci.data_ref.index(), Some(9));
        assert_eq!(stream.position(), 20);

        // NiColorData with 2 Linear RGBA keys (fade from opaque red →
        // half-alpha blue across one second).
        let mut data = Vec::new();
        data.extend_from_slice(&2u32.to_le_bytes()); // num keys
        data.extend_from_slice(&1u32.to_le_bytes()); // Linear
                                                     // Key 0: t=0, (1, 0, 0, 1)
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // Key 1: t=1, (0, 0, 1, 0.5)
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());

        let mut stream = NifStream::new(&data, &header);
        let cd = NiColorData::parse(&mut stream).unwrap();
        assert_eq!(cd.keys.key_type, KeyType::Linear);
        assert_eq!(cd.keys.keys.len(), 2);
        assert_eq!(cd.keys.keys[0].value, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(cd.keys.keys[1].value, [0.0, 0.0, 1.0, 0.5]);
        assert_eq!(cd.keys.keys[1].time, 1.0);
    }

    #[test]
    fn parse_color_data_empty_keygroup() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        data.extend_from_slice(&0u32.to_le_bytes()); // zero keys → no key_type follows
        let mut stream = NifStream::new(&data, &header);
        let cd = NiColorData::parse(&mut stream).unwrap();
        assert!(cd.keys.keys.is_empty());
        assert_eq!(stream.position(), 4);
    }

    #[test]
    fn parse_transform_data_xyz_rotation_reads_all_three_axes() {
        let header = make_header_fnv();
        let mut data = Vec::new();

        // Num Rotation Keys = 1 (spec requires this for XYZ mode).
        data.extend_from_slice(&1u32.to_le_bytes());
        // Rotation Type = XyzRotation (4).
        data.extend_from_slice(&4u32.to_le_bytes());

        // KeyGroup X: 2 Linear keys (time, value).
        //   num_keys, interpolation_type, then (time, value) pairs.
        data.extend_from_slice(&2u32.to_le_bytes()); // num keys
        data.extend_from_slice(&1u32.to_le_bytes()); // Linear
        data.extend_from_slice(&0.0f32.to_le_bytes()); // time
        data.extend_from_slice(&0.1f32.to_le_bytes()); // value
        data.extend_from_slice(&1.0f32.to_le_bytes()); // time
        data.extend_from_slice(&0.2f32.to_le_bytes()); // value

        // KeyGroup Y: 3 Linear keys — different count than X to verify
        // the parser doesn't apply the X count to Y.
        data.extend_from_slice(&3u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes()); // Linear
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        data.extend_from_slice(&1.5f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes());

        // KeyGroup Z: 1 Linear key — smallest to prove Y's larger
        // count didn't over-consume Z's header.
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes()); // Linear
        data.extend_from_slice(&0.5f32.to_le_bytes());
        data.extend_from_slice(&3.0f32.to_le_bytes());

        // Translations: 0 keys.
        data.extend_from_slice(&0u32.to_le_bytes());
        // Scales: 0 keys.
        data.extend_from_slice(&0u32.to_le_bytes());

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let td = NiTransformData::parse(&mut stream).unwrap();
        assert_eq!(
            stream.position() as usize,
            expected_len,
            "parser must consume every byte of X + Y + Z KeyGroups (audit premise: bytes left in stream → downstream drift)"
        );
        assert_eq!(td.rotation_type, Some(KeyType::XyzRotation));
        assert!(td.rotation_keys.is_empty());

        let xyz = td
            .xyz_rotations
            .as_ref()
            .expect("xyz_rotations must be populated in XyzRotation mode");

        // Each axis has its own distinct key count — proves Y/Z weren't
        // silently skipped or overwritten with X's data.
        assert_eq!(xyz[0].keys.len(), 2, "X axis (2 keys)");
        assert_eq!(
            xyz[1].keys.len(),
            3,
            "Y axis (3 keys) — audit imagined this was missed"
        );
        assert_eq!(
            xyz[2].keys.len(),
            1,
            "Z axis (1 key) — audit imagined this was missed"
        );

        // Spot-check authored values so a future parser that reads
        // three KeyGroups but at the wrong offsets still fails.
        assert_eq!(xyz[0].keys[1].value, 0.2);
        assert_eq!(xyz[1].keys[2].value, 2.0);
        assert_eq!(xyz[2].keys[0].value, 3.0);
    }

    /// Regression for #714 — nif.xml specifies an `Order` float
    /// (4-byte phantom) between `Rotation Type` and the three `XYZ
    /// Rotations` KeyGroups when (a) rotation_type == XyzRotation and
    /// (b) version <= 10.1.0.0.  Without the fix the stream under-reads
    /// by 4 bytes and all subsequent blocks walk 4 bytes early.
    #[test]
    fn parse_transform_data_pre10_xyz_rotation_consumes_order_float() {
        // Build a header with version 10.0.1.0 (pre-10.1.0.0 boundary)
        let header = NifHeader {
            version: NifVersion(0x0A000100), // 10.0.1.0
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };

        let mut data = Vec::new();
        // Num Rotation Keys = 1
        data.extend_from_slice(&1u32.to_le_bytes());
        // Rotation Type = XyzRotation (4)
        data.extend_from_slice(&4u32.to_le_bytes());
        // Order float (only present on pre-10.1): a sentinel 1.23456 so
        // a test that silently skips it would sample the next field wrong
        data.extend_from_slice(&1.23456f32.to_le_bytes());
        // KeyGroup X: 1 Linear key
        data.extend_from_slice(&1u32.to_le_bytes()); // num_keys
        data.extend_from_slice(&1u32.to_le_bytes()); // Linear
        data.extend_from_slice(&0.0f32.to_le_bytes()); // time
        data.extend_from_slice(&0.5f32.to_le_bytes()); // value=0.5 (X)
                                                       // KeyGroup Y: 1 Linear key
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes()); // value=1.0 (Y)
                                                       // KeyGroup Z: 1 Linear key
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes()); // value=2.0 (Z)
                                                       // Translations: 0 keys
        data.extend_from_slice(&0u32.to_le_bytes());
        // Scales: 0 keys
        data.extend_from_slice(&0u32.to_le_bytes());

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let td = NiTransformData::parse(&mut stream).unwrap();

        assert_eq!(
            stream.position() as usize,
            expected_len,
            "pre-10.1: Order float must be consumed so stream position is exact"
        );
        let xyz = td.xyz_rotations.as_ref().unwrap();
        // If Order were not consumed the first value (1.23456 interpreted
        // as f32 bytes) would land here and the check would fail.
        assert_eq!(xyz[0].keys[0].value, 0.5, "X axis value");
        assert_eq!(xyz[1].keys[0].value, 1.0, "Y axis value");
        assert_eq!(xyz[2].keys[0].value, 2.0, "Z axis value");
    }

    /// Counterpart to the above: on a v20 NIF there is no Order float.
    /// The existing `parse_transform_data_xyz_rotation_reads_all_three_axes`
    /// already covers post-10.1 correctness, but this test uses identical
    /// key values so the two can be compared side-by-side.
    #[test]
    fn parse_transform_data_post20_xyz_rotation_has_no_order_float() {
        // post-10.1 header (FNV)
        let header = NifHeader {
            version: NifVersion::V20_2_0_7,
            little_endian: true,
            user_version: 11,
            user_version_2: 34,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };

        let mut data = Vec::new();
        // Num Rotation Keys = 1
        data.extend_from_slice(&1u32.to_le_bytes());
        // Rotation Type = XyzRotation (4)
        data.extend_from_slice(&4u32.to_le_bytes());
        // NO Order float on v20+
        // KeyGroup X: 1 Linear key
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        // KeyGroup Y
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        // KeyGroup Z
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&1u32.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes());
        data.extend_from_slice(&2.0f32.to_le_bytes());
        // Translations: 0, Scales: 0
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let td = NiTransformData::parse(&mut stream).unwrap();

        assert_eq!(stream.position() as usize, expected_len);
        let xyz = td.xyz_rotations.as_ref().unwrap();
        assert_eq!(xyz[0].keys[0].value, 0.5);
        assert_eq!(xyz[1].keys[0].value, 1.0);
        assert_eq!(xyz[2].keys[0].value, 2.0);
    }

    /// Regression for #548 — plain `NiBoolInterpolator` keeps the
    /// `Plain` discriminator (the pre-fix default) and reports its
    /// original wire type name. Guards against the Timeline variant
    /// accidentally widening to include plain blocks.
    #[test]
    fn ni_bool_interpolator_plain_stamps_plain_kind() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        data.push(1u8); // value = true
        data.extend_from_slice(&7i32.to_le_bytes()); // data_ref
        let mut stream = NifStream::new(&data, &header);
        let interp = NiBoolInterpolator::parse(&mut stream).unwrap();
        assert!(interp.value);
        assert_eq!(interp.data_ref.index(), Some(7));
        assert_eq!(interp.kind, BoolInterpolatorKind::Plain);
        assert_eq!(interp.block_type_name(), "NiBoolInterpolator");
        assert_eq!(stream.position() as usize, data.len());
    }

    /// Regression for #548 — `NiBoolTimelineInterpolator` shares the
    /// wire layout of `NiBoolInterpolator` per nif.xml line 3287 (no
    /// additional fields), so `parse_timeline` consumes exactly the
    /// same 5 bytes (1 byte bool + 4 byte BlockRef). The discriminator
    /// is what lets downstream importers tell the two apart — pre-fix
    /// 8,450 blocks across FO3 + FNV + Skyrim SE went to NiUnknown.
    #[test]
    fn ni_bool_timeline_interpolator_parses_identical_wire_layout() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        data.push(0u8); // value = false
        data.extend_from_slice(&12i32.to_le_bytes()); // data_ref
        let mut stream = NifStream::new(&data, &header);
        let interp = NiBoolInterpolator::parse_timeline(&mut stream).unwrap();
        assert!(!interp.value);
        assert_eq!(interp.data_ref.index(), Some(12));
        assert_eq!(interp.kind, BoolInterpolatorKind::Timeline);
        assert_eq!(
            interp.block_type_name(),
            "NiBoolTimelineInterpolator",
            "block_type_name must dispatch on the wire-type discriminator"
        );
        assert_eq!(
            stream.position() as usize,
            data.len(),
            "timeline wire layout is identical to plain — no extra fields per nif.xml line 3287"
        );
    }

    /// Regression for #548 — the dispatcher must route
    /// `NiBoolTimelineInterpolator` through `parse_timeline` (not the
    /// plain `parse`). Pre-fix the dispatch arm was absent and the
    /// block fell into the `NiUnknown` fallback at `blocks/mod.rs:705`.
    #[test]
    fn ni_bool_timeline_interpolator_dispatches_via_parse_block() {
        let header = make_header_fnv();
        let mut data = Vec::new();
        data.push(1u8); // value = true
        data.extend_from_slice(&99i32.to_le_bytes()); // data_ref
        let mut stream = NifStream::new(&data, &header);
        let block = crate::blocks::parse_block(
            "NiBoolTimelineInterpolator",
            &mut stream,
            Some(data.len() as u32),
        )
        .expect("dispatch must route Timeline variant — pre-fix it was NiUnknown");
        assert_eq!(block.block_type_name(), "NiBoolTimelineInterpolator");
        let interp = block
            .as_any()
            .downcast_ref::<NiBoolInterpolator>()
            .expect("Timeline and plain share the Rust struct");
        assert_eq!(interp.kind, BoolInterpolatorKind::Timeline);
        assert_eq!(interp.data_ref.index(), Some(99));
    }

    /// Boundary regression for #935 (post-#769 doctrine flip). nif.xml
    /// gates `Order` with `until="10.1.0.0"` which is **inclusive** per
    /// niftools/nifly (see version.rs doctrine). The field IS present
    /// at v10.1.0.0 exactly; the first version that drops it is
    /// v10.1.0.1.
    #[test]
    fn parse_transform_data_xyz_order_at_v10_1_0_0_exactly() {
        let header = NifHeader {
            version: NifVersion::V10_1_0_0,
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes()); // num_rotation_keys = 1
        data.extend_from_slice(&4u32.to_le_bytes()); // KeyType::XyzRotation
        data.extend_from_slice(&0u32.to_le_bytes()); // Order = 0 (XYZ) — IS read at v10.1.0.0 (inclusive)
        data.extend_from_slice(&0u32.to_le_bytes()); // X KeyGroup num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // Y KeyGroup num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // Z KeyGroup num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // translations num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // scales num_keys = 0

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let td = NiTransformData::parse(&mut stream)
            .expect("v10.1.0.0 NiTransformData must consume Order under inclusive doctrine");
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(td.rotation_type, Some(KeyType::XyzRotation));
        assert!(td.xyz_rotations.is_some());
    }

    /// Boundary above the inclusive `until="10.1.0.0"` — at v10.1.0.1
    /// the Order field is finally absent.
    #[test]
    fn parse_transform_data_xyz_no_order_at_v10_1_0_1() {
        let header = NifHeader {
            version: NifVersion(0x0A010001),
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes()); // num_rotation_keys = 1
        data.extend_from_slice(&4u32.to_le_bytes()); // KeyType::XyzRotation
                                                     // NO Order at v10.1.0.1 (just above the inclusive until= boundary)
        data.extend_from_slice(&0u32.to_le_bytes()); // X KeyGroup num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // Y KeyGroup num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // Z KeyGroup num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // translations num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // scales num_keys = 0

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let td = NiTransformData::parse(&mut stream)
            .expect("v10.1.0.1 NiTransformData must skip Order under inclusive doctrine");
        assert_eq!(stream.position() as usize, expected_len);
        assert_eq!(td.rotation_type, Some(KeyType::XyzRotation));
        assert!(td.xyz_rotations.is_some());
    }

    /// Pre-boundary spot check: at v10.0.1.0 (below the boundary) the
    /// `Order` field IS still present and must be consumed.
    #[test]
    fn parse_transform_data_xyz_with_order_below_v10_1_0_0() {
        let header = NifHeader {
            version: NifVersion(0x0A000100), // v10.0.1.0 — below the until= boundary
            little_endian: true,
            user_version: 0,
            user_version_2: 0,
            num_blocks: 0,
            block_types: Vec::new(),
            block_type_indices: Vec::new(),
            block_sizes: Vec::new(),
            strings: Vec::new(),
            max_string_length: 0,
            num_groups: 0,
        };
        let mut data = Vec::new();
        data.extend_from_slice(&1u32.to_le_bytes()); // num_rotation_keys = 1
        data.extend_from_slice(&4u32.to_le_bytes()); // KeyType::XyzRotation
        data.extend_from_slice(&0.0f32.to_le_bytes()); // Order (present pre-10.1.0.0)
        data.extend_from_slice(&0u32.to_le_bytes()); // X KeyGroup num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // Y KeyGroup num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // Z KeyGroup num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // translations num_keys = 0
        data.extend_from_slice(&0u32.to_le_bytes()); // scales num_keys = 0

        let expected_len = data.len();
        let mut stream = NifStream::new(&data, &header);
        let td = NiTransformData::parse(&mut stream)
            .expect("v10.0.1.0 NiTransformData with XYZ rotation must consume Order");
        assert_eq!(stream.position() as usize, expected_len);
        assert!(td.xyz_rotations.is_some());
    }
}

// ── NiBlendInterpolator family ──────────────────────────────────────

/// An entry in the blend interpolator's weighted array.
#[derive(Debug)]
pub struct InterpBlendItem {
    pub interpolator_ref: BlockRef,
    pub weight: f32,
    pub normalized_weight: f32,
    pub priority: u8,
    pub ease_spinner: f32,
}

/// NiBlendInterpolator base data (abstract in Gamebryo, concrete in our parser).
///
/// Used by NiControllerManager for NIF-level animation blending between
/// sequences. Manager-controlled mode (flag bit 0) is the common case
/// for Bethesda games — most optional fields are absent.
#[derive(Debug)]
pub struct NiBlendInterpolator {
    pub flags: u8,
    pub array_size: u8,
    pub weight_threshold: f32,
    pub manager_controlled: bool,
    pub interp_count: u8,
    pub single_index: u8,
    pub items: Vec<InterpBlendItem>,
}

impl NiBlendInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let flags = stream.read_u8()?;
        let array_size = stream.read_u8()?;
        let weight_threshold = stream.read_f32_le()?;

        let manager_controlled = flags & 1 != 0;

        let mut interp_count = 0u8;
        let mut single_index = 0u8;
        let mut items = Vec::new();

        if !manager_controlled {
            interp_count = stream.read_u8()?;
            single_index = stream.read_u8()?;
            let _high_priority = stream.read_u8()? as i8;
            let _next_high_priority = stream.read_u8()? as i8;
            let _single_time = stream.read_f32_le()?;
            let _high_weights_sum = stream.read_f32_le()?;
            let _next_high_weights_sum = stream.read_f32_le()?;
            let _high_ease_spinner = stream.read_f32_le()?;

            items.reserve(array_size as usize);
            for _ in 0..array_size {
                let interpolator_ref = stream.read_block_ref()?;
                let weight = stream.read_f32_le()?;
                let normalized_weight = stream.read_f32_le()?;
                let priority = stream.read_u8()?;
                let ease_spinner = stream.read_f32_le()?;
                items.push(InterpBlendItem {
                    interpolator_ref,
                    weight,
                    normalized_weight,
                    priority,
                    ease_spinner,
                });
            }
        }

        Ok(Self {
            flags,
            array_size,
            weight_threshold,
            manager_controlled,
            interp_count,
            single_index,
            items,
        })
    }
}

/// NiBlendTransformInterpolator — blends NiQuatTransform values.
/// No additional fields beyond NiBlendInterpolator for version >= 10.1.0.110.
#[derive(Debug)]
pub struct NiBlendTransformInterpolator {
    pub base: NiBlendInterpolator,
}

impl NiObject for NiBlendTransformInterpolator {
    fn block_type_name(&self) -> &'static str {
        "NiBlendTransformInterpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBlendTransformInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        Ok(Self {
            base: NiBlendInterpolator::parse(stream)?,
        })
    }
}

/// NiBlendFloatInterpolator — blends float values.
#[derive(Debug)]
pub struct NiBlendFloatInterpolator {
    pub base: NiBlendInterpolator,
    pub value: f32,
}

impl NiObject for NiBlendFloatInterpolator {
    fn block_type_name(&self) -> &'static str {
        "NiBlendFloatInterpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBlendFloatInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiBlendInterpolator::parse(stream)?;
        let value = stream.read_f32_le()?;
        Ok(Self { base, value })
    }
}

/// NiBlendPoint3Interpolator — blends NiPoint3 (Vec3) values.
#[derive(Debug)]
pub struct NiBlendPoint3Interpolator {
    pub base: NiBlendInterpolator,
    pub value: [f32; 3],
}

impl NiObject for NiBlendPoint3Interpolator {
    fn block_type_name(&self) -> &'static str {
        "NiBlendPoint3Interpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBlendPoint3Interpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiBlendInterpolator::parse(stream)?;
        let x = stream.read_f32_le()?;
        let y = stream.read_f32_le()?;
        let z = stream.read_f32_le()?;
        Ok(Self {
            base,
            value: [x, y, z],
        })
    }
}

/// NiBlendBoolInterpolator — blends bool values.
#[derive(Debug)]
pub struct NiBlendBoolInterpolator {
    pub base: NiBlendInterpolator,
    pub value: u8,
}

impl NiObject for NiBlendBoolInterpolator {
    fn block_type_name(&self) -> &'static str {
        "NiBlendBoolInterpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBlendBoolInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let base = NiBlendInterpolator::parse(stream)?;
        let value = stream.read_u8()?;
        Ok(Self { base, value })
    }
}

// ── NiUVData ──────────────────────────────────────────────────────────
//
// Four float KeyGroups that feed NiUVController:
//   0 = offset U, 1 = offset V, 2 = tiling U, 3 = tiling V.
// Referenced only by NiUVController. See issue #154.

#[derive(Debug)]
pub struct NiUVData {
    /// Four animated UV channels: [offset_u, offset_v, tiling_u, tiling_v].
    pub groups: [KeyGroup<FloatKey>; 4],
}

impl NiObject for NiUVData {
    fn block_type_name(&self) -> &'static str {
        "NiUVData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiUVData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let g0 = KeyGroup::<FloatKey>::parse(stream)?;
        let g1 = KeyGroup::<FloatKey>::parse(stream)?;
        let g2 = KeyGroup::<FloatKey>::parse(stream)?;
        let g3 = KeyGroup::<FloatKey>::parse(stream)?;
        Ok(Self {
            groups: [g0, g1, g2, g3],
        })
    }
}

// ── NiBSpline* compressed animation family ─────────────────────────────
//
// Used pervasively by Skyrim / FO4 KF files for actor body and face
// animation. Stores quantized control points for open uniform B-splines
// of degree 3. The interpolator carries per-channel handles that index
// into `NiBSplineData::compact_control_points` (i16 values).
//
// Decompression formula per control point:
//     value = offset + (short / 32767) * half_range
//
// See `anim.rs::extract_transform_channel_bspline` for the De Boor
// evaluator that turns these into sampled TQS keys.

/// `NiBSplineBasisData` — control-point count for the B-spline basis.
#[derive(Debug)]
pub struct NiBSplineBasisData {
    pub num_control_points: u32,
}

impl NiObject for NiBSplineBasisData {
    fn block_type_name(&self) -> &'static str {
        "NiBSplineBasisData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBSplineBasisData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_control_points = stream.read_u32_le()?;
        Ok(Self { num_control_points })
    }
}

/// `NiBSplineData` — flat arrays of B-spline control points.
///
/// Both float and compact (i16) arrays can be populated simultaneously
/// in the same block; handles in the interpolator pick the right slice.
#[derive(Debug)]
pub struct NiBSplineData {
    pub float_control_points: Vec<f32>,
    pub compact_control_points: Vec<i16>,
}

impl NiObject for NiBSplineData {
    fn block_type_name(&self) -> &'static str {
        "NiBSplineData"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBSplineData {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        let num_float = stream.read_u32_le()?;
        let mut float_control_points: Vec<f32> = stream.allocate_vec(num_float)?;
        for _ in 0..num_float {
            float_control_points.push(stream.read_f32_le()?);
        }
        let num_compact = stream.read_u32_le()?;
        let mut compact_control_points: Vec<i16> = stream.allocate_vec(num_compact)?;
        for _ in 0..num_compact {
            // `short` in nif.xml — signed 16-bit.
            let raw = stream.read_u16_le()?;
            compact_control_points.push(raw as i16);
        }
        Ok(Self {
            float_control_points,
            compact_control_points,
        })
    }
}

/// `NiBSplineCompTransformInterpolator` — B-spline driven transform channel
/// using compact (quantized) control points. Inherits
/// `NiBSplineTransformInterpolator` → `NiBSplineInterpolator`.
///
/// Serialized layout (flat, in inheritance order):
/// - NiBSplineInterpolator: `start_time`, `stop_time`, `spline_data_ref`, `basis_data_ref`
/// - NiBSplineTransformInterpolator: `NiQuatTransform` + 3 handles
/// - NiBSplineCompTransformInterpolator: 6 quantization params (offset + half_range per channel)
///
/// A handle value of `u32::MAX` means that channel is static (use the
/// inherited `transform` field directly).
#[derive(Debug)]
pub struct NiBSplineCompTransformInterpolator {
    pub start_time: f32,
    pub stop_time: f32,
    pub spline_data_ref: BlockRef,
    pub basis_data_ref: BlockRef,
    /// Static fallback transform when the corresponding handle is invalid.
    pub transform: NiQuatTransform,
    pub translation_handle: u32,
    pub rotation_handle: u32,
    pub scale_handle: u32,
    pub translation_offset: f32,
    pub translation_half_range: f32,
    pub rotation_offset: f32,
    pub rotation_half_range: f32,
    pub scale_offset: f32,
    pub scale_half_range: f32,
}

impl NiObject for NiBSplineCompTransformInterpolator {
    fn block_type_name(&self) -> &'static str {
        "NiBSplineCompTransformInterpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBSplineCompTransformInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiBSplineInterpolator base
        let start_time = stream.read_f32_le()?;
        let stop_time = stream.read_f32_le()?;
        let spline_data_ref = stream.read_block_ref()?;
        let basis_data_ref = stream.read_block_ref()?;

        // NiBSplineTransformInterpolator
        let transform = stream.read_ni_quat_transform()?;
        let translation_handle = stream.read_u32_le()?;
        let rotation_handle = stream.read_u32_le()?;
        let scale_handle = stream.read_u32_le()?;

        // NiBSplineCompTransformInterpolator
        let translation_offset = stream.read_f32_le()?;
        let translation_half_range = stream.read_f32_le()?;
        let rotation_offset = stream.read_f32_le()?;
        let rotation_half_range = stream.read_f32_le()?;
        let scale_offset = stream.read_f32_le()?;
        let scale_half_range = stream.read_f32_le()?;

        Ok(Self {
            start_time,
            stop_time,
            spline_data_ref,
            basis_data_ref,
            transform,
            translation_handle,
            rotation_handle,
            scale_handle,
            translation_offset,
            translation_half_range,
            rotation_offset,
            rotation_half_range,
            scale_offset,
            scale_half_range,
        })
    }
}

/// `NiBSplineCompFloatInterpolator` — B-spline driven scalar (float)
/// channel using compact (quantized) control points. Inherits
/// `NiBSplineFloatInterpolator` → `NiBSplineInterpolator`.
///
/// nif.xml wire layout (flat, in inheritance order):
/// - NiBSplineInterpolator: `start_time` (f32), `stop_time` (f32),
///   `spline_data_ref` (Ref → NiBSplineData), `basis_data_ref`
///   (Ref → NiBSplineBasisData)
/// - NiBSplineFloatInterpolator: `value` (f32 fallback), `handle` (u32,
///   `0xFFFFFFFF` ≡ static)
/// - NiBSplineCompFloatInterpolator: `float_offset` (f32),
///   `float_half_range` (f32) — quantization params
///
/// Used by FNV/FO3/Skyrim/FO4 KFs to drive alpha or scale curves on a
/// `NiControllerSequence`. Pre-#936 the block had no dispatch arm; the
/// outer parse loop discarded it via the block_size fallback, so paired
/// float channels alongside `NiBSplineCompTransformInterpolator`
/// silently collapsed to constant or rest values.
#[derive(Debug)]
pub struct NiBSplineCompFloatInterpolator {
    pub start_time: f32,
    pub stop_time: f32,
    pub spline_data_ref: BlockRef,
    pub basis_data_ref: BlockRef,
    /// Static fallback value used when `handle == u32::MAX`.
    pub value: f32,
    pub handle: u32,
    pub float_offset: f32,
    pub float_half_range: f32,
}

impl NiObject for NiBSplineCompFloatInterpolator {
    fn block_type_name(&self) -> &'static str {
        "NiBSplineCompFloatInterpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBSplineCompFloatInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiBSplineInterpolator base.
        let start_time = stream.read_f32_le()?;
        let stop_time = stream.read_f32_le()?;
        let spline_data_ref = stream.read_block_ref()?;
        let basis_data_ref = stream.read_block_ref()?;
        // NiBSplineFloatInterpolator.
        let value = stream.read_f32_le()?;
        let handle = stream.read_u32_le()?;
        // NiBSplineCompFloatInterpolator.
        let float_offset = stream.read_f32_le()?;
        let float_half_range = stream.read_f32_le()?;
        Ok(Self {
            start_time,
            stop_time,
            spline_data_ref,
            basis_data_ref,
            value,
            handle,
            float_offset,
            float_half_range,
        })
    }
}

/// `NiBSplineCompPoint3Interpolator` — B-spline driven Vec3 channel
/// using compact (quantized) control points. Inherits
/// `NiBSplinePoint3Interpolator` → `NiBSplineInterpolator`.
///
/// nif.xml wire layout (flat, in inheritance order):
/// - NiBSplineInterpolator: `start_time` (f32), `stop_time` (f32),
///   `spline_data_ref` (Ref → NiBSplineData), `basis_data_ref`
///   (Ref → NiBSplineBasisData)
/// - NiBSplinePoint3Interpolator: `value` (Vector3 fallback), `handle` (u32,
///   `0xFFFFFFFF` ≡ static)
/// - NiBSplineCompPoint3Interpolator: `position_offset` (f32),
///   `position_half_range` (f32) — quantization params
///
/// Channel stride for the compact data slice is 3 (x, y, z). Used by
/// FNV/FO3/Skyrim/FO4 KFs for color / translation curves that ride
/// alongside `NiBSplineCompTransformInterpolator`. Pre-#936 the block
/// landed on the NiUnknown recovery path and the channel was silently
/// dropped.
#[derive(Debug)]
pub struct NiBSplineCompPoint3Interpolator {
    pub start_time: f32,
    pub stop_time: f32,
    pub spline_data_ref: BlockRef,
    pub basis_data_ref: BlockRef,
    /// Static fallback Vec3 used when `handle == u32::MAX`.
    pub value: [f32; 3],
    pub handle: u32,
    pub position_offset: f32,
    pub position_half_range: f32,
}

impl NiObject for NiBSplineCompPoint3Interpolator {
    fn block_type_name(&self) -> &'static str {
        "NiBSplineCompPoint3Interpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBSplineCompPoint3Interpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // NiBSplineInterpolator base.
        let start_time = stream.read_f32_le()?;
        let stop_time = stream.read_f32_le()?;
        let spline_data_ref = stream.read_block_ref()?;
        let basis_data_ref = stream.read_block_ref()?;
        // NiBSplinePoint3Interpolator.
        let vx = stream.read_f32_le()?;
        let vy = stream.read_f32_le()?;
        let vz = stream.read_f32_le()?;
        let handle = stream.read_u32_le()?;
        // NiBSplineCompPoint3Interpolator.
        let position_offset = stream.read_f32_le()?;
        let position_half_range = stream.read_f32_le()?;
        Ok(Self {
            start_time,
            stop_time,
            spline_data_ref,
            basis_data_ref,
            value: [vx, vy, vz],
            handle,
            position_offset,
            position_half_range,
        })
    }
}
