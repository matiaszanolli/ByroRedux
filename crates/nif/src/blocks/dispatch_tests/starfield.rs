//! Starfield BSGeometry dispatch tests.
//!
//! External-mesh and internal-geom branches, SkinAttach, BoneTranslations —
//! #708 / NIF-D5-01 / D5-02 / D5-08.

use crate::blocks::*;
use crate::header::NifHeader;
use crate::stream::NifStream;
use crate::version::NifVersion;
use std::sync::Arc;

/// Starfield header (bsver=172, uv=12). Per
/// `crates/nif/src/version.rs::NifVariant::detect`. Skyrim+ string-table
/// shape — `read_string` resolves `0` to `strings[0]`.
fn starfield_header() -> NifHeader {
    NifHeader {
        version: NifVersion::V20_2_0_7,
        little_endian: true,
        user_version: 12,
        user_version_2: 172,
        num_blocks: 0,
        block_types: Vec::new(),
        block_type_indices: Vec::new(),
        block_sizes: Vec::new(),
        strings: vec![Arc::from("BSGeometry_Test")],
        max_string_length: 16,
        num_groups: 0,
    }
}

/// Build the NiAVObject (no-properties) prefix every BSGeometry shares.
/// `flags` lands on the parent NiAVObject; bit 0x200 is the
/// internal-geom-data gate.
fn starfield_av_prefix(flags: u32) -> Vec<u8> {
    let mut d = Vec::new();
    // NiObjectNET — name index, extra-data ref count, controller ref.
    d.extend_from_slice(&0i32.to_le_bytes()); // name = strings[0]
    d.extend_from_slice(&0u32.to_le_bytes()); // extra_data_refs count
    d.extend_from_slice(&(-1i32).to_le_bytes()); // controller_ref
                                                 // NiAVObject (parse_no_properties): flags(u32) + transform + collision_ref
    d.extend_from_slice(&flags.to_le_bytes());
    // NiTransform: rotation 3×3 matrix (9×f32) + translation (3×f32) + scale (f32)
    for v in [
        1.0f32, 0.0, 0.0, // row 0
        0.0, 1.0, 0.0, // row 1
        0.0, 0.0, 1.0, // row 2
        0.0, 0.0, 0.0, // translation
        1.0, // scale
    ] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&(-1i32).to_le_bytes()); // collision_ref
    d
}

