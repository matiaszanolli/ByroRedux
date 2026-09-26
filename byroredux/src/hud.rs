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
//! exclusive per run (scene.rs launches the Scaleform probe first; a
//! won Scaleform route suppresses this module's MenuXml launch).
//!
//! Shared machinery — archive resolution, triple-buffered overlay
//! textures, change-signature + cadence throttling — is game-agnostic.
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

use crate::asset_provider::{open_with_numeric_siblings, Archive};
use crate::inventory::PlayerVitals;
use crate::systems::PlayerEntity;

/// Skyrim-profile AVIF keys the engine already stamps (`0x3E8` Health,
/// `0x3E9` Magicka, `0x3EA` Stamina). Oblivion's index-based actor
/// values are not yet stamped onto the player capsule, so the Oblivion
/// HUD reads these opportunistically and falls back to full bars.
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
    menu: MenuProfile,
    style: HudStyle,
    bars: &'static [HudBar],
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
    profile: HudGameProfile,
}

impl HudAssets {
    fn extract_texture(&self, path: &str) -> Option<Vec<u8>> {
        self.textures
            .iter()
            .rev()
            .find_map(|archive| archive.extract(path).ok())
    }
}

impl MenuAssets for HudAssets {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        self.misc.extract(path).ok()
    }
    fn texture(&self, path: &str) -> Option<Vec<u8>> {
        self.extract_texture(path)
    }
    fn font(&self, index: u8) -> Option<Vec<u8>> {
        let path = self.profile.menu.font_paths.get(index as usize - 1)?;
        match self.profile.menu.font_archive {
            FontArchive::Misc => self.misc.extract(path).ok(),
            FontArchive::Textures => self.extract_texture(path),
        }
    }
    fn font_texture(&self, path: &str) -> Option<Vec<u8>> {
        match self.profile.menu.font_archive {
            FontArchive::Misc => self.misc.extract(path).ok(),
            FontArchive::Textures => self
                .extract_texture(path)
                .or_else(|| self.misc.extract(path).ok()),
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
    /// needs `&mut self` for the rotation, so the pixels cross through this
    /// owned buffer — retained across refreshes instead of a fresh
    /// swapchain-sized Vec per changed tick.
    upload_buffer: Vec<u8>,
    assets: HudAssets,
    profile: HudGameProfile,
    /// Compass strip texels per degree of heading (0 = strip unknown —
    /// the compass stays at cropx 0 rather than guessing).
    px_per_degree: f32,
    /// Triple-buffered overlay textures, cycled per *upload*. In-flight
    /// frames (two, per the renderer's frames-in-flight) may sample the
    /// current and previous buffers, so uploads always target the buffer
    /// last sampled three frames ago — the hazard contract
    /// [`Texture::overwrite_rgba_pixels`] requires. Fixed handles also
    /// mean zero allocations and zero bindless descriptor writes after
    /// launch, where [`TextureRegistry::update_rgba`] would reallocate a
    /// full image per call.
    texture_handles: [u32; 3],
    current: usize,
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
fn hud_archive_args(args: &[String]) -> Result<Option<(String, String, HudGameProfile)>, String> {
    if !args.iter().any(|arg| arg == "--hud") {
        return Ok(None);
    }
    let esm = args
        .iter()
        .position(|a| a == "--esm")
        .and_then(|i| args.get(i + 1))
        .filter(|v| !v.starts_with("--"))
        .cloned()
        .ok_or_else(|| "--hud requires --esm <path> (archive discovery root)".to_string())?;
    let esm_dir = std::path::PathBuf::from(&esm)
        .parent()
        .map(|d| d.to_path_buf())
        .ok_or_else(|| "--hud: cannot resolve the --esm directory".to_string())?;

    let load_order: Vec<&str> = args
        .iter()
        .enumerate()
        .filter(|(_, arg)| *arg == "--master")
        .filter_map(|(i, _)| args.get(i + 1))
        .filter(|value| !value.starts_with("--"))
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

/// Launch the HUD when `--hud` is present. Mirrors `launch_archive_menu`:
/// opens the archives, assembles the per-game HUD content, registers the
/// transparent overlay textures, and logs the `hud: loaded` line a smoke
/// gate can grep.
pub(crate) fn launch_hud(
    ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
    world: &mut World,
    args: &[String],
) -> Option<MenuXmlHud> {
    let (misc_path, textures_path, profile) = match hud_archive_args(args) {
        Ok(Some(triple)) => triple,
        Ok(None) => return None,
        Err(error) => {
            log::error!("{error}");
            return None;
        }
    };
    let misc = match Archive::open(&misc_path) {
        Ok(misc) => misc,
        Err(e) => {
            log::error!("hud: archive open failed: {e}");
            return None;
        }
    };
    // `open_with_numeric_siblings` reports a failed open as a warning and
    // pushes nothing, so an empty pool is the failure signal.
    let mut textures = Vec::new();
    open_with_numeric_siblings(
        &textures_path,
        "HUD textures",
        &mut textures,
        &mut std::collections::HashSet::new(),
    );
    if textures.is_empty() {
        log::error!("hud: texture archive open failed: '{textures_path}'");
        return None;
    }
    let assets = HudAssets {
        misc,
        textures,
        profile,
    };

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
                let layout = [
                    ("hp_meter", 30.0),
                    ("ap_meter", w as f32 - 330.0),
                ];
                for (i, (name, x)) in layout.iter().enumerate() {
                    match renderer.graft_fragment(&assets, meter_prefab, containers[i], name) {
                        Ok(_) => {
                            renderer.set_override(name, "x", *x);
                            renderer.set_override(name, "y", h as f32 - 100.0);
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

            // Same transparent initial upload the `--menu` route uses, so
            // the composite quad exists before the first rasterized frame.
            // (The registration closure below rebuilds its own upload ctx —
            // the outer one went unused after the triple-buffer rework.)
            let register = |ctx: &mut byroredux_renderer::vulkan::context::VulkanContext| {
                let allocator = ctx.allocator.as_ref().unwrap();
                let upload_ctx = byroredux_renderer::vulkan::GpuUploadCtx {
                    device: &ctx.device,
                    allocator,
                    queue: &ctx.graphics_queue,
                    command_pool: ctx.transfer_pool,
                };
                ctx.texture_registry
                    .register_rgba(upload_ctx, w, h, &vec![0u8; (w * h * 4) as usize])
            };
            match (register(ctx), register(ctx), register(ctx)) {
                (Ok(h0), Ok(h1), Ok(h2)) => {
                    log::info!(
                        "hud: loaded {} misc='{}' textures='{}' textures={h0}/{h1}/{h2} \
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
                        texture_handles: [h0, h1, h2],
                        current: 0,
                        width: w,
                        height: h,
                        last_signature: 0,
                        last_upload: std::time::Instant::now(),
                    })
                }
                _ => {
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
        let hash = hash_signature((
            fractions[0].to_bits(),
            fractions[1].to_bits(),
            fractions[2].to_bits(),
            (heading * 10.0) as i32,
            u8::from(control.visible),
        ));
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
            "hud: render bars={} heading={heading:.1} visible={} -> buffer {}",
            fractions
                .iter()
                .take(self.profile.bar_count())
                .map(|f| format!("{f:.2}"))
                .collect::<Vec<_>>()
                .join("/"),
            u8::from(control.visible),
            (self.current + 1) % self.texture_handles.len()
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

    /// The handle the frame being recorded should composite.
    pub fn current_texture(&self) -> u32 {
        self.texture_handles[self.current]
    }

    /// Upload `pixels` into the next buffer of the rotation and advance.
    /// Returns the handle to composite this frame.
    ///
    /// The overwritten buffer was last sampled three frames ago, so no
    /// in-flight frame still reads it (see the field docs).
    pub fn upload_frame(
        &mut self,
        ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
        pixels: &[u8],
    ) -> u32 {
        let target = (self.current + 1) % self.texture_handles.len();
        let allocator = ctx.allocator.as_ref().unwrap();
        let upload_ctx = byroredux_renderer::vulkan::GpuUploadCtx {
            device: &ctx.device,
            allocator,
            queue: &ctx.graphics_queue,
            command_pool: ctx.transfer_pool,
        };
        let (w, h) = self.frame_size();
        if let Err(error) = ctx.texture_registry.write_rgba_inplace(
            upload_ctx,
            self.texture_handles[target],
            w,
            h,
            pixels,
        ) {
            log::error!("hud: texture upload failed: {error:#}");
        }
        self.current = target;
        self.texture_handles[self.current]
    }

    /// #4608 — render + upload in one `&mut self` call so the caller never
    /// copies the frame out to release a borrow: on change, the raster
    /// crosses through the persistent [`Self::upload_buffer`] (one copy
    /// into retained memory, not a fresh allocation), then the rotation
    /// advances. `None` = unchanged (or rate-limited) — keep compositing
    /// the current texture.
    pub fn render_and_upload(
        &mut self,
        ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
        world: &World,
        camera_forward: [f32; 3],
        control: &HudControl,
    ) -> Option<u32> {
        if self.render(world, camera_forward, control).is_none() {
            return None;
        }
        // Split borrow: fill the persistent buffer while only `renderer`
        // is borrowed, then hand upload_frame the filled buffer — the
        // method takes &mut self for the rotation, so the pixel source
        // must be self-owned by then.
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

/// Bar fractions for the HUD: pinned debug values win, then any stamped
/// actor values on an actor entity, else full bars.
///
/// Oblivion NPCs do not yet carry AVIF-keyed `ActorValues` (the
/// index-keyed Oblivion actor-value profile is future work), so today
/// the Oblivion profile reads the Skyrim-keyed values that *are*
/// stamped in the Skyrim flow; FO3/FNV read their own AVIF keys.
/// `hud.values` drives everything deterministically for smokes either
/// way.
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

/// One bar's fraction: pinned debug value wins, then any stamped actor
/// value, else full. Shared with the Scaleform HUD driver (Skyrim uses
/// the same AVIF keys and the same pin semantics).
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
        let Some(player) = world.try_resource::<PlayerEntity>().and_then(|r| r.0) else {
            return 1.0;
        };
        let Some(values) = world.get::<ActorValues>(player) else {
            return 1.0;
        };
        let Some(entry) = values.get(av) else {
            return 1.0;
        };
        let max = entry.base + entry.permanent_mod + entry.temporary_mod;
        if max > 0.0 {
            (entry.current() / max).clamp(0.0, 1.0)
        } else {
            1.0
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
        hash = (hash.rotate_left(5) ^ part).wrapping_mul(0x2545_f491_4f6c_dd1d);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(values: &[&str]) -> Vec<String> {
        values.iter().map(|v| v.to_string()).collect()
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
