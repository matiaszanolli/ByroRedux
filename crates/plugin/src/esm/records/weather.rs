//! WTHR (Weather) record parser.
//!
//! Weather records define sky appearance, fog distances, wind, sun parameters,
//! and cloud layers. Each worldspace references a default weather; the game
//! interpolates between weathers based on time of day and climate.
//!
//! FNV / FO3 layout (NAM0 = 240 bytes):
//!   10 color groups × 6 time-of-day slots × 4 bytes (RGBA u8).
//!   Groups, per tes5edit fopdoc (FNV + FO3 WTHR records — both games
//!   share this schema):
//!     0 sky_upper, 1 fog, 2 unused, 3 ambient, 4 sunlight, 5 sun,
//!     6 stars, 7 sky_lower, 8 horizon, 9 unused.
//!   Slots: sunrise, day, sunset, night, high_noon, midnight.
//!
//! Pre-fix the constants below were `sky_upper, fog, ambient, sunlight,
//! sun, stars, sky_lower, horizon, clouds_lower, clouds_upper` —
//! collapsing the index-2 "Unused" slot and renumbering every group
//! after it by one. The downstream `scene.rs` / `weather_system`
//! consumers therefore read junk from the real Unused slot for the
//! cell ambient term, the real Ambient slot for the directional
//! sunlight term, and the real Sky-Lower slot for the sky horizon
//! ring. The real Horizon slot (8) was never read at all because the
//! invented `clouds_lower` / `clouds_upper` slots had no consumer
//! (cloud colours come from the cloud TEXTURE paths in
//! DNAM/CNAM/ANAM/BNAM, not NAM0). See issue #729 / EXT-RENDER-1.

use super::common::{read_f32_at, read_zstring};
use crate::esm::reader::{GameKind, SubRecord};

/// Number of color groups in NAM0.
pub const SKY_COLOR_GROUPS: usize = 10;
/// Number of time-of-day slots per color group (FNV).
pub const SKY_TIME_SLOTS: usize = 6;

/// Color group indices into `sky_colors`. Indices match the tes5edit
/// fopdoc layout for FNV + FO3 verbatim — slots 2 and 9 are explicitly
/// "Unused" in the schema and exist here only as no-op placeholders so
/// the on-disk stride still walks 10 groups deep.
pub const SKY_UPPER: usize = 0;
pub const SKY_FOG: usize = 1;
/// Slot 2 is documented as "Unused" by tes5edit fopdoc. Authored data
/// occasionally appears here in vanilla weathers (e.g. NVWastelandClear)
/// but the original engine ignores it. Exposed so consumers that want
/// to dump the full table can still address it by name. See #729.
pub const SKY_UNUSED_2: usize = 2;
pub const SKY_AMBIENT: usize = 3;
pub const SKY_SUNLIGHT: usize = 4;
pub const SKY_SUN: usize = 5;
pub const SKY_STARS: usize = 6;
pub const SKY_LOWER: usize = 7;
pub const SKY_HORIZON: usize = 8;
/// Slot 9 — second documented "Unused" slot. Same rationale as
/// `SKY_UNUSED_2`.
pub const SKY_UNUSED_9: usize = 9;

/// Time-of-day slot indices.
pub const TOD_SUNRISE: usize = 0;
pub const TOD_DAY: usize = 1;
pub const TOD_SUNSET: usize = 2;
pub const TOD_NIGHT: usize = 3;
pub const TOD_HIGH_NOON: usize = 4;
pub const TOD_MIDNIGHT: usize = 5;

/// RGBA color from NAM0 sub-record (u8 per channel).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SkyColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl SkyColor {
    /// Convert to f32 RGB in raw monitor-space (no gamma correction).
    ///
    /// Per commit `0e8efc6`: Gamebryo / Bethesda ESM colors (LIGH, XCLL, NIF
    /// material, WTHR NAM0) are authored as raw monitor-space floats. The
    /// engine feeds them directly into the ACES tone mapper — applying an
    /// sRGB→linear decode darkens every warm hue (e.g. 0.78 → 0.58).
    pub fn to_rgb_f32(&self) -> [f32; 3] {
        [
            self.r as f32 / 255.0,
            self.g as f32 / 255.0,
            self.b as f32 / 255.0,
        ]
    }
}

/// Weather classification flags.
pub const WTHR_PLEASANT: u8 = 0x01;
pub const WTHR_CLOUDY: u8 = 0x02;
pub const WTHR_RAINY: u8 = 0x04;
pub const WTHR_SNOW: u8 = 0x08;

/// Oblivion WTHR HNAM sub-record — 14 f32 HDR + lighting tuning
/// parameters (56 bytes total). Names + offsets per UESP `Oblivion_Mod:WTHR`.
///
/// These are NOT fog distances, despite the pre-#537 parser's
/// interpretation: the first 4 f32 are `eye_adapt_speed`, `blur_radius`,
/// `blur_passes`, `emissive_mult` — reading them as `[fog_day_near,
/// fog_day_far, fog_night_near, fog_night_far]` saturated every
/// Oblivion exterior to solid fog within a few units of the camera.
/// Fog distances come from FNAM (16 bytes) on Oblivion; HNAM feeds
/// the renderer's eye-adaptation / bloom / scene-dimmer pipeline.
///
/// FNV and Fallout 3 do not ship HNAM at all — their fog comes from
/// FNAM + cloud textures from DNAM/CNAM/ANAM/BNAM. See audit M33-05.
///
/// Consumer wiring (HDR eye-adaptation system) is follow-up work —
/// this captures the authored values verbatim so the future bloom /
/// sunlight-dimmer system has a canonical source.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct OblivionHdrLighting {
    /// Rate the scene luminance meter tracks toward the target (0–65536).
    pub eye_adapt_speed: f32,
    /// Bloom blur kernel radius in pixels (0–7).
    pub blur_radius: f32,
    /// Bloom blur iteration count (0–63).
    pub blur_passes: f32,
    /// Emissive pass multiplier — scales self-illum contributions.
    pub emissive_mult: f32,
    /// Target luminance the eye-adapt loop drives toward.
    pub target_lum: f32,
    /// Upper clamp on the scene luminance reading (prevents runaway
    /// adaptation when a bright light enters the view frustum).
    pub upper_lum_clamp: f32,
    /// Bright-pass scale — boosts highlights before the blur kernel.
    pub bright_scale: f32,
    /// Bright-pass clamp on the scaled output.
    pub bright_clamp: f32,
    /// Fallback luminance ramp value when no luminance texture is
    /// available (first-frame / LOD transitions).
    pub lum_ramp_no_tex: f32,
    /// Luminance ramp minimum (eye-adapt floor).
    pub lum_ramp_min: f32,
    /// Luminance ramp maximum (eye-adapt ceiling).
    pub lum_ramp_max: f32,
    /// Directional-sunlight colour multiplier for this weather.
    pub sunlight_dimmer: f32,
    /// Grass colour multiplier (Oblivion-specific — applied at
    /// grass-generator stage before the scene lighting pass).
    pub grass_dimmer: f32,
    /// Tree colour multiplier — same role as `grass_dimmer` for
    /// foliage meshes.
    pub tree_dimmer: f32,
}

/// Skyrim+ directional-ambient lighting cube — one 32-byte entry per
/// TOD slot from the DALC sub-record (4 entries: sunrise / day / sunset
/// / night). Each entry is a 6-axis ambient probe + a "specular" colour +
/// a final f32 fresnel-power tail.
///
/// Wire layout per UESP / xEdit `DALC` struct:
/// ```text
///   bytes  0..4   = +X ambient (R G B 0)   (east / right)
///   bytes  4..8   = -X ambient (R G B 0)   (west / left)
///   bytes  8..12  = +Y ambient (R G B 0)   (north / forward)
///   bytes 12..16  = -Y ambient (R G B 0)   (south / back)
///   bytes 16..20  = +Z ambient (R G B 0)   (up)
///   bytes 20..24  = -Z ambient (R G B 0)   (down / ground)
///   bytes 24..28  = specular colour (R G B 0)
///   bytes 28..32  = fresnel power (f32) — typically 1.0
/// ```
///
/// Engine consumption: the 6 ambient axes drive a Skyrim-style 6-axis
/// directional ambient cube (sky / ground / cardinal-direction fill)
/// for diffuse shading in lieu of the single flat ambient value used
/// on FNV/FO3. The renderer consumer is follow-up work — this struct
/// captures the authored bytes so a Skyrim cell render can sample
/// real per-TOD ambient cubes instead of the procedural single-colour
/// fallback. See #539 / M33-04..07 follow-up.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SkyrimAmbientCube {
    /// `+X` ambient (engine east / right).
    pub pos_x: SkyColor,
    /// `-X` ambient (engine west / left).
    pub neg_x: SkyColor,
    /// `+Y` ambient (engine north / forward — Y-up engine).
    pub pos_y: SkyColor,
    /// `-Y` ambient (engine south / back).
    pub neg_y: SkyColor,
    /// `+Z` ambient (engine up — sky-fill colour).
    pub pos_z: SkyColor,
    /// `-Z` ambient (engine down — ground-fill colour).
    pub neg_z: SkyColor,
    /// DALC specular colour — feeds the Skyrim-specific specular tint
    /// on metallic / wet materials. Renderer consumer is follow-up
    /// work alongside the 6-axis ambient consumer.
    pub specular: SkyColor,
    /// DALC fresnel power tail. Vanilla Skyrim ships `1.0` here;
    /// captured verbatim so a future renderer-side consumer can
    /// drive the fresnel exponent from authored data instead of a
    /// hardcoded constant.
    pub fresnel_power: f32,
}

