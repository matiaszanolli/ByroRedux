//! Starfield `materialsbeta.cdb` discovery, probing and fallback (#3857,
//! split from `asset_provider/material.rs`).
//!
//! Starfield ships its materials in one multi-gigabyte component database
//! rather than per-material sidecars, so the lookup path is entirely
//! different from the BGSM/BGEM one: discover the archive, probe its header,
//! memoise the answer, and — until the full parse lands — apply a PBR
//! fallback so `.mat` materials are not silently untranslated.

use super::*;

use byroredux_nif::import::ImportedMaterial;
use byroredux_sfmaterial::{CdbHeaderInfo, ComponentDatabaseFile};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// True for a Starfield component-database path — the base
/// `materials\materialsbeta.cdb` or any DLC/Creation-namespaced
/// `materials\creations\<plugin>\materialsbeta.cdb`. #1571 / SF-D3-03.
pub(crate) fn is_materialsbeta_cdb_path(path: &str) -> bool {
    let p = path.replace('/', "\\").to_ascii_lowercase();
    p.starts_with("materials\\") && p.ends_with("materialsbeta.cdb")
}

/// Process-lifetime cache of Starfield CDB probe results, keyed by
/// `"<archive source>|<in-archive path>"`. #2705 (SF-D3-01) —
/// `build_material_provider` constructs a brand-new `MaterialProvider` (and
/// therefore a brand-new, empty `csg_cache`-shaped per-instance cache) on
/// every cell transition / save-load / debug-load, so caching *inside*
/// `MaterialProvider` wouldn't help here: the same CDB would still get
/// `archive.extract()`'d — a full zlib inflate of a multi-hundred-MB blob
/// for the vanilla `materialsbeta.cdb` (105 MB measured) — on every single
/// rebuild, even though `probe_starfield_cdb` only reads the file's
/// 16-byte header and on-disk content never changes mid-session. Living at
/// module scope (outside `MaterialProvider`) lets this survive across
/// provider rebuilds instead of being discarded with the provider — the
/// Phase 1 only needs the header validity/count; retaining inflated bytes here
/// held every discovered CDB (233 MB across a Creation-heavy install) for the
/// process lifetime without a consumer. The cache stores that tiny result
/// instead, preserving #2705's skip-reextract behavior without the resident
/// blob. A cap keeps untrusted/modded archive sets from growing keys forever.
pub(crate) const SF_CDB_CACHE_MAX_ENTRIES: usize = 128;

/// Every caller takes this lock as `.unwrap_or_else(|e| e.into_inner())`,
/// recovering rather than re-panicking on poison (#2398 — the deliberate
/// counterpart to the ECS layer's #466 fail-fast doctrine). The map is a pure
/// memoization of a re-derivable archive probe: the worst a torn entry can do
/// is hand back a header probe that gets re-read from the archive, and a
/// panicking material load must not permanently poison texture resolution for
/// every later mesh in the cell.
pub(crate) fn sf_cdb_cache() -> &'static Mutex<HashMap<String, Option<CdbHeaderInfo>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<CdbHeaderInfo>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn sf_cdb_cache_insert(key: String, probe: Option<CdbHeaderInfo>) {
    let mut cache = sf_cdb_cache().lock().unwrap_or_else(|e| e.into_inner());
    if !cache.contains_key(&key) && cache.len() >= SF_CDB_CACHE_MAX_ENTRIES {
        if let Some(evicted) = cache.keys().next().cloned() {
            cache.remove(&evicted);
        }
    }
    cache.insert(key, probe);
}

