//! Tests for `particle` blocks, extracted from ../particle.rs (#2053 / TD1-004).
//!
//! Same qualified path preserved (`tests::FOO`). Mirrors the
//! shader.rs/shader_tests.rs split.

use super::*;
use crate::header::NifHeader;
use crate::version::NifVersion;

/// FO4-style header (version 20.2.0.7, BSVER 130). The `strings`
/// table has one entry so a name index of 0 resolves; -1 = None.
fn make_header_fo4() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 130,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("Sparks")],
        max_string_length: 8,
        num_groups: 0,
    }
}

/// Hand-build the byte sequence for a minimal FO4 NiParticleSystem
/// with `num_modifiers` modifier refs. Layout follows nif.xml's
/// BS_GTE_SSE branch — see [`parse_particle_system`].
fn build_fo4_particle_system_bytes(num_modifiers: u32, translation: [f32; 3]) -> Vec<u8> {
    let mut d = Vec::new();

    // ── NiObjectNETData ─────────────────────────────────────────
    d.extend_from_slice(&(-1i32).to_le_bytes()); // name = None (string index -1)
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count = 0
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref = NULL

    // ── NiAVObject extension (bsver > crate::version::bsver::FLAGS_U32_THRESHOLD ⇒ flags=u32) ──────────
    d.extend_from_slice(&14u32.to_le_bytes()); // flags
    for v in translation {
        d.extend_from_slice(&v.to_le_bytes());
    }
    for row in 0..3 {
        for col in 0..3 {
            let v: f32 = if row == col { 1.0 } else { 0.0 };
            d.extend_from_slice(&v.to_le_bytes());
        }
    }
    d.extend_from_slice(&1.0f32.to_le_bytes()); // scale
                                                // No properties list (bsver > crate::version::bsver::FO3_FNV).
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref

    // ── BS_GTE_SSE NiGeometry override ─────────────────────────
    // Bounding sphere: 4 floats.
    for _ in 0..4 {
        d.extend_from_slice(&0.0f32.to_le_bytes());
    }
    d.extend_from_slice(&(-1i32).to_le_bytes()); // skin_ref

    // ── Shader / alpha refs (bsver > crate::version::bsver::FO3_FNV) ───────────────────────
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());

    // ── NiParticleSystem own (BS_GTE_SSE) ──────────────────────
    d.extend_from_slice(&0u64.to_le_bytes()); // vertex_desc
    d.extend_from_slice(&0u16.to_le_bytes()); // far_begin
    d.extend_from_slice(&0u16.to_le_bytes()); // far_end
    d.extend_from_slice(&0u16.to_le_bytes()); // near_begin
    d.extend_from_slice(&0u16.to_le_bytes()); // near_end
    d.extend_from_slice(&(-1i32).to_le_bytes()); // data_ref

    // ── Universal trailer ──────────────────────────────────────
    d.push(1u8); // world_space = true
    d.extend_from_slice(&num_modifiers.to_le_bytes());
    for i in 0..num_modifiers {
        d.extend_from_slice(&(i as i32).to_le_bytes());
    }

    d
}

/// Regression: #407 — pre-fix, parse_particle_system on FO4
/// (BSVER 130) skipped the BS_GTE_SSE bounding-sphere/skin/vertex_desc/
/// far-near/data prefix and read `world_space` + `num_modifiers` from
/// inside that missing payload. With even one byte of stream beyond the
/// real block, `num_modifiers` would soak up an arbitrary u32 and
/// the loop walked thousands of bytes into the next block (the 75×
/// over-read). The fix consumes the prefix correctly so the parser
/// lands exactly on the trailing modifier refs.
#[test]
fn parse_particle_system_fo4_consumes_full_block() {
    let header = make_header_fo4();
    let bytes = build_fo4_particle_system_bytes(2, [0.0; 3]);

    // 72 (NiAVObject) + 20 (BS_GTE_SSE NiGeo) + 8 (shader/alpha)
    // + 20 (vertex_desc + far/near + data ref) + 13 (world_space +
    // num_modifiers + 2 refs) = 133.
    assert_eq!(
        bytes.len(),
        133,
        "fixture size drift — recheck the BS_GTE_SSE field list"
    );

    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_particle_system(&mut stream, "NiParticleSystem")
        .expect("FO4 NiParticleSystem should parse cleanly");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "parser must consume the full block — drift here is the #407 over-read"
    );
    assert_eq!(block.original_type, "NiParticleSystem");
}

/// Regression: #1333 — the block's `NiAVObjectData` local transform
/// must survive parsing. Pre-fix it was read into `_av` and dropped,
/// so an emitter authored with a non-zero offset (campfire smoke
/// above the fire, FO4 steam stacks) spawned at the host node origin.
/// A non-zero translation on the wire must round-trip into
/// `NiParticleSystem.transform` for the walkers to compose.
#[test]
fn parse_particle_system_retains_local_transform() {
    let header = make_header_fo4();
    let bytes = build_fo4_particle_system_bytes(0, [1.5, -2.0, 3.25]);
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_particle_system(&mut stream, "NiParticleSystem")
        .expect("FO4 NiParticleSystem should parse cleanly");
    assert_eq!(
        [
            block.transform.translation.x,
            block.transform.translation.y,
            block.transform.translation.z,
        ],
        [1.5, -2.0, 3.25],
        "local translation from the NiAVObjectData base must be retained (#1333)"
    );
}

