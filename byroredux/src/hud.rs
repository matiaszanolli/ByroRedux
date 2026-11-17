//! MenuXml HUD — the M48.4/M48.5 legacy-UI track's engine side.
//!
//! [`byroredux_menuxml`] parses and renders Bethesda's XML menus; this
//! module is the `byroredux` half, parameterized by [`HudGameProfile`]:
//!
//! * **Oblivion** — the XML authors the bar/compass art; the driver only
//!   pushes trait overrides (`hudmain_*` tile vocabulary).
//! * **FO3/FNV** — `hud_main_menu.xml` ships empty container rects; the
//!   source engine assembled meters and the compass from
//!   `menus\prefabs\meter.xml` + `hudtemplates.xml` prototypes at
//!   runtime, so the launch path mirrors that assembly (graft meters
//!   under `HitPoints`/`ActionPoints`, instantiate the compass
//!   template) before driving `_Value` / `cropx`.
//!
//! Skyrim is the Scaleform route: `crate::scaleform_hud` launches
//! `hudmenu.swf` through the Ruffle player and owns that driver. The two
//! routes share the [`HudControl`] command resource and are mutually
//! exclusive per run: a `--menu` player owns the overlay first (both
//! routes refuse through [`menu_owned_overlay_skip`], #4723), a won
//! Scaleform route suppresses this module's MenuXml launch, and the
//! frame tick prefers `hud` over `ui_manager` when both exist — which
//! the guards make unreachable.
//!
//! Shared machinery — archive resolution, the persistent overlay
//! texture, change-signature + cadence throttling — is game-agnostic.
//!
//! Console control (`hud.on` / `hud.off` / `hud.values` / `hud.heading`)
//! flows through the [`HudControl`] World resource — commands can only
//! reach resources, and the renderer itself is owned by the frame loop
//! (same split as `LightTuning`).

use byroredux_core::ecs::components::ActorValues;
use byroredux_core::ecs::World;
use byroredux_menuxml::menu::MenuAssets;
use byroredux_menuxml::profile::{FontArchive, MenuProfile};
use byroredux_menuxml::tex::Rgba8;
use byroredux_menuxml::{MenuRenderer, ScreenTraits};

use crate::asset_provider::{extract_first, open_with_numeric_siblings, Archive};
use crate::inventory::PlayerVitals;
use crate::systems::PlayerEntity;

/// One HUD bar's source: a label plus the AVIF editor id its fraction
/// derives from.
///
/// Bar keys are canonical AVIF **editor ids** (`"Health"`,
/// `"ActionPoints"`, …) resolved per load through `PlayerVitals` —
/// #4675: the old literal FormIDs here (0x2C9 / 0x2D0) were
/// `crates/core/src/character/fallout.rs`'s unit-test fixture ids, not
/// real AVIFs (FO4's 0x2C9 is *Experience*), so no real-content entity
/// ever carried them and the bars always drew full.
#[derive(Clone, Copy)]
pub(crate) struct HudBar {
    /// Console/debug label (`hud.status`, `hud.values` usage).
    label: &'static str,
    /// Canonical AVIF editor id the fraction derives from, resolved
    /// through `PlayerVitals::resolved` at frame time. `None` — pin or
    /// full bar; no actor-value source wired yet.
    av: Option<&'static str>,
}

/// How a game's HUD content is assembled from its corpus.
#[derive(Clone, Copy)]
pub(crate) enum HudStyle {
    /// The XML authors the art (Oblivion); the driver only pushes trait
    /// values. `bars`/`compass`/`mode` are `(tile, trait)` override
    /// pairs, per the vanilla XML comments' HUD contract.
    Authored {
        bars: [(&'static str, &'static str); 3],
        compass: (&'static str, &'static str),
        mode: (&'static str, &'static str),
    },
    /// The XML ships container rects (FO3/FNV); the driver grafts the
    /// ops-driven `meter.xml` prefab under each container and
    /// instantiates the compass template at launch, then drives
    /// `_Value` per bar and scrolls the strip via `cropx`.
    Assembled {
        /// Container rect names: bar 0 (HP), bar 1 (AP).
        containers: [&'static str; 2],
        meter_prefab: &'static str,
        compass_template: &'static str,
        /// Resolved archive path of the compass strip texture (its
        /// natural width maps the 360° heading to texels).
        compass_strip: &'static str,
    },
}

/// Everything game-specific about driving a MenuXml HUD.
#[derive(Clone, Copy)]
pub(crate) struct HudGameProfile {
    pub(crate) label: &'static str,
    /// The BSA carrying the menu XML corpus (+ Oblivion's fonts).
    misc_bsa: &'static str,
    /// The texture archive menu art defaults to (overridable). Opened
    /// through `open_with_numeric_siblings`, so FNV's `Textures2` art rides
    /// in on the FO3-shaped `Fallout - Textures.bsa` anchor — the same
    /// transparent split the texture provider gives `--textures-bsa`.
    default_textures_bsa: &'static str,
    menu_path: &'static str,
    pub(crate) menu: MenuProfile,
    style: HudStyle,
    bars: &'static [HudBar],
    sneak: HudSneak,
}

/// M65 (SNK-HUD-003) — the vanilla sneak element a MenuXml HUD drives.
#[derive(Clone, Copy)]
pub(crate) enum HudSneak {
    /// FO3 / FNV: `Fallout3.exe` binds the `sneak_nif` text under the
    /// `SneakMeter` rect (`0x92b64e`) and drives its `string` (the label),
    /// `systemcolor` (1 for hidden / detected, 2 for caution / danger),
    /// `alpha` (a 0.5 s animation for danger, faded to 0 outside the stance)
    /// and `user0` (1 for danger) from `0x937a70`. The XML places neither
    /// tile and leaves the text's font unauthored, so the launch sets them.
    Text { meter: &'static str, text: &'static str },
    /// Oblivion: `hud_reticle.xml`'s `hudreticle_sneak` eye, whose alpha
    /// copies `user8` ("code set - alpha"). The reticle menu is grafted under
    /// the HUD at launch, its crosshair hidden (the native HUD draws one).
    /// `crosshair` names the image itself, not its container `<rect>`:
    /// `visible` is per-tile in this dialect, so hiding the container hides
    /// nothing (#7001).
    Eye { menu: &'static str, root: &'static str, crosshair: &'static str, eye: &'static str },
}

static OBLIVION_BARS: &[HudBar] = &[
    HudBar {
        label: "health",
        av: Some("Health"),
    },
    HudBar {
        label: "magicka",
        av: Some("Magicka"),
    },
    HudBar {
        label: "fatigue",
        av: Some("Fatigue"),
    },
];

static FALLOUT_BARS: &[HudBar] = &[
    HudBar {
        label: "hp",
        av: Some("Health"),
    },
    HudBar {
        label: "ap",
        av: Some("ActionPoints"),
    },
    // No third bar: FO3's XP meter is a level-up popup (authored
    // `visible &false;`), not a persistent bar.
];

impl HudGameProfile {
    pub(crate) fn oblivion() -> Self {
        Self {
            label: "Oblivion",
            misc_bsa: "Oblivion - Misc.bsa",
            default_textures_bsa: "Oblivion - Textures - Compressed.bsa",
            menu_path: "menus\\main\\hud_main_menu.xml",
            menu: MenuProfile::oblivion(),
            style: HudStyle::Authored {
                bars: [
                    ("hudmain_health_full", "user0"),
                    ("hudmain_magic_full", "user0"),
                    ("hudmain_fatigue_full", "user0"),
                ],
                compass: ("hudmain_compass_window", "user0"),
                mode: ("HUDMainMenu", "user3"),
            },
            bars: OBLIVION_BARS,
            sneak: HudSneak::Eye {
                menu: "menus\\main\\hud_reticle.xml",
                root: "HUDReticle",
                crosshair: "hudreticle_reticle_crosshair",
                eye: "hudreticle_sneak",
            },
        }
    }

