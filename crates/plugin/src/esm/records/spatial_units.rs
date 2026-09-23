//! Lift metric Starfield scene data into the common engine distance unit.
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

pub(super) fn normalize(index: &mut EsmIndex) {
    if index.game != GameKind::Starfield {
        return;
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
