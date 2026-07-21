use super::*;

use byroredux_bgsm::template::ResolvedMaterial;
use byroredux_bgsm::{BgemFile, TemplateCache, TemplateResolver};
use byroredux_nif::import::ImportedMesh;
use byroredux_sfmaterial::ComponentDatabaseFile;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

/// True for a Starfield component-database path — the base
/// `materials\materialsbeta.cdb` or any DLC/Creation-namespaced
/// `materials\creations\<plugin>\materialsbeta.cdb`. #1571 / SF-D3-03.
pub(crate) fn is_materialsbeta_cdb_path(path: &str) -> bool {
    let p = path.replace('/', "\\").to_ascii_lowercase();
    p.starts_with("materials\\") && p.ends_with("materialsbeta.cdb")
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
        match archive.extract(&path) {
            Ok(bytes) => {
                log::info!("Discovered Starfield CDB '{path}' in '{source}'");
                provider.register_starfield_cdb(&bytes);
            }
            Err(e) => log::warn!("Failed to extract CDB '{path}' from '{source}': {e}"),
        }
    }
}

/// Conductor diffuse-tint blend (#1591). When saturation-derived
/// `metalness > 0.5`, bias the diffuse albedo halfway toward the authored
/// spec CHROMATICITY so the shader's `F0 = mix(0.04, albedo, metalness)`
/// lands on the right conductor tint even when the DDS albedo is
/// BC1-desaturated. The half weight keeps the diffuse texture's detail
/// (rivets, wear, edge highlights) visually present.
///
/// Blends toward the mult-free `specular_color`, NOT `specular_color ×
/// specular_mult`: per #1476 the `mult` only scales highlight strength —
/// it's not an albedo/F0 quantity — so folding it in darkened the tint
/// toward black for `mult < 1` and overshot past 1.0 (unclamped into
/// `GpuMaterial.diffuse_*`) for `mult > 1`. Making `mult` structurally
/// absent from this signature is the guarantee. Output is clamped to `[0,1]`.
pub(crate) fn conductor_diffuse_tint(diffuse: [f32; 3], specular_color: [f32; 3]) -> [f32; 3] {
    [
        (0.5 * diffuse[0] + 0.5 * specular_color[0]).clamp(0.0, 1.0),
        (0.5 * diffuse[1] + 0.5 * specular_color[1]).clamp(0.0, 1.0),
        (0.5 * diffuse[2] + 0.5 * specular_color[2]).clamp(0.0, 1.0),
    ]
}