    pub(crate) fn fallout3() -> Self {
        Self::fallout("Fallout 3", MenuProfile::fallout3())
    }

    pub(crate) fn fallout_nv() -> Self {
        Self::fallout("Fallout: New Vegas", MenuProfile::fallout_nv())
    }

    fn fallout(label: &'static str, menu: MenuProfile) -> Self {
        Self {
            label,
            misc_bsa: "Fallout - Misc.bsa",
            // FNV keeps its interface art and every font in `Textures2`
            // (FO3 has no such archive). Both anchor on `Textures.bsa`: the
            // numeric-sibling rule adds `Textures2` when it exists, so the
            // choice needs no per-game archive name and a wrong game guess
            // degrades to a missing font table, not a transparent HUD (#4718).
            default_textures_bsa: "Fallout - Textures.bsa",
            menu_path: "menus\\main\\hud_main_menu.xml",
            menu,
            style: HudStyle::Assembled {
                containers: ["HitPoints", "ActionPoints"],
                meter_prefab: "menus\\prefabs\\meter.xml",
                compass_template: "template_compass_window",
                compass_strip: "textures\\interface\\hud\\glow_hud_comp_direction_strip.dds",
            },
            bars: FALLOUT_BARS,
            sneak: HudSneak::Text { meter: "SneakMeter", text: "sneak_nif" },
        }
    }

    /// The driven-bar count (2 Fallout, 3 Oblivion).
    fn bar_count(&self) -> usize {
        self.bars.len()
    }
}

/// The archives a HUD render needs, plus the profile that says which one
/// carries fonts (Oblivion: Misc; FO3/FNV: the texture pool).
pub(crate) struct HudAssets {
    misc: Archive,
    /// The named texture archive followed by its numeric siblings, in open
    /// order. Last-listed wins, as in `TextureProvider::extract`.
    textures: Vec<Archive>,
    pub(crate) profile: HudGameProfile,
}

impl HudAssets {
    /// The menu and texture archives of `profile` in the game data directory `data`, for a test
    /// that loads a corpus document (the HUD, a menu); `None` when the archives are missing.
    #[cfg(test)]
    pub(crate) fn open_in(profile: HudGameProfile, data: &std::path::Path) -> Option<Self> {
        let misc = Archive::open(&data.join(profile.misc_bsa).to_string_lossy()).ok()?;
        let mut textures = Vec::new();
        let textures_path = data.join(profile.default_textures_bsa).to_string_lossy().into_owned();
        open_with_numeric_siblings(&textures_path, "HUD textures", &mut textures, &mut std::collections::HashSet::new());
        Some(Self { misc, textures, profile })
    }

    fn extract_texture(&self, path: &str) -> Option<Vec<u8>> {
        extract_first(self.textures.iter().rev(), path)
    }
}

impl MenuAssets for HudAssets {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        self.misc.extract_or_warn(path)
    }
    fn texture(&self, path: &str) -> Option<Vec<u8>> {
        self.extract_texture(path)
    }
    fn font(&self, index: u8) -> Option<Vec<u8>> {
        let path = self.profile.menu.font_paths.get(index as usize - 1)?;
        match self.profile.menu.font_archive {
            FontArchive::Misc => self.misc.extract_or_warn(path),
            FontArchive::Textures => self.extract_texture(path),
        }
    }
    fn font_texture(&self, path: &str) -> Option<Vec<u8>> {
        match self.profile.menu.font_archive {
            FontArchive::Misc => self.misc.extract_or_warn(path),
            FontArchive::Textures => self
                .extract_texture(path)
                .or_else(|| self.misc.extract_or_warn(path)),
        }
    }
}

/// Which HUD backend a launch installed — reported by `hud.status` and
/// used by the console to shape diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HudBackend {
    /// MenuXml CPU raster (`hud.rs`): Oblivion / FO3 / FNV.
    MenuXml,
    /// Scaleform SWF through the Ruffle player (`scaleform_hud.rs`).
    Scaleform,
}

impl HudBackend {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::MenuXml => "menuxml",
            Self::Scaleform => "scaleform",
        }
    }
}

/// Console-facing HUD control. Inserted at launch with the game
/// profile's bar labels; `hud.*` commands and the frame-loop driver both
/// go through it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudControl {
    /// Which backend owns the overlay (set at launch, immutable after).
    pub backend: HudBackend,
    pub visible: bool,
    /// Pinned bar fractions (0..1), indexed like the profile's bar
    /// table. `None` = derive from actor values / default full — set by
    /// `hud.values`, cleared by `hud.values auto`.
    pub bars: [Option<f32>; 3],
    /// How many bars this game's HUD drives (3 Oblivion, 2 FO3/FNV)
    /// and their labels — sizes `hud.values` and labels `hud.status`.
    pub bar_count: u8,
    pub bar_labels: [&'static str; 3],
    /// Pinned compass heading in degrees. `None` = follow the camera.
    pub heading: Option<f32>,
    /// #4675 — the last auto-derived fraction per bar, written by the
    /// driver every frame (before the change-signature skip) so
    /// `hud.status` reports the LIVE value instead of a constant
    /// "1.00 (auto)" no gate could read.
    pub live: [Option<f32>; 3],
}

impl byroredux_core::ecs::Resource for HudControl {}

impl Default for HudControl {
    fn default() -> Self {
        Self {
            backend: HudBackend::MenuXml,
            visible: true,
            bars: [None; 3],
            bar_count: 3,
            bar_labels: ["health", "magicka", "fatigue"],
            heading: None,
            live: [None; 3],
        }
    }
}

/// The live HUD owned by the frame loop (see module docs for why it is
/// not a World resource: the driver runs beside the render tick, and
/// `HudControl` is the command surface).
pub(crate) struct MenuXmlHud {
    renderer: MenuRenderer,
    /// #4608 — persistent staging for the render→upload handoff. The raster
    /// framebuffer borrows `self.renderer` immutably while `upload_frame`
    /// needs `&mut self`, so the pixels cross through this
    /// owned buffer — retained across refreshes instead of a fresh
    /// swapchain-sized Vec per changed tick.
    upload_buffer: Vec<u8>,
    assets: HudAssets,
    profile: HudGameProfile,
    /// Compass strip texels per degree of heading (0 = strip unknown —
    /// the compass stays at cropx 0 rather than guessing).
    px_per_degree: f32,
    /// The one overlay texture every upload overwrites in place. A fixed
    /// handle avoids image and descriptor churn. The registry records the
    /// copy into the frame command buffer, and its barriers order prior
    /// frames' sampling before it (#3429), so no rotation is needed.
    /// #4892 — this was a three-texture rotation left over from the
    /// retired one-shot-submit contract: two extra swapchain-extent images
    /// (16.6 MB at 1080p, 66 MB at 4K) plus a retained CPU pixel copy each.
    texture_handle: u32,
    width: u32,
    height: u32,
    /// Signature of the last rendered frame's driving inputs. The HUD is
    /// CPU-rasterized and uploaded as a full-frame RGBA texture, so a
    /// static HUD (pinned bars, unchanged heading) must not pay that
    /// every frame — same contract as the Scaleform path's
    /// `UiFrame::Unchanged`.
    last_signature: u64,
    /// When the last upload happened — the rate-limit anchor. A rotating
    /// camera changes the compass heading every frame, and without a
    /// cadence cap the debug-profile CPU raster (~16 ms) plus the
    /// 1280×720×4 staging upload would run at frame rate, which pinned
    /// a machine during the first live run of this feature. 30 Hz keeps
    /// the compass visually continuous at a bounded ~3.5 MB/33 ms
    /// transfer budget.
    last_upload: std::time::Instant,
    /// The clock the danger state's flash runs on.
    started: std::time::Instant,
}

