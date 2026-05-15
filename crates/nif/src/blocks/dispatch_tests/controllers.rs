//! Controller dispatch tests.
//!
//! NiFlipController, NiBSBoneLODController (Oblivion + Bethesda paths), UV
//! controller, KF-animation blocks, legacy particle-system controller.

use super::oblivion_header;
use crate::blocks::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

/// Regression for #394 — `NiFlipController` on Oblivion (>= 10.1.0.104)
/// gates off `Accum Time` and `Delta` fields, so the disk layout
/// reduces to NiTimeController base (26) + NiSingleInterpController
/// interpolator_ref (4) + texture_slot (4) + num_sources (4) +
/// sources[N] (4 each). Test with N=3 sources → 42 bytes total.
#[test]
fn ni_flip_controller_consumes_full_body_oblivion_layout() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // NiTimeController base: next(i32) + flags(u16) + freq(f32) +
    // phase(f32) + start(f32) + stop(f32) + target(i32) = 26 B
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // stop
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target
                                                     // NiSingleInterpController: interpolator_ref (4 B).
    bytes.extend_from_slice(&5i32.to_le_bytes());
    // NiFlipController: texture_slot(4) + num_sources(4) + sources.
    bytes.extend_from_slice(&4u32.to_le_bytes()); // texture_slot = GLOW_MAP
    bytes.extend_from_slice(&3u32.to_le_bytes()); // num_sources
    bytes.extend_from_slice(&11i32.to_le_bytes());
    bytes.extend_from_slice(&12i32.to_le_bytes());
    bytes.extend_from_slice(&13i32.to_le_bytes());
    assert_eq!(bytes.len(), 26 + 4 + 4 + 4 + 4 * 3);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("NiFlipController", &mut stream, Some(bytes.len() as u32))
        .expect("NiFlipController must parse on Oblivion");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiFlipController>()
        .unwrap();
    assert_eq!(ctrl.texture_slot, 4);
    assert_eq!(ctrl.sources.len(), 3);
    assert_eq!(ctrl.sources[0].index(), Some(11));
    assert_eq!(ctrl.sources[2].index(), Some(13));
    assert_eq!(ctrl.base.interpolator_ref.index(), Some(5));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #394 — `NiBSBoneLODController` with one LOD (1
/// bone) + one shape group (1 skin info) + one shape_groups_2
/// entry. Creature-skeleton LOD block on every vanilla Oblivion
/// creature NIF; without this parser, every block after it was
/// truncated.
#[test]
fn ni_bs_bone_lod_controller_consumes_full_body() {
    let header = oblivion_header();
    let mut bytes = Vec::new();
    // NiTimeController base (26 B).
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // LOD + counts.
    bytes.extend_from_slice(&0u32.to_le_bytes()); // lod
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_lods
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_node_groups (unused)
                                                  // Node Groups: NodeSet { num_nodes=1, nodes=[42] }.
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&42i32.to_le_bytes());
    // Shape Groups 1: SkinInfoSet { num_skin_info=1, [shape_ptr=7, skin_instance=8] }.
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_shape_groups
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_skin_info
    bytes.extend_from_slice(&7i32.to_le_bytes()); // shape_ptr
    bytes.extend_from_slice(&8i32.to_le_bytes()); // skin_instance
                                                  // Shape Groups 2: [ref 99].
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_shape_groups_2
    bytes.extend_from_slice(&99i32.to_le_bytes());
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiBSBoneLODController",
        &mut stream,
        Some(bytes.len() as u32),
    )
    .expect("NiBSBoneLODController must parse on Oblivion");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiBsBoneLodController>()
        .unwrap();
    assert_eq!(ctrl.lod, 0);
    assert_eq!(ctrl.node_groups.len(), 1);
    assert_eq!(ctrl.node_groups[0].nodes.len(), 1);
    assert_eq!(ctrl.node_groups[0].nodes[0].index(), Some(42));
    assert_eq!(ctrl.shape_groups_1.len(), 1);
    assert_eq!(ctrl.shape_groups_1[0].skin_infos.len(), 1);
    assert_eq!(
        ctrl.shape_groups_1[0].skin_infos[0].shape_ptr.index(),
        Some(7)
    );
    assert_eq!(ctrl.shape_groups_2.len(), 1);
    assert_eq!(ctrl.shape_groups_2[0].index(), Some(99));
    assert_eq!(stream.position() as usize, bytes.len());
}

