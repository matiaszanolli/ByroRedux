//! Real-data BA2 reader regression tests (FO4-DIM2-05 / #587).
//!
//! Pre-#587 the `byroredux-bsa` crate shipped 11 synthetic unit tests
//! with **zero** byte-equality coverage against vanilla archives.
//! Real-data exercise piggy-backed on `byroredux-nif` / `byroredux-bgsm`
//! tests, which assert downstream parse success but never extracted-byte
//! correctness. The session-7 458,617-file brute-force sweep that
//! validated the reader end-to-end was external; nothing in CI guarded
//! against a future regression flipping a single byte in the GNRL or
//! DX10 path.
//!
//! These tests close that gap. The synthetic uncompressed-GNRL test
//! (`uncompressed_gnrl_packed_size_zero_round_trips`) runs unconditionally
//! — the audit's #587 specifically called out the
//! `packed_size == 0` branch as having zero coverage anywhere in the
//! crate, and a fully-in-memory archive needs no game data.
//!
//! The remaining tests are `#[ignore]`-gated on `BYROREDUX_FO4_DATA`
//! and run on demand via:
//! ```sh
//! cargo test -p byroredux-bsa --test ba2_real -- --ignored
//! ```

use byroredux_bsa::{Ba2Archive, Ba2Variant};
use std::io::Write;
use std::path::PathBuf;

/// Resolve a `Data/` directory from an env var, falling back to the
/// canonical Steam install path on the dev machine. Mirrors the
/// `crates/nif/tests/common/mod.rs::game_data_dir` pattern so all
/// real-data tests gate the same way.
fn data_dir(env_var: &str, fallback: &str) -> Option<PathBuf> {
    if let Ok(v) = std::env::var(env_var) {
        let p = PathBuf::from(&v);
        if p.is_dir() {
            return Some(p);
        }
        eprintln!("{env_var} points to {v:?} which is not a directory; falling back to default");
    }
    let p = PathBuf::from(fallback);
    if p.is_dir() {
        Some(p)
    } else {
        None
    }
}

fn fo4_data_dir() -> Option<PathBuf> {
    data_dir(
        "BYROREDUX_FO4_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
    )
}

/// Pick an arbitrary entry from the archive's listing. Used by the
/// real-data tests so a BA2 patch revision that renames Power Armor
/// meshes doesn't silently fail to find a hardcoded path. We don't
/// care WHICH file we extract — just that *some* extraction succeeds
/// and produces bytes that pass the magic check.
fn pick_entry(archive: &Ba2Archive, suffix: &str) -> Option<String> {
    archive
        .list_files()
        .into_iter()
        .find(|p| p.ends_with(suffix))
        .map(|s| s.to_string())
}

// ── Synthetic uncompressed-GNRL coverage (always runs) ──────────────

