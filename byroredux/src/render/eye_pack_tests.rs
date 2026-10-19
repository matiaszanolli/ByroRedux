//! #5961 — the kind-16 (EyeEnvmap) pack gate: the authored eye fields
//! reach the GPU material only on an eye, and only as a complete set —
//! the shader's eye branch (`triangle.frag`'s `isEyeEnvmap`) then
//! requires an authored cubemap plus a positive scale before it diverts
//! the fragment from the generic reflection path. The issue's
//! CANONICAL-BOUNDARY check rides on this shape: the fields resolve at
//! the NIFAL parser→`Material` boundary (the nif import's
//! `ShaderTypeFields`, pinned by `shader_type_data_tests.rs`) and the
//! pack forwards them verbatim — no per-game re-derivation anywhere.

use byroredux_core::ecs::components::material::ShaderTypeFields;

use super::static_meshes::pack_eye_fields;

/// The vanilla shape: a Skyrim eye property with both centres and a
/// positive scale packs all three, which is what the eye branch reads.
#[test]
fn kind_16_packs_the_authored_eye_fields() {
    let stf = ShaderTypeFields {
        eye_cubemap_scale: Some(1.25),
        eye_left_reflection_center: Some([0.1, 0.2, 0.3]),
        eye_right_reflection_center: Some([0.4, 0.5, 0.6]),
        ..ShaderTypeFields::default()
    };
    assert_eq!(
        pack_eye_fields(16, Some(&stf)),
        ([0.1, 0.2, 0.3], 1.25, [0.4, 0.5, 0.6]),
        "kind 16 forwards the authored centres + scale to the shader"
    );
}

/// The same authored payload on any other kind packs zeros — the eye
/// branch must be unreachable for mis-keyed fields (the MultiLayer
/// sibling carries none of these fields even when a shared carrier
/// leaks them).
#[test]
fn non_eye_kinds_pack_zeros_despite_authored_fields() {
    let stf = ShaderTypeFields {
        eye_cubemap_scale: Some(1.25),
        eye_left_reflection_center: Some([0.1, 0.2, 0.3]),
        eye_right_reflection_center: Some([0.4, 0.5, 0.6]),
        ..ShaderTypeFields::default()
    };
    for kind in [0u32, 1, 2, 11, 100] {
        assert_eq!(
            pack_eye_fields(kind, Some(&stf)),
            ([0.0; 3], 0.0, [0.0; 3]),
            "kind {kind} packs no eye fields"
        );
    }
}

/// Kind 16 with no authored payload packs zeros — the zero scale is
/// what the shader's `eyeCubemapScale > 0.0` guard reads, keeping a
/// data-less eye on the generic reflection path instead of reflecting
/// around the world origin.
#[test]
fn kind_16_without_authored_fields_stays_on_the_generic_path() {
    assert_eq!(
        pack_eye_fields(16, None),
        ([0.0; 3], 0.0, [0.0; 3]),
        "missing payload packs the zero scale the shader guards on"
    );
    assert_eq!(
        pack_eye_fields(16, Some(&ShaderTypeFields::default())),
        ([0.0; 3], 0.0, [0.0; 3]),
        "an empty payload behaves like no payload"
    );
}
