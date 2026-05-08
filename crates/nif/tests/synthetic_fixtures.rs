//! Synthetic NIF binary fixtures for CI testing.
//!
//! These tests construct minimal valid NIF binary data in memory and parse
//! them through the full pipeline. No external game data required — runs
//! in CI without any setup.
//!
//! Each fixture exercises the critical path: header parsing → block type
//! table → block parsing → scene construction. One fixture per game era.

/// Helper: write a u8.
fn w8(buf: &mut Vec<u8>, v: u8) {
    buf.push(v);
}

/// Helper: write a u16 LE.
fn w16(buf: &mut Vec<u8>, v: u16) {
    buf.extend_from_slice(&v.to_le_bytes());
}

/// Helper: write a u32 LE.
fn w32(buf: &mut Vec<u8>, v: u32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

/// Helper: write a f32 LE.
fn wf32(buf: &mut Vec<u8>, v: f32) {
    buf.extend_from_slice(&v.to_le_bytes());
}

/// Helper: write a "sized string" (u32 length prefix + chars, no null terminator).
fn wsstr(buf: &mut Vec<u8>, s: &str) {
    w32(buf, s.len() as u32);
    buf.extend_from_slice(s.as_bytes());
}

/// Helper: write a "short string" (u8 length prefix + chars + null terminator).
/// Used by BSStreamHeader's ExportString fields.
fn wshort(buf: &mut Vec<u8>, s: &str) {
    let len = s.len() + 1; // includes null terminator
    w8(buf, len as u8);
    buf.extend_from_slice(s.as_bytes());
    w8(buf, 0); // null terminator
}

/// Build a minimal Skyrim SE NIF (v20.2.0.7, uv=12, bsver=100).
///
/// Contains: 1 NiNode (root) with no children.
/// Exercises: header, BSStreamHeader, block type table, block sizes,
/// string table, NiNode parsing.
fn build_skyrim_se_nif() -> Vec<u8> {
    let mut nif = Vec::new();

    // ── Header line ──
    nif.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");

    // ── Binary header ──
    w32(&mut nif, 0x14020007); // version = 20.2.0.7
    w8(&mut nif, 1); // little-endian
    w32(&mut nif, 12); // user_version = 12

    let num_blocks: u32 = 1;
    w32(&mut nif, num_blocks);

    // BSStreamHeader: user_version_2 (bsver)
    w32(&mut nif, 100); // bsver = 100 (Skyrim SE)
    wshort(&mut nif, "ByroRedux Test"); // author
                                        // bsver < 131 → process_script
    wshort(&mut nif, ""); // process_script
    wshort(&mut nif, ""); // export_script
                          // bsver >= 103 → max_filepath: 100 < 103, so DO NOT write it

    // Block type table: 1 type ("NiNode")
    w16(&mut nif, 1); // num_block_types
    wsstr(&mut nif, "NiNode"); // type 0

    // Block type indices: block 0 → type 0
    for _ in 0..num_blocks {
        w16(&mut nif, 0);
    }

    // Block sizes (v20.2.0.7+): will be patched after building block data.
    let block_sizes_offset = nif.len();
    for _ in 0..num_blocks {
        w32(&mut nif, 0); // placeholder
    }

    // String table: 1 string ("Scene Root")
    w32(&mut nif, 1); // num_strings
    w32(&mut nif, 10); // max_string_length
    wsstr(&mut nif, "Scene Root");

    // Groups: 0
    w32(&mut nif, 0);

    // ── Block data: NiNode ──
    let block_start = nif.len();

    // NiAVObjectData::parse for Skyrim (bsver >= 34):
    //   name: string index (u32)
    //   num_extra_data: u32
    //   controller_ref: i32 (-1 = none)
    //   flags: u32 (bsver > 26)
    //   translation: 3×f32
    //   rotation: 3×3×f32 (9 floats)
    //   scale: f32
    //   properties (bsver <= 34 only — Skyrim bsver=100 does NOT have this)
    //   collision_ref: i32

    w32(&mut nif, 0); // name = string index 0 ("Scene Root")
    w32(&mut nif, 0); // num_extra_data = 0
    w32(&mut nif, 0xFFFFFFFF); // controller_ref = -1 (none)
    w32(&mut nif, 0x0E); // flags (typical NiNode flags)
                         // Translation
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    // Rotation (identity 3×3)
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 1.0);
    // Scale
    wf32(&mut nif, 1.0);
    // collision_ref
    w32(&mut nif, 0xFFFFFFFF); // -1 (none)

    // NiNode-specific: children list
    w32(&mut nif, 0); // num_children = 0

    // NiNode: effects list (bsver < 130 only — 100 < 130 so yes)
    w32(&mut nif, 0); // num_effects = 0

    let block_size = (nif.len() - block_start) as u32;

    // Patch block size.
    nif[block_sizes_offset..block_sizes_offset + 4].copy_from_slice(&block_size.to_le_bytes());

    nif
}

