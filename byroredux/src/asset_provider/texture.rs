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
        // #3637 — last-listed archive wins, matching Bethesda's own load-
        // order precedence (a later `--textures-bsa` overrides an earlier
        // one). Checking from the end and returning on first hit means the
        // last-listed archive that actually carries the path answers.
        for archive in self.texture_archives.iter().rev() {
            if let Ok(data) = archive.extract(normalized.as_ref()) {
                return Some(data);
            }
        }
        self.extract_via_facegen_tool_path_fallback(&normalized)
    }

    /// Whether a texture exists, without extracting or decompressing it.
    pub(crate) fn has_texture(&self, path: &str) -> bool {
        let normalized = normalize_texture_path(path);
        self.texture_archives
            .iter()
            .any(|archive| archive.contains(normalized.as_ref()))
            || (is_facegen_tool_path(&normalized)
                && self
                    .texture_archives
                    .iter()
                    .any(|archive| archive.find_by_basename(&normalized).is_some()))
    }

    /// #3555 — see [`is_facegen_tool_path`]'s doc. Only tried once every
    /// archive's canonical-key lookup above has already missed, so it
    /// changes nothing for a texture that already resolves normally.
    fn extract_via_facegen_tool_path_fallback(&self, normalized: &str) -> Option<Vec<u8>> {
        if !is_facegen_tool_path(normalized) {
            return None;
        }
        // #3637 — same last-listed-wins precedence as the primary lookup
        // above.
        for archive in self.texture_archives.iter().rev() {
            if let Some(key) = archive.find_by_basename(normalized) {
                if let Ok(data) = archive.extract(&key) {
                    return Some(data);
                }
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
        // #3637 — last-listed archive wins; see `extract`'s doc for why.
        for archive in self.mesh_archives.iter().rev() {
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

    /// Whether a mesh exists at *exactly* this archive key — no `meshes\`
    /// rooting.
    ///
    /// #3735 — SpeedTree `.spt` binaries live outside the `meshes\` root
    /// (`trees\` is itself a top-level archive folder in Oblivion, FO3 and
    /// FNV), so [`normalize_mesh_path`]'s rooting — correct for every other
    /// mesh consumer — turns a correct `trees\<name>.spt` key into a
    /// guaranteed miss. The `.spt` route resolves its own archive key
    /// through `references::import::resolve_spt_model_path` and then needs a
    /// lookup that takes it literally. Archive-internal case and separator
    /// folding still happens inside `BsaArchive`.
    ///
    /// Deliberately additive: `normalize_mesh_path` is shared by every mesh
    /// consumer in the engine and stays untouched, so this cannot change any
    /// other lookup.
    pub(crate) fn has_mesh_exact(&self, path: &str) -> bool {
        self.mesh_archives
            .iter()
            .any(|archive| archive.contains(path))
    }

    /// [`Self::extract_mesh`]'s exact-key counterpart. See
    /// [`Self::has_mesh_exact`] for why the `.spt` route needs it.
    pub(crate) fn extract_mesh_exact(&self, path: &str) -> Option<Vec<u8>> {
        // #3637 — last-listed archive wins; see `extract`'s doc for why.
        for archive in self.mesh_archives.iter().rev() {
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

/// M44 Phase 3.5 — try to populate `FootstepConfig.default_sound`
/// from the `--sounds-bsa` archive(s) (if any were provided). Decodes the
/// canonical FNV dirt-walk left-foot WAV — every kf-era humanoid
/// hits this on every other step. Future Phase 3.5b replaces the
/// single-sound fallback with FOOT-record-driven per-material lookup.
///
/// #3776 — takes the already-built [`SoundArchiveProvider`]
/// rather than re-parsing `args` itself. `--sounds-bsa` is documented
/// (and the provider itself already implements) as repeatable —
/// override/mod archives listed before the vanilla one, first hit wins
/// — but this function used to stop at the *first* `--sounds-bsa`
/// occurrence, so a user who followed that documented ordering got a
/// footstep sound that silently never loaded whenever the canonical path
/// lived only in a later archive. Sharing the provider fixes that by
/// construction: it already searches every opened archive in order.
///
/// Silently skips when:
///   - No `--sounds-bsa` archive opened successfully (flag absent, or
///     every occurrence failed to open — already logged by
///     [`build_sound_archive_provider`]).
///   - No [`FOOTSTEP_CANDIDATES`] key is present in any opened archive.
///   - The decode fails through `byroredux_audio::load_sound_from_bytes`.
///
/// Each failure logs at WARN; engine boot continues regardless.
pub(crate) fn try_load_default_footstep(
    world: &mut byroredux_core::ecs::World,
    sounds: &SoundArchiveProvider,
) {
    if sounds.is_empty() {
        // #3788 — a boot-time, once-only log distinguishing "no --sounds-bsa
        // was supplied at all" from the CANONICAL-not-found warn below, so a
        // silent footstep audio isn't indistinguishable from "not
        // implemented" when auditing a launch that omitted the flag.
        log::info!("M44 Phase 3.5: no --sounds-bsa archive supplied — footstep sound skipped");
        return;
    }
    // Vanilla dirt-walk footsteps ship with left/right alternation; pick
    // one entry per game as the default until FOOT records land.
    let Some((chosen, bytes)) = first_default_sound_hit(sounds, FOOTSTEP_CANDIDATES) else {
        log::warn!(
            "M44 Phase 3.5: no --sounds-bsa archive carries any default footstep candidate"
        );
        return;
    };
    let sound = match byroredux_audio::load_sound_from_bytes(bytes) {
        Ok(s) => s,
        Err(e) => {
            log::warn!("M44 Phase 3.5: decode '{chosen}': {e}");
            return;
        }
    };
    let mut config = world.resource_mut::<crate::components::FootstepConfig>();
    config.default_sound = Some(std::sync::Arc::new(sound));
    log::info!("M44 Phase 3.5: footstep sound loaded from --sounds-bsa ('{chosen}')");
}

/// One boot-time default-sound candidate: an archive key plus the vanilla
/// games whose sound archive was verified — by exact key, against the real
/// file listing — to carry it.
///
/// #3913 — the pre-fix splash list documented itself as covering "the
/// Skyrim and Fallout naming variants" while none of its three keys existed
/// in any Fallout archive (each was one path segment off: FNV nests the
/// medium splash under `splash_m\`, FO3 under `medium\`, and Skyrim's
/// footstep folders are `l\`/`r\` where Fallout's are `left\`/`right\`).
/// A source-only test cannot catch a mistyped archive key, so every row
/// here carries its game tag and
/// `default_sound_candidates_hit_their_tagged_game_archive` (`#[ignore]`,
/// needs game data) re-asserts each row against the archive on disk.
/// Verified 2026-09-06 against `Fallout - Sound.bsa` (FNV 6,465 entries /
/// FO3 2,709), `Skyrim - Sounds.bsa` (6,198) and `Oblivion - Sounds.bsa`
/// (1,533). FO4's `Fallout4 - Sounds.ba2` ships its splashes as `.xwm`,
/// which `byroredux_audio` doesn't decode, so it has no row.
pub(crate) struct DefaultSoundCandidate {
    /// Game tags (the `debug_profiles.toml` profile names) whose vanilla
    /// sound archive carries `key`.
    pub(crate) games: &'static [&'static str],
    /// Archive-relative key, exactly as [`SoundArchiveProvider::extract`]
    /// expects it.
    pub(crate) key: &'static str,
}

/// Default footstep one-shot per game — see [`DefaultSoundCandidate`].
pub(crate) const FOOTSTEP_CANDIDATES: &[DefaultSoundCandidate] = &[
    DefaultSoundCandidate {
        games: &["fnv", "fo3"],
        key: r"sound\fx\fst\dirt\walk\left\fst_dirt_walk_01.wav",
    },
    DefaultSoundCandidate {
        games: &["skyrimse"],
        key: r"sound\fx\fst\dirt\walk\l\fst_dirt_walk_01.wav",
    },
    // Oblivion calls dirt "earth" and has no walk/sneak split at this
    // level (sneak variants live one folder down, `earth\sneak\`).
    DefaultSoundCandidate {
        games: &["oblivion"],
        key: r"sound\fx\fst\earth\fst_earth_01.wav",
    },
];

/// Default medium water-splash one-shot per game — see
/// [`DefaultSoundCandidate`]. Siblings not used here: FNV's `splash_l\` /
/// `splash_h\` and `human\npc_human_splash_0{1,2,3}.wav`; FO3's `light\` /
/// `heavy\`; Skyrim's `phy_water_{l,h}_0N.wav`.
pub(crate) const WATER_SPLASH_CANDIDATES: &[DefaultSoundCandidate] = &[
    DefaultSoundCandidate {
        games: &["fnv"],
        key: r"sound\fx\phy\water\splash_m\phy_water_m_01.wav",
    },
    DefaultSoundCandidate {
        games: &["fo3"],
        key: r"sound\fx\phy\water\medium\phy_water_m_01.wav",
    },
    DefaultSoundCandidate {
        games: &["skyrimse"],
        key: r"sound\fx\phy\water\phy_water_m_01.wav",
    },
    DefaultSoundCandidate {
        games: &["oblivion"],
        key: r"sound\fx\phy\genericcollisions\water\medium\phy_water_m_01.wav",
    },
];

/// First candidate (in table order) that extracts from any opened
/// `--sounds-bsa` archive, as `"<key> (<game tags>)"` for the log line
/// plus its bytes. Only one game's archives are ever open in a session,
/// so table order only matters within a game; the tag in the label says
/// which game's naming the opened archive turned out to follow.
fn first_default_sound_hit(
    sounds: &SoundArchiveProvider,
    candidates: &[DefaultSoundCandidate],
) -> Option<(String, Vec<u8>)> {
    candidates.iter().find_map(|candidate| {
        sounds
            .extract(candidate.key)
            .map(|bytes| (format!("{} ({})", candidate.key, candidate.games.join("/")), bytes))
    })
}

/// M44 water acoustics — load a physical splash one-shot from the
/// `--sounds-bsa` archive(s) (if any were provided). The candidate keys in
/// [`WATER_SPLASH_CANDIDATES`] are verified per game (#3913); the first
/// archive/candidate hit wins, while missing audio remains a silent no-op.
///
/// #3776 — same fix and rationale as [`try_load_default_footstep`]: takes
/// the shared, already-multi-archive [`SoundArchiveProvider`]
/// instead of stopping at the first `--sounds-bsa` occurrence itself.
pub(crate) fn try_load_default_water_splash(
    world: &mut byroredux_core::ecs::World,
    sounds: &SoundArchiveProvider,
) {
    if sounds.is_empty() {
        // #3788 — same rationale as try_load_default_footstep's info log.
        log::info!("water acoustics: no --sounds-bsa archive supplied — splash sound skipped");
        return;
    }
    let Some((chosen, bytes)) = first_default_sound_hit(sounds, WATER_SPLASH_CANDIDATES) else {
        log::warn!("water acoustics: no splash candidate found in any --sounds-bsa archive");
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
    log::info!("water acoustics: loaded '{chosen}' from --sounds-bsa");
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
    // #2584 — one already-opened-paths set per pool, reused across every
    // `--bsa` / `--textures-bsa` occurrence in this run, so a sibling
    // auto-loaded from an earlier archive is recognised if the user also
    // lists it explicitly later.
    let mut mesh_opened: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut textures_opened: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--textures-bsa" => {
                if let Some(path) = args.get(i + 1) {
                    textures_requested = true;
                    open_with_numeric_siblings(
                        path,
                        "textures",
                        &mut provider.texture_archives,
                        &mut textures_opened,
                    );
                    i += 2;
                    continue;
                }
            }
            "--bsa" => {
                if let Some(path) = args.get(i + 1) {
                    mesh_requested = true;
                    open_with_numeric_siblings(
                        path,
                        "mesh",
                        &mut provider.mesh_archives,
                        &mut mesh_opened,
                    );
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

/// The `<base>_n.dds` sibling of `diffuse`, but only when it is actually
/// present in the loaded texture archives (#3551).
///
/// The derive used to fire on every game. FO4 and Skyrim author normals
/// explicitly (BGSM, `BSLightingShaderProperty`) and have no `_n.dds`
/// load-time convention at all, so an empty normal slot there is a mesh that
/// genuinely has no normal map — every fabricated path was an archive lookup
/// that could only miss, plus a phantom `src=derived-normal` row in
/// `tex.missing` that drowned the real misses (measured: 13/16 of FO4's,
/// 8/10 of Skyrim's).
///
/// This gates on presence rather than on [`GameKind`], which is both cheaper
/// and does not need a per-game answer — the convention's exact reach across
/// FNV was the open question a game gate would have had to settle. A mesh on
/// any title that really does ship the sibling still gets it.
///
/// Canonicalises with [`canonical_texture_key`] — the SAME key
/// `resolve_texture_view_with_clamp` derives before its own lookup. Probing
/// the raw path instead would reintroduce the #3334 key drift here as a
/// silently dropped normal map: an authored `Data\Textures\…` diffuse
/// normalises to `textures\data\textures\…`, which is present in no
/// archive, so a present sibling would test absent.
pub(crate) fn derive_present_normal_map_path(
    provider: &TextureProvider,
    diffuse: &str,
) -> Option<String> {
    let derived = derive_normal_map_path(diffuse);
    provider
        .has_texture(&canonical_texture_key(&derived))
        .then_some(derived)
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
    // Canonicalise the path BEFORE it is used as either a cache key or an
    // archive key, so the two agree by construction.
    //
    // 1. `strip_build_prefix` drops an embedded build-server root
    //    (`skyrimhd\build\pc\data\…`). Without it Skyrim AE's HD-bundle
    //    juniper / reach branches / driftwood / mountain clutter all render
    //    as magenta placeholders.
    // 2. `normalize_texture_path` drops a *leading* `data\` (which
    //    `strip_build_prefix` deliberately does not — it requires a
    //    separator BEFORE `data`) and prepends the `textures\` root.
    //
    // Step 2 is the #3334 half. `TextureProvider::extract` below already
    // normalises internally, so extraction always succeeded; only the key
    // was wrong-shaped. Every FNV `WATR.NNAM` authors
    // `Data\Textures\Water\…`, which the registry's own `normalize_path`
    // then turned into `textures/data/textures/water/…` — a second bindless
    // slot and a second GPU upload for a DDS the REFR walk had already
    // loaded under its canonical key, and a cache the WATR resolve could
    // never hit. Same key-drift shape as #3038 / #3412, one layer down.
    let canonical = canonical_texture_key(tex_path);
    let tex_path: &str = &canonical;
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
    use super::{
        map_secondary_texture_handles, missing_archive_errors, FOOTSTEP_CANDIDATES,
        WATER_SPLASH_CANDIDATES,
    };
    use byroredux_nif::import::MaterialTextureSet;
    use byroredux_renderer::TextureColorSpace;

    /// #3913 — every default-sound candidate must exist, by exact key, in
    /// the vanilla sound archive of each game it is tagged with, and every
    /// game with a row must get at least one hit. This is the test that
    /// would have caught the original defect: the pre-fix splash list's
    /// three keys were all Skyrim-shaped or mistyped, so the M44 water
    /// acoustics subsystem was a guaranteed silent no-op on FNV and FO3.
    /// A source-only test cannot know whether an archive key is real.
    ///
    /// Gated on game data; each game is skipped independently when its
    /// archive isn't on disk. Run with:
    /// ```sh
    /// BYROREDUX_FNV_DATA=<path> BYROREDUX_FO3_DATA=<path> \
    /// BYROREDUX_SKYRIMSE_DATA=<path> BYROREDUX_OBLIVION_DATA=<path> \
    ///     cargo test -p byroredux --bin byroredux \
    ///     default_sound_candidates_hit_their_tagged_game_archive -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "needs vanilla game sound archives on disk"]
    fn default_sound_candidates_hit_their_tagged_game_archive() {
        use super::super::audio::build_sound_archive_provider;
        use std::path::PathBuf;

        const STEAM: &str = "/mnt/data/SteamLibrary/steamapps/common";
        let games: [(&str, &str, String, &str); 4] = [
            (
                "fnv",
                "BYROREDUX_FNV_DATA",
                format!("{STEAM}/Fallout New Vegas/Data"),
                "Fallout - Sound.bsa",
            ),
            (
                "fo3",
                "BYROREDUX_FO3_DATA",
                format!("{STEAM}/Fallout 3 goty/Data"),
                "Fallout - Sound.bsa",
            ),
            (
                "skyrimse",
                "BYROREDUX_SKYRIMSE_DATA",
                format!("{STEAM}/Skyrim Special Edition/Data"),
                "Skyrim - Sounds.bsa",
            ),
            (
                "oblivion",
                "BYROREDUX_OBLIVION_DATA",
                format!("{STEAM}/Oblivion/Data"),
                "Oblivion - Sounds.bsa",
            ),
        ];
        let tables = [
            ("FOOTSTEP_CANDIDATES", FOOTSTEP_CANDIDATES),
            ("WATER_SPLASH_CANDIDATES", WATER_SPLASH_CANDIDATES),
        ];

        let mut checked = 0usize;
        for (game, env_var, default_dir, archive_name) in games {
            let dir = std::env::var(env_var)
                .map(PathBuf::from)
                .unwrap_or_else(|_| PathBuf::from(default_dir));
            let archive = dir.join(archive_name);
            if !archive.is_file() {
                eprintln!("skipping {game}: {} not found", archive.display());
                continue;
            }
            let provider = build_sound_archive_provider(&[
                "--sounds-bsa".to_string(),
                archive.to_string_lossy().into_owned(),
            ]);
            assert!(!provider.is_empty(), "{game}: failed to open {}", archive.display());
            for (table_name, table) in tables {
                let tagged: Vec<_> = table.iter().filter(|c| c.games.contains(&game)).collect();
                assert!(!tagged.is_empty(), "{table_name} has no candidate tagged for {game}");
                for candidate in tagged {
                    assert!(
                        provider.extract(candidate.key).is_some(),
                        "{table_name}: key '{}' is tagged {game} but is not in {}",
                        candidate.key,
                        archive.display()
                    );
                    checked += 1;
                }
            }
        }
        eprintln!("verified {checked} default-sound candidate keys against real archives");
    }

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