/// Scan one archive for Starfield component databases and load each into
/// `provider` in archive order. #1571 / SF-D3-03 — the base game ships
/// `materials\materialsbeta.cdb` in `Starfield - Materials.ba2`, but each
/// DLC / Creation ships its own at `materials\creations\<plugin>\…` inside
/// its `* - Main.ba2`, so a hardcoded single-path extract misses them.
pub(crate) fn discover_starfield_cdbs(
    archive: &Archive,
    source: &str,
    provider: &mut MaterialProvider,
) {
    // Collect the matching paths first so the immutable `list_files`
    // borrow is released before the mutable `provider` borrow per extract.
    let cdb_paths: Vec<String> = archive
        .list_files()
        .into_iter()
        .filter(|p| is_materialsbeta_cdb_path(p))
        .map(|p| p.to_owned())
        .collect();
    for path in cdb_paths {
        let cache_key = format!("{source}|{path}");
        let cached = sf_cdb_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&cache_key)
            .copied();
        let probe = match cached {
            Some(probe) => {
                log::info!(
                    "Discovered Starfield CDB '{path}' in '{source}' (cached header probe, \
                     skipped re-extract)"
                );
                probe
            }
            None => match archive.extract(&path) {
                Ok(raw) => {
                    log::info!(
                        "Discovered Starfield CDB '{path}' in '{source}' ({} bytes, extracted)",
                        raw.len()
                    );
                    let probe = probe_starfield_cdb(&raw, &path);
                    sf_cdb_cache_insert(cache_key, probe);
                    probe
                }
                Err(e) => {
                    log::warn!("Failed to extract CDB '{path}' from '{source}': {e}");
                    None
                }
            },
        };
        if let Some(info) = probe {
            provider.register_starfield_cdb_probe(info);
        }
    }
}

/// Header-only validity probe for a Starfield `materialsbeta.cdb` payload.
///
/// Does a 4-byte `BETH` magic reject first (SF-D3-AUDIT-03 / #2102) — the
/// cheapest way to skip header/chunk-index work for a mis-named non-CDB file
/// — then a [`ComponentDatabaseFile::probe_header`] validity check
/// (SF-D3-AUDIT-01 / #2100). Does NOT walk or retain the ~1.44M-entry
/// instance tree (see the `sf_cdb_count` field doc). A malformed payload is
/// warned and dropped.
///
/// #3889 — this is the single registration path. It used to exist twice: an
/// unreachable `MaterialProvider::register_starfield_cdb` that the eight
/// `starfield_mat.rs` tests called, and the inline `probe_header(&raw).ok()`
/// in `discover_starfield_cdbs` that production actually ran. The tests
/// therefore validated a parallel copy, and the `peek_magic` fast reject
/// lived only in the copy production never executed.
pub(crate) fn probe_starfield_cdb(bytes: &[u8], source: &str) -> Option<CdbHeaderInfo> {
    if !ComponentDatabaseFile::peek_magic(bytes) {
        log::warn!(
            "Starfield CDB '{source}' rejected ({} bytes): not a BETH-signature file. \
             Starfield content will fall back to legacy Lambert shading.",
            bytes.len(),
        );
        return None;
    }
    match ComponentDatabaseFile::probe_header(bytes) {
        Ok(info) => {
            log::info!(
                "Starfield CDB '{source}' present: {} chunks ({} bytes, header-only probe). \
                 `.mat` material paths on NIFs will route through Disney BSDF \
                 (Phase 1 — full parse + per-field extraction is the deferred \
                 Phase 2 follow-up).",
                info.chunk_count,
                bytes.len(),
            );
            Some(info)
        }
        Err(e) => {
            log::warn!(
                "Starfield CDB '{source}' header invalid ({} bytes): {e}. \
                 Starfield content will fall back to legacy Lambert shading.",
                bytes.len(),
            );
            None
        }
    }
}