/// Maximum HUD refresh cadence.
pub(crate) const HUD_REFRESH_INTERVAL: std::time::Duration =
    std::time::Duration::from_millis(33);

/// The base ESM a New Vegas load order is rooted at.
const FNV_BASE_ESM: &str = "FalloutNV.esm";
/// The base ESM a Fallout 3 load order is rooted at.
const FO3_BASE_ESM: &str = "Fallout3.esm";

/// Pick FO3 or FNV for a launch whose corpus is `Fallout - Misc.bsa`.
///
/// Both games ship that archive, so the corpus cannot say which it is, and
/// the font table differs (FNV adds a ninth slot). The load order can: every
/// DLC and mod plugin masters to its game's base ESM, so
/// `--master FalloutNV.esm --esm HonestHearts.esm` is New Vegas although
/// `--esm` names neither game (#4718 — this used to pattern-match the
/// `--esm` file name and gave every DLC or mod launch the FO3 profile).
///
/// Precedence: a base ESM named in the load order; else the one sitting
/// beside the corpus (a plugin launched without listing its master); else
/// Fallout 3. `load_order` holds plugin file names, compared
/// case-insensitively like Bethesda's own filesystem lookups.
fn fallout_profile(load_order: &[&str], data_dir: &std::path::Path) -> HudGameProfile {
    let lists = |base: &str| {
        load_order
            .iter()
            .any(|plugin| plugin.eq_ignore_ascii_case(base))
    };
    let new_vegas = if lists(FNV_BASE_ESM) {
        true
    } else if lists(FO3_BASE_ESM) {
        false
    } else {
        data_dir.join(FNV_BASE_ESM).is_file()
    };
    if new_vegas {
        HudGameProfile::fallout_nv()
    } else {
        HudGameProfile::fallout3()
    }
}

/// Resolve `--hud` into archives + a game profile.
///
/// The game is discovered from the corpus itself: whichever vanilla
/// Misc BSA sits beside `--esm`, with FO3 vs FNV decided by the load order
/// (`--master`s then `--esm`, see [`fallout_profile`]). Requires the Misc
/// BSA (menus + Oblivion fonts); the texture BSA defaults per profile and
/// can be overridden with `--hud-textures <path>`. Either way the texture
/// archive is opened with its numeric siblings, like `--textures-bsa`.
/// The HUD's archives: none without `--hud` (`launch_hud` applies the same gate before it asks
/// [`session_menu_assets`]). Kept for the tests that pin the discovery under the HUD flag.
#[cfg(test)]
fn hud_archive_args(args: &[String]) -> Result<Option<(String, String, HudGameProfile)>, String> {
    if !args.iter().any(|arg| arg == "--hud") {
        return Ok(None);
    }
    menu_archive_args(args)
}

/// The session's MenuXml archives and profile, discovered beside the ESM whether or not `--hud`
/// asked for the HUD (the menus that run on their documents need them too, GUI-PAGE-004).
fn menu_archive_args(args: &[String]) -> Result<Option<(String, String, HudGameProfile)>, String> {
    let (esm, esm_dir) = crate::cli_args::esm_discovery_root(args)?;

    let masters = crate::cli_args::parse_all_string_args(args, "--master");
    let load_order: Vec<&str> = masters
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(esm.as_str()))
        .filter_map(|plugin| std::path::Path::new(plugin).file_name()?.to_str())
        .collect();
    let candidates = [
        HudGameProfile::oblivion(),
        fallout_profile(&load_order, &esm_dir),
    ];
    let profile = candidates
        .iter()
        .find(|p| esm_dir.join(p.misc_bsa).is_file());
    let Some(profile) = profile else {
        // Skyrim and FO4 carry their HUDs as Scaleform SWFs out of their
        // interface archives — a different route (`crate::scaleform_hud`),
        // which logs its own diagnostics. Stay quiet here so a `--hud`
        // launch on those games reports one failure, not two.
        if esm_dir.join("Skyrim - Interface.bsa").is_file()
            || esm_dir.join("Fallout4 - Interface.ba2").is_file()
        {
            log::debug!("--hud: Scaleform-era interface archive present — Scaleform route owns the overlay");
            return Ok(None);
        }
        return Err(format!(
            "--hud: no vanilla menu corpus beside '{esm}' (looked for {} \
             and the Skyrim/FO4 interface archives)",
            candidates
                .iter()
                .map(|p| p.misc_bsa)
                .collect::<Vec<_>>()
                .join(" / ")
        ));
    };

    let textures = if let Some(t_idx) = args.iter().position(|a| a == "--hud-textures") {
        let path = args
            .get(t_idx + 1)
            .filter(|v| !v.starts_with("--"))
            .ok_or_else(|| "--hud-textures requires a BSA path".to_string())?;
        std::path::PathBuf::from(path)
    } else {
        let candidate = esm_dir.join(profile.default_textures_bsa);
        if !candidate.is_file() {
            return Err(format!(
                "--hud: no texture archive — pass --hud-textures <path> (no '{}' beside the ESM)",
                candidate.display()
            ));
        }
        candidate
    };
    Ok(Some((
        esm_dir.join(profile.misc_bsa).to_string_lossy().into_owned(),
        textures.to_string_lossy().into_owned(),
        *profile,
    )))
}

/// The one `--menu`-owns-the-overlay refusal both `--hud` routes apply
/// (#4723 — this used to guard the Scaleform route only; the MenuXml
/// launch had no `ui_manager` parameter at all, so `--menu … --hud` on an
/// Oblivion/FO3/FNV game constructed both drivers: the frame tick
/// preferred `hud`, and the `--menu` player froze while still holding
/// input focus — movement, mouse look and Escape→pause all dead, with no
/// error logged). One declaration, so the two routes cannot drift apart
/// again; each logs which route it skipped.
pub(crate) fn menu_owned_overlay_skip(menu_player_installed: bool) -> Option<&'static str> {
    menu_player_installed.then_some("hud: --menu owns the overlay — HUD route skipped")
}

