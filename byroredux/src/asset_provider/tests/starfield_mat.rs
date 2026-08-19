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

    let outcome = merge_external_material(&mut mesh.material, &mut provider, &mut pool);

    // #2709 (SF-D9-03) — this is the exact case the old `bool` return
    // could not name: the sidecar resolved, but the arm forwarded only
    // the `is_pbr` routing flag and no authored field. `PresenceOnly`,
    // never `Merged` — until Phase 2's CDB per-field extraction lands.
    assert_eq!(
        outcome,
        MergeOutcome::PresenceOnly,
        ".mat arm resolves but forwards no authored field"
    );
    assert!(outcome.resolved(), "the .mat sidecar did resolve");
    assert!(
        !outcome.merged(),
        "no authored field was forwarded — must not count as a populated merge"
    );
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

/// Regression / checklist-invariant pin for #2359 (SF-D9-2026-08-03-03).
///
/// The `.mat` arm is a documented Phase 1 stub — `register_starfield_cdb`
/// only probes the header (`ComponentDatabaseFile::probe_header`), the
/// ~1.44M-instance class/object tree is never walked, and there is
/// currently NO code path from CDB contents to `ImportedMaterial`. This
/// test pins that today's `.mat` merge forwards ZERO authored texture
/// data — not one `MaterialTextureSet` role changes — so the checklist
/// invariant ("`.mat` paths land in named `MaterialTextureSet` roles,
/// never a CDB slot index") is enforced from before Phase 2 extraction
/// code exists, per the issue's own suggested fix.
///
/// This test is EXPECTED to need updating once Phase 2 lands: a real
/// CDB lookup should start populating specific `MaterialTextureSet`
/// roles (through this exact `merge_external_material` boundary, per
/// the issue's CANONICAL-BOUNDARY completeness check — never a
/// render-time fallback) and returning `MergeOutcome::Merged`. Until
/// then, both must stay exactly as asserted here.
#[test]
fn mat_path_forwards_no_texture_roles_until_cdb_phase_2_lands() {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut provider = MaterialProvider::new();
    provider.register_starfield_cdb(&minimal_cdb_bytes());
    assert!(provider.has_starfield_cdb());

    let mut mesh =
        imported_mesh_with_material_path(&mut pool, "materials/setpieces/reactor_core.mat");
    let outcome = merge_external_material(&mut mesh.material, &mut provider, &mut pool);

    assert_eq!(
        outcome,
        MergeOutcome::PresenceOnly,
        "#2359: the .mat arm resolves the sidecar but must not claim Merged \
         until it actually forwards CDB-authored data"
    );
    assert_eq!(
        mesh.material.textures,
        byroredux_nif::import::MaterialTextureSet::default(),
        "#2359: every MaterialTextureSet role must stay at its default — \
         Phase 1 forwards zero authored texture data from the CDB"
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
    let outcome = merge_external_material(&mut mesh.material, &mut provider, &mut pool);

    // Falls through past the .mat arm; bgsm/bgem dispatch fails
    // because the path doesn't match either suffix; `Unresolved`
    // (no archive to resolve from anyway).
    assert_eq!(
        outcome,
        MergeOutcome::Unresolved,
        "no CDB + no archives → no merge work"
    );
    assert!(!outcome.resolved());
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

/// #2705 / #3054 — `sf_cdb_cache()` is the process-lifetime cache that lets
/// `discover_starfield_cdbs` skip the ~105 MB zlib inflate on the second and
/// later provider rebuild for the same (archive source, CDB path) pair, while
/// retaining only the tiny header probe result. Exercises the cache primitive directly,
/// in the same spirit as this file's other `register_starfield_cdb`-direct
/// tests that bypass `Archive` entirely (the `bsa` crate has no synthetic
/// in-memory BA2 fixture builder to drive `discover_starfield_cdbs`'s own
/// `archive.list_files()` / `archive.extract()` calls end-to-end).
#[test]
fn sf_cdb_cache_returns_the_same_probe_and_stays_bounded() {
    let key = "sf_cdb_cache_test_key_2705_unique_marker".to_string();
    let other_key = "sf_cdb_cache_test_key_2705_never_inserted".to_string();
    let probe = byroredux_sfmaterial::ComponentDatabaseFile::probe_header(&minimal_cdb_bytes())
        .expect("minimal CDB probe");

    sf_cdb_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clear();

    assert!(
        sf_cdb_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .is_none(),
        "fresh key must miss before any insert"
    );

    sf_cdb_cache_insert(key.clone(), Some(probe));

    let cached = sf_cdb_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(&key)
        .cloned();
    assert_eq!(cached, Some(Some(probe)));

    assert!(
        sf_cdb_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&other_key)
            .is_none(),
        "a different (archive, path) key must still miss"
    );

    for i in 0..(SF_CDB_CACHE_MAX_ENTRIES + 8) {
        sf_cdb_cache_insert(format!("{key}-{i}"), Some(probe));
    }
    let cache = sf_cdb_cache().lock().unwrap_or_else(|e| e.into_inner());
    assert!(cache.len() <= SF_CDB_CACHE_MAX_ENTRIES);
}

/// A Starfield `.bgsm` reference has no external payload; with a CDB loaded it
/// must enter the Starfield capability arm and preserve PBR routing.
#[test]
fn mat_arm_does_not_steal_bgsm_dispatch() {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut provider = MaterialProvider::new();
    provider.register_starfield_cdb(&minimal_cdb_bytes());

    let mut mesh =
        imported_mesh_with_material_path(&mut pool, "materials/setdressing/metallocker01.bgsm");
    let _ = merge_external_material(&mut mesh.material, &mut provider, &mut pool);

    // Starfield NIFs use `.bgsm` names without shipping BGSM payloads. The
    // CDB capability gate routes them through Disney PBR instead of silently
    // falling into an unresolved external-file arm.
    assert!(mesh.material.is_pbr);
}

#[test]
fn starfield_bgem_named_reference_gets_pbr_fallback() {
    let mut pool = byroredux_core::string::StringPool::new();
    let mut provider = MaterialProvider::new();
    provider.register_starfield_cdb(&minimal_cdb_bytes());
    let mut mesh = imported_mesh_with_material_path(&mut pool, "materials/common/glowwhite.bgem");
    let outcome = merge_external_material(&mut mesh.material, &mut provider, &mut pool);
    assert_eq!(outcome, MergeOutcome::PresenceOnly);
    assert!(mesh.material.is_pbr);
}