/// Regression: a junk `num_modifiers` (here `u32::MAX`) must be
/// rejected by the in-stream `check_alloc` gate before the loop
/// can spin trying to read 16 GB of refs. Pre-#407 this would
/// have consumed the rest of the stream + EOF'd; now it short-
/// circuits with `InvalidData`.
#[test]
fn parse_particle_system_rejects_junk_num_modifiers() {
    let header = make_header_fo4();
    // Build a fixture with 0 trailing refs but a corrupt count.
    let mut bytes = build_fo4_particle_system_bytes(0, [0.0; 3]);
    // Overwrite the num_modifiers field. It sits 4 bytes before
    // the end (just after world_space).
    let nm_offset = bytes.len() - 4;
    bytes[nm_offset..nm_offset + 4].copy_from_slice(&u32::MAX.to_le_bytes());

    let mut stream = NifStream::new(&bytes, &header);
    let err = parse_particle_system(&mut stream, "NiParticleSystem")
        .expect_err("junk num_modifiers must short-circuit");
    let msg = err.to_string();
    assert!(
        msg.contains("exceeds hard cap")
            || msg.contains("only ") && msg.contains("bytes remaining"),
        "expected check_alloc rejection, got: {msg}"
    );
}

/// FNV-style header (version 20.2.0.7, BSVER 34). Used by the #383
/// regression tests for FNV-era particle modifiers / emitters.
fn make_header_fnv() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 11,
        user_version_2: 34,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("Mod")],
        max_string_length: 4,
        num_groups: 0,
    }
}

/// `NiPSysModifierBase` payload (string index + order + target + active).
/// 13 bytes on the v20.2.0.7 path (string is a 4-byte index).
fn modifier_base_bytes() -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&(-1i32).to_le_bytes()); // name = None
    d.extend_from_slice(&0u32.to_le_bytes()); // order
    d.extend_from_slice(&(-1i32).to_le_bytes()); // target ref
    d.push(1u8); // active
    d
}

/// #1345 / D6-01 — `BSPSysSimpleColorModifier` must capture its inline
/// 3-key RGBA ramp (was discarded as an opaque `NiPSysBlock`). Verifies
/// byte-exact consumption on FNV (base + 6 floats + 3 Color4 = no FO76
/// trailer) and that `colors[0]`/`colors[2]` (birth/death) round-trip —
/// these feed `extract_first_color_curve`'s fallback so FNV particle FX
/// drive from the authored ramp instead of the heuristic preset.
#[test]
fn bs_simple_color_modifier_captures_inline_ramp() {
    let header = make_header_fnv();
    let mut bytes = modifier_base_bytes();
    // 6 fade/percent floats (Fade In/Out, Color1/2 End/Start percent).
    for f in [0.1f32, 0.9, 0.0, 0.0, 0.0, 1.0] {
        bytes.extend_from_slice(&f.to_le_bytes());
    }
    // Colors[3] Color4 ramp: birth / mid / death (RGBA).
    let birth = [1.0f32, 0.25, 0.0, 1.0];
    let mid = [0.5f32, 0.1, 0.0, 1.0];
    let death = [0.0f32, 0.0, 0.0, 0.0];
    for c in [birth, mid, death] {
        for v in c {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }

    let mut stream = NifStream::new(&bytes, &header);
    let m = BSPSysSimpleColorModifier::parse(&mut stream)
        .expect("BSPSysSimpleColorModifier should parse");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "FNV BSPSysSimpleColorModifier must consume exactly base + 6 floats + 3 Color4 (no FO76 trailer)"
    );
    assert_eq!(m.colors[0], birth, "Colors[0] = birth colour");
    assert_eq!(m.colors[2], death, "Colors[2] = death colour");
    assert_eq!(m.fade_in_percent, 0.1);
    assert_eq!(m.color_2_start_percent, 1.0);
}

/// Regression: #383 — `skip_emitter_base` was reading 12 floats
/// (48 bytes) where nif.xml requires 14 (56 bytes). Every Box /
/// Cylinder / Sphere / Mesh emitter under-read by 8 bytes; the
/// downstream consequence on `parse_mesh_emitter` was that
/// `num_meshes` got a junk u32 from inside the missing fields and
/// the loop walked thousands of bytes into the next block (5,058-
/// byte over-reads on 97-byte blocks observed pre-fix).
///
/// Verified directly on `parse_sphere_emitter` since it's the
/// shortest of the volume emitters and exercises the entire
/// modifier+emitter+volume+radius chain.
#[test]
fn parse_sphere_emitter_consumes_full_block() {
    let header = make_header_fnv();
    let mut d = modifier_base_bytes();
    // 56 bytes of emitter base (14 floats), zeroed.
    d.extend_from_slice(&[0u8; 56]);
    // 4 bytes for the volume emitter object ref.
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // 4 bytes radius.
    d.extend_from_slice(&1.5f32.to_le_bytes());

    // 13 base + 56 emitter + 4 volume + 4 radius = 77 bytes
    // (matches the FNV nif_stats observed `expected 77`).
    assert_eq!(d.len(), 77);

    let mut stream = NifStream::new(&d, &header);
    let block = parse_sphere_emitter(&mut stream)
        .expect("FNV NiPSysSphereEmitter should parse cleanly");
    assert_eq!(stream.position() as usize, d.len());
    assert_eq!(block.original_type, "NiPSysSphereEmitter");
}

