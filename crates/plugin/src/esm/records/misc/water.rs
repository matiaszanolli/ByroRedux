//! Water record (`WATR`) and decoded water parameters.

use super::super::common::{read_zstring, CommonNamedFields};
use crate::esm::reader::{GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

/// Water record — referenced by `CELL.XCWT` (water type form ID on a
/// cell). Pre-fix every XCWT reference dangled at cell load.
///
/// Carries the engine-decoded subset of the `DATA` / `DNAM` payload
/// that the water shader actually consumes (colours + fog + Fresnel +
/// scroll). The full per-game byte layout differs across Oblivion /
/// FO3 / FNV / Skyrim+ and isn't fully decoded here: we capture the
/// raw DNAM bytes alongside the structured `params` so a later, more
/// precise per-game parser can keep the storage shape stable while
/// improving accuracy.
///
/// **Confident decode** (cross-checked against UESP CSWiki for
/// Oblivion + FO3 + FNV WATR.DATA, plus the Gamebryo 2.3 water
/// material header):
///
/// - Oblivion DATA: 102 bytes, layout starting with 11 × f32 +
///   3 × u32-packed RGBA8.
/// - FO3 / FNV DATA: 196-byte extension of the Oblivion layout —
///   first 60 bytes preserve the FNV/FO3-compatible prefix.
///
/// **Best-effort decode** for Skyrim DNAM (252+ bytes) — the field
/// names are documented but the offsets vary between 1.5 / 1.6
/// patches; we read what we can and leave the rest at default.
///
/// **Exact prefix decode** for Fallout 4 DNAM (201 bytes), following
/// xEdit's `wbDefinitionsFO4.pas`: fog colours at 4/8, underwater fog
/// near/far at 44/48, reflectivity/Fresnel at 64/68, reflection colour
/// at 96, with specular and noise controls following the xEdit-defined tail.
#[derive(Debug, Clone, Default)]
pub struct WatrRecord {
    pub form_id: u32,
    pub editor_id: String,
    pub full_name: String,
    /// Authored surface opacity from WATR.ANAM (0..255 normalized). Zero
    /// means the record omitted the field and the canonical fallback applies.
    pub opacity: f32,
    /// Diffuse / noise texture path. FO3 / FNV ship this in `NNAM`
    /// (e.g. `Data\Textures\Water\WastelandWaterPotomac.dds` on every
    /// vanilla FO3 WATR); Skyrim+ ships it in `TNAM`. Both arms write
    /// here; per Bethesda, NNAM and TNAM are game-mutually-exclusive
    /// in vanilla content, so last-arm-wins is safe without a
    /// `GameKind` gate (#1271).
    pub texture_path: String,
    /// Decoded water shader / shading params. Fields are at their
    /// per-spec defaults when the source record omits a sub-record or
    /// when the byte layout doesn't match the parser's expectations.
    pub params: WaterParams,
    /// Skyrim+ authored noise-layer texture paths from NAM2/NAM3/NAM4.
    /// Empty strings denote omitted layers.
    pub noise_texture_paths: [String; 3],
    /// Skyrim SE flow-normal texture from NAM5. Flowing water promotes this
    /// into the renderer's third normal layer; calm water keeps NAM4 there.
    pub flow_noise_texture_path: String,
    /// GNAM's three related-water FormIDs (daytime, nighttime,
    /// underwater). xEdit marks these links unused; they are preserved
    /// accurately rather than misclassified as texture references.
    pub related_waters: [u32; 3],
    /// Raw DNAM bytes — preserved so a future per-game-precise
    /// decoder can re-parse without re-walking the ESM. ~252+ bytes
    /// on Skyrim, ~196 on FNV/FO3, ~102 on Oblivion. Empty when the
    /// record omits DNAM (or pre-FNV DATA is used instead — see
    /// `raw_data`).
    pub raw_dnam: Vec<u8>,
    /// Raw DATA bytes (Oblivion / FO3 / FNV path). Same rationale
    /// as [`Self::raw_dnam`] — preserved for future re-decode.
    pub raw_data: Vec<u8>,
}

/// Engine-side water shader parameter view. The renderer's
/// `WaterMaterial` is derived from this by the cell loader (the
/// loader applies the `WaterKind` heuristic + scroll-vector synthesis
/// from `wind_speed` / `wind_direction`).
///
/// Colours are stored as **linear-RGB f32** (per
/// [`feedback_color_space`] — Gamebryo colour bytes are raw monitor-
/// space floats with no sRGB curve to invert).
///
/// [`feedback_color_space`]: ../../../../../home/matias/.claude/projects/-mnt-data-src-gamebyro-redux/memory/feedback_color_space.md
#[derive(Debug, Clone, Copy)]
pub struct WaterParams {
    /// Linear RGB of the shallow-water tint (DATA / DNAM RGBA bytes
    /// → f32; alpha is dropped — the renderer drives opacity from
    /// `WaterKind` + grazing angle).
    pub shallow_color: [f32; 3],
    /// Linear RGB of the deep-water tint.
    pub deep_color: [f32; 3],
    /// Linear RGB of the authored underwater post-process tint. Legacy
    /// records use the deep-water tint as their canonical fallback.
    pub underwater_color: [f32; 3],
    /// Linear RGB of the reflection tint — multiplied into the RT
    /// reflection ray hit colour by the water shader.
    pub reflection_color: [f32; 3],
    /// NEAR PLANE of the underwater fog ramp (world units) — absorption
    /// starts here; the column is clear before it.
    ///
    /// #2785 — was documented as "the distance at which the shallow tint
    /// reaches 50% mix", which the data contradicts: vanilla authors `0`
    /// for nearly every record (Skyrim 34 WATR, FNV 78, Oblivion 23 —
    /// median `fog_near/fog_far` ≤ 0.001). Read as a half-distance it
    /// would make almost all vanilla water opaque on contact. The
    /// `80.0` default below is a mid-range clear margin, not a median of
    /// authored values.
    pub fog_near: f32,
    /// FAR PLANE of that ramp — distance at which the deep tint fully
    /// takes over. Default `600.0`. Clamped to at least `fog_near + 1`
    /// on parse so the ramp span is never zero.
    pub fog_far: f32,
    /// Underwater fog ramp. Zero/invalid means reuse the above-water ramp.
    pub underwater_fog_near: f32,
    pub underwater_fog_far: f32,
    /// FO4 underwater fog amount (DNAM offset 40). One is neutral.
    pub underwater_fog_amount: f32,
    /// 0..1 reflectivity multiplier (`reflectivity_amount`).
    pub reflectivity: f32,
    /// 0..1 Fresnel amount — drives the surface's edge fresnel
    /// intensity. Default `0.02` (~clean water F0).
    pub fresnel: f32,
    /// Wind speed driving normal-map scroll, world units per second.
    pub wind_speed: f32,
    /// Wind direction in radians (DATA `wind_direction`).
    pub wind_direction: f32,
    /// Wave amplitude — vertex displacement magnitude. Not used by
    /// the flat-mesh shader (we perturb shading normals instead) but
    /// carried for future displacement work / underwater systems.
    pub wave_amplitude: f32,
    /// Wave frequency, Hz.
    pub wave_frequency: f32,
    /// Blinn-Phong exponent for the direct-sun glint. Bethesda names this
    /// `Sun Specular Power`; larger values produce a tighter highlight.
    /// Authored by every supported WATR visual-data generation.
    pub sun_specular_power: f32,
    /// FO3/FNV long DATA tail: authored UV scale for noise layer one.
    /// Zero means the record did not carry the long-tail field.
    pub noise_uv_scale_a: f32,
    /// FO3/FNV long DATA tail: authored UV scale for noise layer two.
    /// Zero means the record did not carry the long-tail field.
    pub noise_uv_scale_b: f32,
    /// Skyrim+/FO4 long DNAM tail: authored UV scale for noise layer three.
    /// Stored in canonical inverse-world-units.
    pub noise_uv_scale_c: f32,
    /// Skyrim+/FO4 noise-layer amplitude multipliers (NAM2/NAM3/NAM4
    /// companion tail). Zero means the record did not carry the fields.
    pub noise_amplitude_scales: [f32; 3],
    /// Authored physical normal magnitude. Skyrim stores this at DNAM 92;
    /// FO4 stores it at DNAM 52. One is the neutral renderer fallback.
    pub normal_magnitude: f32,
    /// Skyrim above-water fog amount (DNAM 132). One is neutral; it scales
    /// the refraction absorption response without changing the authored fog
    /// distance ramp.
    pub above_water_fog_amount: f32,
    /// Skyrim DNAM depth-response multipliers: reflections, refraction,
    /// normals, and specular lighting (offsets 208..224).
    pub depth_weights: [f32; 4],
    /// Skyrim water effect controls: refraction magnitude, local specular
    /// power, reflection magnitude, and sun-specular magnitude.
    pub effect_controls: [f32; 4],
    /// Skyrim's specular-properties magnitude at DNAM offset 160. This
    /// unnamed xEdit field defaults to one and scales authored sun glints.
    pub specular_magnitude: f32,
    /// Authored normal-layer wind directions (radians) and UV speeds. Zero
    /// entries are sentinels for layouts without per-layer motion controls;
    /// FO76/Starfield NAM0 linear velocity is projected into layer 0.
    pub noise_wind_directions: [f32; 3],
    pub noise_wind_speeds: [f32; 3],
    /// Skyrim SE-only flow-map tile scale at DNAM offset 228. A zero
    /// sentinel means the record uses the canonical engine scale.
    pub flowmap_scale: f32,
    /// Starfield DNAM color-absorption ranges (red, green, blue), in world
    /// units. Zero means the source layout did not author per-channel
    /// absorption and the renderer uses its legacy scalar fog curve.
    pub absorption_ranges: [f32; 3],
    /// Starfield DNAM surface roughness. Zero is the absent/legacy sentinel;
    /// authored values are normalized to 0..1 before translation.
    pub roughness: f32,
    /// FO4/FO76 authored suspended-silt amount and its light/dark colors.
    /// A zero amount is the sentinel used by older layouts and leaves the
    /// canonical palette untouched.
    pub silt_amount: f32,
    pub silt_light_color: [f32; 3],
    pub silt_dark_color: [f32; 3],
}

impl Default for WaterParams {
    fn default() -> Self {
        Self {
            shallow_color: [0.10, 0.32, 0.38],
            deep_color: [0.02, 0.06, 0.10],
            underwater_color: [0.02, 0.06, 0.10],
            reflection_color: [0.85, 0.88, 0.92],
            fog_near: 80.0,
            fog_far: 600.0,
            underwater_fog_near: 0.0,
            underwater_fog_far: 0.0,
            underwater_fog_amount: 1.0,
            reflectivity: 0.85,
            fresnel: 0.02,
            wind_speed: 1.0,
            wind_direction: 0.0,
            wave_amplitude: 0.05,
            wave_frequency: 0.6,
            sun_specular_power: 50.0,
            noise_uv_scale_a: 0.0,
            noise_uv_scale_b: 0.0,
            noise_uv_scale_c: 0.0,
            noise_amplitude_scales: [0.0; 3],
            normal_magnitude: 1.0,
            above_water_fog_amount: 1.0,
            depth_weights: [0.0; 4],
            effect_controls: [0.0; 4],
            specular_magnitude: 0.0,
            noise_wind_directions: [0.0; 3],
            noise_wind_speeds: [0.0; 3],
            flowmap_scale: 0.0,
            absorption_ranges: [0.0; 3],
            roughness: 0.0,
            silt_amount: 0.0,
            silt_light_color: [0.0; 3],
            silt_dark_color: [0.0; 3],
        }
    }
}

/// Decode an 8-bit unsigned colour component into the engine's
/// linear-RGB working space. Gamebryo colours are raw monitor-space
/// floats — no sRGB curve to invert (see [`feedback_color_space`]).
///
/// [`feedback_color_space`]: ../../../../../home/matias/.claude/projects/-mnt-data-src-gamebyro-redux/memory/feedback_color_space.md
#[inline]
fn u8_to_linear(byte: u8) -> f32 {
    byte as f32 / 255.0
}

/// xEdit exposes water noise UV scales as large authored tile lengths (for
/// example Skyrim's default water stores 1920/6703/488). The renderer's
/// canonical material is expressed as inverse world-space frequency. Older
/// fixtures already provide the normalized inverse form, so preserve values
/// in `(0, 1]` and invert only the legacy length representation.
#[inline]
fn normalize_noise_uv_scale(value: f32) -> f32 {
    if !value.is_finite() || value <= 0.0 {
        0.0
    } else if value > 1.0 {
        1.0 / value
    } else {
        value
    }
}

/// Parse Oblivion / FO3 / FNV WATR.DATA. The leading wind / wave /
/// fog / reflectivity / fresnel prefix (offsets 0..36) is shared, but
/// the RGBA colour block that follows is **not** at a fixed offset
/// across games:
///
/// ```text
/// offset  size  field
/// ------  ----  --------------------------------
///  0      4     wind_velocity      (f32)
///  4      4     wind_direction     (f32)
///  8      4     wave_amplitude     (f32)
/// 12      4     wave_frequency     (f32)
/// 16      4     sun_specular_power (f32)
/// 20      4     reflectivity_amt   (f32)
/// 24      4     fresnel_amount     (f32)
/// 28      4     fog_distance_near  (f32) — FNV/FO3
/// 32      4     fog_distance_far   (f32)
/// 36      …     colour block — offset is game-dependent (see below)
/// ```
///
/// FO3/FNV ship WATR.DATA as either a 2-byte "Damage-only" stub or a
/// full **186-byte** record. The 186-byte variant carries an extra
/// fog-distance `f32` at offset 36, which pushes the RGBA colour block
/// to **40 / 44 / 48** (verified against the real `PPurityWater01Murky`
/// and `DupontFontWaterType` records, #1778). Oblivion's shorter DATA
/// (and any record below 186 bytes) keeps the legacy **36 / 40 / 44**
/// colour offsets. `decode_data` falls back to defaults for any field
/// whose source offset is past the buffer end, so the 2-byte stub keeps
/// all default colours.
fn decode_data(data: &[u8]) -> WaterParams {
    let mut p = WaterParams::default();
    let mut r = SubReader::new(data);
    if let Ok(v) = r.f32() {
        p.wind_speed = v;
    }
    if let Ok(v) = r.f32() {
        p.wind_direction = v;
    }
    if let Ok(v) = r.f32() {
        p.wave_amplitude = v;
    }
    if let Ok(v) = r.f32() {
        p.wave_frequency = v;
    }
    if let Ok(v) = r.f32() {
        p.sun_specular_power = v.clamp(1.0, 2048.0);
    }
    if let Ok(v) = r.f32() {
        p.reflectivity = v.clamp(0.0, 1.0);
    }
    if let Ok(v) = r.f32() {
        p.fresnel = v.clamp(0.0, 1.0);
    }
    if let Ok(v) = r.f32() {
        p.fog_near = v.max(0.0);
    }
    if let Ok(v) = r.f32() {
        p.fog_far = v.max(p.fog_near + 1.0);
    }
    // FO3/FNV DNAM/DATA tail: Under Water fog near/far at 144/148.
    // Short Oblivion-style records do not carry this tail and retain the
    // zero sentinel, which makes the renderer reuse the above-water ramp.
    if let Some(v) = read_f32_at(data, 144) {
        p.underwater_fog_near = v.max(0.0);
    }
    if let Some(v) = read_f32_at(data, 148) {
        p.underwater_fog_far = v.max(p.underwater_fog_near + 1.0);
    }
    // FO3/FNV long DATA tail: Noise Layer 1/2 UV scales. These are
    // independent authored tiling controls, not the wind/wave prefix.
    if let Some(v) = read_f32_at(data, 172) {
        p.noise_uv_scale_a = normalize_noise_uv_scale(v);
    }
    if let Some(v) = read_f32_at(data, 176) {
        p.noise_uv_scale_b = normalize_noise_uv_scale(v);
    }
    if let Some(v) = read_f32_at(data, 180) {
        p.noise_uv_scale_c = normalize_noise_uv_scale(v);
    }
    for (slot, offset) in p.noise_amplitude_scales.iter_mut().zip([184, 188, 192]) {
        if let Some(v) = read_f32_at(data, offset) {
            *slot = v.max(0.0);
        }
    }
    for (slot, offset) in p.depth_weights.iter_mut().zip([208, 212, 216, 220]) {
        if let Some(v) = read_f32_at(data, offset) {
            *slot = v.max(0.0);
        }
    }
    for (slot, offset) in p.effect_controls.iter_mut().zip([152, 156, 196, 204]) {
        if let Some(v) = read_f32_at(data, offset) {
            *slot = v.max(0.0);
        }
    }
    // The 186-byte FO3/FNV record has an extra fog-distance f32 at
    // offset 36 that shifts the colour block 4 bytes forward (#1778);
    // shorter records (Oblivion, the 2-byte stub) keep the legacy base.
    let color_base = if data.len() >= 186 { 40 } else { 36 };
    if data.len() >= color_base + 4 {
        p.shallow_color = [
            u8_to_linear(data[color_base]),
            u8_to_linear(data[color_base + 1]),
            u8_to_linear(data[color_base + 2]),
        ];
    }
    if data.len() >= color_base + 8 {
        p.deep_color = [
            u8_to_linear(data[color_base + 4]),
            u8_to_linear(data[color_base + 5]),
            u8_to_linear(data[color_base + 6]),
        ];
    }
    if data.len() >= color_base + 12 {
        p.reflection_color = [
            u8_to_linear(data[color_base + 8]),
            u8_to_linear(data[color_base + 9]),
            u8_to_linear(data[color_base + 10]),
        ];
    }
    p
}

/// Decode the pre-FO4 DNAM visual prefix shared by FO3/FNV and TES5.
/// Skyrim 1.5 / 1.6 differ in trailing fields, but xEdit's definitions
/// agree on the 52-byte prefix consumed here. When the buffer is shorter,
/// every field falls back to its canonical default.
fn decode_dnam_pre_fo4(data: &[u8]) -> WaterParams {
    let mut p = WaterParams::default();
    // xEdit's TES5 definition starts the shared wind/wave prefix at byte 0.
    // An unnamed float at byte 28 (between Fresnel and fog-near), not a
    // leading version word, is what shifts the colour block to byte 40.
    if data.len() < 52 {
        return p;
    }
    let mut r = SubReader::new(data);
    if let Ok(v) = r.f32() {
        p.wind_speed = v;
    }
    if let Ok(v) = r.f32() {
        p.wind_direction = v;
    }
    if let Ok(v) = r.f32() {
        p.wave_amplitude = v;
    }
    if let Ok(v) = r.f32() {
        p.wave_frequency = v;
    }
    if let Ok(v) = r.f32() {
        p.sun_specular_power = v.clamp(1.0, 2048.0);
    }
    if let Ok(v) = r.f32() {
        p.reflectivity = v.clamp(0.0, 1.0);
    }
    if let Ok(v) = r.f32() {
        p.fresnel = v.clamp(0.0, 1.0);
    }
    // Unnamed/unused float at 28..32 in the TES5 record definition.
    r.skip_or_eof(4);
    if let Ok(v) = r.f32() {
        p.fog_near = v.max(0.0);
    }
    if let Ok(v) = r.f32() {
        p.fog_far = v.max(p.fog_near + 1.0);
    }
    if data.len() >= 44 {
        p.shallow_color = [
            u8_to_linear(data[40]),
            u8_to_linear(data[41]),
            u8_to_linear(data[42]),
        ];
    }
    if data.len() >= 48 {
        p.deep_color = [
            u8_to_linear(data[44]),
            u8_to_linear(data[45]),
            u8_to_linear(data[46]),
        ];
    }
    if data.len() >= 52 {
        p.reflection_color = [
            u8_to_linear(data[48]),
            u8_to_linear(data[49]),
            u8_to_linear(data[50]),
        ];
    }
    p
}

/// Promote the verified Skyrim DNAM underwater fog pair from the extended
/// tail. Skyrim's vanilla 228-byte records carry the under-water near/far
/// values at byte offsets 144/148; the short 52-byte prefix used by older
/// fixtures simply leaves the canonical sentinel untouched.
fn apply_skyrim_dnam_tail(p: &mut WaterParams, data: &[u8]) {
    if data.len() < 228 {
        return;
    }
    let Some(near) = read_f32_at(data, 144) else {
        return;
    };
    let Some(far) = read_f32_at(data, 148) else {
        return;
    };
    p.underwater_fog_near = near.max(0.0);
    p.underwater_fog_far = far.max(p.underwater_fog_near + 1.0);
    // Skyrim's visible chop is authored by the displacement simulator, not
    // the legacy four-byte "Wave Amplitude" prefix (which xEdit marks
    // unused). Feed its force into the canonical vertex displacement slot;
    // retain the prefix value only when the extended field is absent.
    if let Some(force) = read_f32_at(data, 76) {
        p.wave_amplitude = force.clamp(0.0, 2.0);
    }
    for (slot, offset) in p.noise_wind_directions.iter_mut().zip([100, 104, 108]) {
        if let Some(direction_degrees) = read_f32_at(data, offset) {
            *slot = direction_degrees.to_radians();
        }
    }
    for (slot, offset) in p.noise_wind_speeds.iter_mut().zip([112, 116, 120]) {
        if let Some(speed) = read_f32_at(data, offset) {
            *slot = speed.max(0.0);
        }
    }
    p.wind_direction = p.noise_wind_directions[0];
    p.wind_speed = p.noise_wind_speeds[0];
    if let Some(v) = read_f32_at(data, 172) {
        p.noise_uv_scale_a = normalize_noise_uv_scale(v);
    }
    if let Some(v) = read_f32_at(data, 176) {
        p.noise_uv_scale_b = normalize_noise_uv_scale(v);
    }
    if let Some(v) = read_f32_at(data, 180) {
        p.noise_uv_scale_c = normalize_noise_uv_scale(v);
    }
    for (slot, offset) in p.noise_amplitude_scales.iter_mut().zip([184, 188, 192]) {
        if let Some(v) = read_f32_at(data, offset) {
            *slot = v.max(0.0);
        }
    }
    // Skyrim's physical normal magnitude precedes the noise falloff.
    if let Some(v) = read_f32_at(data, 92) {
        p.normal_magnitude = v.max(0.0);
    }
    if let Some(v) = read_f32_at(data, 132) {
        p.above_water_fog_amount = v.clamp(0.0, 8.0);
    }
    for (slot, offset) in p.depth_weights.iter_mut().zip([208, 212, 216, 220]) {
        if let Some(v) = read_f32_at(data, offset) {
            *slot = v.max(0.0);
        }
    }
    for (slot, offset) in p.effect_controls.iter_mut().zip([152, 156, 196, 204]) {
        if let Some(v) = read_f32_at(data, offset) {
            *slot = v.max(0.0);
        }
    }
    // Skyrim's unnamed specular-properties magnitude sits between local
    // specular power and radius in the canonical 228-byte layout.
    if let Some(v) = read_f32_at(data, 160) {
        p.specular_magnitude = v.max(0.0);
    }
    // Skyrim SE 232-byte records append the flow-map tile scale after the
    // common 228-byte DNAM payload. Older records retain the zero sentinel.
    if let Some(v) = read_f32_at(data, 228) {
        p.flowmap_scale = v.max(0.0);
    }
}

#[inline]
fn read_f32_at(data: &[u8], offset: usize) -> Option<f32> {
    let bytes: [u8; 4] = data.get(offset..offset + 4)?.try_into().ok()?;
    let value = f32::from_le_bytes(bytes);
    value.is_finite().then_some(value)
}

#[inline]
fn read_rgb_at(data: &[u8], offset: usize) -> Option<[f32; 3]> {
    let rgb = data.get(offset..offset + 3)?;
    Some([
        u8_to_linear(rgb[0]),
        u8_to_linear(rgb[1]),
        u8_to_linear(rgb[2]),
    ])
}

/// Decode Fallout 4's 201-byte `WATR.DNAM` visual-data structure.
///
/// This layout is not a prefixed Skyrim layout. Its first bytes are a
/// depth amount followed immediately by shallow/deep RGBA colours. Treating
/// them as Skyrim wind/wave floats made the default exterior ocean resolve
/// to saturated blue, full Fresnel/reflectivity, and a 20x normal strength.
/// The offsets below follow xEdit's authoritative FO4 record definition and
/// are also byte-checked against vanilla `ExtOceanWater` (FormID `0x18`).
fn decode_dnam_fo4(data: &[u8]) -> WaterParams {
    let mut p = WaterParams::default();

    // FO4's first float is the above-water depth/color ramp. Keep it
    // separate from the underwater near/far pair at 44/48.
    if let Some(depth_amount) = read_f32_at(data, 0) {
        p.fog_near = 0.0;
        p.fog_far = depth_amount.max(1.0);
    }

    if let Some(color) = read_rgb_at(data, 4) {
        p.shallow_color = color;
    }
    if let Some(color) = read_rgb_at(data, 8) {
        p.deep_color = color;
    }
    if let Some(color) = read_rgb_at(data, 36) {
        p.underwater_color = color;
    }
    if let Some(amount) = read_f32_at(data, 40) {
        p.underwater_fog_amount = amount.clamp(0.0, 8.0);
    }
    if let Some(near) = read_f32_at(data, 44) {
        p.underwater_fog_near = near.max(0.0);
    }
    if let Some(far) = read_f32_at(data, 48) {
        p.underwater_fog_far = far.max(p.underwater_fog_near + 1.0);
    }
    if let Some(reflectivity) = read_f32_at(data, 64) {
        p.reflectivity = reflectivity.clamp(0.0, 1.0);
    }
    if let Some(fresnel) = read_f32_at(data, 68) {
        p.fresnel = fresnel.clamp(0.0, 1.0);
    }
    if let Some(color) = read_rgb_at(data, 96) {
        p.reflection_color = color;
    }
    if let Some(power) = read_f32_at(data, 100) {
        p.sun_specular_power = power.clamp(1.0, 2048.0);
    }
    if let Some(magnitude) = read_f32_at(data, 104) {
        p.specular_magnitude = magnitude.max(0.0);
    }
    if let Some(magnitude) = read_f32_at(data, 52) {
        p.normal_magnitude = magnitude.max(0.0);
    }

    // FO4's physical section carries the authored displacement force and
    // velocity. Promote those into the canonical animated wave controls;
    // unlike the legacy Skyrim prefix these are live visual parameters.
    if let Some(force) = read_f32_at(data, 76) {
        p.wave_amplitude = force.max(0.0);
    }
    if let Some(velocity) = read_f32_at(data, 80) {
        p.wave_frequency = velocity.max(0.0);
    }

    // FO4 stores three noise-layer directions in degrees at 128/132/136 and
    // three layer speeds at 140/144/148. Preserve the authored vectors so
    // calm water can use the source motion instead of one generic scroll.
    for (slot, offset) in p.noise_wind_directions.iter_mut().zip([128, 132, 136]) {
        if let Some(direction_degrees) = read_f32_at(data, offset) {
            *slot = direction_degrees.to_radians();
        }
    }
    for (slot, offset) in p.noise_wind_speeds.iter_mut().zip([140, 144, 148]) {
        if let Some(speed) = read_f32_at(data, offset) {
            *slot = speed.max(0.0);
        }
    }
    p.wind_direction = p.noise_wind_directions[0];
    p.wind_speed = p.noise_wind_speeds[0];

    // FO4's three noise amplitudes and tile lengths map directly to the
    // canonical multi-layer normal path. xEdit stores UV scales as authored
    // tile lengths (normally around 100), while the renderer consumes their
    // inverse world-space frequency.
    for (slot, offset) in p.noise_amplitude_scales.iter_mut().zip([152, 156, 160]) {
        if let Some(value) = read_f32_at(data, offset) {
            *slot = value.max(0.0);
        }
    }
    for (slot, offset) in [
        &mut p.noise_uv_scale_a,
        &mut p.noise_uv_scale_b,
        &mut p.noise_uv_scale_c,
    ]
    .into_iter()
    .zip([164, 168, 172])
    {
        if let Some(value) = read_f32_at(data, offset) {
            *slot = normalize_noise_uv_scale(value);
        }
    }
    // FO4/FO76 append suspended-silt properties after the noise block:
    // amount (f32), light color (RGBA), dark color (RGBA). Keep the colors
    // separate until the translation boundary so the renderer can blend them
    // with the authored shallow/deep palette once, consistently for both eras.
    if let Some(amount) = read_f32_at(data, 176) {
        p.silt_amount = amount.clamp(0.0, 1.0);
    }
    if let Some(color) = read_rgb_at(data, 180) {
        p.silt_light_color = color;
    }
    if let Some(color) = read_rgb_at(data, 184) {
        p.silt_dark_color = color;
    }
    p
}

/// Decode Fallout 76's WATR visual data. Its fog/physical/specular prefix is
/// shared with FO4, but the 128..148 tail is five unnamed floats rather than
/// FO4 noise directions, speeds, amplitudes, and UV scales. Motion is carried
/// separately by the record's `NAM0` linear-velocity vector.
fn decode_dnam_fo76(data: &[u8]) -> WaterParams {
    let mut p = decode_dnam_fo4(data);
    p.noise_wind_directions = [0.0; 3];
    p.noise_wind_speeds = [0.0; 3];
    p.noise_uv_scale_a = 0.0;
    p.noise_uv_scale_b = 0.0;
    p.noise_uv_scale_c = 0.0;
    p.noise_amplitude_scales = [0.0; 3];
    p.wind_direction = 0.0;
    p.wind_speed = 0.0;
    p
}

/// Decode Starfield's WATR.DNAM visual data. Starfield replaced the RGB fog
/// ramp and legacy specular block with absorption/concentration properties;
/// its offsets are therefore not compatible with either Skyrim or FO4.
fn decode_dnam_starfield(data: &[u8]) -> WaterParams {
    let mut p = WaterParams::default();
    if let Some(depth) = read_f32_at(data, 0) {
        p.fog_near = 0.0;
        p.fog_far = depth.max(1.0);
    }
    // Starfield's first post-depth block is three independent color
    // absorption ranges (xEdit: Color Absorbtion Ranges). Preserve them as
    // distances rather than folding them into a guessed RGB tint; the
    // renderer can then apply Beer–Lambert per channel without losing the
    // authored water chemistry.
    for (slot, offset) in p.absorption_ranges.iter_mut().zip([4, 8, 12]) {
        if let Some(range) = read_f32_at(data, offset) {
            *slot = if range.is_finite() && range > 0.0 {
                range
            } else {
                0.0
            };
        }
    }
    if let Some(color) = read_rgb_at(data, 32) {
        p.underwater_color = color;
    }
    if let Some(amount) = read_f32_at(data, 36) {
        p.underwater_fog_amount = amount.clamp(0.0, 8.0);
    }
    if let Some(near) = read_f32_at(data, 40) {
        p.underwater_fog_near = near.max(0.0);
    }
    if let Some(far) = read_f32_at(data, 44) {
        p.underwater_fog_far = far.max(p.underwater_fog_near + 1.0);
    }
    if let Some(normal) = read_f32_at(data, 48) {
        p.normal_magnitude = normal.max(0.0);
    }
    if let Some(force) = read_f32_at(data, 64) {
        p.wave_amplitude = force.max(0.0);
    }
    if let Some(velocity) = read_f32_at(data, 68) {
        p.wave_frequency = velocity.max(0.0);
    }
    for (slot, offset) in p.noise_wind_directions.iter_mut().zip([84, 88, 92]) {
        if let Some(degrees) = read_f32_at(data, offset) {
            *slot = degrees.to_radians();
        }
    }
    for (slot, offset) in p.noise_wind_speeds.iter_mut().zip([96, 100, 104]) {
        if let Some(speed) = read_f32_at(data, offset) {
            *slot = speed.max(0.0);
        }
    }
    for (slot, offset) in p.noise_amplitude_scales.iter_mut().zip([108, 112, 116]) {
        if let Some(value) = read_f32_at(data, offset) {
            *slot = value.max(0.0);
        }
    }
    for (slot, offset) in [
        &mut p.noise_uv_scale_a,
        &mut p.noise_uv_scale_b,
        &mut p.noise_uv_scale_c,
    ]
    .into_iter()
    .zip([120, 124, 128])
    {
        if let Some(value) = read_f32_at(data, offset) {
            *slot = normalize_noise_uv_scale(value);
        }
    }
    // The post-noise fields are shared by Starfield's flow-map and surface
    // controls, not Skyrim's depth-response block.
    if let Some(scale) = read_f32_at(data, 144) {
        p.flowmap_scale = scale.max(0.0);
    }
    if let Some(roughness) = read_f32_at(data, 148) {
        p.roughness = roughness.clamp(0.0, 1.0);
    }
    p.wind_direction = p.noise_wind_directions[0];
    p.wind_speed = p.noise_wind_speeds[0];
    p
}

pub fn parse_watr(form_id: u32, subs: &[SubRecord], game: GameKind) -> WatrRecord {
    let mut out = WatrRecord {
        form_id,
        opacity: 0.75,
        ..Default::default()
    };
    // #2414 / TD2-117 — the universal named fields come from the
    // shared walker instead of a hand-rolled copy of its arms. It
    // ignores every other sub-record, so the per-record loop below
    // is unchanged.
    let common = CommonNamedFields::from_subs(subs);
    out.editor_id = common.editor_id;
    out.full_name = common.full_name;
    for sub in subs {
        match &sub.sub_type {
            b"ANAM" => {
                if let Some(&opacity) = sub.data.first() {
                    out.opacity = opacity as f32 / 255.0;
                }
            }
            b"TNAM" => out.texture_path = read_zstring(&sub.data),
            b"NNAM" => out.texture_path = read_zstring(&sub.data),
            b"NAM2" => out.noise_texture_paths[0] = read_zstring(&sub.data),
            b"NAM3" => out.noise_texture_paths[1] = read_zstring(&sub.data),
            b"NAM4" => out.noise_texture_paths[2] = read_zstring(&sub.data),
            b"NAM5" => out.flow_noise_texture_path = read_zstring(&sub.data),
            b"DATA" => {
                // Oblivion / FO3 / FNV path. The two byte layouts
                // are compatible on the 60-byte prefix we consume.
                out.params = decode_data(&sub.data);
                out.raw_data = sub.data.clone();
            }
            b"DNAM" => {
                out.params = match game {
                    GameKind::Fallout4 => decode_dnam_fo4(&sub.data),
                    GameKind::Fallout76 => decode_dnam_fo76(&sub.data),
                    // FO3/FNV and Skyrim share this prefix. FO76 and
                    // Starfield keep the same best-effort fallback until
                    // their divergent tails receive explicit projections.
                    GameKind::Skyrim => {
                        let mut p = decode_dnam_pre_fo4(&sub.data);
                        apply_skyrim_dnam_tail(&mut p, &sub.data);
                        p
                    }
                    GameKind::Starfield => decode_dnam_starfield(&sub.data),
                    _ => decode_dnam_pre_fo4(&sub.data),
                };
                out.raw_dnam = sub.data.clone();
            }
            b"GNAM" => {
                // 12 bytes = daytime/nighttime/underwater related waters.
                // Fewer bytes → unfilled slots stay at zero.
                let mut r = SubReader::new(&sub.data);
                for i in 0..3 {
                    if let Ok(fid) = r.u32() {
                        out.related_waters[i] = fid;
                    }
                }
            }
            _ => {}
        }
    }
    // WATR records carry a record-level linear-velocity vector in NAM0.
    // Project its Gamebryo Z-up horizontal (X/Y) components into the
    // renderer's X/Z plane (the Y component becomes -Z after the global
    // coordinate conversion). This is water-local motion, not the shared
    // weather wind used by SpeedTree.
    if let Some(sub) = subs.iter().find(|sub| sub.sub_type == *b"NAM0") {
        if let (Some(x), Some(y)) = (read_f32_at(&sub.data, 0), read_f32_at(&sub.data, 4)) {
            let speed = x.hypot(y);
            if speed.is_finite() && speed > 1.0e-5 {
                out.params.wind_speed = speed;
                out.params.wind_direction = (-y).atan2(x);
                out.params.noise_wind_speeds[0] = speed;
                out.params.noise_wind_directions[0] = out.params.wind_direction;
            }
        }
    }
    out
}

/// Adapter from a parsed `WatrRecord` onto a `WaterParams` view.
/// The per-game decode happens inside `parse_watr`; this helper just returns
/// the structured view.
pub fn watr_to_params(record: &WatrRecord) -> WaterParams {
    record.params
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn parse_watr_picks_edid_full_tnam() {
        let subs = vec![
            sub(b"EDID", b"WaterFreshDefault\0"),
            sub(b"FULL", b"Fresh Water\0"),
            sub(b"ANAM", &[192]),
            sub(b"TNAM", b"textures\\water\\fresh.dds\0"),
        ];
        let w = parse_watr(0x1234, &subs, GameKind::Skyrim);
        assert_eq!(w.form_id, 0x1234);
        assert_eq!(w.editor_id, "WaterFreshDefault");
        assert_eq!(w.full_name, "Fresh Water");
        assert_eq!(w.texture_path, "textures\\water\\fresh.dds");
        assert!((w.opacity - 192.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn parse_watr_decodes_data_fields() {
        // Construct a FO3/FNV-shaped DATA payload — 48 bytes covers
        // every field `decode_data` consumes.
        let mut data = Vec::with_capacity(48);
        data.extend_from_slice(&1.5f32.to_le_bytes()); // wind_speed
        data.extend_from_slice(&0.25f32.to_le_bytes()); // wind_direction
        data.extend_from_slice(&0.10f32.to_le_bytes()); // wave_amplitude
        data.extend_from_slice(&0.80f32.to_le_bytes()); // wave_frequency
        data.extend_from_slice(&37.0f32.to_le_bytes()); // sun_specular_power
        data.extend_from_slice(&0.65f32.to_le_bytes()); // reflectivity
        data.extend_from_slice(&0.04f32.to_le_bytes()); // fresnel
        data.extend_from_slice(&50.0f32.to_le_bytes()); // fog_near
        data.extend_from_slice(&400.0f32.to_le_bytes()); // fog_far
        data.extend_from_slice(&[0x20, 0x60, 0x80, 0xFF]); // shallow RGBA
        data.extend_from_slice(&[0x05, 0x0F, 0x18, 0xFF]); // deep RGBA
        data.extend_from_slice(&[0xC0, 0xD0, 0xE0, 0xFF]); // reflection RGBA

        let subs = vec![sub(b"DATA", &data)];
        let w = parse_watr(0xAAAA, &subs, GameKind::Fallout3NV);
        assert!((w.params.wind_speed - 1.5).abs() < 1e-6);
        assert!((w.params.wave_frequency - 0.80).abs() < 1e-6);
        assert!((w.params.sun_specular_power - 37.0).abs() < 1e-6);
        assert!((w.params.reflectivity - 0.65).abs() < 1e-6);
        assert!((w.params.fresnel - 0.04).abs() < 1e-6);
        assert!((w.params.fog_near - 50.0).abs() < 1e-3);
        assert!((w.params.fog_far - 400.0).abs() < 1e-3);
        // 0x20 = 32 → 32/255 ≈ 0.1255 — within tolerance.
        assert!((w.params.shallow_color[0] - (0x20 as f32 / 255.0)).abs() < 1e-6);
        assert!((w.params.deep_color[2] - (0x18 as f32 / 255.0)).abs() < 1e-6);
        assert_eq!(w.raw_data.len(), 48);
        assert!(w.raw_dnam.is_empty());
    }

    #[test]
    fn parse_watr_186_byte_record_reads_colors_at_40_44_48() {
        // Regression for #1778 — the real FO3/FNV 186-byte WATR.DATA
        // carries an extra fog-distance f32 at offset 36, so the RGBA
        // colour block sits at 40/44/48, NOT 36/40/44. Bytes mirror the
        // real `PPurityWater01Murky` (Fallout3.esm) record.
        let mut data = vec![0u8; 186];
        // offset 36-39: the extra fog f32 (109.0) the legacy code mistook
        // for the shallow colour — its low bytes are obvious garbage as RGB.
        data[36..40].copy_from_slice(&109.0f32.to_le_bytes());
        // offset 40/44/48: the real shallow / deep / reflection colours.
        data[40..44].copy_from_slice(&[36, 47, 36, 0]); // shallow
        data[44..48].copy_from_slice(&[13, 13, 11, 0]); // deep
        data[48..52].copy_from_slice(&[41, 48, 46, 0]); // reflection
        data[144..148].copy_from_slice(&18.0f32.to_le_bytes()); // underwater near
        data[148..152].copy_from_slice(&240.0f32.to_le_bytes()); // underwater far
        data[172..176].copy_from_slice(&(1.0 / 320.0f32).to_le_bytes()); // noise UV 1
        data[176..180].copy_from_slice(&(1.0 / 760.0f32).to_le_bytes()); // noise UV 2
        data[180..184].copy_from_slice(&488.0f32.to_le_bytes()); // noise UV 3 (legacy length)

        let w = parse_watr(0x00100000, &[sub(b"DATA", &data)], GameKind::Fallout3NV);
        assert!((w.params.shallow_color[0] - 36.0 / 255.0).abs() < 1e-6);
        assert!((w.params.shallow_color[1] - 47.0 / 255.0).abs() < 1e-6);
        assert!((w.params.deep_color[2] - 11.0 / 255.0).abs() < 1e-6);
        assert!((w.params.reflection_color[0] - 41.0 / 255.0).abs() < 1e-6);
        assert!((w.params.reflection_color[2] - 46.0 / 255.0).abs() < 1e-6);
        assert_eq!(w.params.underwater_fog_near, 18.0);
        assert_eq!(w.params.underwater_fog_far, 240.0);
        assert!((w.params.noise_uv_scale_a - 1.0 / 320.0).abs() < 1e-6);
        assert!((w.params.noise_uv_scale_b - 1.0 / 760.0).abs() < 1e-6);
        assert!((w.params.noise_uv_scale_c - 1.0 / 488.0).abs() < 1e-6);
        // Guard against the off-by-4 regression: reading shallow @36 would
        // pick up the fog float's bytes (0x00,0x00,0xda → [0,0,218]), so a
        // blue-channel of 218/255 here means the offset shift was lost.
        assert!(
            (w.params.shallow_color[2] - 218.0 / 255.0).abs() > 1e-3,
            "shallow colour read from the fog f32 at offset 36 (off-by-4 regression)"
        );
    }

    #[test]
    fn parse_watr_short_data_keeps_defaults_past_buffer_end() {
        // 12 bytes — only wind_speed, wind_direction, wave_amplitude
        // get decoded; everything else stays at default.
        let mut data = Vec::with_capacity(12);
        data.extend_from_slice(&3.0f32.to_le_bytes());
        data.extend_from_slice(&1.0f32.to_le_bytes());
        data.extend_from_slice(&0.5f32.to_le_bytes());
        let subs = vec![sub(b"DATA", &data)];
        let w = parse_watr(0xBBBB, &subs, GameKind::Fallout3NV);
        assert!((w.params.wind_speed - 3.0).abs() < 1e-6);
        assert!((w.params.wave_amplitude - 0.5).abs() < 1e-6);
        // Defaults preserved past offset 12.
        assert!((w.params.fog_near - 80.0).abs() < 1e-3);
        assert!((w.params.fog_far - 600.0).abs() < 1e-3);
        assert!((w.params.fresnel - 0.02).abs() < 1e-6);
    }

    #[test]
    fn parse_watr_decodes_dnam_skyrim_prefix() {
        // Skyrim DNAM with the exact xEdit TES5 52-byte prefix (the
        // shortest that fills every decoded field).
        let mut data = Vec::with_capacity(152);
        data.extend_from_slice(&2.0f32.to_le_bytes()); // wind_speed @ 0
        data.extend_from_slice(&1.2f32.to_le_bytes()); // wind_direction @ 4
        data.extend_from_slice(&0.20f32.to_le_bytes()); // wave_amplitude @ 8
        data.extend_from_slice(&0.55f32.to_le_bytes()); // wave_frequency @ 12
        data.extend_from_slice(&61.0f32.to_le_bytes()); // sun specular power @ 16
        data.extend_from_slice(&0.75f32.to_le_bytes()); // reflectivity @ 20
        data.extend_from_slice(&0.03f32.to_le_bytes()); // fresnel
        data.extend_from_slice(&0.0f32.to_le_bytes()); // unnamed @ 28
        data.extend_from_slice(&60.0f32.to_le_bytes()); // fog_near
        data.extend_from_slice(&500.0f32.to_le_bytes()); // fog_far
        data.extend_from_slice(&[0x10, 0x40, 0x70, 0xFF]); // shallow
        data.extend_from_slice(&[0x02, 0x08, 0x10, 0xFF]); // deep
        data.extend_from_slice(&[0xA0, 0xB0, 0xC0, 0xFF]); // reflection

        let subs = vec![sub(b"DNAM", &data)];
        let w = parse_watr(0xCCCC, &subs, GameKind::Skyrim);
        assert!((w.params.wind_speed - 2.0).abs() < 1e-6);
        assert!((w.params.wind_direction - 1.2).abs() < 1e-6);
        assert!((w.params.wave_amplitude - 0.20).abs() < 1e-6);
        assert!((w.params.wave_frequency - 0.55).abs() < 1e-6);
        assert!((w.params.sun_specular_power - 61.0).abs() < 1e-6);
        assert!((w.params.reflectivity - 0.75).abs() < 1e-6);
        assert!((w.params.fog_far - 500.0).abs() < 1e-3);
        assert_eq!(w.raw_dnam.len(), 52);
        assert!(w.raw_data.is_empty());

        // The extended Skyrim tail promotes the underwater fog pair.
        data.resize(228, 0);
        data[76..80].copy_from_slice(&0.4f32.to_le_bytes());
        data[92..96].copy_from_slice(&0.05f32.to_le_bytes());
        data[132..136].copy_from_slice(&0.75f32.to_le_bytes());
        data[144..148].copy_from_slice(&(-1000.0f32).to_le_bytes());
        data[148..152].copy_from_slice(&1000.0f32.to_le_bytes());
        data[172..176].copy_from_slice(&1920.0f32.to_le_bytes());
        data[176..180].copy_from_slice(&6703.0f32.to_le_bytes());
        data[180..184].copy_from_slice(&488.0f32.to_le_bytes());
        data[184..188].copy_from_slice(&0.7f32.to_le_bytes());
        data[188..192].copy_from_slice(&0.6f32.to_le_bytes());
        data[192..196].copy_from_slice(&0.5f32.to_le_bytes());
        data[208..212].copy_from_slice(&0.9f32.to_le_bytes());
        data[212..216].copy_from_slice(&0.5f32.to_le_bytes());
        data[216..220].copy_from_slice(&0.1f32.to_le_bytes());
        data[220..224].copy_from_slice(&0.2f32.to_le_bytes());
        data[152..156].copy_from_slice(&9.0f32.to_le_bytes());
        data[156..160].copy_from_slice(&500.0f32.to_le_bytes());
        data[160..164].copy_from_slice(&2.0f32.to_le_bytes());
        data[196..200].copy_from_slice(&0.34f32.to_le_bytes());
        data[204..208].copy_from_slice(&3.2f32.to_le_bytes());
        data.resize(232, 0);
        data[228..232].copy_from_slice(&1.75f32.to_le_bytes());
        data[100..104].copy_from_slice(&270.0f32.to_le_bytes());
        data[104..108].copy_from_slice(&210.0f32.to_le_bytes());
        data[108..112].copy_from_slice(&225.0f32.to_le_bytes());
        data[112..116].copy_from_slice(&0.019f32.to_le_bytes());
        data[116..120].copy_from_slice(&0.013f32.to_le_bytes());
        data[120..124].copy_from_slice(&0.096f32.to_le_bytes());
        let w = parse_watr(0xCCCC, &[sub(b"DNAM", &data)], GameKind::Skyrim);
        assert_eq!(w.params.underwater_fog_near, 0.0);
        assert!((w.params.underwater_fog_far - 1000.0).abs() < 1e-3);
        assert_eq!(w.params.wave_amplitude, 0.4);
        assert!((w.params.noise_uv_scale_a - 1.0 / 1920.0).abs() < 1e-6);
        assert_eq!(w.params.noise_amplitude_scales, [0.7, 0.6, 0.5]);
        assert_eq!(w.params.depth_weights, [0.9, 0.5, 0.1, 0.2]);
        assert_eq!(w.params.effect_controls, [9.0, 500.0, 0.34, 3.2]);
        assert_eq!(w.params.specular_magnitude, 2.0);
        assert_eq!(w.params.normal_magnitude, 0.05);
        assert_eq!(w.params.above_water_fog_amount, 0.75);
        assert_eq!(w.params.noise_wind_directions[0], 270.0f32.to_radians());
        assert_eq!(w.params.noise_wind_directions[1], 210.0f32.to_radians());
        assert_eq!(w.params.noise_wind_speeds, [0.019, 0.013, 0.096]);
        assert_eq!(w.params.flowmap_scale, 1.75);
    }

    #[test]
    fn parse_watr_projects_skyrim_nam0_velocity_into_renderer_water_motion() {
        let mut velocity = Vec::new();
        velocity.extend_from_slice(&3.0f32.to_le_bytes());
        velocity.extend_from_slice(&4.0f32.to_le_bytes());
        velocity.extend_from_slice(&0.0f32.to_le_bytes());
        let w = parse_watr(
            0xCAFE,
            &[sub(b"DNAM", &[0; 228]), sub(b"NAM0", &velocity)],
            GameKind::Skyrim,
        );
        assert_eq!(w.params.wind_speed, 5.0);
        assert!((w.params.wind_direction - (-4.0f32).atan2(3.0)).abs() < 1e-6);
        assert_eq!(w.params.noise_wind_speeds[0], 5.0);
    }

    #[test]
    fn parse_watr_decodes_fo4_visual_data_layout() {
        // Vanilla Fallout4.esm ExtOceanWater (FormID 0x18) values at the
        // exact xEdit FO4 offsets. The full record is 201 bytes; fields not
        // represented by WaterParams remain zero in this focused fixture.
        let mut data = vec![0u8; 201];
        data[0..4].copy_from_slice(&3007.0f32.to_le_bytes()); // depth amount
        data[4..8].copy_from_slice(&[45, 62, 62, 0]); // shallow colour
        data[8..12].copy_from_slice(&[46, 61, 57, 0]); // deep colour
        data[36..40].copy_from_slice(&[18, 27, 36, 0]); // underwater colour
        data[40..44].copy_from_slice(&0.75f32.to_le_bytes()); // underwater fog amount
        data[44..48].copy_from_slice(&(-6400.0f32).to_le_bytes()); // underwater near
        data[48..52].copy_from_slice(&1700.0f32.to_le_bytes()); // underwater far
        data[64..68].copy_from_slice(&0.2935f32.to_le_bytes()); // reflectivity
        data[68..72].copy_from_slice(&0.058f32.to_le_bytes()); // Fresnel
        data[96..100].copy_from_slice(&[51, 68, 70, 0]); // reflection colour
        data[100..104].copy_from_slice(&83.0f32.to_le_bytes()); // sun specular power
        data[104..108].copy_from_slice(&1.25f32.to_le_bytes()); // sun specular magnitude
        data[76..80].copy_from_slice(&0.4f32.to_le_bytes()); // displacement force
        data[80..84].copy_from_slice(&0.6f32.to_le_bytes()); // displacement velocity
        data[52..56].copy_from_slice(&0.5f32.to_le_bytes()); // normal magnitude
        data[128..132].copy_from_slice(&67.824f32.to_le_bytes()); // layer 1 direction (deg)
        data[132..136].copy_from_slice(&210.0f32.to_le_bytes()); // layer 2 direction
        data[136..140].copy_from_slice(&315.0f32.to_le_bytes()); // layer 3 direction
        data[140..144].copy_from_slice(&0.0109f32.to_le_bytes()); // layer 1 speed
        data[144..148].copy_from_slice(&0.020f32.to_le_bytes()); // layer 2 speed
        data[148..152].copy_from_slice(&0.030f32.to_le_bytes()); // layer 3 speed
        data[152..156].copy_from_slice(&0.8f32.to_le_bytes()); // layer 1 amplitude
        data[156..160].copy_from_slice(&0.6f32.to_le_bytes()); // layer 2 amplitude
        data[160..164].copy_from_slice(&0.4f32.to_le_bytes()); // layer 3 amplitude
        data[164..168].copy_from_slice(&200.0f32.to_le_bytes()); // layer 1 UV
        data[168..172].copy_from_slice(&400.0f32.to_le_bytes()); // layer 2 UV
        data[172..176].copy_from_slice(&800.0f32.to_le_bytes()); // layer 3 UV
        data[176..180].copy_from_slice(&0.65f32.to_le_bytes()); // silt amount
        data[180..184].copy_from_slice(&[170, 150, 120, 0]); // silt light
        data[184..188].copy_from_slice(&[55, 45, 35, 0]); // silt dark

        let w = parse_watr(0x18, &[sub(b"DNAM", &data)], GameKind::Fallout4);
        assert_eq!(w.raw_dnam.len(), 201);
        assert!((w.params.shallow_color[0] - 45.0 / 255.0).abs() < 1e-6);
        assert!((w.params.deep_color[2] - 57.0 / 255.0).abs() < 1e-6);
        assert!((w.params.underwater_color[1] - 27.0 / 255.0).abs() < 1e-6);
        assert_eq!(w.params.underwater_fog_amount, 0.75);
        assert!((w.params.reflection_color[1] - 68.0 / 255.0).abs() < 1e-6);
        assert_eq!(w.params.fog_near, 0.0);
        assert_eq!(w.params.fog_far, 3007.0);
        assert_eq!(w.params.underwater_fog_near, 0.0);
        assert_eq!(w.params.underwater_fog_far, 1700.0);
        assert!((w.params.reflectivity - 0.2935).abs() < 1e-6);
        assert!((w.params.fresnel - 0.058).abs() < 1e-6);
        assert!((w.params.sun_specular_power - 83.0).abs() < 1e-6);
        assert_eq!(w.params.specular_magnitude, 1.25);
        assert_eq!(w.params.normal_magnitude, 0.5);
        assert!((w.params.wind_direction - 67.824f32.to_radians()).abs() < 1e-6);
        assert!((w.params.wind_speed - 0.0109).abs() < 1e-6);
        assert_eq!(w.params.noise_wind_directions[1], 210.0f32.to_radians());
        assert_eq!(w.params.noise_wind_speeds, [0.0109, 0.020, 0.030]);
        assert_eq!(w.params.wave_amplitude, 0.4);
        assert_eq!(w.params.wave_frequency, 0.6);
        assert_eq!(w.params.noise_amplitude_scales, [0.8, 0.6, 0.4]);
        assert_eq!(w.params.noise_uv_scale_a, 1.0 / 200.0);
        assert_eq!(w.params.noise_uv_scale_b, 1.0 / 400.0);
        assert_eq!(w.params.noise_uv_scale_c, 1.0 / 800.0);
        assert_eq!(w.params.silt_amount, 0.65);
        assert!((w.params.silt_light_color[0] - 170.0 / 255.0).abs() < 1e-6);
        assert!((w.params.silt_dark_color[2] - 35.0 / 255.0).abs() < 1e-6);
    }

    #[test]
    fn parse_watr_picks_nnam_fo3_fnv_path() {
        // Regression for #1271 — every vanilla FO3 WATR (and every
        // vanilla FNV WATR) ships its noise/diffuse texture path in
        // `NNAM`, not `TNAM`. Payload mirrors `PotomacNRShallow` from
        // Fallout3.esm.
        let subs = vec![
            sub(b"EDID", b"PotomacNRShallow\0"),
            sub(
                b"NNAM",
                b"Data\\Textures\\Water\\WastelandWaterPotomac.dds\0",
            ),
        ];
        let w = parse_watr(0x100A8C, &subs, GameKind::Fallout3NV);
        assert_eq!(w.editor_id, "PotomacNRShallow");
        assert_eq!(
            w.texture_path,
            "Data\\Textures\\Water\\WastelandWaterPotomac.dds"
        );
    }

    #[test]
    fn parse_watr_decodes_named_noise_paths_and_gnam_related_waters() {
        let mut gnam = Vec::with_capacity(12);
        gnam.extend_from_slice(&0x11111111u32.to_le_bytes());
        gnam.extend_from_slice(&0x22222222u32.to_le_bytes());
        gnam.extend_from_slice(&0x33333333u32.to_le_bytes());
        let subs = vec![
            sub(b"NAM2", b"textures\\water\\noise01.dds\0"),
            sub(b"NAM3", b"textures\\water\\noise02.dds\0"),
            sub(b"NAM4", b"textures\\water\\noise03.dds\0"),
            sub(b"NAM5", b"textures\\water\\flow.dds\0"),
            sub(b"GNAM", &gnam),
        ];
        let w = parse_watr(0xDDDD, &subs, GameKind::Skyrim);
        assert_eq!(
            w.noise_texture_paths,
            [
                "textures\\water\\noise01.dds",
                "textures\\water\\noise02.dds",
                "textures\\water\\noise03.dds",
            ]
        );
        assert_eq!(w.flow_noise_texture_path, "textures\\water\\flow.dds");
        assert_eq!(w.related_waters, [0x11111111, 0x22222222, 0x33333333]);
    }

    #[test]
    fn parse_watr_routes_fo76_to_fo4_layout() {
        let mut data = vec![0u8; 201];
        data[0..4].copy_from_slice(&900.0f32.to_le_bytes());
        data[4..8].copy_from_slice(&[10, 20, 30, 0]);
        data[52..56].copy_from_slice(&0.6f32.to_le_bytes());
        data[128..132].copy_from_slice(&999.0f32.to_le_bytes()); // FO76 unknown, not a direction
        let mut velocity = Vec::new();
        velocity.extend_from_slice(&3.0f32.to_le_bytes());
        velocity.extend_from_slice(&4.0f32.to_le_bytes());
        velocity.extend_from_slice(&0.0f32.to_le_bytes());
        let w = parse_watr(
            0x18,
            &[sub(b"DNAM", &data), sub(b"NAM0", &velocity)],
            GameKind::Fallout76,
        );
        assert_eq!(w.params.fog_far, 900.0);
        assert_eq!(w.params.normal_magnitude, 0.6);
        assert!((w.params.shallow_color[2] - 30.0 / 255.0).abs() < 1e-6);
        assert_eq!(w.params.wind_speed, 5.0);
        assert!((w.params.wind_direction - (-4.0f32).atan2(3.0)).abs() < 1e-6);
        assert_eq!(w.params.noise_wind_speeds, [5.0, 0.0, 0.0]);
    }

    #[test]
    fn parse_watr_decodes_starfield_visual_layout() {
        let mut data = vec![0u8; 152];
        data[0..4].copy_from_slice(&1200.0f32.to_le_bytes());
        data[4..8].copy_from_slice(&0.1f32.to_le_bytes());
        data[8..12].copy_from_slice(&0.2f32.to_le_bytes());
        data[12..16].copy_from_slice(&0.3f32.to_le_bytes());
        data[32..35].copy_from_slice(&[12, 24, 36]);
        data[36..40].copy_from_slice(&0.7f32.to_le_bytes());
        data[40..44].copy_from_slice(&4.0f32.to_le_bytes());
        data[44..48].copy_from_slice(&80.0f32.to_le_bytes());
        data[48..52].copy_from_slice(&0.45f32.to_le_bytes());
        data[64..68].copy_from_slice(&0.25f32.to_le_bytes());
        data[68..72].copy_from_slice(&0.5f32.to_le_bytes());
        data[84..88].copy_from_slice(&90.0f32.to_le_bytes());
        data[96..100].copy_from_slice(&0.02f32.to_le_bytes());
        data[108..112].copy_from_slice(&0.8f32.to_le_bytes());
        data[120..124].copy_from_slice(&200.0f32.to_le_bytes());
        data[144..148].copy_from_slice(&3.0f32.to_le_bytes());
        data[148..152].copy_from_slice(&0.5f32.to_le_bytes());
        let w = parse_watr(0x1234, &[sub(b"DNAM", &data)], GameKind::Starfield);
        assert_eq!(w.params.fog_far, 1200.0);
        assert_eq!(w.params.absorption_ranges, [0.1, 0.2, 0.3]);
        assert_eq!(w.params.underwater_fog_amount, 0.7);
        assert_eq!(w.params.underwater_fog_near, 4.0);
        assert_eq!(w.params.underwater_fog_far, 80.0);
        assert_eq!(w.params.normal_magnitude, 0.45);
        assert_eq!(w.params.wave_amplitude, 0.25);
        assert_eq!(w.params.wave_frequency, 0.5);
        assert_eq!(w.params.wind_direction, 90.0f32.to_radians());
        assert_eq!(w.params.wind_speed, 0.02);
        assert_eq!(w.params.noise_amplitude_scales[0], 0.8);
        assert_eq!(w.params.noise_uv_scale_a, 1.0 / 200.0);
        assert_eq!(w.params.depth_weights, [0.0; 4]);
        assert_eq!(w.params.flowmap_scale, 3.0);
        assert_eq!(w.params.roughness, 0.5);
    }
}