/// Launch the HUD when `--hud` is present. Mirrors `launch_archive_menu`:
/// opens the archives, assembles the per-game HUD content, registers the
/// transparent overlay texture (#4892 collapsed the triple buffer to one),
/// and logs the `hud: loaded` line a smoke gate can grep.
/// The session's MenuXml game (Oblivion, Fallout 3, New Vegas): its menu and texture archives and
/// profile, discovered beside the ESM as the HUD's are. `Ok(None)` when the session is no MenuXml
/// game. Shared by the HUD and the menus that run on their documents (M48 GUI-PAGE-004).
/// The archive paths come back with them, for the load log.
pub(crate) fn session_menu_assets(args: &[String]) -> Result<Option<(HudAssets, String, String)>, String> {
    let Some((misc_path, textures_path, profile)) = menu_archive_args(args)? else {
        return Ok(None);
    };
    let misc = Archive::open(&misc_path).map_err(|e| format!("hud: archive open failed: {e}"))?;
    // `open_with_numeric_siblings` reports a failed open as a warning and
    // pushes nothing, so an empty pool is the failure signal.
    let mut textures = Vec::new();
    open_with_numeric_siblings(&textures_path, "HUD textures", &mut textures, &mut std::collections::HashSet::new());
    if textures.is_empty() {
        return Err(format!("hud: texture archive open failed: '{textures_path}'"));
    }
    Ok(Some((HudAssets { misc, textures, profile }, misc_path, textures_path)))
}

pub(crate) fn launch_hud(
    ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
    world: &mut World,
    ui_manager: &Option<byroredux_ui::UiManager>,
    args: &[String],
) -> Option<MenuXmlHud> {
    if let Some(reason) = menu_owned_overlay_skip(ui_manager.is_some()) {
        log::info!("{reason} (MenuXml)");
        return None;
    }
    if !args.iter().any(|arg| arg == "--hud") {
        return None;
    }
    let (assets, misc_path, textures_path) = match session_menu_assets(args) {
        Ok(Some(found)) => found,
        Ok(None) => return None,
        Err(error) => {
            log::error!("{error}");
            return None;
        }
    };
    let profile = assets.profile;

    let (w, h) = ctx.swapchain_extent();
    let screen = ScreenTraits::new(w as f32, h as f32);
    match MenuRenderer::load_with_profile(&assets, profile.menu_path, screen, profile.menu) {
        Ok(mut renderer) => {
            // The Fallout assembly the source engine performed in C++:
            // meters grafted under the named containers, compass
            // instantiated from the hudtemplates prototype. Engine-side
            // placement anchors the meters to the bottom screen corners
            // (the authored container rects carry no x/y).
            let mut px_per_degree = 0.0f32;
            if let HudStyle::Assembled {
                containers,
                meter_prefab,
                compass_template,
                compass_strip,
            } = profile.style
            {
                for (i, name) in ASSEMBLED_METERS.iter().enumerate() {
                    match renderer.graft_fragment(&assets, meter_prefab, containers[i], name) {
                        Ok(_) => {
                            renderer.set_override(name, "width", 300.0);
                            renderer.set_override(name, "visible", 2.0);
                            renderer.set_override(name, "alpha", 255.0);
                        }
                        Err(error) => {
                            log::error!("hud: {} meter graft failed: {error}", profile.label);
                            return None;
                        }
                    }
                }
                match renderer.instantiate_template(compass_template, "HUDMainMenu", "hud_compass")
                {
                    Ok(_) => {
                        // Tile mode makes the strip wrap seamlessly as
                        // cropx scrolls past the texture edge.
                        renderer.set_override("hud_compass", "tile", 2.0);
                        renderer.set_override("hud_compass", "visible", 2.0);
                        renderer.set_override("hud_compass", "alpha", 255.0);
                    }
                    Err(error) => {
                        log::error!("hud: compass instantiation failed: {error}");
                        return None;
                    }
                }
                px_per_degree = assets
                    .texture(compass_strip)
                    .and_then(|b| Rgba8::decode_dds(&b))
                    .map(|strip| strip.width as f32 / 360.0)
                    .unwrap_or(0.0);
                if px_per_degree == 0.0 {
                    log::warn!("hud: compass strip '{compass_strip}' unavailable — compass fixed at north");
                }
            }

            install_sneak_element(&mut renderer, &assets, profile.sneak);
            place_engine_anchored(&mut renderer, &profile, w as f32, h as f32);

            // Same transparent initial upload the `--menu` route uses, so
            // the composite quad exists before the first rasterized frame.
            let registered = {
                let upload_ctx = byroredux_renderer::upload_ctx!(ctx);
                ctx.texture_registry
                    .register_rgba(upload_ctx, w, h, &vec![0u8; (w * h * 4) as usize])
            };
            match registered {
                Ok(texture_handle) => {
                    log::info!(
                        "hud: loaded {} misc='{}' textures='{}' texture={texture_handle} \
                         ({w}x{h}, {} NIF tiles skipped) profile='{}'",
                        profile.menu_path,
                        misc_path,
                        textures_path,
                        renderer.nif_tiles,
                        profile.label
                    );
                    let mut control = HudControl {
                        bar_count: profile.bar_count() as u8,
                        ..HudControl::default()
                    };
                    control.bar_labels = [
                        profile.bars[0].label,
                        profile.bars[1].label,
                        profile.bars.get(2).map(|b| b.label).unwrap_or("xp"),
                    ];
                    world.insert_resource(control);
                    Some(MenuXmlHud {
                        upload_buffer: Vec::new(),
                        renderer,
                        assets,
                        profile,
                        px_per_degree,
                        texture_handle,
                        width: w,
                        height: h,
                        last_signature: 0,
                        last_upload: std::time::Instant::now(),
                        started: std::time::Instant::now(),
                    })
                }
                Err(_) => {
                    log::error!("hud: UI texture registration failed");
                    None
                }
            }
        }
        Err(error) => {
            log::error!("hud: HUD load failed: {error}");
            None
        }
    }
}

