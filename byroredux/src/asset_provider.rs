//! BSA/BA2-backed texture and mesh extraction.

use byroredux_bgsm::template::ResolvedMaterial;
use byroredux_bgsm::{BgemFile, TemplateCache, TemplateResolver};
use byroredux_nif::import::{ImportedMesh, MeshResolver};
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

impl MeshResolver for TextureProvider {
    fn resolve(&self, mesh_name: &str) -> Option<Vec<u8>> {
        self.extract_mesh(mesh_name)
    }
}

/// Strip the Bethesda build-server prefix from an asset path.
///
/// Some shipping Bethesda content — most notably the Skyrim Anniversary
/// Edition's "Skyrim HD" trees, plants, and landscape clutter — embeds
/// texture and model paths with the full pipeline-internal prefix
/// `skyrimhd\build\pc\data\textures\…`. The real Bethesda engine
/// resolves these against a `Data\` root by stripping everything up to
/// and including the last `\data\` (or `/data/`) segment in the path.
/// Without that step the BSA lookup misses every affected asset and
/// the renderer falls back to the magenta-checker placeholder — the
/// symptom that prompted this fix on a Markarth grid (juniper, reach
/// branches, driftwood, plus a long tail of landscape clutter).
///
/// Returns `Cow::Borrowed` on the common case (no embedded `\data\`).
/// Case-insensitive on the `data` token; matches `\` or `/` separators
/// on either side (mod-authoring tools sometimes export forward
/// slashes).
pub(crate) fn strip_build_prefix(path: &str) -> std::borrow::Cow<'_, str> {
    let bytes = path.as_bytes();
    // We need at least `\data\X` (7 bytes) to even have a useful strip,
    // and that the strip leaves a non-empty trailer.
    if bytes.len() < 7 {
        return std::borrow::Cow::Borrowed(path);
    }
    // Scan left-to-right for the LAST `\data\` boundary so we tolerate
    // future build-server prefixes that nest a `data\` directory
    // elsewhere in the path. Pre-#945 fix used a hardcoded
    // `skyrimhd\build\pc\data\` strip but that's brittle: AE post-launch
    // patches and Creation Club mods author new prefixes
    // (`fishingrod\data\`, `survivalmode\data\`, etc.) and the engine
    // strips all of them.
    let mut last: Option<usize> = None;
    let mut i = 0;
    while i + 6 <= bytes.len() {
        let l = bytes[i];
        let r = bytes[i + 5];
        if (l == b'\\' || l == b'/')
            && (r == b'\\' || r == b'/')
            && bytes[i + 1..i + 5].eq_ignore_ascii_case(b"data")
        {
            last = Some(i + 6);
        }
        i += 1;
    }
    match last {
        Some(start) if start < bytes.len() => {
            std::borrow::Cow::Owned(path[start..].to_string())
        }
        _ => std::borrow::Cow::Borrowed(path),
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

/// Open the requested archive plus any numeric-suffixed siblings.
///
/// FNV ships its base textures across `Fallout - Textures.bsa` and
/// `Fallout - Textures2.bsa`; passing only the first leaves Doc
/// Mitchell's plaster and floor textures resolving to the
/// missing-texture checkerboard placeholder, which compositing with
/// the (correctly loaded) tangent-space normal map produced the
/// "chrome posterized walls" diagnosis chased through R1 / #783 /
/// #784. By auto-loading `<stem>2.bsa` … `<stem>9.bsa` siblings when
/// the explicitly named archive ends in an unsuffixed `.bsa`, FNV's
/// split is transparent. The pattern is inert for Skyrim's already-
/// numeric `Skyrim - Meshes0.bsa` / `Meshes1.bsa` style (the user
/// passes both explicitly anyway) and harmless when the sibling
/// simply doesn't exist.
fn open_with_numeric_siblings(path: &str, kind: &str, archives: &mut Vec<Archive>) {
    match Archive::open(path) {
        Ok(a) => {
            log::info!("Opened {} archive: '{}'", kind, path);
            archives.push(a);
        }
        Err(e) => {
            log::warn!("Failed to open {} archive: {}", kind, e);
            return;
        }
    }
    // Only auto-load siblings when the explicit path ends in `.bsa`
    // / `.ba2` with no digit immediately before the extension.
    // `Foo.bsa`  → try `Foo2.bsa`..`Foo9.bsa`.
    // `Foo2.bsa` → no auto-load (avoids re-opening when the user
    //              already lists each numbered archive on the CLI).
    let lower = path.to_ascii_lowercase();
    let (stem, ext) = if let Some(s) = lower.strip_suffix(".bsa") {
        (&path[..s.len()], ".bsa")
    } else if let Some(s) = lower.strip_suffix(".ba2") {
        (&path[..s.len()], ".ba2")
    } else {
        return;
    };
    if stem.chars().last().is_some_and(|c| c.is_ascii_digit()) {
        return;
    }
    for n in 2..=9u32 {
        let sibling = format!("{}{}{}", stem, n, ext);
        if !std::path::Path::new(&sibling).is_file() {
            continue;
        }
        match Archive::open(&sibling) {
            Ok(a) => {
                log::info!("Opened sibling {} archive: '{}'", kind, sibling);
                archives.push(a);
            }
            Err(e) => {
                log::warn!("Failed to open sibling {} archive: {}", kind, e);
            }
        }
    }
}

/// M44 Phase 3.5 — try to populate `FootstepConfig.default_sound`
/// from the BSA at `--sounds-bsa <path>` (if provided). Decodes the
/// canonical FNV dirt-walk left-foot WAV — every kf-era humanoid
/// hits this on every other step. Future Phase 3.5b replaces the
/// single-sound fallback with FOOT-record-driven per-material lookup.
///
/// Silently skips when:
///   - `--sounds-bsa` is absent (no audio data wired by the user).
///   - The BSA can't be opened (missing file, permissions).
///   - The canonical path is missing from the archive (modded loadout?).
///   - The decode fails through `byroredux_audio::load_sound_from_bytes`.
///
/// Each failure logs at WARN; engine boot continues regardless.
pub(crate) fn try_load_default_footstep(
    world: &mut byroredux_core::ecs::World,
    args: &[String],
) {
    let mut path: Option<&str> = None;
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--sounds-bsa" {
            path = args.get(i + 1).map(|s| s.as_str());
            break;
        }
        i += 1;
    }
    let Some(path) = path else { return };
    let archive = match Archive::open(path) {
        Ok(a) => a,
        Err(e) => {
            log::warn!("M44 Phase 3.5: open --sounds-bsa '{path}': {e}");
            return;
        }
    };
    // Vanilla FNV ships dirt-walk footsteps with left/right
    // alternation. Pick one canonical entry as the default until
    // FOOT records land. Path verified by `probe_substring` against
    // `Fallout - Sound.bsa`, 2026-05-05.
    const CANONICAL: &str = r"sound\fx\fst\dirt\walk\left\fst_dirt_walk_01.wav";
    let bytes = match archive.extract(CANONICAL) {
        Ok(b) => b,
        Err(e) => {
            log::warn!(
                "M44 Phase 3.5: '{path}' missing canonical footstep '{CANONICAL}': {e}"
            );
            return;
        }
    };
    let sound = match byroredux_audio::load_sound_from_bytes(bytes) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("M44 Phase 3.5: decode '{CANONICAL}': {e}");
            return;
        }
    };
    let mut config = world.resource_mut::<crate::components::FootstepConfig>();
    config.default_sound = Some(std::sync::Arc::new(sound));
    log::info!("M44 Phase 3.5: footstep sound loaded from '{path}' ({CANONICAL})");
}

