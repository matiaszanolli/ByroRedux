//! The authored god-ray records the Volumetric Lighting PRD asks to be decoded and censused
//! (VLM-HF-010, VLM-HF-011), and nothing more: per §10 Q8 no field feeds the fog medium until its
//! meaning is settled from a source, so each decoded field is listed under VLM-HF-009 in the fog
//! design's corpus findings.
//!
//! - **`VOLI`** (Skyrim SE "Volumetric Lighting"): intensity, a custom-colour contribution, a colour,
//!   density (contribution, size, wind speed, falling speed), phase function (contribution,
//!   scattering) and a sampling range factor. Referenced per time of day by the `WTHR` `HNAM`.
//! - **`GDRY`** (FO4 "God Rays"): back and forward colour, intensity, three colour scales, two
//!   phases and an air colour, in one 60-byte `DATA`. Referenced per time of day by the `WTHR`
//!   `WGDR`, and by `CELL` `XGDR`, `LGTM` `WGDR` and `LIGH` `WGDR`.
//!
//! Layouts are xEdit's (`wbDefinitionsTES5.pas` VOLI, `wbDefinitionsFO4.pas` GDRY), confirmed
//! against the shipped masters by `crates/plugin/examples/fog_godray_census.rs` (Skyrim SE: 42
//! `VOLI`, all 13 single-float sub-records; FO4: 61 `GDRY`, `DATA` always 60 bytes).

use crate::esm::reader::SubRecord;
use crate::esm::sub_reader::SubReader;

/// Skyrim SE `VOLI`: one authored volumetric-lighting profile.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VoliRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// `CNAM` Intensity.
    pub intensity: f32,
    /// `DNAM` Custom Color - Contribution.
    pub custom_color_contribution: f32,
    /// `ENAM` / `FNAM` / `GNAM` — the red, green and blue floats of the custom colour.
    pub custom_color: [f32; 3],
    /// `HNAM` Density - Contribution.
    pub density_contribution: f32,
    /// `INAM` Density - Size.
    pub density_size: f32,
    /// `JNAM` Density - Wind Speed.
    pub density_wind_speed: f32,
    /// `KNAM` Density - Falling Speed.
    pub density_falling_speed: f32,
    /// `LNAM` Phase Function - Contribution.
    pub phase_contribution: f32,
    /// `MNAM` Phase Function - Scattering.
    pub phase_scattering: f32,
    /// `NNAM` Sampling Repartition - Range Factor.
    pub sampling_range_factor: f32,
}

/// FO4 `GDRY`: one authored god-ray profile.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GdryRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub back_color: [f32; 3],
    pub fwd_color: [f32; 3],
    pub intensity: f32,
    pub air_color_scale: f32,
    pub back_color_scale: f32,
    pub fwd_color_scale: f32,
    pub back_phase: f32,
    pub air_color: [f32; 3],
    pub fwd_phase: f32,
}

/// `GDRY.DATA` is one fixed struct (xEdit `wbDefinitionsFO4.pas` GDRY: 3 + 3 + 1 + 3 + 1 + 3 + 1 floats). A
/// shorter body is not a plausible record with zeros mid-layout, so it declines (#6783).
const GDRY_DATA_LEN: usize = 60;

fn edid(subs: &[SubRecord]) -> String {
    super::common::CommonNamedFields::from_subs_with_remap(subs, &None).editor_id
}

fn float_of(subs: &[SubRecord], tag: &[u8; 4]) -> f32 {
    subs.iter()
        .find(|s| &s.sub_type == tag)
        .map(|s| SubReader::new(&s.data).f32_or_default())
        .unwrap_or_default()
}