/// NIFAL particles slice — the emitter base now CAPTURES values
/// (not just byte-advances). Build a sphere emitter with distinct
/// floats per field and assert each lands in the right
/// `EmitterBaseParams` slot, in nif.xml order (Radius Variation
/// interleaved before Life Span).
#[test]
fn emitter_base_captures_values_in_nifxml_order() {
    let header = make_header_fnv();
    let mut d = modifier_base_bytes();
    // Emitter base, 14 floats in nif.xml order:
    for v in [
        1.0f32, // speed
        2.0,    // speed_variation
        3.0,    // declination
        4.0,    // declination_variation
        5.0,    // planar_angle
        6.0,    // planar_angle_variation
        0.1, 0.2, 0.3, 0.4,  // initial_color RGBA
        7.0,  // initial_radius
        8.0,  // radius_variation (since 10.4.0.1 — interleaved here)
        9.0,  // life_span
        10.0, // life_span_variation
    ] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&(-1i32).to_le_bytes()); // volume emitter object ref
    d.extend_from_slice(&1.5f32.to_le_bytes()); // sphere radius

    let mut stream = NifStream::new(&d, &header);
    let block = parse_sphere_emitter(&mut stream).expect("parse");
    assert_eq!(stream.position() as usize, d.len(), "full block consumed");
    let p = block.params;
    assert_eq!(p.speed, 1.0);
    assert_eq!(p.speed_variation, 2.0);
    assert_eq!(p.declination, 3.0);
    assert_eq!(p.declination_variation, 4.0);
    assert_eq!(p.planar_angle, 5.0);
    assert_eq!(p.planar_angle_variation, 6.0);
    assert_eq!(p.initial_color, [0.1, 0.2, 0.3, 0.4]);
    assert_eq!(p.initial_radius, 7.0);
    assert_eq!(p.radius_variation, 8.0, "interleaved before life_span");
    assert_eq!(p.life_span, 9.0);
    assert_eq!(p.life_span_variation, 10.0);
}

/// Regression: #1239 — `skip_emitter_base`'s gate on the trailing
/// 2 floats (`Radius Variation` + `Life Span Variation`) was
/// `bsver() >= 34` (BS_GTE_FO3), which excluded Oblivion (bsver=11,
/// version 20.0.0.5). Per nif.xml `Radius Variation since="10.4.0.1"`,
/// Oblivion's version 20.0.0.5 is well past that gate. The
/// pre-#1239 gate caused every NiPSys*Emitter on Oblivion to
/// under-read by 8 bytes, cascading into the next block and
/// truncating 219 NIFs (15 182 dropped blocks) in
/// `Oblivion - Meshes.bsa`. Switching to the nif.xml version gate
/// (`version >= V10_4_0_1`) covers Oblivion AND keeps FNV/Skyrim+
/// reading the same 14 floats they always did.
///
/// `parse_sphere_emitter` exercises the full
/// modifier+emitter+volume+radius chain on Oblivion. The 13-byte
/// modifier base is shared with FNV (it doesn't change between
/// `make_header_fnv` and `make_header_oblivion` because the
/// affected fields aren't version-gated on the modifier base).
/// Modifier-base bytes for Oblivion. The string is length-prefixed
/// inline (Oblivion v20.0.0.4 is below `STRING_TABLE_THRESHOLD`
/// = V20_1_0_1) rather than a 4-byte string-table index, so this
/// can't share `modifier_base_bytes` with the FNV side.
fn modifier_base_bytes_oblivion() -> Vec<u8> {
    let mut d = Vec::new();
    d.extend_from_slice(&0u32.to_le_bytes()); // name length = 0 → None
    d.extend_from_slice(&0u32.to_le_bytes()); // order
    d.extend_from_slice(&(-1i32).to_le_bytes()); // target ref
    d.push(1u8); // active
    d
}