/// `NiBSBoneLODController` on Bethesda content (bsver != 0) must
/// stop after `node_groups` and skip the `#NISTREAM#`-gated
/// shape-group tail. Pre-fix the parser ate 4+ extra bytes past
/// the block, hit `0xFFFFFFFF` reading the next block's data as
/// `Num Shape Groups`, and bailed via `allocate_vec`. Surfaced by
/// the R3 per-block histogram on FNV creature skeletons (34
/// instances all advertising as `NiUnknown`). Sized to mirror the
/// failing block 6 from `meshes/characters/_male/skeleton.nif`:
/// 26 (base) + 4 (lod) + 4 (num_lods=1) + 4 (num_node_groups) +
/// 4 (num_nodes=5) + 5×4 (ptrs) = 62 bytes total.
#[test]
fn ni_bs_bone_lod_controller_skips_shape_groups_on_bethesda() {
    // FNV header — bsver=34, the BSVER on every creature skeleton
    // that R3 surfaced.
    let header = NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 11,
        user_version_2: 34,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("SkyProp")],
        max_string_length: 8,
        num_groups: 0,
    };
    let mut bytes = Vec::new();
    // NiTimeController base (26 B).
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&0.0f32.to_le_bytes());
    bytes.extend_from_slice(&1.0f32.to_le_bytes());
    bytes.extend_from_slice(&(-1i32).to_le_bytes());
    // LOD + counts.
    bytes.extend_from_slice(&0u32.to_le_bytes()); // lod
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_lods
    bytes.extend_from_slice(&1u32.to_le_bytes()); // num_node_groups (unused)
                                                  // Node Groups: NodeSet { num_nodes=5, nodes=[10,11,12,13,14] }.
    bytes.extend_from_slice(&5u32.to_le_bytes()); // num_nodes
    for ptr in 10i32..15 {
        bytes.extend_from_slice(&ptr.to_le_bytes());
    }
    // No shape-group fields — Bethesda content stops here.
    assert_eq!(bytes.len(), 62);
    // Pre-fix tripwire: a sentinel u32 right after the body so a
    // regressed parser that keeps reading past `bytes.len()` would
    // hit `0xFFFFFFFF` and bail in `allocate_vec`. The
    // `Some(bytes.len() as u32)` block-size cap below already
    // bounds the parser; this is belt-and-braces.
    bytes.extend_from_slice(&u32::MAX.to_le_bytes());
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block(
        "NiBSBoneLODController",
        &mut stream,
        // Block-size hint covers only the real body — pre-fix
        // parser ignored block_size for end-of-block detection
        // and read past it anyway.
        Some(62),
    )
    .expect("NiBSBoneLODController must parse on Bethesda BSVER!=0");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiBsBoneLodController>()
        .unwrap();
    assert_eq!(ctrl.lod, 0);
    assert_eq!(ctrl.node_groups.len(), 1);
    assert_eq!(ctrl.node_groups[0].nodes.len(), 5);
    assert_eq!(ctrl.node_groups[0].nodes[0].index(), Some(10));
    assert_eq!(ctrl.node_groups[0].nodes[4].index(), Some(14));
    // Shape-groups are absent on Bethesda content per #NISTREAM#.
    assert!(ctrl.shape_groups_1.is_empty());
    assert!(ctrl.shape_groups_2.is_empty());
    // Stream must stop exactly at end of body — no overshoot into
    // the sentinel u32 we stamped past byte 62.
    assert_eq!(stream.position(), 62);
}

