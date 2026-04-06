//! NIF file parser for Gamebryo .nif files.
//!
//! Parses the binary NIF format used by Gamebryo 2.3 and derivative engines
//! (Oblivion, Skyrim, Fallout 3/4). Three-phase loading: parse → link → scene.
//!
//! # Usage
//! ```ignore
//! let bytes = std::fs::read("mesh.nif")?;
//! let scene = byroredux_nif::parse_nif(&bytes)?;
//! for block in &scene.blocks {
//!     println!("{}", block.block_type_name());
//! }
//! ```

pub mod anim;
pub mod blocks;
pub mod header;
pub mod import;
pub mod scene;
pub mod stream;
pub mod types;
pub mod version;

use blocks::{parse_block, NiObject};
use header::NifHeader;
use scene::NifScene;
use std::io;
use stream::NifStream;

/// Options for NIF parsing — allows skipping block categories for performance.
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    /// Skip animation blocks (controllers, interpolators, animation data).
    /// Reduces parse time by 40-60% for character NIFs. Skipped blocks are
    /// replaced with NiUnknown placeholders (via block_size).
    /// Only effective when the NIF header has block sizes (v20.2.0.7+).
    pub skip_animation: bool,
}

/// Animation block type names that can be skipped in geometry-only mode.
fn is_animation_block(type_name: &str) -> bool {
    matches!(
        type_name,
        "NiControllerManager"
            | "NiControllerSequence"
            | "NiMultiTargetTransformController"
            | "NiTransformController"
            | "NiVisController"
            | "NiAlphaController"
            | "NiMaterialColorController"
            | "NiTextureTransformController"
            | "NiGeomMorpherController"
            | "NiTransformInterpolator"
            | "BSRotAccumTransfInterpolator"
            | "NiTransformData"
            | "NiKeyframeData"
            | "NiFloatInterpolator"
            | "NiFloatData"
            | "NiPoint3Interpolator"
            | "NiPosData"
            | "NiBoolInterpolator"
            | "NiBoolData"
            | "NiBlendTransformInterpolator"
            | "NiBlendFloatInterpolator"
            | "NiBlendPoint3Interpolator"
            | "NiBlendBoolInterpolator"
            | "NiTextKeyExtraData"
            | "NiDefaultAVObjectPalette"
            | "NiMorphData"
    )
}

/// Parse a NIF file from raw bytes.
///
/// Performs all three phases: parse header → parse blocks → build scene.
pub fn parse_nif(data: &[u8]) -> io::Result<NifScene> {
    parse_nif_with_options(data, &ParseOptions::default())
}