/// Parsed WTHR record.
#[derive(Debug, Clone)]
pub struct WeatherRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// 10 color groups × 6 time-of-day slots. Indexed by SKY_* and TOD_* constants.
    pub sky_colors: [[SkyColor; SKY_TIME_SLOTS]; SKY_COLOR_GROUPS],
    /// Fog day near distance (game units).
    pub fog_day_near: f32,
    /// Fog day far distance (game units).
    pub fog_day_far: f32,
    /// Fog night near distance (game units).
    pub fog_night_near: f32,
    /// Fog night far distance (game units).
    pub fog_night_far: f32,
    /// Wind speed (0–255 scaled).
    pub wind_speed: u8,
    /// Sun glare intensity (0–255 scaled).
    pub sun_glare: u8,
    /// Sun damage factor (0–255).
    pub sun_damage: u8,
    /// Weather classification flags (WTHR_PLEASANT | WTHR_CLOUDY | ...).
    pub classification: u8,
    /// Cloud texture paths. FNV/FO3 ship 4 layers (DNAM/CNAM/ANAM/BNAM
    /// = layers 0–3 in schema-emission order); Oblivion ships 2
    /// (DNAM = 0, CNAM = 1). Paths are `textures\`-root-relative
    /// zstrings — see #468 for the normaliser in `TextureProvider`.
    pub cloud_textures: [Option<String>; 4],
    /// Oblivion-only HDR / eye-adapt / sunlight-dimmer tuning from the
    /// 56-byte HNAM sub-record. `None` for FNV / FO3 / Skyrim+ weather
    /// records, and for Oblivion records that ship a malformed HNAM.
    /// See audit M33-05 / #537.
    pub oblivion_hdr: Option<OblivionHdrLighting>,
    /// Skyrim-only 6-axis directional ambient cubes, one per TOD slot
    /// (sunrise / day / sunset / night). Populated from the four DALC
    /// sub-records on Skyrim WTHR records; `None` on FNV / FO3 /
    /// Oblivion / FO4+ records (different ambient model).
    /// Renderer consumer is follow-up work — captured here so a Skyrim
    /// cell render can later drive 6-axis ambient fill instead of the
    /// procedural single-colour fallback. See #539 / M33-04..07.
    pub skyrim_ambient_cube: Option<[SkyrimAmbientCube; 4]>,
}

impl Default for WeatherRecord {
    fn default() -> Self {
        Self {
            form_id: 0,
            editor_id: String::new(),
            sky_colors: [[SkyColor::default(); SKY_TIME_SLOTS]; SKY_COLOR_GROUPS],
            fog_day_near: 0.0,
            fog_day_far: 10000.0,
            fog_night_near: 0.0,
            fog_night_far: 10000.0,
            wind_speed: 0,
            sun_glare: 0,
            sun_damage: 0,
            classification: 0,
            cloud_textures: [None, None, None, None],
            oblivion_hdr: None,
            skyrim_ambient_cube: None,
        }
    }
}

/// Parse a WTHR record from its sub-records.
///
/// Game-specific schema dispatch:
/// - **Oblivion / FO3 / FNV / FO4 / FO76 / Starfield** — Gamebryo/FO3-
///   era layout. 10-group × 6-TOD NAM0 (160 or 240 bytes on disk;
///   synthesise HIGH_NOON / MIDNIGHT when the on-disk record only
///   ships 4 slots — see #533).
/// - **Skyrim** — re-routes to [`parse_wthr_skyrim`], which handles
///   the 17-group × 4-TOD NAM0 (272 B), 8-float FNAM, 19-B DATA, and
///   4× 32-B DALC ambient cube. See #539 / M33-04..07.
pub fn parse_wthr(form_id: u32, subs: &[SubRecord], game: GameKind) -> WeatherRecord {
    if matches!(game, GameKind::Skyrim) {
        return parse_wthr_skyrim(form_id, subs);
    }

    let mut record = WeatherRecord {
        form_id,
        ..WeatherRecord::default()
    };

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),

            // NAM0: sky colors. Two on-disk strides exist:
            //   - 240 B = 10 groups × 6 TOD slots × 4 B (FNV default + later games):
            //     SUNRISE, DAY, SUNSET, NIGHT, HIGH_NOON, MIDNIGHT.
            //   - 160 B = 10 groups × 4 TOD slots × 4 B (Oblivion, FO3, some older /
            //     DLC FNV records): SUNRISE, DAY, SUNSET, NIGHT only. HIGH_NOON and
            //     MIDNIGHT are synthesised from DAY and NIGHT respectively so the
            //     `[[SkyColor; 6]; 10]` struct layout remains authoritative for
            //     downstream consumers (`weather_system` 7-key interpolator,
            //     `build_tod_keys`). See #533 / audit M33-01.
            b"NAM0" if sub.data.len() >= SKY_COLOR_GROUPS * 4 * 4 => {
                let on_disk_slots = if sub.data.len() >= SKY_COLOR_GROUPS * SKY_TIME_SLOTS * 4 {
                    SKY_TIME_SLOTS
                } else {
                    4
                };
                let mut offset = 0;
                for group in 0..SKY_COLOR_GROUPS {
                    for slot in 0..on_disk_slots {
                        record.sky_colors[group][slot] = SkyColor {
                            r: sub.data[offset],
                            g: sub.data[offset + 1],
                            b: sub.data[offset + 2],
                            a: sub.data[offset + 3],
                        };
                        offset += 4;
                    }
                    if on_disk_slots < SKY_TIME_SLOTS {
                        record.sky_colors[group][TOD_HIGH_NOON] = record.sky_colors[group][TOD_DAY];
                        record.sky_colors[group][TOD_MIDNIGHT] =
                            record.sky_colors[group][TOD_NIGHT];
                    }
                }
            }

            // FNAM: fog distances. Four f32 at offsets 0/4/8/12:
            // `[day_near, day_far, night_near, night_far]`. Game-dependent
            // total size: 16 B in Oblivion, 24 B in FNV/FO3 (trailing 8 B
            // have not been cross-checked against a UESP-authoritative
            // schema so they are ignored here).
            //
            // Pre-fix the FNAM arm had an empty body with a comment
            // ("fallback when HNAM is absent"). But FNV and FO3 do not
            // ship HNAM — they ship only FNAM. The result was that every
            // FNV + FO3 weather defaulted to `fog_day_far = 10000.0`
            // independent of weather type. See audit M33-04 / #536.
            b"FNAM" if sub.data.len() >= 16 => {
                record.fog_day_near = read_f32_at(&sub.data, 0).unwrap_or(0.0);
                record.fog_day_far = read_f32_at(&sub.data, 4).unwrap_or(10000.0);
                record.fog_night_near = read_f32_at(&sub.data, 8).unwrap_or(0.0);
                record.fog_night_far = read_f32_at(&sub.data, 12).unwrap_or(10000.0);
            }

            // HNAM — Oblivion-only HDR / eye-adapt / sunlight-dimmer
            // tuning. 56 bytes = 14 × f32. See #537 / audit M33-05 and
            // the `OblivionHdrLighting` doc comment for the field
            // order. Fog for Oblivion comes from FNAM (handled above),
            // NEVER from HNAM. FNV + FO3 do not ship HNAM.
            //
            // Pre-#537 the parser read the first 16 bytes as fog
            // distances (`[fog_day_near, fog_day_far, fog_night_near,
            // fog_night_far]`), picking up the HDR params
            // `[eye_adapt_speed, blur_radius, blur_passes,
            // emissive_mult]` instead — which produced
            // `fog_day_far ≈ 4.0` on every Oblivion exterior and
            // saturated the scene to solid fog within a few units of
            // the camera.
            //
            // The 16-byte legacy fixture path is retained behind a
            // narrower gate (`== 16`) so synthetic tests that use a
            // 16-byte HNAM as a fog source keep compiling; real
            // Oblivion masters ship only the 56-byte form.
            b"HNAM" if sub.data.len() == 56 => {
                // MILESTONE: M-LIGHT v2 (HDR sky / cloud relighting) — see #1057.
                // Decoded today (all 14 f32 fields populated) but `weather_system`
                // in `byroredux/src/systems/weather.rs` only reads the SDR
                // colors. Wire the HDR fields when the renderer side ships
                // HDR sky/cloud relighting.
                record.oblivion_hdr = Some(OblivionHdrLighting {
                    eye_adapt_speed: read_f32_at(&sub.data, 0).unwrap_or(0.0),
                    blur_radius: read_f32_at(&sub.data, 4).unwrap_or(0.0),
                    blur_passes: read_f32_at(&sub.data, 8).unwrap_or(0.0),
                    emissive_mult: read_f32_at(&sub.data, 12).unwrap_or(0.0),
                    target_lum: read_f32_at(&sub.data, 16).unwrap_or(0.0),
                    upper_lum_clamp: read_f32_at(&sub.data, 20).unwrap_or(0.0),
                    bright_scale: read_f32_at(&sub.data, 24).unwrap_or(0.0),
                    bright_clamp: read_f32_at(&sub.data, 28).unwrap_or(0.0),
                    lum_ramp_no_tex: read_f32_at(&sub.data, 32).unwrap_or(0.0),
                    lum_ramp_min: read_f32_at(&sub.data, 36).unwrap_or(0.0),
                    lum_ramp_max: read_f32_at(&sub.data, 40).unwrap_or(0.0),
                    sunlight_dimmer: read_f32_at(&sub.data, 44).unwrap_or(0.0),
                    grass_dimmer: read_f32_at(&sub.data, 48).unwrap_or(0.0),
                    tree_dimmer: read_f32_at(&sub.data, 52).unwrap_or(0.0),
                });
            }
            b"HNAM" if sub.data.len() == 16 => {
                // Legacy fixture path — synthetic tests pre-#537 built
                // 16-byte HNAMs and used them as fog sources. Preserved
                // so the M33-05 regression test (which asserts a real
                // 56-byte HNAM does NOT clobber FNAM fog) keeps its
                // counterpart. No vanilla master ships this shape.
                record.fog_day_near = read_f32_at(&sub.data, 0).unwrap_or(0.0);
                record.fog_day_far = read_f32_at(&sub.data, 4).unwrap_or(10000.0);
                record.fog_night_near = read_f32_at(&sub.data, 8).unwrap_or(0.0);
                record.fog_night_far = read_f32_at(&sub.data, 12).unwrap_or(10000.0);
            }

            // DATA: general weather data (15 bytes, shared by Oblivion,
            // FO3, FNV). Byte-confirmed layout from #538 / audit M33-06:
            //   [ 0]    wind_speed
            //   [ 1- 2] cloud speed lower / upper (unparsed — no consumer)
            //   [ 3]    trans_delta (unparsed)
            //   [ 4]    sun_glare
            //   [ 5]    sun_damage
            //   [ 6- 9] precipitation / thunder fade params (unparsed)
            //   [10]    reserved / unknown (0xFF on every PLEASANT sample;
            //                                varies on RAINY)
            //   [11]    classification flag byte (WTHR_PLEASANT=0x01,
            //           CLOUDY=0x02, RAINY=0x04, SNOW=0x08)
            //   [12-14] lightning color (RGB) — `ff ff ff` on RAINY, zero
            //           or 0xFF on non-rainy
            //
            // Pre-fix the parser read classification from byte 13 — a
            // zero / padding byte on nearly every record. Verified the
            // new offset against 4 flag bits on Oblivion (Clear/Cloudy/
            // Rain/Snow) and 3 on FNV (0x00/0x01/0x02).
            b"DATA" if sub.data.len() >= 15 => {
                record.wind_speed = sub.data[0];
                record.sun_glare = sub.data[4];
                record.sun_damage = sub.data[5];
                record.classification = sub.data[11];
            }

            // Cloud texture paths. Per-game sub-record FourCCs — verified
            // by byte-level scan of FalloutNV.esm, Fallout3.esm,
            // Oblivion.esm (docs/audits/AUDIT_M33_2026-04-21.md §M33-02):
            //   FNV / FO3 (4 layers, schema-emission order):
            //     DNAM = layer 0 (base)  CNAM = layer 1
            //     ANAM = layer 2         BNAM = layer 3 (top)
            //   Oblivion (2 layers):
            //     DNAM = layer 0         CNAM = layer 1
            // The pre-fix parser matched `00TX/10TX/20TX/30TX` which
            // doesn't appear in any shipped master (M33-02) and read
            // DNAM as `[u8; 4]` cloud speeds (M33-03, #535) — DNAM was
            // claimed both ways and ended up neither.
            //
            // Bodies are `textures\`-root-relative zstrings; #468's
            // `TextureProvider` normaliser prepends the prefix.
            b"DNAM" => record.cloud_textures[0] = Some(read_zstring(&sub.data)),
            b"CNAM" => record.cloud_textures[1] = Some(read_zstring(&sub.data)),
            b"ANAM" => record.cloud_textures[2] = Some(read_zstring(&sub.data)),
            b"BNAM" => record.cloud_textures[3] = Some(read_zstring(&sub.data)),

            _ => {}
        }
    }

    record
}

