//! Starfield `materialsbeta.cdb` discovery, probing and fallback (#3857,
//! split from `asset_provider/material.rs`).
//!
//! Starfield ships its materials in one multi-gigabyte component database
//! rather than per-material sidecars, so the lookup path is entirely
//! different from the BGSM/BGEM one: discover the archive, probe its header,
//! memoise the answer, and — on a lookup miss — apply a PBR fallback so
//! `.mat` materials are not silently untranslated. The per-material lookup
//! itself uses the `MaterialIndex` built in the background at discovery
//! when possible; a lookup builds it or waits on the same single-flight slot
//! if it arrives first (see below).

use super::*;

use byroredux_nif::import::ImportedMaterial;
use byroredux_sfmaterial::{CdbHeaderInfo, CdbMaterial, ComponentDatabaseFile, MaterialIndex};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// At most one CDB index build may hold its inflated archive payload at once.
static CDB_BUILD_GATE: Mutex<()> = Mutex::new(());

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
/// process lifetime without a consumer. The cache stores only whether the
/// probe succeeded (#4386 — registration reads nothing else from the header),
/// preserving #2705's skip-reextract behavior without the resident blob. A cap
/// keeps untrusted/modded archive sets from growing keys forever.
pub(crate) const SF_CDB_CACHE_MAX_ENTRIES: usize = 128;

/// Every caller takes this lock as `.unwrap_or_else(|e| e.into_inner())`,
/// recovering rather than re-panicking on poison (#2398 — the deliberate
/// counterpart to the ECS layer's #466 fail-fast doctrine). The map is a pure
/// memoization of a re-derivable archive probe: the worst a torn entry can do
/// is hand back a header probe that gets re-read from the archive, and a
/// panicking material load must not permanently poison texture resolution for
/// every later mesh in the cell.
pub(crate) fn sf_cdb_cache() -> &'static Mutex<HashMap<String, bool>> {
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Memoise whether one (archive source, CDB path) pair probed as a valid
/// component database. Only validity is kept: registration counts a CDB and
/// reads nothing else from its header (#4386).
pub(crate) fn sf_cdb_cache_insert(key: String, valid: bool) {
    let mut cache = sf_cdb_cache().lock().unwrap_or_else(|e| e.into_inner());
    if !cache.contains_key(&key) && cache.len() >= SF_CDB_CACHE_MAX_ENTRIES {
        if let Some(evicted) = cache.keys().next().cloned() {
            cache.remove(&evicted);
        }
    }
    cache.insert(key, valid);
}

/// One CDB's index slot: single-flight (#5291). `OnceLock::get_or_init`
/// runs the build exactly once per slot and parks every other caller on the
/// same slot until it finishes, so a background prefetch and a main-thread
/// lookup racing for one CDB share one ~2 s / ~250 MB-peak build instead of
/// running two. `Some(None)` memoises a failed build.
pub(crate) type CdbIndexSlot = Arc<OnceLock<Option<Arc<MaterialIndex>>>>;

/// #3398 Phase 2 — process-lifetime cache of BUILT material indexes,
/// keyed by the same `"<archive source>|<in-archive path>"` as
/// [`sf_cdb_cache`] so it survives the `build_material_provider`
/// rebuilds on every cell transition / save-load / debug-load (the
/// #2705 rationale). A slot holding `None` memoises a failed build, so
/// a corrupt CDB is re-attempted at most once per process rather than
/// once per mesh.
///
/// Memory (re-measured 2026-10-11, #5291, `real_cdb` gate, release):
/// building the vanilla base CDB peaks at ~247 MB RSS (HWM, inflated blob
/// included) in 1.6 s, and the retained index is ~115 MB resident once the
/// blob is dropped — versus 9.19 GB for the full generic parse (the
/// "~470 MB" 224a19372 recorded is stale). DLC CDBs are near-copies of the
/// base. Every discovered CDB ends up built: lookups walk sources
/// last-registered-first, so a base-only material misses each DLC index
/// before it hits the base. See `docs/engine/memory-budget.md`.
pub(crate) fn sf_cdb_index_cache() -> &'static Mutex<HashMap<String, CdbIndexSlot>> {
    static CACHE: OnceLock<Mutex<HashMap<String, CdbIndexSlot>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The slot for `key`, creating (and, at the cap, evicting to make room for)
