//! Starfield roughness and metalness: the one place their sources are decided
//! (docs/prd/starfield-material-maps.md §5.1, §5.3; SMM-SRC-001).
//!
//! The compiled database and loose `.mat` files both reach this as a
//! [`CdbMaterial`], so they cannot resolve the same data two ways
//! (SMM-SRC-002). The format rules rest on the `fo76utils` viewer, recorded
//! in `docs/engine/nifal.md` ("Starfield material-map role contract").

use byroredux_core::ecs::components::material::PbrMaterial;
use byroredux_nif::import::{ImportedMaterial, PbrSource};
use byroredux_sfmaterial::{CdbMaterial, SLOT_METALNESS, SLOT_ROUGHNESS};

/// The texture paths the two roles should take: the database Maps. The caller
/// fills the roles by the merge's NIF-first precedence (SMM-SRC-006).
#[derive(Debug, Default)]
pub(super) struct PbrMapPaths {
    pub(super) roughness: Option<String>,
    pub(super) metalness: Option<String>,
}

/// Decide a resolved Starfield material's roughness and metalness: record each
/// source, set each canonical scalar, and return the Maps to bind.
///
/// A material that already carries the decision keeps it, so a second merge
/// cannot read the first one's scalars as another source's.
pub(super) fn apply_cdb_pbr(
    material: &mut ImportedMaterial,
    cdb_mat: &CdbMaterial,
    touched: &mut bool,
) -> PbrMapPaths {
    // A value the NIF legacy classifier filled in is a guess from names and
    // shader scalars, not something another source authored. No installed
    // Starfield shape carries one (all 339,858 shapes naming a material, over
    // the PRD §3.3 models, 2026-11-26).
    let guessed = material.pbr_classified_at_import;
    let neutral = PbrMaterial::NO_SIGNAL_NEUTRAL;
    let mut maps = PbrMapPaths::default();
    if material.roughness_source.is_none() {
        let decided = decide(
            cdb_mat,
            SLOT_ROUGHNESS,
            guessed,
            neutral.roughness,
            &mut material.roughness_override,
            touched,
        );
        material.roughness_source = Some(decided.0);
        maps.roughness = decided.1;
    }
    if material.metalness_source.is_none() {
        let decided = decide(
            cdb_mat,
            SLOT_METALNESS,
            guessed,
            neutral.metalness,
            &mut material.metalness_override,
            touched,
        );
        material.metalness_source = Some(decided.0);
        maps.metalness = decided.1;
    }
    maps
}

/// One property's source (PRD §3.1). `neutral` is the Absent fallback.
fn decide(
    cdb_mat: &CdbMaterial,
    slot: u8,
    guessed: bool,
    neutral: f32,
    scalar: &mut Option<f32>,
    touched: &mut bool,
) -> (PbrSource, Option<String>) {
    // SMM-SRC-006 — an authored value keeps precedence over Map and Constant.
    if scalar.is_some() && !guessed {
        return (PbrSource::Override, None);
    }
    // SMM-SRC-010 — texture and replacement from one layer. SMM-SRC-005 — the
    // texture wins.
    if let Some(path) = cdb_mat.one_layer.texture(slot) {
        // SMM-SRC-011 — what shades if the texture cannot load.
        *scalar = Some(neutral);
        *touched = true;
        return (PbrSource::Map, Some(path.to_owned()));
    }
    // SMM-SRC-004 — the red value of the replacement, linear.
    if let Some([red, ..]) = cdb_mat.one_layer.replacement(slot) {
        let value = red.clamp(0.0, 1.0);
        *scalar = Some(value);
        *touched = true;
        return (PbrSource::Constant(value), None);
    }
    // SMM-SRC-007 / #5197 — today's value: whatever the importer had, else the
    // neutral. Left empty, `Material::resolve_pbr`'s backstop would run the
    // keyword classifier over the database's colour path and read conductors
    // out of asset names.
    if scalar.is_none() {
        *scalar = Some(neutral);
        *touched = true;
    }
    (PbrSource::Absent, None)
}
