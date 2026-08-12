//! Starfield `.mat` resolution arm (#1289 / SF-D3-NEW-01).
//!
//! Extracted from the 2051-LOC `asset_provider/tests.rs` (#2411 / TD1-010)
//! at the topic-divider comments that file already carried. Contents
//! unchanged.

use super::super::*;
use super::imported_mesh_with_material_path;

// ── #1289 / SF-D3-NEW-01 — Starfield `.mat` arm in
//   `merge_external_material`. Verifies the audit-fail closure: a
//   Starfield-shaped mesh (`.mat` material path) flips `is_pbr`
//   when (and only when) the Component Database is loaded.

/// Synthetic minimal CDB: BETH magic + header + STRT (empty) + TYPE
/// chunk declaring zero types. Sufficient for `register_starfield_cdb`
/// to mark `has_starfield_cdb() == true` without needing 105 MB of
/// real Starfield data.
///
/// `chunkCount` field is "chunks INCLUDING the BETH header" per
/// `crates/sfmaterial/src/reader.rs::index_chunks` line 143-147 —
/// BETH + STRT + TYPE = 3, so the post-header chunk loop reads 2.
fn minimal_cdb_bytes() -> Vec<u8> {
    let mut buf = Vec::with_capacity(40);
    // 16-byte header: magic + headerSize + fileVersion + chunkCount=3.
    buf.extend_from_slice(&0x48544542u32.to_le_bytes()); // BETH
    buf.extend_from_slice(&8u32.to_le_bytes()); // headerSize
    buf.extend_from_slice(&4u32.to_le_bytes()); // fileVersion
    buf.extend_from_slice(&3u32.to_le_bytes()); // chunkCount (incl BETH)
                                                // STRT chunk: type + size + empty payload.
    buf.extend_from_slice(b"STRT");
    buf.extend_from_slice(&0u32.to_le_bytes()); // size = 0
                                                // TYPE chunk: type + size=4 + u32 type_count=0.
    buf.extend_from_slice(b"TYPE");
    buf.extend_from_slice(&4u32.to_le_bytes()); // size = 4
    buf.extend_from_slice(&0u32.to_le_bytes()); // type_count = 0
    buf
}

/// #1571 / SF-D3-03 — the discovery predicate must match the base
/// CDB AND every DLC/Creation-namespaced one, and reject everything
/// else. `Ba2Archive::list_files` hands back lowercase/backslash
/// paths, but the predicate normalises so it's robust to either.
#[test]
fn is_materialsbeta_cdb_path_matches_base_and_dlc() {
    // Base game.
    assert!(is_materialsbeta_cdb_path("materials\\materialsbeta.cdb"));
    // DLC / Creations — the paths the hardcoded extract missed.
    assert!(is_materialsbeta_cdb_path(
        "materials\\creations\\shatteredspace\\materialsbeta.cdb"
    ));
    assert!(is_materialsbeta_cdb_path(
        "materials\\creations\\sfbgs003\\materialsbeta.cdb"
    ));
    assert!(is_materialsbeta_cdb_path(
        "materials\\creations\\sfbgs00d\\materialsbeta.cdb"
    ));
    // Forward-slash + mixed-case input still matches (normalised).
    assert!(is_materialsbeta_cdb_path(
        "Materials/Creations/ShatteredSpace/MaterialsBeta.cdb"
    ));
    // Non-CDB / wrong-root paths are rejected.
    assert!(!is_materialsbeta_cdb_path("materials\\foo\\bar.bgsm"));
    assert!(!is_materialsbeta_cdb_path(
        "meshes\\materialsbeta.cdb" // right filename, wrong root
    ));
    assert!(!is_materialsbeta_cdb_path("materialsbeta.cdb")); // no materials\ root
}

/// #1571 — every discovered CDB is held in load order (base first,
/// then DLC) so SF-D3-01 Phase 2 can build one last-wins index. The
/// pre-fix single `Option<Arc<…>>` could only hold one, silently
/// dropping DLC materials once Phase 2 lands.
#[test]
fn discovered_cdbs_accumulate_in_load_order() {
    let mut provider = MaterialProvider::new();
    assert!(!provider.has_starfield_cdb(), "empty provider has no CDB");

    // Base CDB, then a DLC CDB — both pass the header-only probe
    // (#2100), both counted.
    provider.register_starfield_cdb(&minimal_cdb_bytes());
    provider.register_starfield_cdb(&minimal_cdb_bytes());
    assert!(provider.has_starfield_cdb());
    assert_eq!(
        provider.sf_cdb_count, 2,
        "a second CDB must increment the count, not replace the first (was \
         the single-Option bug that dropped DLC CDBs)"
    );

    // A malformed CDB is rejected (peek_magic, #2102) + warned, leaving
    // the count intact.
    provider.register_starfield_cdb(b"not a cdb");
    assert_eq!(
        provider.sf_cdb_count, 2,
        "a rejected CDB must not change the already-counted CDBs"
    );
}