/// it when absent. The returned `Arc` is the caller's own handle: an eviction
/// racing a build cannot lose the build's result for that caller.
fn cdb_index_slot(key: &str) -> CdbIndexSlot {
    cdb_index_slot_with_inserted(key).0
}

/// Return the shared slot and whether this call installed it.
fn cdb_index_slot_with_inserted(key: &str) -> (CdbIndexSlot, bool) {
    let mut cache = sf_cdb_index_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if let Some(slot) = cache.get(key) {
        return (Arc::clone(slot), false);
    }
    if cache.len() >= SF_CDB_CACHE_MAX_ENTRIES {
        if let Some(evicted) = cache.keys().next().cloned() {
            cache.remove(&evicted);
        }
    }
    let slot = CdbIndexSlot::default();
    cache.insert(key.to_owned(), Arc::clone(&slot));
    (slot, true)
}

/// Test-only direct injection of a prebuilt index under a cache key —
/// the merge-arm tests exercise the production lookup path without an
/// on-disk archive. Replaces any slot already there (a parallel test's
/// failed-build memoisation must not shadow the injected index).
#[cfg(test)]
pub(crate) fn sf_cdb_index_cache_insert_for_test(key: &str, index: Arc<MaterialIndex>) {
    let slot = CdbIndexSlot::default();
    let _ = slot.set(Some(index));
    sf_cdb_index_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(key.to_owned(), slot);
}

/// Build (or fetch) the index for one CDB, memoising failures. The
/// source archive is re-opened and the payload re-extracted here —
/// discovery deliberately retains nothing but the header verdict
/// (#4386), and the build is paid once per CDB per process: by the
/// background prefetch [`discover_starfield_cdbs`] starts (#5291) when it
/// gets there first, otherwise by the first lookup that reaches this CDB.
fn cdb_material_index(source: &str, inner: &str) -> Option<Arc<MaterialIndex>> {
    let key = format!("{source}|{inner}");
    cdb_index_slot(&key)
        .get_or_init(|| build_cdb_material_index_serialized(source, inner))
        .clone()
}

fn build_cdb_material_index_serialized(source: &str, inner: &str) -> Option<Arc<MaterialIndex>> {
    let _gate = CDB_BUILD_GATE.lock().unwrap_or_else(|e| e.into_inner());
    contain_cdb_build_panic(source, inner, || build_cdb_material_index(source, inner))
}

/// #5665 — run one CDB index build, turning a panic into the same warned `None`
/// every other build failure already is.
///
/// A `OnceLock` whose initializer panics is left **uninitialised**, so the slot
/// that exists to memoise "this CDB cannot be built" memoised nothing for a
/// panic: the `sf-cdb-index` prefetch thread died with the slot still empty, and
/// the first lookup that reached the CDB re-ran the whole extract + build on the
/// calling thread (a streaming apply slice that `FrameTimeBudget` cannot
/// preempt), panicking again, once per lookup. Containing the panic inside the
/// initializer makes it a result the slot keeps, so a malformed CDB costs one
/// build, one warning and a PBR fallback — the outcome of its sibling failures.
///
/// `build` runs with the build gate held by the caller; catching here, before
/// the unwind can cross the guard, also keeps the gate from being poisoned.
fn contain_cdb_build_panic(
    source: &str,
    inner: &str,
    build: impl FnOnce() -> Option<Arc<MaterialIndex>>,
) -> Option<Arc<MaterialIndex>> {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(build)) {
        Ok(index) => index,
        Err(payload) => {
            let detail = payload
                .downcast_ref::<&str>()
                .map(|s| (*s).to_owned())
                .or_else(|| payload.downcast_ref::<String>().cloned())
                .unwrap_or_else(|| "non-string panic payload".to_owned());
            log::warn!(
                "Starfield CDB '{inner}' in '{source}': material index build \
                 panicked ({detail}) — .mat lookups fall back to PBR routing only"
            );
            None
        }
    }
}

