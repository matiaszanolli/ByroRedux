//! Scaleform HUD — the M48.6 (Skyrim) / M48.7 (Fallout 4) engine side.
//!
//! These games' HUDs are not MenuXml: they are Scaleform movies
//! (`interface\hudmenu.swf` — Skyrim: `Skyrim - Interface.bsa` AVM1;
//! FO4: `Fallout4 - Interface.ba2` AVM2) executed by the Ruffle player
//! that `--menu` already runs end to end. This module is the `--hud`
//! route: it builds the [`byroredux_ui::UiManager`] exactly like the
//! `--menu` route does, keeps world input alive (a HUD is not a modal
//! menu), and services the movie's host protocol.
//!
//! **What the vanilla HUDs are** (pinned by `crates/ui/tests/
//! hudmenu_protocol.rs` + `fallout4_hudmenu_protocol.rs` + the smokes'
//! `hud.debug` gates): passive chrome. Both render their bar/compass/
//! crosshair art at boot and make no state requests while idle —
//! Skyrim's hudmenu calls exactly four host methods, FO4's hudmenu
//! makes zero `BGSCodeObj` calls at idle, and neither registers any
//! state-callback beyond the GameDelegate pair (Skyrim) or the
//! adapter's lifecycle hooks (FO4). In the vanilla engines the meters
//! are fed by GFx *object-path* invocation on HUD components, a
//! surface Ruffle does not expose (`call_internal_interface` reaches
//! registered ExternalInterface callbacks only). So this slice renders
//! and composites the HUD chrome over the live frame and services the
//! protocol; driving the *meters* needs either movie-side injection
//! (Ruffle-surface follow-up) or mod menus whose poll protocol the
//! response handlers can answer.
//!
//! Mechanics shared with the MenuXml HUD ([`crate::hud`]): the driver is
//! owned by the frame loop — it rides `tick_ui_overlay`'s Ruffle branch,
//! which also keeps host-call draining alive — and console state flows
//! through the [`HudControl`] resource. The bridge handle is `Rc`-based
//! and deliberately not `Send`, so diagnostics reach the console through
//! the [`ScaleformHudDiag`] resource mirror instead of the bridge
//! directly.

use std::cell::RefCell;
use std::rc::Rc;

use byroredux_core::ecs::World;
use byroredux_ui::{ScaleformHostBridge, ScaleformValue, UiManager};

use crate::asset_provider::Archive;
use crate::hud::{fraction, HudBackend, HudControl, HUD_REFRESH_INTERVAL};

/// Which Scaleform game's HUD this driver serves. Selection is
/// structural: whichever vanilla interface archive sits beside `--esm`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ScaleformGame {
    /// AVM1 — `Skyrim - Interface.bsa`, `ScaleformProfile::SkyrimAvm1`.
    Skyrim,
    /// AVM2 — `Fallout4 - Interface.ba2`, `ScaleformProfile::Fallout4Avm2`,
    /// with the injected BGSCodeObj forwarding adapter.
    Fallout4,
}

/// One driven bar: console label + the global-space AVIF key the
/// fraction derives from when not pinned.
struct ScaleformBar {
    label: &'static str,
    av: u32,
}

impl ScaleformGame {
    /// The BSA/BA2 carrying the game's UI corpus.
    fn menu_archive(self) -> &'static str {
        match self {
            Self::Skyrim => "Skyrim - Interface.bsa",
            Self::Fallout4 => "Fallout4 - Interface.ba2",
        }
    }

    /// The vanilla HUD movie, archive-relative.
    fn menu_path(self) -> &'static str {
        "interface\\hudmenu.swf"
    }

    /// The driven bars, in `hud.values` order.
    fn bars(self) -> &'static [ScaleformBar] {
        const SKYRIM: &[ScaleformBar] = &[
            ScaleformBar { label: "health", av: 0x3E8 },
            ScaleformBar { label: "magicka", av: 0x3E9 },
            ScaleformBar { label: "stamina", av: 0x3EA },
        ];
        // FO4 keys per `crates/core/src/character/fallout.rs` — the same
        // global space `derive_stored_actor_values` stamps FO4 NPCs with.
        const FALLOUT4: &[ScaleformBar] = &[
            ScaleformBar { label: "health", av: 0x2C9 },
            ScaleformBar { label: "ap", av: 0x2D0 },
        ];
        match self {
            Self::Skyrim => SKYRIM,
            Self::Fallout4 => FALLOUT4,
        }
    }
}