#[test]
fn parse_sphere_emitter_consumes_full_block_oblivion() {
    let header = make_header_oblivion();
    let mut d = modifier_base_bytes_oblivion();
    // 56 bytes of emitter base (14 floats, including the +2 from
    // the post-#1239 gate — pre-fix Oblivion would read only 48
    // and over-read the next block by 8 bytes).
    d.extend_from_slice(&[0u8; 56]);
    d.extend_from_slice(&(-1i32).to_le_bytes()); // volume emitter object ref
    d.extend_from_slice(&1.5f32.to_le_bytes()); // radius

    // 13 base + 56 emitter + 4 volume + 4 radius = 77 bytes — wire
    // layout is identical across the Oblivion and FNV eras post-#1239.
    assert_eq!(d.len(), 77);

    let mut stream = NifStream::new(&d, &header);
    let block = parse_sphere_emitter(&mut stream)
        .expect("Oblivion NiPSysSphereEmitter should parse cleanly post-#1239");
    assert_eq!(stream.position() as usize, d.len());
    assert_eq!(block.original_type, "NiPSysSphereEmitter");
}

/// Regression: #1444 / LC-D9-01 — `NiPSysPartSpawnModifier` is a
/// WorldShift-only (v10.2–10.4) modifier with three trailing fields
/// (Particles Per Second, Time, Spawner ref) past the shared base. It
/// must consume base + 12 bytes; a base-only parse would under-read by
/// 12 B and cascade-truncate the rest of the sizeless v10.x NIF (the
/// #1332 ceiling). The inline-string base layout is identical at v10.x
/// (below the string-table threshold), so `modifier_base_bytes_oblivion`
/// is reused.
#[test]
fn parse_part_spawn_modifier_consumes_base_plus_three_fields() {
    let header = make_header_oblivion();
    let mut d = modifier_base_bytes_oblivion();
    d.extend_from_slice(&40.0f32.to_le_bytes()); // particles_per_second
    d.extend_from_slice(&0.5f32.to_le_bytes()); // time
    d.extend_from_slice(&(-1i32).to_le_bytes()); // spawner ref

    // 13 base + 4 + 4 + 4 = 25 bytes.
    assert_eq!(d.len(), 25);

    let mut stream = NifStream::new(&d, &header);
    let block = parse_part_spawn_modifier(&mut stream)
        .expect("NiPSysPartSpawnModifier should parse base + 3 fields");
    assert_eq!(
        stream.position() as usize,
        d.len(),
        "must consume base + 12 B (a base-only parse would stop 12 B short)"
    );
    assert_eq!(block.original_type, "NiPSysPartSpawnModifier");
}

/// Regression: #1507 — at v10.x (< 10.4.0.1) the emitter base must
/// read `Life Span Variation` (nif.xml gives it NO `since`) but NOT
/// `Radius Variation` (`since="10.4.0.1"`). Pre-fix both were bundled
/// under the `>= 10.4.0.1` gate, dropping 4 bytes and under-reading
/// every v10.x Oblivion NiPSys*Emitter — cascading truncation through
/// the sizeless format (confirmed on `effects/metalsparks.nif`).
#[test]
fn read_emitter_base_reads_life_span_variation_below_10_4_0_1() {
    let version = NifVersion::V10_2_0_0;
    let header = NifHeader {
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
    };
    let mut d = Vec::new();
    // speed, speed_var, decl, decl_var, planar, planar_var (6 floats)
    for v in [1.0f32, 2.0, 3.0, 4.0, 5.0, 6.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    // initial_color (Color4)
    for _ in 0..4 {
        d.extend_from_slice(&1.0f32.to_le_bytes());
    }
    d.extend_from_slice(&1.15f32.to_le_bytes()); // initial_radius
    // radius_variation ABSENT at v10.2 (since 10.4.0.1)
    d.extend_from_slice(&1.5f32.to_le_bytes()); // life_span
    d.extend_from_slice(&0.25f32.to_le_bytes()); // life_span_variation (always present)

    // 6*4 + 16 + 4 + 4 + 4 = 52 B (no radius_variation slot).
    assert_eq!(d.len(), 52);

    let mut stream = NifStream::new(&d, &header);
    let params = read_emitter_base(&mut stream).expect("v10.2 emitter base parses");
    assert_eq!(
        stream.position() as usize,
        d.len(),
        "v10.2 emitter base must consume 52 B — life_span_variation present, radius_variation absent"
    );
    assert_eq!(params.radius_variation, 0.0, "radius_variation absent < 10.4.0.1");
    assert_eq!(params.life_span, 1.5);
    assert_eq!(
        params.life_span_variation, 0.25,
        "life_span_variation must be read at v10.2 (#1507)"
    );
}

/// Regression: #383 — `parse_grow_fade_modifier` on FNV
/// (BS_GTE_FO3 + version 20.2.0.7) was missing the trailing
/// `Base Scale: f32` per nif.xml line 4803. 890 occurrences in
/// vanilla `Fallout - Meshes.bsa` under-read by 4 bytes.
#[test]
fn parse_grow_fade_modifier_reads_base_scale_on_bs_gte_fo3() {
    let header = make_header_fnv();
    let mut d = modifier_base_bytes();
    d.extend_from_slice(&1.0f32.to_le_bytes()); // grow_time
    d.extend_from_slice(&0u16.to_le_bytes()); // grow_generation
    d.extend_from_slice(&2.0f32.to_le_bytes()); // fade_time
    d.extend_from_slice(&0u16.to_le_bytes()); // fade_generation
    d.extend_from_slice(&3.0f32.to_le_bytes()); // base_scale (BS_GTE_FO3)

    // 13 base + 12 (grow_time + grow_gen + fade_time + fade_gen) +
    // 4 (base_scale) = 29 bytes (matches FNV nif_stats observed
    // `expected 29`).
    assert_eq!(d.len(), 29);

    let mut stream = NifStream::new(&d, &header);
    let block = parse_grow_fade_modifier(&mut stream)
        .expect("FNV NiPSysGrowFadeModifier should parse cleanly");
    assert_eq!(stream.position() as usize, d.len());
    // base_scale is now captured (FO3+ gate); value 3.0 from above.
    assert_eq!(block.base_scale, Some(3.0));
}

/// Oblivion-style header (V20_0_0_4, BSVER 11). The `strings` table
/// is empty — NiPSysData carries no name-indexed fields on this path.
fn make_header_oblivion() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_0_0_4,
        little_endian: true,
        user_version: 0,
        user_version_2: 11,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: Vec::new(),
        max_string_length: 0,
        num_groups: 0,
    }
}

