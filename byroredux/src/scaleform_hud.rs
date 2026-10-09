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
use byroredux_ui::{RenderPacing, ScaleformHostBridge, ScaleformValue, UiManager};

use crate::asset_provider::Archive;
use crate::hud::{fraction, HudBackend, HudControl, HUD_REFRESH_INTERVAL};
use crate::inventory::PlayerVitals;

/// #4717 — how often the always-on HUD may render and read back its Ruffle
/// target. A live movie asks for a render at its own frame rate (24–30 Hz)
/// even when nothing on screen moves, each pass costing 5–8 ms on the main
/// thread at 1080p. The active cadence is the shared HUD cadence
/// ([`HUD_REFRESH_INTERVAL`]); after eight byte-identical passes with no push
/// or input the HUD is idle and passes drop to a 250 ms probe, which still
/// finds a movie that starts animating on its own. Engine policy — no source
/// documents what the original engine did.
const HUD_RENDER_PACING: RenderPacing = RenderPacing {
    active_interval: HUD_REFRESH_INTERVAL,
    idle_interval: std::time::Duration::from_millis(250),
    idle_after: 8,
};

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

/// The payload shape a [`ScaleformGame`] poll handler returns (#4725).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PollPayload {
    /// The three driven bars, in `hud.values` order.
    Bars,
    /// The bars followed by the compass heading.
    BarsAndHeading,
}

/// One driven bar: console label + the canonical AVIF **editor id** the
/// fraction derives from when not pinned, resolved per load through
/// `PlayerVitals::resolved` (#4675 — the literal FormIDs this table used
/// to carry were fallout.rs's unit-test fixture ids; FO4's 0x2C9 is
/// *Experience*, so the real bars could never move).
struct ScaleformBar {
    label: &'static str,
    av: &'static str,
}