/// The bar state the response handlers answer requests from. `Rc`
/// shared between the driver (frame loop, refreshed from the World) and
/// the handler closures (Ruffle's thread during `ExternalInterface.call`
/// — the same thread in practice, but the `RefCell` documents the
/// single-threaded bargain). Slots beyond the game's bar count stay 1.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct HudSnapshot {
    pub bars: [f32; 3],
    pub heading: f32,
}

/// Frame-loop driver for the Scaleform HUD. Owned by the `App` (set up
/// in `scene.rs`), ticked from the Ruffle overlay branch.
pub(crate) struct ScaleformHudDriver {
    game: ScaleformGame,
    /// Handle into the live player's bridge — used to register response
    /// handlers (launch) and to read the movie's registered callbacks /
    /// unanswered-method sets (per-tick diagnostics mirror).
    bridge: ScaleformHostBridge,
    /// Shared with the response handler closures.
    snapshot: Rc<RefCell<HudSnapshot>>,
    /// Callbacks the movie has registered so far (mirrored to the diag
    /// resource each refresh).
    callbacks: Vec<String>,
    last_signature: u64,
    last_push: std::time::Instant,
    last_diag_refresh: std::time::Instant,
}

/// World-resource mirror of the Scaleform HUD's bridge diagnostics —
/// `hud.debug`'s data source. The bridge handle is `Rc`-based and
/// single-threaded, so the driver copies the (small, bounded) sets out
/// on its diagnostic cadence rather than the console reaching into it.
#[derive(Debug, Clone, Default)]
pub struct ScaleformHudDiag {
    /// The game's HUD route (for `hud.debug`'s header).
    pub game: &'static str,
    /// The driven bars' labels, in `last_push` slot order.
    pub bar_labels: [&'static str; 3],
    /// How many slots are real (2 FO4, 3 Skyrim).
    pub bar_count: u8,
    /// `ExternalInterface.addCallback` names the movie registered — the
    /// legal push targets. FO4's include the injected adapter's own
    /// `__byro*` lifecycle hooks (proof of `AdapterInjected` at
    /// runtime); the push table skips that prefix.
    pub callbacks: Vec<String>,
    /// Host methods observed with no registration or response.
    pub unknown_methods: Vec<String>,
    /// Cataloged requests observed with no configured response.
    pub unanswered_methods: Vec<String>,
    /// The last computed frame's values (bars, heading) — proof the
    /// driver tick loop is alive.
    pub last_push: Option<(f32, f32, f32, f32)>,
}

impl byroredux_core::ecs::Resource for ScaleformHudDiag {}

/// Probe `--hud` for the Scaleform route: whichever vanilla interface
/// archive (Skyrim BSA, FO4 BA2) sits beside `--esm`. Returns the game
/// and its archive path.
fn hud_scaleform_args(args: &[String]) -> Result<Option<(ScaleformGame, String)>, String> {
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
    for game in [ScaleformGame::Skyrim, ScaleformGame::Fallout4] {
        let archive = esm_dir.join(game.menu_archive());
        if archive.is_file() {
            return Ok(Some((game, archive.to_string_lossy().into_owned())));
        }
    }
    // Not a Scaleform-era install — the MenuXml probe in `crate::hud`
    // reports its own candidates; stay silent here.
    Ok(None)
}

/// Launch the Scaleform HUD when `--hud` is present and the ESM
/// directory carries a vanilla interface archive.
///
/// Skips (with a log) when `ui_manager` is already live — `--menu`
/// owns the overlay in that case, and a second construction would
/// orphan its host-call draining.
pub(crate) fn launch(
    ctx: &mut byroredux_renderer::vulkan::context::VulkanContext,
    world: &mut World,
    ui_manager: &mut Option<UiManager>,
    ui_texture_handle: &mut Option<u32>,
    args: &[String],
) -> Option<ScaleformHudDriver> {
    let (game, archive_path) = match hud_scaleform_args(args) {
        Ok(Some(pair)) => pair,
        Ok(None) => return None,
        Err(error) => {
            log::error!("{error}");
            return None;
        }
    };
    if ui_manager.is_some() {
        log::info!("hud: --menu owns the overlay — Scaleform HUD route skipped");
        return None;
    }

    let (w, h) = ctx.swapchain_extent();
    let archive = match Archive::open(&archive_path) {
        Ok(archive) => archive,
        Err(e) => {
            log::error!("hud: failed to open {}: {e}", archive_path);
            return None;
        }
    };

    let menu_path = game.menu_path();
    let mut ui = UiManager::new(w, h);
    if let Err(e) =
        ui.load_swf_from_resource_provider(std::sync::Arc::new(archive), menu_path, menu_path, None)
    {
        log::error!("hud: failed to load {menu_path}: {e}");
        return None;
    }
    // The overlay texture is full-screen: with Ruffle's default opaque
    // stage the movie's background clear washes the rendered world out
    // entirely. A HUD composites over the frame, so the stage clears to
    // transparent wherever the movie drew nothing. (`--menu` keeps the
    // opaque stage — modal menus own the whole screen by design.)
    ui.set_stage_transparent();
    // A HUD is not a modal menu: world input (fly camera, console) must
    // keep working. `install_player` grabbed focus at load.
    ui.set_input_focus(false);

    let handle = {
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
    let handle = match handle {
        Ok(handle) => handle,
        Err(e) => {
            log::error!("hud: UI texture registration failed: {e}");
            return None;
        }
    };
    *ui_texture_handle = Some(handle);

    let bridge = ui.host_bridge().expect("menu loaded above");

    // Response handlers — the menu→engine channel. Registered for
    // Skyrim only: the names are the SkyUI HUD-widget poll protocol,
    // which is not in FO4's BGSCodeObj catalog (registering them on an
    // FO4 bridge would seed `known_methods` with names no FO4 menu
    // calls). No FO4 handlers are fabricated — FO4's catalog has no
    // meter-shaped queries, so `unknown/unanswered` (via `hud.debug`)
    // stays the discovery instrument there.
    let snapshot: Rc<RefCell<HudSnapshot>> = Rc::new(RefCell::new(HudSnapshot::default()));
    if game == ScaleformGame::Skyrim {
        let stats = snapshot.clone();
        bridge.set_response_handler("updateStats", move |_args| {
            let s = stats.borrow();
            vec![
                ScaleformValue::Number((s.bars[0] * 100.0) as f64),
                ScaleformValue::Number((s.bars[1] * 100.0) as f64),
                ScaleformValue::Number((s.bars[2] * 100.0) as f64),
            ]
        });
        let player_info = snapshot.clone();
        bridge.set_response_handler("RequestPlayerInfo", move |_args| {
            let s = player_info.borrow();
            vec![
                ScaleformValue::Number((s.bars[0] * 100.0) as f64),
                ScaleformValue::Number((s.bars[1] * 100.0) as f64),
                ScaleformValue::Number((s.bars[2] * 100.0) as f64),
                ScaleformValue::Number(s.heading as f64),
            ]
        });
    }

    let driver = ScaleformHudDriver {
        game,
        callbacks: bridge.available_callbacks(),
        bridge,
        snapshot,
        last_signature: 0,
        last_push: std::time::Instant::now(),
        last_diag_refresh: std::time::Instant::now(),
    };
    let (profile, state) = (ui.menu_profile(), ui.host_object_state());
    log::info!(
        "hud: loaded {menu_path} archive='{archive_path}' backend={} \
         profile={profile:?} state={state:?} texture={handle} ({w}x{h}) \
         — Scaleform vanilla HUD",
        HudBackend::Scaleform.as_str()
    );

    let bars = game.bars();
    let mut labels = ["health", "magicka", "stamina"];
    for (i, bar) in bars.iter().enumerate() {
        labels[i] = bar.label;
    }
    world.insert_resource(HudControl {
        backend: HudBackend::Scaleform,
        bar_count: bars.len() as u8,
        bar_labels: labels,
        ..HudControl::default()
    });
    world.insert_resource(ScaleformHudDiag {
        game: game.menu_archive(),
        bar_labels: labels,
        bar_count: bars.len() as u8,
        ..ScaleformHudDiag::default()
    });
    *ui_manager = Some(ui);
    Some(driver)
}

impl ScaleformHudDriver {
    /// One frame: refresh the shared snapshot from the World, mirror
    /// bridge diagnostics to the console resource, and (on change, at
    /// the shared HUD cadence) push state into the movie.
    pub(crate) fn tick(&mut self, world: &World, ui: &mut UiManager, cam_forward: [f32; 3]) {
        let Some(control) = world.try_resource::<HudControl>() else {
            return;
        };
        let control = *control;

        // Mirror the console's visibility into the player: `render()`
        // answers `UiFrame::Hidden` while this is false, which stops the
        // UI quad entirely — `hud.off` really hides a Scaleform HUD (the
        // MenuXml driver's equivalent is its own early return).
        if ui.visible != control.visible {
            ui.visible = control.visible;
        }

        // The shared snapshot the response handlers read.
        let bars = self.game.bars();
        let mut fractions = [1.0f32; 3];
        for (i, bar) in bars.iter().enumerate() {
            fractions[i] = fraction(world, Some(bar.av), control.bars[i]);
        }
        let heading = control.heading.unwrap_or_else(|| {
            f32::atan2(cam_forward[0], -cam_forward[2])
                .to_degrees()
                .rem_euclid(360.0)
        });
        *self.snapshot.borrow_mut() = HudSnapshot {
            bars: fractions,
            heading,
        };

        // Change signature + cadence — the same discipline as the
        // MenuXml driver (a static HUD must not pay per-frame pushes,
        // which each dirty the Ruffle surface and force a re-upload).
        let hash = crate::hud::hash_signature((
            fractions[0].to_bits(),
            fractions[1].to_bits(),
            fractions[2].to_bits(),
            (heading * 10.0) as i32,
            u8::from(control.visible),
        ));
        let changed = hash != self.last_signature;
        let cadence_ok = self.last_push.elapsed() >= HUD_REFRESH_INTERVAL;
        if changed && cadence_ok && control.visible {
            self.last_signature = hash;
            self.last_push = std::time::Instant::now();
            self.push(ui, fractions, heading);
            if let Some(mut diag) = world.try_resource_mut::<ScaleformHudDiag>() {
                diag.last_push = Some((fractions[0], fractions[1], fractions[2], heading));
            }
        }

        // Diagnostics mirror (1 Hz — the sets are bounded and only grow
        // on movie activity, but `hud.debug` should not read a stale
        // snapshot for long).
        if self.last_diag_refresh.elapsed() >= std::time::Duration::from_millis(1000) {
            self.last_diag_refresh = std::time::Instant::now();
            self.callbacks = self.bridge.available_callbacks();
            if let Some(mut diag) = world.try_resource_mut::<ScaleformHudDiag>() {
                diag.callbacks = self.callbacks.clone();
                diag.unknown_methods = self.bridge.unknown_methods();
                diag.unanswered_methods = self.bridge.unanswered_methods();
            }
        }
    }

    /// Push one frame of state into the movie through its registered
    /// callbacks. The vanilla HUDs register no state channels (Skyrim:
    /// the GameDelegate `call`/`respond` pair; FO4: the adapter's own
    /// `__byro*` lifecycle hooks, skipped by the prefix guard), so
    /// against vanilla this finds no targets and that is correct. The
    /// table exists for menus that register state callbacks — the
    /// discovery instrument is `hud.debug`'s callback mirror.
    fn push(&mut self, ui: &mut UiManager, fractions: [f32; 3], heading: f32) {
        for name in &self.callbacks {
            if name.starts_with("__byro") {
                continue; // the injected adapter's lifecycle hooks
            }
            let args: Vec<ScaleformValue> = match name.as_str() {
                // SkyUI-class meter pushes (shapes are the working
                // hypothesis pinned against on-screen evidence).
                "UpdateStats" | "updateStats" => vec![
                    ScaleformValue::Number((fractions[0] * 100.0) as f64),
                    ScaleformValue::Number((fractions[1] * 100.0) as f64),
                    ScaleformValue::Number((fractions[2] * 100.0) as f64),
                ],
                "UpdateCompass" | "updateCompass" => {
                    vec![ScaleformValue::Number(heading as f64)]
                }
                _ => continue,
            };
            let _ = ui.invoke_callback(name, args);
        }
    }
}