/// Build the NiGeometryData base header bytes that
/// `parse_geometry_data_base` consumes for the supplied (version,
/// num_vertices, has_vertices, has_additional_data) shape, with
/// data_flags/normals/colors/UVs all empty. Mirrors the reader at
/// `crates/nif/src/blocks/tri_shape.rs:787` exactly so test fixtures
/// stay in lockstep with the schema gates.
fn nigeo_base_bytes(num_vertices: u16, version: NifVersion) -> Vec<u8> {
    let mut d = Vec::new();
    // group_id since 10.1.0.114
    if version >= NifVersion::V10_1_0_114 {
        d.extend_from_slice(&0i32.to_le_bytes());
    }
    d.extend_from_slice(&num_vertices.to_le_bytes());
    // keep/compress flags since 10.1.0.0
    if version >= NifVersion::V10_1_0_0 {
        d.push(0u8);
        d.push(0u8);
    }
    d.push((num_vertices > 0) as u8); // has_vertices
    for _ in 0..num_vertices {
        d.extend_from_slice(&0.0f32.to_le_bytes());
        d.extend_from_slice(&0.0f32.to_le_bytes());
        d.extend_from_slice(&0.0f32.to_le_bytes());
    }
    // data_flags since 10.0.1.0
    if version >= NifVersion::V10_0_1_0 {
        d.extend_from_slice(&0u16.to_le_bytes());
    }
    d.push(0u8); // has_normals = false
                 // bounding sphere (12 + 4)
    for _ in 0..4 {
        d.extend_from_slice(&0.0f32.to_le_bytes());
    }
    d.push(0u8); // has_vertex_colors = false
                 // (no UV sets — data_flags = 0)
                 // consistency_flags since 10.0.1.0
    if version >= NifVersion::V10_0_1_0 {
        d.extend_from_slice(&0u16.to_le_bytes());
    }
    // additional_data_ref since 20.0.0.4
    if version >= NifVersion::V20_0_0_4 {
        d.extend_from_slice(&(-1i32).to_le_bytes());
    }
    d
}

/// Regression: #581 — `parse_particles_data` skipped the `Particle Info`
/// array entirely on pre-BS202 streams. Oblivion 20.0.0.4 has bsver=11
/// but version < 20.2.0.7 so `is_bs_202 = false`, meaning every
/// NiPSysData blob's particle metadata (28 bytes per particle on
/// post-10.4.0.1 streams) used to vanish from the cursor and cascade
/// drift into every following block. Test asserts the parser now
/// consumes exactly the byte range the Particle Info array occupies.
#[test]
fn parse_particles_data_skips_particle_info_on_oblivion() {
    let header = make_header_oblivion();
    let mut d = nigeo_base_bytes(2, header.version);
    // NiParticlesData tail (Bethesda-particle subset on !BS202 / Oblivion).
    d.push(0u8); // has_radii (since 10.1.0.0)
    d.extend_from_slice(&0u16.to_le_bytes()); // num_active_particles
    d.push(0u8); // has_sizes
    d.push(0u8); // has_rotations (since 10.0.1.0)
    d.push(0u8); // has_rotation_angles (since 20.0.0.4)
    d.push(0u8); // has_rotation_axes
                 // BS202 trailers — Oblivion is !is_bs_202 → not present.

    // NiPSysData own: Particle Info × num_vertices. Oblivion is
    // post-10.4.0.1 → 28 B per particle × 2 = 56 B. Pre-#581 these
    // 56 bytes were not consumed and cascaded into block drift.
    d.extend_from_slice(&[0u8; 2 * 28]);

    d.push(0u8); // has_rotation_speeds (since 20.0.0.2)
    d.extend_from_slice(&0u16.to_le_bytes()); // num_added (!BS202)
    d.extend_from_slice(&0u16.to_le_bytes()); // added_particles_base

    let mut stream = NifStream::new(&d, &header);
    let block = parse_particles_data(&mut stream, "NiPSysData")
        .expect("Oblivion NiPSysData with 2 particles should parse cleanly");
    assert_eq!(
        stream.position() as usize,
        d.len(),
        "stream must land exactly at end-of-block — Particle Info skip is what closes the gap"
    );
    assert_eq!(block.original_type, "NiPSysData");
}

