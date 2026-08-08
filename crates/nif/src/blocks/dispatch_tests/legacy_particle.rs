//! Regression tests for #2526 / NIF-D1-NEW-01 — `legacy_particle.rs`'s
//! version-gated fields were previously read unconditionally, corrupting
//! stream position for any genuine pre-Gamebryo (Morrowind-era) instance.
//!
//! Zero real corpus coverage exists for this band (none of the 7
//! supported games can author these block types — `NiAutoNormalParticles`
//! / `NiRotatingParticles` / `NiParticleBomb` are all `until="V10_0_1_0"`
//! per nif.xml, and every supported game is past that ceiling), so these
//! are synthetic byte-stream fixtures at exactly the version boundaries
//! nif.xml declares, per the issue's own suggested fix.

use crate::blocks::parse_block;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;

fn header_at(version: NifVersion) -> NifHeader {
    NifHeader {
        version,
        little_endian: true,
        user_version: 0,
        user_version_2: 0,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// `NiAVObjectData` body for a version strictly below `V10_0_1_0` (the
/// `< V10_0_1_0` branch throughout `NiObjectNETData::parse` /
/// `NiAVObjectData::parse`): inline empty name, single extra-data ref,
/// controller ref, u16 flags, inline Translation→Rotation→Scale
/// transform, Velocity (present at `<= V4_2_2_0`), empty properties
/// list, and the pre-`V10_0_1_0` bounding-volume-skip collision path
/// (`Has Bounding Volume = false`, itself a 32-bit bool below
/// `V4_1_0_1`).
fn niavobject_bytes_pre_v10_0_1_0() -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&0u32.to_le_bytes()); // name: inline string len=0
    d.extend_from_slice(&(-1i32).to_le_bytes()); // extra_data: single ref, null
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    d.extend_from_slice(&0u16.to_le_bytes()); // flags (u16, bsver<=26)
                                               // transform: translation(3f32) + rotation(9f32 identity) + scale(f32)
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes());
    }
    for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        for v in row {
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
    d.extend_from_slice(&0.0f32.to_le_bytes()); // velocity.x (<= V4_2_2_0)
    d.extend_from_slice(&0.0f32.to_le_bytes()); // velocity.y
    d.extend_from_slice(&0.0f32.to_le_bytes()); // velocity.z
    d.extend_from_slice(&0u32.to_le_bytes()); // properties: empty list
    d.extend_from_slice(&0u32.to_le_bytes()); // Has Bounding Volume = false (32-bit bool, < V4_1_0_1)
    d
}

/// `NiAVObjectData` body at exactly `V10_0_1_0`: block_ref_list-form
/// extra data, no Velocity (present only `<= V4_2_2_0`), and a
/// dedicated collision `BlockRef` instead of the bounding-volume skip.
fn niavobject_bytes_at_v10_0_1_0() -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&0u32.to_le_bytes()); // name: inline string len=0 (still < STRING_TABLE_THRESHOLD)
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs: empty list (>= V10_0_1_0)
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
    d.extend_from_slice(&0u16.to_le_bytes()); // flags (u16, bsver<=26)
    for _ in 0..3 {
        d.extend_from_slice(&0.0f32.to_le_bytes()); // translation
    }
    for row in [[1.0f32, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] {
        for v in row {
            d.extend_from_slice(&v.to_le_bytes()); // rotation (identity)
        }
    }
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
                                                 // no velocity — version > V4_2_2_0
    d.extend_from_slice(&0u32.to_le_bytes()); // properties: empty list
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref (dedicated, >= V10_0_1_0)
    d
}

