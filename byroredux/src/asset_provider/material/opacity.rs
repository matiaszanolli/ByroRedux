//! Starfield opacity: the one place its source and cutout rule are decided
//! (docs/prd/starfield-material-maps.md §5.1, §5.2; SMM-SRC-001).
//!
//! The compiled database and loose `.mat` files both reach this as a
//! [`CdbMaterial`], so they cannot resolve the same data two ways
//! (SMM-SRC-002). The format rules rest on the `fo76utils` viewer, recorded
//! in `docs/engine/nifal.md` ("Starfield material-map role contract").

use byroredux_nif::import::{ImportedMaterial, OpacityHeld};
use byroredux_sfmaterial::{CdbMaterial, SLOT_OPACITY};

use crate::asset_provider::constant_texture::constant_texture_path;

/// The cutout threshold when nothing on the `Parent` chain authors one
/// (SMM-OPA-004).
const DEFAULT_ALPHA_TEST_THRESHOLD: f32 = 1.0 / 3.0;

/// `alphaComparePass` function 4, `alpha > threshold` (SMM-OPA-004). The
/// engine's default is 6, greater-or-equal.
const ALPHA_TEST_GREATER: u8 = 4;

/// Apply a resolved Starfield material's cutout rule, and return the texture
/// path its opacity role should take: the Map, or the generated texture
/// holding a Constant. `None` is Absent, with the rule recorded on the
/// material when it authors opacity data. The caller fills the role by the
/// merge's NIF-first precedence (SMM-SRC-006).
pub(super) fn apply_cdb_opacity(
    material: &mut ImportedMaterial,
    cdb_mat: &CdbMaterial,
    touched: &mut bool,
) -> Option<String> {
    // #5196 / #5661 — opacity and its cutout apply only where an object on
    // the `Parent` chain authors `HasOpacity = true`. `None` means none
    // does, and the class default is false: the vanilla CDB's diff census
    // finds `true` authored 1,449 times, always over an inherited `None`.
    // That default is measured, not sourced. Treating `None` as present
    // alpha-tested 1,265 of 10,152 materials against base-colour alpha.
    let authors_slot = authors_opacity(cdb_mat);
    if cdb_mat.has_opacity != Some(true) {
        // SMM-OPA-006, whatever slot 2 holds.
        material.opacity_held = authors_slot.then_some(OpacityHeld::NoHasOpacity);
        return None;
    }

    // SMM-OPA-004 — tested when the threshold is above zero.
    let threshold = cdb_mat
        .alpha_test_threshold
        .unwrap_or(DEFAULT_ALPHA_TEST_THRESHOLD);
    if threshold > 0.0 {
        material.alpha_test = true;
        material.alpha_threshold = threshold;
        material.alpha_test_func = ALPHA_TEST_GREATER;
        *touched = true;
    }

    // SMM-OPA-005 — texture and replacement from the named layer alone.
    let Some(layer) = &cdb_mat.opacity_layer else {
        material.opacity_held = Some(OpacityHeld::UnresolvedLayer);
        return None;
    };
    // SMM-SRC-005 — the texture wins; the replacement is the source only
    // when the layer authors no texture path. A texture that then fails to
    // load shades as Absent (SMM-SRC-011), not as the replacement.
    let source = layer
        .texture(SLOT_OPACITY)
        .map(str::to_owned)
        .or_else(|| {
            // SMM-SRC-004 — the red value of the replacement, linear.
            layer
                .replacement(SLOT_OPACITY)
                .map(|[red, ..]| constant_texture_path(unorm8(red)))
        });
    if source.is_none() {
        material.opacity_held = authors_slot.then_some(OpacityHeld::UnsuppliedLayer);
    }
    source
}

/// Whether any layer authors a texture or an enabled replacement for slot 2.
/// An empty path is no texture.
fn authors_opacity(cdb_mat: &CdbMaterial) -> bool {
    cdb_mat
        .textures
        .iter()
        .any(|(slot, path)| *slot == SLOT_OPACITY && !path.is_empty())
        || cdb_mat
            .flat_color_slots
            .iter()
            .any(|(slot, _)| *slot == SLOT_OPACITY)
}

fn unorm8(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}