/// Sibling guard: pre-10.4.0.1 streams (Morrowind 4.x, early Gamebryo)
/// carry the 40-byte NiParticleInfo layout (Rotation Axis is
/// `until="10.4.0.1"` per nif.xml line 2267). Test parses NiPSysData
/// at the boundary version 10.4.0.1 with `num_vertices = 1` to
/// confirm the version branch picks 40 B.
///
/// NOTE: NiPSysData isn't widespread in real pre-10.4 content; the
/// test is a pure version-branch guard that the 40-byte path is
/// reachable and exact.
#[test]
fn parse_particles_data_uses_40_byte_particle_info_on_pre_10_4_0_1() {
    // v10.4.0.0 sits inside the v10.4.0.1 `until=` boundary (inclusive
    // per the version.rs doctrine). The Rotation Axis is present at
    // v <= 10.4.0.1; the layout shrinks to 28 B starting at v10.4.0.2.
    // This test exercises the 40-byte legacy layout.
    let version = NifVersion::V10_4_0_0;
    let header = NifHeader {
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
    };
    let mut d = nigeo_base_bytes(1, version);
    // NiParticlesData tail at 10.4.0.1: has_radii since 10.1.0.0 ✓,
    // has_rotations since 10.0.1.0 ✓; has_rotation_angles/axes
    // since 20.0.0.4 → NOT present.
    d.push(0u8); // has_radii
    d.extend_from_slice(&0u16.to_le_bytes()); // num_active_particles
    d.push(0u8); // has_sizes
    d.push(0u8); // has_rotations

    // Particle Info: 1 × 40 = 40 B (pre-10.4.0.1 layout — Rotation
    // Axis present).
    d.extend_from_slice(&[0u8; 40]);

    // has_rotation_speeds gated on >= 20.0.0.2; 10.4.0.0 < that → not
    // read. Num Added Particles + Added Particles Base carry NO `since`
    // (only `!#BS202#`), so they ARE present here (#1507).
    d.extend_from_slice(&0u16.to_le_bytes()); // num_added
    d.extend_from_slice(&0u16.to_le_bytes()); // added_particles_base

    let mut stream = NifStream::new(&d, &header);
    let block = parse_particles_data(&mut stream, "NiPSysData")
        .expect("10.4.0.1 NiPSysData should parse with 40-byte Particle Info");
    assert_eq!(
        stream.position() as usize,
        d.len(),
        "10.4.0.1 layout must consume the full 40 B per particle (Rotation Axis present)"
    );
    assert_eq!(block.original_type, "NiPSysData");
}

/// Sibling guard: BS202 streams (FO3+ at 20.2.0.7+) MUST NOT skip
/// the Particle Info bytes — the array is `vercond="!#BS202#"`, so
/// the field is absent on Bethesda streams entirely. Pre-fix
/// behavior on this path was already correct; the test guards
/// against a future refactor accidentally dropping the `!is_bs_202`
/// gate.
#[test]
fn parse_particles_data_does_not_skip_particle_info_on_bs202() {
    let header = make_header_fnv(); // 20.2.0.7, bsver=34 → is_bs_202 = true
                                    // BS202+non-NiParticlesData uses parse_psys_geometry_data_base
                                    // → array_count = 0 regardless of has_vertices, but the bool
                                    // headers are still serialized.
    let mut d = nigeo_base_bytes(0, header.version);
    d.push(0u8); // has_radii
    d.extend_from_slice(&0u16.to_le_bytes()); // num_active_particles
    d.push(0u8); // has_sizes
    d.push(0u8); // has_rotations
    d.push(0u8); // has_rotation_angles
    d.push(0u8); // has_rotation_axes
                 // BS202 trailers (FNV bsver=34 → byte-sized num_subtex, no
                 // bsver>34 aspect block).
    d.push(0u8); // has_texture_indices
    d.push(0u8); // num_subtex_offsets (byte)
                 // NO Particle Info — gated on !is_bs_202 (false here).
    d.push(0u8); // has_rotation_speeds (since 20.0.0.2)
                 // NO num_added / added_particles_base — !is_bs_202 (false here).

    let mut stream = NifStream::new(&d, &header);
    let block = parse_particles_data(&mut stream, "NiPSysData")
        .expect("FNV NiPSysData should parse cleanly with no Particle Info skip");
    assert_eq!(
        stream.position() as usize,
        d.len(),
        "BS202 path must NOT skip Particle Info bytes (the field is absent)"
    );
    assert_eq!(block.original_type, "NiPSysData");
}

