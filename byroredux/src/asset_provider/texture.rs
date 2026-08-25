use super::*;

use byroredux_nif::import::{MaterialTextureSet, MeshResolver};
use byroredux_renderer::{TextureColorSpace, VulkanContext};

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

    /// Whether a texture exists, without extracting or decompressing it.
    pub(crate) fn has_texture(&self, path: &str) -> bool {
        let normalized = normalize_texture_path(path);
        self.texture_archives
            .iter()
            .any(|archive| archive.contains(normalized.as_ref()))
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

    /// Whether a mesh exists, without paying for extraction + decompression.
    ///
    /// The baked-LOD band selector (`cell_loader::lod_bands`) probes one
    /// `.btr` / `.bto` per candidate quad on every reconcile purely to decide
    /// which level to draw; going through [`Self::extract_mesh`] for that
    /// would inflate + discard a whole macro-mesh per probe.
    pub(crate) fn has_mesh(&self, path: &str) -> bool {
        let normalised = normalize_mesh_path(path);
        self.mesh_archives
            .iter()
            .any(|archive| archive.contains(normalised.as_ref()))
    }
}

impl MeshResolver for TextureProvider {
    fn resolve(&self, mesh_name: &str) -> Option<Vec<u8>> {
        self.extract_mesh(mesh_name)
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

/// M44 water acoustics — load a physical splash one-shot when a sound BSA is
/// supplied. Candidate paths cover the Skyrim and Fallout naming variants;
/// the first archive hit wins, while missing audio remains a silent no-op.
pub(crate) fn try_load_default_water_splash(
    world: &mut byroredux_core::ecs::World,
    args: &[String],
) {
    let Some(path) = args
        .windows(2)
        .find(|pair| pair[0] == "--sounds-bsa")
        .map(|pair| pair[1].as_str())
    else {
        return;
    };
    let archive = match Archive::open(path) {
        Ok(a) => a,
        Err(e) => {
            log::warn!("water acoustics: open --sounds-bsa '{path}': {e}");
            return;
        }
    };
    const CANDIDATES: &[&str] = &[
        r"sound\fx\phy\water\phy_water_m_01.wav",
        r"sound\fx\phy\phy_water_m_01.wav",
        r"sound\fx\fst\water\walk\l\fst_water_walk_03.wav",
    ];
    let Some((chosen, bytes)) = CANDIDATES.iter().find_map(|candidate| {
        archive
            .extract(candidate)
            .ok()
            .map(|bytes| (*candidate, bytes))
    }) else {
        log::warn!("water acoustics: no splash candidate found in '{path}'");
        return;
    };
    let sound = match byroredux_audio::load_sound_from_bytes(bytes) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("water acoustics: decode '{chosen}': {e}");
            return;
        }
    };
    world
        .resource_mut::<crate::components::WaterAudioConfig>()
        .splash_sound = Some(std::sync::Arc::new(sound));
    log::info!("water acoustics: loaded '{chosen}' from '{path}'");
}

/// #1776 — the aggregate "requested but zero opened" check, pulled out pure so
/// the guard is unit-testable. Returns one error line per archive kind that was
/// requested on the CLI yet resolved to zero opened archives — the wrong-CWD /
/// mistyped-path trap (bare `--bsa` names resolve against the current
/// directory, not the `--esm` folder). A kind that wasn't requested at all (a
/// loose-NIF run with no `--bsa`) is never flagged.
fn missing_archive_errors(
    mesh_requested: bool,
    mesh_empty: bool,
    textures_requested: bool,
    textures_empty: bool,
) -> Vec<&'static str> {
    let mut errs = Vec::new();
    if mesh_requested && mesh_empty {
        errs.push(
            "--bsa was specified but 0 mesh archives opened — check the path / CWD \
             (bare names resolve against the current directory, not the --esm folder). \
             The scene will load near-empty.",
        );
    }
    if textures_requested && textures_empty {
        errs.push(
            "--textures-bsa was specified but 0 texture archives opened — check the \
             path / CWD. Surfaces will render with placeholder textures.",
        );
    }
    errs
}

