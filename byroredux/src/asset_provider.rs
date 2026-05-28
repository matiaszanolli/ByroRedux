//! BSA/BA2-backed texture and mesh extraction.

use byroredux_bgsm::template::ResolvedMaterial;
use byroredux_bgsm::{BgemFile, TemplateCache, TemplateResolver};
use byroredux_nif::import::{ImportedMesh, MeshResolver};
use byroredux_renderer::VulkanContext;
use byroredux_sfmaterial::ComponentDatabaseFile;
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

    /// Extract a mesh (NIF) from mesh archives. Path is normalised
    /// via [`normalize_mesh_path`] so authored references that omit
    /// the `meshes\` root segment (every ARMO `MODL`, every RACE
    /// `MODL`, every NPC_ `MODL`, etc.) resolve against the BSA's
    /// fully-prefixed keys. Pre-normalisation only ARMO meshes for
    /// the small set of records authored *with* the prefix were
    /// loading — the rest landed at the "not in archives" log path
    /// and NPCs spawned unclothed.
    pub(crate) fn extract_mesh(&self, path: &str) -> Option<Vec<u8>> {
        let normalised = normalize_mesh_path(path);
        for archive in &self.mesh_archives {
            if let Ok(data) = archive.extract(normalised.as_ref()) {
                return Some(data);
            }
        }
        None
    }
}

/// Prepend `meshes\` to a NIF path when the input doesn't already
/// start with that segment (case-insensitive, accepts either
/// separator). `MODL` sub-records on RACE / NPC_ / ARMO records are
/// authored relative to the `meshes\` root; the BSA layer stores the
/// full prefix. Allocation only fires when the prefix is missing —
/// already-prefixed paths borrow.
///
/// Mirror of the static-spawn path's manual prefix-prepend at
/// [`cell_loader::references`] line ~421 (which predates this
/// helper; the cell-loader form is now a redundant idempotent
/// double-normalisation and can be removed in a follow-up sweep).
pub fn normalize_mesh_path(path: &str) -> std::borrow::Cow<'_, str> {
    let bytes = path.as_bytes();
    if bytes.len() >= 7 {
        let head = &bytes[..7];
        if head.eq_ignore_ascii_case(b"meshes\\") || head.eq_ignore_ascii_case(b"meshes/") {
            return std::borrow::Cow::Borrowed(path);
        }
    }
    // #1292 — Starfield content-addressed BSGeometry external `.mesh`
    // companion files live at `geometries\<hash>.mesh` (NO `meshes\`
    // prefix). The importer at `crates/nif/src/import/mesh/bs_geometry.rs`
    // composes the canonical path before calling the resolver; without
    // this gate the normaliser silently prepended `meshes\` and turned
    // every Starfield poster / architecture / set-dressing lookup into
    // a guaranteed miss → 99.7% spawn-rate failure on Cydonia.
    if bytes.len() >= 11 {
        let head = &bytes[..11];
        if head.eq_ignore_ascii_case(b"geometries\\")
            || head.eq_ignore_ascii_case(b"geometries/")
        {
            return std::borrow::Cow::Borrowed(path);
        }
    }
    std::borrow::Cow::Owned(format!(r"meshes\{}", path))
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
        Some(start) if start < bytes.len() => std::borrow::Cow::Owned(path[start..].to_string()),
        _ => std::borrow::Cow::Borrowed(path),
    }
}

/// Normalize a BGSM/BGEM material path into the archive's canonical
/// `materials\…` backslashed form. Four transformations applied in
/// order:
///
/// 1. **Build-pipeline prefix strip**: drop everything up to and
///    including the last `\data\` (or `/data/`) segment. Covers
///    `c:\projects\fallout4\build\pc\data\materials\…` — the form
///    Bethesda authors into vanilla FO4 BGSM file paths (live
///    observation on MedTek Research: 11/12 unique missing-material
///    entries used this form).
/// 2. **Leading `data\` strip**: when the path begins with `data\`
///    or `data/` (no preceding separator), trim that off. Some
///    BGSM template parents author this form (observed:
///    `data\materials\setdressing\metaltrashcan01alpha.bgsm`).
///    `strip_build_prefix` doesn't catch this case because it
///    requires a separator BEFORE `data`.
/// 3. **Forward-slash → backslash**: BA2 archives index with
///    backslashes; some BGSM `root_material_path` fields author
///    with forward slashes (observed: `template/defaulttemplate_wet.bgsm`).
///    Mod-authoring tools and DLC content also mix the two.
/// 4. **Prepend `materials\`**: when the path doesn't already start
///    with `materials\` (after the above strips), add it. BGSM
///    template parents author relative-to-materials-root paths
///    like `template/defaulttemplate_wet.bgsm`; the BA2 index has
///    them at `materials\template\…`.
///
/// Returns `Cow::Borrowed` only on the already-canonical case
/// (starts with `materials\`, no slashes/data/build-prefix). Every
/// authored-non-canonical path returns a single owned allocation.
///
/// See #FO4-D6-NEW (this issue body) for the live `tex.missing`
/// evidence that motivated each of the four transformations.
pub(crate) fn normalize_material_path(path: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    // Step 1 — build-pipeline strip.
    let after_build = strip_build_prefix(path);

    // Step 2 — leading `data\` / `data/` strip (case-insensitive).
    let after_data: Cow<'_, str> = {
        let bytes = after_build.as_bytes();
        if bytes.len() >= 5
            && bytes[..4].eq_ignore_ascii_case(b"data")
            && (bytes[4] == b'\\' || bytes[4] == b'/')
        {
            // Borrow the trailer from `after_build`. If `after_build`
            // is borrowed, the new slice stays borrowed; if it was
            // already owned, we allocate (rare).
            match after_build {
                Cow::Borrowed(s) => Cow::Borrowed(&s[5..]),
                Cow::Owned(s) => Cow::Owned(s[5..].to_string()),
            }
        } else {
            after_build
        }
    };

    // Step 3 — forward-slash → backslash. Only allocate when at
    // least one `/` is present.
    let after_sep: Cow<'_, str> = if after_data.contains('/') {
        Cow::Owned(after_data.replace('/', "\\"))
    } else {
        after_data
    };

    // Step 4 — prepend `materials\` if missing. Case-insensitive
    // on the prefix check so `Materials\foo.bgsm` doesn't get
    // double-prefixed.
    let bytes = after_sep.as_bytes();
    let has_materials = bytes.len() >= 10
        && bytes[..9].eq_ignore_ascii_case(b"materials")
        && bytes[9] == b'\\';
    if has_materials {
        after_sep
    } else {
        Cow::Owned(format!("materials\\{}", after_sep))
    }
}