impl MenuXmlHud {
    /// Compute this frame's trait values and rasterize.
    ///
    /// `camera_forward` drives the compass heading (the pre-Skyrim
    /// games share Oblivion's convention: north is Gamebryo +Y, which
    /// the Z-up→Y-up import maps to engine −Z; east stays +X — so
    /// heading = `atan2(f.x, -f.z)` degrees, 0 = north). `None` means
    /// "unchanged since the last uploaded frame" — keep compositing the
    /// existing texture (mirrors `UiFrame::Unchanged`).
    pub fn render(
        &mut self,
        world: &World,
        camera_forward: [f32; 3],
        control: &HudControl,
    ) -> Option<&[u8]> {
        let fractions = bar_fractions(world, control, &self.profile);
        let heading = control.heading.unwrap_or_else(|| {
            f32::atan2(camera_forward[0], -camera_forward[2])
                .to_degrees()
                .rem_euclid(360.0)
        });
        // Heading quantized to 0.1° — sub-pixel for every game's compass
        // face (Oblivion's is 2048 px/360° ≈ 0.18°/px).
        let sneak = crate::detection::indicator_view(world);
        let sneak_alpha = sneak_alpha(sneak.as_ref(), self.started.elapsed().as_secs_f32());
        let hash = hash_signature((
            fractions[0].to_bits(),
            fractions[1].to_bits(),
            fractions[2].to_bits(),
            (heading * 10.0) as i32,
            u8::from(control.visible),
        ));
        let hash = mix_signature(
            hash,
            sneak.as_ref().map_or(u64::MAX, |view| view.state as u64) ^ (u64::from(sneak_alpha) << 8),
        );
        if hash == self.last_signature {
            return None;
        }
        // Rate limit: only a *changed* HUD pays the raster+upload, but a
        // continuously rotating camera changes it every frame — cap the
        // cadence independently of change detection.
        if self.last_upload.elapsed() < HUD_REFRESH_INTERVAL {
            return None;
        }
        self.last_signature = hash;
        self.last_upload = std::time::Instant::now();
        log::debug!(
            "hud: render bars={} heading={heading:.1} visible={}",
            fractions
                .iter()
                .take(self.profile.bar_count())
                .map(|f| format!("{f:.2}"))
                .collect::<Vec<_>>()
                .join("/"),
            u8::from(control.visible),
        );

        match self.profile.style {
            HudStyle::Authored {
                bars,
                compass,
                mode,
            } => {
                self.renderer.set_override(mode.0, mode.1, 1.0);
                for ((tile, trait_name), fraction) in bars.iter().zip(fractions) {
                    self.renderer.set_override(tile, trait_name, fraction);
                }
                self.renderer
                    .set_override(compass.0, compass.1, heading);
            }
            HudStyle::Assembled { .. } => {
                self.renderer.set_override("hp_meter", "_Value", fractions[0]);
                self.renderer.set_override("ap_meter", "_Value", fractions[1]);
                if self.px_per_degree > 0.0 {
                    self.renderer
                        .set_override("hud_compass", "cropx", heading * self.px_per_degree);
                }
            }
        }
        drive_sneak_element(&mut self.renderer, self.profile.sneak, sneak.as_ref(), sneak_alpha);
        let pixels = self.renderer.render_frame(&self.assets);
        // Debug: BYRO_HUD_DUMP=1 writes each rendered frame's raw RGBA so
        // engine-side output can be diffed against the crate renderer.
        if std::env::var("BYRO_HUD_DUMP").as_deref() == Ok("1") {
            let _ = std::fs::write("/tmp/hud_engine_frame.rgba", pixels);
        }
        Some(pixels)
    }

    pub fn frame_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The HUD's MenuXml document (`menus\main\hud_main_menu.xml`) — its surface in the menu
    /// registry (M48 GUI-REG-001).
    pub(crate) fn document_path(&self) -> &'static str {
        self.profile.menu_path
    }

    /// Re-render at a new screen extent (M48 GUI-REG-008, ui.md § *Resize*): the document's
    /// screen traits and the engine-anchored placements follow, the overlay texture is replaced at
    /// the new size (same handle, old image deferred), and the next tick rasterizes afresh.
    /// `false` when the extent did not change.
    pub(crate) fn resize(
        &mut self,
        ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
        width: u32,
        height: u32,
    ) -> bool {
        if (width, height) == (self.width, self.height) || width == 0 || height == 0 {
            return false;
        }
        self.renderer.set_screen(ScreenTraits::new(width as f32, height as f32));
        place_engine_anchored(&mut self.renderer, &self.profile, width as f32, height as f32);
        let upload_ctx = byroredux_renderer::upload_ctx!(ctx);
        if let Err(error) = ctx.texture_registry.update_rgba(
            upload_ctx,
            self.texture_handle,
            width,
            height,
            &vec![0u8; (width * height * 4) as usize],
        ) {
            log::error!("hud: texture resize failed: {error:#}");
        }
        self.width = width;
        self.height = height;
        self.last_signature = 0;
        true
    }

    /// The handle the frame being recorded should composite.
    pub fn current_texture(&self) -> u32 {
        self.texture_handle
    }

    /// Queue `pixels` as the overlay texture's replacement. Returns the
    /// handle to composite this frame.
    ///
    /// The registry records the copy into this frame's command buffer ahead
    /// of every consumer, behind a barrier that waits for earlier frames'
    /// sampling (see the field docs), so overwriting in place is safe.
    pub fn upload_frame(
        &mut self,
        ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
        pixels: &[u8],
    ) -> u32 {
        let upload_ctx = byroredux_renderer::upload_ctx!(ctx);
        let (w, h) = self.frame_size();
        if let Err(error) = ctx.texture_registry.write_rgba_inplace(
            upload_ctx,
            self.texture_handle,
            w,
            h,
            pixels,
        ) {
            log::error!("hud: texture upload failed: {error:#}");
        }
        self.texture_handle
    }

    /// #4608 — render + upload in one `&mut self` call so the caller never
    /// copies the frame out to release a borrow: on change, the raster
    /// crosses through the persistent [`Self::upload_buffer`] (one copy
    /// into retained memory, not a fresh allocation), then it is queued for
    /// upload. `None` = unchanged (or rate-limited) — keep compositing
    /// the current texture.
    pub fn render_and_upload(
        &mut self,
        ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
        world: &World,
        camera_forward: [f32; 3],
        control: &HudControl,
    ) -> Option<u32> {
        self.render(world, camera_forward, control)?;
        // Split borrow: fill the persistent buffer while only `renderer`
        // is borrowed, then hand upload_frame the filled buffer — the
        // method takes &mut self, so the pixel source must be self-owned by
        // then.
        {
            let pixels = self.renderer.frame_pixels();
            let upload = &mut self.upload_buffer;
            upload.clear();
            upload.extend_from_slice(pixels);
        }
        let upload = std::mem::take(&mut self.upload_buffer);
        let handle = self.upload_frame(ctx, &upload);
        self.upload_buffer = upload;
        Some(handle)
    }
}

/// M65 (SNK-HUD-003) — place the vanilla sneak element at launch. A failure
/// leaves the HUD without it rather than refusing the HUD.
fn install_sneak_element(renderer: &mut MenuRenderer, assets: &HudAssets, sneak: HudSneak) {
    match sneak {
        HudSneak::Text { meter: _, text } => {
            // Top centre (the text is `<justify> &center;`, placed by
            // `place_engine_anchored`), in the HUD's label font, slot 7.
            renderer.set_override(text, "font", 7.0);
            renderer.set_override(text, "alpha", 0.0);
        }
        HudSneak::Eye { menu, root, crosshair, eye } => {
            if let Err(error) = renderer.graft_fragment(assets, menu, "HUDMainMenu", "hud_reticle") {
                log::warn!("hud: {menu} graft failed, no sneak eye: {error}");
                return;
            }
            renderer.set_override(root, "visible", 2.0);
            renderer.set_override(root, "alpha", 255.0);
            renderer.set_override(crosshair, "visible", 0.0);
            renderer.set_override(eye, "user8", 0.0);
        }
    }
}

/// The two Fallout meters `--hud` grafts under the named containers (the authored container rects
/// carry no x/y, so the engine places them, [`place_engine_anchored`]).
const ASSEMBLED_METERS: [&str; 2] = ["hp_meter", "ap_meter"];