/// Build a minimal FO3/FNV NIF (v20.2.0.7, uv=11, bsver=34).
fn build_fo3_nif() -> Vec<u8> {
    let mut nif = Vec::new();

    nif.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
    w32(&mut nif, 0x14020007);
    w8(&mut nif, 1);
    w32(&mut nif, 11); // user_version = 11 (FO3/FNV)

    let num_blocks: u32 = 1;
    w32(&mut nif, num_blocks);

    // BSStreamHeader
    w32(&mut nif, 34); // bsver = 34 (FNV)
    wshort(&mut nif, "ByroRedux Test");
    wshort(&mut nif, ""); // process_script (bsver < 131)
    wshort(&mut nif, ""); // export_script

    // Block types
    w16(&mut nif, 1);
    wsstr(&mut nif, "NiNode");

    // Block type indices
    w16(&mut nif, 0);

    // Block sizes
    let block_sizes_offset = nif.len();
    w32(&mut nif, 0);

    // String table
    w32(&mut nif, 1);
    w32(&mut nif, 4);
    wsstr(&mut nif, "Root");

    // Groups
    w32(&mut nif, 0);

    // NiNode block data (bsver=34: has properties list)
    let block_start = nif.len();
    w32(&mut nif, 0); // name
    w32(&mut nif, 0); // num_extra_data
    w32(&mut nif, 0xFFFFFFFF); // controller_ref
    w32(&mut nif, 0x0E); // flags (bsver > 26)
                         // Translation
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    // Rotation (identity)
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 1.0); // scale
                         // Properties list (bsver <= 34 → present)
    w32(&mut nif, 0); // num_properties = 0
                      // Collision ref
    w32(&mut nif, 0xFFFFFFFF);
    // Children
    w32(&mut nif, 0);
    // Effects (bsver < 130)
    w32(&mut nif, 0);

    let block_size = (nif.len() - block_start) as u32;
    nif[block_sizes_offset..block_sizes_offset + 4].copy_from_slice(&block_size.to_le_bytes());

    nif
}

/// Build a minimal Oblivion NIF (v20.0.0.5, no BSStreamHeader, no block sizes).
fn build_oblivion_nif() -> Vec<u8> {
    let mut nif = Vec::new();

    nif.extend_from_slice(b"Gamebryo File Format, Version 20.0.0.5\n");
    w32(&mut nif, 0x14000005); // v20.0.0.5
    w8(&mut nif, 1); // little-endian
    w32(&mut nif, 0); // user_version = 0 (Oblivion)

    let num_blocks: u32 = 1;
    w32(&mut nif, num_blocks);

    // No BSStreamHeader (user_version < 3 and not v10.0.1.2)

    // Block types
    w16(&mut nif, 1);
    wsstr(&mut nif, "NiNode");

    // Block type indices
    w16(&mut nif, 0);

    // No block sizes (v < 20.2.0.7)
    // No string table (v < 20.1.0.1)

    // Groups
    w32(&mut nif, 0);

    // NiNode block data (Oblivion: uv < 11, bsver = 0)
    // NiAVObjectData for Oblivion:
    //   name: SizedString (u32 len + chars)
    //   num_extra_data: u32
    //   extra_data_refs[]: i32 each
    //   controller_ref: i32
    //   flags: u16 (bsver <= 26 → u16 not u32)
    //   translation: 3×f32
    //   rotation: 3×3×f32
    //   scale: f32
    //   properties: num_properties (u32) + refs (i32 each)
    //   collision_ref: i32  (only for bsver >= 34 — Oblivion bsver=0 has collision at end... actually let me check)

    // For Oblivion (bsver=0): flags is u16, no BSStreamHeader, strings inline
    wsstr(&mut nif, "Scene Root"); // name (inline sized string, not string table index)
    w32(&mut nif, 0); // num_extra_data
    w32(&mut nif, 0xFFFFFFFF); // controller_ref
    w16(&mut nif, 0x000E); // flags (u16 for bsver <= 26)
                           // Translation
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    // Rotation (identity)
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 1.0); // scale
                         // Properties list
    w32(&mut nif, 0); // num_properties
                      // has_collision for Oblivion (bsver 0): always present
    w32(&mut nif, 0xFFFFFFFF); // collision_ref
                               // Children
    w32(&mut nif, 0);
    // Effects (bsver < 130)
    w32(&mut nif, 0);

    nif
}

