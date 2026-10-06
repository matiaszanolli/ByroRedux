//! Image-space modifier (`IMAD`) records.
//!
//! Skyrim stores each animated channel as a sequence of normalized-time
//! keys. `DNAM` supplies the duration and radial-blur centre; the remaining
//! subrecords carry scalar or RGBA curves. Keeping the authored curves here
//! lets the runtime sample them without retaining opaque plugin bytes.

use super::super::common::CommonNamedFields;
use crate::esm::reader::{GameKind, SubRecord};

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
///
/// The cinematic block's sub-record codes are per game. Skyrim / FO4 / FO76
/// author `0x11` Saturation, `0x12` Brightness, `0x13` Contrast (xEdit
/// `wbTimeInterpolatorsMultAdd`). FO3/FNV author four channels in the
/// `IMGS.DNAM` cinematic order — `0x11` Saturation, `0x12` Contrast Avg Lum,
/// `0x13` Contrast, `0x14` Brightness. Each code's add curve is the code +
/// `0x40` (`Q`/`R`/`S`/`T IAD`).
///
/// The FO3/FNV `0x12`/`0x13` assignment deliberately departs from xEdit's
/// (and JIP NVSE's) IMAD labels, which name `0x12` Contrast. Census of every
/// FO3 + FNV IMAD (the weather "ISFX" grades set channels as mult 0 + add
/// target): 109 records set `0x12` and the values span 0.0..1.4, with the
/// *daytime* Capital Wasteland grades (`WastelandDayISFX`,
/// `WastelandEastDayISFX`) at exactly 0.0 — a contrast of zero is a flat
/// grey frame, which those weathers plainly do not render, while a contrast
/// pivot of zero is ordinary. `0x13` instead sits in 0.9..1.7 (median 1.03),
/// the same band as the `IMGS` Contrast value. The pivot is not consumed —
/// the presentation grade pivots on a fixed mid-grey, as for `IMGS`.
pub fn parse_imad(form_id: u32, subs: &[SubRecord], game: GameKind) -> ImadRecord {
    let mut out = ImadRecord {
        form_id,
        ..Default::default()
    };

    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    out.editor_id = common.editor_id;
    // (brightness, contrast) cinematic code bytes; saturation is 0x11 on all.
    let (bri, con) = match game {
        GameKind::Fallout3NV => (0x14u8, 0x13u8),
        _ => (0x12, 0x13),
    };
    for sub in subs {
        if let [code, b'I', b'A', b'D'] = sub.sub_type {
            let (mult, add) = (code & !0x40, code & 0x40 != 0);
            let channel = match mult {
                0x11 => Some((&mut out.saturation_mult, &mut out.saturation_add)),
                c if c == bri => Some((&mut out.brightness_mult, &mut out.brightness_add)),
                c if c == con => Some((&mut out.contrast_mult, &mut out.contrast_add)),
                _ => None,
            };
            if let Some((m, a)) = channel {
                push_scalar(if add { a } else { m }, &sub.data);
            }
            continue;
        }
        match &sub.sub_type {
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
            _ => {}
        }
    }
    out
}

fn push_scalar(keys: &mut Vec<ImadScalarKey>, data: &[u8]) {
    keys.extend(data.as_chunks::<8>().0.iter().filter_map(|c| scalar_key(c)));
}

fn push_color(keys: &mut Vec<ImadColorKey>, data: &[u8]) {
    keys.extend(data.as_chunks::<20>().0.iter().filter_map(|c| color_key(c)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

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
                sub(b"EDID", b"PlayerAlduinIMOD\0"),
                sub(b"DNAM", dnam),
                sub(b"BNAM", scalar.clone()),
                sub(&[0x11, b'I', b'A', b'D'], scalar),
                sub(b"TNAM", color),
            ],
            GameKind::Skyrim,
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

    /// FO3/FNV order the cinematic block Saturation / Contrast Avg Lum /
    /// Contrast / Brightness (`0x11..=0x14`, the IMGS order — see
    /// [`parse_imad`]), Skyrim+ Saturation / Brightness / Contrast — the same
    /// code byte means a different channel per game.
    #[test]
    fn cinematic_codes_follow_the_game_layout() {
        let key = |value: f32| {
            let mut bytes = 0.0f32.to_le_bytes().to_vec();
            bytes.extend_from_slice(&value.to_le_bytes());
            bytes
        };
        let subs = [
            sub(&[0x11, b'I', b'A', b'D'], key(0.5)),
            sub(&[0x51, b'I', b'A', b'D'], key(0.05)),
            sub(&[0x12, b'I', b'A', b'D'], key(1.2)),
            sub(&[0x52, b'I', b'A', b'D'], key(0.02)),
            sub(&[0x13, b'I', b'A', b'D'], key(0.7)),
            sub(&[0x14, b'I', b'A', b'D'], key(0.9)),
            sub(&[0x54, b'I', b'A', b'D'], key(0.04)),
        ];
        let fnv = parse_imad(1, &subs, GameKind::Fallout3NV);
        assert_eq!(fnv.saturation_mult[0].value, 0.5);
        assert_eq!(fnv.saturation_add[0].value, 0.05);
        // 0x12/0x52 is the contrast pivot on FO3/FNV — never the contrast.
        assert_eq!(fnv.contrast_mult[0].value, 0.7);
        assert!(fnv.contrast_add.is_empty());
        assert_eq!(fnv.brightness_mult[0].value, 0.9);
        assert_eq!(fnv.brightness_add[0].value, 0.04);

        let skyrim = parse_imad(1, &subs, GameKind::Skyrim);
        assert_eq!(skyrim.brightness_mult[0].value, 1.2);
        assert_eq!(skyrim.brightness_add[0].value, 0.02);
        assert_eq!(skyrim.contrast_mult[0].value, 0.7);
        assert!(skyrim.contrast_add.is_empty());
    }

    #[test]
    fn ignores_truncated_keys_and_keeps_safe_center() {
        let record = parse_imad(
            7,
            &[
                sub(b"DNAM", vec![0; 8]),
                sub(b"BNAM", vec![0; 7]),
                sub(b"TNAM", vec![0; 19]),
            ],
            GameKind::Skyrim,
        );
        assert_eq!(record.radial_blur_center, [0.5, 0.5]);
        assert!(record.blur_radius.is_empty());
        assert!(record.tint_color.is_empty());
    }
}
