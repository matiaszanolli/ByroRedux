//! Image-space modifier (`IMAD`) records.
//!
//! Skyrim stores each animated channel as a sequence of normalized-time
//! keys. `DNAM` supplies the duration and radial-blur centre; the remaining
//! subrecords carry scalar or RGBA curves. Keeping the authored curves here
//! lets the runtime sample them without retaining opaque plugin bytes.

use super::super::common::read_zstring;
use crate::esm::reader::SubRecord;

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImadScalarKey {
    pub time: f32,
    pub value: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImadColorKey {
    pub time: f32,
    pub color: [f32; 4],
}

/// Animated image-space channels consumed by the final presentation pass.
/// #2380 (SAVE-D1-15) — `Serialize`/`Deserialize` added so this record can
/// ride inside `CinematicPresentationState.image_space_modifier_catalog`
/// when that resource is saved. Plain data (`String`/`u32`/`f32`/`Vec`) —
/// no `EntityId`, no engine-side handle.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ImadRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub flags: u32,
    pub duration_seconds: f32,
    pub radial_blur_center: [f32; 2],
    pub blur_radius: Vec<ImadScalarKey>,
    pub double_vision_strength: Vec<ImadScalarKey>,
    pub tint_color: Vec<ImadColorKey>,
    pub fade_color: Vec<ImadColorKey>,
    pub radial_blur_strength: Vec<ImadScalarKey>,
    pub radial_blur_ramp_up: Vec<ImadScalarKey>,
    pub radial_blur_start: Vec<ImadScalarKey>,
    pub radial_blur_ramp_down: Vec<ImadScalarKey>,
    pub radial_blur_down_start: Vec<ImadScalarKey>,
    pub motion_blur_strength: Vec<ImadScalarKey>,
    pub saturation_mult: Vec<ImadScalarKey>,
    pub saturation_add: Vec<ImadScalarKey>,
    pub brightness_mult: Vec<ImadScalarKey>,
    pub brightness_add: Vec<ImadScalarKey>,
    pub contrast_mult: Vec<ImadScalarKey>,
    pub contrast_add: Vec<ImadScalarKey>,
}

impl Default for ImadRecord {
    fn default() -> Self {
        Self {
            form_id: 0,
            editor_id: String::new(),
            flags: 0,
            duration_seconds: 0.0,
            radial_blur_center: [0.5, 0.5],
            blur_radius: Vec::new(),
            double_vision_strength: Vec::new(),
            tint_color: Vec::new(),
            fade_color: Vec::new(),
            radial_blur_strength: Vec::new(),
            radial_blur_ramp_up: Vec::new(),
            radial_blur_start: Vec::new(),
            radial_blur_ramp_down: Vec::new(),
            radial_blur_down_start: Vec::new(),
            motion_blur_strength: Vec::new(),
            saturation_mult: Vec::new(),
            saturation_add: Vec::new(),
            brightness_mult: Vec::new(),
            brightness_add: Vec::new(),
            contrast_mult: Vec::new(),
            contrast_add: Vec::new(),
        }
    }
}

fn read_f32(data: &[u8], offset: usize) -> Option<f32> {
    let bytes: [u8; 4] = data.get(offset..offset + 4)?.try_into().ok()?;
    Some(f32::from_le_bytes(bytes))
}

fn scalar_key(data: &[u8]) -> Option<ImadScalarKey> {
    Some(ImadScalarKey {
        time: read_f32(data, 0)?,
        value: read_f32(data, 4)?,
    })
}

fn color_key(data: &[u8]) -> Option<ImadColorKey> {
    Some(ImadColorKey {
        time: read_f32(data, 0)?,
        color: [
            read_f32(data, 4)?,
            read_f32(data, 8)?,
            read_f32(data, 12)?,
            read_f32(data, 16)?,
        ],
    })
}

