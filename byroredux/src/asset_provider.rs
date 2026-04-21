//! BSA/BA2-backed texture and mesh extraction.

use byroredux_bgsm::template::ResolvedMaterial;
use byroredux_bgsm::{BgemFile, TemplateCache, TemplateResolver};
use byroredux_nif::import::ImportedMesh;
use byroredux_renderer::VulkanContext;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// A game archive that can extract files by path.
/// Wraps either a BSA (Oblivion–Skyrim SE) or BA2 (FO4–Starfield) archive.
enum Archive {
    Bsa(byroredux_bsa::BsaArchive),
    Ba2(byroredux_bsa::Ba2Archive),
}

impl Archive {
    /// Open an archive file, auto-detecting BSA vs BA2 from the file magic.
    fn open(path: &str) -> Result<Self, String> {
        let magic = std::fs::read(path)
            .map_err(|e| format!("read '{}': {}", path, e))
            .and_then(|data| {
                if data.len() < 4 {
                    Err(format!("'{}' too small", path))
                } else {
                    Ok([data[0], data[1], data[2], data[3]])
                }
            })?;
        if &magic == b"BTDX" {
            byroredux_bsa::Ba2Archive::open(path)
                .map(Archive::Ba2)
                .map_err(|e| format!("BA2 '{}': {}", path, e))
        } else {
            byroredux_bsa::BsaArchive::open(path)
                .map(Archive::Bsa)
                .map_err(|e| format!("BSA '{}': {}", path, e))
        }
    }

    fn extract(&self, path: &str) -> Result<Vec<u8>, std::io::Error> {
        match self {
            Archive::Bsa(a) => a.extract(path),
            Archive::Ba2(a) => a.extract(path),
        }
    }
}

/// Provides file data by searching BSA/BA2 archives.
pub(crate) struct TextureProvider {
    texture_archives: Vec<Archive>,
    mesh_archives: Vec<Archive>,
}

impl TextureProvider {
    pub(crate) fn new() -> Self {
        Self {
            texture_archives: Vec::new(),
            mesh_archives: Vec::new(),
        }
    }

    /// Extract a texture (DDS) from texture archives.
    ///
    /// Paths are normalized before lookup: anything that doesn't already
    /// start with `textures\` gets the prefix prepended. Bethesda WTHR /
    /// CLMT / LTEX records author paths relative to the `textures\`
    /// root (e.g. `sky\cloudsnoon.dds`, `landscape\dirt02.dds`) but
    /// the archive layer stores them with the full `textures\` prefix.
    /// Pre-#468 every such lookup silently missed and clouds / sun
    /// textures rendered as disabled. Callers that already supply a
    /// fully-qualified path (the cell loader's `textures\landscape\…`
    /// path-building sites) go through unchanged.
    pub(crate) fn extract(&self, path: &str) -> Option<Vec<u8>> {
        let normalized = normalize_texture_path(path);
        for archive in &self.texture_archives {
            if let Ok(data) = archive.extract(normalized.as_ref()) {
                return Some(data);
            }
        }
        None
    }

    /// Extract a mesh (NIF) from mesh archives.
    pub(crate) fn extract_mesh(&self, path: &str) -> Option<Vec<u8>> {
        for archive in &self.mesh_archives {
            if let Ok(data) = archive.extract(path) {
                return Some(data);
            }
        }
        None
    }
}

/// Prepend `textures\` to a texture path if the path doesn't already
/// begin with that segment (case-insensitive). Returns `Cow::Borrowed`
/// when the input is already fully qualified so we don't allocate on
/// the hot path (every REFR material resolves a texture).
///
/// See #468 — Bethesda WTHR cloud / CLMT sun / LTEX landscape records
/// all author paths relative to the `textures\` root, but the BSA / BA2
/// layer stores them with the full prefix.
pub(crate) fn normalize_texture_path(path: &str) -> std::borrow::Cow<'_, str> {
    // Fast ASCII-lowercase check on just the first 9 bytes (`textures\`).
    // Avoids allocating a full lowercase copy of the whole path on every
    // texture lookup. Matches on either `/` or `\` separators — archive
    // paths are backslashed on Bethesda systems, but forward slashes can
    // sneak in via mod authoring tools.
    let bytes = path.as_bytes();
    let has_prefix = bytes.len() >= 9
        && bytes[..8].eq_ignore_ascii_case(b"textures")
        && (bytes[8] == b'\\' || bytes[8] == b'/');
    if has_prefix {
        std::borrow::Cow::Borrowed(path)
    } else {
        std::borrow::Cow::Owned(format!("textures\\{}", path))
    }
}