/// Number of NAM0 colour groups on a Skyrim WTHR record's on-disk
/// layout. Reading more reveals the extras (Sky Glare / Cloud LOD /
/// Effects Lighting / Moon Glare) that the engine doesn't yet
/// consume; the first 9 groups align byte-for-byte with the existing
/// `SKY_UPPER..SKY_HORIZON` constants used by `weather_system` so we
/// slot them straight into the shared `[10][6]` array.
const SKYRIM_NAM0_GROUPS: usize = 17;

/// Number of TOD slots Skyrim ships on disk per NAM0 group — sunrise /
/// day / sunset / night. The engine's existing TOD model carries 6
/// slots (HIGH_NOON / MIDNIGHT synthesised from DAY / NIGHT — same
/// pattern Oblivion uses, see #533).
const SKYRIM_NAM0_TOD_SLOTS: usize = 4;

/// Skyrim WTHR DALC sub-record size (32 bytes). Per UESP: 6 RGB+pad
/// ambient axes + 1 RGB+pad specular colour + 1 f32 fresnel power.
const SKYRIM_DALC_SIZE: usize = 32;

/// Skyrim WTHR FNAM sub-record size (32 bytes = 8 × f32). Layout per
/// UESP / xEdit:
/// `[day_near, day_far, night_near, night_far, day_power, night_power,
///   day_max, night_max]`. v1 only consumes the first four
/// (distances); the power / max fields go to follow-up renderer wiring
/// once the volumetric fog path can honour them.
const SKYRIM_FNAM_SIZE: usize = 32;

/// Skyrim WTHR DATA sub-record size (19 bytes). Holds wind speed,
/// transition timings, sun glare / damage, precipitation / thunder
/// fade, classification, and lightning colour. v1 extracts wind +
/// classification; the rest is captured-on-disk-only and waits for
/// the precipitation / sun-damage gameplay consumer to land.
const SKYRIM_DATA_SIZE: usize = 19;

