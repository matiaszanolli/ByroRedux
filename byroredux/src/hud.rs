//! Oblivion MenuXml HUD — the M48.4 legacy-UI track's engine side.
//!
//! [`byroredux_menuxml`] parses and renders Bethesda's XML menus; this
//! module is the `byroredux` half: resolves the two archives a HUD needs
//! (`Oblivion - Misc.bsa` for menu XML + fonts, a texture BSA for menu
//! art), registers the overlay texture through the same
//! [`crate::asset_provider::archive::Archive`] path `--menu` uses, and
//! drives the per-frame trait overrides (bar fractions, compass heading)
//! from engine state.
//!
//! Console control (`hud.on` / `hud.off` / `hud.values` / `hud.heading`)
//! flows through the [`HudControl`] World resource — commands can only
//! reach resources, and the renderer itself is owned by the frame loop
//! (same split as `LightTuning`).

use byroredux_core::ecs::components::ActorValues;
use byroredux_core::ecs::World;
use byroredux_menuxml::menu::MenuAssets;
use byroredux_menuxml::{MenuRenderer, ScreenTraits};

use crate::asset_provider::Archive;

/// Oblivion.ini `[Fonts]` order — the `<font>` trait's 1-based table.
const FONT_PATHS: [&str; 5] = [
    "fonts\\Kingthings_Regular.fnt",
    "fonts\\Kingthings_Shadowed.fnt",
    "fonts\\Tahoma_Bold_Small.fnt",
    "fonts\\Daedric_Font.fnt",
    "fonts\\Handwritten.fnt",
];

/// Skyrim-profile AVIF keys the engine already stamps (`0x3E8` Health,
/// `0x3E9` Magicka, `0x3EA` Stamina). Oblivion's index-based actor
/// values are not yet stamped onto the player capsule, so the HUD reads
/// these opportunistically and falls back to full bars.
const AV_HEALTH: u32 = 0x3E8;
const AV_MAGICKA: u32 = 0x3E9;
const AV_STAMINA: u32 = 0x3EA;

/// The two archives a HUD render needs.
pub(crate) struct HudAssets {
    misc: Archive,
    textures: Archive,
}

impl MenuAssets for HudAssets {
    fn menu_xml(&self, path: &str) -> Option<Vec<u8>> {
        self.misc.extract(path).ok()
    }
    fn texture(&self, path: &str) -> Option<Vec<u8>> {
        self.textures.extract(path).ok()
    }
    fn font(&self, index: u8) -> Option<Vec<u8>> {
        self.misc.extract(FONT_PATHS.get(index as usize - 1)?).ok()
    }
    fn font_texture(&self, path: &str) -> Option<Vec<u8>> {
        self.misc.extract(path).ok()
    }
}

/// Console-facing HUD control. Inserted at launch; `hud.*` commands and
/// the frame-loop driver both go through it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HudControl {
    pub visible: bool,
    /// Pinned bar fractions (0..1). `None` = derive from actor values /
    /// default full — set by `hud.values`, cleared by `hud.values auto`.
    pub health: Option<f32>,
    pub magicka: Option<f32>,
    pub fatigue: Option<f32>,
    /// Pinned compass heading in degrees. `None` = follow the camera.
    pub heading: Option<f32>,
}

impl byroredux_core::ecs::Resource for HudControl {}

impl Default for HudControl {
    fn default() -> Self {
        Self {
            visible: true,
            health: None,
            magicka: None,
            fatigue: None,
            heading: None,
        }
    }
}