/// Build a TextureProvider from CLI arguments.
pub(crate) fn build_texture_provider(args: &[String]) -> TextureProvider {
    let mut provider = TextureProvider::new();
    let mut mesh_requested = false;
    let mut textures_requested = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--textures-bsa" => {
                if let Some(path) = args.get(i + 1) {
                    textures_requested = true;
                    open_with_numeric_siblings(path, "textures", &mut provider.texture_archives);
                    i += 2;
                    continue;
                }
            }
            "--bsa" => {
                if let Some(path) = args.get(i + 1) {
                    mesh_requested = true;
                    open_with_numeric_siblings(path, "mesh", &mut provider.mesh_archives);
                    i += 2;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    // #1776 — `open_with_numeric_siblings` already warns per failed archive, but
    // a run that requested archives yet opened NONE loads near-empty and prints
    // a spurious bench FPS (~36 entities / ~1792 FPS) that reads as real data.
    // Escalate the aggregate "0 opened despite a request" to a loud error so a
    // misconfigured (wrong-CWD / mistyped) invocation is self-evident in the log.
    for err in missing_archive_errors(
        mesh_requested,
        provider.mesh_archives.is_empty(),
        textures_requested,
        provider.texture_archives.is_empty(),
    ) {
        log::error!("{err}");
    }
    provider
}

/// Resolve a texture path to a texture handle, with BSA/BA2 lookup and caching.
///
/// Derive the Bethesda load-time normal-map sibling of a diffuse texture
/// path: `<base_stem>_n.dds`. Oblivion (and FO3/FNV) ship tangent-space
/// normal maps via this filename convention rather than an explicit NIF
/// texture slot, so a `NiTexturingProperty` mesh with a base texture but
/// no normal/bump slot still has a normal map on disk under this name
/// (#1303 / OBL-D4-NEW-01).
///
/// The extension is preserved (`.dds` → `_n.dds`, `.DDS` → `_n.DDS`) and
/// the suffix inserted before it. Callers apply this only when the mesh
/// left `normal_map` empty; the candidate is then resolved like any other
/// texture, so a non-existent sibling fails soft (resolves to the
/// fallback handle and is skipped) — modern meshes that already carry an
/// explicit normal slot never reach this path.
pub(crate) fn derive_normal_map_path(diffuse: &str) -> String {
    match diffuse.rfind('.') {
        Some(dot) => format!("{}_n{}", &diffuse[..dot], &diffuse[dot..]),
        None => format!("{diffuse}_n.dds"),
    }
}

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
    resolve_texture_with_clamp_and_color_space(
        ctx,
        tex_provider,
        tex_path,
        clamp_mode,
        TextureColorSpace::Srgb,
    )
}

fn resolve_texture_with_clamp_and_color_space(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    tex_path: Option<&str>,
    clamp_mode: u8,
    color_space: TextureColorSpace,
) -> u32 {
    resolve_texture_view_with_clamp(ctx, tex_provider, tex_path, clamp_mode, false, color_space)
}

fn resolve_environment_texture_with_clamp(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    tex_path: Option<&str>,
    clamp_mode: u8,
) -> u32 {
    resolve_texture_view_with_clamp(
        ctx,
        tex_provider,
        tex_path,
        clamp_mode,
        true,
        TextureColorSpace::Srgb,
    )
}