/// The un-memoised build behind [`cdb_material_index`]'s slot.
fn build_cdb_material_index(source: &str, inner: &str) -> Option<Arc<MaterialIndex>> {
    // #5319 (PAR-D2-2026-10-05-01) — this was the raw `.ok()?` shape
    // #4658 removed from the providers: open/extract failures hit
    // neither warn arm, so a CDB whose archive could not be
    // re-opened (or whose entry failed to re-extract) memoized None
    // for the process with no log line — indistinguishable from
    // "this material is not in any CDB". Match, warn with the
    // source/path/error, then memoize.
    let archive = match Archive::open_shared(source) {
        Ok((archive, _)) => archive,
        Err(e) => {
            log::warn!(
                "Starfield CDB '{inner}' in '{source}': archive open \
                 failed ({e}) — .mat lookups fall back to PBR routing only"
            );
            return None;
        }
    };
    let bytes = match archive.extract(inner) {
        Ok(bytes) => bytes,
        Err(e) => {
            log::warn!(
                "Starfield CDB '{inner}' in '{source}': extract \
                 failed ({e}) — .mat lookups fall back to PBR routing only"
            );
            return None;
        }
    };
    match MaterialIndex::build(&bytes) {
        Ok(index) => {
            log::info!(
                "Starfield CDB '{inner}' in '{source}': material index built \
                 ({} keyed objects)",
                index.material_count()
            );
            Some(Arc::new(index))
        }
        Err(e) => {
            log::warn!(
                "Starfield CDB '{inner}' in '{source}': material index build \
                 failed ({e}) — .mat lookups fall back to PBR routing only"
            );
            None
        }
    }
}

/// #5291 — start building one discovered CDB's index off the main thread, so
/// the ~2 s build overlaps the ESM parse / archive opens that follow
/// discovery instead of landing inside the first `.mat` merge (or, worse, a
/// streamed apply slice that `FrameTimeBudget` cannot preempt).
///
/// The slot is installed synchronously before the worker is queued. Any
/// lookup that reaches a still-queued CDB therefore parks on its slot, and
/// the shared build gate inside the slot initializer serialises both
/// prefetch and lazy builds. Each build holds the ~105 MB inflated blob plus
/// the index, so N discovered CDBs must not peak at N× that at once. A slot
/// that already exists (built, building, or memoised failed) is left alone.
fn prefetch_cdb_material_index(source: &str, inner: &str) {
    let key = format!("{source}|{inner}");
    let (slot, inserted) = cdb_index_slot_with_inserted(&key);
    if !inserted {
        return;
    }
    let (source, inner) = (source.to_owned(), inner.to_owned());
    let spawned = std::thread::Builder::new()
        .name("sf-cdb-index".to_owned())
        .spawn(move || {
            let _ = slot.get_or_init(|| build_cdb_material_index_serialized(&source, &inner));
        });
    if let Err(e) = spawned {
        // The lookup path still builds lazily; prefetch is only an overlap.
        log::warn!("Starfield CDB index prefetch thread not started ({e}); will build lazily");
    }
}