/// Regression test for issue #144: Oblivion-era KF animation roots
/// must dispatch through the right parsers.
#[test]
fn oblivion_kf_animation_blocks_route_correctly() {
    // NiKeyframeController: parses as NiSingleInterpController
    // (26-byte NiTimeControllerBase + 4-byte interpolator ref).
    let header = oblivion_header();
    let mut kf_bytes = Vec::new();
    // NiTimeControllerBase: next_controller, flags, frequency, phase,
    // start_time, stop_time, target_ref.
    kf_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    kf_bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    kf_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    kf_bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    kf_bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    kf_bytes.extend_from_slice(&1.0f32.to_le_bytes()); // stop_time
    kf_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref
    kf_bytes.extend_from_slice(&7i32.to_le_bytes()); // interpolator_ref
    let mut stream = NifStream::new(&kf_bytes, &header);
    let block = parse_block(
        "NiKeyframeController",
        &mut stream,
        Some(kf_bytes.len() as u32),
    )
    .expect("NiKeyframeController should dispatch through NiSingleInterpController");
    let ctrl = block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiSingleInterpController>()
        .expect("NiKeyframeController did not downcast to NiSingleInterpController");
    assert_eq!(ctrl.interpolator_ref.index(), Some(7));

    // NiSequenceStreamHelper: NiObjectNET with no extra fields.
    // name (string table index 0) + extra_data count (0) + controller ref (-1)
    let mut ssh_bytes = Vec::new();
    ssh_bytes.extend_from_slice(&0i32.to_le_bytes()); // name
    ssh_bytes.extend_from_slice(&0u32.to_le_bytes()); // extra_data count
    ssh_bytes.extend_from_slice(&(-1i32).to_le_bytes()); // controller
    let mut stream = NifStream::new(&ssh_bytes, &header);
    let block = parse_block(
        "NiSequenceStreamHelper",
        &mut stream,
        Some(ssh_bytes.len() as u32),
    )
    .expect("NiSequenceStreamHelper should dispatch to its own parser");
    assert!(block
        .as_any()
        .downcast_ref::<crate::blocks::controller::NiSequenceStreamHelper>()
        .is_some());
}

/// Regression test for issue #154: NiUVController + NiUVData.
#[test]
fn oblivion_uv_controller_and_data_roundtrip() {
    use crate::blocks::controller::NiUVController;
    use crate::blocks::interpolator::NiUVData;

    let header = oblivion_header();

    // NiUVController: NiTimeControllerBase (26 bytes) + u16 target + i32 data ref.
    let mut uvc = Vec::new();
    uvc.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    uvc.extend_from_slice(&0u16.to_le_bytes()); // flags
    uvc.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    uvc.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    uvc.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    uvc.extend_from_slice(&2.5f32.to_le_bytes()); // stop_time
    uvc.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref
    uvc.extend_from_slice(&0u16.to_le_bytes()); // target_attribute
    uvc.extend_from_slice(&42i32.to_le_bytes()); // data ref
    let mut stream = NifStream::new(&uvc, &header);
    let block = parse_block("NiUVController", &mut stream, Some(uvc.len() as u32))
        .expect("NiUVController dispatch");
    let c = block.as_any().downcast_ref::<NiUVController>().unwrap();
    assert_eq!(c.target_attribute, 0);
    assert_eq!(c.data_ref.index(), Some(42));
    assert!((c.base.stop_time - 2.5).abs() < 1e-6);
    assert_eq!(stream.position(), uvc.len() as u64);

    // NiUVData: four KeyGroup<FloatKey>. First group has 2 linear
    // keys scrolling U from 0→1; the rest are empty.
    let mut uvd = Vec::new();
    // Group 0: num_keys=2, key_type=Linear(1), key (time, value)×2
    uvd.extend_from_slice(&2u32.to_le_bytes());
    uvd.extend_from_slice(&1u32.to_le_bytes()); // KeyType::Linear
    uvd.extend_from_slice(&0.0f32.to_le_bytes()); // t=0
    uvd.extend_from_slice(&0.0f32.to_le_bytes()); // v=0
    uvd.extend_from_slice(&1.0f32.to_le_bytes()); // t=1
    uvd.extend_from_slice(&1.0f32.to_le_bytes()); // v=1
                                                  // Groups 1-3: num_keys=0 (no key_type field when empty).
    for _ in 0..3 {
        uvd.extend_from_slice(&0u32.to_le_bytes());
    }
    let mut stream = NifStream::new(&uvd, &header);
    let block =
        parse_block("NiUVData", &mut stream, Some(uvd.len() as u32)).expect("NiUVData dispatch");
    let d = block.as_any().downcast_ref::<NiUVData>().unwrap();
    assert_eq!(d.groups[0].keys.len(), 2);
    assert_eq!(d.groups[0].keys[1].value, 1.0);
    assert!(d.groups[1].keys.is_empty());
    assert!(d.groups[2].keys.is_empty());
    assert!(d.groups[3].keys.is_empty());
    assert_eq!(stream.position(), uvd.len() as u64);
}

