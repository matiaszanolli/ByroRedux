//! `LGTM` lighting templates and `IMGS` image spaces.


use super::super::super::common::{remap_fid, CommonNamedFields};
use crate::esm::cell::StarfieldLighting;
use crate::esm::reader::{FormIdRemap, GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;
use byroredux_core::imagespace::ImageSpace;

/// `LGTM` `DATA` offset of the fog-far colour on the Skyrim-shaped body (FO4 / FO76 share it; xEdit `LGTM`
/// `DATA`): past the head through Fog Power at 36 and the unused 40-71 bundle.
const LGTM_FOG_FAR_COLOR_AT: usize = 72;

/// Lighting template (`LGTM`). Provides a named bundle of XCLL-shaped
/// lighting values that cells can reference via `LTMP` and selectively
/// override per-field. Full per-field inheritance fallback lands
/// alongside #379. Skyrim templates also carry the extended fog/fade
/// values in `DATA` and the directional ambient/specular bundle in
/// `DALC`; both are needed when CELL.XCLL selects template fields via
/// its inheritance mask.
///
/// The `DATA` sub-record mirrors XCLL byte-for-byte (bytes 0-39):
///   0-3:   ambient  (RGBA, byte order per cell.rs XCLL parser)
///   4-7:   directional (RGBA)
///   8-11:  fog color (RGBA)
///   12-15: fog near (f32)
///   16-19: fog far (f32)
///   20-23: rotation X (i32 degrees)
///   24-27: rotation Y (i32 degrees)
///   28-31: directional fade (f32)
///   32-35: fog clip (f32)
///   36-39: fog power (f32)
#[derive(Debug, Clone, Default)]
pub struct LgtmRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Ambient color normalised to [0, 1] RGB. XCLL uses RGB byte order
    /// (see `cell.rs` comment; corrected post-#389 revert).
    pub ambient: [f32; 3],
    pub directional: [f32; 3],
    pub fog_color: [f32; 3],
    pub fog_near: f32,
    pub fog_far: f32,
    pub directional_rotation: [f32; 2],
    pub directional_fade: Option<f32>,
    pub fog_clip: Option<f32>,
    pub fog_power: Option<f32>,
    pub fog_far_color: Option<[f32; 3]>,
    pub fog_max: Option<f32>,
    pub light_fade_begin: Option<f32>,
    pub light_fade_end: Option<f32>,
    pub directional_ambient: Option<[[f32; 3]; 6]>,
    pub specular_color: Option<[f32; 3]>,
    pub specular_alpha: Option<f32>,
    pub fresnel_power: Option<f32>,
    /// Starfield-only height-fog model from the 108-byte DATA tail
    /// (#5002) — same shape as the SF XCLL arm in `cell/walkers.rs`.
    pub starfield: Option<StarfieldLighting>,
    /// The height-fog tail (VLM-HF-004): FO4 / FO76 `DATA` bytes 92... Starfield's lives in
    /// [`Self::starfield`].
    pub height_fog: Option<crate::esm::cell::HeightFogTail>,
    /// FO4 `WGDR` "God Rays" (VLM-HF-011): the template's `GDRY` profile, load-order remapped.
    /// Decoded for the census, consumed by nothing (Volumetric Lighting PRD §10 Q8).
    pub god_rays_form: Option<u32>,
}

