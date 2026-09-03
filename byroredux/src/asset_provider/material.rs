use super::*;

use byroredux_bgsm::template::ResolvedMaterial;
use byroredux_bgsm::{BgemFile, BgsmFile, TemplateCache, TemplateResolver};
use byroredux_nif::import::ImportedMaterial;
use byroredux_sfmaterial::{CdbHeaderInfo, ComponentDatabaseFile};
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

/// True for a Starfield component-database path — the base
/// `materials\materialsbeta.cdb` or any DLC/Creation-namespaced
/// `materials\creations\<plugin>\materialsbeta.cdb`. #1571 / SF-D3-03.
pub(crate) fn is_materialsbeta_cdb_path(path: &str) -> bool {
    let p = path.replace('/', "\\").to_ascii_lowercase();
    p.starts_with("materials\\") && p.ends_with("materialsbeta.cdb")
}

/// #1077 / FO4-D6-003 (Phase 1: data propagation) — forwards one BGSM
/// chain step's `translucency` / `model_space_normals` shader-flag bits
/// into `ImportedMaterial`. Same child-first precedence as every texture
/// slot in [`merge_external_material`]'s walk: first authored `true`
/// wins, so a step whose own flag is unset (`false`) doesn't clobber a
/// value an earlier (closer) step in the chain already set. Sets
/// `*touched = true` whenever it flips either flag.
///
/// Extracted out of the merge loop (#2702 / FO4-D2-03) so its three
/// regression tests call the real production logic instead of a
/// hand-copied mirror — the mirror was proven able to diverge silently
/// from `merge_external_material` (the FO4-D2-01 `is_pbr` contract flip
/// landed with a green mirror suite while its own comment kept stating
/// the pre-flip behaviour). #2700 restored the pre-flip contract itself
/// (see [`merge_external_material`]'s BGSM arm) — `is_pbr` is unconditional
/// on any successful BGSM resolve again, not gated on a `bgsm.pbr` bit
/// vanilla content essentially never sets.
pub(crate) fn forward_bgsm_phase1_flags(
    material: &mut ImportedMaterial,
    bgsm: &BgsmFile,
    touched: &mut bool,
) {
    if !material.has_translucency && bgsm.translucency {
        material.has_translucency = true;
        *touched = true;
    }
    if !material.model_space_normals && bgsm.model_space_normals {
        material.model_space_normals = true;
        *touched = true;
    }
}

/// #2607 (FO4-D7-02) — forward BGSM's rim / backlight / subsurface shading
/// scalars onto `ImportedMaterial`'s matching sinks.
///
/// The parser has decoded these since the crate existed, `ImportedMaterial`
/// has had `rimlight_power` / `backlight_power` / `subsurface_rolloff` since
/// #2284 wired the NIF-native path, and `translate_material` already forwards
/// all three onto the canonical `Material` — only this hop was missing, so
/// every BGSM-authored surface fed the Disney subsurface/rimlight lobe
/// hardcoded zeros. #1352 (unconditional `MAT_FLAG_PBR_BSDF` for BGSM
/// content) is what makes that visible rather than inert.
///
/// **Gated on the authored enable bits, deliberately.** The parser reads this
/// whole group only on the `version < 8` branch — the v>=8 layout spends those
/// bytes on the translucency suite instead — so on a modern BGSM `rim_power`
/// and `subsurface_lighting_rolloff` still hold their struct defaults of
/// 2.0 / 0.3. Those are never-parsed values, and forwarding them would be
/// fabrication, not translation. `rim_lighting` / `subsurface_lighting` are
/// false in exactly that case, which makes them the correct and complete gate.
///
/// `back_light_power` shares the rim enable bit: the format gives it none of
/// its own, and the Bethesda Material Editor authors it in the rim-lighting
/// group.
///
/// Child-first precedence via the caller's sentinels, matching every other
/// payload-carrying field in the walk. Extracted from the merge loop for the
/// same reason as [`forward_bgsm_phase1_flags`] (#2702): so the regression
/// tests drive the real production logic rather than a hand-copied mirror.
pub(crate) fn forward_bgsm_rim_subsurface(
    material: &mut ImportedMaterial,
    bgsm: &BgsmFile,
    set_rim: &mut bool,
    set_subsurface: &mut bool,
    touched: &mut bool,
) {
    if !*set_rim && (bgsm.rim_lighting || bgsm.back_lighting) {
        material.rimlight_power = bgsm.rim_power;
        material.backlight_power = bgsm.back_light_power;
        material.rim_lighting = bgsm.rim_lighting;
        material.back_lighting = bgsm.back_lighting;
        *set_rim = true;
        *touched = true;
    }
    if !*set_subsurface && bgsm.subsurface_lighting {
        material.subsurface_rolloff = bgsm.subsurface_lighting_rolloff;
        material.soft_lighting = true;
        *set_subsurface = true;
        *touched = true;
    }
}

/// #2608 (FO4-D7-03) — forward BGSM's authored env-map mask scale.
///
/// Same drop-at-the-merge-boundary class as [`forward_bgsm_rim_subsurface`]:
/// `merge_external_material` forwards env-map *textures* but was dropping the
/// scale that modulates them.
///
/// **Gated on `base.environment_mapping`, deliberately.**
/// `BaseMaterial::parse_after_magic` reads the `(environment_mapping,
/// environment_mapping_mask_scale)` pair only when `version < 10`; from v10
/// those bytes became `depth_bias` and the parser substitutes a synthetic
/// `(false, 1.0)`. Forwarding unconditionally would stamp `env_map_scale =
/// 1.0` onto every modern BGSM, and `Material::resolve_pbr`'s
/// `env_map_scale > 0.3` arm reads that as authored reflection intent — a
/// fabricated input driving real roughness on the majority of FO4 content.
/// The enable bit is false in precisely the never-parsed case.
///
/// (BGSM has no v>=10 re-read of this pair, so the base bit is the whole
/// story here. BGEM's `env_mapping_enabled()` accessor exists because BGEM
/// *does* re-read it in its own subclass section.)
pub(crate) fn forward_bgsm_env_map_scale(
    material: &mut ImportedMaterial,
    bgsm: &BgsmFile,
    set_env_map_scale: &mut bool,
    touched: &mut bool,
) {
    if !*set_env_map_scale && bgsm.base.environment_mapping {
        material.env_map_scale = bgsm.base.environment_mapping_mask_scale;
        *set_env_map_scale = true;
        *touched = true;
    }
}