/// Parse a Skyrim WTHR record. Called by [`parse_wthr`] when
/// `game == GameKind::Skyrim`; mirrors the FO3-era branch's shape but
/// handles the Skyrim-specific NAM0 stride (17 groups × 4 TOD slots
/// = 272 B), 32-byte FNAM (8 floats), 19-byte DATA, and the 4× DALC
/// sub-record set that ships the 6-axis directional ambient cube.
///
/// **NAM0 mapping**: the first 9 of Skyrim's 17 groups align with the
/// FO3-era group constants (`SKY_UPPER` .. `SKY_HORIZON`) — sky upper,
/// fog near, unknown, ambient, sunlight, sun, stars, sky lower,
/// horizon. Groups 9..17 carry Skyrim-exclusive data (Effects
/// Lighting, Cloud LOD Diffuse / Ambient, Fog Far, Sky Statics, Water
/// Multiplier, Sun Glare, Moon Glare) — captured into the 10-group
/// shared array's slot 9 for "Effects Lighting" but the remaining 7
/// groups are discarded for v1 because the renderer / weather_system
/// has no consumer for them yet. Follow-up work covers the Sky Glare
/// + Cloud LOD wiring when the renderer's M-glare / cloud-cover work
/// surfaces. See M33-04..07.
///
/// **TOD synth**: Skyrim ships 4 TOD slots per group (sunrise / day /
/// sunset / night); HIGH_NOON and MIDNIGHT are synthesised from DAY
/// and NIGHT respectively so the shared `[group][6]` table stays
/// authoritative for `weather_system`. Mirrors the Oblivion / FO3
/// short-NAM0 fallback in [`parse_wthr`].
///
/// **Ambient cube**: the 4× DALC sub-records populate
/// [`WeatherRecord::skyrim_ambient_cube`] (one entry per TOD slot)
/// so a future renderer-side 6-axis ambient consumer has authored
/// per-direction values to drive diffuse shading instead of falling
/// back to a single procedural ambient. See [`SkyrimAmbientCube`].
fn parse_wthr_skyrim(form_id: u32, subs: &[SubRecord]) -> WeatherRecord {
    let mut record = WeatherRecord {
        form_id,
        ..WeatherRecord::default()
    };

    // DALC entries arrive in TOD order (sunrise, day, sunset, night).
    // Accumulate into a temporary array so out-of-order or partial
    // records don't corrupt earlier slots.
    let mut dalc_buf: [Option<SkyrimAmbientCube>; 4] = [None; 4];
    let mut dalc_idx: usize = 0;

    for sub in subs {
        match &sub.sub_type {
            b"EDID" => record.editor_id = read_zstring(&sub.data),

            // NAM0 — 17 groups × 4 TOD slots × 4 bytes RGBA = 272 B.
            // First 9 groups align with the FO3-era constants and slot
            // straight into the shared `[10][6]` array. Groups 9..17
            // are Skyrim-exclusive (Effects Lighting / Cloud LOD /
            // Fog Far / Sky Statics / Water Multiplier / Sun Glare /
            // Moon Glare); slot the 10th group (Effects Lighting) into
            // `SKY_UNUSED_9` so a future consumer has it without
            // schema churn, and discard the rest for v1.
            b"NAM0"
                if sub.data.len()
                    >= SKYRIM_NAM0_GROUPS * SKYRIM_NAM0_TOD_SLOTS * 4 =>
            {
                let mut offset = 0;
                for group in 0..SKYRIM_NAM0_GROUPS {
                    for slot in 0..SKYRIM_NAM0_TOD_SLOTS {
                        // Skyrim ships sunrise / day / sunset / night
                        // in the same TOD-index order as the engine's
                        // `TOD_SUNRISE`..`TOD_NIGHT` constants.
                        let color = SkyColor {
                            r: sub.data[offset],
                            g: sub.data[offset + 1],
                            b: sub.data[offset + 2],
                            a: sub.data[offset + 3],
                        };
                        if group < SKY_COLOR_GROUPS {
                            record.sky_colors[group][slot] = color;
                        }
                        offset += 4;
                    }
                    // Synthesise HIGH_NOON / MIDNIGHT for the first
                    // 10 groups so the shared `[group][6]` table is
                    // dense for `weather_system`. Mirrors the Oblivion
                    // / FO3 short-NAM0 fallback above.
                    if group < SKY_COLOR_GROUPS {
                        record.sky_colors[group][TOD_HIGH_NOON] =
                            record.sky_colors[group][TOD_DAY];
                        record.sky_colors[group][TOD_MIDNIGHT] =
                            record.sky_colors[group][TOD_NIGHT];
                    }
                }
            }

            // FNAM — 32 bytes = 8 × f32 on Skyrim. First 4 are fog
            // distances (compatible with the FO3-era 4-distance
            // model). Trailing 4 are day/night fog power + max —
            // captured in a follow-up; v1 discards.
            b"FNAM" if sub.data.len() >= SKYRIM_FNAM_SIZE => {
                record.fog_day_near = read_f32_at(&sub.data, 0).unwrap_or(0.0);
                record.fog_day_far = read_f32_at(&sub.data, 4).unwrap_or(10000.0);
                record.fog_night_near = read_f32_at(&sub.data, 8).unwrap_or(0.0);
                record.fog_night_far = read_f32_at(&sub.data, 12).unwrap_or(10000.0);
            }

            // DATA — 19 bytes. v1 extracts wind speed + classification.
            // Byte 0 = wind. Byte 14 = classification flag bitmask
            // (WTHR_PLEASANT | WTHR_CLOUDY | WTHR_RAINY | WTHR_SNOW).
            // Bytes 1..14 are transition timings + sun glare + sun
            // damage + precip / thunder fade — captured-on-disk-only
            // for the gameplay consumer.
            b"DATA" if sub.data.len() >= SKYRIM_DATA_SIZE => {
                record.wind_speed = sub.data[0];
                record.classification = sub.data[14];
            }

            // DALC — 32 bytes per entry, 4 entries (one per TOD slot).
            // Captured into `skyrim_ambient_cube`. Out-of-order or
            // extra entries clamp at 4 — Bethesda content always ships
            // exactly 4 in sunrise/day/sunset/night order per UESP.
            b"DALC" if sub.data.len() >= SKYRIM_DALC_SIZE => {
                if dalc_idx < dalc_buf.len() {
                    let cube = parse_skyrim_dalc(&sub.data);
                    dalc_buf[dalc_idx] = Some(cube);
                    dalc_idx += 1;
                }
            }

            // Other sub-records (LNAM, MNAM, NNAM, RNAM, QNAM, PNAM,
            // JNAM, NAM1, TNAM ×many, IMSP, 00TX..L0TX) carry data
            // the renderer doesn't yet consume:
            //   - Cloud-layer enable mask + per-layer colours /
            //     alphas / speeds — wiring up the 32-layer cloud
            //     pipeline is a separate effort (vs FNV's 4 layers).
            //   - Aurora data (NAM1) — Skyrim-exclusive volumetric.
            //   - ImageSpace references (IMSP) — needs the ImageSpace
            //     record parser to land first.
            //   - 00TX..L0TX cloud texture paths — paired with the
            //     32-layer pipeline above.
            // Captured-on-disk-only for the moment; the silent skip
            // here is intentional rather than a regression. Follow-up
            // tracking issues will surface each sub-record's wiring.
            _ => {}
        }
    }

    // Promote the 4 DALC slots into the WeatherRecord when at least
    // one was authored. All-None stays as `None` so the consumer can
    // tell apart "no DALC authored" from "DALC authored with zero
    // values everywhere" (the latter is valid for some Skyrim
    // weathers).
    if dalc_buf.iter().any(|d| d.is_some()) {
        // Fill missing slots with the most recent one (Bethesda
        // content always ships all 4, but defensive against truncated
        // mod records).
        let mut last = SkyrimAmbientCube::default();
        let mut filled: [SkyrimAmbientCube; 4] = [SkyrimAmbientCube::default(); 4];
        for (i, slot) in dalc_buf.iter().enumerate() {
            if let Some(s) = slot {
                last = *s;
            }
            filled[i] = last;
        }
        record.skyrim_ambient_cube = Some(filled);
    }

    record
}