/// The live HUD owned by the frame loop (see module docs for why it is
/// not a World resource: the driver runs beside the render tick, and
/// `HudControl` is the command surface).
pub(crate) struct OblivionHud {
    renderer: MenuRenderer,
    assets: HudAssets,
    pub texture_handle: u32,
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

/// Resolve `--hud` into archive paths.
///
/// Requires the Misc BSA (menus + fonts); the texture BSA defaults to the
/// sibling `Oblivion - Textures - Compressed.bsa` and can be overridden
/// with `--hud-textures <path>`. Discovery walks the `--esm` directory so
/// the smoke fixtures only add one flag.
fn hud_archive_args(args: &[String]) -> Result<Option<(String, String)>, String> {
    let Some(hud_index) = args.iter().position(|arg| arg == "--hud") else {
        return Ok(None);
    };
    let esm_dir = args
        .iter()
        .position(|a| a == "--esm")
        .and_then(|i| args.get(i + 1))
        .filter(|v| !v.starts_with("--"))
        .map(std::path::PathBuf::from)
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
        .ok_or_else(|| "--hud requires --esm <path> (archive discovery root)".to_string())?;

    let misc = esm_dir.join("Oblivion - Misc.bsa");
    if !misc.is_file() {
        return Err(format!(
            "--hud: '{}' not found (vanilla 'Oblivion - Misc.bsa' carries the menu XMLs and fonts)",
            misc.display()
        ));
    }
    let textures = if let Some(t_idx) = args.iter().position(|a| a == "--hud-textures") {
        let path = args
            .get(t_idx + 1)
            .filter(|v| !v.starts_with("--"))
            .ok_or_else(|| "--hud-textures requires a BSA path".to_string())?;
        std::path::PathBuf::from(path)
    } else {
        let candidate = esm_dir.join("Oblivion - Textures - Compressed.bsa");
        if !candidate.is_file() {
            return Err(format!(
                "--hud: no texture archive — pass --hud-textures <path> (no '{}' beside the ESM)",
                candidate.display()
            ));
        }
        candidate
    };
    let _ = hud_index;
    Ok(Some((
        misc.to_string_lossy().into_owned(),
        textures.to_string_lossy().into_owned(),
    )))
}

/// Launch the HUD when `--hud` is present. Mirrors `launch_archive_menu`:
/// opens the archives, registers the transparent overlay texture, and
/// logs the `hud: loaded` line a smoke gate can grep.
pub(crate) fn launch_hud(
    ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
    world: &mut World,
    args: &[String],
) -> Option<OblivionHud> {
    let pair = match hud_archive_args(args) {
        Ok(Some(pair)) => pair,
        Ok(None) => return None,
        Err(error) => {
            log::error!("{error}");
            return None;
        }
    };
    let (misc_path, textures_path) = &pair;
    let assets = match (Archive::open(misc_path), Archive::open(textures_path)) {
        (Ok(misc), Ok(textures)) => HudAssets { misc, textures },
        (Err(e), _) | (_, Err(e)) => {
            log::error!("hud: archive open failed: {e}");
            return None;
        }
    };

    let (w, h) = ctx.swapchain_extent();
    match MenuRenderer::load(
        &assets,
        "menus\\main\\hud_main_menu.xml",
        ScreenTraits::new(w as f32, h as f32),
    ) {
        Ok(renderer) => {
            let allocator = ctx.allocator.as_ref().unwrap();
            let upload_ctx = byroredux_renderer::vulkan::GpuUploadCtx {
                device: &ctx.device,
                allocator,
                queue: &ctx.graphics_queue,
                command_pool: ctx.transfer_pool,
            };
            // Same transparent initial upload the `--menu` route uses, so
            // the composite quad exists before the first rasterized frame.
            match ctx.texture_registry.register_rgba(
                upload_ctx,
                w,
                h,
                &vec![0u8; (w * h * 4) as usize],
            ) {
                Ok(handle) => {
                    log::info!(
                        "hud: loaded menus\\main\\hud_main_menu.xml misc='{}' textures='{}' \
                         texture={handle} ({w}x{h}, {} NIF tiles skipped)",
                        misc_path,
                        textures_path,
                        renderer.nif_tiles
                    );
                    world.insert_resource(HudControl::default());
                    Some(OblivionHud {
                        renderer,
                        assets,
                        texture_handle: handle,
                        width: w,
                        height: h,
                        last_signature: 0,
                        last_upload: std::time::Instant::now(),
                    })
                }
                Err(error) => {
                    log::error!("hud: UI texture registration failed: {error:#}");
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

impl OblivionHud {
    /// Compute this frame's trait values and rasterize.
    ///
    /// `camera_forward` drives the compass heading (Oblivion's north is
    /// Gamebryo +Y, which the Z-up→Y-up import maps to engine −Z; east
    /// stays +X — so heading = `atan2(f.x, -f.z)` degrees, 0 = north).
    /// Compute this frame's trait values and rasterize. `None` means
    /// "unchanged since the last uploaded frame" — keep compositing the
    /// existing texture (mirrors `UiFrame::Unchanged`).
    ///
    /// `camera_forward` drives the compass heading (Oblivion's north is
    /// Gamebryo +Y, which the Z-up→Y-up import maps to engine −Z; east
    /// stays +X — so heading = `atan2(f.x, -f.z)` degrees, 0 = north).
    pub fn render(
        &mut self,
        world: &World,
        camera_forward: [f32; 3],
        control: &HudControl,
    ) -> Option<&[u8]> {
        let (health, magicka, fatigue) = bar_fractions(world, control);
        let heading = control.heading.unwrap_or_else(|| {
            f32::atan2(camera_forward[0], -camera_forward[2])
                .to_degrees()
                .rem_euclid(360.0)
        });
        // Heading quantized to 0.1° — the compass face is 2048 px over
        // 360° (~0.18°/px), so finer deltas are sub-pixel.
        let hash = hash_signature((
            health.to_bits(),
            magicka.to_bits(),
            fatigue.to_bits(),
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

        self.renderer.set_override("HUDMainMenu", "user3", 1.0);
        self.renderer
            .set_override("hudmain_health_full", "user0", health);
        self.renderer
            .set_override("hudmain_magic_full", "user0", magicka);
        self.renderer
            .set_override("hudmain_fatigue_full", "user0", fatigue);
        self.renderer
            .set_override("hudmain_compass_window", "user0", heading);
        Some(self.renderer.render_frame(&self.assets))
    }

    pub fn frame_size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

/// Bar fractions for the HUD: pinned debug values win, then any stamped
/// Skyrim-profile actor values on an actor entity, else full bars.
///
/// Oblivion NPCs do not yet carry AVIF-keyed `ActorValues` (the
/// index-keyed Oblivion actor-value profile is future work), so today
/// this reads the Skyrim-keyed values that *are* stamped on actors in
/// the Skyrim flow and defaults to full elsewhere — the HUD renders
/// correct art either way, and `hud.values` drives it deterministically
/// for smokes.
fn bar_fractions(world: &World, control: &HudControl) -> (f32, f32, f32) {
    let fallback = (control.health, control.magicka, control.fatigue);
    let fraction = |av: u32, pinned: Option<f32>| -> f32 {
        pinned.map(|v| v.clamp(0.0, 1.0)).unwrap_or_else(|| {
            let mut found = None;
            if let Some(query) = world.query::<ActorValues>() {
                for (_, values) in query.iter() {
                    if let Some(av) = values.get(av) {
                        found = Some(*av);
                        break;
                    }
                }
            }
            match found {
                Some(av) => {
                    let max = av.base + av.permanent_mod + av.temporary_mod;
                    if max > 0.0 {
                        (av.current() / max).clamp(0.0, 1.0)
                    } else {
                        1.0
                    }
                }
                None => 1.0,
            }
        })
    };
    (
        fraction(AV_HEALTH, fallback.0),
        fraction(AV_MAGICKA, fallback.1),
        fraction(AV_STAMINA, fallback.2),
    )
}

/// FxHash-style combiner for the frame signature — cheap and stable
/// within a process, which is all the unchanged-skip needs.
fn hash_signature(sig: (u32, u32, u32, i32, u8)) -> u64 {
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