/// The engine-side placements the source engine performed in C++, against the screen extent:
/// the assembled meters at the bottom corners, the sneak text top centre (the XML authors only the
/// meter's 356 × 34 size, and a child's x / y are screen positions unless it copies its parent's).
/// Run at launch and again after a resize (M48 GUI-REG-008).
fn place_engine_anchored(renderer: &mut MenuRenderer, profile: &HudGameProfile, width: f32, height: f32) {
    if matches!(profile.style, HudStyle::Assembled { .. }) {
        for (name, x) in ASSEMBLED_METERS.iter().zip([30.0, width - 330.0]) {
            renderer.set_override(name, "x", x);
            renderer.set_override(name, "y", height - 100.0);
        }
    }
    if let HudSneak::Text { meter, text } = profile.sneak {
        renderer.set_override(meter, "x", width / 2.0 - 178.0);
        renderer.set_override(meter, "y", 40.0);
        renderer.set_override(text, "x", width / 2.0);
        renderer.set_override(text, "y", 40.0);
    }
}

/// The sneak element's alpha this frame: 0 without an indicator, a 0.5 s
/// pulse in the danger state (`Fallout3.exe` `0x937bd2`), 255 otherwise.
fn sneak_alpha(view: Option<&byroredux_native_ui::StealthIndicatorView>, secs: f32) -> u8 {
    match view.map(|view| view.state) {
        None => 0,
        Some(byroredux_core::stealth::IndicatorState::Danger) => {
            (255.0 * (std::f32::consts::PI * secs / 0.5).sin().abs()) as u8
        }
        Some(_) => 255,
    }
}

/// Drive the vanilla sneak element from this frame's indicator.
fn drive_sneak_element(
    renderer: &mut MenuRenderer,
    sneak: HudSneak,
    view: Option<&byroredux_native_ui::StealthIndicatorView>,
    alpha: u8,
) {
    use byroredux_core::stealth::IndicatorState;
    match sneak {
        HudSneak::Text { text, .. } => {
            renderer.set_override(text, "alpha", f32::from(alpha));
            let Some(view) = view else { return };
            if let byroredux_native_ui::StealthIndicatorLook::Label(label) = &view.look {
                renderer.set_override_str(text, "string", label);
            }
            let alarmed = view.state >= IndicatorState::Caution;
            renderer.set_override(text, "systemcolor", if alarmed { 2.0 } else { 1.0 });
            renderer.set_override(text, "user0", f32::from(u8::from(view.state == IndicatorState::Danger)));
        }
        HudSneak::Eye { eye, .. } => {
            let lit = view.is_some_and(|view| view.state > IndicatorState::Hidden);
            renderer.set_override(eye, "user8", if lit { 255.0 } else { 0.0 });
        }
    }
}

/// Bar fractions for the HUD: pinned debug values win, then the player's
/// own actor values, else full bars.
///
/// Every profile resolves its bar editor ids through the per-load
/// `PlayerVitals` table and reads `PlayerEntity`'s `ActorValues` only
/// (#4675) — no literal FormIDs, no scan of other actors. Oblivion's
/// engine-supplied records resolve through the same table. `hud.values`
/// can still pin the bars for deterministic smokes.
fn bar_fractions(world: &World, control: &HudControl, profile: &HudGameProfile) -> [f32; 3] {
    // #4675 — editor ids resolve through the per-load `PlayerVitals`
    // table (the same resolution `build_player_vitals` performs), never
    // embedded literal FormIDs.
    let vitals = world.try_resource::<PlayerVitals>();
    let mut out = [1.0f32; 3];
    for (slot, bar) in profile.bars.iter().enumerate() {
        let key = bar
            .av
            .and_then(|id| vitals.as_ref().and_then(|v| v.resolved(id)));
        out[slot] = fraction(world, key, control.bars[slot]);
    }
    // hud.status's auto arm reports these; the driver computes them
    // every frame, before the change-signature skip.
    if let Some(mut control_mut) = world.try_resource_mut::<HudControl>() {
        control_mut.live = out.map(Some);
    }
    out
}

/// One bar's fraction: pinned debug value wins, then the player's actor
/// value for `av` (a resolved AVIF FormID): current over its undamaged maximum, empty when the player
/// carries the pool with no maximum, and full when it carries no such pool or `av` is unresolved. Shared with the
/// Scaleform HUD driver, which uses the same `PlayerVitals` resolution and
/// pin semantics.
pub(crate) fn fraction(world: &World, av: Option<u32>, pinned: Option<f32>) -> f32 {
    pinned.map(|v| v.clamp(0.0, 1.0)).unwrap_or_else(|| {
        let av = match av {
            Some(av) => av,
            None => return 1.0,
        };
        // #4675 — the PLAYER's values only. The old "first stamped actor"
        // fallback scanned the whole `ActorValues` storage in insertion
        // (swap-remove perturbed) order, so the bar could track any NPC
        // that happened to carry the key — and it ran per bar per frame
        // before the change-signature check. `PlayerEntity` is the
        // contract the native vitals path already uses.
        let Some(player) = PlayerEntity::get(world) else {
            return 1.0;
        };
        let Some(values) = world.get::<ActorValues>(player) else {
            return 1.0;
        };
        let Some(entry) = values.get(av) else {
            return 1.0;
        };
        let max = entry.undamaged();
        if max > 0.0 {
            (entry.current() / max).clamp(0.0, 1.0)
        } else {
            // A pool the actor carries with no maximum has nothing to show: empty, not full (#7041). A pool
            // it does not carry at all (no entry, or no resolved key) stays at the "no information" full bar.
            0.0
        }
    })
}

/// FxHash-style combiner for the frame signature — cheap and stable
/// within a process, which is all the unchanged-skip needs. Shared with
/// the Scaleform HUD driver (`scaleform_hud.rs`).
pub(crate) fn hash_signature(sig: (u32, u32, u32, i32, u8)) -> u64 {
    let mut hash: u64 = 0x517c_c1b7_2722_0a95;
    for part in [
        sig.0 as u64,
        sig.1 as u64,
        sig.2 as u64,
        sig.3 as u64,
        sig.4 as u64,
    ] {
        hash = mix_signature(hash, part);
    }
    hash
}

