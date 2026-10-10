//! Lift metric Starfield scene data into the common engine distance unit.
//! FO76 authors its scene in engine units but shares Starfield's WATR DNAM
//! decoder, and with it the per-metre absorption triplet (#5169); that one
//! lane is the only FO76 lift.
//!
//! Called exactly once after a plugin's binary walk, before either public
//! index is returned or merged. Per-record readers still expose wire values.
//! NIF import performs the matching mesh/bone conversion. No game-dependent
//! distance multiplier belongs in lighting, skinning, camera or rendering.

use super::EsmIndex;
use crate::esm::cell::{CellData, CellLighting};
use crate::esm::reader::GameKind;
use byroredux_core::lighting::BETHESDA_UNITS_PER_METER as UNITS;

fn vector(v: &mut [f32; 3]) {
    for x in v {
        *x *= UNITS;
    }
}

fn optional(v: &mut Option<f32>) {
    if let Some(x) = v {
        *x *= UNITS;
    }
}

fn lighting(light: &mut CellLighting) {
    light.fog_near *= UNITS;
    light.fog_far *= UNITS;
    optional(&mut light.fog_clip);
    optional(&mut light.light_fade_begin);
    optional(&mut light.light_fade_end);
    if let Some(sf) = &mut light.starfield {
        sf.near_height_mid *= UNITS;
        sf.near_height_range *= UNITS;
        sf.far_height_mid *= UNITS;
        sf.far_height_range *= UNITS;
    }
}

fn cell(cell: &mut CellData) {
    for refr in &mut cell.references {
        vector(&mut refr.position);
        if let Some(dest) = &mut refr.teleport {
            vector(&mut dest.position);
        }
        if let Some(primitive) = &mut refr.primitive {
            vector(&mut primitive.bounds);
        }
        if let Some(velocity) = &mut refr.water_velocity {
            vector(velocity);
        }
        optional(&mut refr.radius_override);
        // #5299 — XRGD ragdoll-pose positions are metric on Starfield
        // (median entry offset 0.162 vs FO4's 15.33 BU), the same distance
        // lane as `refr.position`; the Euler rotations are dimensionless
        // and stay wire-valued.
        for bone in &mut refr.ragdoll_pose {
            vector(&mut bone.position);
        }
        // REFR XSCL and Euler angles are dimensionless. SCOL composition
        // must not multiply the unit conversion again through parent scale.
    }
    if let Some(light) = &mut cell.lighting {
        lighting(light);
    }
    optional(&mut cell.water_height);
    if let Some(land) = &mut cell.landscape {
        for height in &mut land.heights {
            *height *= UNITS;
        }
    }
}

/// Per-metre WATR absorption → per-BU, for the slots the record's DNAM
/// actually authored (offsets 4/8/12). A short or absent DNAM leaves the
/// decoder's engine-unit defaults, which must not be lifted.
fn watr_absorption_per_metre(water: &mut super::misc::WatrRecord) {
    for (slot, offset) in water
        .params
        .absorption_coefficients
        .iter_mut()
        .zip([4, 8, 12])
    {
        if water.raw_dnam.len() >= offset + 4 {
            *slot /= UNITS;
        }
    }
}

