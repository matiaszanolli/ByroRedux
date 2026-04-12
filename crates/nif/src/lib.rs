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
pub mod kfm;
pub mod scene;
pub mod stream;
pub mod types;
pub mod version;

use blocks::{parse_block, NiObject};
use header::NifHeader;
use scene::NifScene;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use stream::NifStream;

/// Options for NIF parsing — allows skipping block categories for performance.
#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    /// Skip animation blocks (controllers, interpolators, animation data).
    /// Reduces parse time by 40-60% for character NIFs. Skipped blocks are
    /// replaced with NiUnknown placeholders (via block_size).
    /// Only effective when the NIF header has block sizes (v20.2.0.7+).
    pub skip_animation: bool,
    /// Hand-registered skip sizes for Oblivion-era unknown block types.
    ///
    /// Oblivion NIFs (v20.0.0.4/5) have no `block_sizes` table, so when a
    /// block parser returns `Err` the main loop cannot resume — it stops
    /// and marks the scene truncated, losing every subsequent block.
    ///
    /// When a type name is registered here, the loop instead seeks forward
    /// by the given size and inserts an `NiUnknown` placeholder, letting
    /// the rest of the file parse normally. Intended for rare/undiscovered
    /// block types whose size is known from an external source (Gamebryo
    /// 2.3 headers, nif_stats corpus analysis, modder documentation) but
    /// whose full schema has not been implemented yet. See #224.
    pub oblivion_skip_sizes: HashMap<String, u32>,
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
    // Set to `true` if an Oblivion-style (no block-sizes) parse bails out
    // early — `NifScene.truncated` exposes the state to downstream
    // consumers so they can decide how to handle the incomplete graph.
    let mut truncated = false;
    let mut dropped_block_count: usize = 0;

    if header.block_sizes.is_empty() && header.num_blocks > 0 {
        log::debug!(
            "NIF v{} has no block sizes — all {} block parsers must be byte-perfect (no recovery on error)",
            header.version,
            header.num_blocks
        );
    }

    // Pre-Gamebryo NetImmerse files (NIF v < 5.0.0.1, e.g. Morrowind at
    // v4.0.0.2) inline each block's type name as a sized string instead of
    // using a global block-type table. We read them inline in the loop below.
    let inline_type_names = header.block_types.is_empty() && header.num_blocks > 0;
    if inline_type_names {
        log::debug!(
            "NIF v{} uses inline block type names (pre-Gamebryo, {} blocks)",
            header.version,
            header.num_blocks
        );
    }

    for i in 0..header.num_blocks as usize {
        // Resolve block type name: from header table (Gamebryo+) or inline string (pre-Gamebryo).
        let inline_name: String;
        let type_name: &str = if inline_type_names {
            // Pre-Gamebryo: each block is prefixed by a u32-length-prefixed type name string.
            inline_name = stream.read_sized_string()?;
            &inline_name
        } else {
            header.block_type_name(i).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("block {} has no type name", i),
                )
            })?
        };

        let block_size = header.block_sizes.get(i).copied();
        let start_pos = stream.position();

        // Skip animation blocks when geometry-only parsing is requested.
        if options.skip_animation && is_animation_block(type_name) {
            if let Some(size) = block_size {
                stream.skip(size as u64)?;
                blocks.push(Box::new(blocks::NiUnknown {
                    type_name: Arc::from(type_name),
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
                    i,
                    type_name,
                    start_pos,
                    consumed
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
                if let Some(size) = block_size {
                    // With block_size we can recover: seek to the expected end of
                    // the block, record an NiUnknown placeholder, and keep going.
                    // Without this, a single buggy block parser (e.g. a Havok
                    // layout quirk) takes down the entire NIF. The unit tests
                    // still exercise the happy path; this is the belt-and-braces
                    // path that keeps `parse_rate_*` integration tests meaningful.
                    log::warn!(
                        "Block {} '{}' (size {}, offset {}, consumed {}): {} — \
                         seeking past block and inserting NiUnknown",
                        i,
                        type_name,
                        size,
                        start_pos,
                        consumed,
                        e
                    );
                    stream.set_position(start_pos + size as u64);
                    blocks.push(Box::new(blocks::NiUnknown {
                        type_name: Arc::from(type_name),
                        data: Vec::new(),
                    }));
                    continue;
                }
                // Without block_size (Oblivion), there's no header-driven
                // recovery. Before giving up, check the caller's registered
                // `oblivion_skip_sizes` map — if the type has a known fixed
                // size, rewind any partial read, skip forward, insert a
                // placeholder, and continue. This is the escape hatch for
                // rare/undiscovered Oblivion block types (#224).
                if let Some(&skip_size) = options.oblivion_skip_sizes.get(type_name) {
                    // Rewind whatever the failed parse consumed, then skip
                    // the full registered size. `set_position` is safe; the
                    // stream is backed by an in-memory slice.
                    stream.set_position(start_pos);
                    if stream.skip(skip_size as u64).is_ok() {
                        log::info!(
                            "Block {} '{}' (offset {}): skipped {} bytes via \
                             oblivion_skip_sizes hint (was: {})",
                            i, type_name, start_pos, skip_size, e
                        );
                        blocks.push(Box::new(blocks::NiUnknown {
                            type_name: Arc::from(type_name),
                            data: Vec::new(),
                        }));
                        continue;
                    }
                    // If the skip would go past EOF, fall through to the
                    // truncation path — the caller's hint was wrong.
                    log::warn!(
                        "Block {} '{}' (offset {}): oblivion_skip_sizes hint of {} \
                         bytes would exceed file length; truncating",
                        i, type_name, start_pos, skip_size
                    );
                }

                // Stop parsing but keep blocks parsed so far. This allows
                // geometry blocks to be imported even when collision blocks
                // fail. `truncated = true` is exposed via NifScene so
                // consumers that care about completeness can detect the
                // partial state. See #175.
                let dropped = header.num_blocks as usize - i;
                log::warn!(
                    "Block {} '{}' (offset {}, consumed {}): {} — stopping parse; \
                     keeping {} blocks, DISCARDING {} subsequent blocks (scene marked truncated)",
                    i, type_name, start_pos, consumed, e, blocks.len(), dropped
                );
                truncated = true;
                dropped_block_count = dropped;
                break;
            }
        }
    }

    // Phase 3: Identify root. Root is typically the first NiNode block.
    // When the scene is truncated that "first NiNode" may be a subtree
    // rather than the real root — the warning above documents the risk.
    let root_index = if !blocks.is_empty() {
        blocks
            .iter()
            .position(|b| matches!(b.block_type_name(), "NiNode"))
            .or(Some(0))
    } else {
        None
    };

    Ok(NifScene {
        blocks,
        root_index,
        truncated,
        dropped_block_count,
    })
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

    /// Regression test for issue #175: `NifScene.truncated` defaults to
    /// `false` on a happy-path parse, and can be distinguished from a
    /// genuinely-truncated scene by downstream consumers. The full
    /// end-to-end "Oblivion block parser errors mid-file" path is
    /// exercised by the ignored `parse_rate_oblivion` integration test
    /// against real .nif corpora — this unit test just pins the public
    /// field surface so that a future refactor of the error path can't
    /// silently drop the field.
    #[test]
    fn nif_scene_truncated_flag_defaults_false_on_clean_parse() {
        let data = build_test_nif_with_node();
        let scene = parse_nif(&data).unwrap();
        assert!(
            !scene.truncated,
            "clean parse must not set the truncated flag"
        );
        assert_eq!(scene.len(), 1);
    }

    #[test]
    fn nif_scene_struct_carries_truncated_field() {
        // Hand-constructed marker: verifies the field exists on the
        // struct surface so consumers like `cell_loader` can branch on
        // it without fear of the field being silently removed.
        let scene = NifScene {
            blocks: Vec::new(),
            root_index: None,
            truncated: true,
            dropped_block_count: 3,
        };
        assert!(scene.truncated);
        assert_eq!(scene.dropped_block_count, 3);
        assert!(scene.is_empty());
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
        assert_eq!(node.av.net.name.as_deref(), Some("SceneRoot"));
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

    /// Build a minimal Oblivion (v20.0.0.5) NIF with `num_unknown` blocks of
    /// a registered unknown type, each `payload_size` bytes of garbage.
    /// v20.0.0.5 has no `block_sizes` table and no string table, which is
    /// exactly the configuration that exercises the `oblivion_skip_sizes`
    /// recovery path in the main parse loop.
    fn build_oblivion_nif_with_unknowns(
        type_name: &str,
        num_unknown: usize,
        payload_size: usize,
    ) -> Vec<u8> {
        let mut buf = Vec::new();

        // ASCII header line.
        buf.extend_from_slice(b"Gamebryo File Format, Version 20.0.0.5\n");

        // Binary header.
        buf.extend_from_slice(&0x14000005u32.to_le_bytes()); // version
        buf.push(1); // little_endian
        buf.extend_from_slice(&11u32.to_le_bytes()); // user_version (Oblivion)
        buf.extend_from_slice(&(num_unknown as u32).to_le_bytes()); // num_blocks

        // BSStreamHeader (triggered by user_version >= 3).
        buf.extend_from_slice(&11u32.to_le_bytes()); // user_version_2
        buf.push(0); // author short_string: length 0
        buf.push(0); // process_script (user_version_2 < 131)
        buf.push(0); // export_script

        // Block types table.
        buf.extend_from_slice(&1u16.to_le_bytes()); // num_block_types
        buf.extend_from_slice(&(type_name.len() as u32).to_le_bytes());
        buf.extend_from_slice(type_name.as_bytes());

        // Block type indices — all blocks point at type 0.
        for _ in 0..num_unknown {
            buf.extend_from_slice(&0u16.to_le_bytes());
        }

        // No block_sizes (version < 20.2.0.7).
        // No string table (version < 20.1.0.1).

        // num_groups = 0.
        buf.extend_from_slice(&0u32.to_le_bytes());

        // Block data: each block is `payload_size` bytes of 0xAB.
        for _ in 0..num_unknown {
            buf.extend(std::iter::repeat(0xABu8).take(payload_size));
        }

        buf
    }

    /// Regression test for issue #224: on Oblivion NIFs (no block_sizes) the
    /// caller can register `oblivion_skip_sizes` hints that let the parser
    /// skip past unknown block types instead of truncating the scene.
    #[test]
    fn oblivion_skip_sizes_hint_recovers_unknown_blocks() {
        let type_name = "BSUnknownOblivionSkipTest";
        let payload = 24;
        let data = build_oblivion_nif_with_unknowns(type_name, 3, payload);

        // Default options: no hints → parse should truncate after the first
        // failing block, keeping 0 blocks.
        let default_scene = parse_nif(&data).unwrap();
        assert!(
            default_scene.truncated,
            "unknown-type Oblivion NIF must truncate without a hint"
        );
        assert_eq!(default_scene.dropped_block_count, 3);
        assert!(default_scene.blocks.is_empty());

        // With a registered hint the parser should skip past all 3 blocks.
        let mut options = ParseOptions::default();
        options
            .oblivion_skip_sizes
            .insert(type_name.to_string(), payload as u32);
        let scene = parse_nif_with_options(&data, &options).unwrap();

        assert!(!scene.truncated, "hint must prevent truncation");
        assert_eq!(scene.dropped_block_count, 0);
        assert_eq!(scene.len(), 3);
        for i in 0..3 {
            assert_eq!(scene.get(i).unwrap().block_type_name(), "NiUnknown");
        }
    }

    /// A too-large hint (past EOF) must NOT crash or advance the stream —
    /// the parser falls back to the truncation path gracefully.
    #[test]
    fn oblivion_skip_sizes_oversized_hint_falls_back_to_truncation() {
        let type_name = "BSUnknownOblivionOversize";
        let data = build_oblivion_nif_with_unknowns(type_name, 1, 16);

        let mut options = ParseOptions::default();
        // Hint is 9999 bytes but the payload is only 16 — skip would go
        // past EOF, so the parser should log a warning and truncate.
        options
            .oblivion_skip_sizes
            .insert(type_name.to_string(), 9999);
        let scene = parse_nif_with_options(&data, &options).unwrap();

        assert!(scene.truncated);
        assert_eq!(scene.dropped_block_count, 1);
        assert!(scene.blocks.is_empty());
    }

    // Real-game NIF parse coverage lives in `tests/parse_real_nifs.rs`, which
    // walks entire mesh archives and asserts a per-game success-rate threshold.
    // The old /tmp-based single-file smoke tests were removed in N23.10.
}