/// Regression: #383 — `parse_rotation_modifier` was missing the
/// `Random Rot Speed Sign: bool` field (since 20.0.0.2 per nif.xml
/// line 4878). 1,149 occurrences in vanilla `Fallout - Meshes.bsa`
/// under-read by 1 byte.
#[test]
fn parse_rotation_modifier_reads_random_rot_speed_sign_post_20_0_0_2() {
    let header = make_header_fnv();
    let mut d = modifier_base_bytes();
    d.extend_from_slice(&1.0f32.to_le_bytes()); // initial_speed
    d.extend_from_slice(&0.5f32.to_le_bytes()); // speed_variation
    d.extend_from_slice(&0.0f32.to_le_bytes()); // initial_angle
    d.extend_from_slice(&0.1f32.to_le_bytes()); // angle_variation
    d.push(0u8); // random_rot_speed_sign (since 20.0.0.2)
    d.push(1u8); // random_axis
    d.extend_from_slice(&[0u8; 12]); // axis vec3

    // 13 base + 16 (initial_speed + 3 vars) + 1 (rot_sign) +
    // 1 (random_axis) + 12 (vec3) = 43 bytes (matches FNV
    // nif_stats observed `expected 43`).
    assert_eq!(d.len(), 43);

    let mut stream = NifStream::new(&d, &header);
    let block = parse_rotation_modifier(&mut stream)
        .expect("FNV NiPSysRotationModifier should parse cleanly");
    assert_eq!(stream.position() as usize, d.len());
    assert_eq!(block.original_type, "NiPSysRotationModifier");
}

/// Regression: #1306 / OBL-D6-NEW-03 — `Random Rot Speed Sign` is
/// `since="20.0.0.2"` per nif.xml with NO bsver/game condition, so
/// Oblivion (v20.0.0.4, bsver 11) emits it too. The #383 fix wrongly
/// gated it on `bsver >= 34`, so Oblivion under-read by 1 byte and FX
/// emitters (fire/torch/smoke) dropped from the render. Block-tracing
/// `meshes\fire\firetorchsmall.nif` localized the drift to this exact
/// byte. Under the old gate this test consumes only 42 bytes (1 short);
/// with the version gate it consumes the full 43.
#[test]
fn parse_rotation_modifier_reads_random_rot_speed_sign_oblivion() {
    let header = make_header_oblivion();
    let mut d = modifier_base_bytes_oblivion();
    d.extend_from_slice(&1.0f32.to_le_bytes()); // initial_speed
    d.extend_from_slice(&0.5f32.to_le_bytes()); // speed_variation
    d.extend_from_slice(&0.0f32.to_le_bytes()); // initial_angle
    d.extend_from_slice(&0.1f32.to_le_bytes()); // angle_variation
    d.push(1u8); // random_rot_speed_sign — present on Oblivion (#1306)
    d.push(1u8); // random_axis
    d.extend_from_slice(&[0u8; 12]); // axis vec3 (e.g. unit X)

    // 13 base + 16 (speed + 3 vars) + 1 (rot_sign) + 1 (random_axis)
    // + 12 (vec3) = 43 bytes — identical to the FNV layout.
    assert_eq!(d.len(), 43);

    let mut stream = NifStream::new(&d, &header);
    let block = parse_rotation_modifier(&mut stream)
        .expect("Oblivion NiPSysRotationModifier should parse cleanly");
    assert_eq!(
        stream.position() as usize,
        d.len(),
        "Oblivion must read random_rot_speed_sign (since 20.0.0.2); the old \
         bsver>=34 gate skipped it and under-read by 1 byte"
    );
    assert_eq!(block.original_type, "NiPSysRotationModifier");
}

/// FO76 header (`#BS_F76#`): v20.2.0.7 stream with `user_version_2 == 155`.
fn make_header_fo76() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 155,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("Mod")],
        max_string_length: 4,
        num_groups: 0,
    }
}

/// Regression: #1896 — on FO76 (`#BS_F76#`, bsver 155) nif.xml interleaves
/// `Unknown Vector` (Vector4, 16 B) + `Unknown Byte` (1 B) *between*
/// Rotation Speed Variation and Rotation Angle. Pre-fix the single
/// `skip(12)` left the stream 17 bytes short (recovered by block_size,
/// inert but misaligning). The FNV layout (bsver 34) must stay 17 bytes
/// shorter — the interleaved fields are absent there.
#[test]
fn parse_rotation_modifier_reads_fo76_interleaved_fields() {
    let header = make_header_fo76();
    let mut d = modifier_base_bytes();
    d.extend_from_slice(&1.0f32.to_le_bytes()); // initial_speed
    d.extend_from_slice(&0.5f32.to_le_bytes()); // speed_variation
    d.extend_from_slice(&[0u8; 16]); // FO76 Unknown Vector (Vector4)
    d.push(0u8); // FO76 Unknown Byte
    d.extend_from_slice(&0.0f32.to_le_bytes()); // rotation_angle
    d.extend_from_slice(&0.1f32.to_le_bytes()); // rotation_angle_variation
    d.push(0u8); // random_rot_speed_sign
    d.push(1u8); // random_axis
    d.extend_from_slice(&[0u8; 12]); // axis vec3

    // 13 base + 4 speed + 4 var + 17 FO76 + 8 (angle+var) + 1 + 1 + 12 = 60.
    assert_eq!(d.len(), 60);

    let mut stream = NifStream::new(&d, &header);
    parse_rotation_modifier(&mut stream)
        .expect("FO76 NiPSysRotationModifier should parse cleanly");
    assert_eq!(
        stream.position() as usize,
        d.len(),
        "FO76 must consume the interleaved #BS_F76# Unknown Vector + Byte (17 B)"
    );
}