/// Decode the Skyrim/FO3-family IMAD channels used by cinematic rendering.
/// Unknown HDR/DOF channels remain safely ignored until they have a live
/// consumer.
pub fn parse_imad(form_id: u32, subs: &[SubRecord]) -> ImadRecord {
    let mut out = ImadRecord {
        form_id,
        ..Default::default()
    };

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"DNAM" => {
                if let Some(bytes) = sub.data.get(0..4) {
                    out.flags = u32::from_le_bytes(bytes.try_into().expect("four-byte slice"));
                }
                out.duration_seconds = read_f32(&sub.data, 4).unwrap_or_default();
                // DNAM offsets 204/208 in the 244-byte Skyrim layout.
                if let (Some(x), Some(y)) = (read_f32(&sub.data, 204), read_f32(&sub.data, 208)) {
                    if x.is_finite() && y.is_finite() {
                        out.radial_blur_center = [x, y];
                    }
                }
            }
            b"BNAM" => push_scalar(&mut out.blur_radius, &sub.data),
            b"VNAM" => push_scalar(&mut out.double_vision_strength, &sub.data),
            b"TNAM" => push_color(&mut out.tint_color, &sub.data),
            b"NAM3" => push_color(&mut out.fade_color, &sub.data),
            b"RNAM" => push_scalar(&mut out.radial_blur_strength, &sub.data),
            b"SNAM" => push_scalar(&mut out.radial_blur_ramp_up, &sub.data),
            b"UNAM" => push_scalar(&mut out.radial_blur_start, &sub.data),
            b"NAM1" => push_scalar(&mut out.radial_blur_ramp_down, &sub.data),
            b"NAM2" => push_scalar(&mut out.radial_blur_down_start, &sub.data),
            b"NAM4" => push_scalar(&mut out.motion_blur_strength, &sub.data),
            [0x11, b'I', b'A', b'D'] => push_scalar(&mut out.saturation_mult, &sub.data),
            b"QIAD" => push_scalar(&mut out.saturation_add, &sub.data),
            [0x12, b'I', b'A', b'D'] => push_scalar(&mut out.brightness_mult, &sub.data),
            b"RIAD" => push_scalar(&mut out.brightness_add, &sub.data),
            [0x13, b'I', b'A', b'D'] => push_scalar(&mut out.contrast_mult, &sub.data),
            b"SIAD" => push_scalar(&mut out.contrast_add, &sub.data),
            _ => {}
        }
    }
    out
}

fn push_scalar(keys: &mut Vec<ImadScalarKey>, data: &[u8]) {
    keys.extend(data.chunks_exact(8).filter_map(scalar_key));
}

fn push_color(keys: &mut Vec<ImadColorKey>, data: &[u8]) {
    keys.extend(data.chunks_exact(20).filter_map(color_key));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(sub_type: [u8; 4], data: Vec<u8>) -> SubRecord {
        SubRecord { sub_type, data }
    }

    #[test]
    fn parses_duration_center_and_presentation_curves() {
        let mut dnam = vec![0; 244];
        dnam[0..4].copy_from_slice(&3u32.to_le_bytes());
        dnam[4..8].copy_from_slice(&7.0f32.to_le_bytes());
        dnam[204..208].copy_from_slice(&0.25f32.to_le_bytes());
        dnam[208..212].copy_from_slice(&0.75f32.to_le_bytes());
        let mut scalar = Vec::new();
        for value in [0.0f32, 0.0, 0.5, 2.0] {
            scalar.extend_from_slice(&value.to_le_bytes());
        }
        let mut color = Vec::new();
        for value in [0.25f32, 1.0, 0.5, 0.25, 0.75] {
            color.extend_from_slice(&value.to_le_bytes());
        }

        let record = parse_imad(
            0x0010_1DAC,
            &[
                sub(*b"EDID", b"PlayerAlduinIMOD\0".to_vec()),
                sub(*b"DNAM", dnam),
                sub(*b"BNAM", scalar.clone()),
                sub([0x11, b'I', b'A', b'D'], scalar),
                sub(*b"TNAM", color),
            ],
        );

        assert_eq!(record.editor_id, "PlayerAlduinIMOD");
        assert_eq!(record.flags, 3);
        assert_eq!(record.duration_seconds, 7.0);
        assert_eq!(record.radial_blur_center, [0.25, 0.75]);
        assert_eq!(record.blur_radius.len(), 2);
        assert_eq!(record.blur_radius[1].value, 2.0);
        assert_eq!(record.saturation_mult[1].time, 0.5);
        assert_eq!(record.tint_color[0].color, [1.0, 0.5, 0.25, 0.75]);
    }

    #[test]
    fn ignores_truncated_keys_and_keeps_safe_center() {
        let record = parse_imad(
            7,
            &[
                sub(*b"DNAM", vec![0; 8]),
                sub(*b"BNAM", vec![0; 7]),
                sub(*b"TNAM", vec![0; 19]),
            ],
        );
        assert_eq!(record.radial_blur_center, [0.5, 0.5]);
        assert!(record.blur_radius.is_empty());
        assert!(record.tint_color.is_empty());
    }
}