/// Hand-build a 1-file BA2 v1 GNRL with `packed_size = 0` (the
/// "no compression" sentinel that vanilla archives essentially never
/// emit). Verifies the archive reader byte-equality round-trips the
/// raw payload through `extract`. Pre-#587 this branch had zero
/// coverage anywhere in the crate.
#[test]
fn uncompressed_gnrl_packed_size_zero_round_trips() {
    // Synthetic payload — 64 bytes of structured data so a corrupt
    // extract is obvious from the assertion message.
    let payload: Vec<u8> = (0u8..64).collect();
    let path_in_archive = "test\\synth.bin";

    // ── Header (24 bytes): BTDX + version + GNRL + file_count + name_table_offset
    let mut buf = Vec::new();
    buf.extend_from_slice(b"BTDX"); // magic
    buf.extend_from_slice(&1u32.to_le_bytes()); // version 1 (FO4 baseline)
    buf.extend_from_slice(b"GNRL"); // type tag
    buf.extend_from_slice(&1u32.to_le_bytes()); // file count = 1
    // name_table_offset placeholder — patched after we know the layout.
    let name_table_offset_pos = buf.len();
    buf.extend_from_slice(&0u64.to_le_bytes());

    // ── Single 36-byte GNRL record ──
    // The BA2 reader hashes paths via `normalize_path` so name_hash /
    // ext / dir_hash don't have to be authentic — they're never used
    // for lookup. Padding must be `0xBAADF00D` to match the real
    // format (the reader logs a debug-level warn on mismatch but still
    // parses the record, so a placeholder works either way; we use the
    // canonical value so the format reads cleanly).
    let record_pos = buf.len();
    buf.extend_from_slice(&[0u8; 4]); // name_hash
    buf.extend_from_slice(b"bin\0"); // ext
    buf.extend_from_slice(&[0u8; 4]); // dir_hash
    buf.extend_from_slice(&0u32.to_le_bytes()); // flags
    let offset_field_pos = buf.len();
    buf.extend_from_slice(&0u64.to_le_bytes()); // offset (patched)
    buf.extend_from_slice(&0u32.to_le_bytes()); // packed_size = 0 (uncompressed)
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes()); // unpacked_size
    buf.extend_from_slice(&0xBAADF00Du32.to_le_bytes()); // padding sentinel
    debug_assert_eq!(buf.len() - record_pos, 36);

    // ── Payload data ──
    let payload_offset = buf.len() as u64;
    buf.extend_from_slice(&payload);

    // ── Name table: u16 length + path bytes ──
    let name_table_offset = buf.len() as u64;
    buf.extend_from_slice(&(path_in_archive.len() as u16).to_le_bytes());
    buf.extend_from_slice(path_in_archive.as_bytes());

    // Patch the placeholders.
    buf[offset_field_pos..offset_field_pos + 8]
        .copy_from_slice(&payload_offset.to_le_bytes());
    buf[name_table_offset_pos..name_table_offset_pos + 8]
        .copy_from_slice(&name_table_offset.to_le_bytes());

    // Write to a temporary file so the reader can `mmap`/`File::open`
    // it through the standard path (the reader doesn't expose an
    // in-memory entry point).
    let tmp = std::env::temp_dir().join(format!(
        "byroredux-test-{}-uncompressed-gnrl.ba2",
        std::process::id(),
    ));
    {
        let mut f = std::fs::File::create(&tmp).expect("create tmp BA2");
        f.write_all(&buf).expect("write tmp BA2");
    }

    let result = (|| -> Result<(), String> {
        let archive = Ba2Archive::open(&tmp).map_err(|e| format!("open: {e}"))?;
        assert_eq!(archive.version(), 1);
        assert_eq!(archive.variant(), Ba2Variant::General);
        assert_eq!(archive.file_count(), 1);

        // Path lookup is case + slash insensitive.
        assert!(archive.contains(path_in_archive));
        assert!(archive.contains("TEST/SYNTH.BIN"));

        let extracted = archive
            .extract(path_in_archive)
            .map_err(|e| format!("extract: {e}"))?;
        if extracted != payload {
            return Err(format!(
                "extracted bytes diverge from synth payload — len {} vs {}, \
                 first 16: {:?} vs {:?}",
                extracted.len(),
                payload.len(),
                &extracted[..extracted.len().min(16)],
                &payload[..payload.len().min(16)]
            ));
        }
        Ok(())
    })();

    // Always clean up the tmp file before re-raising any assertion.
    let _ = std::fs::remove_file(&tmp);
    result.expect("uncompressed-GNRL synth round-trip");
}

// ── Real-FO4 BA2 coverage (gated, opt-in via `--ignored`) ───────────