impl ScaleformGame {
    /// The BSA/BA2 carrying the game's UI corpus.
    fn menu_archive(self) -> &'static str {
        match self {
            Self::Skyrim => "Skyrim - Interface.bsa",
            Self::Fallout4 => "Fallout4 - Interface.ba2",
        }
    }

    /// The menu→engine poll handlers this game's HUD widgets call, as
    /// (callback name, payload shape) data on the game itself (#4725 —
    /// the choice used to be an `if` inside launch). The names are the
    /// SkyUI HUD-widget poll protocol, which is not in FO4's BGSCodeObj
    /// catalog (registering them on an FO4 bridge would seed
    /// `known_methods` with names no FO4 menu calls). No FO4 handlers
    /// are fabricated — FO4's catalog has no meter-shaped queries, so
    /// `unknown/unanswered` (via `hud.debug`) stays the discovery
    /// instrument there.
    pub(crate) fn poll_handlers(self) -> &'static [(&'static str, PollPayload)] {
        match self {
            Self::Skyrim => &[
                ("updateStats", PollPayload::Bars),
                ("RequestPlayerInfo", PollPayload::BarsAndHeading),
            ],
            Self::Fallout4 => &[],
        }
    }

    /// The vanilla HUD movie, archive-relative.
    fn menu_path(self) -> &'static str {
        "interface\\hudmenu.swf"
    }

    /// The driven bars, in `hud.values` order.
    fn bars(self) -> &'static [ScaleformBar] {
        const SKYRIM: &[ScaleformBar] = &[
            ScaleformBar { label: "health", av: "Health" },
            ScaleformBar { label: "magicka", av: "Magicka" },
            ScaleformBar { label: "stamina", av: "Stamina" },
        ];
        // Canonical editor ids, resolved through PlayerVitals at frame
        // time (#4675) — see the field doc for why these are not FormIDs.
        const FALLOUT4: &[ScaleformBar] = &[
            ScaleformBar { label: "health", av: "Health" },
            ScaleformBar { label: "ap", av: "ActionPoints" },
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
    /// Ruffle render-and-readback passes run so far (#4717) — a static HUD
    /// should grow this at the idle probe rate, not the movie's frame rate.
    pub render_passes: u64,
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
    if let Some(reason) = crate::hud::menu_owned_overlay_skip(ui_manager.is_some()) {
        log::info!("{reason} (Scaleform)");
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
    // #4717 — the HUD is up all session, so its render + full-target readback
    // is paced instead of running at the movie's own frame rate.
    ui.set_render_pacing(HUD_RENDER_PACING);
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

    // Response handlers — the menu→engine channel. The per-game set is
    // [`ScaleformGame::poll_handlers`] data (#4725), not an inline `if`.
    let snapshot: Rc<RefCell<HudSnapshot>> = Rc::new(RefCell::new(HudSnapshot::default()));
    for (name, payload) in game.poll_handlers() {
        let snapshot = snapshot.clone();
        bridge.set_response_handler(*name, move |_args| {
            let s = snapshot.borrow();
            let mut values: Vec<ScaleformValue> = s
                .bars
                .iter()
                .map(|bar| ScaleformValue::Number((bar * 100.0) as f64))
                .collect();
            if matches!(payload, PollPayload::BarsAndHeading) {
                values.push(ScaleformValue::Number(s.heading as f64));
            }
            values
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
        // Copy the control out and drop its read guard here: `let control =
        // *control;` shadows the guard without releasing it, and the `live`
        // write below then takes a write lock on the same resource while
        // that read lock is held — a same-thread deadlock (a panic under the
        // debug lock tracker) on the first tick of every Scaleform HUD.
        let Some(control) = world.try_resource::<HudControl>().map(|control| *control) else {
            return;
        };

        // Mirror the console's visibility into the player: `render()`
        // answers `UiFrame::Hidden` while this is false, which stops the
        // UI quad entirely — `hud.off` really hides a Scaleform HUD (the
        // MenuXml driver's equivalent is its own early return).
        if ui.visible != control.visible {
            ui.visible = control.visible;
        }

        // The shared snapshot the response handlers read. #4675 — editor
        // ids resolve through the per-load `PlayerVitals` table.
        let vitals = world.try_resource::<PlayerVitals>();
        let bars = self.game.bars();
        let mut fractions = [1.0f32; 3];
        for (i, bar) in bars.iter().enumerate() {
            let key = vitals.as_ref().and_then(|v| v.resolved(bar.av));
            fractions[i] = fraction(world, key, control.bars[i]);
        }
        if let Some(mut control_mut) = world.try_resource_mut::<HudControl>() {
            control_mut.live = fractions.map(Some);
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
                diag.render_passes = ui.render_passes();
            }
        }
    }

    /// Push one frame of state into the movie through its registered
    /// callbacks. The vanilla HUDs register no state channels (Skyrim:
    /// the GameDelegate `call`/`respond` pair; FO4: the adapter's own
    /// `__byro*` lifecycle hooks, which the reservation table keeps out —
    /// #4719/#4725), so against vanilla this finds no targets and that is
    /// correct. The table exists for menus that register state callbacks —
    /// the discovery instrument is `hud.debug`'s callback mirror.
    ///
    /// #5278 — the loop carries no explicit reserved-name skip: the match
    /// below handles exactly four gameplay names, and
    /// `the_push_table_never_targets_a_reserved_engine_callback` pins that
    /// those four and `RESERVED_ENGINE_CALLBACKS` are disjoint, so the
    /// `_ => continue` arm already declines every reserved lifecycle hook.
    /// An arm that ever names one fails that test and the guard must come
    /// back.
    fn push(&mut self, ui: &mut UiManager, fractions: [f32; 3], heading: f32) {
        for name in &self.callbacks {
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

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_ui::ScaleformProfile;

    fn driver() -> ScaleformHudDriver {
        ScaleformHudDriver {
            game: ScaleformGame::Skyrim,
            bridge: ScaleformHostBridge::new(ScaleformProfile::SkyrimAvm1),
            snapshot: Rc::new(RefCell::new(HudSnapshot::default())),
            callbacks: Vec::new(),
            last_signature: 0,
            last_push: std::time::Instant::now(),
            last_diag_refresh: std::time::Instant::now(),
        }
    }

    /// `tick` reads `HudControl` and writes its `live` bars back in the same
    /// call. The read guard used to outlive the copy (`let control =
    /// *control;` shadows without releasing), so the write took a write lock
    /// on a resource this thread still held for reading: the debug lock
    /// tracker panicked on the first tick of every Scaleform `--hud` launch
    /// (regression from #4675).
    #[test]
    fn tick_publishes_live_bars_without_deadlocking_on_hud_control() {
        let mut world = World::new();
        world.insert_resource(HudControl::default());
        let mut ui = UiManager::new(4, 4);

        driver().tick(&world, &mut ui, [0.0, 0.0, -1.0]);

        assert_eq!(
            world.resource::<HudControl>().live[0],
            Some(1.0),
            "the driver publishes the live bar fractions each tick"
        );
    }

    /// #4725 — the per-game poll-handler set is table data on
    /// [`ScaleformGame`], not an inline `if` in launch: Skyrim registers
    /// the SkyUI HUD-widget polls, FO4 registers none (its catalog has no
    /// meter-shaped queries).
    #[test]
    fn poll_handlers_are_per_game_table_data() {
        let skyrim: Vec<_> = ScaleformGame::Skyrim
            .poll_handlers()
            .iter()
            .map(|(name, payload)| (*name, *payload))
            .collect();
        assert_eq!(
            skyrim,
            vec![
                ("updateStats", PollPayload::Bars),
                ("RequestPlayerInfo", PollPayload::BarsAndHeading),
            ],
            "the SkyUI poll protocol is the whole Skyrim table"
        );
        assert!(
            ScaleformGame::Fallout4.poll_handlers().is_empty(),
            "no FO4 handlers are fabricated"
        );
    }


    /// #4724 — Scaleform archive discovery: `--hud` without `--esm` is an
    /// error, a directory carrying a vanilla interface archive selects its
    /// game (Skyrim probed before FO4), and a non-Scaleform install stays
    /// silent (`Ok(None)`) so the MenuXml probe reports its own candidates.
    #[test]
    fn hud_scaleform_args_discovers_by_interface_archive() {
        let dir = tempfile::TempDir::new().unwrap();
        let esm = dir.path().join("Skyrim.esm");
        std::fs::write(&esm, b"").unwrap();

        // No --hud flag: silent.
        assert_eq!(
            hud_scaleform_args(&["--esm".into(), esm.to_str().unwrap().into()]).unwrap(),
            None
        );
        // --hud without --esm: the archive root is unresolvable.
        assert!(hud_scaleform_args(&["--hud".into()]).is_err());
        // No interface archive beside the ESM: silent (MenuXml probe owns it).
        assert_eq!(
            hud_scaleform_args(&["--hud".into(), "--esm".into(), esm.to_str().unwrap().into()])
                .unwrap(),
            None
        );

        // Skyrim's archive wins the probe order.
        std::fs::write(dir.path().join("Skyrim - Interface.bsa"), b"").unwrap();
        let (game, archive) = hud_scaleform_args(
            &["--hud".into(), "--esm".into(), esm.to_str().unwrap().into()],
        )
        .unwrap()
        .unwrap();
        assert_eq!(game, ScaleformGame::Skyrim);
        assert!(archive.ends_with("Skyrim - Interface.bsa"));

        // An FO4 install (no Skyrim archive) selects Fallout4.
        std::fs::remove_file(dir.path().join("Skyrim - Interface.bsa")).unwrap();
        std::fs::write(dir.path().join("Fallout4 - Interface.ba2"), b"").unwrap();
        let (game, _) = hud_scaleform_args(
            &["--hud".into(), "--esm".into(), esm.to_str().unwrap().into()],
        )
        .unwrap()
        .unwrap();
        assert_eq!(game, ScaleformGame::Fallout4);
    }

    /// #4724/#5278 — no reserved engine callback can ever receive a push.
    /// The table handles exactly the four gameplay names below, and this
    /// asserts those and `RESERVED_ENGINE_CALLBACKS` are DISJOINT — the
    /// property that lets `push`'s `_ => continue` arm decline every
    /// reserved lifecycle hook with no explicit guard (the pre-#5278 skip
    /// could never fire and the pre-#5278 test asserted nothing: it pushed
    /// into a playerless `UiManager` where invoke is a no-op, then checked
    /// the reservation table against itself). A future arm naming a
    /// reserved hook fails here, and the explicit guard must come back.
    #[test]
    fn the_push_table_never_targets_a_reserved_engine_callback() {
        let handled = ["UpdateStats", "updateStats", "UpdateCompass", "updateCompass"];
        // The arm set must stay exactly `handled` — pin the production
        // table's arms so extending it without re-reading this test is
        // caught by the production scan below, then check the disjointness.
        let production = include_str!("scaleform_hud.rs");
        let push = production
            .split("fn push(&mut self")
            .nth(1)
            .expect("push must stay in this file")
            .split("\n    }")
            .next()
            .expect("push body terminates");
        for name in handled {
            assert!(
                push.contains(&format!("\"{name}\"")),
                "the handled-name list above must match push's arms ({name} missing)"
            );
        }
        for reserved in byroredux_ui::RESERVED_ENGINE_CALLBACKS {
            assert!(
                !handled.contains(&reserved),
                "{reserved}: a reserved lifecycle hook must never become a push target"
            );
        }
    }
}