/// #3398 Phase 2 — resolve a material path against every discovered
/// CDB, last-registered wins (matching the archive-precedence
/// convention in this module: the DLC/Creation CDB overlays the base).
/// `None` = not in any CDB.
pub(crate) fn lookup_cdb_material(
    sources: &[(String, String)],
    path: &str,
) -> Option<CdbMaterial> {
    for (source, inner) in sources.iter().rev() {
        if let Some(index) = cdb_material_index(source, inner) {
            if let Some(mat) = index.lookup(path) {
                return Some(mat);
            }
        }
    }
    None
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
    // Copy the matching paths out first so the borrow of the archive's
    // memoised scan (#5288) is released before the mutable `provider` borrow
    // per extract.
    for path in archive.cdb_paths().to_vec() {
        let cache_key = format!("{source}|{path}");
        let cached = sf_cdb_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&cache_key)
            .copied();
        let valid = match cached {
            Some(valid) => {
                log::info!(
                    "Discovered Starfield CDB '{path}' in '{source}' (cached header probe, \
                     skipped re-extract)"
                );
                valid
            }
            None => match archive.extract(&path) {
                Ok(raw) => {
                    log::info!(
                        "Discovered Starfield CDB '{path}' in '{source}' ({} bytes, extracted)",
                        raw.len()
                    );
                    let valid = probe_starfield_cdb(&raw, &path).is_some();
                    sf_cdb_cache_insert(cache_key, valid);
                    valid
                }
                Err(e) => {
                    log::warn!("Failed to extract CDB '{path}' from '{source}': {e}");
                    false
                }
            },
        };
        if valid {
            provider.register_starfield_cdb_probe(source.to_owned(), path.clone());
            prefetch_cdb_material_index(source, &path);
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
                 (the index is built in the background at discovery; a lookup \
                 that arrives first builds it or waits on the same build).",
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
///   correct and not a shortcut: Starfield ships no `.bgsm`/`.bgem` sidecar
///   files at all, and the only loose `.mat` files are 20 third-party
///   Creation JSON exports (a census of all 129 vanilla + Creation
///   archives), which the arm decodes through `loose_mat` (#4277) before
///   reaching this fallback; a `.mat` path with no decodable file resolves
///   through the CDB, so nothing remains for a resolver to miss.
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
    // #4283 — but the CDB IS a resolved external material description, so
    // the format-agnostic provenance the glass classifier reads is set:
    // Starfield effect-shader glass takes the dielectric path FO4's
    // identical authoring does, instead of being locked out by the FO4
    // convention the flag now exclusively carries.
    material.external_material_resolved = true;
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #5665 — a panicking build is memoised like any other failed one: the
    /// slot keeps `None`, the build ran exactly once, and a second lookup does
    /// not run it again. (A bare `OnceLock` is left empty by a panicking
    /// initializer, so without the containment every lookup re-ran the build.)
    #[test]
    fn a_panicking_cdb_build_is_memoised_and_runs_once() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        let builds = AtomicUsize::new(0);
        let slot = CdbIndexSlot::default();
        for _ in 0..3 {
            let got = slot
                .get_or_init(|| {
                    contain_cdb_build_panic("test-source", "test.cdb", || {
                        builds.fetch_add(1, Ordering::SeqCst);
                        panic!("malformed CDB chunk")
                    })
                })
                .clone();
            assert!(got.is_none(), "a panic resolves to the PBR fallback, not an index");
        }
        assert_eq!(
            builds.load(Ordering::SeqCst),
            1,
            "the failed build must be memoised, not retried per lookup"
        );
        assert!(
            matches!(slot.get(), Some(None)),
            "the slot holds the memoised failure"
        );
    }

    /// #5665 — containment passes an ordinary `None` through unchanged and turns
    /// a panic into `None` rather than unwinding into the caller (the prefetch
    /// thread, or a lookup on the main thread).
    #[test]
    fn a_contained_build_never_unwinds_into_its_caller() {
        assert!(contain_cdb_build_panic("s", "i", || None).is_none());
        assert!(contain_cdb_build_panic("s", "i", || panic!("boom")).is_none());
        assert!(contain_cdb_build_panic("s", "i", || panic!("{}", String::from("owned"))).is_none());
    }

    /// #5319 (PAR-D2-2026-10-05-01) — the lazy build's open/extract arms
    /// used to be raw `.ok()?` (the shape #4658 removed elsewhere): a CDB
    /// whose archive cannot be re-opened memoized `None` for the process
    /// with NO log line, indistinguishable from "not in any CDB". The
    /// arms now warn (source, path, error) before memoizing; this test
    /// pins the memoized-failure contract those arms feed — a failed
    /// open resolves to None, is memoized under the source|inner key,
    /// and stays None on the cached second call. (Log emission itself is
    /// not assertable here without hijacking the process-global logger.)
    #[test]
    fn unfetchable_cdb_resolves_none_and_memoizes_the_failure() {
        let source = "byroredux-cdb-test:definitely-not-on-disk.ba2";
        let inner = "materials\\materialsbeta.cdb";
        let key = format!("{source}|{inner}");

        // Whatever a parallel test left behind — start from a clean slot.
        sf_cdb_index_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);

        assert!(
            cdb_material_index(source, inner).is_none(),
            "an archive that cannot be opened must resolve to None"
        );
        let cached = sf_cdb_index_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .and_then(|slot| slot.get().cloned());
        assert!(
            matches!(cached, Some(None)),
            "the failed build must be memoized as None, got {cached:?}"
        );
        assert!(
            cdb_material_index(source, inner).is_none(),
            "the memoized failure must answer the second lookup"
        );

        // Leave no stale slot for parallel tests to trip over.
        sf_cdb_index_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
    }

    /// #5291 — discovery starts the index build off the calling thread and
    /// publishes it through the slot the lookup path reads: the slot fills
    /// without any lookup having asked (an unopenable archive is the
    /// cheapest build that still runs the whole open → memoise path), and
    /// the later lookup answers from it.
    #[test]
    fn prefetch_fills_the_lookup_slot_without_a_lookup() {
        let source = "byroredux-cdb-test:prefetch-not-on-disk.ba2";
        let inner = "materials\\materialsbeta.cdb";
        let key = format!("{source}|{inner}");
        sf_cdb_index_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);

        prefetch_cdb_material_index(source, inner);

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let filled = loop {
            let slot = sf_cdb_index_cache()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&key)
                .cloned();
            if let Some(result) = slot.and_then(|slot| slot.get().cloned()) {
                break Some(result);
            }
            if std::time::Instant::now() > deadline {
                break None;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        };
        assert!(
            matches!(filled, Some(None)),
            "the background build must publish its (here failed) result, got {filled:?}"
        );
        assert!(cdb_material_index(source, inner).is_none());

        sf_cdb_index_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
    }

    /// A queued prefetch owns its single-flight slot before its worker can
    /// take the shared build gate, so a lookup waits instead of building
    /// outside the gate on the calling thread.
    #[test]
    fn lookup_waits_for_a_queued_prefetch_slot() {
        let gate = CDB_BUILD_GATE.lock().unwrap_or_else(|e| e.into_inner());
        let source = "byroredux-cdb-test:queued-prefetch.ba2";
        let inner = "materials\\materialsbeta.cdb";
        let key = format!("{source}|{inner}");
        sf_cdb_index_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);

        prefetch_cdb_material_index(source, inner);
        let slot = sf_cdb_index_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .cloned()
            .expect("prefetch installs its slot before queuing the worker");
        assert!(
            slot.get().is_none(),
            "the held gate keeps the queued slot empty"
        );

        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let lookup = std::thread::spawn(move || {
            started_tx.send(()).unwrap();
            result_tx
                .send(cdb_material_index(source, inner).is_none())
                .unwrap();
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("lookup thread started");
        assert!(
            matches!(
                result_rx.recv_timeout(std::time::Duration::from_millis(250)),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout)
            ),
            "lookup must park on the queued slot until the serialized build can run"
        );

        drop(gate);
        assert!(result_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .expect("the queued CDB build completed"));
        lookup.join().unwrap();
        sf_cdb_index_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
    }

    /// #5291 — a slot that already exists (built, building, or injected) is
    /// left alone by the prefetch: it must not start a second ~2 s build, and
    /// a failed one must never clobber a good index.
    #[test]
    fn prefetch_leaves_an_existing_slot_alone() {
        let source = "byroredux-cdb-test:prefetch-existing.ba2";
        let inner = "materials\\materialsbeta.cdb";
        let key = format!("{source}|{inner}");
        let slot = cdb_index_slot(&key);
        assert!(slot.get().is_none(), "fixture: a fresh, unbuilt slot");

        prefetch_cdb_material_index(source, inner);
        // The unopenable archive would memoise None within milliseconds if a
        // build had been started; give it ample time to show.
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(
            slot.get().is_none(),
            "prefetch started a build for a slot that already existed"
        );

        sf_cdb_index_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);
    }

    /// #5291 real-data gate: the background prefetch builds the vanilla
    /// base CDB's index and a later lookup resolves the probe-verified
    /// material from it. Run with `-- --ignored`.
    #[test]
    #[ignore = "needs Starfield game data on disk"]
    fn prefetch_builds_the_vanilla_index_for_a_later_lookup() {
        let data = std::env::var("BYROREDUX_STARFIELD_DATA").unwrap_or_else(|_| {
            "/mnt/data/SteamLibrary/steamapps/common/Starfield/Data".into()
        });
        let source = std::path::Path::new(&data)
            .join("Starfield - Materials.ba2")
            .to_string_lossy()
            .into_owned();
        let inner = "materials\\materialsbeta.cdb";
        let key = format!("{source}|{inner}");
        sf_cdb_index_cache()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key);

        let started = std::time::Instant::now();
        prefetch_cdb_material_index(&source, inner);
        // The prefetch returns immediately; the build is on its own thread.
        assert!(
            started.elapsed() < std::time::Duration::from_millis(500),
            "prefetch must not build on the calling thread"
        );
        let found = lookup_cdb_material(
            &[(source.clone(), inner.to_owned())],
            "Materials\\Actors\\BipedA\\BipedA_Nightmare\\BipedA_Nightmare.mat",
        );
        assert!(found.is_some(), "the prefetched index must resolve the probe material");
        assert!(
            sf_cdb_index_cache()
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(&key)
                .and_then(|slot| slot.get().cloned())
                .is_some_and(|built| built.is_some()),
            "the slot holds the built index"
        );
    }
}