pub(super) fn normalize(index: &mut EsmIndex) {
    match index.game {
        GameKind::Starfield => {}
        // #5169 — FO76's WATR distances are BU (underwater fog -9000 / 850,
        // noise falloff 4096, UV tiles 279+), but its absorption triplet is
        // per-metre like Starfield's: five vanilla records author exactly
        // Starfield's 0.3 / 0.075 / 0.01, and `ExtClearWaterDefault`'s
        // 0.24 / 0.18 / 0.2 read per BU would make clear water opaque within
        // ~4 BU (6 cm). Only that lane is lifted.
        GameKind::Fallout76 => {
            for water in index.waters.values_mut() {
                watr_absorption_per_metre(water);
            }
            return;
        }
        _ => return,
    }
    for c in index
        .cells
        .cells
        .values_mut()
        .chain(
            index
                .cells
                .exterior_cells
                .values_mut()
                .flat_map(|cells| cells.values_mut()),
        )
        .chain(index.cells.worldspace_persistent_cells.values_mut())
    {
        cell(c);
    }
    for object in index.cells.statics.values_mut() {
        if let Some(light) = &mut object.light_data {
            light.radius *= UNITS;
            light.movement_amplitude *= UNITS;
        }
    }
    for scol in index.cells.scols.values_mut() {
        for part in &mut scol.parts {
            for placement in &mut part.placements {
                vector(&mut placement.pos);
            }
        }
    }
    for world in index.cells.worldspaces.values_mut() {
        world.usable_min.0 *= UNITS;
        world.usable_min.1 *= UNITS;
        world.usable_max.0 *= UNITS;
        world.usable_max.1 *= UNITS;
        optional(&mut world.default_water_height);
        optional(&mut world.lod_water_height);
    }
    for light in index.lighting_templates.values_mut() {
        light.fog_near *= UNITS;
        light.fog_far *= UNITS;
        optional(&mut light.fog_clip);
        optional(&mut light.light_fade_begin);
        optional(&mut light.light_fade_end);
        // #5002 — the template's SF height-fog model lifts exactly like
        // the XCLL one in `lighting()` above: the four height
        // mid/ranges ×UNITS, the dimensionless scales untouched.
        if let Some(sf) = &mut light.starfield {
            sf.near_height_mid *= UNITS;
            sf.near_height_range *= UNITS;
            sf.far_height_mid *= UNITS;
            sf.far_height_range *= UNITS;
        }
    }
    // #5134 — WTHR fog distances are metric on Starfield, same as the
    // XCLL/LGTM fog lifted above: Starfield.esm's `DefaultWeather` FNAM
    // reads 10 / 3000 (metres, matching the metric LGTM corpus), and
    // `translate_weather` feeds these fields to the fog-medium fitter
    // which divides by `BETHESDA_UNITS_PER_METER`. Unlifted, every
    // Starfield exterior with a resolved climate fitted a ~70× too-dense
    // medium (3000 m read as 42.9 m). Keep `translate_weather` unchanged
    // — units are settled before the boundary.
    // #5001 — Starfield now decodes the FO4-form height-fog tail, so the
    // eight height mid/range fields (day/night × near/far × mid/range)
    // get the same ×70 lift. The two high-density scales are
    // dimensionless and stay untouched, per the same rule that keeps
    // `light_fade`-style ratios out of the lift.
    // #5301 — only AUTHORED distances are lifted. `parse_wthr` leaves
    // `fog_{day,night}_far` at the engine-unit default 10 000 when no FNAM
    // was decoded, and lifting that default would turn "no fog authored"
    // into a 700 000-unit far plane — the same rule the WATR lanes below
    // keep ("a short or absent DNAM leaves the decoder's defaults, which
    // must not be lifted").
    for weather in index.weathers.values_mut() {
        if weather.fog_distances_authored {
            weather.fog_day_near *= UNITS;
            weather.fog_day_far *= UNITS;
            weather.fog_night_near *= UNITS;
            weather.fog_night_far *= UNITS;
        }
        if let Some(height) = &mut weather.fog_height {
            height.day_near_height_mid *= UNITS;
            height.day_near_height_range *= UNITS;
            height.night_near_height_mid *= UNITS;
            height.night_near_height_range *= UNITS;
            height.day_far_height_mid *= UNITS;
            height.day_far_height_range *= UNITS;
            height.night_far_height_mid *= UNITS;
            height.night_far_height_range *= UNITS;
        }
    }
    // #5151 — Starfield WATR DNAM distances are metric (vanilla `WaterClear`:
    // depth 8, underwater fog -150 / 75, noise falloff 100) and its
    // absorption triplet is per-metre extinction (0.16558 / 0.09624 /
    // 0.07627 — liquid water's red > green > blue). Lengths ×UNITS, inverse
    // lengths ÷UNITS. Only fields the record actually authored are touched:
    // a short or absent DNAM leaves the decoder's engine-unit defaults,
    // which must not be lifted. #5170 — the unit of three lane families is
    // still unsettled (capture-gated tracker): the noise-UV tile sizes
    // (120/124/128, vanilla 72.11 / 39 / 13), the displacement simulator
    // (72/76/80) and the normal falloff triplet (52/56/60). None is lifted
    // here; `starfield_watr_defers_the_unclassified_dnam_lanes` pins that
    // deferral.
    for water in index.waters.values_mut() {
        watr_absorption_per_metre(water);
        let authored = |offset: usize| water.raw_dnam.len() >= offset + 4;
        let (depth, near, far, falloff) =
            (authored(0), authored(40), authored(44), authored(132));
        let p = &mut water.params;
        if depth {
            p.depth_amount *= UNITS;
        }
        if near {
            p.underwater_fog_near *= UNITS;
        }
        if far {
            p.underwater_fog_far *= UNITS;
        }
        if falloff {
            p.noise_falloff *= UNITS;
        }
    }
    // NAVM was drained from cells into this one map before normalization.
    // Opaque packed payloads remain wire data; only decoded geometry is lifted.
    for nav in index.navmeshes.values_mut() {
        for position in &mut nav.vertices {
            vector(position);
        }
        if let Some(grid) = &mut nav.grid_accel {
            for bound in &mut grid.bounds {
                *bound *= UNITS;
            }
        }
    }
}