/// Regression for #2526 / NIF-D1-NEW-01. `NiGeometry.Has Shader`
/// (nif.xml `since="10.0.1.0" until="20.1.0.3"`) must NOT be read below
/// its own `since` ceiling — at true Morrowind (v4.0.0.2), reading it
/// unconditionally consumed a phantom byte with no data behind it.
#[test]
fn ni_auto_normal_particles_below_v10_0_1_0_skips_has_shader_phantom_byte() {
    let header = header_at(NifVersion::V4_0_0_2);
    let mut data = niavobject_bytes_pre_v10_0_1_0();
    data.extend_from_slice(&(-1i32).to_le_bytes()); // data_ref
    data.extend_from_slice(&(-1i32).to_le_bytes()); // skin_instance_ref
                                                      // NO has_shader byte — below the field's own `since` ceiling.
    let mut stream = NifStream::new(&data, &header);
    let block = parse_block("NiAutoNormalParticles", &mut stream, Some(data.len() as u32))
        .expect("must parse cleanly with has_shader gated off below V10_0_1_0");
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "must consume exactly the fixture's bytes — no phantom has_shader read"
    );
    let m = block
        .as_any()
        .downcast_ref::<crate::blocks::legacy_particle::NiLegacyParticles>()
        .unwrap();
    assert!(!m.has_shader);
}

/// Companion: at exactly `V10_0_1_0` — the single-point intersection of
/// the field's own `since="10.0.1.0"` and this object's own
/// `until="V10_0_1_0"` — `Has Shader` IS present and must be read.
///
/// `V10_0_1_0` also falls inside `NifVersion::has_object_group_id`'s
/// `[V10_0_0_0, V10_1_0_114)` band, so `parse_block_inner` itself
/// consumes an extra 4-byte `groupID` prefix (`#688`) before dispatch —
/// unrelated to this issue's fix, but required for the fixture to be
/// byte-accurate at this version.
#[test]
fn ni_auto_normal_particles_at_exactly_v10_0_1_0_reads_has_shader() {
    let header = header_at(NifVersion::V10_0_1_0);
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes()); // groupID (#688, has_object_group_id band)
    data.extend_from_slice(&niavobject_bytes_at_v10_0_1_0());
    data.extend_from_slice(&(-1i32).to_le_bytes()); // data_ref
    data.extend_from_slice(&(-1i32).to_le_bytes()); // skin_instance_ref
    data.push(0u8); // has_shader = false (8-bit bool, >= V4_1_0_1)
    let mut stream = NifStream::new(&data, &header);
    let block = parse_block("NiAutoNormalParticles", &mut stream, Some(data.len() as u32))
        .expect("must parse cleanly with has_shader gated on at exactly V10_0_1_0");
    assert_eq!(stream.position() as usize, data.len());
    let m = block
        .as_any()
        .downcast_ref::<crate::blocks::legacy_particle::NiLegacyParticles>()
        .unwrap();
    assert!(!m.has_shader);
}

/// Regression for #2526 / NIF-D1-NEW-01. `NiParticlesData.Has Radii`
/// (since="10.1.0.0", no `until`) and `NiRotatingParticlesData`'s `Has
/// Rotation Angles`/`Has Rotation Axes` (since="20.0.0.4", no `until`)
/// are structurally unreachable for any version this object type can
/// exist at (`until="V10_0_1_0"`) — they must never be read. `Has
/// Rotations` (since="10.0.1.0", no `until`) shares `Has Shader`'s
/// single-point valid window; at v4.0.0.2 it must also be skipped.
#[test]
fn ni_auto_normal_particles_data_at_v4_0_0_2_skips_all_structurally_unreachable_fields() {
    let header = header_at(NifVersion::V4_0_0_2);
    let mut data = Vec::new();
    // parse_geometry_data_base at < V10_0_1_0: no group_id, no
    // keep/compress flags, no data_flags field, no consistency flags,
    // no additional_data_ref.
    data.extend_from_slice(&0u16.to_le_bytes()); // num_vertices = 0
    data.extend_from_slice(&0u32.to_le_bytes()); // has_vertices = false (32-bit bool)
    data.extend_from_slice(&0u32.to_le_bytes()); // has_normals = false (32-bit bool)
    for _ in 0..3 {
        data.extend_from_slice(&0.0f32.to_le_bytes()); // bounding sphere center
    }
    data.extend_from_slice(&0.0f32.to_le_bytes()); // bounding sphere radius
    data.extend_from_slice(&0u32.to_le_bytes()); // has_vertex_colors = false (32-bit bool)
    data.extend_from_slice(&0u16.to_le_bytes()); // num_uv_sets (< V10_0_1_0 inline u16)
    data.extend_from_slice(&0u32.to_le_bytes()); // has_uv = false (32-bit bool, <= V4_0_0_2)
                                                  // NO has_radii byte — structurally unreachable.
    data.extend_from_slice(&0u16.to_le_bytes()); // num_active
    data.push(0u8); // has_sizes = false (unconditional 1-byte bool, no version gate)
                     // NO has_rotations byte — below V10_0_1_0.
                     // NO has_rotation_angles / has_rotation_axes bytes — structurally unreachable.

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block(
        "NiAutoNormalParticlesData",
        &mut stream,
        Some(data.len() as u32),
    )
    .expect("must parse cleanly with all four fields gated off at v4.0.0.2");
    assert_eq!(
        stream.position() as usize,
        data.len(),
        "must consume exactly the fixture's bytes — no phantom reads for \
         has_radii / has_rotations / has_rotation_angles / has_rotation_axes"
    );
    let m = block
        .as_any()
        .downcast_ref::<crate::blocks::legacy_particle::NiLegacyParticlesData>()
        .unwrap();
    assert!(m.radii.is_empty());
    assert!(m.rotations.is_empty());
    assert!(m.rotation_angles.is_empty());
    assert!(m.rotation_axes.is_empty());
}