/// Derive scalar metalness from a BGSM leaf's authored specular (#1476,
/// `08ed03be`). `spec` is `specular_color * specular_mult` for the pbr
/// branch, or raw `specular_color` for the legacy branch — see call site.
///
/// - `pbr = true`: true metallic-roughness authoring, `spec` is F0 —
///   metalness follows F0 luminance.
/// - `pbr = false`: legacy spec-glossiness. `mult` only scales highlight
///   TINT, not F0 — it is ~white `[1,1,1]` for every dielectric (concrete,
///   wood, plaster, painted metal). Keying metalness off luminance here is
///   BACKWARDS: vanilla `paintpeelingconcrete` authors `spec=[1,1,1]
///   mult=1.0` (lum 1.0 → metalness 1.0, mirror-chrome concrete) while real
///   metals author lower, often tinted spec — `metallocker` `[1,0.85,0.70]
///   mult=0.45`. The only legacy signal that distinguishes a conductor is
///   spec CHROMATICITY (conductor F0 is tinted; dielectric F0 is
///   achromatic grey), so metalness is derived from spec-color saturation
///   `(max-min)/max`, which is mult-invariant: white spec → 0, tinted
///   spec → metallic.
pub(crate) fn bgsm_metalness(spec: [f32; 3], pbr: bool) -> f32 {
    if pbr {
        let spec_lum = 0.2126 * spec[0] + 0.7152 * spec[1] + 0.0722 * spec[2];
        ((spec_lum - 0.04) / 0.96).clamp(0.0, 1.0)
    } else {
        let mx = spec[0].max(spec[1]).max(spec[2]);
        let mn = spec[0].min(spec[1]).min(spec[2]);
        if mx > 1.0e-4 {
            ((mx - mn) / mx).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }
}

/// Select the shared transmissive-glass behavior from BGEM authoring.
///
/// Modern BGEM v21+ files expose `glass_enabled`, while older FO4 BGEMs
/// predate that field. Vanilla still authors clear hard-surface shells in
/// those files through a coherent feature set: standard alpha blending,
/// no depth write, two-sided/non-occluding geometry, lit view-angle falloff,
/// and an environment-map + mask + normal-map stack. The Port-A-Diner dome
/// is the canonical v2 example. Treat that feature bundle as the legacy
/// spelling of the same shared glass behavior; the individual maps remain
/// material overlays after classification.
pub(crate) fn bgem_uses_glass_behavior(bgem: &BgemFile) -> bool {
    if bgem.glass_enabled || bgem.base.refraction {
        return true;
    }

    let blend = bgem.base.alpha_blend_mode;
    let standard_alpha = blend.function > 0 && blend.src_blend == 6 && blend.dst_blend == 7;
    let hard_transparent_shell = standard_alpha
        && bgem.base.alpha > 0.0
        && bgem.base.alpha < 1.0
        && !bgem.base.alpha_test
        && !bgem.base.z_buffer_write
        && bgem.base.z_buffer_test
        && bgem.base.two_sided
        && bgem.base.non_occluder
        && !bgem.base.decal;
    let reflective_surface_maps = bgem.base.environment_mapping
        && !bgem.envmap_texture.is_empty()
        && !bgem.envmap_mask_texture.is_empty()
        && !bgem.normal_texture.is_empty();
    let lit_fresnel_falloff = bgem.effect_lighting_enabled
        && bgem.falloff_enabled
        && !bgem.soft_enabled
        && !bgem.blood_enabled
        && !bgem.base.grayscale_to_palette_color
        && !bgem.grayscale_to_palette_alpha
        && bgem.grayscale_texture.is_empty();

    bgem.base.version < 21
        && hard_transparent_shell
        && reflective_surface_maps
        && lit_fresnel_falloff
}

/// Select the thin-shell variant of the shared glass behavior.
///
/// `non_occluder` is behavioral authoring, not merely a culling hint: the
/// surface is meant to composite over geometry behind it and does not define
/// the boundary of a closed optical volume. Keep this decision in the source
/// translator so downstream rendering stays format-agnostic.
pub(crate) fn bgem_uses_thin_glass_behavior(bgem: &BgemFile) -> bool {
    bgem.base.non_occluder && bgem_uses_glass_behavior(bgem)
}

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
                            log::info!("Opened materials archive: '{}'", path);
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
            // archives owned by the TextureProvider. The archive is
            // re-opened here purely to read its file table (the entry data
            // isn't touched) and dropped after the scan.
            "--bsa" => {
                if let Some(path) = args.get(i + 1) {
                    match Archive::open(path) {
                        Ok(a) => discover_starfield_cdbs(&a, path, &mut provider),
                        Err(e) => {
                            log::warn!("Failed to open '{}' for CDB discovery: {}", path, e)
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
/// path into `ImportedMesh.material_path`; this provider opens the files
/// out of `Fallout4 - Materials.ba2` (or equivalent) and hands back the
/// parsed + template-resolved chain. The LRU is owned by `bgsm`'s
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
    /// Number of Starfield `materialsbeta.cdb` Component Databases
    /// discovered across the loaded archives. The base game ships one
    /// (`materials\materialsbeta.cdb` in `Starfield - Materials.ba2`);
    /// each DLC / Creation ships its own at
    /// `materials\creations\<plugin>\materialsbeta.cdb` inside its
    /// `* - Main.ba2`. `0` for non-Starfield content.
    /// #1289 / SF-D3-NEW-01, multi-CDB discovery #1571 / SF-D3-03.
    ///
    /// Phase 1 (today): presence-only — [`merge_bgsm_into_mesh`]'s `.mat`
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
    /// #1585 / F6 — process-lifetime cache of the `<Plugin> - Geometry.csg`
    /// companion blob, keyed by the cell's master plugin path. Mirrors the
    /// `sf_cdbs` `Arc` hold: the CSG owns a warm zlib `ChunkCache`, so
    /// re-opening it per precombine cell-load (the pre-fix behaviour)
    /// re-read and re-parsed the ~3700-entry chunk table every tile and
    /// discarded all inter-cell chunk reuse. The negative (`None`) result
    /// is cached too, so a non-FO4 / no-CSG plugin isn't re-stat'd on
    /// every precombine cell.
    pub(crate) csg_cache: HashMap<String, Option<Arc<byroredux_bsa::CsgArchive>>>,
}

/// #951 / SAFE-26 — bounded-cache caps for `MaterialProvider`. Sized to
/// comfortably hold the unique BGEM/BGSM-ref count of any single vanilla
/// cell (~100s) plus a few cells of streaming residency.
pub(crate) const MAX_BGEM_CACHE_ENTRIES: usize = 1024;
pub(crate) const MAX_FAILED_PATHS: usize = 1024;

impl MaterialProvider {
    pub(crate) fn new() -> Self {
        Self {
            archives: Vec::new(),
            bgsm_cache: TemplateCache::new(256),
            bgem_cache: HashMap::new(),
            bgem_cache_order: VecDeque::new(),
            failed_paths: HashSet::new(),
            failed_paths_order: VecDeque::new(),
            sf_cdb_count: 0,
            csg_cache: HashMap::new(),
        }
    }

    /// Resolve + open the `<Plugin> - Geometry.csg` companion blob once per
    /// session (keyed by `plugin_path`) and hand back a shared handle.
    /// #1585 / F6 — mirrors the `sf_cdbs` `Arc` caching: precombine cell-loads
    /// re-opened this ~240 MB blob every tile, re-parsing the chunk table and
    /// throwing away the warm zlib `ChunkCache` that amortises inflate across
    /// adjacent tiles sharing PSG regions. The negative result is cached so a
    /// plugin with no companion CSG isn't re-probed per cell.
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
    }

    /// True once at least one Starfield Component Database has been
    /// loaded (base and/or DLC). Drives the `.mat` arm in
    /// [`merge_bgsm_into_mesh`] — flipping `mesh.is_pbr = true` on `.mat`
    /// material paths only when a CDB is present means modded `.mat`
    /// paths against a non-Starfield archive set don't accidentally route
    /// to Disney BSDF. #1289 / SF-D3-NEW-01.
    pub(crate) fn has_starfield_cdb(&self) -> bool {
        self.sf_cdb_count > 0
    }

    /// Validate + register a Starfield `materialsbeta.cdb` payload for the
    /// presence gate — `discover_starfield_cdbs` calls this once per CDB
    /// found across the loaded archives (#1571). Runs a `peek_magic` cheap
    /// reject (SF-D3-AUDIT-03 / #2102) then a header-only
    /// [`ComponentDatabaseFile::probe_header`] validity check
    /// (SF-D3-AUDIT-01 / #2100) and bumps `sf_cdb_count` on success — it
    /// does NOT walk or retain the ~1.44M-entry instance tree (see the
    /// `sf_cdb_count` field doc). A malformed payload is warned and
    /// dropped, leaving the count intact. #1289 / SF-D3-NEW-01.
    pub(crate) fn register_starfield_cdb(&mut self, bytes: &[u8]) {
        // Cheapest reject first: 4-byte magic. Skips the header/chunk-index
        // work for a mis-named non-CDB file. SF-D3-AUDIT-03 / #2102.
        if !ComponentDatabaseFile::peek_magic(bytes) {
            log::warn!(
                "Starfield CDB rejected ({} bytes): not a BETH-signature file. \
                 Starfield content will fall back to legacy Lambert shading.",
                bytes.len(),
            );
            return;
        }
        match ComponentDatabaseFile::probe_header(bytes) {
            Ok(info) => {
                log::info!(
                    "Starfield CDB present: {} chunks ({} bytes, header-only probe). \
                     `.mat` material paths on NIFs will route through Disney BSDF \
                     (Phase 1 — full parse + per-field extraction is the deferred \
                     Phase 2 follow-up).",
                    info.chunk_count,
                    bytes.len(),
                );
                self.sf_cdb_count += 1;
            }
            Err(e) => {
                log::warn!(
                    "Starfield CDB header invalid ({} bytes): {}. \
                     Starfield content will fall back to legacy Lambert shading.",
                    bytes.len(),
                    e,
                );
            }
        }
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
        for archive in &self.archives {
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
                for archive in self.archives {
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
                        if self.failed_paths.len() >= MAX_FAILED_PATHS {
                            // #1430 — half-eviction: keep the newer half resident.
                            for _ in 0..MAX_FAILED_PATHS / 2 {
                                if let Some(old) = self.failed_paths_order.pop_front() {
                                    self.failed_paths.remove(&old);
                                }
                            }
                        }
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
                if self.failed_paths.len() >= MAX_FAILED_PATHS {
                    for _ in 0..MAX_FAILED_PATHS / 2 {
                        if let Some(old) = self.failed_paths_order.pop_front() {
                            self.failed_paths.remove(&old);
                        }
                    }
                }
                if self.failed_paths.insert(key.clone()) {
                    self.failed_paths_order.push_back(key);
                    log::warn!("BGSM resolve failed for '{}': {}", path, e);
                }
                None
            }
        }
    }

    /// Read the first 4 bytes of a material file from the archives to detect
    /// whether it is BGSM or BGEM by magic, independent of its file extension.
    /// Returns `None` when the file isn't found or the magic is unrecognised.
    fn peek_magic(&self, path: &str) -> Option<byroredux_bgsm::MaterialKind> {
        let bytes = self.extract_from_archives(path)?;
        byroredux_bgsm::detect_kind(&bytes)
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
        let bytes = self.extract_from_archives(&key)?;
        match byroredux_bgsm::parse_bgem(&bytes) {
            Ok(parsed) => {
                let arc = Arc::new(parsed);
                // #951 / SAFE-26 / #1430 — half-eviction on cap: remove the
                // oldest N/2 entries by insertion order so the recent
                // working-set stays resident instead of clearing everything.
                if self.bgem_cache.len() >= MAX_BGEM_CACHE_ENTRIES {
                    for _ in 0..MAX_BGEM_CACHE_ENTRIES / 2 {
                        if let Some(old) = self.bgem_cache_order.pop_front() {
                            self.bgem_cache.remove(&old);
                        }
                    }
                }
                self.bgem_cache_order.push_back(key.clone());
                self.bgem_cache.insert(key, Arc::clone(&arc));
                Some(arc)
            }
            Err(e) => {
                // Bound failed_paths the same way — broken-content
                // accumulates more slowly than working BGEM count, but
                // capping both prevents the unbounded-growth class.
                // #1430 — half-eviction here too.
                if self.failed_paths.len() >= MAX_FAILED_PATHS {
                    for _ in 0..MAX_FAILED_PATHS / 2 {
                        if let Some(old) = self.failed_paths_order.pop_front() {
                            self.failed_paths.remove(&old);
                        }
                    }
                }
                if self.failed_paths.insert(key.clone()) {
                    self.failed_paths_order.push_back(key);
                    log::warn!("BGEM parse failed for '{}': {}", path, e);
                }
                None
            }
        }
    }
}

/// Merge BGSM / BGEM material data into an `ImportedMesh` whose
/// `material_path` points at a .bgsm or .bgem file. NIF fields take
/// precedence — only empty slots are filled in from the resolved
/// material chain. This matches Bethesda's runtime behaviour where the
/// shader property can override template defaults per-mesh.
///
/// For BGSM the template chain is walked child-first: the first
/// non-empty value for any given field wins. BGEM has no inheritance
/// (the format carries no `root_material_path`) so we read the single
/// parsed file. Returns `true` when any field was filled.
/// Narrow a BGSM/BGEM `src_blend`/`dst_blend` value to the `u8` the
/// Gamebryo `NiAlphaProperty` blend-factor field (and
/// [`gamebryo_to_vk_blend_factor`](byroredux_renderer)) expects.
///
/// **No translation happens here** — `src_blend`/`dst_blend` are
/// already Gamebryo-native values (`ONE=0, ZERO=1, DST_COLOR=4,
/// SRC_ALPHA=6, ONE_MINUS_SRC_ALPHA=7, …`, the same scale
/// `gamebryo_to_vk_blend_factor` reads), re-derived directly from the
/// reference implementation
/// (`Material-Editor:BaseMaterialFile.cs::ConvertAlphaBlendMode`):
/// `Standard = (src=6,dst=7)`, `Additive = (src=6,dst=0)`,
/// `Multiplicative = (src=4,dst=1)`. Feeding those straight through
/// `gamebryo_to_vk_blend_factor` already produces the correct blend
/// state for all three.
///
/// This function used to be named `gl_to_gamebryo_blend` and swap
/// `0↔1` on the premise that these fields were a "GL-style enum"
/// inverted from the Gamebryo nibble. That premise was false (no such
/// GL-style enum appears anywhere in the reference source — real GL
/// blend enums are large hex constants like `GL_SRC_ALPHA = 0x0302`,
/// not small integers). The swap (#1651) fixed its motivating case (an
/// additive BGEM rendering invisible) only by accident — the fixture
/// used to justify it was a synthetic `(function=2, src=1, dst=1)`
/// tuple the reference parser never actually emits — and broke the two
/// real modes that touch `0`/`1`: Additive's `dst=0` swapped to `1`
/// (`ZERO`, killing the additive accumulation) and Multiplicative's
/// `dst=1` swapped to `0` (`ONE`, leaking the destination through).
/// Standard's `(6,7)` pair is a fixed point of the swap, which is why
/// the regression went unnoticed. Renamed on the #1823 fix so the name
/// no longer implies a translation direction that doesn't exist — a
/// future reader should not "restore" the swap.
pub(crate) fn bgsm_blend_to_gamebryo(raw: u32) -> u8 {
    raw as u8
}

/// SF3-02 / #1831 — chooses the diagnostic message for a material path
/// that fell through to the unknown-format arm of [`merge_bgsm_into_mesh`].
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

pub(crate) fn merge_bgsm_into_mesh(
    mesh: &mut ImportedMesh,
    provider: &mut MaterialProvider,
    pool: &mut byroredux_core::string::StringPool,
) -> bool {
    let Some(path_sym) = mesh.material_path else {
        return false;
    };
    // `StringPool::resolve` returns the canonical lowercased form, so
    // we own the string for the BGSM dispatch + suffix matching here
    // without an extra `to_ascii_lowercase` allocation. See #609.
    let path: String = match pool.resolve(path_sym) {
        Some(s) => s.to_string(),
        None => return false,
    };

    // `touched` flips to `true` on any merged field. Allowed unused
    // assignment: the success branches (BGSM / BGEM) set `touched`
    // unconditionally via `mesh.from_bgsm = true`, so the `false`
    // initializer is overwritten before any read — but the
    // initializer is load-bearing for the failure / unknown-kind
    // path that returns it without further assignment.
    #[allow(unused_assignments)]
    let mut touched = false;
    // `fill` populates an `Option<FixedString>` slot only when it's
    // None and the incoming value is non-empty. Routes through the
    // engine's `StringPool` so the BGSM/BGEM-resolved paths share the
    // same intern table as the NIF-side paths (#609 / D6-NEW-01).
    fn fill(
        slot: &mut Option<byroredux_core::string::FixedString>,
        value: &str,
        touched: &mut bool,
        pool: &mut byroredux_core::string::StringPool,
    ) {
        if slot.is_none() && !value.is_empty() {
            *slot = Some(pool.intern(value));
            *touched = true;
        }
    }

    // #1289 / SF-D3-NEW-01 — Starfield `.mat` arm. Starfield material
    // paths captured by the NIF stopcond (`crates/nif/src/blocks/
    // shader.rs::is_material_path`) end in `.mat`. The actual material
    // data lives in the binary Component Database at
    // `materials\materialsbeta.cdb` inside `Starfield - Materials.ba2`,
    // loaded once at provider init via [`register_starfield_cdb`].
    //
    // Phase 1 (this commit): flip `mesh.is_pbr = true` so
    // `pack_bgsm_material_flags` packs `MAT_FLAG_PBR_BSDF` and
    // `triangle.frag` routes Starfield content through the Disney BSDF
    // path instead of the legacy Lambert + simple-GGX path (the audit
    // FAIL closure). Defaults for metalness / roughness / textures
    // stay at the NIF-derived values — better than Lambert but still
    // approximate; Phase 2 will walk the CDB to extract authored values.
    //
    // The CDB-presence gate (`has_starfield_cdb`) prevents accidental
    // PBR routing for modded `.mat` paths against a non-Starfield
    // archive set (FO4 / FNV cells with a stray mod-authored `.mat`
    // shouldn't get Disney BSDF).
    if path.ends_with(".mat") && provider.has_starfield_cdb() {
        mesh.is_pbr = true;
        // `from_bgsm` deliberately NOT set — that flag gates BGSM
        // spec-glossiness translation (FO4-specific format convention).
        // Starfield .mat authors metalness/roughness directly, but this
        // `.mat` arm returns early without touching them — NIF import
        // (`bs_geometry.rs`) already set `metalness_override`/
        // `roughness_override` to `Some(classify_legacy_pbr(...))` before
        // this function ever runs, so the NaN-sentinel path in
        // `Material::resolve_pbr` never fires for Starfield content.
        // Phase 2 must *overwrite* those `Some` values with CDB-authored
        // ones rather than relying on that unreachable fallback.
        return true;
    }

    // BGSM/BGEM scalar-override state. The `Option<String>` slots use
    // `is_none()` to detect "NIF left this empty", but scalar PBR fields
    // default to concrete values on the NIF side (e.g. emissive_mult = 0.0,
    // specular_strength = 1.0), so we can't key off the default.
    // Instead we track per-field "has a BGSM entry already overridden
    // this slot" flags — BGSM resolver chain is walked child-first so
    // the first authored value wins, matching the texture-slot policy.
    // Pre-#583 every scalar the BGSM parser decoded was silently dropped
    // and the mesh rendered on NIF-fallback PBR.
    let mut set_emissive = false;
    let mut set_specular = false;
    let mut set_glossiness = false;
    let mut set_alpha = false;
    let mut set_uv = false;
    let mut set_blend = false;
    let mut set_fresnel = false;
    let mut set_palette_scale = false;

    // Determine dispatch kind from magic (authoritative) with extension as
    // fallback. Warn once per path when they disagree — e.g. a mod shipping a
    // `.bgsm`-named file that carries BGEM magic (wrong-extension footgun).
    use byroredux_bgsm::MaterialKind;
    let ext_kind = if path.ends_with(".bgsm") {
        Some(MaterialKind::Bgsm)
    } else if path.ends_with(".bgem") {
        Some(MaterialKind::Bgem)
    } else {
        None
    };
    let magic_kind = provider.peek_magic(&path);
    if let (Some(ext), Some(magic)) = (ext_kind, magic_kind) {
        if ext != magic {
            log::warn!(
                "material '{}': extension implies {:?} but file magic implies {:?}; \
                 dispatching on magic to avoid wrong override semantics",
                path,
                ext,
                magic
            );
        }
    }
    // Magic wins when present; extension is the fallback for files not (yet)
    // in any loaded archive (caller already got None from peek_magic).
    let dispatch_kind = magic_kind.or(ext_kind);

    if dispatch_kind == Some(MaterialKind::Bgsm) {
        let Some(resolved) = provider.resolve_bgsm(&path) else {
            return false;
        };
        // BGSM resolution succeeded — telemetry-only flag (no renderer
        // branch); the substantive work happens in the spec-glossiness
        // → metallic-roughness translation below.
        mesh.from_bgsm = true;
        touched = true;
        // #1352 / FO4-D7-03 — route ALL BGSM-authored content through the
        // Disney diffuse lobe (MAT_FLAG_PBR_BSDF via `pack_bgsm_material_flags`),
        // not just the rarely-authored `bgsm.pbr == true` case (0 of 793
        // sampled vanilla FO4 BGSMs set it). The spec-glossiness →
        // metallic-roughness translation below gives every `from_bgsm` mesh
        // valid metalness/roughness for the lobe to consume; the per-BGSM
        // `if bgsm.pbr` set below is now subsumed (kept as a defensive
        // backstop). NOTE: this changes the diffuse shading of all vanilla
        // FO4 BGSM content (was Lambert, correct-as-authored for Bethesda's
        // modified Blinn-Phong pipeline) — pending RenderDoc visual
        // validation on real FO4 content. Reverting is this single line.
        mesh.is_pbr = true;

        // ── Translation layer (BGSM spec-glossiness → standard PBR) ──
        //
        // The renderer consumes a single PBR contract: `albedo`,
        // `metalness`, `roughness`, `F0 = mix(0.04, albedo, metalness)`.
        // BGSM authors a DIFFERENT contract; how `specular_color * mult`
        // relates to metalness depends on the BGSM's `pbr` flag:
        //
        // * `pbr == true` (rare — 0 of 793 sampled vanilla FO4 BGSMs set
        //   it; almost exclusively modded content): the material was
        //   authored in a metallic-roughness workflow and `spec_color *
        //   mult` IS F0 directly (dielectric ≈ 0.04, conductor ≈ tinted).
        //   Luminance → metalness is correct here.
        //
        // * `pbr == false` (legacy spec-glossiness — essentially all
        //   vanilla FO4 architecture/clutter): `spec_color` is the Blinn
        //   highlight TINT, not F0. It is ~white `[1,1,1]` for every
        //   dielectric (concrete, wood, plaster, painted metal) and the
        //   `mult` only scales highlight strength. Keying metalness off
        //   luminance is not just wrong but BACKWARDS: vanilla
        //   `paintpeelingconcrete` authors `spec=[1,1,1] mult=1.0`
        //   (lum 1.0 → metalness 1.0, mirror-chrome concrete) while real
        //   metals author LOWER, often TINTED spec — `metalrubberductpipe`
        //   `[1,1,1] mult=0.73`, `metallocker` `[1,0.85,0.70] mult=0.45`.
        //   The only legacy signal that actually distinguishes a conductor
        //   is spec CHROMATICITY (conductor F0 is tinted; dielectric F0 is
        //   achromatic grey), so we derive metalness from spec-color
        //   saturation, which is invariant to `mult`. White spec → 0
        //   (concrete is dielectric); tinted spec → metallic (brass/gold/
        //   copper keep their look). Pure-white-spec steel reads dielectric
        //   — a minor under-read, but never the pervasive chrome the old
        //   luminance path produced. (Per-texel metalness from the spec
        //   map would recover white-spec steel; deferred — needs a
        //   metalness-map shader binding. See `feedback_format_translation`.)
        //
        // Roughness is `1 - smoothness` either way (the per-texel
        // `gloss_map` then modulates it in-shader: `mix(1, roughness,
        // glossSample)`), so the scalar is only the smooth-end of the lobe.
        //
        // Derivation is LEAF-only — the leaf author's choice is
        // authoritative; template parents are background defaults the
        // artist explicitly overrode if they set a different value.
        //
        // For metallic materials, also tint `mesh.diffuse_color` toward
        // the authored spec_color so the per-pixel `F0 = mix(0.04,
        // albedo, metalness)` lands on the right conductor tint when
        // the diffuse texture is BC1-desaturated (a known FO4 issue —
        // raw_metal_diff DDS textures lose saturation vs the authored
        // spec RGB). Pure dielectric materials keep `diffuse_color`
        // untouched so painted-plastic textures aren't shifted.
        let leaf = &resolved.file;
        let spec_r = leaf.specular_color[0] * leaf.specular_mult;
        let spec_g = leaf.specular_color[1] * leaf.specular_mult;
        let spec_b = leaf.specular_color[2] * leaf.specular_mult;
        // pbr: spec*mult is F0. Legacy: mult-free specular_color, since
        // `mult` only scales highlight strength, not F0 — see
        // `bgsm_metalness` doc comment (#1476).
        let metalness = if leaf.pbr {
            bgsm_metalness([spec_r, spec_g, spec_b], true)
        } else {
            bgsm_metalness(leaf.specular_color, false)
        };
        let roughness = (1.0 - leaf.smoothness).clamp(0.04, 1.0);
        mesh.metalness_override = Some(metalness);
        mesh.roughness_override = Some(roughness);
        if metalness > 0.5 {
            // #1591 — blend toward the mult-free `specular_color`, NOT
            // `spec_*` (= specular_color × specular_mult); the mult-bearing
            // `spec_*` stays for the pbr F0-luminance path above where
            // mult-as-scale is correct. See `conductor_diffuse_tint`.
            mesh.diffuse_color = conductor_diffuse_tint(mesh.diffuse_color, leaf.specular_color);
        }
        for step in resolved.walk() {
            let bgsm = &step.file;
            fill(
                &mut mesh.texture_path,
                &bgsm.diffuse_texture,
                &mut touched,
                pool,
            );
            fill(
                &mut mesh.normal_map,
                &bgsm.normal_texture,
                &mut touched,
                pool,
            );
            fill(&mut mesh.glow_map, &bgsm.glow_texture, &mut touched, pool);
            // Smoothness/spec mask — .r encodes per-texel specular
            // strength in the engine's existing gloss_map slot. #453.
            fill(
                &mut mesh.gloss_map,
                &bgsm.smooth_spec_texture,
                &mut touched,
                pool,
            );
            // #1353 / FO4-D8-07 — BGSM greyscale-to-palette LUT path
            // (`SLSF1::Greyscale_To_PaletteColor`, used by FO4 NPC /
            // creature colour variants; the palette slot is authored on
            // v<=2 BGSMs). First non-empty in the template chain wins, to
            // match the texture fills above. Routed to ResolvedPaths →
            // GreyscaleLutHandle and flagged via EFFECT_PALETTE_COLOR in
            // `pack_bgsm_material_flags` so the lit-path remap in
            // triangle.frag samples it.
            if mesh.bgsm_greyscale_lut_path.is_none() && !bgsm.greyscale_texture.is_empty() {
                mesh.bgsm_greyscale_lut_path = Some(bgsm.greyscale_texture.clone());
                touched = true;
            }
            // Legacy v <= 2 environment cube; newer BGSMs drop the slot.
            fill(&mut mesh.env_map, &bgsm.envmap_texture, &mut touched, pool);
            fill(
                &mut mesh.parallax_map,
                &bgsm.displacement_texture,
                &mut touched,
                pool,
            );
            // #1076 / FO4-D6-002 — BGSM v>2 standalone slots that
            // pre-fix were parsed but dropped on the floor. Each is
            // empty on the v<=2 path (the parser leaves the String
            // default) so the `fill` no-op suffices to gate the
            // forward without an explicit version check.
            fill(
                &mut mesh.specular_map,
                &bgsm.specular_texture,
                &mut touched,
                pool,
            );
            fill(
                &mut mesh.lighting_map,
                &bgsm.lighting_texture,
                &mut touched,
                pool,
            );
            fill(&mut mesh.flow_map, &bgsm.flow_texture, &mut touched, pool);
            fill(
                &mut mesh.wrinkle_map,
                &bgsm.wrinkles_texture,
                &mut touched,
                pool,
            );
            // #1077 / FO4-D6-003 (Phase 1: data propagation) —
            // BGSM-only shader flags. Same child-first precedence as
            // the texture slots: first authored `true` wins, parent
            // chain only contributes when the child leaves the flag
            // at its bool default. The default-false case is
            // structurally identical to the texture-slot "empty
            // string" gate — both behave as "not authored, fall
            // through to parent / leave unchanged".
            //
            // The renderer-side consumer (Phase 2) is deferred per
            // the original #1077 split: gating PBR vs Gamebryo
            // shading in `triangle.frag` based on `is_pbr` needs
            // RenderDoc-validated visual diffs against FO4 content,
            // which is out of scope for this Phase 1 close-out.
            if !mesh.is_pbr && bgsm.pbr {
                mesh.is_pbr = true;
                touched = true;
            }
            if !mesh.has_translucency && bgsm.translucency {
                mesh.has_translucency = true;
                touched = true;
            }
            if !mesh.model_space_normals && bgsm.model_space_normals {
                mesh.model_space_normals = true;
                touched = true;
            }

            // #1147 Phase 2b — BGSM v>=8 translucency suite. Same
            // child-first precedence as the flags above. The
            // `has_translucency` flag is the gate; if the child
            // already set it, the corresponding subsurface params
            // also came from the child and we don't overwrite them.
            // If `has_translucency` is set by this chain entry but
            // the params are still at default-zero, propagate them.
            if bgsm.translucency
                && mesh.translucency_transmissive_scale == 0.0
                && mesh.translucency_subsurface_color == [0.0; 3]
            {
                mesh.translucency_subsurface_color = bgsm.translucency_subsurface_color;
                mesh.translucency_transmissive_scale = bgsm.translucency_transmissive_scale;
                mesh.translucency_turbulence = bgsm.translucency_turbulence;
                mesh.translucency_thick_object = bgsm.translucency_thick_object;
                mesh.translucency_mix_albedo = bgsm.translucency_mix_albedo_with_subsurface_color;
                touched = true;
            }

            // Scalar PBR forwarding (#583). Child-first: first authored
            // value wins. Parser already decodes these fields; the
            // pre-fix merge dropped them on the floor.
            if !set_emissive && bgsm.emit_enabled {
                mesh.emissive_color = bgsm.emittance_color;
                mesh.emissive_mult = bgsm.emittance_mult;
                set_emissive = true;
                touched = true;
            }
            if !set_specular {
                mesh.specular_color = bgsm.specular_color;
                mesh.specular_strength = bgsm.specular_mult;
                set_specular = true;
                touched = true;
            }
            if !set_glossiness {
                // BGSM authors `smoothness` 0–1 (Bethesda Material Editor
                // convention); `Material::glossiness` is on the 0–100 NIF
                // scale (`classify_pbr` divides by 100). Multiply by 100
                // to normalize — without this, BGSM-driven FO4 materials
                // that don't keyword-match the metal/wood/glass arms in
                // `classify_pbr` fell through to the glossiness fallback
                // with `roughness=0.95`, killing direct specular and the
                // RT-reflection metalness/roughness gate (Med-Tek floors).
                mesh.glossiness = bgsm.smoothness * 100.0;
                set_glossiness = true;
                touched = true;
            }
            // #1454 — BGSM authors Fresnel power (Schlick exponent for the
            // rim Fresnel term). Child-first: first BGSM in the template
            // chain wins. Vanilla FO4 defaults to 5.0, matching the
            // `ImportedMesh` default, so no vanilla regression; mod-authored
            // non-default values (power armor, shiny metals) were silently
            // falling back to 5.0 before this fix.
            if !set_fresnel {
                mesh.fresnel_power = bgsm.fresnel_power;
                set_fresnel = true;
                touched = true;
            }
            // #1455 — BGSM authors greyscale-to-palette scale. Child-first.
            // Modulates the LUT remap intensity for NPC creature colour
            // variants (deathclaw, supermutant). Default 1.0 = no change.
            if !set_palette_scale {
                mesh.grayscale_to_palette_scale = bgsm.grayscale_to_palette_scale;
                set_palette_scale = true;
                touched = true;
            }
            if !set_alpha {
                mesh.mat_alpha = bgsm.base.alpha;
                set_alpha = true;
                touched = true;
            }
            if !set_uv {
                mesh.uv_offset = [bgsm.base.u_offset, bgsm.base.v_offset];
                mesh.uv_scale = [bgsm.base.u_scale, bgsm.base.v_scale];
                set_uv = true;
                touched = true;
            }
            // Boolean gameplay flags OR across the template chain — if
            // ANY ancestor marks the material as two-sided / decal /
            // alpha-test, the concrete instance is too.
            if bgsm.base.two_sided {
                mesh.two_sided = true;
                touched = true;
            }
            if bgsm.base.decal {
                mesh.is_decal = true;
                touched = true;
            }
            if bgsm.base.alpha_test && !mesh.alpha_test {
                mesh.alpha_test = true;
                mesh.alpha_threshold = f32::from(bgsm.base.alpha_test_ref) / 255.0;
                touched = true;
            }
            // BGSM alpha-blend forwarding. FO4+ moved per-material blend
            // state out of NiAlphaProperty into BGSM, so a BGSM-only
            // glass / decal authored with `alpha_blend_mode.function == 1`
            // (Standard) leaves the NIF-side `has_alpha` at false and
            // every Institute / lab pane renders fully opaque
            // (`INSTANCE_FLAG_ALPHA_BLEND` never sets → MATERIAL_KIND_GLASS
            // never classifies → opaque path).
            //
            // Child-first precedence (matches the texture / scalar walks):
            // first authored function > 0 wins. function == 0 (None)
            // intentionally does NOT clear an already-set blend — a leaf
            // that opts out shouldn't erase a parent's blend authoring.
            //
            // BGSM `src_blend` / `dst_blend` are already Gamebryo-native
            // values — `bgsm_blend_to_gamebryo` just narrows the `u32`
            // to the `u8` the renderer's blend-factor field expects, no
            // translation. See its doc for why (#1823, regression of a
            // wrong #1651 fix that assumed a GL-style enum requiring a
            // swap).
            if !set_blend && bgsm.base.alpha_blend_mode.function > 0 {
                mesh.has_alpha = true;
                mesh.src_blend_mode = bgsm_blend_to_gamebryo(bgsm.base.alpha_blend_mode.src_blend);
                mesh.dst_blend_mode = bgsm_blend_to_gamebryo(bgsm.base.alpha_blend_mode.dst_blend);
                set_blend = true;
                touched = true;
            }
        }
    } else if dispatch_kind == Some(MaterialKind::Bgem) {
        let Some(bgem) = provider.resolve_bgem(&path) else {
            return false;
        };
        // BGEM (effect material) has no smoothness/specular authoring —
        // metalness and roughness are left as NaN sentinels so resolve_pbr
        // runs the keyword classifier. glass_enabled surfaces get the glass
        // roughness override from classify_glass_into_material downstream.
        mesh.from_bgsm = true;
        touched = true;
        fill(
            &mut mesh.texture_path,
            &bgem.base_texture,
            &mut touched,
            pool,
        );
        fill(
            &mut mesh.normal_map,
            &bgem.normal_texture,
            &mut touched,
            pool,
        );
        fill(&mut mesh.glow_map, &bgem.glow_texture, &mut touched, pool);
        // #1453 — BGEM's grayscale_texture is the palette/gradient LUT for
        // effect materials (fire-gradient, electricity-gradient, magic VFX).
        // Forward it to the same `bgsm_greyscale_lut_path` field that BGSM
        // uses — both serve as the greyscale LUT path the renderer resolves
        // for `GreyscaleLutHandle` and the `EFFECT_PALETTE_COLOR` flag.
        if mesh.bgsm_greyscale_lut_path.is_none() && !bgem.grayscale_texture.is_empty() {
            mesh.bgsm_greyscale_lut_path = Some(bgem.grayscale_texture.clone());
            // #1580 — BGEM's own alpha-variant bool decides whether the LUT
            // gates EFFECT_PALETTE_ALPHA or the default EFFECT_PALETTE_COLOR;
            // see `pack_bgsm_material_flags` in `cell_loader.rs`.
            mesh.bgsm_greyscale_lut_is_alpha = bgem.grayscale_to_palette_alpha;
            touched = true;
        }
        fill(&mut mesh.env_map, &bgem.envmap_texture, &mut touched, pool);
        fill(
            &mut mesh.env_mask,
            &bgem.envmap_mask_texture,
            &mut touched,
            pool,
        );
        // #1076 / FO4-D6-002 SIBLING — BGEM also exposes
        // `specular_texture` + `lighting_texture` (the two BGSM v>2
        // slots that exist on the BGEM side too; BGEM does NOT
        // author `flow_texture` or `wrinkles_texture` per
        // `crates/bgsm/src/bgem.rs`). Forward them here so the BGEM
        // path has the same coverage as the BGSM path.
        fill(
            &mut mesh.specular_map,
            &bgem.specular_texture,
            &mut touched,
            pool,
        );
        fill(
            &mut mesh.lighting_map,
            &bgem.lighting_texture,
            &mut touched,
            pool,
        );

        // BGEM has no inheritance so there's no child-first chain.
        // `base_color × base_color_scale` is the primary effect tint —
        // the same authoring the NIF-side walker reads from
        // `BSEffectShaderProperty.base_color` / `base_color_scale`. Set
        // EmissiveSource::Effect so the renderer knows this slot is an
        // effect-diffuse tint, not a genuine emissive scalar. #1358.
        // `emittance_color` (v≥11 additive glow) is deferred until a
        // second emissive slot exists on `ImportedMesh`.
        mesh.emissive_color = bgem.base_color;
        mesh.emissive_mult = bgem.base_color_scale;
        mesh.emissive_source = byroredux_core::ecs::components::material::EmissiveSource::Effect;
        mesh.mat_alpha = bgem.base.alpha;
        mesh.uv_offset = [bgem.base.u_offset, bgem.base.v_offset];
        mesh.uv_scale = [bgem.base.u_scale, bgem.base.v_scale];
        if bgem.base.two_sided {
            mesh.two_sided = true;
        }
        if bgem.base.decal {
            mesh.is_decal = true;
        }
        if bgem.base.alpha_test {
            mesh.alpha_test = true;
            mesh.alpha_threshold = f32::from(bgem.base.alpha_test_ref) / 255.0;
        }
        // BGEM alpha-blend — same GL→Gamebryo translation as the BGSM
        // branch above, applied to the BSEffectShaderProperty path.
        // This is the path that hit #1651: additive glow/effect cards
        // author `(One, One)` = `(1, 1)` which, forwarded raw, the
        // renderer reads as `(ZERO, ZERO)` and renders invisible.
        // BGEM has no inheritance so no child-first guard needed.
        if bgem.base.alpha_blend_mode.function > 0 {
            mesh.has_alpha = true;
            mesh.src_blend_mode = bgsm_blend_to_gamebryo(bgem.base.alpha_blend_mode.src_blend);
            mesh.dst_blend_mode = bgsm_blend_to_gamebryo(bgem.base.alpha_blend_mode.dst_blend);
        }
        // #1280 sub-step 3b — forward BGEM glass semantics so the
        // spawn-time classifier in `helpers::classify_glass_into_material`
        // can fire the glass path even when neither the texture path nor
        // the mesh name carries a glass keyword. v21+ files expose the
        // direct `glass_enabled` field; older FO4 files use the equivalent
        // blend/depth/falloff/environment-map feature bundle recognized by
        // `bgem_uses_glass_behavior` (Port-A-Diner's v2 dome).
        if bgem_uses_glass_behavior(&bgem) {
            mesh.bgem_glass = true;
            // `non_occluder` is the authored behavioral distinction between
            // a thin transmissive shell (display dome/window sheet) and a
            // closed glass volume. Preserve it independently of BGEM so the
            // shared glass shader can choose a surface-consistent base path;
            // texture maps remain ordinary overlays either way.
            mesh.thin_glass = bgem_uses_thin_glass_behavior(&bgem);
        }
        // Soft-particle depth fade + view-angle falloff cone. The NIF
        // `BSEffectShaderProperty` path fills `mesh.effect_shader` from the
        // block; the BGEM path is the FO4+ equivalent and must mirror it so
        // `material_translate` can build `Material.{effect_falloff,
        // effect_shader_flags}` (soft_falloff_depth + MAT_FLAG_EFFECT_SOFT)
        // the same way. Without this every FO4 BGEM mist/steam/beam volume
        // (`soft = true` in the authored file) rendered with no depth feather
        // and stacked to an opaque white-out (HalluciGen labs). `lighting_influence`
        // is authored 0..1 in BGEM but carried 0..255 on the shared payload.
        mesh.effect_shader = Some(byroredux_nif::import::BsEffectShaderData {
            falloff_start_angle: bgem.falloff_start_angle,
            falloff_stop_angle: bgem.falloff_stop_angle,
            falloff_start_opacity: bgem.falloff_start_opacity,
            falloff_stop_opacity: bgem.falloff_stop_opacity,
            soft_falloff_depth: bgem.soft_depth,
            effect_soft: bgem.soft_enabled,
            effect_lit: bgem.effect_lighting_enabled,
            lighting_influence: (bgem.lighting_influence.clamp(0.0, 1.0) * 255.0).round() as u8,
            ..Default::default()
        });
        touched = true;
    } else {
        // Unknown extension — most likely a Starfield .mat JSON path that
        // SF-D3-01's suffix gate now correctly routes here. The .mat format
        // is not yet parsed (tracked in SF-D6-03). Log once per path so the
        // absence of material data is visible without spamming every frame.
        //
        // A `.mat` path only falls through to this generic arm when the
        // CDB-presence gate above (`has_starfield_cdb`) found no CDB
        // loaded — that's a real degradation (e.g. a future patch bumps
        // CDB fileVersion past the #1569 pins, or `--materials-ba2` was
        // omitted) already logged once, far earlier, in
        // `register_starfield_cdb`. SF3-02 / #1831 — name that cause
        // explicitly instead of the generic "unsupported format" message,
        // so an operator sees one clear degradation line rather than
        // per-mesh spam disconnected from the upstream CDB failure.
        static WARNED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
            std::sync::OnceLock::new();
        let mut set = WARNED
            .get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if set.insert(path.to_owned()) {
            log::warn!("{}", unresolved_material_warning(&path, provider.has_starfield_cdb()));
        }
        return false;
    }

    touched
}
