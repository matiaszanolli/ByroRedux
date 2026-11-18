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
use byroredux_ui::path::PATH_CALLBACKS;
use byroredux_ui::{PathOp, PathOutcome, RenderPacing, ScaleformHostBridge, ScaleformValue, UiLayer, UiManager, WhenAbsent};

use crate::asset_provider::Archive;
use crate::hud::{bar_reading, publish_readings, shown_bits, BarReading, HudBackend, HudControl, HUD_REFRESH_INTERVAL};

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

    /// The path-service operations that feed the vanilla HUD this frame's vitals and heading
    /// (M48 GUI-HUD-001…003, -005): the members Phase 0 found the original engine drives
    /// (`docs/data/game-ui/hud-feeds-*.tsv`), confirmed live by driving them.
    ///
    /// - **Skyrim** (AVM1): `_root.HUDMovieBaseInstance.Set{Health,Magicka,Stamina}MeterPercent
    ///   (aPercent 0..100, abForce)` with `abForce = false`, the movie's own path: it runs the
    ///   meter's fade-in (`RunMeterAnim`) and sets the tween target (`true` sets the bar of a meter
    ///   that stays faded out). Each is followed by the meter object's own `SetPercent` (on
    ///   `HealthMeterLeft`, `MagickaMeter`, `StaminaMeter`), which floors the frame it jumps to:
    ///   the tween (`Components.Meter.Update`) calls `gotoAndStop` with a fractional frame, which
    ///   Ruffle ignores, so without it the bar would never move. Survival penalties are not
    ///   modelled, so the penalty-scaled value the handler computes equals the percent.
    ///   `SetCompassAngle(aPlayerAngle, aCompassAngle, abShowCompass)`, both angles the heading in
    ///   degrees. The movie's own `GameDelegate` `call` callback does not reach these handlers at
    ///   runtime (Phase 0 read that from bytecode; Phase 2 measured it).
    /// - **Fallout 4** (AVM2): the `MeterBarWidget.Percent` setters (0..1) of
    ///   `LeftMeters_mc.HPMeter_mc.MeterBar_mc` and
    ///   `RightMeters_mc.ActionPointMeter_mc.MeterBar_mc`, plus the HP meter's rads overlay
    ///   `RadsBar_mc`, which the movie initialises full and draws over the HP fill. The engine
    ///   models no Fallout 4 radiation (`PlayerVitals` carries FO3 / FNV rads only), so the overlay
    ///   is 0: the state of a player with no rads. The compass widget has no ActionScript at all:
    ///   [`Fallout4Compass`] builds and moves it as native code does (#7036).
    fn meter_feeds(self, fractions: [f32; 3], heading: f32) -> Vec<PathOp> {
        let number = |value: f32| ScaleformValue::Number(value as f64);
        match self {
            Self::Skyrim => {
                let mut feeds = Vec::with_capacity(7);
                for ((name, object), fraction) in
                    [("Health", "HealthMeterLeft"), ("Magicka", "MagickaMeter"), ("Stamina", "StaminaMeter")]
                        .into_iter()
                        .zip(fractions)
                {
                    let percent = number(fraction * 100.0);
                    feeds.push(PathOp::Call(
                        format!("_root.HUDMovieBaseInstance.Set{name}MeterPercent"),
                        vec![percent.clone(), ScaleformValue::Bool(false)],
                    ));
                    feeds.push(PathOp::Call(format!("_root.HUDMovieBaseInstance.{object}.SetPercent"), vec![percent]));
                }
                feeds.push(PathOp::Call(
                        "_root.HUDMovieBaseInstance.SetCompassAngle".into(),
                        vec![number(heading), number(heading), ScaleformValue::Bool(true)],
                ));
                feeds
            }
            Self::Fallout4 => vec![
                PathOp::Set("LeftMeters_mc.HPMeter_mc.MeterBar_mc.Percent".into(), number(fractions[0])),
                PathOp::Set("LeftMeters_mc.HPMeter_mc.RadsBar_mc.Percent".into(), number(0.0)),
                PathOp::Set("RightMeters_mc.ActionPointMeter_mc.MeterBar_mc.Percent".into(), number(fractions[1])),
            ],
        }
    }

    /// The operations that show or hide each driven bar's widget (#7043): hidden for a pool the player
    /// does not carry, shown again when it appears. The same widgets [`Self::meter_feeds`] drives, through
    /// the property each movie's runtime owns: `visible` (AVM2, Fallout 4) and `_visible` (AVM1, Skyrim).
    /// Skyrim's `HealthMeterLeft` is the left half of the health meter; a vanilla Skyrim player carries all
    /// three pools, so this channel only ever writes `true` there.
    fn bar_visibility(self, shown: [bool; 3]) -> Vec<PathOp> {
        let widgets: &[&str] = match self {
            Self::Skyrim => &[
                "_root.HUDMovieBaseInstance.HealthMeterLeft._visible",
                "_root.HUDMovieBaseInstance.MagickaMeter._visible",
                "_root.HUDMovieBaseInstance.StaminaMeter._visible",
            ],
            Self::Fallout4 => &["LeftMeters_mc.HPMeter_mc.visible", "RightMeters_mc.ActionPointMeter_mc.visible"],
        };
        widgets
            .iter()
            .zip(shown)
            .map(|(path, shown)| PathOp::Set((*path).to_owned(), ScaleformValue::Bool(shown)))
            .collect()
    }

    /// The operations that stand in, once, for native writes the engine does not model yet, so
    /// a HUD widget nothing feeds shows nothing rather than its authored sample text (#6665: the
    /// sample text drew once GUI-FONT-001/003 gave the movies their fonts).
    ///
    /// - **Skyrim**: `_root.HUDMovieBaseInstance.HideSubtitle()`, the native call that clears the
    ///   subtitle (`hud-feeds-skyrim-se.tsv`; `ShowSubtitle` is its pair), whose authored text is
    ///   "Dialogue Line 1 / 2".
    /// - **Fallout 4**: `visible = false` on each widget native code feeds and the engine does
    ///   not yet (`hud-feeds-fallout4.tsv`): the XP gain bar, quest updates, the rads counter, the
    ///   region text, the ammo counts, the VATS crit meter, the subtitle, the stealth meter and the
    ///   enemy health bar. This is the engine's policy for an unfed widget, not a claim about how
    ///   native hides them; each comes back when its feed lands (M48 Phase 6).
    fn unfed_widgets(self) -> Vec<PathOp> {
        match self {
            Self::Skyrim => vec![PathOp::Call("_root.HUDMovieBaseInstance.HideSubtitle".into(), Vec::new())],
            Self::Fallout4 => [
                "HUDNotificationsGroup_mc.XPMeter_mc",
                "HUDNotificationsGroup_mc.QuestUpdates_mc",
                "LeftMeters_mc.RadsMeter_mc",
                "LeftMeters_mc.LocationText_mc",
                "RightMeters_mc.AmmoCount_mc",
                "RightMeters_mc.ExplosiveAmmoCount_mc",
                "BottomCenterGroup_mc.CritMeter_mc",
                "BottomCenterGroup_mc.SubtitleText_mc",
                "TopCenterGroup_mc.StealthMeter_mc",
                "TopCenterGroup_mc.EnemyHealthMeter_mc",
            ]
            .into_iter()
            .map(|widget| PathOp::Set(format!("{widget}.visible"), ScaleformValue::Bool(false)))
            .collect(),
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

/// #7036 — Fallout 4's compass (GUI-HUD-005). `BottomCenterGroup_mc.CompassWidget_mc` has no
/// ActionScript: native `HUDCompass` builds and moves it. Measured from `Fallout4.exe` (RVAs):
///
/// - construction (`0x9b8240`) keeps `halfW = (CompassBar_mc.width − 2) · 0.5` (151 px on the
///   vanilla 304 px bar) and instantiates one `DirectionMarkerWidget` (the movie's exported
///   letter-and-tick class) per cardinal into `OtherMarkerHolder_mc`, at 0, π/2, π and 3π/2 rad;
///   each sets `Direction_tf.text` to `$Compass_North` / `East` / `South` / `West` and
///   `Direction_tf.autoSize` to `"center"` (`0x9bb800`);
/// - every update (`0x9b88d1`, half field of view `0x252e960` = 0.872665 rad = 50°) places each
///   marker through `0x9bb5d0`: `Δ = wrap(heading − angle)` into ±π; a direction marker
///   (never clamped to the edge) is hidden once `|Δ| ≥ halfFov`, otherwise shown at
///   `x = −halfW · Δ / halfFov`.
///
/// The engine stands in for that construction through the path service's attach
/// ([`PathOp::Attach`]) and feeds `x` / `visible` from the camera heading. The quest and
/// other location markers the native compass also places are not modelled. Each label is set
/// to its key, as native does, and Ruffle's `$`-key pass translates it (GUI-FONT-004). The letters
/// draw in the bundled fallback face until Fallout 4's font classes (`$MAIN_Font_Bold`) are
/// mapped from its `fontconfig.txt` (GUI-FONT-001, M48 Phase 6).
#[derive(Default)]
struct Fallout4Compass {
    /// `halfW`, read from the movie once its adapter answers.
    half_width: Option<f32>,
    /// The markers are attached and labelled.
    built: bool,
    /// The movie cannot carry the compass (no `DirectionMarkerWidget`, no holder): stop asking.
    disabled: bool,
}

const FALLOUT4_COMPASS: &str = "BottomCenterGroup_mc.CompassWidget_mc";
/// `HUDCompass`'s half field of view, `0x252e960` (0.872665 rad).
const FALLOUT4_COMPASS_HALF_FOV_DEGREES: f32 = 50.0;
/// The direction markers: the child name the engine gives each, its translation key, its angle
/// (degrees clockwise from north, as the heading).
const FALLOUT4_COMPASS_DIRECTIONS: [(&str, &str, f32); 4] = [
    ("ByroCompassNorth", "$Compass_North", 0.0),
    ("ByroCompassEast", "$Compass_East", 90.0),
    ("ByroCompassSouth", "$Compass_South", 180.0),
    ("ByroCompassWest", "$Compass_West", 270.0),
];

impl Fallout4Compass {
    fn marker(name: &str) -> String {
        format!("{FALLOUT4_COMPASS}.OtherMarkerHolder_mc.{name}")
    }

    /// The markers' placement for `heading` (degrees): `(x, visible)` per direction, by the
    /// native rule.
    fn placement(half_width: f32, heading: f32) -> [(f32, bool); 4] {
        FALLOUT4_COMPASS_DIRECTIONS.map(|(_, _, angle)| {
            let delta = (heading - angle + 180.0).rem_euclid(360.0) - 180.0;
            let visible = delta.abs() < FALLOUT4_COMPASS_HALF_FOV_DEGREES;
            (-half_width * delta / FALLOUT4_COMPASS_HALF_FOV_DEGREES, visible)
        })
    }

    /// One push: build the markers once the adapter answers, then place them. Each operation's
    /// path and outcome goes to `hud.debug`'s paths line. `false` while the compass still waits on
    /// the movie, so the driver pushes again on its cadence even with a still heading.
    fn feed(&mut self, layer: &mut UiLayer, heading: f32, feed: &mut Vec<(String, String)>) -> bool {
        if self.disabled {
            return true;
        }
        let mut run = |op: PathOp, when_absent: WhenAbsent, feed: &mut Vec<(String, String)>| {
            let path = op.path().to_owned();
            let outcome = layer.path_op(op, when_absent);
            feed.push((path, outcome_name(&outcome).to_owned()));
            outcome
        };
        if self.half_width.is_none() {
            let width = format!("{FALLOUT4_COMPASS}.CompassBar_mc.width");
            match run(PathOp::Get(width), WhenAbsent::Refuse, feed) {
                PathOutcome::Found(ScaleformValue::Number(width)) => self.half_width = Some(((width - 2.0) * 0.5) as f32),
                PathOutcome::NotReady => return false,
                _ => return self.disable("its CompassBar_mc has no width"),
            }
        }
        if !self.built {
            for (name, key, _) in FALLOUT4_COMPASS_DIRECTIONS {
                let marker = Self::marker(name);
                let text = format!("{marker}.Direction_tf");
                let steps = [
                    PathOp::Attach(marker, "DirectionMarkerWidget".to_owned()),
                    PathOp::Set(format!("{text}.autoSize"), ScaleformValue::String("center".to_owned())),
                    // Through `htmlText` with the authored `<b>` (the tag's placeholder is
                    // `<b>NSEW</b>`): Ruffle's `text` setter drops to the field's non-bold default
                    // format, which no `$MAIN_Font_Bold` face matches once fonts resolve. The key
                    // is translated by Ruffle's `$`-key pass, per HTML run.
                    PathOp::Set(format!("{text}.htmlText"), ScaleformValue::String(format!("<b>{key}</b>"))),
                ];
                for op in steps {
                    match run(op, WhenAbsent::Refuse, feed) {
                        PathOutcome::Found(_) => {}
                        PathOutcome::NotReady => return false,
                        _ => return self.disable("a DirectionMarkerWidget could not be attached and labelled"),
                    }
                }
            }
            self.built = true;
        }
        let half_width = self.half_width.unwrap_or_default();
        for ((name, _, _), (x, visible)) in FALLOUT4_COMPASS_DIRECTIONS.iter().zip(Self::placement(half_width, heading)) {
            let marker = Self::marker(name);
            run(PathOp::Set(format!("{marker}.visible"), ScaleformValue::Bool(visible)), WhenAbsent::Queue, feed);
            if visible {
                run(PathOp::Set(format!("{marker}.x"), ScaleformValue::Number(x as f64)), WhenAbsent::Queue, feed);
            }
        }
        true
    }

    fn disable(&mut self, why: &str) -> bool {
        log::warn!("hud: the Fallout 4 compass stays static: {why}");
        self.disabled = true;
        true
    }
}

fn outcome_name(outcome: &PathOutcome) -> &'static str {
    match outcome {
        PathOutcome::Found(_) => "found",
        PathOutcome::Absent => "absent",
        PathOutcome::NotReady => "not-ready",
        PathOutcome::Queued => "queued",
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
    /// The HUD movie's layer in the [`UiManager`] stack (its `menu_name`).
    menu: String,
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
    /// Fallout 4's compass (#7036); `None` on Skyrim, whose movie drives its own.
    compass: Option<Fallout4Compass>,
    /// [`ScaleformGame::unfed_widgets`] were queued.
    unfed_hidden: bool,
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
    /// The last meter / compass feed through the path service: each path and its outcome
    /// (M48 GUI-HUD-001…003), and the operations still queued for their target.
    pub path_feed: Vec<(String, String)>,
    pub queued_paths: usize,
}

impl byroredux_core::ecs::Resource for ScaleformHudDiag {}

/// Probe `--hud` for the Scaleform route: whichever vanilla interface
/// archive (Skyrim BSA, FO4 BA2) sits beside `--esm`. Returns the game
/// and its archive path.
fn hud_scaleform_args(args: &[String]) -> Result<Option<(ScaleformGame, String)>, String> {
    if !args.iter().any(|arg| arg == "--hud") {
        return Ok(None);
    }
    interface_archive(args)
}

/// The Scaleform game and its vanilla interface archive beside `--esm` (Skyrim probed before
/// Fallout 4), or `None` on a pre-Scaleform install. The HUD and `ui.open` (M48 GUI-OPS-005)
/// load their movies from it.
pub(crate) fn interface_archive(args: &[String]) -> Result<Option<(ScaleformGame, String)>, String> {
    let (_esm, esm_dir) = crate::cli_args::esm_discovery_root(args)?;
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

    let compass = (game == ScaleformGame::Fallout4).then(Fallout4Compass::default);
    let menu_path = game.menu_path();
    let mut ui = UiManager::new(w, h);
    if let Err(e) =
        ui.load_swf_from_resource_provider(std::sync::Arc::new(archive), menu_path, menu_path, None)
    {
        log::error!("hud: failed to load {menu_path}: {e}");
        return None;
    }
    let menu = UiManager::installed_menu_name(menu_path);
    // A HUD is not a modal menu: world input (fly camera, console) must
    // keep working. `install_player` grabbed focus at load; the menu
    // registry would hand it back to the world on the next frame (the HUD's
    // flags claim no input, GUI-REG-009), but not before this one.
    ui.set_input_owner(None);
    let layer = ui.layer_mut(&menu).expect("menu loaded above");
    // The overlay texture is full-screen: with Ruffle's default opaque
    // stage the movie's background clear washes the rendered world out
    // entirely. A HUD composites over the frame, so the stage clears to
    // transparent wherever the movie drew nothing. (`--menu` keeps the
    // opaque stage — modal menus own the whole screen by design.)
    layer.set_stage_transparent();
    // #4717 — the HUD is up all session, so its render + full-target readback
    // is paced instead of running at the movie's own frame rate.
    layer.set_render_pacing(HUD_RENDER_PACING);

    let handle = {
        let upload_ctx = byroredux_renderer::upload_ctx!(ctx);
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
    layer.set_texture(Some(handle));
    let bridge = layer.host_bridge();

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

    // `Some(..)`: the boot-line tokens kept their pre-stack shape (`state=Some(AdapterInjected)`
    // is m48-7's gate), from when the manager held at most one optional movie.
    let (profile, state) = (Some(layer.profile()), Some(layer.host_object_state()));
    let driver = ScaleformHudDriver {
        game,
        menu,
        callbacks: bridge.available_callbacks(),
        bridge,
        snapshot,
        last_signature: 0,
        last_push: std::time::Instant::now(),
        last_diag_refresh: std::time::Instant::now(),
        compass,
        unfed_hidden: false,
    };
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
    /// The HUD movie's layer name in the [`UiManager`] stack.
    pub(crate) fn menu(&self) -> &str {
        &self.menu
    }

    /// `layer` is the HUD movie's layer; `None` (closed) still refreshes the snapshot the
    /// response handlers read.
    pub(crate) fn tick(&mut self, world: &World, mut layer: Option<&mut UiLayer>, cam_forward: [f32; 3]) {
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
        if let Some(layer) = layer.as_deref_mut()
            && layer.visible() != control.visible {
                layer.set_visible(control.visible);
            }

        // The shared snapshot the response handlers read. #4675 — editor
        // ids resolve through the per-load `PlayerVitals` table.
        let bars = self.game.bars();
        let mut readings = [BarReading { fraction: 1.0, shown: true }; 3];
        for (i, bar) in bars.iter().enumerate() {
            readings[i] = bar_reading(world, Some(bar.av), control.bars[i]);
        }
        publish_readings(world, &readings);
        let fractions = readings.map(|r| r.fraction);
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
        let hash = crate::hud::mix_signature(hash, shown_bits(&readings));
        let changed = hash != self.last_signature;
        let cadence_ok = self.last_push.elapsed() >= HUD_REFRESH_INTERVAL;
        if changed && cadence_ok && control.visible {
            self.last_signature = hash;
            self.last_push = std::time::Instant::now();
            if let Some(layer) = layer.as_deref_mut() {
                self.push(layer, fractions, heading);
                // M48 GUI-HUD-001…003 — the vanilla meters and compass through the path service.
                // Queued: the movie's adapter registers on its first frame (AVM1) or when the
                // lifecycle class is constructed (AVM2); a newer value replaces a held one.
                let mut feed: Vec<(String, String)> = self
                    .game
                    .meter_feeds(fractions, heading)
                    .into_iter()
                    .chain(self.game.bar_visibility(readings.map(|r| r.shown)))
                    .map(|op| {
                        let path = op.path().to_owned();
                        let outcome = layer.path_op(op, WhenAbsent::Queue);
                        (path, outcome_name(&outcome).to_owned())
                    })
                    .collect();
                if !std::mem::replace(&mut self.unfed_hidden, true) {
                    // Queued: they land once the adapter answers, like the meter feeds.
                    for op in self.game.unfed_widgets() {
                        let path = op.path().to_owned();
                        let outcome = layer.path_op(op, WhenAbsent::Queue);
                        feed.push((path, outcome_name(&outcome).to_owned()));
                    }
                }
                if let Some(compass) = self.compass.as_mut()
                    && !compass.feed(layer, heading, &mut feed) {
                        // Still waiting on the movie: push again next cadence, heading or not.
                        self.last_signature = 0;
                    }
                if let Some(mut diag) = world.try_resource_mut::<ScaleformHudDiag>() {
                    diag.path_feed = feed;
                }
            }
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
                // The path adapters' own callbacks are the path service's surface, reported on
                // `hud.debug`'s paths line; this list stays the movie's push targets (and FO4's
                // adapter lifecycle hooks, m48-7's AdapterInjected observable).
                diag.callbacks = self
                    .callbacks
                    .iter()
                    .filter(|name| !PATH_CALLBACKS.contains(&name.as_str()))
                    .cloned()
                    .collect();
                diag.unknown_methods = self.bridge.unknown_methods();
                diag.unanswered_methods = self.bridge.unanswered_methods();
                diag.render_passes = layer.as_deref().map_or(0, UiLayer::render_passes);
                diag.queued_paths = layer.as_deref().map_or(0, UiLayer::queued_paths);
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
    fn push(&mut self, ui: &mut UiLayer, fractions: [f32; 3], heading: f32) {
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
    use crate::inventory::PlayerVitals;
    use byroredux_ui::ScaleformProfile;

    fn driver() -> ScaleformHudDriver {
        ScaleformHudDriver {
            game: ScaleformGame::Skyrim,
            menu: "HUD Menu".to_owned(),
            bridge: ScaleformHostBridge::new(ScaleformProfile::SkyrimAvm1),
            snapshot: Rc::new(RefCell::new(HudSnapshot::default())),
            callbacks: Vec::new(),
            last_signature: 0,
            last_push: std::time::Instant::now(),
            last_diag_refresh: std::time::Instant::now(),
            compass: None,
            unfed_hidden: false,
        }
    }

    /// #7043 — the bar-visibility channel names each driven bar's own widget, hides it for a pool the
    /// player does not carry and shows it again, one write per bar and in `hud.values` order.
    #[test]
    fn bar_visibility_writes_each_bars_widget_in_order() {
        let ops = |game: ScaleformGame, shown| {
            game.bar_visibility(shown)
                .into_iter()
                .map(|op| match op {
                    PathOp::Set(path, ScaleformValue::Bool(visible)) => (path, visible),
                    other => panic!("a bar's visibility is a boolean property write, got {other:?}"),
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            ops(ScaleformGame::Fallout4, [true, false, true]),
            [
                ("LeftMeters_mc.HPMeter_mc.visible".to_owned(), true),
                ("RightMeters_mc.ActionPointMeter_mc.visible".to_owned(), false),
            ],
            "FO4 drives two bars; the third slot is unused"
        );
        let skyrim = ops(ScaleformGame::Skyrim, [false, true, false]);
        assert_eq!(skyrim.len(), 3);
        assert_eq!(skyrim.iter().map(|(_, visible)| *visible).collect::<Vec<_>>(), [false, true, false]);
        assert!(skyrim.iter().all(|(path, _)| path.starts_with("_root.HUDMovieBaseInstance.") && path.ends_with("._visible")));
        // Every widget it addresses is one the meter feed already drives, so a bar the feed moves is the
        // bar the channel hides.
        let fed = ScaleformGame::Skyrim.meter_feeds([0.0; 3], 0.0);
        for (path, _) in &skyrim {
            let object = path.trim_end_matches("._visible");
            assert!(fed.iter().any(|op| op.path() == format!("{object}.SetPercent")), "{object} is not fed");
        }
    }

    /// #7043 — a tick publishes which bars are shown, `HudControl::shown`, for `hud.status`: a player
    /// carrying no pool the game authors reads hidden, one carrying it reads shown.
    #[test]
    fn tick_publishes_which_bars_are_shown() {
        let mut world = World::new();
        world.insert_resource(HudControl::default());
        world.insert_resource(PlayerVitals::resolving(&[("Health", 1), ("Magicka", 2)]));
        let player = world.spawn();
        let mut values = byroredux_core::ecs::components::ActorValues::new();
        values.set_base(1, 100.0);
        world.insert(player, values);
        world.insert_resource(crate::systems::PlayerEntity(Some(player)));
        driver().tick(&world, None, [0.0, 0.0, -1.0]);
        let control = world.resource::<HudControl>();
        assert_eq!(control.shown, [true, false, false], "Health carried; Magicka resolves but is not carried; Stamina is not authored");
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
        driver().tick(&world, None, [0.0, 0.0, -1.0]);

        assert_eq!(
            world.resource::<HudControl>().live[0],
            Some(1.0),
            "the driver publishes the live bar fractions each tick"
        );
    }

    /// M48 GUI-HUD-001…003, -005 — the vanilla meters are fed through the paths Phase 0 listed:
    /// Skyrim's percent setters on 0..100 with `abForce = false` (the movie's own fade-in) and
    /// its compass, Fallout 4's `Percent` setters on 0..1 with the rads overlay cleared; no path
    /// drives Fallout 4's code-less compass.
    #[test]
    fn meter_feeds_follow_the_phase0_paths() {
        let skyrim = ScaleformGame::Skyrim.meter_feeds([0.35, 1.0, 0.5], 90.0);
        assert_eq!(
            skyrim[0],
            PathOp::Call(
                "_root.HUDMovieBaseInstance.SetHealthMeterPercent".into(),
                vec![ScaleformValue::Number(35.0), ScaleformValue::Bool(false)]
            )
        );
        // The movie's handler, then the meter's own flooring `SetPercent` (Ruffle drops the tween's
        // fractional frames).
        assert_eq!(
            skyrim[1],
            PathOp::Call("_root.HUDMovieBaseInstance.HealthMeterLeft.SetPercent".into(), vec![ScaleformValue::Number(35.0)])
        );
        assert_eq!(skyrim[4].path(), "_root.HUDMovieBaseInstance.SetStaminaMeterPercent");
        assert_eq!(skyrim[5].path(), "_root.HUDMovieBaseInstance.StaminaMeter.SetPercent");
        assert_eq!(
            skyrim[6],
            PathOp::Call(
                "_root.HUDMovieBaseInstance.SetCompassAngle".into(),
                vec![ScaleformValue::Number(90.0), ScaleformValue::Number(90.0), ScaleformValue::Bool(true)]
            )
        );
        let fallout4 = ScaleformGame::Fallout4.meter_feeds([0.35, 0.8, 1.0], 90.0);
        let paths: Vec<&str> = fallout4.iter().map(PathOp::path).collect();
        assert_eq!(
            paths,
            [
                "LeftMeters_mc.HPMeter_mc.MeterBar_mc.Percent",
                "LeftMeters_mc.HPMeter_mc.RadsBar_mc.Percent",
                "RightMeters_mc.ActionPointMeter_mc.MeterBar_mc.Percent"
            ]
        );
        assert_eq!(fallout4[0], PathOp::Set(paths[0].into(), ScaleformValue::Number(0.35f32 as f64)));
        assert_eq!(fallout4[1], PathOp::Set(paths[1].into(), ScaleformValue::Number(0.0)));
        assert!(!paths.iter().any(|path| path.contains("Compass")), "Fallout 4's compass is Fallout4Compass's (#7036)");
    }

    /// #7036 — the Fallout 4 compass places its direction markers by `HUDCompass`'s rule: each at
    /// `x = −halfW · Δ / 50°` with `Δ = wrap(heading − angle)`, hidden from `|Δ| ≥ 50°` on.
    #[test]
    fn fallout4_compass_markers_follow_the_native_placement() {
        let half_width = 151.0;
        let near = |(x, visible): (f32, bool), want: f32| visible && (x - want).abs() < 1e-3;
        // Facing north: N centred, E and W out of the 100° window, S behind.
        let [n, e, s, w] = Fallout4Compass::placement(half_width, 0.0);
        assert!(near(n, 0.0), "{n:?}");
        assert!(!e.1 && !s.1 && !w.1);
        // Facing 20° (east of north): N sits left, 20/50 of the half width.
        let [n, e, ..] = Fallout4Compass::placement(half_width, 20.0);
        assert!(near(n, -half_width * 0.4), "{n:?}");
        assert!(!e.1, "E is 70° away");
        // Facing 70°: E to the left of centre by 20°, N out.
        let [n, e, ..] = Fallout4Compass::placement(half_width, 70.0);
        assert!(near(e, half_width * 0.4), "{e:?}");
        assert!(!n.1);
        // Wrap: facing 350°, N is 10° to the right; W at 280° is out (70°).
        let [n, _, _, w] = Fallout4Compass::placement(half_width, 350.0);
        assert!(near(n, half_width * 0.2), "{n:?}");
        assert!(!w.1);
        // The window edge is exclusive, as the native `comiss`/`jb` pair.
        assert!(!Fallout4Compass::placement(half_width, 50.0)[0].1);
        // hud.heading 90 vs 270: the visible marker changes, which the smoke diffs.
        assert!(Fallout4Compass::placement(half_width, 90.0)[1].1 && !Fallout4Compass::placement(half_width, 90.0)[3].1);
        assert!(Fallout4Compass::placement(half_width, 270.0)[3].1 && !Fallout4Compass::placement(half_width, 270.0)[1].1);
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