/// Audit-fail closure: a `.mat` path on a Starfield mesh with the
/// CDB loaded must flip `is_pbr=true` so `pack_imported_material_flags`
/// packs `MAT_FLAG_PBR_BSDF` and `triangle.frag` routes through
/// Disney BSDF instead of legacy Lambert.
#[test]
fn merge_sets_is_pbr_on_mat_path_when_cdb_loaded() {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut provider = MaterialProvider::new();
    provider.register_starfield_cdb(&minimal_cdb_bytes());
    assert!(
        provider.has_starfield_cdb(),
        "minimal CDB payload must mark the provider as Starfield-loaded"
    );

    let mut mesh = imported_mesh_with_material_path(&mut pool, "materials/setpieces/cargobay.mat");
    assert!(
        !mesh.material.is_pbr,
        "fresh ImportedMesh defaults to is_pbr=false"
    );

    let touched = merge_external_material(&mut mesh.material, &mut provider, &mut pool);

    assert!(touched, ".mat arm must report touched=true");
    assert!(
        mesh.material.is_pbr,
        "Starfield .mat path must flip is_pbr=true → MAT_FLAG_PBR_BSDF in shader"
    );
    // `from_bgsm` deliberately stays false — that flag gates BGSM
    // spec-glossiness translation which is wrong for Starfield .mat
    // (metalness/roughness direct authoring).
    assert!(
        !mesh.material.from_bgsm,
        "Starfield path must NOT set from_bgsm"
    );
}

/// CDB-presence gate: a `.mat` path against a non-Starfield archive
/// set (no CDB loaded) must NOT flip `is_pbr`. Modded `.mat` paths
/// on FO4 / FNV / Skyrim cells shouldn't accidentally route to
/// Disney BSDF.
#[test]
fn merge_skips_mat_path_when_cdb_absent() {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut provider = MaterialProvider::new();
    // No `register_starfield_cdb` call.
    assert!(!provider.has_starfield_cdb());

    let mut mesh = imported_mesh_with_material_path(&mut pool, "materials/modded.mat");
    let touched = merge_external_material(&mut mesh.material, &mut provider, &mut pool);

    // Falls through past the .mat arm; bgsm/bgem dispatch fails
    // because the path doesn't match either suffix; returns false
    // (no archive to resolve from anyway).
    assert!(!touched, "no CDB + no archives → no merge work");
    assert!(
        !mesh.material.is_pbr,
        ".mat path without CDB must NOT flip is_pbr"
    );
}

/// SF3-02 / #1831 — a `.mat` path with no CDB loaded gets the
/// CDB-specific diagnostic, naming the actual degradation instead of
/// the generic "unsupported format" message.
#[test]
fn unresolved_material_warning_names_missing_cdb_for_mat_path() {
    let msg = unresolved_material_warning("materials/modded.mat", false);
    assert!(
        msg.contains("no CDB is loaded/parsed"),
        "expected the CDB-specific diagnostic, got: {msg}"
    );
    assert!(msg.contains("--materials-ba2"));
}

/// A `.mat` path is only reachable in this arm when the CDB IS present
/// but nonetheless useless here (defence in depth) — must fall back to
/// the generic message rather than falsely blaming a present CDB.
#[test]
fn unresolved_material_warning_falls_back_when_cdb_present() {
    let msg = unresolved_material_warning("materials/modded.mat", true);
    assert!(
        msg.contains("unsupported format"),
        "expected the generic diagnostic when a CDB is loaded, got: {msg}"
    );
}

/// A non-`.mat` unrecognised extension always gets the generic message,
/// regardless of CDB state — the CDB-specific wording is `.mat`-only.
#[test]
fn unresolved_material_warning_generic_for_non_mat_path() {
    let msg = unresolved_material_warning("materials/weird.xyz", false);
    assert!(msg.contains("unsupported format"));
    assert!(!msg.contains("CDB"));
}

/// A `.bgsm` path must NOT enter the Starfield arm even when the
/// CDB is loaded — the FO4 BGSM dispatch wins, preserving
/// spec-glossiness translation.
#[test]
fn mat_arm_does_not_steal_bgsm_dispatch() {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut provider = MaterialProvider::new();
    provider.register_starfield_cdb(&minimal_cdb_bytes());

    let mut mesh =
        imported_mesh_with_material_path(&mut pool, "materials/setdressing/metallocker01.bgsm");
    let _ = merge_external_material(&mut mesh.material, &mut provider, &mut pool);

    // The .bgsm path falls past the .mat arm into BGSM dispatch,
    // which fails on the missing archive (no .bgsm to extract).
    // `is_pbr` stays at its default — BGSM dispatch doesn't flip
    // it without a successful resolve.
    assert!(
        !mesh.material.is_pbr,
        ".bgsm path must not be hijacked by the Starfield arm"
    );
}