/// Every archive path the `--bsa` CDB-discovery arm of
/// [`build_material_provider`] should scan for a given explicitly-named
/// archive: the primary path plus any numeric-suffixed sibling that
/// actually exists. `exists` is injected (rather than calling
/// `std::path::Path::is_file` directly) so this stays unit-testable
/// without touching the real filesystem or fabricating a real BA2 file —
/// same reasoning as `sniff_magic_from`'s injected `Read` (#2615). #2621
/// / SF-D3-04.
pub(crate) fn cdb_scan_candidates(primary: &str, exists: impl Fn(&str) -> bool) -> Vec<String> {
    let mut paths = vec![primary.to_string()];
    paths.extend(
        numeric_sibling_paths(primary)
            .into_iter()
            .filter(|p| exists(p)),
    );
    paths
}

/// SF3-02 / #1831 — chooses the diagnostic message for a material path
/// that fell through to the unknown-format arm of [`merge_external_material`].
/// A `.mat` path only reaches that arm when no Starfield CDB is loaded
/// (the CDB-presence gate short-circuits it otherwise), which is a
/// distinct, more actionable cause than "unrecognised extension" — name
/// it explicitly so it doesn't read as generic per-mesh spam disconnected
/// from the CDB load failure logged far earlier.
pub(crate) fn unresolved_material_warning(path: &str, has_starfield_cdb: bool) -> String {
    if path.ends_with(".mat") && !has_starfield_cdb {
        format!(
            "material path '{path}' is a Starfield .mat but no CDB is loaded/parsed \
             — check --materials-ba2 and CDB version; mesh will use NIF defaults"
        )
    } else {
        format!(
            "material path '{path}' is not a .bgsm/.bgem — unsupported format (Starfield .mat?); mesh will use NIF defaults"
        )
    }
}

/// The CDB-gated PBR flip — Starfield's fallback when no authored sidecar
/// payload is available for a material path.
///
/// #3230 (SF-2026-08-20-D9-01) — this used to be an unconditional early
/// return at the top of [`merge_external_material`], which meant that on any
/// session with a Starfield CDB registered, a `.bgsm`/`.bgem` path that
/// *did* resolve to a real file was never parsed: the resolvers sit further
/// down the function and the return preceded them. Now there are two
/// distinct entry points:
///
/// * The `.mat` arm calls it **directly**, and still early-returns. That is
///   correct and not a shortcut: Starfield ships no `.mat` sidecar files at
///   all (a census of all 129 vanilla + Creation archives found zero loose
///   `.bgsm`/`.bgem` and only 20 `.mat`, all third-party), so there is no
///   resolver for a `.mat` path to miss.
/// * `.bgsm`/`.bgem` names reach it **only after** `resolve_bgsm` /
///   `resolve_bgem` has actually missed.
///
/// Keeping the fallback for those names (rather than returning
/// `Unresolved`) is deliberate, and measured: the CDB key's extension
/// column is the literal constant `"mat"`, so a CDB lookup ignores the
/// reference's own suffix and `.bgsm`/`.bgem`-named Starfield paths really
/// do resolve to CDB materials — 17 of 57 in the sampled corpus. See
/// `docs/audits/SF_CDB_PHASE2_SPIKE_2026-08-29.md` §1. The CDB is the right
/// destination on a sidecar miss, not a consolation prize.
pub(super) fn apply_cdb_pbr_fallback(material: &mut ImportedMaterial, path: &str) -> MergeOutcome {
    material.is_pbr = true;
    // `from_bgsm` deliberately NOT set — that flag gates BGSM
    // spec-glossiness translation (an FO4-specific format convention).
    if !path.ends_with(".mat") {
        static WARNED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
            std::sync::OnceLock::new();
        let mut warned = WARNED
            .get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if warned.insert(path.to_owned()) {
            // #3230 — the message can now state the miss as fact. Pre-fix it
            // claimed "has no external BGSM/BGEM payload" without ever having
            // looked, which was actively wrong for the case this fix restores.
            log::warn!(
                "Starfield shader material '{}': no BGSM/BGEM sidecar resolved; \
                 falling back to CDB-gated PBR routing",
                path
            );
        }
    }
    MergeOutcome::PresenceOnly
}