/// Parse one 32-byte DALC sub-record body into a [`SkyrimAmbientCube`].
/// Layout per UESP — 6 RGB+pad axes, 1 RGB+pad specular, 1 f32 fresnel.
fn parse_skyrim_dalc(data: &[u8]) -> SkyrimAmbientCube {
    debug_assert!(data.len() >= SKYRIM_DALC_SIZE);
    let read_color = |off: usize| SkyColor {
        r: data[off],
        g: data[off + 1],
        b: data[off + 2],
        a: data[off + 3],
    };
    SkyrimAmbientCube {
        pos_x: read_color(0),
        neg_x: read_color(4),
        pos_y: read_color(8),
        neg_y: read_color(12),
        pos_z: read_color(16),
        neg_z: read_color(20),
        specular: read_color(24),
        fresnel_power: read_f32_at(data, 28).unwrap_or(1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::reader::{GameKind, SubRecord};

    fn make_sub(sub_type: &[u8; 4], data: Vec<u8>) -> SubRecord {
        SubRecord {
            sub_type: *sub_type,
            data,
        }
    }

    #[test]
    fn parse_wthr_basic() {
        let mut nam0_data = vec![0u8; 240];
        // Set sky_upper/day slot (group 0, slot 1) to (100, 150, 200, 255).
        let offset = (0 * SKY_TIME_SLOTS + TOD_DAY) * 4;
        nam0_data[offset] = 100;
        nam0_data[offset + 1] = 150;
        nam0_data[offset + 2] = 200;
        nam0_data[offset + 3] = 255;

        // Set horizon/day (group 8 per #729, slot 1).
        let offset = (SKY_HORIZON * SKY_TIME_SLOTS + TOD_DAY) * 4;
        nam0_data[offset] = 180;
        nam0_data[offset + 1] = 170;
        nam0_data[offset + 2] = 140;
        nam0_data[offset + 3] = 255;

        let mut hnam_data = vec![0u8; 16];
        hnam_data[0..4].copy_from_slice(&500.0_f32.to_le_bytes());
        hnam_data[4..8].copy_from_slice(&15000.0_f32.to_le_bytes());
        hnam_data[8..12].copy_from_slice(&200.0_f32.to_le_bytes());
        hnam_data[12..16].copy_from_slice(&8000.0_f32.to_le_bytes());

        let mut data_bytes = vec![0u8; 15];
        data_bytes[0] = 30; // wind speed
        data_bytes[4] = 128; // sun glare
        data_bytes[5] = 10; // sun damage
                            // Classification sits at byte 11. See #538 / audit M33-06 —
                            // pre-fix the parser read byte 13 (padding).
        data_bytes[11] = WTHR_PLEASANT;

        let subs = vec![
            make_sub(b"EDID", b"TestWeather\0".to_vec()),
            make_sub(b"NAM0", nam0_data),
            make_sub(b"HNAM", hnam_data),
            make_sub(b"DATA", data_bytes),
            // Cloud layers in schema-emission order (DNAM/CNAM/ANAM/BNAM).
            // See #534 / audit M33-02 for the FourCC-to-layer mapping.
            make_sub(b"DNAM", b"sky\\clouds_00_base.dds\0".to_vec()),
            make_sub(b"CNAM", b"sky\\clouds_01.dds\0".to_vec()),
            make_sub(b"ANAM", b"sky\\clouds_02.dds\0".to_vec()),
            make_sub(b"BNAM", b"sky\\clouds_03_top.dds\0".to_vec()),
        ];

        let w = parse_wthr(0x1234, &subs, GameKind::Fallout3NV);
        assert_eq!(w.form_id, 0x1234);
        assert_eq!(w.editor_id, "TestWeather");

        // Sky upper / day
        let upper_day = w.sky_colors[SKY_UPPER][TOD_DAY];
        assert_eq!((upper_day.r, upper_day.g, upper_day.b), (100, 150, 200));

        // Horizon / day
        let horiz_day = w.sky_colors[SKY_HORIZON][TOD_DAY];
        assert_eq!((horiz_day.r, horiz_day.g, horiz_day.b), (180, 170, 140));

        // Fog
        assert!((w.fog_day_near - 500.0).abs() < 0.01);
        assert!((w.fog_day_far - 15000.0).abs() < 0.01);
        assert!((w.fog_night_near - 200.0).abs() < 0.01);
        assert!((w.fog_night_far - 8000.0).abs() < 0.01);

        // General data
        assert_eq!(w.wind_speed, 30);
        assert_eq!(w.sun_glare, 128);
        assert_eq!(w.classification, WTHR_PLEASANT);

        // Cloud layer paths — DNAM/CNAM/ANAM/BNAM = 0/1/2/3 (#534).
        assert_eq!(
            w.cloud_textures[0].as_deref(),
            Some("sky\\clouds_00_base.dds"),
        );
        assert_eq!(w.cloud_textures[1].as_deref(), Some("sky\\clouds_01.dds"));
        assert_eq!(w.cloud_textures[2].as_deref(), Some("sky\\clouds_02.dds"));
        assert_eq!(
            w.cloud_textures[3].as_deref(),
            Some("sky\\clouds_03_top.dds"),
        );
    }

    /// Regression for #534 / audit M33-02: the pre-fix parser matched
    /// `00TX/10TX/20TX/30TX` which never appear in any shipped master.
    /// Guard: those FourCCs should NOT be recognised — a WTHR that only
    /// contains them produces zero cloud layers.
    #[test]
    fn parse_wthr_00tx_fourccs_are_inert() {
        let subs = vec![
            make_sub(b"EDID", b"StaleFourCCs\0".to_vec()),
            make_sub(b"00TX", b"sky\\stale.dds\0".to_vec()),
            make_sub(b"10TX", b"sky\\stale.dds\0".to_vec()),
        ];
        let w = parse_wthr(0xFADE, &subs, GameKind::Fallout3NV);
        assert!(w.cloud_textures.iter().all(|c| c.is_none()));
    }

    /// Regression for #538 + #543 / audit M33-06: classification sits
    /// at DATA byte 11, not byte 13. Byte-confirmed against 4 flag
    /// values on Oblivion (Clear/Cloudy/Rain/Snow) and 3 on FNV
    /// (0x00/0x01/0x02). Pre-fix the parser read byte 13 — a padding
    /// byte that read back as 0x00 or 0xFF on nearly every weather,
    /// making the `classification` field effectively unusable for any
    /// future downstream consumer.
    #[test]
    fn parse_wthr_classification_is_byte_11() {
        // Build DATA with unique sentinels at every byte so an offset
        // slip shows up as a wrong value rather than a lucky zero.
        let mut data_bytes = vec![0u8; 15];
        for (i, slot) in data_bytes.iter_mut().enumerate() {
            *slot = 0xA0 + i as u8;
        }
        // Override the fields the parser reads + place the RAINY flag
        // at byte 11.
        data_bytes[0] = 50; // wind
        data_bytes[4] = 200; // glare
        data_bytes[5] = 30; // damage
        data_bytes[11] = WTHR_RAINY;
        // Bytes 10 and 13 hold decoy values that would have misparsed
        // pre-fix. 0xD3 at byte 13 is the slot the old code read.
        let old_byte_13_noise = data_bytes[13];
        assert_ne!(old_byte_13_noise, WTHR_RAINY);

        let subs = vec![
            make_sub(b"EDID", b"RainyOffsetCheck\0".to_vec()),
            make_sub(b"DATA", data_bytes),
        ];
        let w = parse_wthr(0x600, &subs, GameKind::Fallout3NV);
        assert_eq!(w.wind_speed, 50);
        assert_eq!(w.sun_glare, 200);
        assert_eq!(w.sun_damage, 30);
        assert_eq!(w.classification, WTHR_RAINY);
    }

    /// Regression for #536 / audit M33-04: FNAM must carry fog.
    /// Pre-fix the arm body was empty ("fallback when HNAM is absent"),
    /// but FNV + FO3 ship only FNAM (no HNAM), so every FNV + FO3
    /// weather defaulted to `fog_day_far = 10000.0`. This test uses
    /// the 24-byte FNV stride (16 B of 4 fog f32 + 8 trailing unknown
    /// bytes) to pin the fog fields.
    #[test]
    fn parse_wthr_fnam_fog_24byte_stride() {
        let mut fnam_data = vec![0u8; 24];
        fnam_data[0..4].copy_from_slice(&(-500.0_f32).to_le_bytes());
        fnam_data[4..8].copy_from_slice(&85_000.0_f32.to_le_bytes());
        fnam_data[8..12].copy_from_slice(&(-1000.0_f32).to_le_bytes());
        fnam_data[12..16].copy_from_slice(&40_000.0_f32.to_le_bytes());
        // Trailing 8 B are ignored by the parser — fill with sentinels.
        fnam_data[16..24].copy_from_slice(&[0xFE; 8]);
        let subs = vec![
            make_sub(b"EDID", b"FNVFogCheck\0".to_vec()),
            make_sub(b"FNAM", fnam_data),
        ];
        let w = parse_wthr(0xF09, &subs, GameKind::Fallout3NV);
        assert!((w.fog_day_near + 500.0).abs() < 0.01);
        assert!((w.fog_day_far - 85_000.0).abs() < 0.01);
        assert!((w.fog_night_near + 1000.0).abs() < 0.01);
        assert!((w.fog_night_far - 40_000.0).abs() < 0.01);
    }

    /// Oblivion FNAM is 16 B — same 4-f32 fog layout, just without
    /// the FNV/FO3 trailing 8 B.
    #[test]
    fn parse_wthr_fnam_fog_16byte_stride() {
        let mut fnam_data = vec![0u8; 16];
        fnam_data[0..4].copy_from_slice(&(-500.0_f32).to_le_bytes());
        fnam_data[4..8].copy_from_slice(&16_000.0_f32.to_le_bytes());
        fnam_data[8..12].copy_from_slice(&(-600.0_f32).to_le_bytes());
        fnam_data[12..16].copy_from_slice(&6_000.0_f32.to_le_bytes());
        let subs = vec![
            make_sub(b"EDID", b"OBLFogCheck\0".to_vec()),
            make_sub(b"FNAM", fnam_data),
        ];
        let w = parse_wthr(0x0B1, &subs, GameKind::Fallout3NV);
        assert!((w.fog_day_near + 500.0).abs() < 0.01);
        assert!((w.fog_day_far - 16_000.0).abs() < 0.01);
        assert!((w.fog_night_near + 600.0).abs() < 0.01);
        assert!((w.fog_night_far - 6_000.0).abs() < 0.01);
    }

    /// Real 56-byte Oblivion HNAM must NOT be interpreted as fog.
    /// Pre-#537 the HNAM arm gated on `>= 16` and would overwrite
    /// FNAM's correct fog values with the first 4 f32 of HNAM's
    /// lighting parameters (`0.7, 4.0, 2.0, 1.0` — fog-far of 4.0
    /// saturated every Oblivion exterior to solid fog_color within a
    /// few units of the camera). The 56-byte arm now captures the
    /// HDR tuning into `oblivion_hdr` and leaves fog to FNAM. See
    /// audit M33-05.
    #[test]
    fn parse_wthr_oblivion_hnam_56byte_does_not_clobber_fnam() {
        let mut fnam_data = vec![0u8; 16];
        fnam_data[0..4].copy_from_slice(&(-500.0_f32).to_le_bytes());
        fnam_data[4..8].copy_from_slice(&16_000.0_f32.to_le_bytes());
        fnam_data[8..12].copy_from_slice(&(-600.0_f32).to_le_bytes());
        fnam_data[12..16].copy_from_slice(&6_000.0_f32.to_le_bytes());
        let mut hnam_data = vec![0u8; 56];
        // Lighting-model values that would be DISASTROUS if read as fog.
        hnam_data[0..4].copy_from_slice(&0.7_f32.to_le_bytes());
        hnam_data[4..8].copy_from_slice(&4.0_f32.to_le_bytes());
        hnam_data[8..12].copy_from_slice(&2.0_f32.to_le_bytes());
        hnam_data[12..16].copy_from_slice(&1.0_f32.to_le_bytes());
        let subs = vec![
            make_sub(b"EDID", b"OBLMixed\0".to_vec()),
            make_sub(b"FNAM", fnam_data),
            make_sub(b"HNAM", hnam_data),
        ];
        let w = parse_wthr(0xEED, &subs, GameKind::Fallout3NV);
        assert!(
            (w.fog_day_far - 16_000.0).abs() < 0.01,
            "fog_day_far={}",
            w.fog_day_far
        );
        assert!(
            (w.fog_night_far - 6_000.0).abs() < 0.01,
            "fog_night_far={}",
            w.fog_night_far
        );
    }

    /// Regression for #537 / audit M33-05 — a real 56-byte Oblivion
    /// HNAM must decode into `oblivion_hdr` with the 14 UESP-documented
    /// fields in the right order. Values chosen per the audit's live
    /// sample from `SEClearTrans`: first 4 f32 = `0.7, 4.0, 2.0, 1.0`
    /// = eye_adapt_speed / blur_radius / blur_passes / emissive_mult.
    /// Remaining slots filled with sentinel values so an offset slip
    /// surfaces as a wrong field rather than a lucky zero.
    #[test]
    fn parse_wthr_oblivion_hnam_56byte_decodes_to_hdr_fields() {
        // 14 distinct sentinel values so we can tell fields apart.
        let values: [f32; 14] = [
            0.7,  // eye_adapt_speed
            4.0,  // blur_radius
            2.0,  // blur_passes
            1.0,  // emissive_mult
            0.85, // target_lum
            10.0, // upper_lum_clamp
            0.25, // bright_scale
            0.95, // bright_clamp
            1.5,  // lum_ramp_no_tex
            0.05, // lum_ramp_min
            2.5,  // lum_ramp_max
            0.8,  // sunlight_dimmer
            0.9,  // grass_dimmer
            0.75, // tree_dimmer
        ];
        let mut hnam = Vec::with_capacity(56);
        for v in values {
            hnam.extend_from_slice(&v.to_le_bytes());
        }
        assert_eq!(hnam.len(), 56);

        let subs = vec![
            make_sub(b"EDID", b"SEClearTrans\0".to_vec()),
            make_sub(b"HNAM", hnam),
        ];
        let w = parse_wthr(0x0100_0001, &subs, GameKind::Fallout3NV);
        let hdr = w
            .oblivion_hdr
            .expect("56-byte HNAM must populate oblivion_hdr");
        assert_eq!(hdr.eye_adapt_speed, 0.7);
        assert_eq!(hdr.blur_radius, 4.0);
        assert_eq!(hdr.blur_passes, 2.0);
        assert_eq!(hdr.emissive_mult, 1.0);
        assert_eq!(hdr.target_lum, 0.85);
        assert_eq!(hdr.upper_lum_clamp, 10.0);
        assert_eq!(hdr.bright_scale, 0.25);
        assert_eq!(hdr.bright_clamp, 0.95);
        assert_eq!(hdr.lum_ramp_no_tex, 1.5);
        assert_eq!(hdr.lum_ramp_min, 0.05);
        assert_eq!(hdr.lum_ramp_max, 2.5);
        assert_eq!(hdr.sunlight_dimmer, 0.8);
        assert_eq!(hdr.grass_dimmer, 0.9);
        assert_eq!(hdr.tree_dimmer, 0.75);

        // Fog fields must stay at defaults — HNAM must NOT leak into
        // the fog slots.
        assert_eq!(w.fog_day_near, 0.0);
        assert!(
            (w.fog_day_far - 10000.0).abs() < 0.01,
            "fog_day_far must retain its default (got {})",
            w.fog_day_far
        );
    }

    /// Sibling: FNV / FO3 / Skyrim+ weather records have no HNAM, so
    /// `oblivion_hdr` must stay `None` — consumers can pattern-match
    /// on `Some` to tell "this is an Oblivion weather" apart.
    #[test]
    fn parse_wthr_non_oblivion_leaves_oblivion_hdr_none() {
        let mut fnam_data = vec![0u8; 16];
        fnam_data[4..8].copy_from_slice(&32_000.0_f32.to_le_bytes());
        let subs = vec![
            make_sub(b"EDID", b"FNVDay\0".to_vec()),
            make_sub(b"FNAM", fnam_data),
        ];
        let w = parse_wthr(0x0200_0001, &subs, GameKind::Fallout3NV);
        assert!(w.oblivion_hdr.is_none());
    }

    /// Regression for #535 / audit M33-03: DNAM must be treated as a
    /// cloud-texture-path zstring, never as `[u8; 4]` cloud speeds.
    /// Feeding a 4-byte payload that would previously have decoded to
    /// `cloud_speeds = [1, 2, 3, 4]` now parses as the tiny zstring it
    /// resembles — and `cloud_speeds` no longer exists on the struct.
    #[test]
    fn parse_wthr_dnam_is_texture_path_not_speeds() {
        let subs = vec![
            make_sub(b"EDID", b"DnamPathCheck\0".to_vec()),
            make_sub(b"DNAM", b"sky\\a.dds\0".to_vec()),
        ];
        let w = parse_wthr(0x5ECD, &subs, GameKind::Fallout3NV);
        assert_eq!(w.cloud_textures[0].as_deref(), Some("sky\\a.dds"));
    }

    /// Regression for #533 / audit M33-01: the 160-byte Oblivion/FO3
    /// NAM0 stride (10 groups × 4 TOD slots × 4 B) must parse. The
    /// pre-fix gate demanded 240 B and silently dropped every Oblivion
    /// and FO3 weather — sky colours stayed at `WeatherRecord::default()`
    /// zero RGB. HIGH_NOON / MIDNIGHT are synthesised from DAY / NIGHT
    /// so the six-slot downstream layout remains valid.
    #[test]
    fn parse_wthr_nam0_160_byte_stride() {
        // 10 groups × 4 slots × 4 B = 160 B. Fill each group's 4 slots
        // with distinct colours so we can assert ordering + synthesis.
        let mut nam0_data = vec![0u8; 160];
        for group in 0..SKY_COLOR_GROUPS {
            for slot in 0..4 {
                let off = (group * 4 + slot) * 4;
                nam0_data[off] = (group * 10) as u8;
                nam0_data[off + 1] = (slot * 50) as u8;
                nam0_data[off + 2] = (group + slot * 2) as u8;
                nam0_data[off + 3] = 255;
            }
        }

        let subs = vec![
            make_sub(b"EDID", b"OblivionClear\0".to_vec()),
            make_sub(b"NAM0", nam0_data),
        ];

        let w = parse_wthr(0x2468, &subs, GameKind::Fallout3NV);
        // On-disk slots populate as authored. Indices match #729 / xEdit
        // fopdoc — `SKY_SUN = 5`, `SKY_HORIZON = 8`.
        let sun_sunrise = w.sky_colors[SKY_SUN][TOD_SUNRISE];
        assert_eq!((sun_sunrise.r, sun_sunrise.g), (50, 0));
        let horiz_sunset = w.sky_colors[SKY_HORIZON][TOD_SUNSET];
        assert_eq!((horiz_sunset.r, horiz_sunset.g), (80, 100));

        // Synthesised slots: HIGH_NOON = DAY, MIDNIGHT = NIGHT.
        for group in 0..SKY_COLOR_GROUPS {
            let day = w.sky_colors[group][TOD_DAY];
            let high_noon = w.sky_colors[group][TOD_HIGH_NOON];
            assert_eq!(
                (day.r, day.g, day.b),
                (high_noon.r, high_noon.g, high_noon.b),
                "HIGH_NOON should mirror DAY for group {}",
                group,
            );
            let night = w.sky_colors[group][TOD_NIGHT];
            let midnight = w.sky_colors[group][TOD_MIDNIGHT];
            assert_eq!(
                (night.r, night.g, night.b),
                (midnight.r, midnight.g, midnight.b),
                "MIDNIGHT should mirror NIGHT for group {}",
                group,
            );
        }
    }

    /// Sanity: a NAM0 shorter than 160 B is still silently dropped
    /// (malformed — no downstream should have to guess what the stride
    /// was). The gate stays `>= 160` to preserve that invariant.
    #[test]
    fn parse_wthr_nam0_below_160_drops() {
        let subs = vec![
            make_sub(b"EDID", b"Truncated\0".to_vec()),
            make_sub(b"NAM0", vec![0xFF; 80]),
        ];
        let w = parse_wthr(0xBADD, &subs, GameKind::Fallout3NV);
        // All slots remain at SkyColor::default() (all zero).
        for group in 0..SKY_COLOR_GROUPS {
            for slot in 0..SKY_TIME_SLOTS {
                let c = w.sky_colors[group][slot];
                assert_eq!((c.r, c.g, c.b, c.a), (0, 0, 0, 0));
            }
        }
    }

    /// Regression for #729 / EXT-RENDER-1: SKY_* constants must match
    /// the tes5edit fopdoc layout for FNV / FO3 verbatim. Pre-fix the
    /// indices collapsed the index-2 "Unused" slot and renumbered every
    /// group after it by one (so the renderer's "ambient" was actually
    /// the Unused slot, "sunlight" was actually Ambient, etc.). This
    /// test pins the canonical mapping so a future drift surfaces here
    /// rather than as a silent lighting regression in exteriors.
    #[test]
    fn sky_group_indices_match_xedit_fopdoc() {
        assert_eq!(SKY_UPPER, 0);
        assert_eq!(SKY_FOG, 1);
        assert_eq!(SKY_UNUSED_2, 2);
        assert_eq!(SKY_AMBIENT, 3);
        assert_eq!(SKY_SUNLIGHT, 4);
        assert_eq!(SKY_SUN, 5);
        assert_eq!(SKY_STARS, 6);
        assert_eq!(SKY_LOWER, 7);
        assert_eq!(SKY_HORIZON, 8);
        assert_eq!(SKY_UNUSED_9, 9);
    }

    /// Regression for #729 / EXT-RENDER-1: the parser must place each
    /// authored byte at the slot xEdit documents for it. Build a NAM0
    /// where every group ships a unique sentinel byte at its Day slot
    /// and verify each `SKY_*` constant resolves to the right sentinel —
    /// catches a future re-collapse of the Unused slot from any
    /// direction (constant drift, parser stride math, struct re-order).
    #[test]
    fn parse_wthr_nam0_groups_route_to_documented_slots() {
        let mut nam0 = vec![0u8; SKY_COLOR_GROUPS * SKY_TIME_SLOTS * 4];
        // Sentinel R = group_index * 16 so each group is uniquely
        // identifiable. G/B fixed so an off-by-one slip surfaces as a
        // wrong R-channel value rather than a lucky zero.
        for group in 0..SKY_COLOR_GROUPS {
            let off = (group * SKY_TIME_SLOTS + TOD_DAY) * 4;
            nam0[off] = (group as u8) * 16;
            nam0[off + 1] = 0xAA;
            nam0[off + 2] = 0xBB;
            nam0[off + 3] = 0xFF;
        }
        let subs = vec![
            make_sub(b"EDID", b"SlotPin\0".to_vec()),
            make_sub(b"NAM0", nam0),
        ];
        let w = parse_wthr(0x729, &subs, GameKind::Fallout3NV);

        let day = TOD_DAY;
        assert_eq!(w.sky_colors[SKY_UPPER][day].r, 0); // 0  Sky-Upper
        assert_eq!(w.sky_colors[SKY_FOG][day].r, 16); // 1  Fog
        assert_eq!(w.sky_colors[SKY_UNUSED_2][day].r, 32); // 2  Unused
        assert_eq!(w.sky_colors[SKY_AMBIENT][day].r, 48); // 3  Ambient
        assert_eq!(w.sky_colors[SKY_SUNLIGHT][day].r, 64); // 4  Sunlight
        assert_eq!(w.sky_colors[SKY_SUN][day].r, 80); // 5  Sun
        assert_eq!(w.sky_colors[SKY_STARS][day].r, 96); // 6  Stars
        assert_eq!(w.sky_colors[SKY_LOWER][day].r, 112); // 7  Sky-Lower
        assert_eq!(w.sky_colors[SKY_HORIZON][day].r, 128); // 8  Horizon
        assert_eq!(w.sky_colors[SKY_UNUSED_9][day].r, 144); // 9  Unused
    }

    /// #539 / M33-07 — Skyrim WTHR has a different NAM0 stride (and
    /// different cloud / fog FourCCs), so the FO3-era schema arms must
    /// NOT fire under the Skyrim variant. Pre-fix the parser had no
    /// `GameKind` parameter and would have read the first 240 B of a
    /// hypothetical 320-B Skyrim NAM0 as if they were FNV colours
    /// (silent garbage). With the gate in place the EDID still flows
    /// through (universal across all Bethesda games) but every other
    /// sub-record is dropped until the proper Skyrim parser lands.
    #[test]
    fn parse_wthr_skyrim_skips_fnv_schema_subrecords() {
        // Synthetic 240-B NAM0 (FNV stride). On Skyrim this should be
        // ignored — the real Skyrim NAM0 stride differs and feeding
        // these bytes through the FNV path would produce garbage.
        let mut nam0_data = vec![0u8; 240];
        let off = (0 * SKY_TIME_SLOTS + TOD_DAY) * 4;
        nam0_data[off] = 200;
        nam0_data[off + 1] = 100;
        nam0_data[off + 2] = 50;
        nam0_data[off + 3] = 255;

        // FNAM fog fixture — should also be skipped on Skyrim.
        let mut fnam_data = vec![0u8; 16];
        fnam_data[4..8].copy_from_slice(&5_555.0_f32.to_le_bytes());

        // DATA (15 B) — also skipped on Skyrim.
        let mut data_data = vec![0u8; 15];
        data_data[11] = WTHR_RAINY;

        let subs = vec![
            make_sub(b"EDID", b"SkyrimWeather\0".to_vec()),
            make_sub(b"NAM0", nam0_data),
            make_sub(b"FNAM", fnam_data),
            make_sub(b"DATA", data_data),
        ];
        let w = parse_wthr(0xDEAD, &subs, GameKind::Skyrim);

        // EDID universal — kept.
        assert_eq!(w.editor_id, "SkyrimWeather");
        // Sky colours stayed at default (no FNV-schema parse).
        let sky_upper_day = w.sky_colors[0][TOD_DAY];
        assert_eq!(sky_upper_day.r, 0);
        assert_eq!(sky_upper_day.g, 0);
        assert_eq!(sky_upper_day.b, 0);
        // Fog stayed at default (FNAM was dropped — default is the
        // 10 000-unit far plane from `WeatherRecord::default()`, NOT
        // the 5 555 we put in the synthetic FNAM payload).
        assert_eq!(w.fog_day_far, 10_000.0);
        // Classification stayed at default (DATA was dropped).
        assert_eq!(w.classification, 0);
    }

    /// Sibling pin: the same fixture under `GameKind::Fallout3NV` MUST
    /// parse cleanly, so the gate doesn't accidentally drop FNV /
    /// FO3 / Oblivion data. Pairs with the Skyrim-skip pin above.
    #[test]
    fn parse_wthr_fnv_schema_still_parses_under_fnv_kind() {
        let mut nam0_data = vec![0u8; 240];
        let off = (0 * SKY_TIME_SLOTS + TOD_DAY) * 4;
        nam0_data[off] = 200;
        nam0_data[off + 1] = 100;
        nam0_data[off + 2] = 50;
        nam0_data[off + 3] = 255;

        let mut fnam_data = vec![0u8; 16];
        fnam_data[4..8].copy_from_slice(&5_555.0_f32.to_le_bytes());

        let subs = vec![
            make_sub(b"EDID", b"FnvWeather\0".to_vec()),
            make_sub(b"NAM0", nam0_data),
            make_sub(b"FNAM", fnam_data),
        ];
        let w = parse_wthr(0xBEEF, &subs, GameKind::Fallout3NV);

        assert_eq!(w.editor_id, "FnvWeather");
        let sky_upper_day = w.sky_colors[0][TOD_DAY];
        assert_eq!(sky_upper_day.r, 200);
        assert_eq!(sky_upper_day.g, 100);
        assert_eq!(sky_upper_day.b, 50);
        assert!((w.fog_day_far - 5_555.0).abs() < 0.001);
    }

    #[test]
    fn sky_color_to_rgb_f32() {
        let c = SkyColor {
            r: 255,
            g: 0,
            b: 128,
            a: 255,
        };
        let rgb = c.to_rgb_f32();
        assert!((rgb[0] - 1.0).abs() < 0.001);
        assert!(rgb[1].abs() < 0.001);
        assert!((rgb[2] - 128.0 / 255.0).abs() < 0.001);
    }

    // ── #539 / M33-04..07 — Skyrim WTHR parser ────────────────────────
    //
    // Pre-fix Skyrim WTHRs were indexed by FormID but every sky / fog /
    // cloud sub-record was silently skipped: the runtime warn cited
    // M32.5 as the gating milestone. Every Skyrim cell fell through to
    // `insert_procedural_fallback_resources` (Mojave-warm defaults),
    // producing the wrong sun colour / wrong ambient / wrong horizon
    // band on every Skyrim render. Markarth was the cell that surfaced
    // it. These tests pin the parser against the on-disk layout
    // sampled from `Skyrim.esm` via `dump_wthr_subs`.

    /// Build a 272-byte Skyrim NAM0 body — 17 groups × 4 TOD slots ×
    /// 4 bytes RGBA. Each group/slot gets a recognisable value so
    /// per-group / per-slot wiring can be asserted in isolation.
    fn build_skyrim_nam0(groups: &[[[u8; 4]; 4]; SKYRIM_NAM0_GROUPS]) -> Vec<u8> {
        let mut out = Vec::with_capacity(SKYRIM_NAM0_GROUPS * SKYRIM_NAM0_TOD_SLOTS * 4);
        for group in groups {
            for slot in group {
                out.extend_from_slice(slot);
            }
        }
        out
    }

    #[test]
    fn parse_skyrim_routes_through_dedicated_branch() {
        // The dispatch in `parse_wthr` must re-route Skyrim records to
        // `parse_wthr_skyrim` — not run them through the FO3-era
        // branch (which would over-read NAM0 by 32 B and corrupt FNAM).
        // Sanity check: an empty Skyrim record must produce default
        // sky_colors but a non-default `editor_id` if EDID is present.
        let subs = vec![make_sub(b"EDID", b"SkyrimClear\0".to_vec())];
        let w = parse_wthr(0xDEAD, &subs, GameKind::Skyrim);
        assert_eq!(w.editor_id, "SkyrimClear");
        assert!(w.skyrim_ambient_cube.is_none());
        // Default fog distances ride through.
        assert_eq!(w.fog_day_far, 10_000.0);
    }

    #[test]
    fn parse_skyrim_nam0_lifts_first_nine_groups() {
        // Stripe each of the 17 groups with a recognisable colour so
        // we can assert which group landed in which slot of the
        // shared `[10][6]` table.
        let mut groups: [[[u8; 4]; 4]; SKYRIM_NAM0_GROUPS] =
            [[[0u8; 4]; 4]; SKYRIM_NAM0_GROUPS];
        for (g_idx, group) in groups.iter_mut().enumerate() {
            for (s_idx, slot) in group.iter_mut().enumerate() {
                *slot = [g_idx as u8 + 1, s_idx as u8 + 1, 0xAA, 0];
            }
        }
        let nam0 = build_skyrim_nam0(&groups);

        let subs = vec![
            make_sub(b"EDID", b"SkyrimCloudy\0".to_vec()),
            make_sub(b"NAM0", nam0),
        ];
        let w = parse_wthr(0x1234, &subs, GameKind::Skyrim);

        // Group 0 (SKY_UPPER), all 4 TOD slots.
        for slot in 0..4 {
            assert_eq!(w.sky_colors[SKY_UPPER][slot].r, 1);
            assert_eq!(w.sky_colors[SKY_UPPER][slot].g, slot as u8 + 1);
        }
        // Group 5 (SKY_SUN), TOD_SUNRISE.
        assert_eq!(w.sky_colors[SKY_SUN][TOD_SUNRISE].r, 6);
        assert_eq!(w.sky_colors[SKY_SUN][TOD_SUNRISE].g, 1);
        // Group 9 (last that fits in the shared 10-group array — Skyrim
        // "Effects Lighting" / FO3-era SKY_UNUSED_9).
        assert_eq!(w.sky_colors[9][TOD_DAY].r, 10);
        // HIGH_NOON synthesised from DAY (slot 1 colour with `g = 2`).
        assert_eq!(w.sky_colors[SKY_UPPER][TOD_HIGH_NOON].r, 1);
        assert_eq!(w.sky_colors[SKY_UPPER][TOD_HIGH_NOON].g, 2);
        // MIDNIGHT synthesised from NIGHT (slot 3 colour with `g = 4`).
        assert_eq!(w.sky_colors[SKY_UPPER][TOD_MIDNIGHT].r, 1);
        assert_eq!(w.sky_colors[SKY_UPPER][TOD_MIDNIGHT].g, 4);
    }

    #[test]
    fn parse_skyrim_fnam_lifts_four_fog_distances() {
        // 8 × f32 = 32 B. First 4 are fog distances; trailing 4
        // (day_power, night_power, day_max, night_max) are
        // captured-on-disk-only in v1.
        let mut fnam = Vec::new();
        for v in [
            1_200.0f32, 80_000.0, 1_200.0, 40_000.0, 0.4, 0.4, 0.85, 0.85,
        ] {
            fnam.extend_from_slice(&v.to_le_bytes());
        }
        let subs = vec![
            make_sub(b"EDID", b"SkyrimStorm\0".to_vec()),
            make_sub(b"FNAM", fnam),
        ];
        let w = parse_wthr(0xCAFE, &subs, GameKind::Skyrim);
        assert!((w.fog_day_near - 1_200.0).abs() < 0.001);
        assert!((w.fog_day_far - 80_000.0).abs() < 0.001);
        assert!((w.fog_night_near - 1_200.0).abs() < 0.001);
        assert!((w.fog_night_far - 40_000.0).abs() < 0.001);
    }

    #[test]
    fn parse_skyrim_data_lifts_wind_and_classification() {
        // 19 bytes. Byte 0 = wind. Byte 14 = classification flags.
        let mut data = vec![0u8; 19];
        data[0] = 0x19; // wind = 25
        data[14] = WTHR_CLOUDY; // classification = cloudy
        let subs = vec![
            make_sub(b"EDID", b"SkyrimRainy\0".to_vec()),
            make_sub(b"DATA", data),
        ];
        let w = parse_wthr(0xFACE, &subs, GameKind::Skyrim);
        assert_eq!(w.wind_speed, 25);
        assert_eq!(w.classification, WTHR_CLOUDY);
    }

    #[test]
    fn parse_skyrim_dalc_captures_four_ambient_cubes() {
        // 4× DALC entries, each 32 B = 6 RGB+pad axes + RGB+pad spec
        // + f32 fresnel.
        fn make_dalc(slot_marker: u8) -> Vec<u8> {
            let mut buf = vec![0u8; 32];
            // 6 ambient axes — stripe each with slot_marker + axis_id.
            for axis in 0..6 {
                let off = axis * 4;
                buf[off] = slot_marker + axis as u8 * 0x10;
                buf[off + 1] = 0x80;
                buf[off + 2] = 0xC0;
                buf[off + 3] = 0;
            }
            // Specular colour at bytes 24..28.
            buf[24] = 0x88;
            buf[25] = 0x99;
            buf[26] = 0xAA;
            // Fresnel power = 1.0 at bytes 28..32.
            buf[28..32].copy_from_slice(&1.0f32.to_le_bytes());
            buf
        }

        let subs = vec![
            make_sub(b"EDID", b"SkyrimSnow\0".to_vec()),
            make_sub(b"DALC", make_dalc(0x10)), // sunrise
            make_sub(b"DALC", make_dalc(0x20)), // day
            make_sub(b"DALC", make_dalc(0x30)), // sunset
            make_sub(b"DALC", make_dalc(0x40)), // night
        ];
        let w = parse_wthr(0xBABE, &subs, GameKind::Skyrim);
        let cubes = w
            .skyrim_ambient_cube
            .expect("Skyrim DALC must populate the ambient cube");
        // Slot 0 (sunrise) +X = 0x10.
        assert_eq!(cubes[0].pos_x.r, 0x10);
        // Slot 0 (sunrise) -Z (axis 5) = 0x10 + 0x50 = 0x60.
        assert_eq!(cubes[0].neg_z.r, 0x60);
        // Slot 1 (day) +X = 0x20.
        assert_eq!(cubes[1].pos_x.r, 0x20);
        // Slot 3 (night) +X = 0x40.
        assert_eq!(cubes[3].pos_x.r, 0x40);
        // Specular + fresnel ride through.
        assert_eq!(cubes[0].specular.r, 0x88);
        assert!((cubes[0].fresnel_power - 1.0).abs() < 1e-6);
    }

    #[test]
    fn parse_skyrim_dalc_under_four_pads_with_last_authored() {
        // Defensive: a truncated mod record might ship only 2 DALC
        // entries. Missing tail slots get the last authored cube so
        // downstream consumers don't read default-zero ambient for
        // sunset / night when the record only authored sunrise / day.
        fn make_dalc(marker: u8) -> Vec<u8> {
            let mut buf = vec![0u8; 32];
            buf[0] = marker;
            buf[28..32].copy_from_slice(&1.0f32.to_le_bytes());
            buf
        }
        let subs = vec![
            make_sub(b"EDID", b"TruncatedDalcMod\0".to_vec()),
            make_sub(b"DALC", make_dalc(0xAA)),
            make_sub(b"DALC", make_dalc(0xBB)),
        ];
        let w = parse_wthr(0x9999, &subs, GameKind::Skyrim);
        let cubes = w.skyrim_ambient_cube.expect("partial DALC still populates");
        assert_eq!(cubes[0].pos_x.r, 0xAA);
        assert_eq!(cubes[1].pos_x.r, 0xBB);
        // Sunset / night fall back to last authored (day = 0xBB).
        assert_eq!(cubes[2].pos_x.r, 0xBB);
        assert_eq!(cubes[3].pos_x.r, 0xBB);
    }

    #[test]
    fn parse_skyrim_extra_dalc_clamps_at_four_does_not_overflow() {
        // 5+ DALC entries (some mods do this) clamp at 4 — never
        // overflow the fixed-size buffer.
        fn make_dalc(marker: u8) -> Vec<u8> {
            let mut buf = vec![0u8; 32];
            buf[0] = marker;
            buf[28..32].copy_from_slice(&1.0f32.to_le_bytes());
            buf
        }
        let mut subs = vec![make_sub(b"EDID", b"OverflowDalc\0".to_vec())];
        for marker in 0..8u8 {
            subs.push(make_sub(b"DALC", make_dalc(0x10 + marker)));
        }
        let w = parse_wthr(0x5555, &subs, GameKind::Skyrim);
        let cubes = w.skyrim_ambient_cube.expect("DALC must populate");
        // Only first 4 take.
        assert_eq!(cubes[0].pos_x.r, 0x10);
        assert_eq!(cubes[3].pos_x.r, 0x13);
    }

    #[test]
    fn parse_skyrim_full_record_round_trip() {
        // End-to-end: NAM0 + FNAM + DATA + DALC all populate together.
        let mut groups: [[[u8; 4]; 4]; SKYRIM_NAM0_GROUPS] =
            [[[0u8; 4]; 4]; SKYRIM_NAM0_GROUPS];
        // Stripe so we can verify cross-group alignment.
        groups[SKY_UPPER][TOD_DAY] = [40, 110, 155, 0];
        groups[SKY_AMBIENT][TOD_DAY] = [160, 180, 195, 0];
        groups[SKY_SUN][TOD_DAY] = [200, 180, 140, 0];

        let mut fnam = Vec::new();
        for v in [1200.0f32, 80000.0, 1200.0, 40000.0, 0.4, 0.4, 0.85, 0.85] {
            fnam.extend_from_slice(&v.to_le_bytes());
        }
        let mut data = vec![0u8; 19];
        data[0] = 0x10;
        data[14] = WTHR_PLEASANT;
        let mut dalc = vec![0u8; 32];
        dalc[16] = 0xC0; // +Z (up) ambient .R = 0xC0
        dalc[28..32].copy_from_slice(&1.0f32.to_le_bytes());

        let subs = vec![
            make_sub(b"EDID", b"SkyrimFullRoundTrip\0".to_vec()),
            make_sub(b"NAM0", build_skyrim_nam0(&groups)),
            make_sub(b"FNAM", fnam),
            make_sub(b"DATA", data),
            make_sub(b"DALC", dalc.clone()),
            make_sub(b"DALC", dalc.clone()),
            make_sub(b"DALC", dalc.clone()),
            make_sub(b"DALC", dalc),
        ];
        let w = parse_wthr(0xC0DE, &subs, GameKind::Skyrim);
        assert_eq!(w.editor_id, "SkyrimFullRoundTrip");
        assert_eq!(w.sky_colors[SKY_UPPER][TOD_DAY].r, 40);
        assert_eq!(w.sky_colors[SKY_AMBIENT][TOD_DAY].g, 180);
        assert_eq!(w.sky_colors[SKY_SUN][TOD_DAY].b, 140);
        // HIGH_NOON synthesised from DAY round-trips.
        assert_eq!(
            w.sky_colors[SKY_SUN][TOD_HIGH_NOON],
            w.sky_colors[SKY_SUN][TOD_DAY]
        );
        assert!((w.fog_day_far - 80_000.0).abs() < 0.001);
        assert_eq!(w.wind_speed, 0x10);
        assert_eq!(w.classification, WTHR_PLEASANT);
        let cubes = w.skyrim_ambient_cube.unwrap();
        assert_eq!(cubes[0].pos_z.r, 0xC0);
    }
}