/// Append the BSGeometry trailer (bounds + boundMinMax + 3 refs) and
/// `mesh_count` external-mesh slots (each: 3×u32 + sized-string).
fn starfield_external_geometry_bytes(flags: u32, mesh_names: &[&str]) -> Vec<u8> {
    assert!(mesh_names.len() <= 4);
    let mut d = starfield_av_prefix(flags);
    // bounds: Vector3 center + f32 radius
    for v in [0.0f32, 0.0, 0.0, 1.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    // boundMinMax: 6 × f32
    for v in [-1.0f32, -1.0, -1.0, 1.0, 1.0, 1.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    // 3 refs: skin / shader / alpha
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // 4 mesh slots — `mesh_names.len()` populated, rest absent.
    for i in 0..4 {
        if i < mesh_names.len() {
            d.push(1u8); // present
            d.extend_from_slice(&123u32.to_le_bytes()); // tri_size
            d.extend_from_slice(&456u32.to_le_bytes()); // num_verts
            d.extend_from_slice(&64u32.to_le_bytes()); // flags (nifly: "often 64")
                                                       // sized string: u32 length + bytes
            let name = mesh_names[i].as_bytes();
            d.extend_from_slice(&(name.len() as u32).to_le_bytes());
            d.extend_from_slice(name);
        } else {
            d.push(0u8); // absent
        }
    }
    d
}


/// Regression for #708 / NIF-D5-01: BSGeometry now dispatches and
/// captures the external-mesh-path branch (the 99% Starfield case).
/// Pre-fix it fell into NiUnknown and the entire mesh body was lost.
#[test]
fn starfield_bs_geometry_external_mesh_dispatches() {
    let header = starfield_header();
    let bytes = starfield_external_geometry_bytes(
        0, // no internal-geom flag → external-mesh branch
        &[
            "abcdef0123456789abcdef0123456789abcdef012",
            "secondary.mesh",
        ],
    );
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BSGeometry", &mut stream, Some(bytes.len() as u32))
        .expect("BSGeometry must dispatch");
    let geo = block
        .as_any()
        .downcast_ref::<bs_geometry::BSGeometry>()
        .expect("BSGeometry downcast");

    assert_eq!(geo.bounding_sphere.1, 1.0, "bounds.radius");
    assert!(!geo.has_internal_geom_data());
    assert_eq!(geo.meshes.len(), 2, "2 of 4 slots populated");
    match &geo.meshes[0].kind {
        bs_geometry::BSGeometryMeshKind::External { mesh_name } => {
            assert_eq!(mesh_name, "abcdef0123456789abcdef0123456789abcdef012");
        }
        _ => panic!("expected external mesh kind"),
    }
    assert_eq!(geo.meshes[0].tri_size, 123);
    assert_eq!(geo.meshes[0].num_verts, 456);
    assert_eq!(
        stream.position() as usize,
        bytes.len(),
        "BSGeometry must consume the whole block exactly"
    );
}

/// Internal-geom-data branch: bit 0x200 of NiAVObject flags switches
/// the per-mesh slot from external-name to inline `BSGeometryMeshData`.
/// Build a minimal-version body (`version > 2` early-out) so the test
/// stays compact while still exercising the internal-mesh dispatch.
#[test]
fn starfield_bs_geometry_internal_geom_data_branch_dispatches() {
    let header = starfield_header();
    // av-prefix flags with bit 0x200 set.
    let mut d = starfield_av_prefix(0x200);
    // bounds + boundMinMax + 3 refs (same trailer as external case).
    for v in [0.0f32, 0.0, 0.0, 0.5] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    for v in [-1.0f32, -1.0, -1.0, 1.0, 1.0, 1.0] {
        d.extend_from_slice(&v.to_le_bytes());
    }
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    d.extend_from_slice(&(-1i32).to_le_bytes());
    // One populated mesh slot followed by 3 absent.
    d.push(1u8);
    d.extend_from_slice(&0u32.to_le_bytes()); // tri_size
    d.extend_from_slice(&0u32.to_le_bytes()); // num_verts
    d.extend_from_slice(&64u32.to_le_bytes()); // flags
                                               // BSGeometryMeshData: version=99 (>2 → early-out, no body follows).
    d.extend_from_slice(&99u32.to_le_bytes());
    d.push(0u8);
    d.push(0u8);
    d.push(0u8);

    let mut stream = NifStream::new(&d, &header);
    let block = parse_block("BSGeometry", &mut stream, Some(d.len() as u32))
        .expect("BSGeometry must dispatch (internal branch)");
    let geo = block
        .as_any()
        .downcast_ref::<bs_geometry::BSGeometry>()
        .expect("BSGeometry downcast");

    assert!(geo.has_internal_geom_data());
    assert_eq!(geo.meshes.len(), 1);
    match &geo.meshes[0].kind {
        bs_geometry::BSGeometryMeshKind::Internal { mesh_data } => {
            assert_eq!(mesh_data.version, 99);
            assert!(mesh_data.vertices.is_empty(), "version > 2 → empty body");
        }
        _ => panic!("expected internal mesh kind"),
    }
}

/// Regression for #708 / NIF-D5-02: SkinAttach now dispatches and
/// reaches the `bones` NiStringVector decode. Pre-fix every Starfield
/// SkinAttach extra-data block fell into NiUnknown alongside its
/// parent BSGeometry.
#[test]
fn starfield_skin_attach_dispatches() {
    let header = starfield_header();
    let mut bytes = Vec::new();
    // NiExtraData prefix: name string-table index = 0.
    bytes.extend_from_slice(&0i32.to_le_bytes());
    // bones: NiStringVector — u32 count + count × NiString(u32 length + bytes).
    bytes.extend_from_slice(&3u32.to_le_bytes()); // count
    for s in ["Spine", "Head", "L_Hand"] {
        bytes.extend_from_slice(&(s.len() as u32).to_le_bytes());
        bytes.extend_from_slice(s.as_bytes());
    }
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("SkinAttach", &mut stream, Some(bytes.len() as u32))
        .expect("SkinAttach must dispatch");
    let extra = block
        .as_any()
        .downcast_ref::<extra_data::NiExtraData>()
        .expect("SkinAttach downcast to NiExtraData");
    let bones = extra
        .skin_attach_bones
        .as_ref()
        .expect("skin_attach_bones populated");
    assert_eq!(
        bones,
        &vec!["Spine".to_string(), "Head".into(), "L_Hand".into()]
    );
    assert_eq!(stream.position() as usize, bytes.len());
}

/// Regression for #708 / NIF-D5-08: BoneTranslations dispatches and
/// captures `(bone, translation)` pairs.
#[test]
fn starfield_bone_translations_dispatches() {
    let header = starfield_header();
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0i32.to_le_bytes()); // name index
    bytes.extend_from_slice(&2u32.to_le_bytes()); // numTranslations
    for (name, trans) in [("Spine", [0.1f32, 0.2, 0.3]), ("Head", [-0.4, 0.5, -0.6])] {
        bytes.extend_from_slice(&(name.len() as u32).to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
        for v in trans {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    let mut stream = NifStream::new(&bytes, &header);
    let block = parse_block("BoneTranslations", &mut stream, Some(bytes.len() as u32))
        .expect("BoneTranslations must dispatch");
    let extra = block
        .as_any()
        .downcast_ref::<extra_data::NiExtraData>()
        .expect("BoneTranslations downcast");
    let translations = extra
        .bone_translations
        .as_ref()
        .expect("bone_translations populated");
    assert_eq!(translations.len(), 2);
    assert_eq!(translations[0].0, "Spine");
    assert!((translations[0].1[0] - 0.1).abs() < 1e-6);
    assert_eq!(translations[1].0, "Head");
    assert!((translations[1].1[2] + 0.6).abs() < 1e-6);
    assert_eq!(stream.position() as usize, bytes.len());
}

// ── O5-3 / #688 — early-Gamebryo NiObject groupID prefix ─────────