fn resolve_texture_view_with_clamp(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    tex_path: Option<&str>,
    clamp_mode: u8,
    cubemap: bool,
    color_space: TextureColorSpace,
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
        return if cubemap {
            0
        } else {
            ctx.texture_registry.neutral_fallback()
        };
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
    let cached = if cubemap {
        ctx.texture_registry
            .acquire_cubemap_by_path_with_clamp(tex_path, clamp_mode)
    } else {
        ctx.texture_registry
            .acquire_by_path_with_clamp_and_color_space(tex_path, clamp_mode, color_space)
    };
    if let Some(cached) = cached {
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
        let queued = if cubemap {
            ctx.texture_registry.enqueue_cubemap_dds_with_clamp(
                &ctx.device,
                tex_path,
                dds_bytes,
                clamp_mode,
            )
        } else {
            ctx.texture_registry.enqueue_dds_with_clamp_and_color_space(
                &ctx.device,
                tex_path,
                dds_bytes,
                clamp_mode,
                color_space,
            )
        };
        match queued {
            Ok(h) => {
                log::debug!(
                    "Queued DDS {}texture: '{}' (clamp_mode {}, handle {h})",
                    if cubemap { "cube " } else { "" },
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
    if cubemap {
        0
    } else {
        ctx.texture_registry.fallback()
    }
}

/// Resolve every non-base texture role with the material's authored sampler
/// addressing mode, then re-attach the already-resolved base handle.
///
/// A NIF material owns one `TexClampMode` for its texture set. Keeping this
/// walk here prevents the cell and loose-NIF spawn paths from drifting, and
/// guarantees that normal/detail/gloss/height/mask textures use the same
/// addressing profile as their base colour. Missing secondary roles stay at
/// handle 0; an authored-but-missing secondary texture also collapses to 0 so
/// the shader treats that optional contribution as absent instead of sampling
/// the diagnostic magenta fallback.
pub(crate) fn resolve_material_texture_handles_with_clamp(
    ctx: &mut VulkanContext,
    tex_provider: &TextureProvider,
    textures: &MaterialTextureSet<Option<String>>,
    base_handle: u32,
    clamp_mode: u8,
) -> MaterialTextureSet<u32> {
    let fallback = ctx.texture_registry.fallback();
    map_secondary_texture_handles(textures, base_handle, |path, cubemap, color_space| {
        let handle = if cubemap {
            resolve_environment_texture_with_clamp(ctx, tex_provider, Some(path), clamp_mode)
        } else {
            resolve_texture_with_clamp_and_color_space(
                ctx,
                tex_provider,
                Some(path),
                clamp_mode,
                color_space,
            )
        };
        if handle == fallback {
            0
        } else {
            handle
        }
    })
}

fn map_secondary_texture_handles(
    textures: &MaterialTextureSet<Option<String>>,
    base_handle: u32,
    mut resolve: impl FnMut(&str, bool, TextureColorSpace) -> u32,
) -> MaterialTextureSet<u32> {
    let srgb = TextureColorSpace::Srgb;
    let linear = TextureColorSpace::Linear;
    let environment = textures
        .environment
        .as_deref()
        .map(|path| resolve(path, true, srgb))
        .unwrap_or(0);
    let mut slot = |path: &Option<String>, color_space| {
        path.as_deref()
            .map(|path| resolve(path, false, color_space))
            .unwrap_or(0)
    };

    MaterialTextureSet {
        base_color: base_handle,
        normal: slot(&textures.normal, linear),
        emissive: slot(&textures.emissive, srgb),
        detail: slot(&textures.detail, srgb),
        smooth_spec: slot(&textures.smooth_spec, linear),
        dark: slot(&textures.dark, srgb),
        height: slot(&textures.height, linear),
        environment,
        environment_mask: slot(&textures.environment_mask, linear),
        tint: slot(&textures.tint, srgb),
        inner_layer: slot(&textures.inner_layer, srgb),
        specular: slot(&textures.specular, linear),
        lighting_mask: slot(&textures.lighting_mask, linear),
        back_lighting: slot(&textures.back_lighting, srgb),
        lighting: slot(&textures.lighting, linear),
        flow: slot(&textures.flow, linear),
        wrinkle: slot(&textures.wrinkle, linear),
        greyscale_lut: slot(&textures.greyscale_lut, srgb),
        reflectance: slot(&textures.reflectance, linear),
        emittance_gradient: slot(&textures.emittance_gradient, srgb),
        glass_roughness_scratch: slot(&textures.glass_roughness_scratch, linear),
        glass_dirt_overlay: slot(&textures.glass_dirt_overlay, srgb),
        decals: std::array::from_fn(|i| slot(&textures.decals[i], srgb)),
    }
}

#[cfg(test)]
mod tests {
    use super::{map_secondary_texture_handles, missing_archive_errors};
    use byroredux_nif::import::MaterialTextureSet;
    use byroredux_renderer::TextureColorSpace;

    /// #1776 — the aggregate guard must fire exactly for a kind that was
    /// requested on the CLI yet opened zero archives (the wrong-CWD / mistyped
    /// trap), and never for a kind that wasn't requested (a loose-NIF run).
    #[test]
    fn missing_archive_errors_fires_only_for_requested_empty_kinds() {
        // --bsa given but nothing opened → one error.
        assert_eq!(missing_archive_errors(true, true, false, false).len(), 1);
        // both kinds requested + both empty → two errors.
        assert_eq!(missing_archive_errors(true, true, true, true).len(), 2);
        // requested AND opened (non-empty) → no error (the happy path).
        assert!(missing_archive_errors(true, false, true, false).is_empty());
        // not requested at all (loose-NIF run, no --bsa) → no error even though
        // the provider is empty — the pre-#1776 behaviour for that case.
        assert!(missing_archive_errors(false, true, false, true).is_empty());
        // mixed: meshes opened, textures requested-but-empty → one error.
        assert_eq!(missing_archive_errors(true, false, true, true).len(), 1);
    }

    #[test]
    fn common_material_texture_walk_covers_every_secondary_role_once() {
        let some = |name: &str| Some(name.to_string());
        let textures = MaterialTextureSet {
            base_color: some("base"),
            normal: some("normal"),
            emissive: some("emissive"),
            detail: some("detail"),
            smooth_spec: some("smooth_spec"),
            dark: some("dark"),
            height: some("height"),
            environment: some("environment"),
            environment_mask: some("environment_mask"),
            tint: some("tint"),
            inner_layer: some("inner_layer"),
            specular: some("specular"),
            lighting_mask: some("lighting_mask"),
            back_lighting: some("back_lighting"),
            lighting: some("lighting"),
            flow: some("flow"),
            wrinkle: some("wrinkle"),
            greyscale_lut: some("greyscale_lut"),
            reflectance: some("reflectance"),
            emittance_gradient: some("emittance_gradient"),
            glass_roughness_scratch: some("glass_roughness_scratch"),
            glass_dirt_overlay: some("glass_dirt_overlay"),
            decals: [
                some("decal_0"),
                some("decal_1"),
                some("decal_2"),
                some("decal_3"),
            ],
        };

        let mut seen = Vec::new();
        let mut next_handle = 1u32;
        let handles =
            map_secondary_texture_handles(&textures, 777, |path, cubemap, color_space| {
                seen.push((path.to_string(), cubemap, color_space));
                let handle = next_handle;
                next_handle += 1;
                handle
            });

        assert_eq!(handles.base_color, 777);
        assert!(!seen.iter().any(|(path, _, _)| path == "base"));
        assert_eq!(seen.len(), textures.secondary_values().count());
        assert_eq!(
            seen.iter()
                .filter(|(_, cubemap, _)| *cubemap)
                .map(|(path, _, _)| path.as_str())
                .collect::<Vec<_>>(),
            vec!["environment"],
        );
        for role in [
            "normal",
            "smooth_spec",
            "height",
            "environment_mask",
            "specular",
            "lighting_mask",
            "glass_roughness_scratch",
        ] {
            assert!(seen.iter().any(|(path, cubemap, color_space)| path == role
                && !cubemap
                && *color_space == TextureColorSpace::Linear));
        }
        for role in [
            "emissive",
            "detail",
            "dark",
            "back_lighting",
            "glass_dirt_overlay",
            "decal_0",
        ] {
            assert!(seen.iter().any(|(path, cubemap, color_space)| path == role
                && !cubemap
                && *color_space == TextureColorSpace::Srgb));
        }
        assert!(handles.secondary_values().all(|&handle| handle != 0));
    }
}