/// Parse grid coordinates from a "x,y" string.
pub(crate) fn parse_grid_coords(s: &str) -> (i32, i32) {
    let parts: Vec<&str> = s.split(',').collect();
    if parts.len() == 2 {
        let x = parts[0].trim().parse::<i32>().unwrap_or(0);
        let y = parts[1].trim().parse::<i32>().unwrap_or(0);
        (x, y)
    } else {
        log::warn!("Invalid grid format '{}', using (0,0)", s);
        (0, 0)
    }
}

/// Build a TextureProvider from CLI arguments.
pub(crate) fn build_texture_provider(args: &[String]) -> TextureProvider {
    let mut provider = TextureProvider::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--textures-bsa" => {
                if let Some(path) = args.get(i + 1) {
                    match Archive::open(path) {
                        Ok(a) => {
                            log::info!("Opened textures archive: '{}'", path);
                            provider.texture_archives.push(a);
                        }
                        Err(e) => log::warn!("Failed to open textures archive: {}", e),
                    }
                    i += 2;
                    continue;
                }
            }
            "--bsa" => {
                if let Some(path) = args.get(i + 1) {
                    match Archive::open(path) {
                        Ok(a) => {
                            log::info!("Opened mesh archive: '{}'", path);
                            provider.mesh_archives.push(a);
                        }
                        Err(e) => log::warn!("Failed to open mesh archive: {}", e),
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
        if args[i] == "--materials-ba2" {
            if let Some(path) = args.get(i + 1) {
                match Archive::open(path) {
                    Ok(a) => {
                        log::info!("Opened materials archive: '{}'", path);
                        provider.push_archive(a);
                    }
                    Err(e) => log::warn!("Failed to open materials archive: {}", e),
                }
                i += 2;
                continue;
            }
        }
        i += 1;
    }
    provider
}

/// Resolve a texture path to a texture handle, with BSA/BA2 lookup and caching.
pub(crate) fn resolve_texture(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    tex_path: Option<&str>,
) -> u32 {
    let Some(tex_path) = tex_path else {
        return ctx.texture_registry.fallback();
    };
    if let Some(cached) = ctx.texture_registry.get_by_path(tex_path) {
        return cached;
    }
    if let Some(dds_bytes) = tex_provider.extract(tex_path) {
        let alloc = ctx.allocator.as_ref().unwrap();
        match ctx.texture_registry.load_dds(
            &ctx.device,
            alloc,
            &ctx.graphics_queue,
            ctx.transfer_pool,
            tex_path,
            &dds_bytes,
        ) {
            Ok(h) => {
                log::info!("Loaded DDS texture: '{}'", tex_path);
                return h;
            }
            Err(e) => {
                log::warn!("Failed to load DDS '{}': {}", tex_path, e);
            }
        }
    } else {
        log::debug!("Texture not found in archive: '{}'", tex_path);
    }
    ctx.texture_registry.fallback()
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
    archives: Vec<Archive>,
    /// BGSM chain cache from the `bgsm` crate — handles template
    /// inheritance with case-insensitive keying + LRU eviction.
    bgsm_cache: TemplateCache,
    /// BGEM has no template inheritance (the format carries no
    /// `root_material_path`), so we cache parsed files directly by path.
    bgem_cache: HashMap<String, Arc<BgemFile>>,
    /// Paths we've already warned about so a broken file doesn't spam
    /// the log on every cell load.
    failed_paths: HashSet<String>,
}

impl MaterialProvider {
    pub(crate) fn new() -> Self {
        Self {
            archives: Vec::new(),
            bgsm_cache: TemplateCache::new(256),
            bgem_cache: HashMap::new(),
            failed_paths: HashSet::new(),
        }
    }

    fn push_archive(&mut self, archive: Archive) {
        self.archives.push(archive);
    }

    fn extract_from_archives(&self, path: &str) -> Option<Vec<u8>> {
        for archive in &self.archives {
            if let Ok(bytes) = archive.extract(path) {
                return Some(bytes);
            }
        }
        None
    }

    /// Resolve a BGSM file + its template chain. Returns `None` when the
    /// file isn't in any loaded archive, when parse fails, or when the
    /// template chain has a cycle. Logs once per path on the failure paths.
    pub(crate) fn resolve_bgsm(&mut self, path: &str) -> Option<Arc<ResolvedMaterial>> {
        let key = path.to_ascii_lowercase();
        // Archive slice is borrowed into the ad-hoc resolver so the
        // cache's mutable borrow doesn't alias archive reads.
        struct ArchiveReader<'a> {
            archives: &'a [Archive],
        }
        impl<'a> TemplateResolver for ArchiveReader<'a> {
            fn read(&mut self, path: &str) -> Option<Vec<u8>> {
                for archive in self.archives {
                    if let Ok(bytes) = archive.extract(path) {
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
            Err(e) => {
                if self.failed_paths.insert(key) {
                    log::warn!("BGSM resolve failed for '{}': {}", path, e);
                }
                None
            }
        }
    }

    /// Resolve a BGEM effect-material file. No template inheritance.
    pub(crate) fn resolve_bgem(&mut self, path: &str) -> Option<Arc<BgemFile>> {
        let key = path.to_ascii_lowercase();
        if let Some(hit) = self.bgem_cache.get(&key) {
            return Some(Arc::clone(hit));
        }
        let bytes = self.extract_from_archives(&key)?;
        match byroredux_bgsm::parse_bgem(&bytes) {
            Ok(parsed) => {
                let arc = Arc::new(parsed);
                self.bgem_cache.insert(key, Arc::clone(&arc));
                Some(arc)
            }
            Err(e) => {
                if self.failed_paths.insert(key) {
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
pub(crate) fn merge_bgsm_into_mesh(
    mesh: &mut ImportedMesh,
    provider: &mut MaterialProvider,
) -> bool {
    let Some(path) = mesh.material_path.clone() else {
        return false;
    };
    let lower = path.to_ascii_lowercase();

    let mut touched = false;
    let mut fill = |slot: &mut Option<String>, value: &str| {
        if slot.is_none() && !value.is_empty() {
            *slot = Some(value.to_string());
            touched = true;
        }
    };

    if lower.ends_with(".bgsm") {
        let Some(resolved) = provider.resolve_bgsm(&path) else {
            return false;
        };
        for step in resolved.walk() {
            let bgsm = &step.file;
            fill(&mut mesh.texture_path, &bgsm.diffuse_texture);
            fill(&mut mesh.normal_map, &bgsm.normal_texture);
            fill(&mut mesh.glow_map, &bgsm.glow_texture);
            // Smoothness/spec mask — .r encodes per-texel specular
            // strength in the engine's existing gloss_map slot. #453.
            fill(&mut mesh.gloss_map, &bgsm.smooth_spec_texture);
            // Legacy v <= 2 environment cube; newer BGSMs drop the slot.
            fill(&mut mesh.env_map, &bgsm.envmap_texture);
            fill(&mut mesh.parallax_map, &bgsm.displacement_texture);
        }
    } else if lower.ends_with(".bgem") {
        let Some(bgem) = provider.resolve_bgem(&path) else {
            return false;
        };
        fill(&mut mesh.texture_path, &bgem.base_texture);
        fill(&mut mesh.normal_map, &bgem.normal_texture);
        fill(&mut mesh.glow_map, &bgem.glow_texture);
        fill(&mut mesh.env_map, &bgem.envmap_texture);
        fill(&mut mesh.env_mask, &bgem.envmap_mask_texture);
    } else {
        return false;
    }

    touched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_adds_prefix_when_missing() {
        // WTHR cloud path authored relative to `textures\` root.
        let out = normalize_texture_path("sky\\cloudsnoon.dds");
        assert_eq!(out.as_ref(), "textures\\sky\\cloudsnoon.dds");
    }

    #[test]
    fn normalize_leaves_fully_qualified_paths_borrowed() {
        // Cell loader's landscape path path-building already supplies
        // the `textures\` prefix; the fn must pass through without
        // allocating (Cow::Borrowed).
        let input = "textures\\landscape\\dirt02.dds";
        let out = normalize_texture_path(input);
        assert_eq!(out.as_ref(), input);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn normalize_is_case_insensitive_on_prefix() {
        // A future tool or mod authoring flow might export
        // `Textures\…` or `TEXTURES\…`; the prefix check is ASCII-
        // case-insensitive and shouldn't double up.
        let out = normalize_texture_path("Textures\\sky\\cloudsnoon.dds");
        assert_eq!(out.as_ref(), "Textures\\sky\\cloudsnoon.dds");
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn normalize_accepts_forward_slash_separator() {
        // Mod authoring tools occasionally emit forward slashes.
        // The prefix check accepts either.
        let out = normalize_texture_path("textures/sky/cloudsnoon.dds");
        assert_eq!(out.as_ref(), "textures/sky/cloudsnoon.dds");
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn normalize_prefixes_non_textures_paths_as_owned() {
        // Any path whose first segment isn't `textures\` — e.g. a
        // relative CLMT `sun_01.dds` or a broken mod export — gets
        // the prefix prepended. The fn allocates in this branch, so
        // result is Cow::Owned.
        let out = normalize_texture_path("sun_01.dds");
        assert_eq!(out.as_ref(), "textures\\sun_01.dds");
        assert!(matches!(out, std::borrow::Cow::Owned(_)));
    }

    #[test]
    fn normalize_short_string_gets_prefixed() {
        // Guard against the `bytes.len() >= 9` check: a 1-byte path
        // should still prefix cleanly.
        let out = normalize_texture_path("a");
        assert_eq!(out.as_ref(), "textures\\a");
    }

    // ── MaterialProvider / BGSM merge (#493) ──────────────────────────
    //
    // The merge logic in `merge_bgsm_into_mesh` has three moving parts:
    //   1. Dispatch on `material_path` extension (.bgsm / .bgem / other)
    //   2. For BGSM: walk the template chain child-first, fill empties
    //   3. For BGEM: single-file fill, no inheritance
    //
    // We test part 2 + 3 directly against synthetic `ResolvedMaterial` /
    // `BgemFile` values (bypassing archive IO which the `bgsm` crate's
    // own tests cover). Part 1 is covered through the resolve_bgsm
    // failure-dedup test below.

    use byroredux_bgsm::template::ResolvedMaterial;
    use byroredux_bgsm::{BgemFile, BgsmFile};

    /// Same fill closure as `merge_bgsm_into_mesh` so the tests verify
    /// the exact precedence rule the prod helper uses.
    fn fill(slot: &mut Option<String>, value: &str) -> bool {
        if slot.is_none() && !value.is_empty() {
            *slot = Some(value.to_string());
            true
        } else {
            false
        }
    }

    /// Walks a ResolvedMaterial chain child-first, filling the 6 slots
    /// the prod merge helper writes for BGSM files. The slot set mirrors
    /// `merge_bgsm_into_mesh` exactly; the closure is inlined in prod
    /// for a single allocation-free pass.
    fn apply_bgsm_chain(
        resolved: &ResolvedMaterial,
        texture_path: &mut Option<String>,
        normal_map: &mut Option<String>,
        glow_map: &mut Option<String>,
        gloss_map: &mut Option<String>,
        env_map: &mut Option<String>,
        parallax_map: &mut Option<String>,
    ) {
        for step in resolved.walk() {
            fill(texture_path, &step.file.diffuse_texture);
            fill(normal_map, &step.file.normal_texture);
            fill(glow_map, &step.file.glow_texture);
            fill(gloss_map, &step.file.smooth_spec_texture);
            fill(env_map, &step.file.envmap_texture);
            fill(parallax_map, &step.file.displacement_texture);
        }
    }

    #[test]
    fn bgsm_merge_fills_only_empty_slots() {
        // NIF already authored a diffuse; BGSM should NOT overwrite it.
        let mut texture_path: Option<String> = Some("nif_diffuse.dds".into());
        let mut normal_map: Option<String> = None;
        let mut glow_map: Option<String> = None;
        let mut gloss_map: Option<String> = None;
        let mut env_map: Option<String> = None;
        let mut parallax_map: Option<String> = None;

        let bgsm = BgsmFile {
            diffuse_texture: "bgsm_diffuse.dds".into(),
            normal_texture: "bgsm_normal.dds".into(),
            glow_texture: "bgsm_glow.dds".into(),
            ..Default::default()
        };
        let resolved = ResolvedMaterial {
            file: bgsm,
            parent: None,
        };

        apply_bgsm_chain(
            &resolved,
            &mut texture_path,
            &mut normal_map,
            &mut glow_map,
            &mut gloss_map,
            &mut env_map,
            &mut parallax_map,
        );

        // NIF-authored field preserved.
        assert_eq!(texture_path.as_deref(), Some("nif_diffuse.dds"));
        // Empty slots filled.
        assert_eq!(normal_map.as_deref(), Some("bgsm_normal.dds"));
        assert_eq!(glow_map.as_deref(), Some("bgsm_glow.dds"));
    }

    #[test]
    fn bgsm_merge_child_wins_over_parent() {
        let mut texture_path: Option<String> = None;
        let mut normal_map: Option<String> = None;
        let mut glow_map: Option<String> = None;
        let mut gloss_map: Option<String> = None;
        let mut env_map: Option<String> = None;
        let mut parallax_map: Option<String> = None;

        let child = BgsmFile {
            diffuse_texture: "child_diffuse.dds".into(),
            // child leaves normal empty → parent fills it
            ..Default::default()
        };
        let parent = BgsmFile {
            diffuse_texture: "parent_diffuse.dds".into(),
            normal_texture: "parent_normal.dds".into(),
            ..Default::default()
        };
        let resolved = ResolvedMaterial {
            file: child,
            parent: Some(Arc::new(ResolvedMaterial {
                file: parent,
                parent: None,
            })),
        };

        apply_bgsm_chain(
            &resolved,
            &mut texture_path,
            &mut normal_map,
            &mut glow_map,
            &mut gloss_map,
            &mut env_map,
            &mut parallax_map,
        );

        assert_eq!(texture_path.as_deref(), Some("child_diffuse.dds"));
        assert_eq!(normal_map.as_deref(), Some("parent_normal.dds"));
    }

    #[test]
    fn bgem_merge_fills_effect_slots() {
        // BGEM has no template inheritance — single file.
        let mut texture_path: Option<String> = None;
        let mut normal_map: Option<String> = None;
        let mut env_mask: Option<String> = None;

        let bgem = BgemFile {
            base_texture: "effect_base.dds".into(),
            normal_texture: "effect_normal.dds".into(),
            envmap_mask_texture: "effect_mask.dds".into(),
            ..Default::default()
        };

        fill(&mut texture_path, &bgem.base_texture);
        fill(&mut normal_map, &bgem.normal_texture);
        fill(&mut env_mask, &bgem.envmap_mask_texture);

        assert_eq!(texture_path.as_deref(), Some("effect_base.dds"));
        assert_eq!(normal_map.as_deref(), Some("effect_normal.dds"));
        assert_eq!(env_mask.as_deref(), Some("effect_mask.dds"));
    }

    /// Every failing-to-resolve path logs at most once, so a broken
    /// material in a 1000-REFR cell doesn't spam the log.
    #[test]
    fn failed_path_set_dedups_warnings() {
        let mut provider = MaterialProvider::new();
        // No archives registered → every resolve_bgsm fails at the
        // archive read step. The failed_paths set grows on the first
        // call only.
        let before = provider.failed_paths.len();
        let _ = provider.resolve_bgsm("materials/absent.bgsm");
        let _ = provider.resolve_bgsm("materials/absent.bgsm");
        let _ = provider.resolve_bgsm("materials/absent.bgsm");
        let after = provider.failed_paths.len();
        assert_eq!(after, before + 1);
    }

    /// `build_material_provider` on CLI args without `--materials-ba2`
    /// returns an empty provider — the merge helper short-circuits
    /// when the archive lookup fails, so pre-FO4 content pays zero cost.
    #[test]
    fn build_material_provider_without_args_is_empty() {
        let provider = build_material_provider(&[]);
        assert!(provider.archives.is_empty());
    }
}