/// Regression: #1896 — `BSPSysSimpleColorModifier` consumes its trailing
/// FO76-only `Unknown Shorts[26]` (52 B, `#BS_F76#`) so `consumed ==
/// block_size`. `colors` is still captured; the tail is opaque/discarded.
#[test]
fn bs_simple_color_modifier_consumes_fo76_trailer() {
    let header = make_header_fo76();
    let mut bytes = modifier_base_bytes();
    for f in [0.1f32, 0.9, 0.0, 0.0, 0.0, 1.0] {
        bytes.extend_from_slice(&f.to_le_bytes()); // 6 fade/percent floats
    }
    for _ in 0..3 {
        bytes.extend_from_slice(&[0u8; 16]); // 3 × Color4
    }
    bytes.extend_from_slice(&[0u8; 52]); // FO76 Unknown Shorts[26]

    // 13 base + 24 (6 floats) + 48 (3 Color4) + 52 trailer = 137.
    assert_eq!(bytes.len(), 137);

    let mut stream = NifStream::new(&bytes, &header);
    let modifier = BSPSysSimpleColorModifier::parse(&mut stream)
        .expect("FO76 BSPSysSimpleColorModifier should parse cleanly");
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "FO76 must consume the trailing #BS_F76# Unknown Shorts[26] (52 B)"
    );
    // The colour ramp is still captured (all zero here, but present).
    assert_eq!(modifier.colors.len(), 3);
}

/// Regression: NiFlipController follow-up to #1306 — `World Aligned` is
/// `vercond="!#NI_BS_LTE_16#"` (bsver > 16), NOT a NIF-version gate. The
/// old `version >= V20_0_0_4` gate read the byte on Oblivion (bsver 11)
/// where it is absent → a 1-byte over-read that drifted into the next
/// block (`meshes\fire\firetorchsmall.nif`). Oblivion must NOT read it.
#[test]
fn parse_gravity_modifier_skips_world_aligned_on_oblivion() {
    let header = make_header_oblivion();
    let mut d = modifier_base_bytes_oblivion(); // 13 bytes
    d.extend_from_slice(&(-1i32).to_le_bytes()); // gravity_object ref
    d.extend_from_slice(&[0u8; 12]); // gravity_axis vec3
    d.extend_from_slice(&0.0f32.to_le_bytes()); // decay
    d.extend_from_slice(&1.0f32.to_le_bytes()); // strength
    d.extend_from_slice(&0u32.to_le_bytes()); // force_type
    d.extend_from_slice(&0.0f32.to_le_bytes()); // turbulence
    d.extend_from_slice(&1.0f32.to_le_bytes()); // turbulence_scale
                                                // NO world_aligned byte on Oblivion (bsver 11 <= 16).
    assert_eq!(d.len(), 13 + 4 + 12 + 4 + 4 + 4 + 4 + 4); // 49

    let mut stream = NifStream::new(&d, &header);
    parse_gravity_modifier(&mut stream).expect("Oblivion gravity modifier parses");
    assert_eq!(
        stream.position() as usize,
        d.len(),
        "Oblivion (bsver 11) must NOT read World Aligned; the old version gate \
         over-read by 1 byte"
    );
}

/// FNV (bsver 34 > 16) DOES carry `World Aligned` — confirms the bsver
/// gate is correct on both sides of the #NI_BS_LTE_16 boundary.
#[test]
fn parse_gravity_modifier_reads_world_aligned_on_fnv() {
    let header = make_header_fnv();
    let mut d = modifier_base_bytes(); // 13 bytes (FNV base)
    d.extend_from_slice(&(-1i32).to_le_bytes()); // gravity_object
    d.extend_from_slice(&[0u8; 12]); // gravity_axis
    d.extend_from_slice(&0.0f32.to_le_bytes()); // decay
    d.extend_from_slice(&1.0f32.to_le_bytes()); // strength
    d.extend_from_slice(&0u32.to_le_bytes()); // force_type
    d.extend_from_slice(&0.0f32.to_le_bytes()); // turbulence
    d.extend_from_slice(&1.0f32.to_le_bytes()); // turbulence_scale
    d.push(1u8); // world_aligned — present on FNV
    assert_eq!(d.len(), 13 + 4 + 12 + 4 + 4 + 4 + 4 + 4 + 1); // 50

    let mut stream = NifStream::new(&d, &header);
    parse_gravity_modifier(&mut stream).expect("FNV gravity modifier parses");
    assert_eq!(
        stream.position() as usize,
        d.len(),
        "FNV must read World Aligned"
    );
}