/// Parse a NIF file with options (e.g., skip animation blocks).
pub fn parse_nif_with_options(data: &[u8], options: &ParseOptions) -> io::Result<NifScene> {
    // Phase 1: Parse header
    let (header, block_data_offset) = NifHeader::parse(data)?;
    log::debug!(
        "NIF version {}, {} blocks, {} strings",
        header.version,
        header.num_blocks,
        header.strings.len()
    );

    // Validate endianness — we only support little-endian (all PC game content).
    // Big-endian NIFs (Xbox 360 console ports) would produce silently wrong data.
    if !header.little_endian {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Big-endian NIF files are not supported (console format)",
        ));
    }

    // Phase 2: Parse blocks
    let block_data = &data[block_data_offset..];
    let mut stream = NifStream::new(block_data, &header);
    let mut blocks: Vec<Box<dyn NiObject>> = Vec::with_capacity(header.num_blocks as usize);

    if header.block_sizes.is_empty() && header.num_blocks > 0 {
        log::debug!(
            "NIF v{} has no block sizes — all {} block parsers must be byte-perfect (no recovery on error)",
            header.version,
            header.num_blocks
        );
    }

    for i in 0..header.num_blocks as usize {
        let type_name = header.block_type_name(i).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("block {} has no type name", i),
            )
        })?;

        let block_size = header.block_sizes.get(i).copied();
        let start_pos = stream.position();

        // Skip animation blocks when geometry-only parsing is requested.
        if options.skip_animation && is_animation_block(type_name) {
            if let Some(size) = block_size {
                stream.skip(size as u64);
                blocks.push(Box::new(blocks::NiUnknown {
                    type_name: type_name.to_string(),
                    data: Vec::new(), // Don't store data — just a placeholder
                }));
                continue;
            }
            // No block_size (Oblivion) — must parse, can't skip
        }

        match parse_block(type_name, &mut stream, block_size) {
            Ok(block) => {
                let consumed = stream.position() - start_pos;
                log::trace!(
                    "Block {} '{}': offset {}, consumed {} bytes",
                    i, type_name, start_pos, consumed
                );
                // Verify we consumed exactly block_size bytes (if known)
                if let Some(size) = block_size {
                    let consumed = stream.position() - start_pos;
                    if consumed != size as u64 {
                        log::warn!(
                            "Block {} '{}': expected {} bytes, consumed {}. Adjusting position.",
                            i,
                            type_name,
                            size,
                            consumed
                        );
                        stream.set_position(start_pos + size as u64);
                    }
                }
                blocks.push(block);
            }
            Err(e) => {
                let consumed = stream.position() - start_pos;
                if block_size.is_some() {
                    // With block_size, this is a hard error — block sizes guarantee recovery
                    return Err(io::Error::new(
                        e.kind(),
                        format!(
                            "block {} '{}' (size {:?}, offset {}, consumed {}): {}",
                            i, type_name, block_size, start_pos, consumed, e
                        ),
                    ));
                }
                // Without block_size (Oblivion), stop parsing but keep blocks parsed so far.
                // This allows geometry blocks to be imported even when collision blocks fail.
                log::warn!(
                    "Block {} '{}' (offset {}, consumed {}): {} — stopping parse, keeping {} blocks",
                    i, type_name, start_pos, consumed, e, blocks.len()
                );
                break;
            }
        }
    }

    // Phase 3: Identify root
    let root_index = if !blocks.is_empty() {
        // Root is typically the first NiNode block
        blocks
            .iter()
            .position(|b| matches!(b.block_type_name(), "NiNode"))
            .or(Some(0))
    } else {
        None
    };

    Ok(NifScene { blocks, root_index })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a complete minimal NIF file (v20.2.0.7, Skyrim-style)
    /// containing a single NiNode block with known field values.
    fn build_test_nif_with_node() -> Vec<u8> {
        let mut buf = Vec::new();

        // ── Header ──────────────────────────────────────────────────
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
        buf.extend_from_slice(&0x14020007u32.to_le_bytes()); // version
        buf.push(1); // little-endian
        buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (FNV)
        buf.extend_from_slice(&1u32.to_le_bytes()); // num_blocks = 1
        buf.extend_from_slice(&34u32.to_le_bytes()); // user_version_2 (FNV)

        // Short strings (author, process, export)
        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);

        // Block types: 1 type "NiNode"
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&6u32.to_le_bytes());
        buf.extend_from_slice(b"NiNode");

        // Block type indices: block 0 → type 0
        buf.extend_from_slice(&0u16.to_le_bytes());

        // ── Build NiNode block data first to know its size ──────────
        let mut block = Vec::new();

        // NiObjectNET: name (string table index 0 = "SceneRoot")
        block.extend_from_slice(&0i32.to_le_bytes());
        // extra_data_refs: count=0
        block.extend_from_slice(&0u32.to_le_bytes());
        // controller_ref: -1 (null)
        block.extend_from_slice(&(-1i32).to_le_bytes());

        // NiAVObject: flags (u32 for version >= 20.2.0.7)
        block.extend_from_slice(&14u32.to_le_bytes());
        // transform: translation (1.0, 2.0, 3.0)
        block.extend_from_slice(&1.0f32.to_le_bytes());
        block.extend_from_slice(&2.0f32.to_le_bytes());
        block.extend_from_slice(&3.0f32.to_le_bytes());
        // identity rotation (9 floats)
        for r in &[1.0f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0] {
            block.extend_from_slice(&r.to_le_bytes());
        }
        // scale: 1.0
        block.extend_from_slice(&1.0f32.to_le_bytes());
        // properties: count=0
        block.extend_from_slice(&0u32.to_le_bytes());
        // collision_ref: -1
        block.extend_from_slice(&(-1i32).to_le_bytes());

        // NiNode: children count=0
        block.extend_from_slice(&0u32.to_le_bytes());
        // effects count=0
        block.extend_from_slice(&0u32.to_le_bytes());

        // ── Back to header: block sizes ─────────────────────────────
        buf.extend_from_slice(&(block.len() as u32).to_le_bytes());

        // String table: 1 string "SceneRoot"
        buf.extend_from_slice(&1u32.to_le_bytes()); // num_strings
        buf.extend_from_slice(&9u32.to_le_bytes()); // max_string_length
        buf.extend_from_slice(&9u32.to_le_bytes()); // "SceneRoot" length
        buf.extend_from_slice(b"SceneRoot");

        // num_groups = 0
        buf.extend_from_slice(&0u32.to_le_bytes());

        // ── Block data ──────────────────────────────────────────────
        buf.extend_from_slice(&block);

        buf
    }

    #[test]
    fn parse_nif_minimal_node() {
        let data = build_test_nif_with_node();
        let scene = parse_nif(&data).unwrap();

        assert_eq!(scene.len(), 1);
        assert_eq!(scene.root_index, Some(0));

        let root = scene.root().unwrap();
        assert_eq!(root.block_type_name(), "NiNode");

        // Downcast and verify fields
        let node = scene.get_as::<blocks::node::NiNode>(0).unwrap();
        assert_eq!(node.av.net.name, Some("SceneRoot".to_string()));
        assert_eq!(node.av.flags, 14);
        assert_eq!(node.av.transform.translation.x, 1.0);
        assert_eq!(node.av.transform.translation.y, 2.0);
        assert_eq!(node.av.transform.translation.z, 3.0);
        assert_eq!(node.av.transform.scale, 1.0);
        assert!(node.children.is_empty());
        assert!(node.effects.is_empty());
        assert!(node.av.net.controller_ref.is_null());
        assert!(node.av.collision_ref.is_null());
    }

    #[test]
    fn parse_nif_empty_file() {
        // Build a NIF with 0 blocks
        let mut buf = Vec::new();
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
        buf.extend_from_slice(&0x14020007u32.to_le_bytes());
        buf.push(1);
        buf.extend_from_slice(&12u32.to_le_bytes()); // user_version
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_blocks = 0
        buf.extend_from_slice(&83u32.to_le_bytes()); // user_version_2

        buf.push(1);
        buf.push(0); // author
        buf.push(1);
        buf.push(0); // process
        buf.push(1);
        buf.push(0); // export

        buf.extend_from_slice(&0u16.to_le_bytes()); // num_block_types
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_strings
        buf.extend_from_slice(&0u32.to_le_bytes()); // max_string_length
        buf.extend_from_slice(&0u32.to_le_bytes()); // num_groups

        let scene = parse_nif(&buf).unwrap();
        assert!(scene.is_empty());
        assert_eq!(scene.root_index, None);
    }

    #[test]
    fn parse_nif_unknown_block_skipped() {
        let mut buf = Vec::new();
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.2.0.7\n");
        buf.extend_from_slice(&0x14020007u32.to_le_bytes());
        buf.push(1);
        buf.extend_from_slice(&12u32.to_le_bytes());
        buf.extend_from_slice(&1u32.to_le_bytes()); // 1 block
        buf.extend_from_slice(&83u32.to_le_bytes());

        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);
        buf.push(1);
        buf.push(0);

        // 1 block type: "BSUnknownFutureType"
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&19u32.to_le_bytes());
        buf.extend_from_slice(b"BSUnknownFutureType");

        // Block 0 → type 0
        buf.extend_from_slice(&0u16.to_le_bytes());

        // Block size: 8 bytes of dummy data
        buf.extend_from_slice(&8u32.to_le_bytes());

        // String table: 0 strings
        buf.extend_from_slice(&0u32.to_le_bytes());
        buf.extend_from_slice(&0u32.to_le_bytes());

        // num_groups = 0
        buf.extend_from_slice(&0u32.to_le_bytes());

        // Block data: 8 bytes of garbage
        buf.extend_from_slice(&[0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE]);

        let scene = parse_nif(&buf).unwrap();
        assert_eq!(scene.len(), 1);
        // Unknown block is preserved as NiUnknown
        assert_eq!(scene.get(0).unwrap().block_type_name(), "NiUnknown");
    }

    #[test]
    fn scene_get_as_wrong_type_returns_none() {
        let data = build_test_nif_with_node();
        let scene = parse_nif(&data).unwrap();

        // Block 0 is NiNode, not NiTriShape
        let result = scene.get_as::<blocks::tri_shape::NiTriShape>(0);
        assert!(result.is_none());
    }

    /// Parse a real Fallout: New Vegas NIF file (beer bottle).
    ///
    /// Requires /tmp/test_fnv_bottle.nif to be present — these are real game
    /// assets that can't be committed to the repo.
    #[test]
    #[ignore]
    fn parse_real_fnv_bottle() {
        let path = std::path::Path::new("/tmp/test_fnv_bottle.nif");
        if !path.exists() {
            eprintln!("Skipping: {path:?} not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let scene = parse_nif(&data).expect("parse_nif should succeed on FNV bottle");

        assert_eq!(scene.len(), 12, "FNV bottle should have 12 blocks");

        let meshes = import::import_nif(&scene);
        assert!(
            !meshes.is_empty(),
            "should import at least one mesh from bottle"
        );

        let m = &meshes[0];
        assert!(!m.positions.is_empty(), "mesh should have vertices");
        assert!(!m.indices.is_empty(), "mesh should have indices");
        eprintln!(
            "Bottle mesh: {} verts, {} indices, texture={:?}",
            m.positions.len(),
            m.indices.len(),
            m.texture_path
        );
        eprintln!("  translation: {:?}", m.translation);
        eprintln!("  scale: {}", m.scale);
        eprintln!("  scale: {}", m.scale);
        // Vertex bounds
        let (mut min, mut max) = (m.positions[0], m.positions[0]);
        for p in &m.positions {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        eprintln!("  vertex bounds: min={:?} max={:?}", min, max);
    }

    /// Parse a real Fallout: New Vegas NIF file (deathclaw sign).
    #[test]
    #[ignore]
    fn parse_real_fnv_sign() {
        let path = std::path::Path::new("/tmp/test_fnv.nif");
        if !path.exists() {
            eprintln!("Skipping: {path:?} not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let scene = parse_nif(&data).expect("parse_nif should succeed on FNV sign");

        assert_eq!(scene.len(), 18, "FNV sign should have 18 blocks");

        let meshes = import::import_nif(&scene);
        assert!(
            !meshes.is_empty(),
            "should import at least one mesh from sign"
        );
        eprintln!("Sign: {} meshes imported", meshes.len());
    }

    /// Parse a real Fallout: New Vegas NIF file (cave rock).
    #[test]
    #[ignore]
    fn parse_real_fnv_rock() {
        let path = std::path::Path::new("/tmp/test_fnv_rock.nif");
        if !path.exists() {
            eprintln!("Skipping: {path:?} not found");
            return;
        }
        let data = std::fs::read(path).unwrap();
        let scene = parse_nif(&data).expect("parse_nif should succeed on FNV rock");

        let meshes = import::import_nif(&scene);
        assert!(
            !meshes.is_empty(),
            "should import at least one mesh from rock"
        );
        eprintln!(
            "Rock: {} meshes, first has {} verts",
            meshes.len(),
            meshes[0].positions.len()
        );
    }
}