// ── Tests ───────────────────────────────────────────────────────────

#[test]
fn parse_synthetic_skyrim_se() {
    let data = build_skyrim_se_nif();
    let scene = byroredux_nif::parse_nif(&data).expect("Skyrim SE NIF should parse");
    assert_eq!(scene.blocks.len(), 1, "should have 1 block");
    assert_eq!(scene.blocks[0].block_type_name(), "NiNode");
    assert!(!scene.truncated, "should not be truncated");
}

#[test]
fn parse_synthetic_fo3() {
    let data = build_fo3_nif();
    let scene = byroredux_nif::parse_nif(&data).expect("FO3/FNV NIF should parse");
    assert_eq!(scene.blocks.len(), 1);
    assert_eq!(scene.blocks[0].block_type_name(), "NiNode");
    assert!(!scene.truncated);
}

#[test]
fn parse_synthetic_oblivion() {
    let data = build_oblivion_nif();
    let scene = byroredux_nif::parse_nif(&data).expect("Oblivion NIF should parse");
    assert_eq!(scene.blocks.len(), 1);
    assert_eq!(scene.blocks[0].block_type_name(), "NiNode");
    assert!(!scene.truncated);
}

#[test]
fn synthetic_skyrim_header_version() {
    let data = build_skyrim_se_nif();
    let (header, _) = byroredux_nif::header::NifHeader::parse(&data).unwrap();
    assert_eq!(
        header.version,
        byroredux_nif::version::NifVersion::V20_2_0_7
    );
    assert_eq!(header.user_version, 12);
    assert_eq!(header.user_version_2, 100);
    assert_eq!(header.num_blocks, 1);
    assert_eq!(header.block_types.len(), 1);
    assert_eq!(&*header.block_types[0], "NiNode");
    assert_eq!(header.strings.len(), 1);
    assert_eq!(&*header.strings[0], "Scene Root");
}

#[test]
fn synthetic_fo3_variant_detection() {
    let data = build_fo3_nif();
    let (header, _) = byroredux_nif::header::NifHeader::parse(&data).unwrap();
    let variant = byroredux_nif::version::NifVariant::detect(
        header.version,
        header.user_version,
        header.user_version_2,
    );
    assert_eq!(variant, byroredux_nif::version::NifVariant::FalloutNV);
}

#[test]
fn synthetic_oblivion_variant_detection() {
    let data = build_oblivion_nif();
    let (header, _) = byroredux_nif::header::NifHeader::parse(&data).unwrap();
    let variant = byroredux_nif::version::NifVariant::detect(
        header.version,
        header.user_version,
        header.user_version_2,
    );
    assert_eq!(variant, byroredux_nif::version::NifVariant::Oblivion);
}

// ── Morrowind (v4.0.0.2) ──────────────────────────────────────────────