fn niparticlebomb_prefix_and_scalars() -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&(-1i32).to_le_bytes()); // next_modifier
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    for v in [0.1f32, 1.0, 2.0, 0.0] {
        d.extend_from_slice(&v.to_le_bytes()); // decay, duration, delta_v, start
    }
    d.extend_from_slice(&1u32.to_le_bytes()); // decay_type
    d
}

fn niparticlebomb_position_and_direction() -> Vec<u8> {
    let mut d = Vec::new();
    for v in [0.0f32, 0.0, 0.0, 0.0, 0.0, 1.0] {
        d.extend_from_slice(&v.to_le_bytes()); // position, direction
    }
    d
}

/// Regression for #2526 / NIF-D1-NEW-01. `NiParticleBomb.Symmetry Type`
/// (since="4.1.0.12") must not be read below that version — true
/// Morrowind (v4.0.0.2) predates it.
#[test]
fn ni_particle_bomb_below_v4_1_0_12_skips_symmetry_type() {
    let header = header_at(NifVersion::V4_0_0_2);
    let mut data = niparticlebomb_prefix_and_scalars();
    // NO symmetry_type — below the field's own since ceiling.
    data.extend_from_slice(&niparticlebomb_position_and_direction());

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block("NiParticleBomb", &mut stream, Some(data.len() as u32))
        .expect("must parse cleanly with symmetry_type gated off below V4_1_0_12");
    assert_eq!(stream.position() as usize, data.len());
    let m = block
        .as_any()
        .downcast_ref::<crate::blocks::legacy_particle::NiParticleBomb>()
        .unwrap();
    assert_eq!(m.symmetry_type, 0);
}

/// Companion: at exactly `V4_1_0_12` — the field's own `since` ceiling,
/// and still within `NiParticleBomb`'s own `until="V10_0_1_0"` — Symmetry
/// Type IS present and must be read.
#[test]
fn ni_particle_bomb_at_exactly_v4_1_0_12_reads_symmetry_type() {
    let header = header_at(NifVersion::V4_1_0_12);
    let mut data = niparticlebomb_prefix_and_scalars();
    data.extend_from_slice(&7u32.to_le_bytes()); // symmetry_type
    data.extend_from_slice(&niparticlebomb_position_and_direction());

    let mut stream = NifStream::new(&data, &header);
    let block = parse_block("NiParticleBomb", &mut stream, Some(data.len() as u32))
        .expect("must parse cleanly with symmetry_type gated on at exactly V4_1_0_12");
    assert_eq!(stream.position() as usize, data.len());
    let m = block
        .as_any()
        .downcast_ref::<crate::blocks::legacy_particle::NiParticleBomb>()
        .unwrap();
    assert_eq!(m.symmetry_type, 7);
}