pub fn parse_lgtm(
    form_id: u32,
    subs: &[SubRecord],
    game: GameKind,
    remap: &Option<FormIdRemap>,
) -> LgtmRecord {
    let mut out = LgtmRecord {
        form_id,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs_with_remap(subs, remap);
    out.editor_id = common.editor_id;
    for sub in subs {
        match &sub.sub_type {
            b"DATA" if sub.data.len() >= 20 => {
                let mut r = SubReader::new(&sub.data);
                out.ambient = r.rgb_color().unwrap_or([0.0; 3]);
                out.directional = r.rgb_color().unwrap_or([0.0; 3]);
                out.fog_color = r.rgb_color().unwrap_or([0.0; 3]);
                out.fog_near = r.f32_or_default();
                out.fog_far = r.f32_or_default();
                let rot_x = (r.i32_or_default() as f32).to_radians();
                let rot_y = (r.i32_or_default() as f32).to_radians();
                out.directional_rotation = [rot_x, rot_y];
                out.directional_fade = r.f32().ok();
                out.fog_clip = r.f32().ok();
                out.fog_power = r.f32().ok();

                // Skyrim v34+ DATA mirrors XCLL except that its ambient
                // bundle at 40-71 is explicitly unused; the live ambient
                // cube/specular/fresnel values are in DALC below. The final
                // u32 (88-91) is likewise unused on LGTM.
                //
                // #5002 (SF-2026-09-29-D4-03) — Starfield's 108-byte DATA
                // is the SF XCLL shape (xEdit SF1 `wbDefinitionsSF1.pas`
                // LGTM.DATA), NOT Skyrim's tail: byte 28 is Gravity Scale
                // (not Directional Fade), 40-55 carry fog-far-colour /
                // fog-max / light fades, and 56-107 are the volumetric
                // height-fog model. Reading it with Skyrim's offsets pulled
                // fog_max from High Density Scale and the fades from the
                // dimensionless Fog Near/Far scales — which `normalize`
                // then lifted ×70 as light fades. All 6 vanilla
                // `Starfield.esm` LGTMs carry exactly 108 bytes.
                // #5346 — the tail is decoded by the one shared decoder
                // (`decode_starfield_lighting_tail`), the same bytes the SF
                // XCLL arm reads; the hand-copied offset table this replaced
                // was the drift risk.
                if game == GameKind::Starfield && sub.data.len() >= 108 {
                    out.directional_fade = None; // byte 28 is Gravity Scale on SF
                    let (tail_fields, starfield) =
                        crate::esm::cell::decode_starfield_lighting_tail(&sub.data[28..]);
                    out.fog_clip = Some(tail_fields.fog_clip);
                    out.fog_power = Some(tail_fields.fog_power);
                    out.fog_far_color = Some(tail_fields.fog_far_color);
                    out.fog_max = Some(tail_fields.fog_max);
                    out.light_fade_begin = Some(tail_fields.light_fade_begin);
                    out.light_fade_end = Some(tail_fields.light_fade_end);
                    out.starfield = Some(starfield);
                } else if sub.data.len() >= crate::esm::cell::XCLL_HEIGHT_FOG_TAIL_AT {
                    // The Skyrim-shaped body skips the unused 40-71 bundle: the fog-far
                    // colour starts at 72.
                    r.skip_to_or_eof(LGTM_FOG_FAR_COLOR_AT);
                    out.fog_far_color = r.rgb_color().ok();
                    out.fog_max = r.f32().ok();
                    out.light_fade_begin = r.f32().ok();
                    out.light_fade_end = r.f32().ok();
                    // The same tail as the XCLL's, from byte 92 (VLM-HF-004).
                    if matches!(game, GameKind::Fallout4 | GameKind::Fallout76) {
                        out.height_fog = crate::esm::cell::decode_creation_height_fog_tail(&sub.data);
                    }
                }
            }
            b"WGDR" if sub.data.len() >= 4 => {
                if let Ok(form) = SubReader::new(&sub.data).u32() {
                    out.god_rays_form = Some(remap_fid(form, remap));
                }
            }
            b"DALC" if sub.data.len() >= 24 => {
                let mut r = SubReader::new(&sub.data);
                let mut ambient_cube = [[0.0f32; 3]; 6];
                for face in &mut ambient_cube {
                    *face = r.rgb_color().unwrap_or([0.0; 3]);
                }
                out.directional_ambient = Some(ambient_cube);
                if let Ok(spec) = r.rgba_color() {
                    out.specular_color = Some([spec[0], spec[1], spec[2]]);
                    out.specular_alpha = Some(spec[3]);
                }
                out.fresnel_power = r.f32().ok();
            }
            _ => {}
        }
    }
    out
}

/// Image-space record (`IMGS`) — a cell's (or a weather slot's) base
/// colour grade. Interiors reference one through `CELL.XCIM`; FO3/FNV
/// exteriors through `WRLD.INAM`; Skyrim/FO4 exteriors through the
/// weather's per-time-of-day `WTHR.IMSP`.
///
/// #4416 — decoded at the parser boundary into the canonical
/// [`ImageSpace`] (cinematic saturation / brightness / contrast + tint),
/// so no consumer branches on the game. Layouts, from xEdit
/// (`wbDefinitions{FO3,FNV,TES5,FO4}.pas`, IMGS) checked against the
/// shipped masters:
///
/// - **FO3/FNV** keep everything in one `DNAM`, and its layout follows the
///   DNAM *size*, not the record's form version (xEdit gates "Skin
///   Dimmer" on form version 10, but the 148-byte version-11..13 records
///   do not carry it): 132 and 148 bytes put saturation @96, contrast
///   @104, brightness @108, tint RGB @112 and tint amount @124; 152 bytes
///   insert Skin Dimmer @56 and shift those to @100/@108/@112/@116/@128.
///   The contrast pivot ("Contrast Avg Lum Value") is not carried — the
///   presentation grade pivots on a fixed mid-grey.
/// - **Skyrim / FO4 / FO76** split the blocks: `CNAM` = saturation,
///   brightness, contrast; `TNAM` = tint amount then RGB; `DNAM` is depth
///   of field there. Skyrim's two `Default*` records carry only the
///   legacy 56-byte `ENAM` (7 HDR floats, the cinematic triple, then tint
///   amount + RGB per xEdit; UESP lists that tail as unknown).
/// - **Oblivion** has no IMGS (its HDR lives on `WTHR.HNAM`); Starfield's
///   is a reflection blob. Both decode to `None`.
#[derive(Debug, Clone, Default)]
pub struct ImgsRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// The decoded grade; `None` when the record carries no layout this
    /// decoder knows (see the type doc), which consumers treat as the
    /// identity grade.
    pub image_space: Option<ImageSpace>,
}