/// Build a minimal Morrowind NIF (v4.0.0.2, NetImmerse era).
///
/// Contains: 1 NiNode (root) with inline RTTI type name (no block type table).
/// Exercises: pre-Gamebryo header, inline block type names, single extra_data ref,
/// velocity field, bounding volume, NiNode parsing.
fn build_morrowind_nif() -> Vec<u8> {
    let mut nif = Vec::new();

    // Header line (NetImmerse, not Gamebryo)
    nif.extend_from_slice(b"NetImmerse File Format, Version 4.0.0.2\n");

    // Version u32 (v4.0.0.2 = 0x04000002)
    w32(&mut nif, 0x04000002);

    // No endianness byte (v < 20.0.0.4)
    // No user_version (v < 10.0.1.8)

    // num_blocks
    w32(&mut nif, 1);

    // No BSStreamHeader (v < 10.0.1.2 and user_version < 3)
    // No block types table (v < 10.0.1.0)
    // No block sizes (v < 20.2.0.7)
    // No string table (v < 20.1.0.1)
    // No num_groups (v < 10.0.1.0)

    // ── Block 0: NiNode ──
    // Pre-Gamebryo: each block prefixed by its type name as a sized string.
    wsstr(&mut nif, "NiNode");

    // NiObjectNETData (pre-Gamebryo format):
    //   name: SizedString
    //   extra_data_ref: single Ref (i32), not counted list
    //   controller_ref: Ref (i32)
    wsstr(&mut nif, "Scene Root"); // name
    w32(&mut nif, 0xFFFFFFFF); // extra_data_ref (NULL = -1)
    w32(&mut nif, 0xFFFFFFFF); // controller_ref (NULL = -1)

    // NiAVObjectData (pre-Gamebryo format):
    //   flags: u16 (bsver <= 26)
    //   transform: NiTransform (3×3 rotation + 3 translation + 1 scale = 13 floats)
    //   velocity: NiPoint3 (only v <= 4.2.2.0)
    //   properties: num_properties (u32) + refs
    //   has_bounding_volume: bool + optional bounding volume (no collision_ref)
    w16(&mut nif, 0x000E); // flags

    // Translation
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    // Rotation (identity)
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 1.0);
    wf32(&mut nif, 1.0); // scale

    // Velocity (only v <= 4.2.2.0)
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);
    wf32(&mut nif, 0.0);

    // Properties list
    w32(&mut nif, 0); // num_properties

    // Bounding volume (replaces collision_ref in pre-Gamebryo)
    w8(&mut nif, 0); // has_bounding_volume = false

    // NiNode-specific: children + effects
    w32(&mut nif, 0); // num_children
    w32(&mut nif, 0); // num_effects

    nif
}

#[test]
fn parse_synthetic_morrowind() {
    let data = build_morrowind_nif();
    let scene = byroredux_nif::parse_nif(&data).expect("Morrowind NIF should parse");
    assert_eq!(scene.blocks.len(), 1, "should have 1 block");
    assert_eq!(scene.blocks[0].block_type_name(), "NiNode");
    assert!(!scene.truncated, "should not be truncated");
}

#[test]
fn synthetic_morrowind_variant_detection() {
    let data = build_morrowind_nif();
    let (header, _) = byroredux_nif::header::NifHeader::parse(&data).unwrap();
    assert_eq!(header.version, byroredux_nif::version::NifVersion::V4_0_0_2);
    assert_eq!(header.user_version, 0);
    assert_eq!(header.num_blocks, 1);
    assert!(
        header.block_types.is_empty(),
        "pre-Gamebryo has no block type table"
    );
    let variant = byroredux_nif::version::NifVariant::detect(
        header.version,
        header.user_version,
        header.user_version_2,
    );
    assert_eq!(variant, byroredux_nif::version::NifVariant::Morrowind);
}

#[test]
fn synthetic_morrowind_node_has_name() {
    let data = build_morrowind_nif();
    let scene = byroredux_nif::parse_nif(&data).unwrap();
    let node = scene.blocks[0]
        .as_any()
        .downcast_ref::<byroredux_nif::blocks::node::NiNode>()
        .expect("block 0 should be NiNode");
    assert_eq!(node.name(), Some("Scene Root"));
}