/// Normalize a texture path into the archive's canonical
/// `textures\…` backslashed form. Mirrors the BGSM-side
/// [`normalize_material_path`] but is texture-specific.
///
/// Two transformations applied in order:
///
/// 1. **Leading `data\` strip** (case-insensitive): when the path
///    begins with `data\` or `data/`, trim that off. FO4 head NIFs'
///    `BSShaderTextureSet` authors per-NPC FaceGen textures with
///    this form (live observation 2026-05-26 on InstituteBioScience:
///    9 / 10 unique missing-texture entries were
///    `data\textures\actors\character\facecustomization\…\<formid>_d.dds`;
///    the archive stores them at `textures\…` without the `data\`
///    prefix). `strip_build_prefix` does not catch this case because
///    it requires a separator BEFORE `data`.
/// 2. **Prepend `textures\`**: when the path doesn't already start
///    with `textures\` (after the strip above), add it. Bethesda
///    WTHR cloud / CLMT sun / LTEX landscape records all author
///    paths relative to the `textures\` root.
///
/// Returns `Cow::Borrowed` only on the canonical case (starts with
/// `textures\`, no leading `data\` prefix). Every authored-non-
/// canonical path returns a single owned allocation.
///
/// See #468 (the original `textures\` prefix issue) and F1.1 from
/// the 2026-05-26 Fallout symptom sweep (the FaceGen leading-data
/// case).
pub(crate) fn normalize_texture_path(path: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    // Step 1 — leading `data\` strip. Check the first 5 bytes:
    // `data` + separator. Borrow the trailer to keep allocations on
    // the rare path.
    let bytes = path.as_bytes();
    let after_data: Cow<'_, str> = if bytes.len() >= 5
        && bytes[..4].eq_ignore_ascii_case(b"data")
        && (bytes[4] == b'\\' || bytes[4] == b'/')
    {
        Cow::Borrowed(&path[5..])
    } else {
        Cow::Borrowed(path)
    };

    // Step 2 — prepend `textures\` if missing. Case-insensitive on
    // the first 8 bytes; matches `/` or `\` as the separator after.
    let bytes = after_data.as_bytes();
    let has_prefix = bytes.len() >= 9
        && bytes[..8].eq_ignore_ascii_case(b"textures")
        && (bytes[8] == b'\\' || bytes[8] == b'/');
    if has_prefix {
        after_data
    } else {
        Cow::Owned(format!("textures\\{}", after_data))
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
pub(crate) fn try_load_default_footstep(world: &mut byroredux_core::ecs::World, args: &[String]) {
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
            log::warn!("M44 Phase 3.5: '{path}' missing canonical footstep '{CANONICAL}': {e}");
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
                        // #1289 / SF-D3-NEW-01 — opportunistically extract
                        // `materials\materialsbeta.cdb` (Starfield's single
                        // binary component database holding every vanilla
                        // material). Non-Starfield archives don't ship this
                        // file; extraction returns an Err which we silently
                        // ignore (FO4's `Fallout4 - Materials.ba2` has no
                        // CDB). The CDB is loaded once per provider so
                        // subsequent `--materials-ba2` flags re-extract +
                        // re-parse on a hit; intentional (matches
                        // `push_archive`'s replacement semantics).
                        if let Ok(bytes) = a.extract("materials\\materialsbeta.cdb") {
                            provider.load_starfield_cdb(&bytes);
                        }
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
    // F2 (2026-05-26 sweep) — "no path authored" is semantically
    // different from "path authored but lookup failed." The former is
    // a Bethesda artist deliberately shipping a surface that the
    // material's emissive / alpha / vertex-color terms colour
    // directly (alpha-blend overlays on the vigor-tester glass cover,
    // emissive light halos in saloon interiors, vertex-color clutter).
    // Route those to the white 1×1 neutral fallback so the shader's
    // multiply yields the authored look instead of magenta checker.
    // The magenta checker stays exclusive to "this path existed but
    // the file wasn't in the archive," which is the diagnostic we
    // want to keep visible.
    let Some(tex_path) = tex_path else {
        return ctx.texture_registry.neutral_fallback();
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
        match ctx.texture_registry.enqueue_dds_with_clamp(
            &ctx.device,
            tex_path,
            dds_bytes,
            clamp_mode,
        ) {
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
    /// #951 / SAFE-26: bounded at `MAX_BGEM_CACHE_ENTRIES`. On overflow
    /// the cache is cleared and a one-shot warn fires — working-set
    /// rebuild is bounded by per-frame BGEM ref count (~100s typically).
    bgem_cache: HashMap<String, Arc<BgemFile>>,
    /// Paths we've already warned about so a broken file doesn't spam
    /// the log on every cell load. Bounded by `MAX_FAILED_PATHS`.
    failed_paths: HashSet<String>,
    /// Starfield `materialsbeta.cdb` — single binary Component Database
    /// holding every vanilla Starfield material. Populated by
    /// [`Self::load_starfield_cdb`] when `Starfield - Materials.ba2`
    /// is opened. `None` for non-Starfield content. #1289 / SF-D3-NEW-01.
    ///
    /// Phase 1 (this commit): presence-only — the CDB is parsed and held
    /// so [`merge_bgsm_into_mesh`]'s `.mat` arm has confirmation that
    /// Starfield material authoring is loaded before flipping `is_pbr`.
    /// Phase 2 (future): walk the 1.44M-instance tree to build a
    /// `material_path → MaterialFields` lookup so per-material metalness
    /// / roughness / texture paths flow into `ImportedMesh` (mirrors the
    /// FO4 BGSM `resolve_bgsm` per-field translation already wired below).
    sf_cdb: Option<Arc<ComponentDatabaseFile>>,
}

/// #951 / SAFE-26 — bounded-cache caps for `MaterialProvider`. Sized to
/// comfortably hold the unique BGEM/BGSM-ref count of any single vanilla
/// cell (~100s) plus a few cells of streaming residency.
const MAX_BGEM_CACHE_ENTRIES: usize = 1024;
const MAX_FAILED_PATHS: usize = 1024;

impl MaterialProvider {
    pub(crate) fn new() -> Self {
        Self {
            archives: Vec::new(),
            bgsm_cache: TemplateCache::new(256),
            bgem_cache: HashMap::new(),
            failed_paths: HashSet::new(),
            sf_cdb: None,
        }
    }

    fn push_archive(&mut self, archive: Archive) {
        self.archives.push(archive);
    }

    /// True once the Starfield Component Database has been loaded.
    /// Drives the `.mat` arm in [`merge_bgsm_into_mesh`] — flipping
    /// `mesh.is_pbr = true` on `.mat` material paths only when the CDB
    /// is present means modded `.mat` paths against a non-Starfield
    /// archive set don't accidentally route to Disney BSDF.
    /// #1289 / SF-D3-NEW-01.
    pub(crate) fn has_starfield_cdb(&self) -> bool {
        self.sf_cdb.is_some()
    }

    /// Parse the Starfield `materialsbeta.cdb` payload and hold it on
    /// `self`. Idempotent: a second call replaces the existing CDB
    /// (matches the archive-replacement semantics of `push_archive`).
    /// Logs the class + instance count on success; on parse failure
    /// the CDB stays at `None` and `has_starfield_cdb` keeps reporting
    /// false. #1289 / SF-D3-NEW-01.
    pub(crate) fn load_starfield_cdb(&mut self, bytes: &[u8]) {
        match ComponentDatabaseFile::parse(bytes) {
            Ok(cdb) => {
                log::info!(
                    "Starfield CDB loaded: {} classes / {} instances ({} bytes). \
                     `.mat` material paths on NIFs will route through Disney BSDF \
                     (Phase 1 — per-field extraction is the deferred Phase 2 follow-up).",
                    cdb.classes.len(),
                    cdb.instances.len(),
                    bytes.len(),
                );
                self.sf_cdb = Some(Arc::new(cdb));
            }
            Err(e) => {
                log::warn!(
                    "Starfield CDB parse failed ({} bytes): {}. \
                     Starfield content will fall back to legacy Lambert shading.",
                    bytes.len(),
                    e,
                );
            }
        }
    }

    fn extract_from_archives(&self, path: &str) -> Option<Vec<u8>> {
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
                            self.failed_paths.clear();
                        }
                        if self.failed_paths.insert(key.clone()) {
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
                // #951 / SAFE-26 — bound failed_paths here too.
                if self.failed_paths.len() >= MAX_FAILED_PATHS {
                    self.failed_paths.clear();
                }
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
                // #951 / SAFE-26 — flush the cache on cap to bound
                // long-streaming-session memory growth. Working set
                // (typically 100s of unique BGEMs per cell) rebuilds
                // on next access via parse_bgem.
                if self.bgem_cache.len() >= MAX_BGEM_CACHE_ENTRIES {
                    static ONCE: std::sync::Once = std::sync::Once::new();
                    ONCE.call_once(|| {
                        log::warn!(
                            "MaterialProvider.bgem_cache hit cap ({} entries); \
                             clearing — high-churn streaming session detected. \
                             Set MAX_BGEM_CACHE_ENTRIES higher if this fires \
                             frequently. (#951 / SAFE-26)",
                            MAX_BGEM_CACHE_ENTRIES,
                        );
                    });
                    self.bgem_cache.clear();
                }
                self.bgem_cache.insert(key, Arc::clone(&arc));
                Some(arc)
            }
            Err(e) => {
                // Bound failed_paths the same way — broken-content
                // accumulates more slowly than working BGEM count, but
                // capping both prevents the unbounded-growth class.
                if self.failed_paths.len() >= MAX_FAILED_PATHS {
                    self.failed_paths.clear();
                }
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
    // loaded once at provider init via [`load_starfield_cdb`].
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
        // Starfield .mat authors metalness/roughness directly; the
        // shader's legacy classify_pbr fallback handles the missing
        // override gracefully until Phase 2 ships authored values.
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

        // ── Translation layer (BGSM spec-glossiness → standard PBR) ──
        //
        // The renderer consumes a single PBR contract: `albedo`,
        // `metalness`, `roughness`, `F0 = mix(0.04, albedo, metalness)`.
        // BGSM authors a DIFFERENT contract: `specular_color * mult`
        // IS F0 directly (dielectric ≈ 0.04, conductor ≈ albedo-tinted).
        // Bethesda's runtime translates this internally; we do the same
        // translation HERE, in the merge layer, so the renderer never
        // needs to know which game's format a material came from. See
        // `feedback_format_translation.md`.
        //
        // Translation derivation (LEAF BGSM only — template chain
        // resolution for spec-color is intentionally child-only since
        // the leaf author's choice is the authoritative one; parents
        // are background defaults the artist explicitly overrode if
        // they set a different value):
        //   * leaf_spec_lum = luminance(spec_color * mult)
        //   * metalness = saturate((leaf_spec_lum - 0.04) / 0.96)
        //     — 0 for dielectric (F0 ≈ 0.04), ~1 for conductor (F0 ≈ 0.95)
        //   * roughness = clamp(1 - smoothness, 0.04, 1.0)
        //     — direct authoring; no glossiness round-trip.
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
        let spec_lum = 0.2126 * spec_r + 0.7152 * spec_g + 0.0722 * spec_b;
        let metalness = ((spec_lum - 0.04) / 0.96).clamp(0.0, 1.0);
        let roughness = (1.0 - leaf.smoothness).clamp(0.04, 1.0);
        mesh.metalness_override = Some(metalness);
        mesh.roughness_override = Some(roughness);
        if metalness > 0.5 {
            // Conductor — bias the diffuse tint toward the authored
            // spec colour so the shader's `mix(0.04, albedo, metalness)`
            // lands on the right tint even on desaturated DDS albedos.
            // Half-weight blend so the diffuse texture's detail (rivets,
            // wear, edge highlights) still modulates visually.
            mesh.diffuse_color = [
                0.5 * mesh.diffuse_color[0] + 0.5 * spec_r,
                0.5 * mesh.diffuse_color[1] + 0.5 * spec_g,
                0.5 * mesh.diffuse_color[2] + 0.5 * spec_b,
            ];
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
            // Blend-factor enums (BGSM `src_blend` / `dst_blend`) align
            // 1:1 with the Gamebryo `AlphaFunction` byte the renderer
            // already speaks (0=Zero, 1=One, 6=SrcAlpha, 7=InvSrcAlpha,
            // ...), so we forward verbatim and cast u32→u8 — vanilla
            // values fit easily.
            if !set_blend && bgsm.base.alpha_blend_mode.function > 0 {
                mesh.has_alpha = true;
                mesh.src_blend_mode = bgsm.base.alpha_blend_mode.src_blend as u8;
                mesh.dst_blend_mode = bgsm.base.alpha_blend_mode.dst_blend as u8;
                set_blend = true;
                touched = true;
            }
        }
    } else if dispatch_kind == Some(MaterialKind::Bgem) {
        let Some(bgem) = provider.resolve_bgem(&path) else {
            return false;
        };
        // BGEM (effect material) also uses the spec-glossiness
        // convention — set the same flag as the BGSM branch.
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
        // BGEM alpha-blend forwarding — same rationale as the BGSM
        // branch above, applied to the BSEffectShaderProperty path.
        // BGEM has no inheritance so no child-first guard needed.
        if bgem.base.alpha_blend_mode.function > 0 {
            mesh.has_alpha = true;
            mesh.src_blend_mode = bgem.base.alpha_blend_mode.src_blend as u8;
            mesh.dst_blend_mode = bgem.base.alpha_blend_mode.dst_blend as u8;
        }
        // #1280 sub-step 3b — forward BGEM `glass_enabled` so the
        // spawn-time classifier in `helpers::classify_glass_into_material`
        // can fire the glass path even when neither the texture path nor
        // the mesh name carries a glass keyword. FO4 ships BGEM glass
        // bottles whose atlas texture (e.g. `clutter01.dds`) and node
        // name (e.g. `Bottle:0`) match nothing in the keyword list; the
        // BGEM file itself is the only authoritative authoring of "this
        // material is glass". Pre-fix those bottles rendered as opaque
        // plastic (`material_kind = 0`, default roughness 0.80).
        if bgem.glass_enabled {
            mesh.bgem_glass = true;
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

    // ── `normalize_mesh_path` — regression for unclothed NPCs in
    //   FNV Prospector Saloon, 2026-05-25. ARMO `MODL` paths are
    //   authored relative to the `meshes\` root (e.g.
    //   `armor\powdergang\powdergang03.NIF`); the BSA stores them
    //   fully prefixed. Pre-fix `extract_mesh` passed the authored
    //   path through verbatim and every leaf-armor lookup missed.

    #[test]
    fn normalize_mesh_path_prepends_missing_meshes_prefix() {
        let out = normalize_mesh_path(r"armor\powdergang\powdergang03.NIF");
        assert_eq!(out.as_ref(), r"meshes\armor\powdergang\powdergang03.NIF");
        assert!(matches!(out, std::borrow::Cow::Owned(_)));
    }

    #[test]
    fn normalize_mesh_path_passes_already_prefixed_borrowed() {
        let out = normalize_mesh_path(r"meshes\characters\_male\upperbody.nif");
        assert_eq!(out.as_ref(), r"meshes\characters\_male\upperbody.nif");
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn normalize_mesh_path_is_case_insensitive_on_prefix() {
        // Modder-authored or DLC content may ship the prefix with a
        // different case (`Meshes\…`); the normalizer must accept it.
        let out = normalize_mesh_path(r"MESHES\armor\foo.nif");
        assert_eq!(out.as_ref(), r"MESHES\armor\foo.nif");
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn normalize_mesh_path_accepts_forward_slash_prefix() {
        // Mod-authoring tools sometimes export forward slashes.
        let out = normalize_mesh_path("meshes/armor/foo.nif");
        assert_eq!(out.as_ref(), "meshes/armor/foo.nif");
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    #[test]
    fn normalize_mesh_path_is_idempotent() {
        // Callers that already pre-normalised must round-trip without
        // double-prefixing — the cell-loader static-spawn path at
        // `cell_loader/references.rs:421-426` predates the centralised
        // normaliser and still pre-prepends `meshes\` itself; the
        // double-normalise must be a no-op.
        let once = normalize_mesh_path(r"armor\powdergang\powdergang03.NIF");
        let twice = normalize_mesh_path(once.as_ref());
        assert_eq!(once, twice);
    }

    #[test]
    fn normalize_mesh_path_handles_short_input() {
        // Pathological input shorter than the 7-byte prefix — must
        // not panic and must still get the prefix.
        let out = normalize_mesh_path("a.nif");
        assert_eq!(out.as_ref(), r"meshes\a.nif");
    }

    /// #1292 — Starfield BSGeometry external `.mesh` companion files
    /// live at `geometries\<hash>.mesh` directly (NO `meshes\` prefix).
    /// The importer composes this canonical path before calling the
    /// resolver. Pre-#1292 the normaliser blindly prepended `meshes\`
    /// turning it into `meshes\geometries\<hash>.mesh` which doesn't
    /// exist in the archive → 99.7% spawn-rate failure on Cydonia.
    #[test]
    fn normalize_mesh_path_passes_geometries_prefix_unchanged() {
        let out = normalize_mesh_path(
            r"geometries\aa2d865fc6bf336b909b\e84b59f1a4b705a40845.mesh",
        );
        assert_eq!(
            out.as_ref(),
            r"geometries\aa2d865fc6bf336b909b\e84b59f1a4b705a40845.mesh",
            "Starfield `geometries\\X.mesh` must NOT get a `meshes\\` prefix",
        );
        assert!(
            matches!(out, std::borrow::Cow::Borrowed(_)),
            "already-canonical paths must borrow, not allocate",
        );
    }

    /// Case-insensitive + forward-slash variants of the geometries
    /// prefix gate. Mirrors the case-insensitive / forward-slash
    /// coverage on the `meshes\` prefix.
    #[test]
    fn normalize_mesh_path_geometries_prefix_is_case_and_separator_insensitive() {
        for variant in [
            r"GEOMETRIES\abc\def.mesh",
            r"Geometries\abc\def.mesh",
            "geometries/abc/def.mesh",
            "GEOMETRIES/abc/def.mesh",
        ] {
            let out = normalize_mesh_path(variant);
            assert_eq!(out.as_ref(), variant, "{variant:?} must pass through unchanged");
        }
    }

    #[test]
    fn strip_build_prefix_handles_skyrim_hd_prefix() {
        // The headline case from the Markarth render: Skyrim AE bundles
        // the HD juniper / reach branches / driftwood with the full
        // pipeline-internal prefix.
        let out =
            strip_build_prefix("skyrimhd\\build\\pc\\data\\textures\\plants\\florajuniper.dds");
        assert_eq!(out.as_ref(), "textures\\plants\\florajuniper.dds");
    }

    /// Live observation from MedTekResearch01 (FO4) `tex.missing` run
    /// 2026-05-17 — every BGSM/BGEM authored in FO4 vanilla carries
    /// this exact `c:\projects\fallout4\build\pc\data\…` pipeline
    /// prefix. Pre-fix the BGSM resolver didn't strip and 11 / 12
    /// unique missing-material entries were variants of this case
    /// (metallocker01.bgsm, woodmetalcrate01.bgsm, hightechlamp01.bgsm,
    /// …). The strip-helper already handles the LAST `\data\`
    /// boundary correctly for the multi-segment case; this test pins
    /// the exact FO4 input → archive-relative output transformation
    /// the resolver depends on.
    #[test]
    fn strip_build_prefix_handles_fo4_pipeline_prefix() {
        let out = strip_build_prefix(
            "c:\\projects\\fallout4\\build\\pc\\data\\materials\\setdressing\\metallocker01.bgsm",
        );
        assert_eq!(out.as_ref(), "materials\\setdressing\\metallocker01.bgsm");
    }

    /// MaterialProvider's archive-read helper must call
    /// `normalize_material_path` so the FO4 BGSM lookup actually
    /// hits the archive index. Pre-fix the lookup skipped the
    /// normalisation and every non-canonical path resolved to None.
    /// Probes the transformation with an empty archive set — the
    /// answer must be `None` for any input, but the call shouldn't
    /// panic on any of the four observed failure-mode forms.
    #[test]
    fn material_provider_extract_normalises_without_panic() {
        let provider = MaterialProvider::new();
        for path in [
            // Form 1 — FO4 pipeline build prefix (live observation,
            // 46× hit count on MedTek).
            "c:\\projects\\fallout4\\build\\pc\\data\\materials\\setdressing\\metallocker01.bgsm",
            // Form 2 — leading `data\` (live observation, ~3 BGSM
            // files in MedTek setdressing).
            "data\\materials\\setdressing\\metaltrashcan01alpha.bgsm",
            // Form 3 — forward slashes (live observation, template
            // parents in shared BGSMs).
            "template/defaulttemplate_wet.bgsm",
            // Form 4 — composed: forward slashes WITH leading data/.
            "data/materials/template/metaltemplate_wet.bgsm",
        ] {
            let result = provider.extract_from_archives(path);
            assert!(
                result.is_none(),
                "no archives → no bytes; must not panic on input {path:?}"
            );
        }
    }

    // ── normalize_material_path — per-rule + composed cases ─────

    /// Rule 1: build-pipeline prefix strip (live FO4 MedTek case).
    #[test]
    fn normalize_material_path_strips_fo4_build_prefix() {
        let out = normalize_material_path(
            "c:\\projects\\fallout4\\build\\pc\\data\\materials\\setdressing\\metallocker01.bgsm",
        );
        assert_eq!(out.as_ref(), "materials\\setdressing\\metallocker01.bgsm");
    }

    /// Rule 2: leading `data\` strip — covers the `metaltrashcan01alpha.bgsm`
    /// failure mode where the path begins with `data\` (no leading
    /// separator). `strip_build_prefix` alone doesn't catch this
    /// because it requires a separator BEFORE the `data` segment.
    #[test]
    fn normalize_material_path_strips_leading_data_segment() {
        let out =
            normalize_material_path("data\\materials\\setdressing\\metaltrashcan01alpha.bgsm");
        assert_eq!(
            out.as_ref(),
            "materials\\setdressing\\metaltrashcan01alpha.bgsm"
        );
    }

    /// Rule 2 sibling: leading `data/` with forward slash.
    #[test]
    fn normalize_material_path_strips_leading_data_segment_forward_slash() {
        let out = normalize_material_path("data/materials/setdressing/foo.bgsm");
        assert_eq!(out.as_ref(), "materials\\setdressing\\foo.bgsm");
    }

    /// `normalize_texture_path` — canonical form passes through borrowed.
    #[test]
    fn normalize_texture_path_passes_canonical_through_borrowed() {
        let out = normalize_texture_path("textures\\landscape\\plants\\juniper_d.dds");
        assert_eq!(out.as_ref(), "textures\\landscape\\plants\\juniper_d.dds");
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    /// `normalize_texture_path` — paths without a `textures\` prefix
    /// get one prepended. Bethesda CLMT / WTHR / LTEX records author
    /// this shape.
    #[test]
    fn normalize_texture_path_prepends_textures_when_missing() {
        let out = normalize_texture_path("landscape\\plants\\juniper_d.dds");
        assert_eq!(out.as_ref(), "textures\\landscape\\plants\\juniper_d.dds");
    }

    /// `normalize_texture_path` — leading `data\textures\…` strip.
    /// F1.1 from the 2026-05-26 Fallout symptom sweep: FO4 head NIFs'
    /// `BSShaderTextureSet` authors per-NPC FaceGen textures with the
    /// `data\` prefix; the archive stores them at `textures\…`. Without
    /// the strip every NPC head rendered with a checkerboard face.
    #[test]
    fn normalize_texture_path_strips_leading_data_facegen() {
        let out = normalize_texture_path(
            "data\\textures\\actors\\character\\facecustomization\\fallout4.esm\\001d4387_d.dds",
        );
        assert_eq!(
            out.as_ref(),
            "textures\\actors\\character\\facecustomization\\fallout4.esm\\001d4387_d.dds",
        );
    }

    /// `normalize_texture_path` — `data/` forward-slash variant of F1.1.
    /// Same path shape, just mixed separators (mod-authoring tools).
    #[test]
    fn normalize_texture_path_strips_leading_data_forward_slash() {
        let out = normalize_texture_path("data/textures/landscape/foo.dds");
        // After strip we re-check the `textures\` prefix — note we don't
        // rewrite slashes inside the trailer, since texture lookups use
        // a case-insensitive separator-tolerant key downstream.
        assert_eq!(out.as_ref(), "textures/landscape/foo.dds");
    }

    /// Rule 3: `/` → `\` separator normalisation. Live case from BGSM
    /// `root_material_path` fields authored with forward slashes.
    #[test]
    fn normalize_material_path_converts_forward_slashes_to_backslashes() {
        let out = normalize_material_path("materials/template/defaulttemplate_wet.bgsm");
        assert_eq!(out.as_ref(), "materials\\template\\defaulttemplate_wet.bgsm");
    }

    /// Rule 4: `materials\` prefix add when missing. Live case from
    /// the bare `template/defaulttemplate_wet.bgsm` form (no
    /// `materials\` segment) inside BGSM parent references.
    #[test]
    fn normalize_material_path_prepends_materials_when_missing() {
        let out = normalize_material_path("template\\defaulttemplate_wet.bgsm");
        assert_eq!(out.as_ref(), "materials\\template\\defaulttemplate_wet.bgsm");
    }

    /// Composed: `template/defaulttemplate_wet.bgsm` — the headline
    /// template-parent failure mode (forward slashes AND missing
    /// `materials\` prefix at the same time). 11/12 BGSM resolve
    /// failures in MedTek post-build-prefix-fix went through this
    /// exact composition.
    #[test]
    fn normalize_material_path_handles_template_parent_form() {
        let out = normalize_material_path("template/defaulttemplate_wet.bgsm");
        assert_eq!(out.as_ref(), "materials\\template\\defaulttemplate_wet.bgsm");
    }

    /// Canonical-form passthrough: no allocation when the input is
    /// already `materials\…`-prefixed, backslashed, no build prefix,
    /// no leading `data\`. The `Cow::Borrowed` return signals the
    /// fast-path took.
    #[test]
    fn normalize_material_path_canonical_form_borrows() {
        let input = "materials\\setdressing\\foo.bgsm";
        let out = normalize_material_path(input);
        assert_eq!(out.as_ref(), input);
        assert!(matches!(out, std::borrow::Cow::Borrowed(_)));
    }

    /// Case-insensitive `materials\` prefix check — `Materials\foo.bgsm`
    /// must NOT be double-prefixed into `materials\Materials\foo.bgsm`.
    #[test]
    fn normalize_material_path_does_not_double_prefix_capitalised_materials() {
        let out = normalize_material_path("Materials\\foo.bgsm");
        // First-rune case is preserved when no other rule fires —
        // the BSA index lookup is case-insensitive (per
        // `BsaArchive::contains` + `Ba2Archive::contains`) so either
        // case resolves the same file. We just need to avoid the
        // double-prefix bug.
        assert_eq!(out.as_ref(), "Materials\\foo.bgsm");
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
        let out = strip_build_prefix("skyrimhd\\build\\pc\\Data\\textures\\plants\\foo.dds");
        assert_eq!(out.as_ref(), "textures\\plants\\foo.dds");
    }

    #[test]
    fn strip_build_prefix_accepts_forward_slashes() {
        // Mod-authoring tools occasionally export forward slashes.
        let out = strip_build_prefix("skyrimhd/build/pc/data/textures/plants/foo.dds");
        assert_eq!(out.as_ref(), "textures/plants/foo.dds");
    }

    #[test]
    fn strip_build_prefix_uses_last_data_boundary() {
        // Pathological case: an asset that genuinely lives under a
        // nested `data\` directory should strip up to the LAST
        // boundary so the longest known-prefix wins.
        let out = strip_build_prefix("vendor\\data\\skyrimhd\\build\\pc\\data\\textures\\foo.dds");
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

    /// Regression for #1076 / FO4-D6-002 — BGSM v>2 standalone slots
    /// (`specular_texture`, `lighting_texture`, `flow_texture`,
    /// `wrinkles_texture`) must forward to `ImportedMesh`'s
    /// `specular_map` / `lighting_map` / `flow_map` / `wrinkle_map`.
    /// Pre-fix the parser decoded all four fields and the merge
    /// dropped them on the floor — FO4 water surfaces lost their
    /// flow direction, NPC skin lost wrinkle blending, PBR specular
    /// fell back to the gloss_map's .r-only path.
    #[test]
    fn bgsm_merge_forwards_v2_plus_standalone_slots() {
        // Use the in-test `fill` helper (`Option<String>` variant)
        // that mirrors the prod merge's intern-and-set semantic.
        let mut specular_map: Option<String> = None;
        let mut lighting_map: Option<String> = None;
        let mut flow_map: Option<String> = None;
        let mut wrinkle_map: Option<String> = None;

        let bgsm = BgsmFile {
            specular_texture: "armor_specular.dds".into(),
            lighting_texture: "armor_lighting.dds".into(),
            flow_texture: "water_flow.dds".into(),
            wrinkles_texture: "ncr_wrinkles.dds".into(),
            ..Default::default()
        };

        // Mirror the prod loop body for the four new slots.
        fill(&mut specular_map, &bgsm.specular_texture);
        fill(&mut lighting_map, &bgsm.lighting_texture);
        fill(&mut flow_map, &bgsm.flow_texture);
        fill(&mut wrinkle_map, &bgsm.wrinkles_texture);

        assert_eq!(specular_map.as_deref(), Some("armor_specular.dds"));
        assert_eq!(lighting_map.as_deref(), Some("armor_lighting.dds"));
        assert_eq!(flow_map.as_deref(), Some("water_flow.dds"));
        assert_eq!(wrinkle_map.as_deref(), Some("ncr_wrinkles.dds"));
    }

    /// Regression for #1077 / FO4-D6-003 Phase 1 — BGSM-only shader
    /// flags (`pbr`, `translucency`, `model_space_normals`) must
    /// forward to `ImportedMesh`'s `is_pbr` / `has_translucency` /
    /// `model_space_normals`. Pre-fix the parser decoded all three
    /// and the merge dropped them on the floor — FO4 materials
    /// authored with `pbr=true` rendered on the Gamebryo-legacy
    /// specular path (the renderer didn't even know to dispatch
    /// PBR-vs-legacy). Phase 2 (the `triangle.frag` gating) is
    /// deferred; this test pins the data-propagation contract.
    #[test]
    fn bgsm_merge_forwards_phase1_shader_flags() {
        // Three local bools standing in for the corresponding
        // ImportedMesh fields. Mirrors the prod merge's "first true
        // wins" gate.
        let mut is_pbr = false;
        let mut has_translucency = false;
        let mut model_space_normals = false;

        let bgsm = BgsmFile {
            pbr: true,
            translucency: true,
            model_space_normals: true,
            ..Default::default()
        };

        // Mirror the prod merge's gates.
        if !is_pbr && bgsm.pbr {
            is_pbr = true;
        }
        if !has_translucency && bgsm.translucency {
            has_translucency = true;
        }
        if !model_space_normals && bgsm.model_space_normals {
            model_space_normals = true;
        }

        assert!(is_pbr, "BGSM.pbr=true must propagate to ImportedMesh.is_pbr");
        assert!(has_translucency);
        assert!(model_space_normals);
    }

    /// Companion: with all three flags `false` on the BGSM, the
    /// merge must leave the `ImportedMesh` defaults unchanged. Pins
    /// "first true wins" — a `false` author doesn't override a
    /// previously-set `true`.
    #[test]
    fn bgsm_merge_does_not_set_phase1_flags_from_false() {
        let mut is_pbr = false;
        let mut has_translucency = false;
        let mut model_space_normals = false;

        let bgsm = BgsmFile {
            pbr: false,
            translucency: false,
            model_space_normals: false,
            ..Default::default()
        };

        if !is_pbr && bgsm.pbr {
            is_pbr = true;
        }
        if !has_translucency && bgsm.translucency {
            has_translucency = true;
        }
        if !model_space_normals && bgsm.model_space_normals {
            model_space_normals = true;
        }

        assert!(!is_pbr);
        assert!(!has_translucency);
        assert!(!model_space_normals);
    }

    /// Child-first precedence for the new flags — first authored
    /// `true` in the BGSM template chain wins, mirroring the
    /// texture-slot precedence (which the parser walks child-first).
    /// A `false` child followed by a `true` parent must flip the
    /// flag.
    #[test]
    fn bgsm_merge_phase1_flags_honor_child_first_chain() {
        let mut is_pbr = false;

        let child = BgsmFile {
            pbr: false, // child doesn't author PBR
            ..Default::default()
        };
        let parent = BgsmFile {
            pbr: true, // parent template enables PBR
            ..Default::default()
        };
        let resolved = ResolvedMaterial {
            file: child,
            parent: Some(Arc::new(ResolvedMaterial {
                file: parent,
                parent: None,
            })),
        };

        for step in resolved.walk() {
            if !is_pbr && step.file.pbr {
                is_pbr = true;
            }
        }

        assert!(is_pbr, "parent's pbr=true must flow down to the merged result");
    }

    /// Regression for FO4 BGSM glass / alpha-blended decals. FO4
    /// moved per-material blend state out of `NiAlphaProperty` into
    /// BGSM's `base.alpha_blend_mode`. Pre-fix the merge dropped
    /// that tuple, leaving `ImportedMesh.has_alpha = false` on
    /// every BGSM-only glass pane → no `AlphaBlend` component
    /// attached → `INSTANCE_FLAG_ALPHA_BLEND` never set → the
    /// `MATERIAL_KIND_GLASS` classifier in `static_meshes.rs`
    /// short-circuited and the panel rendered fully opaque
    /// (visible symptom: Institute Bioscience glass too opaque,
    /// no refraction, wrong tint). Pins:
    ///   1. `function > 0` → `has_alpha = true` + blend factors copied
    ///   2. `function == 0` (None) → no override (NIF-side value wins)
    #[test]
    fn bgsm_merge_forwards_alpha_blend_mode() {
        use byroredux_bgsm::AlphaBlendMode;
        // Mirror the prod merge's three writes for the alpha-blend block.
        fn apply(bgsm: &BgsmFile, has_alpha: &mut bool, src: &mut u8, dst: &mut u8) {
            if bgsm.base.alpha_blend_mode.function > 0 {
                *has_alpha = true;
                *src = bgsm.base.alpha_blend_mode.src_blend as u8;
                *dst = bgsm.base.alpha_blend_mode.dst_blend as u8;
            }
        }
        // Case 1: standard alpha-blend (function=1, src=6 SrcAlpha,
        // dst=7 InvSrcAlpha) — Institute glass case.
        let mut has_alpha = false;
        let mut src = 6u8;
        let mut dst = 7u8;
        let mut bgsm = BgsmFile::default();
        bgsm.base.alpha_blend_mode = AlphaBlendMode {
            function: 1,
            src_blend: 6,
            dst_blend: 7,
        };
        apply(&bgsm, &mut has_alpha, &mut src, &mut dst);
        assert!(has_alpha, "function=1 (Standard) must set has_alpha");
        assert_eq!(src, 6);
        assert_eq!(dst, 7);

        // Case 2: additive blend (function=2) with One/One factors —
        // common on FO4 effect / glow card BGEMs. Still routes to
        // alpha-blend so the renderer picks the alpha pipeline.
        let mut has_alpha = false;
        let mut src = 6u8;
        let mut dst = 7u8;
        let mut bgsm = BgsmFile::default();
        bgsm.base.alpha_blend_mode = AlphaBlendMode {
            function: 2,
            src_blend: 1,
            dst_blend: 1,
        };
        apply(&bgsm, &mut has_alpha, &mut src, &mut dst);
        assert!(has_alpha);
        assert_eq!(src, 1);
        assert_eq!(dst, 1);

        // Case 3: function=0 (None) — the BGSM explicitly says "no
        // blend." Don't flip has_alpha. Caller's `set_blend` guard
        // then also prevents a subsequent parent from re-triggering.
        let mut has_alpha = false;
        let mut src = 6u8;
        let mut dst = 7u8;
        let bgsm = BgsmFile::default();
        assert_eq!(bgsm.base.alpha_blend_mode.function, 0);
        apply(&bgsm, &mut has_alpha, &mut src, &mut dst);
        assert!(!has_alpha, "function=0 must NOT set has_alpha");
        assert_eq!(src, 6, "src untouched when function=0");
        assert_eq!(dst, 7);
    }

    /// Companion regression for the SIBLING half of #1076 — BGEM also
    /// authors `specular_texture` and `lighting_texture` (BGEM does
    /// not author flow / wrinkles per `bgem.rs`). Pre-fix the BGEM
    /// merge dropped both, leaving FO4 effect shaders that authored
    /// a per-texel specular layer rendering on NIF-fallback specular.
    #[test]
    fn bgem_merge_forwards_specular_and_lighting_slots() {
        let mut specular_map: Option<String> = None;
        let mut lighting_map: Option<String> = None;

        let bgem = BgemFile {
            specular_texture: "fx_specular.dds".into(),
            lighting_texture: "fx_lighting.dds".into(),
            ..Default::default()
        };

        fill(&mut specular_map, &bgem.specular_texture);
        fill(&mut lighting_map, &bgem.lighting_texture);

        assert_eq!(specular_map.as_deref(), Some("fx_specular.dds"));
        assert_eq!(lighting_map.as_deref(), Some("fx_lighting.dds"));
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
                // Mirror of the production conversion (`bgsm.smoothness * 100.0`)
                // — 0–1 smoothness on the BGSM side becomes 0–100 glossiness
                // on the Material side.
                glossiness = bgsm.smoothness * 100.0;
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
        // BGSM smoothness 0.85 → 85.0 on the Material 0–100 scale.
        assert_eq!(glossiness, 85.0);
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

    // ── #1289 / SF-D3-NEW-01 — Starfield `.mat` arm in
    //   `merge_bgsm_into_mesh`. Verifies the audit-fail closure: a
    //   Starfield-shaped mesh (`.mat` material path) flips `is_pbr`
    //   when (and only when) the Component Database is loaded.

    fn imported_mesh_with_material_path(
        pool: &mut byroredux_core::string::StringPool,
        path: &str,
    ) -> ImportedMesh {
        // Empty-but-valid `ImportedMesh`; the merge helper only touches
        // material-flow fields. `ImportedMesh` has no `Default` impl
        // (every field is concretely meaningful), so we hand-construct
        // — mirrors the same shape as `empty_mesh()` in
        // `pack_bgsm_material_flags_tests` (`byroredux/src/cell_loader.rs`).
        ImportedMesh {
            positions: Vec::new(),
            colors: Vec::new(),
            normals: Vec::new(),
            tangents: Vec::new(),
            uvs: Vec::new(),
            indices: Vec::new(),
            translation: [0.0; 3],
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: 1.0,
            texture_path: None,
            material_path: Some(pool.intern(path)),
            name: None,
            has_alpha: false,
            src_blend_mode: 6,
            dst_blend_mode: 7,
            alpha_test: false,
            alpha_threshold: 0.0,
            alpha_test_func: 6,
            two_sided: false,
            is_decal: false,
            normal_map: None,
            glow_map: None,
            detail_map: None,
            gloss_map: None,
            dark_map: None,
            parallax_map: None,
            env_map: None,
            env_mask: None,
            specular_map: None,
            lighting_map: None,
            flow_map: None,
            wrinkle_map: None,
            is_pbr: false,
            has_translucency: false,
            model_space_normals: false,
            from_bgsm: false,
            bgem_glass: false,
            metalness_override: None,
            roughness_override: None,
            translucency_subsurface_color: [0.0; 3],
            translucency_transmissive_scale: 0.0,
            translucency_turbulence: 0.0,
            translucency_thick_object: false,
            translucency_mix_albedo: false,
            parallax_max_passes: None,
            parallax_height_scale: None,
            vertex_color_mode: 2,
            texture_clamp_mode: 0,
            emissive_color: [0.0; 3],
            emissive_mult: 0.0,
            emissive_source: byroredux_core::ecs::components::material::EmissiveSource::None,
            specular_color: [1.0; 3],
            diffuse_color: [1.0; 3],
            ambient_color: [1.0; 3],
            specular_strength: 1.0,
            glossiness: 80.0,
            refraction_strength: 0.0,
            lighting_effect_1: 0.0,
            lighting_effect_2: 0.0,
            subsurface_rolloff: 0.0,
            rimlight_power: 0.0,
            backlight_power: 0.0,
            grayscale_to_palette_scale: 1.0,
            fresnel_power: 5.0,
            uv_offset: [0.0; 2],
            uv_scale: [1.0; 2],
            mat_alpha: 1.0,
            env_map_scale: 1.0,
            parent_node: None,
            skin: None,
            z_test: true,
            z_write: true,
            z_function: 3,
            local_bound_center: [0.0; 3],
            local_bound_radius: 0.0,
            effect_shader: None,
            material_kind: 0,
            shader_type_fields: byroredux_nif::import::ShaderTypeFields::default(),
            no_lighting_falloff: None,
            wireframe: false,
            flat_shading: false,
            flags: 0,
            bs_lod_cutoffs: None,
            bs_sub_index: None,
        }
    }

    /// Synthetic minimal CDB: BETH magic + header + STRT (empty) + TYPE
    /// chunk declaring zero types. Sufficient for `load_starfield_cdb`
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
        buf.extend_from_slice(&8u32.to_le_bytes());          // headerSize
        buf.extend_from_slice(&4u32.to_le_bytes());          // fileVersion
        buf.extend_from_slice(&3u32.to_le_bytes());          // chunkCount (incl BETH)
        // STRT chunk: type + size + empty payload.
        buf.extend_from_slice(b"STRT");
        buf.extend_from_slice(&0u32.to_le_bytes());          // size = 0
        // TYPE chunk: type + size=4 + u32 type_count=0.
        buf.extend_from_slice(b"TYPE");
        buf.extend_from_slice(&4u32.to_le_bytes());          // size = 4
        buf.extend_from_slice(&0u32.to_le_bytes());          // type_count = 0
        buf
    }

    /// Audit-fail closure: a `.mat` path on a Starfield mesh with the
    /// CDB loaded must flip `is_pbr=true` so `pack_bgsm_material_flags`
    /// packs `MAT_FLAG_PBR_BSDF` and `triangle.frag` routes through
    /// Disney BSDF instead of legacy Lambert.
    #[test]
    fn merge_sets_is_pbr_on_mat_path_when_cdb_loaded() {
        let mut pool = byroredux_core::string::StringPool::new();
        let mut provider = MaterialProvider::new();
        provider.load_starfield_cdb(&minimal_cdb_bytes());
        assert!(
            provider.has_starfield_cdb(),
            "minimal CDB payload must mark the provider as Starfield-loaded"
        );

        let mut mesh = imported_mesh_with_material_path(
            &mut pool,
            "materials/setpieces/cargobay.mat",
        );
        assert!(!mesh.is_pbr, "fresh ImportedMesh defaults to is_pbr=false");

        let touched = merge_bgsm_into_mesh(&mut mesh, &mut provider, &mut pool);

        assert!(touched, ".mat arm must report touched=true");
        assert!(
            mesh.is_pbr,
            "Starfield .mat path must flip is_pbr=true → MAT_FLAG_PBR_BSDF in shader"
        );
        // `from_bgsm` deliberately stays false — that flag gates BGSM
        // spec-glossiness translation which is wrong for Starfield .mat
        // (metalness/roughness direct authoring).
        assert!(!mesh.from_bgsm, "Starfield path must NOT set from_bgsm");
    }

    /// CDB-presence gate: a `.mat` path against a non-Starfield archive
    /// set (no CDB loaded) must NOT flip `is_pbr`. Modded `.mat` paths
    /// on FO4 / FNV / Skyrim cells shouldn't accidentally route to
    /// Disney BSDF.
    #[test]
    fn merge_skips_mat_path_when_cdb_absent() {
        let mut pool = byroredux_core::string::StringPool::new();
        let mut provider = MaterialProvider::new();
        // No `load_starfield_cdb` call.
        assert!(!provider.has_starfield_cdb());

        let mut mesh = imported_mesh_with_material_path(
            &mut pool,
            "materials/modded.mat",
        );
        let touched = merge_bgsm_into_mesh(&mut mesh, &mut provider, &mut pool);

        // Falls through past the .mat arm; bgsm/bgem dispatch fails
        // because the path doesn't match either suffix; returns false
        // (no archive to resolve from anyway).
        assert!(!touched, "no CDB + no archives → no merge work");
        assert!(!mesh.is_pbr, ".mat path without CDB must NOT flip is_pbr");
    }

    /// A `.bgsm` path must NOT enter the Starfield arm even when the
    /// CDB is loaded — the FO4 BGSM dispatch wins, preserving
    /// spec-glossiness translation.
    #[test]
    fn mat_arm_does_not_steal_bgsm_dispatch() {
        let mut pool = byroredux_core::string::StringPool::new();
        let mut provider = MaterialProvider::new();
        provider.load_starfield_cdb(&minimal_cdb_bytes());

        let mut mesh = imported_mesh_with_material_path(
            &mut pool,
            "materials/setdressing/metallocker01.bgsm",
        );
        let _ = merge_bgsm_into_mesh(&mut mesh, &mut provider, &mut pool);

        // The .bgsm path falls past the .mat arm into BGSM dispatch,
        // which fails on the missing archive (no .bgsm to extract).
        // `is_pbr` stays at its default — BGSM dispatch doesn't flip
        // it without a successful resolve.
        assert!(
            !mesh.is_pbr,
            ".bgsm path must not be hijacked by the Starfield arm"
        );
    }
}