/// Decode a Skyrim SE `VOLI`. A missing sub-record leaves its field at zero.
pub fn parse_voli(form_id: u32, subs: &[SubRecord]) -> VoliRecord {
    VoliRecord {
        form_id,
        editor_id: edid(subs),
        intensity: float_of(subs, b"CNAM"),
        custom_color_contribution: float_of(subs, b"DNAM"),
        custom_color: [
            float_of(subs, b"ENAM"),
            float_of(subs, b"FNAM"),
            float_of(subs, b"GNAM"),
        ],
        density_contribution: float_of(subs, b"HNAM"),
        density_size: float_of(subs, b"INAM"),
        density_wind_speed: float_of(subs, b"JNAM"),
        density_falling_speed: float_of(subs, b"KNAM"),
        phase_contribution: float_of(subs, b"LNAM"),
        phase_scattering: float_of(subs, b"MNAM"),
        sampling_range_factor: float_of(subs, b"NNAM"),
    }
}

/// Decode an FO4 `GDRY` from its `DATA` (60 bytes: 15 floats). A `DATA` shorter than [`GDRY_DATA_LEN`]
/// leaves every field at its default; a longer one keeps the first 60 bytes.
pub fn parse_gdry(form_id: u32, subs: &[SubRecord]) -> GdryRecord {
    let mut out = GdryRecord { form_id, editor_id: edid(subs), ..Default::default() };
    if let Some(data) = subs.iter().find(|s| &s.sub_type == b"DATA" && s.data.len() >= GDRY_DATA_LEN) {
        let mut r = SubReader::new(&data.data);
        let rgb = |r: &mut SubReader| [r.f32_or_default(), r.f32_or_default(), r.f32_or_default()];
        out.back_color = rgb(&mut r);
        out.fwd_color = rgb(&mut r);
        out.intensity = r.f32_or_default();
        out.air_color_scale = r.f32_or_default();
        out.back_color_scale = r.f32_or_default();
        out.fwd_color_scale = r.f32_or_default();
        out.back_phase = r.f32_or_default();
        out.air_color = rgb(&mut r);
        out.fwd_phase = r.f32_or_default();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::test_support::sub;

    fn f(v: f32) -> [u8; 4] {
        v.to_le_bytes()
    }

    /// VLM-HF-010: the thirteen sub-records of a Skyrim SE `VOLI` land in their named fields.
    #[test]
    fn parse_voli_decodes_every_field() {
        let subs = vec![
            sub(b"EDID", b"VolumetricDayClear\0"),
            sub(b"CNAM", &f(2.5)),
            sub(b"DNAM", &f(0.25)),
            sub(b"ENAM", &f(0.9)),
            sub(b"FNAM", &f(0.8)),
            sub(b"GNAM", &f(0.7)),
            sub(b"HNAM", &f(0.3)),
            sub(b"INAM", &f(10.0)),
            sub(b"JNAM", &f(15.0)),
            sub(b"KNAM", &f(0.35)),
            sub(b"LNAM", &f(0.75)),
            sub(b"MNAM", &f(0.85)),
            sub(b"NNAM", &f(40.0)),
        ];
        let v = parse_voli(0x1234, &subs);
        assert_eq!(v.editor_id, "VolumetricDayClear");
        assert_eq!(
            (v.intensity, v.custom_color_contribution, v.custom_color),
            (2.5, 0.25, [0.9, 0.8, 0.7])
        );
        assert_eq!(
            (v.density_contribution, v.density_size, v.density_wind_speed, v.density_falling_speed),
            (0.3, 10.0, 15.0, 0.35)
        );
        assert_eq!(
            (v.phase_contribution, v.phase_scattering, v.sampling_range_factor),
            (0.75, 0.85, 40.0)
        );
        assert_eq!(parse_voli(1, &[]), VoliRecord { form_id: 1, ..Default::default() }, "no sub-records: zeros");
    }

    /// VLM-HF-011: a 60-byte `GDRY` `DATA` is fifteen floats in xEdit's order.
    #[test]
    fn parse_gdry_decodes_the_60_byte_data() {
        let mut data = Vec::new();
        for v in [
            0.1, 0.2, 0.3, // back colour
            0.4, 0.5, 0.6, // fwd colour
            7.0, // intensity
            3.0, 2.0, 4.0, // air / back / fwd colour scale
            0.15, // back phase
            0.5, 0.6, 0.7, // air colour
            0.75, // fwd phase
        ] {
            data.extend_from_slice(&f(v));
        }
        assert_eq!(data.len(), 60);
        let g = parse_gdry(0x99, &[sub(b"EDID", b"GodRaysClear\0"), sub(b"DATA", &data)]);
        assert_eq!(g.editor_id, "GodRaysClear");
        assert_eq!((g.back_color, g.fwd_color), ([0.1, 0.2, 0.3], [0.4, 0.5, 0.6]));
        assert_eq!(g.intensity, 7.0);
        assert_eq!((g.air_color_scale, g.back_color_scale, g.fwd_color_scale), (3.0, 2.0, 4.0));
        assert_eq!((g.back_phase, g.air_color, g.fwd_phase), (0.15, [0.5, 0.6, 0.7], 0.75));
        // #6783 — a short DATA declines whole: no plausible partial record with zeros mid-layout.
        let short = parse_gdry(1, &[sub(b"DATA", &data[..28])]);
        assert_eq!(short, GdryRecord { form_id: 1, ..Default::default() });
    }

    /// VLM-HF-010: the Skyrim SE `WTHR` `HNAM` names one `VOLI` per time of day (Sunrise, Day,
    /// Sunset, Night), a NULL slot is `None`, and the same sub-record on a pre-SE weather is ignored.
    #[test]
    fn parse_wthr_sse_hnam_and_voli() {
        use crate::esm::records::parse_wthr;
        use crate::esm::reader::GameKind;
        let mut hnam = Vec::new();
        for form in [0x0010_0001u32, 0x0010_0002, 0, 0x0010_0004] {
            hnam.extend_from_slice(&form.to_le_bytes());
        }
        let w = parse_wthr(0x77, &[sub(b"EDID", b"SkyrimClear\0"), sub(b"HNAM", &hnam)], GameKind::Skyrim, &None);
        assert_eq!(w.volumetric_lighting, vec![Some(0x0010_0001), Some(0x0010_0002), None, Some(0x0010_0004)]);
        assert!(w.god_rays.is_empty(), "HNAM is not WGDR");
        let none = parse_wthr(0x77, &[sub(b"EDID", b"SkyrimLE\0")], GameKind::Skyrim, &None);
        assert!(none.volumetric_lighting.is_empty(), "a weather without HNAM authors none (Skyrim.esm itself)");
        // Starfield's WTHR is a different schema: HNAM is not a VOLI list there.
        let sf = parse_wthr(0x77, &[sub(b"HNAM", &hnam)], GameKind::Starfield, &None);
        assert!(sf.volumetric_lighting.is_empty());
    }

    /// VLM-HF-011: the FO4 `GDRY` links decode: `WTHR` `WGDR` (eight time-of-day slots) and the
    /// `LGTM` `WGDR`; the `CELL` `XGDR` and `LIGH` `WGDR` have their own parser tests.
    #[test]
    fn parse_fo4_gdry_and_links() {
        use crate::esm::reader::GameKind;
        use crate::esm::records::{parse_lgtm, parse_wthr};
        let mut wgdr = Vec::new();
        for slot in 0..8u32 {
            wgdr.extend_from_slice(&(if slot == 3 { 0 } else { 0x0020_0000 + slot }).to_le_bytes());
        }
        let w = parse_wthr(0x88, &[sub(b"WGDR", &wgdr)], GameKind::Fallout4, &None);
        assert_eq!(w.god_rays.len(), 8);
        assert_eq!(w.god_rays[0], Some(0x0020_0000));
        assert_eq!(w.god_rays[3], None, "a NULL slot");
        assert_eq!(w.god_rays[7], Some(0x0020_0007));
        assert!(w.volumetric_lighting.is_empty());
        let l = parse_lgtm(0x99, &[sub(b"WGDR", 0x0030_0001u32.to_le_bytes())], GameKind::Fallout4, &None);
        assert_eq!(l.god_rays_form, Some(0x0030_0001));
    }
}