/// Fold one more lane into a frame signature.
pub(crate) fn mix_signature(hash: u64, part: u64) -> u64 {
    (hash.rotate_left(5) ^ part).wrapping_mul(0x2545_f491_4f6c_dd1d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// M65 (SNK-HUD-003) — the vanilla sneak element, rendered from the
    /// installed corpus with no indicator and then with `view`; `None` when
    /// the archives are missing.
    fn sneak_frames(
        profile: HudGameProfile,
        data: std::path::PathBuf,
        view: byroredux_native_ui::StealthIndicatorView,
    ) -> Option<SneakFrames> {
        let assets = HudAssets::open_in(profile, &data)?;
        let mut renderer =
            MenuRenderer::load_with_profile(&assets, profile.menu_path, ScreenTraits::new(1280.0, 720.0), profile.menu)
                .expect("the vanilla HUD loads");
        if let HudStyle::Authored { mode, .. } = profile.style {
            renderer.set_override(mode.0, mode.1, 1.0);
        }
        let baseline = renderer.render_frame(&assets).to_vec();
        install_sneak_element(&mut renderer, &assets, profile.sneak);
        place_engine_anchored(&mut renderer, &profile, 1280.0, 720.0);
        drive_sneak_element(&mut renderer, profile.sneak, None, 0);
        let hidden = renderer.render_frame(&assets).to_vec();
        drive_sneak_element(&mut renderer, profile.sneak, Some(&view), 255);
        let shown = renderer.render_frame(&assets).to_vec();
        Some(SneakFrames { baseline, hidden, shown })
    }

    /// The HUD before the sneak element is installed, with it installed and no
    /// indicator, and with the indicator lit.
    struct SneakFrames {
        baseline: Vec<u8>,
        hidden: Vec<u8>,
        shown: Vec<u8>,
    }

    fn changed_pixels(a: &[u8], b: &[u8]) -> usize {
        a.as_chunks::<4>().0.iter().zip(b.as_chunks::<4>().0).filter(|(x, y)| x != y).count()
    }

    #[test]
    #[ignore = "needs FNV game data on disk"]
    fn the_fnv_hud_draws_the_sneak_label() {
        let data = byroredux_plugin::esm::test_paths::fnv_data_dir();
        let view = byroredux_native_ui::StealthIndicatorView {
            state: byroredux_core::stealth::IndicatorState::Hidden,
            look: byroredux_native_ui::StealthIndicatorLook::Label("[HIDDEN]".into()),
        };
        let Some(frames) = sneak_frames(HudGameProfile::fallout_nv(), data, view) else {
            byroredux_plugin::esm::test_paths::require_or_skip("the FNV Misc and Textures archives");
            return;
        };
        let stray = changed_pixels(&frames.baseline, &frames.hidden);
        assert_eq!(stray, 0, "the installed, unlit sneak label drew {stray} pixels");
        let changed = changed_pixels(&frames.hidden, &frames.shown);
        eprintln!("FNV sneak label: {changed} pixels");
        assert!(changed > 100, "the [HIDDEN] label drew {changed} pixels");
    }

    #[test]
    #[ignore = "needs Oblivion game data on disk"]
    fn the_oblivion_hud_lights_the_sneak_eye() {
        let data = byroredux_plugin::esm::test_paths::oblivion_data_dir();
        let view = byroredux_native_ui::StealthIndicatorView {
            state: byroredux_core::stealth::IndicatorState::Detected,
            look: byroredux_native_ui::StealthIndicatorLook::Eye,
        };
        let Some(frames) = sneak_frames(HudGameProfile::oblivion(), data, view) else {
            byroredux_plugin::esm::test_paths::require_or_skip("the Oblivion Misc and Textures archives");
            return;
        };
        // #7001 — the grafted reticle adds nothing until the eye lights: a
        // changed-pixel count alone passed while its crosshair (438 px at the
        // screen centre) was drawn in both frames.
        let stray = changed_pixels(&frames.baseline, &frames.hidden);
        assert_eq!(stray, 0, "the grafted reticle drew {stray} pixels with the eye off");
        let changed = changed_pixels(&frames.hidden, &frames.shown);
        eprintln!("Oblivion sneak eye: {changed} pixels");
        assert!(changed > 100, "the lit eye drew {changed} pixels");
    }

    fn argv(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
    }

    /// #4723 — both `--hud` routes refuse through this one helper when a
    /// `--menu` player owns the overlay, so the frame tick's hud-over-
    /// ui_manager preference can never freeze a focused menu player.
    #[test]
    fn a_menu_owned_overlay_blocks_both_hud_routes() {
        assert_eq!(
            menu_owned_overlay_skip(true),
            Some("hud: --menu owns the overlay — HUD route skipped"),
        );
        assert_eq!(menu_owned_overlay_skip(false), None);
    }

    /// #4724 — the player-vs-NPC fraction contract: the pinned debug value
    /// wins (clamped to 0..1); the auto path reads the PLAYER's stamped
    /// actor value only (#4675 — the pre-fix first-stamped-actor scan made
    /// a bar track any NPC), and every "no signal" arm degrades to full.
    #[test]
    fn fraction_pins_clamps_and_reads_only_the_player() {
        let mut world = World::new();
        let player = world.spawn();
        let mut values = ActorValues::new();
        // Health 25 of a 100 max → 0.25.
        values.set_base(43, 100.0);
        values.apply_damage(43, 75.0);
        world.insert(player, values);
        world.insert_resource(PlayerEntity(Some(player)));
        // A bystander NPC carrying the same key must be irrelevant.
        let npc = world.spawn();
        let mut npc_values = ActorValues::new();
        npc_values.set_base(43, 1000.0);
        world.insert(npc, npc_values);

        // Pinned wins over everything, clamped.
        assert_eq!(fraction(&world, Some(43), Some(0.75)), 0.75);
        assert_eq!(fraction(&world, Some(43), Some(9.0)), 1.0);
        assert_eq!(fraction(&world, Some(43), Some(-3.0)), 0.0);

        // Auto reads the player's 0.25, not the bystander's 1.0.
        assert!((fraction(&world, Some(43), None) - 0.25).abs() < 1e-6);

        // Every no-signal arm is full.
        assert_eq!(fraction(&world, None, None), 1.0, "no vitals resource");
        let mut bare = World::new();
        assert_eq!(fraction(&bare, Some(43), None), 1.0, "no player stamped");
        let orphan = bare.spawn();
        bare.insert(orphan, ActorValues::new());
        bare.insert_resource(PlayerEntity(Some(orphan)));
        assert_eq!(fraction(&bare, Some(43), None), 1.0, "key not carried");
        // #7041 — a pool the player carries with no maximum has nothing to show: empty, and it cannot divide.
        let mut zero = ActorValues::new();
        zero.set_base(43, 0.0);
        bare.insert(orphan, zero);
        assert_eq!(fraction(&bare, Some(43), None), 0.0, "carried zero-max pool is empty");
    }

    /// #5508 — the bar's `max` includes the `set_override` layer: with
    /// base 105 + override 500 and 200 damage the bar reads 405/605, not a
    /// clamped-full 1.0 against a stale 105 max.
    #[test]
    fn fraction_max_includes_the_set_override_layer() {
        let mut world = World::new();
        let player = world.spawn();
        let mut values = ActorValues::new();
        values.set_base(43, 105.0);
        values.set_override(43, 500.0);
        values.apply_damage(43, 200.0);
        world.insert(player, values);
        world.insert_resource(PlayerEntity(Some(player)));
        let want = 405.0 / 605.0;
        assert!((fraction(&world, Some(43), None) - want).abs() < 1e-6);
    }

    /// #4724 — the change-signature combiner: stable within a process, and
    /// every lane participates (a lane that stopped mattering would freeze
    /// the HUD upload on its first frame).
    #[test]
    fn hash_signature_is_stable_and_lane_sensitive() {
        let base = (1u32, 2u32, 3u32, 4i32, 5u8);
        assert_eq!(hash_signature(base), hash_signature(base));
        let (b0, b1, b2, b3, b4) = base;
        for candidate in [
            (b0 + 1, b1, b2, b3, b4),
            (b0, b1 + 1, b2, b3, b4),
            (b0, b1, b2 + 1, b3, b4),
            (b0, b1, b2, b3 + 1, b4),
            (b0, b1, b2, b3, b4 + 1),
        ] {
            assert_ne!(
                hash_signature(base),
                hash_signature(candidate),
                "lane {candidate:?} must change the signature"
            );
        }
    }

    /// #4724 — the bar table composes `fraction` per profile slot: a pinned
    /// control value wins per slot, an unpinned one falls through to the
    /// player's actor values (full when the player carries nothing), and the
    /// driver publishes what it derived into `HudControl::live`.
    #[test]
    fn bar_fractions_composes_the_profile_table_through_fraction() {
        let mut world = World::new();
        let player = world.spawn();
        let mut values = ActorValues::new();
        values.set_base(7, 100.0);
        values.apply_damage(7, 90.0);
        world.insert(player, values);
        world.insert_resource(PlayerEntity(Some(player)));

        let profile = HudGameProfile::oblivion();
        let control = HudControl {
            bars: [Some(0.1), None, Some(0.9)],
            ..HudControl::default()
        };
        world.insert_resource(control);
        // The player carries no Oblivion bar keys here: slot 0 pinned (0.1),
        // slot 1 auto → full, slot 2 pinned (0.9).
        let out = bar_fractions(&world, &control, &profile);
        assert_eq!(out[0], 0.1);
        assert!((out[1] - 1.0).abs() < 1e-6);
        assert_eq!(out[2], 0.9);
        // The auto arm's derivation is published for hud.status.
        {
            let control = world.resource::<HudControl>();
            assert_eq!(control.live[1], Some(1.0), "the auto slot's derivation");
            assert_eq!(control.live[0], Some(0.1), "pinned slots publish too");
        }
    }

    #[test]
    fn oblivion_hud_tracks_all_three_installed_vital_values() {
        use byroredux_core::character::CharacterRulesProfile;
        use byroredux_plugin::esm::{reader::{GameKind, GlobalSlot}, records::EsmIndex};
        let mut index = EsmIndex { game: GameKind::Oblivion, character_rules: CharacterRulesProfile::OBLIVION, ..Default::default() };
        index.install_oblivion_actor_values(GlobalSlot::Regular(1)).unwrap();
        let mut world = World::new();
        crate::inventory::install_catalog(&mut world, &index);
        let player = world.spawn();
        let mut values = ActorValues::new();
        for (name, base, damage) in [("Health", 100.0, 40.0), ("Magicka", 80.0, 20.0), ("Fatigue", 200.0, 100.0)] {
            let id = index.actor_value_form_id(name).unwrap();
            values.set_base(id, base);
            values.apply_damage(id, damage);
        }
        world.insert(player, values);
        world.insert_resource(PlayerEntity(Some(player)));
        let control = HudControl::default();
        world.insert_resource(control);
        let fractions = bar_fractions(&world, &control, &HudGameProfile::oblivion());
        assert_eq!(fractions, [0.6, 0.75, 0.5]);
        assert_eq!(world.resource::<HudControl>().live, [Some(0.6), Some(0.75), Some(0.5)]);
    }

    /// A data directory holding the named (empty) files — `hud_archive_args`
    /// only checks that archives exist beside the ESM; opening is later.
    fn data_dir(files: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for file in files {
            std::fs::write(dir.path().join(file), b"").unwrap();
        }
        dir
    }

    /// #4718 — the game is decided by the load order, not by what `--esm`
    /// is called. A DLC or mod plugin masters to its game's base ESM.
    #[test]
    fn fallout_profile_follows_the_load_order_not_the_esm_name() {
        let empty = data_dir(&[]);
        let dir = empty.path();
        assert_eq!(
            fallout_profile(&["FalloutNV.esm"], dir).label,
            "Fallout: New Vegas"
        );
        assert_eq!(fallout_profile(&["Fallout3.esm"], dir).label, "Fallout 3");
        // The reported shape: a New Vegas DLC listed as `--esm`.
        assert_eq!(
            fallout_profile(&["FalloutNV.esm", "HonestHearts.esm"], dir).label,
            "Fallout: New Vegas"
        );
        assert_eq!(
            fallout_profile(&["Fallout3.esm", "Anchorage.esm"], dir).label,
            "Fallout 3"
        );
        // File names compare case-insensitively.
        assert_eq!(
            fallout_profile(&["falloutnv.esm", "MyMod.esp"], dir).label,
            "Fallout: New Vegas"
        );
    }

    /// A plugin launched without naming its master falls back to which base
    /// ESM sits beside the corpus; with neither, Fallout 3 is the default.
    #[test]
    fn fallout_profile_falls_back_to_the_base_esm_beside_the_corpus() {
        let new_vegas = data_dir(&["FalloutNV.esm"]);
        assert_eq!(
            fallout_profile(&["MyMod.esm"], new_vegas.path()).label,
            "Fallout: New Vegas"
        );
        let neither = data_dir(&[]);
        assert_eq!(
            fallout_profile(&["MyMod.esm"], neither.path()).label,
            "Fallout 3"
        );
        // An explicitly listed Fallout 3 master beats a stray FalloutNV.esm.
        assert_eq!(
            fallout_profile(&["Fallout3.esm", "MyMod.esm"], new_vegas.path()).label,
            "Fallout 3"
        );
    }

    /// End to end through `hud_archive_args`: the New Vegas DLC launch that
    /// used to resolve the FO3 profile and a transparent HUD.
    #[test]
    fn a_new_vegas_dlc_launch_resolves_the_new_vegas_profile() {
        // No `FalloutNV.esm` on disk: the `--master` argument alone must
        // decide, not the directory fallback.
        let dir = data_dir(&[
            "HonestHearts.esm",
            "Fallout - Misc.bsa",
            "Fallout - Textures.bsa",
            "Fallout - Textures2.bsa",
        ]);
        let esm = dir.path().join("HonestHearts.esm");
        let master = std::path::Path::new("elsewhere").join("FalloutNV.esm");
        let args = argv(&[
            "--master",
            master.to_str().unwrap(),
            "--esm",
            esm.to_str().unwrap(),
            "--hud",
        ]);
        let (misc, textures, profile) = hud_archive_args(&args).unwrap().unwrap();
        assert_eq!(profile.label, "Fallout: New Vegas");
        assert_eq!(profile.menu.font_paths.len(), 9, "FNV's ninth font slot");
        assert_eq!(
            std::path::Path::new(&misc).file_name().unwrap(),
            "Fallout - Misc.bsa"
        );
        // The anchor is `Textures.bsa`; `open_with_numeric_siblings` adds
        // `Textures2.bsa`, which carries FNV's HUD art and every font.
        assert_eq!(
            std::path::Path::new(&textures).file_name().unwrap(),
            "Fallout - Textures.bsa"
        );
        assert!(
            byroredux_bsa::numeric_sibling_paths(&textures)
                .iter()
                .any(|sibling| sibling.ends_with("Fallout - Textures2.bsa")),
            "the anchor must pull FNV's Textures2 in"
        );
    }

    /// The Fallout 3 launch is unchanged, and an explicit `--hud-textures`
    /// still wins over the profile default.
    #[test]
    fn a_fallout_3_launch_and_the_textures_override_still_resolve() {
        let dir = data_dir(&["Fallout3.esm", "Fallout - Misc.bsa", "Fallout - Textures.bsa"]);
        let esm = dir.path().join("Fallout3.esm");
        let mut args = argv(&["--esm", esm.to_str().unwrap(), "--hud"]);
        let (_, textures, profile) = hud_archive_args(&args).unwrap().unwrap();
        assert_eq!(profile.label, "Fallout 3");
        assert!(textures.ends_with("Fallout - Textures.bsa"));

        args.extend(argv(&["--hud-textures", "elsewhere/My Textures.bsa"]));
        let (_, textures, _) = hud_archive_args(&args).unwrap().unwrap();
        assert_eq!(textures, "elsewhere/My Textures.bsa");
    }
}