/// Parse an `IMGS` record into an [`ImgsRecord`] (#624, #4416).
pub fn parse_imgs(
    form_id: u32,
    subs: &[SubRecord],
    game: crate::esm::reader::GameKind,
) -> ImgsRecord {
    let common = CommonNamedFields::from_subs_with_remap(subs, &None);
    ImgsRecord {
        form_id,
        editor_id: common.editor_id,
        image_space: decode_image_space(subs, game),
    }
}

fn decode_image_space(
    subs: &[SubRecord],
    game: crate::esm::reader::GameKind,
) -> Option<ImageSpace> {
    use crate::esm::reader::GameKind;
    let floats = |data: &[u8]| -> Vec<f32> {
        data.as_chunks::<4>().0
            .iter()
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    };
    let find = |code: &[u8; 4]| subs.iter().find(|s| &s.sub_type == code).map(|s| &s.data);
    match game {
        GameKind::Fallout3NV => {
            let f = floats(find(b"DNAM")?);
            // Float indices of (saturation, contrast, brightness, tint rgb,
            // tint amount) per DNAM size; see the type doc.
            let (sat, con, bri, tint) = match f.len() * 4 {
                132 | 148 => (24, 26, 27, 28),
                152 => (25, 27, 28, 29),
                _ => return None,
            };
            Some(ImageSpace {
                saturation: f[sat],
                brightness: f[bri],
                contrast: f[con],
                tint_color: [f[tint], f[tint + 1], f[tint + 2], f[tint + 3]],
            })
        }
        GameKind::Skyrim | GameKind::Fallout4 | GameKind::Fallout76 => {
            let cinematic = find(b"CNAM").map(|d| floats(d)).filter(|f| f.len() >= 3);
            let tint = find(b"TNAM").map(|d| floats(d)).filter(|f| f.len() >= 4);
            if cinematic.is_none() && tint.is_none() {
                let legacy = find(b"ENAM").map(|d| floats(d)).filter(|f| f.len() >= 14)?;
                return Some(ImageSpace {
                    saturation: legacy[7],
                    brightness: legacy[8],
                    contrast: legacy[9],
                    tint_color: [legacy[11], legacy[12], legacy[13], legacy[10]],
                });
            }
            let mut out = ImageSpace::default();
            if let Some(c) = cinematic {
                (out.saturation, out.brightness, out.contrast) = (c[0], c[1], c[2]);
            }
            if let Some(t) = tint {
                out.tint_color = [t[1], t[2], t[3], t[0]];
            }
            Some(out)
        }
        GameKind::Oblivion | GameKind::Starfield => None,
    }
}
