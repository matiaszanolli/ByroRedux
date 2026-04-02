//! NIF interpolator and keyframe data blocks.
//!
//! These blocks hold the actual animation keyframe data that controllers reference.
//! NiTransformInterpolator → NiTransformData (position/rotation/scale keys)
//! NiFloatInterpolator → NiFloatData (single-channel float keys)

use crate::stream::NifStream;
use crate::types::{BlockRef, NiQuatTransform};
use super::NiObject;
use std::any::Any;
use std::io;

// ── Key types ─────────────────────────────────────────────────────────

/// Interpolation type for keyframe data (nif.xml KeyType enum).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum KeyType {
    Linear = 1,
    Quadratic = 2,
    Tbc = 3,
    XyzRotation = 4,
}

impl KeyType {
    pub fn from_u32(v: u32) -> io::Result<Self> {
        match v {
            1 => Ok(Self::Linear),
            2 => Ok(Self::Quadratic),
            3 => Ok(Self::Tbc),
            4 => Ok(Self::XyzRotation),
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
        let mut keys = Vec::with_capacity(num_keys as usize);
        for _ in 0..num_keys {
            let time = stream.read_f32_le()?;
            let value = stream.read_f32_le()?;
            let mut tangent_forward = 0.0;
            let mut tangent_backward = 0.0;
            let mut tbc = None;
            match key_type {
                KeyType::Linear => {}
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
        let mut keys = Vec::with_capacity(num_keys as usize);
        for _ in 0..num_keys {
            let time = stream.read_f32_le()?;
            let x = stream.read_f32_le()?;
            let y = stream.read_f32_le()?;
            let z = stream.read_f32_le()?;
            let mut tangent_forward = [0.0; 3];
            let mut tangent_backward = [0.0; 3];
            let mut tbc = None;
            match key_type {
                KeyType::Linear => {}
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
                // XYZ rotation: no quaternion keys, three float key groups instead
                let x_keys = KeyGroup::<FloatKey>::parse(stream)?;
                let y_keys = KeyGroup::<FloatKey>::parse(stream)?;
                let z_keys = KeyGroup::<FloatKey>::parse(stream)?;
                xyz_rotations = Some([x_keys, y_keys, z_keys]);
            } else {
                // Quaternion keys
                rotation_keys.reserve(num_rotation_keys as usize);
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
        Ok(Self { transform, data_ref })
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

// ── NiTextKeyExtraData ────────────────────────────────────────────────

/// Text keys embedded in animation files — event markers like "start", "end", "hit".
#[derive(Debug)]
pub struct NiTextKeyExtraData {
    pub name: Option<String>,
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
        // NiObjectNET::name
        let name = stream.read_string()?;
        // NiExtraData base: next_extra_data_ref (version < 20.1) — skip for modern
        // For 20.1+ there's no next_extra_data_ref, just the string index above
        // num_text_keys
        let num_text_keys = stream.read_u32_le()?;
        let mut text_keys = Vec::with_capacity(num_text_keys as usize);
        for _ in 0..num_text_keys {
            let time = stream.read_f32_le()?;
            let text = stream.read_string()?.unwrap_or_default();
            text_keys.push((time, text));
        }
        Ok(Self { name, text_keys })
    }
}

// ── NiBoolInterpolator ────────────────────────────────────────────────

/// Interpolates a boolean value (visibility). References NiBoolData.
#[derive(Debug)]
pub struct NiBoolInterpolator {
    pub value: bool,
    pub data_ref: BlockRef,
}

impl NiObject for NiBoolInterpolator {
    fn block_type_name(&self) -> &'static str {
        "NiBoolInterpolator"
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl NiBoolInterpolator {
    pub fn parse(stream: &mut NifStream) -> io::Result<Self> {
        // Bool value is stored as a u8 in pre-20.2 and u32 in 20.2+
        // nif.xml says "NiBoolInterpolator" has: bool_value (NiBool), data_ref
        let value = stream.read_bool()?;
        let data_ref = stream.read_block_ref()?;
        Ok(Self { value, data_ref })
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
        let mut keys = Vec::with_capacity(num_keys as usize);
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
            strings: vec!["TestName".to_string(), "start".to_string()],
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
        for _ in 0..3 { data.extend_from_slice(&0.0f32.to_le_bytes()); }

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
        assert_eq!(tk.name, Some("TestName".to_string()));
        assert_eq!(tk.text_keys.len(), 1);
        assert_eq!(tk.text_keys[0].0, 0.0);
        assert_eq!(tk.text_keys[0].1, "start");
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
}