/// Process-lifetime cache of Starfield CDB probe results, keyed by
/// `"<archive source>|<in-archive path>"`. #2705 (SF-D3-01) —
/// `build_material_provider` constructs a brand-new `MaterialProvider` (and
/// therefore a brand-new, empty `csg_cache`-shaped per-instance cache) on
/// every cell transition / save-load / debug-load, so caching *inside*
/// `MaterialProvider` wouldn't help here: the same CDB would still get
/// `archive.extract()`'d — a full zlib inflate of a multi-hundred-MB blob
/// for the vanilla `materialsbeta.cdb` (105 MB measured) — on every single
/// rebuild, even though `register_starfield_cdb` only reads the file's
/// 16-byte header and on-disk content never changes mid-session. Living at
/// module scope (outside `MaterialProvider`) lets this survive across
/// provider rebuilds instead of being discarded with the provider — the
/// Phase 1 only needs the header validity/count; retaining inflated bytes here
/// held every discovered CDB (233 MB across a Creation-heavy install) for the
/// process lifetime without a consumer. The cache stores that tiny result
/// instead, preserving #2705's skip-reextract behavior without the resident
/// blob. A cap keeps untrusted/modded archive sets from growing keys forever.
pub(super) const SF_CDB_CACHE_MAX_ENTRIES: usize = 128;
/// Every caller takes this lock as `.unwrap_or_else(|e| e.into_inner())`,
/// recovering rather than re-panicking on poison (#2398 — the deliberate
/// counterpart to the ECS layer's #466 fail-fast doctrine). The map is a pure
/// memoization of a re-derivable archive probe: the worst a torn entry can do
/// is hand back a header probe that gets re-read from the archive, and a
/// panicking material load must not permanently poison texture resolution for
/// every later mesh in the cell.
pub(super) fn sf_cdb_cache() -> &'static Mutex<HashMap<String, Option<CdbHeaderInfo>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<CdbHeaderInfo>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(super) fn sf_cdb_cache_insert(key: String, probe: Option<CdbHeaderInfo>) {
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
                    let probe = ComponentDatabaseFile::probe_header(&raw).ok();
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
    // #2626 / SF-D9-2026-08-07-01 — `base.refraction` used to short-circuit
    // this too. It's a shared BaseMaterial screen-distortion bit authored
    // on heat shimmer, cloaking shells, force-field ripple, and fire/plasma
    // distortion — none of which are glass — and unlike `glass_enabled`
    // (a v21+ field authored specifically to mean glass) it's neither
    // version-gated nor bundled with any of the other glass-shaped
    // conjuncts below. Checking it unconditionally fired on v2 through v22
    // alike, demoting correctly-classified effect-shader content (the
    // #2297 fire-refraction corpus) to MATERIAL_KIND_GLASS.
    if bgem.glass_enabled {
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
    let reflective_surface_maps = bgem.env_mapping_enabled()
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

    /// Validate + register a Starfield `materialsbeta.cdb` payload for the
    /// presence gate — `discover_starfield_cdbs` calls this once per CDB
    /// found across the loaded archives (#1571). Runs a `peek_magic` cheap
    /// reject (SF-D3-AUDIT-03 / #2102) then a header-only
    /// [`ComponentDatabaseFile::probe_header`] validity check
    /// (SF-D3-AUDIT-01 / #2100) and bumps `sf_cdb_count` on success — it
    /// does NOT walk or retain the ~1.44M-entry instance tree (see the
    /// `sf_cdb_count` field doc). A malformed payload is warned and
    /// dropped, leaving the count intact. #1289 / SF-D3-NEW-01.
    #[cfg_attr(not(test), allow(dead_code))]
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
                self.register_starfield_cdb_probe(info);
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

    fn register_starfield_cdb_probe(&mut self, _info: CdbHeaderInfo) {
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
            if self.failed_paths.len() >= MAX_FAILED_PATHS {
                for _ in 0..MAX_FAILED_PATHS / 2 {
                    if let Some(old) = self.failed_paths_order.pop_front() {
                        self.failed_paths.remove(&old);
                    }
                }
            }
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
fn apply_cdb_pbr_fallback(material: &mut ImportedMaterial, path: &str) -> MergeOutcome {
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

/// What [`merge_external_material`] actually did, for the caller and for
/// diagnostics.
///
/// #2709 (SF-D9-03) — replaces a bare `bool` whose doc claimed it "flips
/// to `true` on any merged field". That was false for the Starfield `.mat`
/// arm, which returns success having set only `is_pbr` and forwarded no
/// texture, scalar, or alpha state at all. The two cases are visually
/// identical downstream (a mesh with engine defaults), so collapsing them
/// into one `true` left no signal anywhere distinguishing "this cell's
/// materials resolved" from "this cell's materials resolved to nothing" —
/// precisely the state the dominant Starfield population is in, and the
/// reason a total material blackout produces no actionable diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MergeOutcome {
    /// No `material_path`, an unresolvable pool symbol, a path in no
    /// loaded archive, a parse failure, or an unrecognised extension.
    /// Nothing was written; the mesh keeps its NIF-derived material.
    Unresolved,
    /// The sidecar was recognised and confirmed present, but the merge
    /// forwarded no authored field — only a routing flag. Today this is
    /// exactly the Starfield `.mat` arm: the CDB-presence gate flips
    /// `is_pbr` so the mesh takes the Disney lobe, and per-field
    /// extraction from the Component Database is the deferred Phase 2
    /// (#1289 / #2359). A caller counting "materials that actually
    /// supplied data" must NOT count this.
    PresenceOnly,
    /// At least one authored field (texture slot, scalar, alpha/blend
    /// state, or shader flag) was forwarded onto the `ImportedMaterial`.
    Merged,
}

// Consumed by the merge regression tests today; no production caller
// reads the outcome yet (all four discard it explicitly — see their
// `let _ =` sites). Kept as the type's API rather than inlined into the
// tests so the deferred per-cell "materials resolved / of which
// presence-only" telemetry sink #2709 asks for has something to call,
// and so a future reader doesn't reach for `== MergeOutcome::Merged`
// spelled out at each site.
#[cfg_attr(not(test), allow(dead_code))]
impl MergeOutcome {
    /// True when the sidecar resolved at all, whether or not it supplied
    /// any authored field. This is the old `bool` return's meaning —
    /// use it only where "did the path resolve" is genuinely the
    /// question, not as a proxy for "did the mesh get material data".
    pub(crate) fn resolved(self) -> bool {
        !matches!(self, MergeOutcome::Unresolved)
    }

    /// True only when authored data actually landed on the material.
    pub(crate) fn merged(self) -> bool {
        matches!(self, MergeOutcome::Merged)
    }
}

/// Merge a BGSM, BGEM, or Starfield `.mat` sidecar into the
/// source-normalized NIF material payload.
///
/// NIF fields take precedence — only empty slots are filled from the
/// resolved material chain, matching Bethesda's runtime behaviour where
/// the shader property overrides template defaults per-material. For BGSM
/// the template chain is walked child-first (first non-empty value for a
/// given field wins); BGEM has no inheritance (the format carries no
/// `root_material_path`) so the single parsed file is read.
///
/// This boundary deliberately accepts [`ImportedMaterial`] rather than an
/// [`byroredux_nif::import::ImportedMesh`]: external formats can patch material
/// semantics, but cannot mutate geometry, transforms, skinning, or scene
/// ownership.
///
/// See [`MergeOutcome`] for what the return value distinguishes and why
/// it is not a `bool` (#2709 / SF-D9-03).
#[must_use = "a PresenceOnly merge resolved the sidecar but forwarded no authored \
              field — discarding the outcome erases the only signal distinguishing \
              it from a fully-populated merge (#2709)"]
pub(crate) fn merge_external_material(
    material: &mut ImportedMaterial,
    provider: &mut MaterialProvider,
    pool: &mut byroredux_core::string::StringPool,
) -> MergeOutcome {
    let Some(path_sym) = material.material_path else {
        return MergeOutcome::Unresolved;
    };
    // `StringPool::resolve` returns the canonical lowercased form, so
    // we own the string for the BGSM dispatch + suffix matching here
    // without an extra `to_ascii_lowercase` allocation. See #609.
    let path: String = match pool.resolve(path_sym) {
        Some(s) => s.to_string(),
        None => return MergeOutcome::Unresolved,
    };

    // `touched` flips to `true` on any merged AUTHORED field — it is what
    // separates `MergeOutcome::Merged` from `PresenceOnly` at the returns
    // below, so it must NOT be set by a routing-only flag flip. Allowed
    // unused assignment: the BGSM / BGEM success branches set it
    // unconditionally alongside `material.from_bgsm = true`, so the
    // `false` initializer is overwritten before any read there — but the
    // initializer is load-bearing for the failure / unknown-kind path.
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
    // Phase 1 (this commit): flip `material.is_pbr = true` so
    // `pack_imported_material_flags` packs `MAT_FLAG_PBR_BSDF` and
    // `triangle.frag` routes Starfield content through the Disney BSDF
    // path instead of the legacy Lambert + simple-GGX path (the audit
    // FAIL closure). Defaults for metalness / roughness / textures
    // stay at the NIF-derived values — better than Lambert but still
    // approximate; Phase 2 will walk the CDB to extract authored values.
    //
    // The CDB-presence check prevents accidental PBR routing for modded
    // sidecars against non-Starfield archives. Starfield's shipped NIFs use
    // `.bgsm`/`.bgem`-named references even though no such files exist in its
    // archives (#3053), so those names are in scope for it too — but as a
    // *fallback* reached on a resolve miss, not as a gate that pre-empts the
    // resolvers (#3230). Only `.mat` short-circuits.
    let starfield_named_material =
        path.ends_with(".mat") || path.ends_with(".bgsm") || path.ends_with(".bgem");
    let starfield_cdb_gate = starfield_named_material && provider.has_starfield_cdb();

    // `.mat` short-circuits: vanilla Starfield ships no `.mat`/`.bgsm`/
    // `.bgem` sidecars, but an installed Creation/mod archive can — 20 JSON
    // `.mat` exports measured across 129 installed archives (2026-08-30).
    // The short-circuit is retained anyway because no JSON `.mat` resolver
    // exists yet, not because the files cannot exist. See
    // [`apply_cdb_pbr_fallback`] for the full rationale and for why the
    // `.bgsm`/`.bgem` names do NOT short-circuit here any more (#3230).
    //
    // `from_bgsm` is deliberately left unset by that helper — the flag
    // gates BGSM spec-glossiness translation (an FO4 format convention).
    // Starfield `.mat` authors metalness/roughness directly, and this arm
    // forwards neither: NIF import (`bs_geometry.rs` / `bs_tri_shape.rs` /
    // `import::material::classify_legacy_pbr`) already ran the keyword
    // classifier on `metalness_override` / `roughness_override` before
    // this function was called.
    //
    // #2707 (SF-D8-01) fixed what those overrides actually are for the
    // DOMINANT Starfield case (a `material_reference` stub — 97.9% of
    // sampled meshes): pre-fix, `classify_legacy_pbr` ran on an
    // all-defaults `MaterialInfo` (the walker returns before writing any
    // field for a stub) and unconditionally stamped its terminal
    // `Some(0.0)/Some(0.85)` fallback anyway, permanently disabling
    // `Material::resolve_pbr`'s NaN-sentinel backstop. Post-fix, a stub
    // with no classifier signal at all leaves both overrides `None`, so
    // the NaN sentinel DOES reach `resolve_pbr` — which re-runs the same
    // classifier against whatever real texture / normal-map / env-map-scale
    // data has been merged in BY THEN (this function's own BGSM/BGEM/`.mat`
    // resolution included), rather than the empty snapshot the importer
    // saw. Non-stub Starfield meshes (inline shader data present) still
    // arrive with real `Some(...)` overrides, unchanged.
    //
    // Phase 2 (#3398, CDB per-field extraction) should *overwrite* whichever
    // value is present here with CDB-authored data when a lookup succeeds; a
    // lookup MISS correctly falls through to the sentinel/classifier fallback
    // instead of silently keeping a fabricated constant.
    //
    // #2709 (SF-D9-03) — `PresenceOnly`, not `Merged`: this arm sets exactly
    // one routing flag and forwards no authored field. Phase 2 should return
    // `Merged` once a CDB lookup actually supplies data.
    if starfield_cdb_gate && path.ends_with(".mat") {
        return apply_cdb_pbr_fallback(material, &path);
    }

    // #3230 — for `.bgsm`/`.bgem` names the CDB flip is a *fallback*, not a
    // gate. `starfield_cdb_gate` is non-`.mat` by construction past the
    // early return above, so this is exactly "a Starfield session named a
    // sidecar we should try to parse first". Consumed at each resolve-miss
    // site below.
    let cdb_pbr_fallback = starfield_cdb_gate;

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
    // #3507 — `tile_u`/`tile_v` (BGSM's texture-address authoring channel)
    // dropped on the floor, same shape as the NIF-side arm this issue
    // paired with.
    let mut set_clamp_mode = false;
    let mut set_blend = false;
    let mut set_fresnel = false;
    let mut set_palette_scale = false;
    // #2607 — the v<8 rim / backlight / subsurface group. Backlight shares
    // `set_rim`: the format gives it no enable bit of its own.
    let mut set_rim = false;
    let mut set_subsurface = false;
    // #2608 — env-map mask scale, authored only on the v<10 base layout.
    let mut set_env_map_scale = false;
    // #2212 (NIFAL-D8-01) — chain-local, unlike `material.alpha_test`
    // itself. The NIF F4SF2 bit-25 path (`dedicated_shader.rs`) can
    // pre-set `material.alpha_test = true` before this loop ever runs, so
    // gating the threshold payload on `!material.alpha_test` (as the code
    // used to) let that lower-priority NIF-synthesized default block the
    // authored BGSM `alpha_test_ref` from ever landing. Track "has a BGSM
    // in THIS chain already set the threshold" separately from the
    // OR'd boolean, matching every other payload-carrying field above.
    let mut set_alpha_test = false;

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
            // #3230 — a Starfield session reaches here having genuinely
            // tried and missed, which is the state the CDB flip describes.
            // Taking it BEFORE the diagnostic below is deliberate: that
            // warning's whole premise ("keeps its NIF-native keyword-
            // classified material") is false once the flip runs.
            if cdb_pbr_fallback {
                return apply_cdb_pbr_fallback(material, &path);
            }
            // #2601 — `resolve_bgsm` already logged WHY the resolve failed
            // (missing archive entry, parse error, template-cycle recovery
            // failure — see its own `log::warn!` sites) and recorded `path`
            // into `failed_paths` so repeat failures don't re-spam the log.
            // What neither of those says is the CONSEQUENCE decided right
            // here: this mesh keeps whatever `into_imported_material`
            // (crates/nif) already guessed from the NIF-native keyword
            // classifier — visually indistinguishable from a material that
            // was deliberately authored non-PBR. That's the documented root
            // cause of the recurring "chrome/posterized FO4 surface" class
            // (see feedback_chrome_means_missing_textures) — a broken BGSM
            // reference silently looks identical to intentional legacy
            // authoring. Logging the causal link at the point it's decided
            // means grepping for "keeps its keyword-classified fallback"
            // finds every mesh affected in one search, instead of having to
            // correlate `resolve_bgsm`'s low-level reason against which
            // REFRs actually dispatched as BGSM.
            log::warn!(
                "material '{}': BGSM resolve failed — mesh keeps its NIF-native \
                 keyword-classified material (legacy Lambert guess) instead of \
                 the authoritative BGSM PBR data",
                path
            );
            return MergeOutcome::Unresolved;
        };
        // BGSM resolution succeeded — telemetry-only flag (no renderer
        // branch); the substantive work happens in the spec-glossiness
        // → metallic-roughness translation below.
        material.from_bgsm = true;
        touched = true;
        // #1352 — any successful BGSM resolve routes the material through
        // the Disney/PBR diffuse lobe, unconditionally. #2700 (FO4-D2-01):
        // `a0f75fc5` narrowed this to `resolved.walk().any(|s| s.file.pbr)`
        // — "BGSM is a container, not a BRDF declaration" — measured at 0
        // of 6,616 vanilla FO4 BGSMs, so the narrower gate silently
        // reverted #1352 across every vanilla FO4 surface with a green
        // test suite covering the divergence. It also went unnoticed that
        // this makes `forward_bgsm_rim_subsurface`'s rim/backlight/
        // subsurface scalars (#2607) dead weight for the same 100% of
        // content: that lobe is the only consumer of those fields, and it
        // was never selected. The metalness/roughness/F0 derivation just
        // below already treats every BGSM material as physically based
        // regardless of the `pbr` bit — that bit only changes HOW
        // metalness is derived (spec-color-as-F0 vs spec-color
        // chromaticity), never WHETHER the material is PBR — so gating
        // only the diffuse lobe on a bit real content essentially never
        // sets was an internally inconsistent narrower contract, not a
        // deliberate one (the `a0f75fc5` commit message never mentions
        // PBR routing). Restored to #1352's original, still-documented
        // intent (see the sibling tests in `tests/bgsm_merge.rs`).
        material.is_pbr = true;

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
        // For metallic materials, also tint `material.diffuse_color` toward
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
        material.metalness_override = Some(metalness);
        material.roughness_override = Some(roughness);
        // #2609 — the flag whose meaning is "authoritative PBR scalars were
        // merged", set at the exact site that merges them. `from_bgsm` above
        // cannot serve that role: the BGEM arm sets it too while leaving both
        // overrides `None` (BGEM authors no smoothness/specular), so a
        // consumer reading `from_bgsm` as "scalars present" is wrong on every
        // effect material. Keep this write adjacent to the two it describes.
        material.bgsm_pbr_scalars_authored = true;
        if metalness > 0.5 {
            // #1591 — blend toward the mult-free `specular_color`, NOT
            // `spec_*` (= specular_color × specular_mult); the mult-bearing
            // `spec_*` stays for the pbr F0-luminance path above where
            // mult-as-scale is correct. See `conductor_diffuse_tint`.
            material.diffuse_color =
                conductor_diffuse_tint(material.diffuse_color, leaf.specular_color);
        }
        for step in resolved.walk() {
            let bgsm = &step.file;
            fill(
                &mut material.textures.base_color,
                &bgsm.diffuse_texture,
                &mut touched,
                pool,
            );
            fill(
                &mut material.textures.normal,
                &bgsm.normal_texture,
                &mut touched,
                pool,
            );
            fill(
                &mut material.textures.emissive,
                &bgsm.glow_texture,
                &mut touched,
                pool,
            );
            // Smoothness/spec mask — .r encodes per-texel specular
            // strength in the engine's existing gloss_map slot. #453.
            fill(
                &mut material.textures.smooth_spec,
                &bgsm.smooth_spec_texture,
                &mut touched,
                pool,
            );
            // #1353 / FO4-D8-07 — BGSM greyscale-to-palette LUT path
            // (`SLSF1::Greyscale_To_PaletteColor`, used by FO4 NPC /
            // creature colour variants; the palette slot is authored on
            // v<=2 BGSMs). First non-empty in the template chain wins, to
            // match the texture fills above. Routed through the common
            // greyscale_lut role and flagged via EFFECT_PALETTE_COLOR in
            // `pack_imported_material_flags` so the lit-path remap samples it.
            //
            // #2108 (SF-D9-01) — the greyscale slot is a legal, always-
            // serialized field; its presence alone does NOT mean the
            // material wants the remap. Capture the authoritative
            // `grayscale_to_palette_color` enable bit from THIS SAME BGSM
            // (not `fill`'s generic helper, and not OR'd across the whole
            // chain) at the exact step that supplies the texture — an
            // ancestor's own enable bit is irrelevant once a closer BGSM
            // already won the texture slot.
            if material.textures.greyscale_lut.is_none() && !bgsm.greyscale_texture.is_empty() {
                material.bgsm_greyscale_lut_enabled = bgsm.base.grayscale_to_palette_color;
                // #2643 — BGSM has no alpha-variant field, so the color
                // bit is the only one this format can author.
                material.bgsm_greyscale_lut_color = bgsm.base.grayscale_to_palette_color;
            }
            fill(
                &mut material.textures.greyscale_lut,
                &bgsm.greyscale_texture,
                &mut touched,
                pool,
            );
            // Legacy v <= 2 environment cube; newer BGSMs drop the slot.
            fill(
                &mut material.textures.environment,
                &bgsm.envmap_texture,
                &mut touched,
                pool,
            );
            fill(
                &mut material.textures.height,
                &bgsm.displacement_texture,
                &mut touched,
                pool,
            );
            // #2627 / SF-D9-2026-08-07-02 — the v<=2 legacy texture list
            // reads envmap, glow, inner_layer, wrinkles, displacement (see
            // `bgsm.rs`'s parser comment); this was the one slot in that
            // set the merge never forwarded, even though the sink is a
            // live, populated role — the NIF `BSLightingShaderProperty`
            // multi-layer-parallax path already resolves
            // `MaterialTextureSet::inner_layer` to a real texture handle.
            // A BGSM authoring its inner layer externally (Skyrim SE
            // ice/glass, FO4 layered panes) rendered with the layer
            // silently absent.
            fill(
                &mut material.textures.inner_layer,
                &bgsm.inner_layer_texture,
                &mut touched,
                pool,
            );
            // #1076 / FO4-D6-002 — BGSM v>2 standalone slots that
            // pre-fix were parsed but dropped on the floor. Each is
            // empty on the v<=2 path (the parser leaves the String
            // default) so the `fill` no-op suffices to gate the
            // forward without an explicit version check.
            fill(
                &mut material.textures.specular,
                &bgsm.specular_texture,
                &mut touched,
                pool,
            );
            fill(
                &mut material.textures.lighting,
                &bgsm.lighting_texture,
                &mut touched,
                pool,
            );
            fill(
                &mut material.textures.flow,
                &bgsm.flow_texture,
                &mut touched,
                pool,
            );
            fill(
                &mut material.textures.wrinkle,
                &bgsm.wrinkles_texture,
                &mut touched,
                pool,
            );
            // #2642 (SF-D9-2026-08-07-03) — `bgsm.distance_field_alpha_texture`
            // (v>=17, FO76/Starfield-era) is deliberately NOT forwarded here.
            // `MaterialTextureSet` has no dedicated role for it — a genuine
            // deferred-consumer gap, not a wiring bug, unlike every other
            // texture slot in this block. Signage/decal cutouts authored
            // with distance-field alpha fall back to plain alpha test until
            // a role + shader consumer exist.
            // #1077 / FO4-D6-003 (Phase 1: data propagation) — BGSM-only
            // shader flags, extracted to `forward_bgsm_phase1_flags`
            // (#2702 / FO4-D2-03) so its regression tests exercise this
            // exact call, not a hand-copied mirror.
            forward_bgsm_phase1_flags(material, bgsm, &mut touched);

            // #1147 Phase 2b — BGSM v>=8 translucency suite. Same
            // child-first precedence as the flags above. The
            // `has_translucency` flag is the gate; if the child
            // already set it, the corresponding subsurface params
            // also came from the child and we don't overwrite them.
            // If `has_translucency` is set by this chain entry but
            // the params are still at default-zero, propagate them.
            if bgsm.translucency
                && material.translucency_transmissive_scale == 0.0
                && material.translucency_subsurface_color == [0.0; 3]
            {
                material.translucency_subsurface_color = bgsm.translucency_subsurface_color;
                material.translucency_transmissive_scale = bgsm.translucency_transmissive_scale;
                material.translucency_turbulence = bgsm.translucency_turbulence;
                material.translucency_thick_object = bgsm.translucency_thick_object;
                material.translucency_mix_albedo =
                    bgsm.translucency_mix_albedo_with_subsurface_color;
                touched = true;
            }

            // Scalar PBR forwarding (#583). Child-first: first authored
            // value wins. Parser already decodes these fields; the
            // pre-fix merge dropped them on the floor.
            if !set_emissive && bgsm.emit_enabled {
                material.emissive_color = bgsm.emittance_color;
                material.emissive_mult = bgsm.emittance_mult;
                set_emissive = true;
                touched = true;
            }
            if !set_specular {
                material.specular_color = bgsm.specular_color;
                material.specular_strength = bgsm.specular_mult;
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
                material.glossiness = bgsm.smoothness * 100.0;
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
                material.fresnel_power = bgsm.fresnel_power;
                set_fresnel = true;
                touched = true;
            }
            // #1455 — BGSM authors greyscale-to-palette scale. Child-first.
            // Modulates the LUT remap intensity for NPC creature colour
            // variants (deathclaw, supermutant). Default 1.0 = no change.
            if !set_palette_scale {
                material.grayscale_to_palette_scale = bgsm.grayscale_to_palette_scale;
                set_palette_scale = true;
                touched = true;
            }
            // #2607 / #2608 (FO4-D7-02 / FO4-D7-03) — two more BGSM field
            // groups that decoded correctly and were dropped at this exact
            // hop. Both are enable-bit gated; see the fn docs for why
            // unconditional forwarding would be fabrication rather than
            // translation.
            forward_bgsm_rim_subsurface(
                material,
                bgsm,
                &mut set_rim,
                &mut set_subsurface,
                &mut touched,
            );
            forward_bgsm_env_map_scale(material, bgsm, &mut set_env_map_scale, &mut touched);
            if !set_alpha {
                material.mat_alpha = bgsm.base.alpha;
                set_alpha = true;
                touched = true;
            }
            if !set_uv {
                material.uv_offset = [bgsm.base.u_offset, bgsm.base.v_offset];
                material.uv_scale = [bgsm.base.u_scale, bgsm.base.v_scale];
                set_uv = true;
                touched = true;
            }
            // #3507 — `tile_u`/`tile_v` are BGSM's own texture-address-mode
            // authoring channel (`base.rs:174-175` decodes them from the
            // same bit-packed byte nif.xml's `TexClampMode` enum uses:
            // bit 1 = S-axis wrap, bit 0 = T-axis wrap), separate from and
            // in addition to the NIF shader property's `texture_clamp_mode`
            // field this issue's sibling fix restores.
            if !set_clamp_mode {
                material.texture_clamp_mode =
                    ((bgsm.base.tile_u as u8) << 1) | (bgsm.base.tile_v as u8);
                set_clamp_mode = true;
                touched = true;
            }
            // Boolean gameplay flags OR across the template chain — if
            // ANY ancestor marks the material as two-sided / decal /
            // alpha-test, the concrete instance is too.
            if bgsm.base.two_sided {
                material.two_sided = true;
                touched = true;
            }
            if bgsm.base.decal {
                material.is_decal = true;
                touched = true;
            }
            // #2212 (NIFAL-D8-01) — the boolean itself stays a pure OR
            // across the chain (matches the `two_sided` / `decal` siblings
            // above and the doc comment's stated policy), but the
            // threshold payload uses the chain-local `set_alpha_test`
            // sentinel, not `material.alpha_test`'s value, so a NIF
            // F4SF2-bit-25-synthesized default (pre-set before this loop
            // runs) can never outrank the authored BGSM `alpha_test_ref`.
            if bgsm.base.alpha_test {
                material.alpha_test = true;
                if !set_alpha_test {
                    material.alpha_threshold = f32::from(bgsm.base.alpha_test_ref) / 255.0;
                    set_alpha_test = true;
                }
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
                material.has_alpha = true;
                material.src_blend_mode =
                    bgsm_blend_to_gamebryo(bgsm.base.alpha_blend_mode.src_blend);
                material.dst_blend_mode =
                    bgsm_blend_to_gamebryo(bgsm.base.alpha_blend_mode.dst_blend);
                set_blend = true;
                touched = true;
            }
            // #2704 (FO4-D7-02) — Deferred: no consumer. These eleven BGSM
            // scalars decode correctly on the parser side (`bgsm.rs`) but
            // have no `ImportedMaterial` sink here, same deferred-consumer
            // class as the BGEM v21+/v22 glass-overlay suite above:
            //   * the entire wetness-control suite — `wetness_control_spec_scale`,
            //     `wetness_control_spec_power_scale`, `wetness_control_spec_min_var`,
            //     `wetness_control_env_map_scale`, `wetness_control_fresnel_power`,
            //     `wetness_control_metalness` — the authored input the ROADMAP
            //     M61 wet-surface feature would need
            //   * `custom_porosity`, `porosity_value` (v>=9 porosity pair)
            //   * `adaptive_emissive_exposure_offset` (v>=13 adaptive-emissive tuning)
            //   * `aniso_lighting`
            //   * `external_emittance`
            // No runtime effect today; flagging so the next completeness
            // sweep can tell "not yet wired" from "overlooked".
        }
    } else if dispatch_kind == Some(MaterialKind::Bgem) {
        let Some(bgem) = provider.resolve_bgem(&path) else {
            // #3230 — sibling of the BGSM arm's fallback above.
            if cdb_pbr_fallback {
                return apply_cdb_pbr_fallback(material, &path);
            }
            // #2601 — sibling of the BGSM arm's diagnostic above. Same
            // consequence: this mesh keeps the NIF-native keyword-
            // classified fallback instead of authoritative BGEM data.
            log::warn!(
                "material '{}': BGEM resolve failed — mesh keeps its NIF-native \
                 keyword-classified material (legacy Lambert guess) instead of \
                 the authoritative BGEM data",
                path
            );
            return MergeOutcome::Unresolved;
        };
        // BGEM (effect material) has no smoothness/specular authoring —
        // metalness and roughness are left as NaN sentinels so resolve_pbr
        // runs the keyword classifier. glass_enabled surfaces get the glass
        // roughness override from classify_glass_into_material downstream.
        material.from_bgsm = true;
        // #2366 — v20+ BGEMs can explicitly opt into the PBR specular
        // workflow. Preserve an existing true value and promote false only
        // when the parsed effect-material flag requests it.
        material.is_pbr |= bgem.effect_pbr_specular;
        touched = true;
        fill(
            &mut material.textures.base_color,
            &bgem.base_texture,
            &mut touched,
            pool,
        );
        fill(
            &mut material.textures.normal,
            &bgem.normal_texture,
            &mut touched,
            pool,
        );
        fill(
            &mut material.textures.emissive,
            &bgem.glow_texture,
            &mut touched,
            pool,
        );
        // #1453 — BGEM's grayscale_texture is the palette/gradient LUT for
        // effect materials (fire-gradient, electricity-gradient, magic VFX).
        // Forward it to the same common greyscale_lut role BGSM uses — both
        // resolve through MaterialTextureHandles and the
        // `EFFECT_PALETTE_COLOR` flag.
        if material.textures.greyscale_lut.is_none() && !bgem.grayscale_texture.is_empty() {
            material.textures.greyscale_lut = Some(pool.intern(&bgem.grayscale_texture));
            // #1580 / #2643 — BGEM's own alpha-variant bool and the shared
            // color bit are independent authoring (the format permits
            // setting both at once), so track them as two separate flags
            // and let `pack_imported_material_flags` OR both
            // EFFECT_PALETTE_COLOR / EFFECT_PALETTE_ALPHA in independently
            // — see `pack_imported_material_flags` in `cell_loader.rs`.
            // Previously `bgsm_greyscale_lut_is_alpha` alone decided
            // COLOR-vs-ALPHA, which silently dropped the color variant
            // whenever a BGEM authored both bits.
            material.bgsm_greyscale_lut_is_alpha = bgem.grayscale_to_palette_alpha;
            material.bgsm_greyscale_lut_color = bgem.base.grayscale_to_palette_color;
            // #2108 (SF-D9-01) — either enable bit (the shared
            // `grayscale_to_palette_color`, or BGEM's alpha-variant
            // `grayscale_to_palette_alpha`) turns the remap on; which of
            // COLOR/ALPHA the packer sets is decided separately, above, by
            // `bgsm_greyscale_lut_color` / `bgsm_greyscale_lut_is_alpha`.
            // The texture slot being filled is not itself an enable signal.
            material.bgsm_greyscale_lut_enabled =
                bgem.base.grayscale_to_palette_color || bgem.grayscale_to_palette_alpha;
            touched = true;
        }
        // #2643 (SF-D9-2026-08-07-04) — gate the envmap texture fill on
        // the authored `env_mapping_enabled()` bit, the version-aware
        // accessor `bgem_uses_glass_behavior` above already consults
        // (`reflective_surface_maps`, #2358). Previously this filled
        // unconditionally from `envmap_texture`/`envmap_mask_texture`
        // regardless of whether the material actually enabled env
        // mapping, so the same authored bit was honoured for glass
        // classification and ignored for texture binding within one
        // file — a BGEM with a stale/unused envmap slot but the enable
        // bit off would still bind it.
        if bgem.env_mapping_enabled() {
            fill(
                &mut material.textures.environment,
                &bgem.envmap_texture,
                &mut touched,
                pool,
            );
            fill(
                &mut material.textures.environment_mask,
                &bgem.envmap_mask_texture,
                &mut touched,
                pool,
            );
        }
        // #1076 / FO4-D6-002 SIBLING — BGEM also exposes
        // `specular_texture` + `lighting_texture` (the two BGSM v>2
        // slots that exist on the BGEM side too; BGEM does NOT
        // author `flow_texture` or `wrinkles_texture` per
        // `crates/bgsm/src/bgem.rs`). Forward them here so the BGEM
        // path has the same coverage as the BGSM path.
        fill(
            &mut material.textures.specular,
            &bgem.specular_texture,
            &mut touched,
            pool,
        );
        fill(
            &mut material.textures.lighting,
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
        // #3371 (SKY-2026-08-27-D7-03) — the fourth writer of
        // `emissive_source`, and the one #2591 missed. Gate it on the
        // same `emissive_contribution_is_authored` predicate as the three
        // NIF-side sites: a BGEM with `base_color == [0,0,0]` or
        // `base_color_scale == 0.0` authored no contribution, and tagging
        // it `Effect` regardless degenerates the discriminator to "has an
        // effect shader" — exactly what #2591 fixed elsewhere.
        material.emissive_color = bgem.base_color;
        material.emissive_mult = bgem.base_color_scale;
        if byroredux_core::ecs::components::material::emissive_contribution_is_authored(
            material.emissive_color,
            material.emissive_mult,
        ) {
            material.emissive_source =
                byroredux_core::ecs::components::material::EmissiveSource::Effect;
        }
        material.mat_alpha = bgem.base.alpha;
        material.uv_offset = [bgem.base.u_offset, bgem.base.v_offset];
        material.uv_scale = [bgem.base.u_scale, bgem.base.v_scale];
        // #3507 — same `tile_u`/`tile_v` → `texture_clamp_mode` mapping as
        // the BGSM chain above; BGEM shares `BaseMaterial` and has no
        // inheritance, so this is an unconditional write like its
        // `uv_offset`/`uv_scale` neighbours.
        material.texture_clamp_mode = ((bgem.base.tile_u as u8) << 1) | (bgem.base.tile_v as u8);
        if bgem.base.two_sided {
            material.two_sided = true;
        }
        if bgem.base.decal {
            material.is_decal = true;
        }
        if bgem.base.alpha_test {
            material.alpha_test = true;
            material.alpha_threshold = f32::from(bgem.base.alpha_test_ref) / 255.0;
        }
        // BGEM alpha-blend — same GL→Gamebryo translation as the BGSM
        // branch above, applied to the BSEffectShaderProperty path.
        // This is the path that hit #1651: additive glow/effect cards
        // author `(One, One)` = `(1, 1)` which, forwarded raw, the
        // renderer reads as `(ZERO, ZERO)` and renders invisible.
        // BGEM has no inheritance so no child-first guard needed.
        if bgem.base.alpha_blend_mode.function > 0 {
            material.has_alpha = true;
            material.src_blend_mode = bgsm_blend_to_gamebryo(bgem.base.alpha_blend_mode.src_blend);
            material.dst_blend_mode = bgsm_blend_to_gamebryo(bgem.base.alpha_blend_mode.dst_blend);
        }
        // #1280 sub-step 3b — forward BGEM glass semantics so the
        // spawn-time classifier in `helpers::classify_glass_into_material`
        // can fire the glass path even when neither the texture path nor
        // the mesh name carries a glass keyword. v21+ files expose the
        // direct `glass_enabled` field; older FO4 files use the equivalent
        // blend/depth/falloff/environment-map feature bundle recognized by
        // `bgem_uses_glass_behavior` (Port-A-Diner's v2 dome).
        if bgem_uses_glass_behavior(&bgem) {
            material.bgem_glass = true;
            // `non_occluder` is the authored behavioral distinction between
            // a thin transmissive shell (display dome/window sheet) and a
            // closed glass volume. Preserve it independently of BGEM so the
            // shared glass shader can choose a surface-consistent base path;
            // texture maps remain ordinary overlays either way.
            material.thin_glass = bgem_uses_thin_glass_behavior(&bgem);
            // BGEM v21+/v22 glass-overlay suite. These are semantic glass
            // inputs, not generic detail/specular maps: preserve dedicated
            // roles so a material that authors both cannot overwrite one.
            material.glass_fresnel_color = bgem.glass_fresnel_color;
            material.glass_refraction_scale = bgem.glass_refraction_scale_base;
            material.glass_blur_scale = bgem.glass_blur_scale_base;
            material.glass_blur_scale_factor = bgem.glass_blur_scale_factor;
            fill(
                &mut material.textures.glass_roughness_scratch,
                &bgem.glass_roughness_scratch,
                &mut touched,
                pool,
            );
            fill(
                &mut material.textures.glass_dirt_overlay,
                &bgem.glass_dirt_overlay,
                &mut touched,
                pool,
            );
            //
            // #2608 correction: `environment_mapping_mask_scale` was listed
            // above as sink-less, and no longer is —
            // `ImportedMaterial.env_map_scale` is now wired on the BGSM arm
            // (`forward_bgsm_env_map_scale`), and `env_mapping_enabled()` is
            // the exact authored gate on this side. It is left unforwarded
            // DELIBERATELY rather than overlooked: BGEM leaves the PBR
            // overrides `None`, so `Material::resolve_pbr` runs its keyword
            // classifier, whose `env_map_scale > 0.3` arm would then start
            // moving roughness on every env-mapped FO4 effect material. That
            // is a shading change to evaluate on its own evidence, not a
            // drop to fix in passing.
        }
        // Soft-particle depth fade + view-angle falloff cone. The NIF
        // `BSEffectShaderProperty` path fills `material.effect_shader` from the
        // block; the BGEM path is the FO4+ equivalent and must mirror it so
        // `material_translate` can build `Material.{effect_falloff,
        // effect_shader_flags}` (soft_falloff_depth + MAT_FLAG_EFFECT_SOFT)
        // the same way. Without this every FO4 BGEM mist/steam/beam volume
        // (`soft = true` in the authored file) rendered with no depth feather
        // and stacked to an opaque white-out (HalluciGen labs). `lighting_influence`
        // is authored 0..1 in BGEM but carried 0..255 on the shared payload.
        material.effect_shader = Some(byroredux_nif::import::BsEffectShaderData {
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
            log::warn!(
                "{}",
                unresolved_material_warning(&path, provider.has_starfield_cdb())
            );
        }
        // #3230 SIBLING — not reachable for a `.bgsm`/`.bgem` path today
        // (`ext_kind` is always `Some` for those, so `dispatch_kind` is too,
        // and both its variants are handled above). It becomes reachable the
        // moment `peek_magic` learns a third `MaterialKind`, and the answer
        // for a Starfield session would be the same as the two arms above,
        // so wire it now rather than leave a hole for that change to fall in.
        if cdb_pbr_fallback {
            return apply_cdb_pbr_fallback(material, &path);
        }
        return MergeOutcome::Unresolved;
    }

    // Both the BGSM and BGEM arms above set `touched` unconditionally
    // alongside `material.from_bgsm = true`, so reaching here with
    // `touched == false` is not currently possible — the `PresenceOnly`
    // arm is deliberate rather than defensive, and stays correct if a
    // future arm resolves without forwarding anything.
    if touched {
        MergeOutcome::Merged
    } else {
        MergeOutcome::PresenceOnly
    }
}
