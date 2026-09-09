//! `MaterialProvider` — archive-backed sidecar lookup and its caches
//! (#3857, split from `asset_provider/material.rs`).
//!
//! Resolves a material path to a parsed `BgsmFile`/`BgemFile` by searching
//! the loaded archives, memoising both hits and misses. Four bounded LRU
//! caches live here; all four evict through [`half_evict`].

use super::*;

use byroredux_bgsm::template::ResolvedMaterial;
use byroredux_bgsm::{BgemFile, TemplateCache, TemplateResolver};
use byroredux_sfmaterial::CdbHeaderInfo;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// Build a MaterialProvider from CLI arguments. Accepts repeated
/// `--materials-ba2 <path>` flags so a user can layer modded materials
/// on top of the vanilla `Fallout4 - Materials.ba2`. Silently returns
/// an empty provider when no flags are present — the merge helper
/// short-circuits when called on a mesh whose `material_path` can't
/// resolve anywhere.
pub(crate) fn build_material_provider(args: &[String]) -> MaterialProvider {
    let mut provider = MaterialProvider::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--materials-ba2" => {
                if let Some(path) = args.get(i + 1) {
                    match Archive::open(path) {
                        Ok(a) => {
                            // #3637 — shadow-count diagnostic shared with
                            // TextureProvider's --bsa/--textures-bsa opens.
                            log_opened_archive("materials", path, &a, &provider.archives);
                            // #1289 / SF-D3-NEW-01 → #1571 / SF-D3-03 —
                            // scan the archive for every Starfield component
                            // database (base `materials\materialsbeta.cdb`
                            // plus any DLC/Creation-namespaced CDB) instead
                            // of extracting one hardcoded path. Non-Starfield
                            // archives (FO4's `Fallout4 - Materials.ba2`)
                            // ship none, so the scan is a no-op there.
                            discover_starfield_cdbs(&a, path, &mut provider);
                            provider.push_archive(a);
                        }
                        Err(e) => log::warn!("Failed to open materials archive: {}", e),
                    }
                    i += 2;
                    continue;
                }
            }
            // #1571 / SF-D3-03 — DLC / Creation CDBs ship inside the
            // `* - Main.ba2` MESH archives (passed via `--bsa`), at
            // `materials\creations\<plugin>\materialsbeta.cdb` — never the
            // base path and never `--materials-ba2`. Scan those for CDBs
            // too, but do NOT push them as material archives: they're mesh
            // archives owned by the TextureProvider. Each archive is
            // re-opened here purely to read its file table (the entry data
            // isn't touched) and dropped after the scan.
            //
            // #2621 / SF-D3-04 — the texture provider covers Starfield's
            // zero-padded numeric-sibling series (`Meshes01.ba2` →
            // `Meshes02..09.ba2`, via `open_with_numeric_siblings`); this
            // arm didn't, so a DLC/Creation CDB shipped in a sibling
            // archive (rather than the one explicitly named on the command
            // line) was never scanned — #1571's original failure mode
            // reappearing one level up, at archive selection instead of
            // path selection. `cdb_scan_candidates` reuses
            // `numeric_sibling_paths` (the same pure candidate-list logic
            // `open_with_numeric_siblings` is built on) rather than that
            // helper itself, since each sibling's own path — not the
            // primary's — needs to reach `discover_starfield_cdbs` as
            // `source`, for correct per-archive cache-key and log
            // attribution.
            //
            // Not fixed here (documented, LOW, same site per the issue): a
            // loose `Data\materials\materialsbeta.cdb` — the natural
            // mod-override shape — is never discovered by any path in this
            // function; every source scanned here is an archive.
            "--bsa" => {
                if let Some(path) = args.get(i + 1) {
                    for candidate in
                        cdb_scan_candidates(path, |p| std::path::Path::new(p).is_file())
                    {
                        match Archive::open(&candidate) {
                            Ok(a) => discover_starfield_cdbs(&a, &candidate, &mut provider),
                            Err(e) => log::warn!(
                                "Failed to open '{}' for CDB discovery: {}",
                                candidate,
                                e
                            ),
                        }
                    }
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    provider
}

/// BGSM/BGEM material file resolver backed by Materials BA2 archives.
///
/// FO4+ authors materials as external .bgsm / .bgem files, referenced by
/// `BSLightingShaderProperty.net.name` (lit) or
/// `BSEffectShaderProperty.net.name` (effect). The NIF side captures the
/// path into `ImportedMesh.material.material_path`; this provider opens the
/// files out of `Fallout4 - Materials.ba2` (or equivalent) and hands back
/// the parsed + template-resolved chain. The LRU is owned by `bgsm`'s
/// [`TemplateCache`] so integration doesn't reinvent chain-walking.
///
/// Parse failures are logged once per path and return `None` — callers
/// must tolerate absence and keep the NIF defaults. Never hard-fail a
/// cell load on a broken BGSM. See #493.
pub(crate) struct MaterialProvider {
    pub(crate) archives: Vec<Archive>,
    /// BGSM chain cache from the `bgsm` crate — handles template
    /// inheritance with case-insensitive keying + LRU eviction.
    bgsm_cache: TemplateCache,
    /// BGEM has no template inheritance (the format carries no
    /// `root_material_path`), so we cache parsed files directly by path.
    /// #951 / SAFE-26 / #1430: bounded at `MAX_BGEM_CACHE_ENTRIES`. On
    /// overflow the oldest N/2 entries are evicted so the recent
    /// working-set stays resident (half-eviction by insertion order).
    bgem_cache: HashMap<String, Arc<BgemFile>>,
    /// Insertion-order key tracker for [`bgem_cache`] — drives half-eviction.
    bgem_cache_order: VecDeque<String>,
    /// Paths we've already warned about so a broken file doesn't spam
    /// the log on every cell load. Bounded by `MAX_FAILED_PATHS`.
    /// #1430: evicts oldest N/2 entries on overflow (same pattern as bgem_cache).
    pub(crate) failed_paths: HashSet<String>,
    /// Insertion-order key tracker for [`failed_paths`] — drives half-eviction.
    failed_paths_order: VecDeque<String>,
    /// #3899 — memoised `BGSM`-vs-`BGEM` magic per normalised path, for paths
    /// no material cache can answer for (not yet resolved, or resolved and
    /// failed). `None` means "extracted, but the magic was unrecognised or the
    /// file is in no archive" — a real answer worth remembering, since the
    /// alternative is re-extracting and re-inflating the whole file to learn
    /// it again on the next REFR that references it. Bounded like its
    /// siblings; see [`MAX_MAGIC_CACHE_ENTRIES`].
    magic_cache: HashMap<String, Option<byroredux_bgsm::MaterialKind>>,
    /// Insertion-order key tracker for [`magic_cache`] — drives half-eviction.
    magic_cache_order: VecDeque<String>,
    /// Number of Starfield `materialsbeta.cdb` Component Databases
    /// discovered across the loaded archives. The base game ships one
    /// (`materials\materialsbeta.cdb` in `Starfield - Materials.ba2`);
    /// each DLC / Creation ships its own at
    /// `materials\creations\<plugin>\materialsbeta.cdb` inside its
    /// `* - Main.ba2`. `0` for non-Starfield content.
    /// #1289 / SF-D3-NEW-01, multi-CDB discovery #1571 / SF-D3-03.
    ///
    /// Phase 1 (today): presence-only — [`merge_external_material`]'s `.mat`
    /// arm only needs confirmation that Starfield material authoring is
    /// loaded before flipping `is_pbr`, so discovery runs a header-only
    /// probe ([`ComponentDatabaseFile::probe_header`]) and records the
    /// count. It deliberately does NOT retain the full parsed tree: the
    /// vanilla CDB materialises ~1.44M typed entries (multi-second parse,
    /// hundreds of MB–GB of RAM) that nothing reads today.
    /// SF-D3-AUDIT-01 / #2100.
    /// Phase 2 (future, SF-D3-01 #1289): re-`parse` each CDB on demand and
    /// walk the instance trees in load order to build ONE
    /// `material_path → MaterialFields` lookup (DLC last-wins) so
    /// per-material metalness / roughness / texture paths flow into
    /// `ImportedMesh` (mirrors the FO4 BGSM `resolve_bgsm` per-field
    /// translation already wired below) — a single index, no second
    /// per-game material path (CANONICAL-BOUNDARY). Archive order is
    /// preserved in `self.archives`, so re-discovery reproduces load order.
    pub(crate) sf_cdb_count: usize,
    /// #1585 / F6 — per-`MaterialProvider`-instance cache of the
    /// `<Plugin> - Geometry.csg` companion blob, keyed by the cell's master
    /// plugin path. The CSG owns a warm zlib `ChunkCache`, so re-opening it
    /// per precombine cell-load (the pre-fix behaviour) re-read and
    /// re-parsed the ~3700-entry chunk table every tile and discarded all
    /// inter-cell chunk reuse; this cache amortises that cost across every
    /// tile loaded under the SAME provider build. The negative (`None`)
    /// result is cached too, so a non-FO4 / no-CSG plugin isn't re-stat'd on
    /// every precombine cell.
    ///
    /// #2706 (SF-D3-02) — this field previously described itself as
    /// "mirrors the `sf_cdbs` `Arc` hold"; no such field ever existed
    /// (`sf_cdb_count: usize` below is presence-only). Unlike this field,
    /// [`sf_cdb_cache`] (added by #2705) lives at module scope and is
    /// genuinely process-lifetime — it survives across the provider
    /// rebuilds that `build_material_provider` performs on every cell
    /// transition / save-load / debug-load, whereas `csg_cache` here is
    /// discarded along with the rest of `MaterialProvider` on every such
    /// rebuild (see the #2039 / PERF-D7-02 caching design note in
    /// `app_step.rs`) — the two are NOT lifetime-equivalent.
    pub(crate) csg_cache: HashMap<String, Option<Arc<byroredux_bsa::CsgArchive>>>,
}

/// Half-evict a bounded LRU cache (#3857).
///
/// When `len` has reached `cap`, drop the oldest `cap / 2` keys by insertion
/// order, so the recent working set survives instead of the cache clearing
/// wholesale (#1430 / #951 / SAFE-26). Below the cap this is a no-op.
///
/// The four caches in this file wrote this same block out six times, with
/// only the collection and the constant changing — and the copy inside
/// `resolve_bgsm` was the file's only nesting-depth-7 site. `remove` is a
/// closure rather than a bound because the callers are split between a
/// `HashSet` and two `HashMap`s, which share no removal trait; the closure
/// keeps one copy of the loop and lets each caller name its own container.
fn half_evict(order: &mut VecDeque<String>, len: usize, cap: usize, mut remove: impl FnMut(&str)) {
    if len < cap {
        return;
    }
    for _ in 0..cap / 2 {
        let Some(old) = order.pop_front() else {
            break;
        };
        remove(&old);
    }
}

pub(crate) const MAX_BGEM_CACHE_ENTRIES: usize = 1024;
pub(crate) const MAX_FAILED_PATHS: usize = 1024;
/// #3899 — entries are one `Option<MaterialKind>` (a byte) plus the key, so
/// this is far cheaper per entry than the parsed-material caches and can be
/// sized to cover a whole streaming working set rather than a cell's.
pub(crate) const MAX_MAGIC_CACHE_ENTRIES: usize = 4096;

impl MaterialProvider {
    pub(crate) fn new() -> Self {
        Self {
            archives: Vec::new(),
            bgsm_cache: TemplateCache::new(256),
            bgem_cache: HashMap::new(),
            bgem_cache_order: VecDeque::new(),
            magic_cache: HashMap::new(),
            magic_cache_order: VecDeque::new(),
            failed_paths: HashSet::new(),
            failed_paths_order: VecDeque::new(),
            sf_cdb_count: 0,
            csg_cache: HashMap::new(),
        }
    }

    /// Resolve + open the `<Plugin> - Geometry.csg` companion blob once per
    /// PROVIDER BUILD (keyed by `plugin_path`) and hand back a shared handle
    /// — NOT once per session; `self.csg_cache` is discarded along with the
    /// rest of `MaterialProvider` on every `build_material_provider` rebuild
    /// (#2706 / SF-D3-02 corrected the prior "mirrors the `sf_cdbs` `Arc`
    /// caching" claim here — no such field exists; the real process-lifetime
    /// CDB cache is [`sf_cdb_cache`], added by #2705, which lives at module
    /// scope specifically because it needs to outlive provider rebuilds).
    /// #1585 / F6 — precombine cell-loads re-opened this ~240 MB blob every
    /// tile, re-parsing the chunk table and throwing away the warm zlib
    /// `ChunkCache` that amortises inflate across adjacent tiles sharing PSG
    /// regions; this cache fixes that WITHIN one provider build. The
    /// negative result is cached too, so a plugin with no companion CSG
    /// isn't re-probed per cell.
    pub(crate) fn geometry_csg(
        &mut self,
        plugin_path: &str,
    ) -> Option<Arc<byroredux_bsa::CsgArchive>> {
        if let Some(cached) = self.csg_cache.get(plugin_path) {
            return cached.clone();
        }
        let opened = crate::cell_loader::precombined::open_geometry_csg(plugin_path).map(Arc::new);
        self.csg_cache
            .insert(plugin_path.to_owned(), opened.clone());
        opened
    }

    fn push_archive(&mut self, archive: Archive) {
        self.archives.push(archive);
        // #3899 — the magic memo caches NEGATIVE answers too ("in no loaded
        // archive" and "magic unrecognised" both memoise as `None`). A new
        // archive can turn a `None` into a real kind, so drop the memo rather
        // than let a peek taken before this load pin a stale miss. The two
        // material caches need no equivalent: they only ever memoise
        // successful parses, which a later archive cannot invalidate (archive
        // precedence is last-listed-wins on *content*, and `extract_from_archives`
        // already applies it at parse time). Clearing here is free — archives
        // are loaded during setup, before any streaming peek.
        self.magic_cache.clear();
        self.magic_cache_order.clear();
    }

    /// True once at least one Starfield Component Database has been
    /// loaded (base and/or DLC). Drives the `.mat` arm in
    /// [`merge_external_material`] — flipping `material.is_pbr = true` on `.mat`
    /// material paths only when a CDB is present means modded `.mat`
    /// paths against a non-Starfield archive set don't accidentally route
    /// to Disney BSDF. #1289 / SF-D3-NEW-01.
    pub(crate) fn has_starfield_cdb(&self) -> bool {
        self.sf_cdb_count > 0
    }

    pub(crate) fn register_starfield_cdb_probe(&mut self, _info: CdbHeaderInfo) {
        self.sf_cdb_count += 1;
    }

    pub(crate) fn extract_from_archives(&self, path: &str) -> Option<Vec<u8>> {
        // #FO4-D6-NEW — canonicalise the path through
        // `normalize_material_path` (build-prefix strip + leading
        // `data\` strip + `/` → `\` + `materials\` prefix-add) before
        // the archive lookup. The texture resolver at
        // `resolve_texture_with_clamp` already does its own
        // equivalent. Pre-fix, FO4 MedTek `tex.missing` reported 11
        // unique missing-material entries that each failed one or
        // more of the four normalisation rules. See the
        // `normalize_material_path` doc for the full transformation
        // list and per-issue evidence.
        let normalized = normalize_material_path(path);
        // #3637 — last-listed `--materials-ba2` wins, matching the
        // TextureProvider mesh/texture fix (same CLI convention: repeatable,
        // no documented "list mods first" inversion — that's `--scripts-bsa`
        // only, see `ScriptProvider::resolve_pex`'s doc / #1743).
        for archive in self.archives.iter().rev() {
            if let Ok(bytes) = archive.extract(&normalized) {
                return Some(bytes);
            }
        }
        None
    }

    /// Resolve a BGSM file + its template chain. Returns `None` when the
    /// file isn't in any loaded archive, when parse fails, or when the
    /// template chain has a cycle. Logs once per path on the failure paths.
    pub(crate) fn resolve_bgsm(&mut self, path: &str) -> Option<Arc<ResolvedMaterial>> {
        // #FO4-D6-NEW — canonicalise via `normalize_material_path`
        // (build-prefix strip + `data\` strip + `/` → `\` +
        // `materials\` prefix-add) so the cache key + every
        // recursive parent-walk lookup uses the archive-relative
        // form. Live tex.missing observations against MedTek
        // Research:
        //   * top-level material_path: `c:\projects\fallout4\build\pc\
        //     data\materials\setdressing\metallocker01.bgsm` →
        //     normalised to `materials\setdressing\metallocker01.bgsm`
        //   * template parent `root_material_path` inside the BGSM:
        //     `template/defaulttemplate_wet.bgsm` → normalised to
        //     `materials\template\defaulttemplate_wet.bgsm`
        //   * occasional leaf: `data\materials\…` → normalised by
        //     stripping the leading `data\`.
        // See `normalize_material_path` for the full rule set.
        let key = normalize_material_path(path).to_ascii_lowercase();
        // Archive slice is borrowed into the ad-hoc resolver so the
        // cache's mutable borrow doesn't alias archive reads. The
        // resolver normalises on every read so recursive template-
        // parent walks (`root_material_path` carrying any of the
        // four non-canonical forms) resolve correctly.
        struct ArchiveReader<'a> {
            archives: &'a [Archive],
        }
        impl<'a> TemplateResolver for ArchiveReader<'a> {
            fn read(&mut self, path: &str) -> Option<Vec<u8>> {
                let normalized = normalize_material_path(path);
                // #3637 — same last-listed-wins precedence as
                // `extract_from_archives`.
                for archive in self.archives.iter().rev() {
                    if let Ok(bytes) = archive.extract(&normalized) {
                        return Some(bytes);
                    }
                }
                None
            }
        }
        let mut reader = ArchiveReader {
            archives: &self.archives,
        };
        match self.bgsm_cache.resolve(&mut reader, &key) {
            Ok(r) => Some(r),
            Err(byroredux_bgsm::template::ResolveError::DepthLimit { .. }) => {
                // #FO4-D6-NEW — vanilla FO4 ships
                // `materials\template\defaulttemplate_wet.bgsm` with a
                // `root_material_path` field that self-references its
                // own archive path. POST-#1148 the bgsm crate detects
                // cycles internally and returns a cycle-broken chain
                // (parent=None at the cycle anchor), so this catch is
                // a safety net only — it now fires for theoretical
                // >16-deep chains, NOT the documented `defaulttemplate_
                // wet.bgsm` self-reference (which the resolver handles).
                //
                // Recovery (when this DOES fire, on genuine deep chains):
                // re-read the leaf's bytes through the already-normalising
                // `ArchiveReader::read` and construct a parentless
                // `ResolvedMaterial`. The leaf carries authored textures
                // + PBR scalars, which is the load-bearing material data.
                //
                // Vanilla content tops out at depth 3, so the safety net
                // is effectively dormant. Keeping it preserves the
                // graceful-degradation guarantee against any future
                // mod / DLC content that authors >16-deep chains.
                // See audit AUDIT_INCREMENTAL_2026-05-22 ID-5.
                let bytes = reader.read(&key)?;
                let file = match byroredux_bgsm::parse_bgsm(&bytes) {
                    Ok(f) => f,
                    Err(parse_err) => {
                        // #1430 — half-eviction: keep the newer half resident.
                        half_evict(
                            &mut self.failed_paths_order,
                            self.failed_paths.len(),
                            MAX_FAILED_PATHS,
                            |old| {
                                self.failed_paths.remove(old);
                            },
                        );
                        if self.failed_paths.insert(key.clone()) {
                            self.failed_paths_order.push_back(key);
                            log::warn!(
                                "BGSM leaf-only recovery parse failed for '{}': {} \
                                 (self-referential template depth-limit hit)",
                                path,
                                parse_err
                            );
                        }
                        return None;
                    }
                };
                static ONCE: std::sync::Once = std::sync::Once::new();
                ONCE.call_once(|| {
                    log::info!(
                        "BGSM template-cycle recovery active — vanilla FO4 \
                         `defaulttemplate_wet.bgsm` self-references; leaf-only \
                         resolve used. See #FO4-D6-NEW."
                    );
                });
                Some(Arc::new(byroredux_bgsm::template::ResolvedMaterial {
                    file,
                    parent: None,
                }))
            }
            Err(e) => {
                // #951 / SAFE-26 / #1430 — half-eviction on overflow.
                half_evict(
                    &mut self.failed_paths_order,
                    self.failed_paths.len(),
                    MAX_FAILED_PATHS,
                    |old| {
                        self.failed_paths.remove(old);
                    },
                );
                if self.failed_paths.insert(key.clone()) {
                    self.failed_paths_order.push_back(key);
                    log::warn!("BGSM resolve failed for '{}': {}", path, e);
                }
                None
            }
        }
    }

    /// Detect whether a material file is BGSM or BGEM by magic, independent of
    /// its file extension. Returns `None` when the file isn't found or the
    /// magic is unrecognised.
    ///
    /// #3899 (FO4-2026-09-05-D2-02) — this used to go straight to
    /// `extract_from_archives`, i.e. a full archive extract **plus zlib
    /// inflate of the whole material file**, on every merge, purely to read
    /// four bytes. It consulted neither material cache, so a material already
    /// parsed and cached was re-extracted and re-inflated on every subsequent
    /// REFR that referenced it — redundant work proportional to REFR count
    /// rather than distinct-material count, on the cell-streaming hot path,
    /// where it also serialised against the archive's file mutex (the same
    /// mutex #3659 is about; the two compound).
    ///
    /// Three tiers, cheapest first:
    ///
    /// 1. **Either material cache holds the path** — the magic is implied by
    ///    which cache answered, because that is exactly what dispatched the
    ///    parse that populated it. Zero I/O. This is the steady state: once a
    ///    material resolves, every later reference lands here.
    /// 2. **`magic_cache` holds the path** — we extracted once before and
    ///    remembered the answer. Covers the reference that arrives before the
    ///    first resolve completes, and the material that is present but fails
    ///    to parse (which never reaches tier 1 at all, so without this tier it
    ///    would re-extract forever).
    /// 3. **Extract, detect, memoise.** Paid once per distinct path.
    ///
    /// Deliberately NOT consulted: `failed_paths`. Despite its name it is a
    /// log-dedup set, not a negative cache — nothing in this file ever reads
    /// it, only inserts (its `insert` return value gates a `warn!`). It also
    /// could not answer this question if it were: a path fails for reasons
    /// that say nothing about its magic ("present but unparseable" and "in no
    /// archive" land in the same set). Tier 2 covers that case properly.
    pub(crate) fn peek_magic(&mut self, path: &str) -> Option<byroredux_bgsm::MaterialKind> {
        use byroredux_bgsm::MaterialKind;
        let key = normalize_material_path(path).to_ascii_lowercase();

        // Tier 1 — a populated material cache already answers this.
        if self.bgem_cache.contains_key(&key) {
            return Some(MaterialKind::Bgem);
        }
        if self.bgsm_cache.contains(&key) {
            return Some(MaterialKind::Bgsm);
        }
        // Tier 2 — memoised from a previous peek.
        if let Some(hit) = self.magic_cache.get(&key) {
            return *hit;
        }

        // Tier 3 — the expensive path, paid once per distinct material.
        let kind = self
            .extract_from_archives(&key)
            .and_then(|bytes| byroredux_bgsm::detect_kind(&bytes));
        // #951 / SAFE-26 / #1430 — same half-eviction on overflow the sibling
        // caches use: drop the oldest N/2 by insertion order so the recent
        // working set survives instead of clearing everything.
        half_evict(
            &mut self.magic_cache_order,
            self.magic_cache.len(),
            MAX_MAGIC_CACHE_ENTRIES,
            |old| {
                self.magic_cache.remove(old);
            },
        );
        self.magic_cache_order.push_back(key.clone());
        self.magic_cache.insert(key, kind);
        kind
    }

    /// Seed a parsed BGEM directly so merge tests exercise the production
    /// dispatch path without constructing an archive fixture.
    /// Seed a resolved BGSM chain directly so merge tests exercise the real
    /// `merge_external_material` BGSM arm rather than a hand-copied mirror of
    /// its loop (#2702's failure mode). The BGEM sibling below has existed
    /// since the BGEM arm landed; this one was missing only because
    /// `TemplateCache` had no insert.
    #[cfg(test)]
    pub(crate) fn insert_bgsm_for_test(
        &mut self,
        path: &str,
        resolved: byroredux_bgsm::template::ResolvedMaterial,
    ) {
        let key = normalize_material_path(path).to_ascii_lowercase();
        self.bgsm_cache.insert_resolved(&key, Arc::new(resolved));
    }

    /// #3899 — memo size, for the peek-magic cache-tier tests. The order
    /// tracker is the load-bearing one: it grows once per tier-3 (extract +
    /// inflate) run, so an unchanged length across two peeks is direct
    /// evidence the second was served from the memo.
    #[cfg(test)]
    pub(crate) fn magic_cache_len(&self) -> usize {
        self.magic_cache.len()
    }

    #[cfg(test)]
    pub(crate) fn magic_cache_order_len(&self) -> usize {
        self.magic_cache_order.len()
    }

    #[cfg(test)]
    pub(crate) fn insert_bgem_for_test(&mut self, path: &str, bgem: BgemFile) {
        let key = normalize_material_path(path).to_ascii_lowercase();
        self.bgem_cache.insert(key, Arc::new(bgem));
    }

    /// Resolve a BGEM effect-material file. No template inheritance.
    pub(crate) fn resolve_bgem(&mut self, path: &str) -> Option<Arc<BgemFile>> {
        // #FO4-D6-NEW — same `normalize_material_path` canonicalisation
        // as `resolve_bgsm` applied to the cache key. The archive
        // read goes through `extract_from_archives` (which already
        // normalises), so this line is purely for cache-key
        // canonicalisation — two paths that differ only by which
        // non-canonical form they carry must share one cache entry.
        let key = normalize_material_path(path).to_ascii_lowercase();
        if let Some(hit) = self.bgem_cache.get(&key) {
            return Some(Arc::clone(hit));
        }
        // #2601 — was `self.extract_from_archives(&key)?`, a silent early
        // return that never touched `failed_paths` and never logged.
        // Unlike `resolve_bgsm` (whose `bgsm_cache.resolve` wraps EVERY
        // failure mode, including "not in any archive", in one `Err` arm
        // that already records + logs), this "not found" case bypassed
        // the parse-failure arm below entirely, so a missing BGEM file
        // left no diagnostic trail at all — not even the low-level one
        // the BGSM sibling already had. Explicit match instead of `?` so
        // "not found" gets the same bookkeeping as "found but failed to
        // parse".
        let Some(bytes) = self.extract_from_archives(&key) else {
            half_evict(
                &mut self.failed_paths_order,
                self.failed_paths.len(),
                MAX_FAILED_PATHS,
                |old| {
                    self.failed_paths.remove(old);
                },
            );
            if self.failed_paths.insert(key.clone()) {
                self.failed_paths_order.push_back(key);
                log::warn!("BGEM not found in any loaded archive: '{}'", path);
            }
            return None;
        };
        match byroredux_bgsm::parse_bgem(&bytes) {
            Ok(parsed) => {
                let arc = Arc::new(parsed);
                // #951 / SAFE-26 / #1430 — half-eviction on cap: remove the
                // oldest N/2 entries by insertion order so the recent
                // working-set stays resident instead of clearing everything.
                half_evict(
                    &mut self.bgem_cache_order,
                    self.bgem_cache.len(),
                    MAX_BGEM_CACHE_ENTRIES,
                    |old| {
                        self.bgem_cache.remove(old);
                    },
                );
                self.bgem_cache_order.push_back(key.clone());
                self.bgem_cache.insert(key, Arc::clone(&arc));
                Some(arc)
            }
            Err(e) => {
                // Bound failed_paths the same way — broken-content
                // accumulates more slowly than working BGEM count, but
                // capping both prevents the unbounded-growth class.
                // #1430 — half-eviction here too.
                half_evict(
                    &mut self.failed_paths_order,
                    self.failed_paths.len(),
                    MAX_FAILED_PATHS,
                    |old| {
                        self.failed_paths.remove(old);
                    },
                );
                if self.failed_paths.insert(key.clone()) {
                    self.failed_paths_order.push_back(key);
                    log::warn!("BGEM parse failed for '{}': {}", path, e);
                }
                None
            }
        }
    }
}