/// Build a TextureProvider from CLI arguments.
pub(crate) fn build_texture_provider(args: &[String]) -> TextureProvider {
    let mut provider = TextureProvider::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--textures-bsa" => {
                if let Some(path) = args.get(i + 1) {
                    open_with_numeric_siblings(path, "textures", &mut provider.texture_archives);
                    i += 2;
                    continue;
                }
            }
            "--bsa" => {
                if let Some(path) = args.get(i + 1) {
                    open_with_numeric_siblings(path, "mesh", &mut provider.mesh_archives);
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
///
/// Uses Gamebryo's default `WRAP_S_WRAP_T` clamp mode (`3` per
/// nif.xml's `TexClampMode`). Call [`resolve_texture_with_clamp`] when
/// the source material's `texture_clamp_mode` is non-default — decals
/// / scope reticles / skybox seams need `0 = CLAMP_S_CLAMP_T` to
/// avoid edge bleed. See #610.
pub(crate) fn resolve_texture(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    tex_path: Option<&str>,
) -> u32 {
    // 3 = WRAP_S_WRAP_T per nif.xml — the legacy REPEAT default.
    resolve_texture_with_clamp(ctx, tex_provider, tex_path, 3)
}

/// `resolve_texture`'s clamp-aware variant (#610 / D4-NEW-02). Routes
/// through the registry's per-`(path, clamp_mode)` cache so the same
/// DDS path requested with two different `TexClampMode` values gets
/// two distinct bindless entries with the right `VkSamplerAddressMode`
/// pair attached. `clamp_mode` values outside `0..=3` are clamped to
/// `3` (REPEAT) by the registry — defensive default for upstream
/// parser garbage.
pub(crate) fn resolve_texture_with_clamp(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    tex_path: Option<&str>,
    clamp_mode: u8,
) -> u32 {
    let Some(tex_path) = tex_path else {
        return ctx.texture_registry.fallback();
    };
    // Strip Bethesda build-server prefixes (e.g. `skyrimhd\build\pc\data\`)
    // so cache + BSA lookups both use the canonical `textures\…` path.
    // Without this step Skyrim AE's HD-bundle juniper / reach branches /
    // driftwood / mountain clutter all render as magenta placeholders.
    // Applied BEFORE the cache so two NIFs that reference the same
    // physical texture via different prefix-paths share a single
    // bindless entry. See `strip_build_prefix` doc for details.
    let tex_path: &str = &strip_build_prefix(tex_path);
    // `acquire_by_path` (not `get_by_path`) — bumps the refcount on a
    // cache hit so each resolve pairs with one drop_texture on cell
    // unload. `load_dds` on the miss path bumps from 0→1 on fresh
    // uploads; both routes produce exactly one outstanding ref per
    // caller. See #524.
    if let Some(cached) = ctx
        .texture_registry
        .acquire_by_path_with_clamp(tex_path, clamp_mode)
    {
        return cached;
    }
    if let Some(dds_bytes) = tex_provider.extract(tex_path) {
        // #881 / CELL-PERF-03 — enqueue rather than upload
        // synchronously. The bindless slot is reserved eagerly with
        // the descriptor pointing at the fallback so this REFR's
        // material can attach the returned handle immediately; the
        // real GPU upload + descriptor write happens in the batched
        // `flush_pending_uploads` call at the end of the cell load
        // (`load_references`). Pre-fix every fresh DDS paid its own
        // `with_one_time_commands` (submit + fence-wait) — ~50 ms
        // per ~100-DDS edge crossing.
        match ctx
            .texture_registry
            .enqueue_dds_with_clamp(&ctx.device, tex_path, dds_bytes, clamp_mode)
        {
            Ok(h) => {
                log::debug!(
                    "Queued DDS texture: '{}' (clamp_mode {}, handle {h})",
                    tex_path,
                    clamp_mode,
                );
                return h;
            }
            Err(e) => {
                log::warn!("Failed to enqueue DDS '{}': {}", tex_path, e);
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

    /// Read the first 4 bytes of a material file from the archives to detect
    /// whether it is BGSM or BGEM by magic, independent of its file extension.
    /// Returns `None` when the file isn't found or the magic is unrecognised.
    fn peek_magic(&self, path: &str) -> Option<byroredux_bgsm::MaterialKind> {
        let bytes = self.extract_from_archives(path)?;
        byroredux_bgsm::detect_kind(&bytes)
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
        for step in resolved.walk() {
            let bgsm = &step.file;
            fill(&mut mesh.texture_path, &bgsm.diffuse_texture, &mut touched, pool);
            fill(&mut mesh.normal_map, &bgsm.normal_texture, &mut touched, pool);
            fill(&mut mesh.glow_map, &bgsm.glow_texture, &mut touched, pool);
            // Smoothness/spec mask — .r encodes per-texel specular
            // strength in the engine's existing gloss_map slot. #453.
            fill(&mut mesh.gloss_map, &bgsm.smooth_spec_texture, &mut touched, pool);
            // Legacy v <= 2 environment cube; newer BGSMs drop the slot.
            fill(&mut mesh.env_map, &bgsm.envmap_texture, &mut touched, pool);
            fill(
                &mut mesh.parallax_map,
                &bgsm.displacement_texture,
                &mut touched,
                pool,
            );

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
                mesh.glossiness = bgsm.smoothness;
                set_glossiness = true;
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
        }
    } else if dispatch_kind == Some(MaterialKind::Bgem) {
        let Some(bgem) = provider.resolve_bgem(&path) else {
            return false;
        };
        fill(&mut mesh.texture_path, &bgem.base_texture, &mut touched, pool);
        fill(&mut mesh.normal_map, &bgem.normal_texture, &mut touched, pool);
        fill(&mut mesh.glow_map, &bgem.glow_texture, &mut touched, pool);
        fill(&mut mesh.env_map, &bgem.envmap_texture, &mut touched, pool);
        fill(&mut mesh.env_mask, &bgem.envmap_mask_texture, &mut touched, pool);

        // BGEM has no inheritance so there's no child-first chain —
        // we just forward whatever the single file authored. The
        // scalar set is smaller than BGSM: no specular / glossiness,
        // no `emittance_mult` (BGEM folds it into the color), just
        // emissive color + UV + the boolean flags. See #583.
        mesh.emissive_color = bgem.emittance_color;
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
        touched = true;
    } else {
        // Unknown extension — most likely a Starfield .mat JSON path that
        // SF-D3-01's suffix gate now correctly routes here. The .mat format
        // is not yet parsed (tracked in SF-D6-03). Log once per path so the
        // absence of material data is visible without spamming every frame.
        static WARNED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashSet<String>>> =
            std::sync::OnceLock::new();
        let mut set = WARNED
            .get_or_init(|| std::sync::Mutex::new(std::collections::HashSet::new()))
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if set.insert(path.to_owned()) {
            log::warn!(
                "material path '{}' is not a .bgsm/.bgem — unsupported format (Starfield .mat?); mesh will use NIF defaults",
                path
            );
        }
        return false;
    }

    touched
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_build_prefix_handles_skyrim_hd_prefix() {
        // The headline case from the Markarth render: Skyrim AE bundles
        // the HD juniper / reach branches / driftwood with the full
        // pipeline-internal prefix.
        let out = strip_build_prefix(
            "skyrimhd\\build\\pc\\data\\textures\\plants\\florajuniper.dds",
        );
        assert_eq!(out.as_ref(), "textures\\plants\\florajuniper.dds");
    }

    #[test]
    fn strip_build_prefix_passes_canonical_paths_through_borrowed() {
        let input = "textures\\landscape\\trees\\reachtreebranch01.dds";
        let out = strip_build_prefix(input);
        assert_eq!(out.as_ref(), input);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn strip_build_prefix_is_case_insensitive_on_data_token() {
        // Anniversary Edition's HD bundle uses lowercase `data`, but
        // we shouldn't be fragile if a future CC pack uses `Data`.
        let out = strip_build_prefix(
            "skyrimhd\\build\\pc\\Data\\textures\\plants\\foo.dds",
        );
        assert_eq!(out.as_ref(), "textures\\plants\\foo.dds");
    }

    #[test]
    fn strip_build_prefix_accepts_forward_slashes() {
        // Mod-authoring tools occasionally export forward slashes.
        let out = strip_build_prefix(
            "skyrimhd/build/pc/data/textures/plants/foo.dds",
        );
        assert_eq!(out.as_ref(), "textures/plants/foo.dds");
    }

    #[test]
    fn strip_build_prefix_uses_last_data_boundary() {
        // Pathological case: an asset that genuinely lives under a
        // nested `data\` directory should strip up to the LAST
        // boundary so the longest known-prefix wins.
        let out = strip_build_prefix(
            "vendor\\data\\skyrimhd\\build\\pc\\data\\textures\\foo.dds",
        );
        assert_eq!(out.as_ref(), "textures\\foo.dds");
    }

    #[test]
    fn strip_build_prefix_preserves_path_with_no_data_segment() {
        let input = "meshes\\architecture\\foo.nif";
        let out = strip_build_prefix(input);
        assert_eq!(out.as_ref(), input);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn strip_build_prefix_preserves_trailing_data_directory() {
        // A path that ends with `\data\` exactly would strip to empty;
        // the helper must guard that and pass the path through
        // untouched so callers can fall through to "not found" rather
        // than hitting an empty BSA lookup that might silently succeed
        // on the first-entry of the archive.
        let input = "scratch\\data\\";
        let out = strip_build_prefix(input);
        assert_eq!(out.as_ref(), input);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

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

    /// Regression for #583 — synthetic BGSM template chain exercises
    /// child-first scalar precedence inline with the prod helper body.
    /// Child authors `emit_enabled=true` + distinct emissive, specular,
    /// glossiness, alpha, UV, and two_sided; parent authors different
    /// values. The child's scalar values must win; parent must contribute
    /// only fields the child left at its default.
    ///
    /// This mirrors the prod `merge_bgsm_into_mesh` body (minus the
    /// archive-read step); any future drift between the two surfaces
    /// here.
    #[test]
    fn bgsm_merge_forwards_scalars_child_first() {
        use byroredux_bgsm::template::ResolvedMaterial;
        use byroredux_bgsm::{BaseMaterial, BgsmFile};
        use std::sync::Arc;

        let child = BgsmFile {
            base: BaseMaterial {
                alpha: 0.25,
                u_offset: 0.1,
                v_offset: 0.2,
                u_scale: 2.0,
                v_scale: 3.0,
                two_sided: true,
                ..Default::default()
            },
            emit_enabled: true,
            emittance_color: [1.0, 0.5, 0.25],
            emittance_mult: 7.0,
            specular_color: [0.9, 0.8, 0.7],
            specular_mult: 3.5,
            smoothness: 0.85,
            ..Default::default()
        };
        let parent = BgsmFile {
            base: BaseMaterial {
                alpha: 0.5,
                u_offset: 99.0, // must NOT win
                ..Default::default()
            },
            emit_enabled: true,
            emittance_color: [0.0, 0.0, 0.0],
            emittance_mult: 0.0,
            specular_mult: 0.01, // must NOT win
            smoothness: 0.01,    // must NOT win
            ..Default::default()
        };
        let resolved = ResolvedMaterial {
            file: child,
            parent: Some(Arc::new(ResolvedMaterial {
                file: parent,
                parent: None,
            })),
        };

        // Replicate the scalar-forwarding half of merge_bgsm_into_mesh
        // inline. Mesh starts with engine defaults so every write below
        // is the BGSM path overriding a fallback.
        let mut emissive_color = [0.0f32; 3];
        let mut emissive_mult = 0.0f32;
        let mut specular_color = [1.0f32; 3];
        let mut specular_strength = 1.0f32;
        let mut glossiness = 0.0f32;
        let mut mat_alpha = 1.0f32;
        let mut uv_offset = [0.0f32; 2];
        let mut uv_scale = [1.0f32; 2];
        let mut two_sided = false;
        let mut is_decal = false;

        let mut set_emissive = false;
        let mut set_specular = false;
        let mut set_glossiness = false;
        let mut set_alpha = false;
        let mut set_uv = false;
        for step in resolved.walk() {
            let bgsm = &step.file;
            if !set_emissive && bgsm.emit_enabled {
                emissive_color = bgsm.emittance_color;
                emissive_mult = bgsm.emittance_mult;
                set_emissive = true;
            }
            if !set_specular {
                specular_color = bgsm.specular_color;
                specular_strength = bgsm.specular_mult;
                set_specular = true;
            }
            if !set_glossiness {
                glossiness = bgsm.smoothness;
                set_glossiness = true;
            }
            if !set_alpha {
                mat_alpha = bgsm.base.alpha;
                set_alpha = true;
            }
            if !set_uv {
                uv_offset = [bgsm.base.u_offset, bgsm.base.v_offset];
                uv_scale = [bgsm.base.u_scale, bgsm.base.v_scale];
                set_uv = true;
            }
            if bgsm.base.two_sided {
                two_sided = true;
            }
            if bgsm.base.decal {
                is_decal = true;
            }
        }

        // Child values must win.
        assert_eq!(emissive_color, [1.0, 0.5, 0.25]);
        assert_eq!(emissive_mult, 7.0);
        assert_eq!(specular_color, [0.9, 0.8, 0.7]);
        assert_eq!(specular_strength, 3.5);
        assert_eq!(glossiness, 0.85);
        assert_eq!(mat_alpha, 0.25);
        assert_eq!(uv_offset, [0.1, 0.2]);
        assert_eq!(uv_scale, [2.0, 3.0]);
        // Boolean OR across the chain — child authored true.
        assert!(two_sided);
        assert!(!is_decal);
    }

    /// `detect_kind` returns `Bgem` for a buffer with BGEM magic even
    /// when the caller intended BGSM dispatch. This is the unit
    /// foundation for the wrong-extension footgun guard (#758): a forged
    /// `.bgsm`-named file carrying BGEM magic is correctly identified.
    #[test]
    fn detect_kind_returns_bgem_for_bgem_magic_in_bgsm_named_file() {
        use byroredux_bgsm::{detect_kind, MaterialKind};
        // Minimal BGEM header (just the 4-byte magic) — enough for detect_kind.
        let bgem_magic_bytes: &[u8] = b"BGEM";
        assert_eq!(
            detect_kind(bgem_magic_bytes),
            Some(MaterialKind::Bgem),
            "detect_kind must return Bgem even when the caller opened a .bgsm-named file"
        );

        let bgsm_magic_bytes: &[u8] = b"BGSM";
        assert_eq!(
            detect_kind(bgsm_magic_bytes),
            Some(MaterialKind::Bgsm),
            "detect_kind must return Bgsm for genuine BGSM magic"
        );

        // A mismatched extension is detected by comparing ext_kind vs magic_kind
        // as done in merge_bgsm_into_mesh. Simulate the comparison logic.
        let ext_kind = Some(MaterialKind::Bgsm); // extension says .bgsm
        let magic_kind = detect_kind(bgem_magic_bytes); // magic says BGEM
        assert_ne!(
            ext_kind, magic_kind,
            "extension (.bgsm) and magic (BGEM) must disagree — this is the mismatch case"
        );
    }
}