/// Regression test for issue #143: NiParticleSystemController with
/// zero particles. Verifies the huge scalar field chain consumes
/// the expected byte count.
#[test]
fn oblivion_legacy_particle_system_controller_roundtrip() {
    use crate::blocks::legacy_particle::NiParticleSystemController;

    let header = oblivion_header();

    // NiTimeControllerBase: 26 bytes.
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // next_controller
    bytes.extend_from_slice(&0u16.to_le_bytes()); // flags
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // frequency
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // phase
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // start_time
    bytes.extend_from_slice(&3.0f32.to_le_bytes()); // stop_time
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // target_ref

    // Controller body scalar soup — mostly zeros, non-zero marker
    // values to verify specific field offsets.
    for v in [
        50.0f32, // speed
        5.0,     // speed_variation
        0.0,     // declination
        0.5,     // declination_variation
        0.0,     // planar_angle
        6.28,    // planar_angle_variation
    ] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    // initial_normal (vec3)
    for v in [0.0f32, 0.0, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    // initial_color (RGBA)
    for v in [1.0f32, 0.5, 0.25, 1.0] {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes.extend_from_slice(&1.5f32.to_le_bytes()); // initial_size
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // emit_start_time
    bytes.extend_from_slice(&10.0f32.to_le_bytes()); // emit_stop_time
    bytes.push(0u8); // reset_particle_system
    bytes.extend_from_slice(&25.0f32.to_le_bytes()); // birth_rate
    bytes.extend_from_slice(&2.0f32.to_le_bytes()); // lifetime
    bytes.extend_from_slice(&0.5f32.to_le_bytes()); // lifetime_variation
    bytes.push(1u8); // use_birth_rate
    bytes.push(0u8); // spawn_on_death
    for v in [0.0f32; 3] {
        bytes.extend_from_slice(&v.to_le_bytes());
    } // emitter_dimensions
    bytes.extend_from_slice(&0xDEADBEEFu32.to_le_bytes()); // emitter ptr hash
    bytes.extend_from_slice(&1u16.to_le_bytes()); // num_spawn_generations
    bytes.extend_from_slice(&1.0f32.to_le_bytes()); // percentage_spawned
    bytes.extend_from_slice(&1u16.to_le_bytes()); // spawn_multiplier
    bytes.extend_from_slice(&0.1f32.to_le_bytes()); // spawn_speed_chaos
    bytes.extend_from_slice(&0.1f32.to_le_bytes()); // spawn_dir_chaos

    bytes.extend_from_slice(&0u16.to_le_bytes()); // num_particles
    bytes.extend_from_slice(&0u16.to_le_bytes()); // num_valid
                                                  // No particle records.
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // unknown_ref
    bytes.extend_from_slice(&0u32.to_le_bytes()); // num_emitter_points
    bytes.extend_from_slice(&0u32.to_le_bytes()); // trailer_emitter_type
    bytes.extend_from_slice(&0.0f32.to_le_bytes()); // unknown_trailer_float
    bytes.extend_from_slice(&(-1i32).to_le_bytes()); // trailer_emitter_modifier

    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block(
        "NiParticleSystemController",
        &mut s,
        Some(bytes.len() as u32),
    )
    .expect("NiParticleSystemController dispatch");
    let c = b
        .as_any()
        .downcast_ref::<NiParticleSystemController>()
        .unwrap();
    assert!((c.speed - 50.0).abs() < 1e-6);
    assert!((c.birth_rate - 25.0).abs() < 1e-6);
    assert!((c.lifetime - 2.0).abs() < 1e-6);
    assert_eq!(c.emitter, 0xDEADBEEF);
    assert_eq!(c.num_particles, 0);
    assert_eq!(s.position(), bytes.len() as u64);

    // NiBSPArrayController aliases to the same parser with the
    // identical payload — verify it dispatches.
    let mut s = NifStream::new(&bytes, &header);
    let b = parse_block("NiBSPArrayController", &mut s, Some(bytes.len() as u32))
        .expect("NiBSPArrayController dispatch");
    assert!(b
        .as_any()
        .downcast_ref::<NiParticleSystemController>()
        .is_some());
}

// ── #124 / audit NIF-513 — bhkNPCollisionObject family ──────────