/// FO4 v8 GNRL — open `Fallout4 - Meshes.ba2`, extract a real NIF,
/// assert the first bytes spell out the Gamebryo magic header (the
/// audit's "first 4 bytes = `Game`" check). The exact entry path is
/// resolved through `list_files` so a future BA2 patch revision that
/// reshuffles Power Armor meshes doesn't silently break the test.
#[test]
#[ignore]
fn fo4_meshes_ba2_v8_gnrl_extracts_nif_with_gamebryo_magic() {
    let Some(data) = fo4_data_dir() else {
        eprintln!("Skipping: BYROREDUX_FO4_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Fallout4 - Meshes.ba2");
    if !archive_path.is_file() {
        eprintln!("Skipping: {archive_path:?} not found");
        return;
    }

    let archive = Ba2Archive::open(&archive_path).expect("open FO4 Meshes.ba2");
    assert_eq!(archive.version(), 8, "FO4 Meshes.ba2 must be v8");
    assert_eq!(archive.variant(), Ba2Variant::General);
    assert!(
        archive.file_count() > 1000,
        "FO4 Meshes.ba2 ships ~42k entries; got {}",
        archive.file_count()
    );

    let entry = pick_entry(&archive, ".nif").expect("at least one .nif in FO4 Meshes.ba2");
    let bytes = archive
        .extract(&entry)
        .unwrap_or_else(|e| panic!("extract '{entry}' failed: {e}"));
    assert!(
        bytes.len() >= 20,
        "NIF '{entry}' decompressed to {} bytes — too small to carry the magic header",
        bytes.len()
    );
    // First 4 bytes spell "Game" (start of "Gamebryo File Format").
    assert_eq!(
        &bytes[..4],
        b"Game",
        "extracted '{entry}' lacks Gamebryo magic; got {:?}",
        &bytes[..4]
    );
}

/// FO4 v7 DX10 — open `Fallout4 - Textures1.ba2`, extract a known
/// cubemap, assert the synthesized DDS carries the magic + the DX10
/// `D3D10_RESOURCE_MISC_TEXTURECUBE` flag in its extended header.
#[test]
#[ignore]
fn fo4_textures1_ba2_v7_dx10_synthesizes_cubemap_dds() {
    let Some(data) = fo4_data_dir() else {
        eprintln!("Skipping: BYROREDUX_FO4_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Fallout4 - Textures1.ba2");
    if !archive_path.is_file() {
        eprintln!("Skipping: {archive_path:?} not found");
        return;
    }

    let archive = Ba2Archive::open(&archive_path).expect("open FO4 Textures1.ba2");
    assert_eq!(archive.version(), 7, "FO4 Textures1.ba2 must be v7");
    assert_eq!(archive.variant(), Ba2Variant::Dx10);

    // Cubemap files in FO4 use the `\cubemaps\` directory and a
    // `_e.dds` / `cube*.dds` name. Pick whichever is present so a
    // single-name rename in a patch doesn't break the test.
    let cubemap = archive
        .list_files()
        .into_iter()
        .find(|p| p.contains("\\cubemaps\\") && p.ends_with(".dds"))
        .map(|s| s.to_string())
        .expect("FO4 Textures1.ba2 must ship at least one cubemap");

    let dds = archive
        .extract(&cubemap)
        .unwrap_or_else(|e| panic!("extract '{cubemap}' failed: {e}"));
    assert!(
        dds.len() > 148,
        "DDS for '{cubemap}' must include header + payload; got {} bytes",
        dds.len()
    );
    // DDS magic.
    assert_eq!(
        &dds[..4],
        b"DDS ",
        "extracted '{cubemap}' lacks DDS magic; got {:?}",
        &dds[..4]
    );

    // DX10 extended header sits at offset 128. Layout (per ba2.rs:638):
    //   128..132 dxgiFormat
    //   132..136 resourceDimension
    //   136..140 miscFlag           ← cubemap bit lives here
    //   140..144 arraySize          (must be 6 for cubemaps per #593)
    //   144..148 miscFlags2
    let misc_flag = u32::from_le_bytes(dds[136..140].try_into().unwrap());
    let array_size = u32::from_le_bytes(dds[140..144].try_into().unwrap());
    const D3D10_MISC_TEXTURECUBE: u32 = 0x0000_0004;
    assert!(
        misc_flag & D3D10_MISC_TEXTURECUBE != 0,
        "cubemap '{cubemap}' must carry D3D10_RESOURCE_MISC_TEXTURECUBE \
         (0x4) in its DX10 extended-header miscFlag; got 0x{misc_flag:08x}"
    );
    assert_eq!(
        array_size, 6,
        "cubemap '{cubemap}' must report arraySize = 6 (DX10 spec / #593); got {array_size}"
    );
}

/// Brute-force regression sweep — open `Fallout4 - Meshes.ba2` and
/// extract every NIF entry, asserting zero errors. Mirrors the
/// session-7 458,617-file external sweep but committed as a CI guard.
/// `#[ignore]`-gated because the full extract takes several seconds
/// and hits ~10 GB of disk I/O; opt-in via `--ignored`.
#[test]
#[ignore]
fn fo4_meshes_ba2_v8_brute_force_extract_zero_errors() {
    let Some(data) = fo4_data_dir() else {
        eprintln!("Skipping: BYROREDUX_FO4_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Fallout4 - Meshes.ba2");
    if !archive_path.is_file() {
        eprintln!("Skipping: {archive_path:?} not found");
        return;
    }

    let archive = Ba2Archive::open(&archive_path).expect("open FO4 Meshes.ba2");
    let entries: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|p| p.ends_with(".nif"))
        .map(|s| s.to_string())
        .collect();
    assert!(
        entries.len() > 30_000,
        "FO4 Meshes.ba2 ships ~35k NIFs; got {}",
        entries.len()
    );

    let mut errors: Vec<(String, String)> = Vec::new();
    let mut total_bytes: u64 = 0;
    for path in &entries {
        match archive.extract(path) {
            Ok(bytes) => total_bytes += bytes.len() as u64,
            Err(e) => {
                errors.push((path.clone(), e.to_string()));
                if errors.len() >= 16 {
                    // Stop accumulating once we have a representative
                    // sample — printing 35k error lines isn't useful.
                    break;
                }
            }
        }
    }

    eprintln!(
        "FO4 brute-force extract: {} NIFs, {:.1} GB total, {} errors",
        entries.len(),
        total_bytes as f64 / 1_073_741_824.0,
        errors.len(),
    );
    if !errors.is_empty() {
        for (path, err) in &errors {
            eprintln!("  ERR  {path}: {err}");
        }
        panic!(
            "FO4 Meshes.ba2 extract sweep produced {} errors (audit: must be 0)",
            errors.len()
        );
    }
}

// ── Real-Starfield BA2 coverage (gated, opt-in via `--ignored`) ─────

fn starfield_data_dir() -> Option<PathBuf> {
    data_dir(
        "BYROREDUX_STARFIELD_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data",
    )
}

/// Starfield v2 GNRL — open `Starfield - Meshes01.ba2`, extract a real
/// NIF, assert the first four bytes spell the Gamebryo magic `"Game"`.
/// This guards the v2 GNRL extraction path that was only verified by a
/// one-shot external sweep in session 7 (#756).
#[test]
#[ignore]
fn starfield_meshes01_ba2_v2_gnrl_extracts_nif_with_starfield_magic() {
    let Some(data) = starfield_data_dir() else {
        eprintln!("Skipping: BYROREDUX_STARFIELD_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Starfield - Meshes01.ba2");
    if !archive_path.is_file() {
        eprintln!("Skipping: {archive_path:?} not found");
        return;
    }

    let archive = Ba2Archive::open(&archive_path).expect("open Starfield Meshes01.ba2");
    assert_eq!(archive.version(), 2, "Starfield Meshes01.ba2 must be v2");
    assert_eq!(archive.variant(), Ba2Variant::General);
    assert!(
        archive.file_count() > 10_000,
        "Starfield Meshes01.ba2 ships >300k entries; got {}",
        archive.file_count()
    );

    let entry = pick_entry(&archive, ".nif").expect("at least one .nif in Starfield Meshes01.ba2");
    let bytes = archive
        .extract(&entry)
        .unwrap_or_else(|e| panic!("extract '{entry}' failed: {e}"));
    assert!(
        bytes.len() >= 20,
        "NIF '{entry}' decompressed to {} bytes — too small for magic header",
        bytes.len()
    );
    assert_eq!(
        &bytes[..4],
        b"Game",
        "extracted '{entry}' lacks Gamebryo magic; got {:?}",
        &bytes[..4]
    );
}

/// Starfield v3 DX10 — open `Starfield - Textures01.ba2`, extract a
/// DDS, assert DDS magic and that the decompressed buffer is non-trivial.
/// This specifically guards the v3 LZ4-block decompression path
/// (`compression_method = 3` in the 12-byte v3 header extension) that
/// was only exercised externally (#756).
#[test]
#[ignore]
fn starfield_textures01_ba2_v3_dx10_extracts_lz4_block_dds() {
    let Some(data) = starfield_data_dir() else {
        eprintln!("Skipping: BYROREDUX_STARFIELD_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Starfield - Textures01.ba2");
    if !archive_path.is_file() {
        eprintln!("Skipping: {archive_path:?} not found");
        return;
    }

    let archive = Ba2Archive::open(&archive_path).expect("open Starfield Textures01.ba2");
    assert_eq!(archive.version(), 3, "Starfield Textures01.ba2 must be v3");
    assert_eq!(archive.variant(), Ba2Variant::Dx10);

    let entry = pick_entry(&archive, ".dds").expect("at least one .dds in Starfield Textures01.ba2");
    let dds = archive
        .extract(&entry)
        .unwrap_or_else(|e| panic!("extract '{entry}' failed: {e}"));
    assert!(
        dds.len() > 128,
        "DDS '{entry}' must include header + payload; got {} bytes",
        dds.len()
    );
    assert_eq!(
        &dds[..4],
        b"DDS ",
        "extracted '{entry}' lacks DDS magic; got {:?}",
        &dds[..4]
    );
    // LZ4 round-trip smoke: at least one byte in the mip data must be non-zero.
    assert!(
        dds[128..].iter().any(|&b| b != 0),
        "DDS '{entry}' payload appears to be all-zero after LZ4 decompress (decompression bug?)"
    );
}

/// Starfield v2 DX10 (zlib) — open `Constellation - Textures.ba2`,
/// extract a DDS, assert DDS magic. This guards the DX10 path for
/// Starfield DLC archives that use v2 (zlib) rather than v3 (LZ4),
/// ensuring we don't gate extraction on archive type_tag alone (#756).
#[test]
#[ignore]
fn starfield_constellation_textures_ba2_v2_dx10_extracts_zlib_dds() {
    let Some(data) = starfield_data_dir() else {
        eprintln!("Skipping: BYROREDUX_STARFIELD_DATA not set and default path missing");
        return;
    };
    let archive_path = data.join("Constellation - Textures.ba2");
    if !archive_path.is_file() {
        eprintln!("Skipping: {archive_path:?} not found");
        return;
    }

    let archive = Ba2Archive::open(&archive_path).expect("open Constellation Textures.ba2");
    assert_eq!(archive.version(), 2, "Constellation Textures.ba2 must be v2");
    assert_eq!(archive.variant(), Ba2Variant::Dx10);

    let entry = pick_entry(&archive, ".dds")
        .expect("at least one .dds in Constellation Textures.ba2");
    let dds = archive
        .extract(&entry)
        .unwrap_or_else(|e| panic!("extract '{entry}' failed: {e}"));
    assert!(
        dds.len() > 128,
        "DDS '{entry}' must include header + payload; got {} bytes",
        dds.len()
    );
    assert_eq!(
        &dds[..4],
        b"DDS ",
        "extracted '{entry}' lacks DDS magic; got {:?}",
        &dds[..4]
    );
}
