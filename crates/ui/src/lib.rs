//! Scaleform/SWF UI system — Ruffle integration for Bethesda menu rendering.
//!
//! Wraps Ruffle (Rust Flash player) to parse and execute Bethesda's Scaleform GFx
//! menu SWF files. Renders to an RGBA pixel buffer suitable for Vulkan texture upload.
//!
//! Note: UiManager is NOT an ECS Resource because Ruffle's Player is not Send+Sync.
//! It lives in the main loop alongside VulkanContext.

pub mod avm1_host;
mod avm1_path;
mod avm2_host;
mod catalog;
mod host;
mod input;
mod navigator;
mod pacing;
pub mod path;
mod player;
mod prepare;
mod profile;

use std::sync::Arc;

pub use avm2_host::ScaleformHostObjectState;
pub use catalog::{
    ScaleformHostCatalog, ScaleformHostMethod, ScaleformHostMethodKind, ScaleformHostObject,
};
pub use host::{
    ScaleformHostBridge, ScaleformHostCall, ScaleformHostDispatch, ScaleformValue,
    MAX_DISTINCT_HOST_METHOD_NAMES, MAX_QUEUED_CALLS, RESERVED_ENGINE_CALLBACKS,
};
pub use input::{
    UiImeEvent, UiInputEvent, UiKeyDescriptor, UiKeyLocation, UiLogicalKey, UiMouseButton,
    UiMouseWheelDelta, UiNamedKey, UiPhysicalKey, UiTextControlCode,
};
pub use navigator::{ScaleformResourceLoad, ScaleformResourceProvider};
pub use pacing::RenderPacing;
pub use path::{PathOp, PathOutcome, WhenAbsent};
pub use player::SwfPlayer;
pub use profile::ScaleformProfile;
/// What [`UiManager::render`] wants the compositor to do this frame.
///
/// #2972 — three states, because `Option` only has two and the overlay has
/// three distinct outcomes. `Unchanged` and `Hidden` both used to be `None`,
/// so hiding the overlay kept compositing its last uploaded frame.
#[derive(Debug)]
pub enum UiFrame<'a> {
    /// Freshly rendered pixels; upload them and composite.
    Fresh(&'a [u8]),
    /// Nothing changed — keep compositing the previously uploaded texture.
    Unchanged,
    /// The overlay is hidden or has no live player — stop compositing.
    Hidden,
}

/// One open movie: a Ruffle player, the menu name it reports, the path it was loaded from, and the
/// compositor texture the engine uploads it to.
pub struct UiLayer {
    player: SwfPlayer,
    /// Name of the menu (e.g. `MessageBoxMenu`), canonicalized by
    /// [`UiManager::installed_menu_name`].
    menu_name: String,
    /// What the menu was loaded as: the archive path or the loose file.
    path: String,
    /// Whether the layer is drawn and ticked (`hud.off` hides a HUD without closing it).
    visible: bool,
    /// The engine's texture handle for this layer's frames, if it registered one.
    texture: Option<u32>,
    /// Unique per installed player: a menu replaced under the same name gets a new id, so per-layer
    /// latches (the host-call drop counter, #3772) can tell the two apart.
    id: u64,
    /// Path operations held for a retry, and the absent paths already reported (GUI-PATH-003).
    paths: path::PathQueue,
}

impl UiLayer {
    pub fn menu_name(&self) -> &str {
        &self.menu_name
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn visible(&self) -> bool {
        self.visible
    }

    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    pub fn texture(&self) -> Option<u32> {
        self.texture
    }

    pub fn set_texture(&mut self, texture: Option<u32>) {
        self.texture = texture;
    }

    /// The menu's resolved Scaleform profile (#3771).
    pub fn profile(&self) -> ScaleformProfile {
        self.player.profile()
    }

    pub fn dimensions(&self) -> (u32, u32) {
        self.player.dimensions()
    }

    /// Host bridge for draining ActionScript calls and inspecting callbacks.
    pub fn host_bridge(&self) -> ScaleformHostBridge {
        self.player.host_bridge()
    }

    /// The menu's native-object adapter state (#3427).
    pub fn host_object_state(&self) -> ScaleformHostObjectState {
        self.player.host_object_state()
    }

    /// Take the ActionScript→engine calls recorded since the previous drain (#2714).
    pub fn drain_host_calls(&self) -> Vec<ScaleformHostCall> {
        self.player.host_bridge().drain_calls()
    }

    /// Calls this menu's bridge evicted under [`MAX_QUEUED_CALLS`] since it was loaded (#2969);
    /// read beside [`Self::drain_host_calls`].
    pub fn dropped_host_calls(&self) -> u64 {
        self.player.host_bridge().dropped_calls()
    }

    /// Get, set or invoke a member by its dotted path from the movie root (M48 GUI-PATH-001):
    /// the path service, the only engine route to a movie member by path (GUI-PATH-005).
    /// `when_absent` decides, per call, what happens when the target is not there yet
    /// (GUI-PATH-003): [`WhenAbsent::Queue`] holds the operation and retries it after every tick
    /// until it lands ([`PathOutcome::Queued`]); [`WhenAbsent::Refuse`] answers
    /// [`PathOutcome::Absent`] (reported once per path, GUI-PATH-002) or
    /// [`PathOutcome::NotReady`] now.
    pub fn path_op(&mut self, op: PathOp, when_absent: WhenAbsent) -> PathOutcome {
        let outcome = self.player.path_op(&op);
        match (outcome, when_absent) {
            (PathOutcome::Absent | PathOutcome::NotReady, WhenAbsent::Queue) => {
                self.paths.hold(op);
                PathOutcome::Queued
            }
            (PathOutcome::Absent, WhenAbsent::Refuse) => {
                self.paths.report_absent(&self.menu_name, &op);
                PathOutcome::Absent
            }
            (outcome, _) => outcome,
        }
    }

    /// Path operations waiting for their target.
    pub fn queued_paths(&self) -> usize {
        self.paths.len()
    }

    /// Retry the held path operations; the ones whose target is still missing stay held.
    fn retry_queued_paths(&mut self) {
        for op in self.paths.take() {
            if !matches!(self.player.path_op(&op), PathOutcome::Found(_)) {
                self.paths.hold(op);
            }
        }
    }

    /// Invoke a callback registered by the menu's ActionScript.
    pub fn invoke_callback(
        &mut self,
        name: &str,
        arguments: impl IntoIterator<Item = ScaleformValue>,
    ) -> Option<ScaleformValue> {
        self.player.invoke_callback(name, arguments)
    }

    /// Clear the stage to transparent (see [`SwfPlayer::set_stage_transparent`]) — the HUD route
    /// opts in so the world shows through wherever the movie drew nothing.
    pub fn set_stage_transparent(&mut self) {
        self.player.set_stage_transparent();
    }

    /// Pace the render-and-readback (see [`RenderPacing`], #4717).
    pub fn set_render_pacing(&mut self, pacing: RenderPacing) {
        self.player.set_render_pacing(pacing);
    }

    /// Render-and-readback passes run so far; `hud.debug` mirrors it.
    pub fn render_passes(&self) -> u64 {
        self.player.render_passes()
    }

    /// Render the current frame and report what the compositor should do with this layer.
    ///
    /// #2972 — three states, so "hidden" and "unchanged" never share one answer: a hidden layer
    /// stops compositing instead of keeping its last upload over the world.
    pub fn render(&mut self) -> UiFrame<'_> {
        if !self.visible {
            return UiFrame::Hidden;
        }
        match self.player.render() {
            Some(pixels) => UiFrame::Fresh(pixels),
            None => UiFrame::Unchanged,
        }
    }
}

/// The open Scaleform movies, bottom to top (M48 GUI-REG-001, design Q3).
///
/// Every visible layer ticks and renders to its own overlay texture, composited in stack order;
/// only the input owner receives `UiInputEvent`. Which layer owns input is the menu registry's
/// decision (GUI-REG-009): the engine hands it over with [`Self::set_input_owner`] every frame.
/// Managed directly by the main loop (not ECS) because Ruffle's Player contains non-Send backends
/// (video, audio).
pub struct UiManager {
    layers: Vec<UiLayer>,
    /// The `menu_name` of the layer that owns keyboard and pointer input.
    input_owner: Option<String>,
    /// Viewport dimensions every layer renders at; [`Self::resize`] follows the swapchain
    /// (GUI-REG-008).
    pub width: u32,
    pub height: u32,
    next_id: u64,
}

impl UiManager {
    pub fn new(width: u32, height: u32) -> Self {
        Self { layers: Vec::new(), input_owner: None, width, height, next_id: 0 }
    }

    /// Load a SWF file and create a Ruffle player for it.
    pub fn load_swf(&mut self, swf_data: &[u8], name: &str) -> anyhow::Result<()> {
        let player = SwfPlayer::new(swf_data, self.width, self.height)?;
        self.install_player(player, name);
        Ok(())
    }

    /// Load a SWF using an explicit Bethesda Scaleform profile.
    pub fn load_swf_with_profile(
        &mut self,
        swf_data: &[u8],
        name: &str,
        profile: ScaleformProfile,
    ) -> anyhow::Result<()> {
        let player = SwfPlayer::new_with_profile(swf_data, self.width, self.height, profile)?;
        self.install_player(player, name);
        Ok(())
    }

    /// Load a menu and its relative `ImportAssets` dependencies from one
    /// Gamebryo archive resource provider.
    /// #3771 — `profile` is `Option`; see `SwfPlayer::from_resource_provider`'s
    /// doc for when a caller would legitimately pass `Some(..)` (an
    /// independent cross-check source) versus `None` (trust the single
    /// detect `prepare_movie` already performs). The resolved profile is
    /// always readable afterward via [`UiLayer::profile`], whichever way
    /// it was reached.
    pub fn load_swf_from_resource_provider(
        &mut self,
        provider: Arc<dyn ScaleformResourceProvider>,
        movie_path: &str,
        name: &str,
        profile: Option<ScaleformProfile>,
    ) -> anyhow::Result<()> {
        let player = SwfPlayer::from_resource_provider(
            provider,
            movie_path,
            self.width,
            self.height,
            profile,
        )?;
        self.install_player(player, name);
        Ok(())
    }

    /// The `menu_name` a menu loaded as `name` reports. Both load routes
    /// hand `install_player` an on-disk movie path (`interface\hudmenu.swf`),
    /// while `UI.IsMenuOpen` compares real Bethesda menu names — the path's
    /// stem canonicalizes so the two meet (#4757); anything unrecognized
    /// passes through unchanged.
    pub fn installed_menu_name(name: &str) -> String {
        canonical_menu_name(name).unwrap_or_else(|| name.to_string())
    }

    /// Put `player` on top as `name`. A menu already open under that name is replaced in place
    /// (its texture handle carries over). The new layer takes input; the engine's next registry
    /// sync confirms or moves it.
    fn install_player(&mut self, player: SwfPlayer, name: &str) {
        let menu_name = Self::installed_menu_name(name);
        self.next_id += 1;
        let layer = UiLayer {
            player,
            menu_name: menu_name.clone(),
            path: name.to_owned(),
            visible: true,
            texture: None,
            id: self.next_id,
            paths: path::PathQueue::default(),
        };
        self.set_input_owner(None);
        match self.layers.iter().position(|open| open.menu_name == menu_name) {
            Some(at) => {
                let texture = self.layers[at].texture;
                self.layers[at] = UiLayer { texture, ..layer };
            }
            None => self.layers.push(layer),
        }
        self.set_input_owner(Some(&menu_name));
        log::info!(
            "Loaded SWF menu '{}' ({}x{}, {} layer(s))",
            name,
            self.width,
            self.height,
            self.layers.len()
        );
    }

    /// Close the menu `name` (GUI-REG-007): its player is dropped, and if it owned input nothing
    /// does until the engine hands input to the next owner — in the same frame, from the registry.
    /// The closed layer comes back so the caller can release its texture.
    pub fn close(&mut self, name: &str) -> Option<UiLayer> {
        let at = self.layers.iter().position(|open| open.menu_name.eq_ignore_ascii_case(name))?;
        if self.input_owner.as_deref().is_some_and(|owner| owner.eq_ignore_ascii_case(name)) {
            self.set_input_owner(None);
        }
        Some(self.layers.remove(at))
    }

    /// The open movies, bottom to top.
    pub fn layers(&self) -> &[UiLayer] {
        &self.layers
    }

    pub fn layers_mut(&mut self) -> &mut [UiLayer] {
        &mut self.layers
    }

    pub fn layer(&self, name: &str) -> Option<&UiLayer> {
        self.layers.iter().find(|open| open.menu_name.eq_ignore_ascii_case(name))
    }

    pub fn layer_mut(&mut self, name: &str) -> Option<&mut UiLayer> {
        self.layers.iter_mut().find(|open| open.menu_name.eq_ignore_ascii_case(name))
    }

    pub fn is_empty(&self) -> bool {
        self.layers.is_empty()
    }

    /// Advance every visible movie by dt seconds.
    pub fn tick(&mut self, dt: f64) {
        for layer in self.layers.iter_mut().filter(|layer| layer.visible) {
            layer.player.tick(dt);
            layer.retry_queued_paths();
        }
    }

    /// Re-render every movie at a new viewport extent (GUI-REG-008, design § *Resize*): the
    /// engine calls this after it recreates the swapchain, and the next upload of each layer
    /// replaces its texture at the new size. `false` when the extent did not change.
    pub fn resize(&mut self, width: u32, height: u32) -> bool {
        if (width, height) == (self.width, self.height) || width == 0 || height == 0 {
            return false;
        }
        self.width = width;
        self.height = height;
        for layer in &mut self.layers {
            layer.player.resize(width, height);
        }
        true
    }

    /// The wgpu devices the open movies render through (GUI-REG-005): one per process once any
    /// movie was created (#2733), never one per movie.
    pub fn device_count() -> u32 {
        player::shared_device_count()
    }

    /// The `menu_name` of the movie that owns input, if a movie does.
    pub fn input_owner(&self) -> Option<&str> {
        self.input_owner.as_deref()
    }

    /// Whether a visible movie currently owns keyboard and pointer input.
    pub fn has_input_focus(&self) -> bool {
        self.input_owner.as_deref().and_then(|owner| self.layer(owner)).is_some_and(|layer| layer.visible)
    }

    /// Hand keyboard and pointer input to the movie `owner` (`None`: to no movie). Each change is
    /// forwarded to Ruffle exactly once (`FocusLost` to the old owner, `FocusGained` to the new);
    /// an owner that is not an open, visible movie owns nothing. Returns whether it changed.
    pub fn set_input_owner(&mut self, owner: Option<&str>) -> bool {
        let owner = owner
            .and_then(|name| self.layer(name))
            .filter(|layer| layer.visible)
            .map(|layer| layer.menu_name.clone());
        if owner == self.input_owner {
            return false;
        }
        if let Some(old) = self.input_owner.take() {
            if let Some(layer) = self.layer_mut(&old) {
                layer.player.handle_input(UiInputEvent::FocusLost);
            }
        }
        if let Some(new) = owner.as_deref() {
            if let Some(layer) = self.layer_mut(new) {
                layer.player.handle_input(UiInputEvent::FocusGained);
            }
        }
        self.input_owner = owner;
        true
    }

    /// Dispatch an input event to the movie that owns input.
    ///
    /// `true` means the menu captured the event. Capture is based on focus,
    /// not on whether an individual ActionScript listener returned handled:
    /// modal menus must not leak unhandled keys into world controls.
    pub fn handle_input(&mut self, event: UiInputEvent) -> bool {
        if !self.has_input_focus() {
            return false;
        }
        let owner = self.input_owner.clone().unwrap_or_default();
        match self.layer_mut(&owner) {
            Some(layer) => {
                layer.player.handle_input(event);
                true
            }
            None => false,
        }
    }

    /// Update Ruffle's native-pointer stage state for the movie that owns input.
    pub fn set_mouse_in_stage(&mut self, is_in_stage: bool) -> bool {
        if !self.has_input_focus() {
            return false;
        }
        let owner = self.input_owner.clone().unwrap_or_default();
        match self.layer_mut(&owner) {
            Some(layer) => {
                layer.player.set_mouse_in_stage(is_in_stage);
                true
            }
            None => false,
        }
    }
}

/// Convert known Bethesda movie filenames to the menu identifiers exposed to
/// Papyrus. This boundary is shared by archive-loaded and developer-loaded SWFs.
fn canonical_menu_name(name: &str) -> Option<String> {
    let basename = name.rsplit(['/', '\\']).next().unwrap_or(name);
    let stem = basename.rsplit_once('.').map_or(basename, |(stem, _)| stem);
    let canonical = match stem.to_ascii_lowercase().as_str() {
        "hudmenu" => "HUDMenu",
        "inventorymenu" => "InventoryMenu",
        "magicmenu" => "MagicMenu",
        "statsmenu" => "StatsMenu",
        "mapmenu" => "MapMenu",
        "questjournal" => "Journal Menu",
        "messageboxmenu" => "MessageBoxMenu",
        "loadingmenu" => "LoadingMenu",
        "mainmenu" => "MainMenu",
        "pausemenu" => "PauseMenu",
        "containermenu" => "ContainerMenu",
        "bartermenu" => "BarterMenu",
        "favoritesmenu" => "FavoritesMenu",
        "dialoguemenu" => "Dialogue Menu",
        "crafting menu" => "Crafting Menu",
        _ => return None,
    };
    Some(canonical.to_string())
}

#[cfg(test)]
mod menu_name_tests {
    use super::{canonical_menu_name, UiManager};

    #[test]
    fn archive_and_dev_paths_resolve_to_bethesda_menu_names() {
        assert_eq!(
            canonical_menu_name("Interface\\HUDMenu.SWF").as_deref(),
            Some("HUDMenu")
        );
        assert_eq!(
            canonical_menu_name("inventorymenu.swf").as_deref(),
            Some("InventoryMenu")
        );
        assert_eq!(canonical_menu_name("modmenu.swf"), None);
        // The public boundary both load routes go through reports the same
        // canonical name (#4757); unknown stems pass through untouched.
        assert_eq!(UiManager::installed_menu_name("interface\\hudmenu.swf"), "HUDMenu");
        assert_eq!(
            UiManager::installed_menu_name("Data\\Interface\\modmenu.swf"),
            "Data\\Interface\\modmenu.swf"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #2714 — the engine's per-frame drain walks the open layers, so an
    /// empty manager is simply no layers: nothing to drain, nothing to
    /// composite, no input owner.
    /// M48 GUI-PATH-003 — a path operation issued before its target exists, with
    /// `WhenAbsent::Queue`, is held (the adapter registers on frame 1, which has not run yet)
    /// and lands after the next tick; with `WhenAbsent::Refuse` the same call answers now.
    #[test]
    fn a_queued_path_lands_once_the_movie_runs_its_first_frame() {
        if !crate::player::vulkan_adapter_available() {
            eprintln!("skipped: no Vulkan adapter (headless CI) — #4595");
            return;
        }
        let mut manager = UiManager::new(64, 64);
        manager.load_swf(&crate::avm1_path::test_movie::movie(), "test.swf").unwrap();
        let layer = manager.layer_mut("test.swf").unwrap();
        let set = PathOp::Set("_root.meter.value".into(), ScaleformValue::Number(31.0));
        assert_eq!(layer.path_op(set.clone(), WhenAbsent::Refuse), PathOutcome::NotReady);
        assert_eq!(layer.path_op(set, WhenAbsent::Queue), PathOutcome::Queued);
        assert_eq!(layer.queued_paths(), 1);
        manager.tick(1.0 / 30.0);
        let layer = manager.layer_mut("test.swf").unwrap();
        assert_eq!(layer.queued_paths(), 0, "the held set landed after the tick");
        assert_eq!(
            layer.path_op(PathOp::Get("_root.meter.value".into()), WhenAbsent::Refuse),
            PathOutcome::Found(ScaleformValue::Number(31.0))
        );
        assert_eq!(layer.path_op(PathOp::Get("_root.nope".into()), WhenAbsent::Refuse), PathOutcome::Absent);
    }

    #[test]
    fn an_empty_manager_has_nothing_to_drain_or_focus() {
        let mut manager = UiManager::new(1280, 720);
        assert!(manager.is_empty() && manager.layers().is_empty());
        assert!(!manager.has_input_focus());
        // An owner that is not an open movie owns nothing.
        assert!(!manager.set_input_owner(Some("InventoryMenu")));
        assert_eq!(manager.input_owner(), None);
        assert!(manager.close("InventoryMenu").is_none());
        assert!(!manager.handle_input(UiInputEvent::FocusGained));
        assert!(manager.resize(1920, 1080) && (manager.width, manager.height) == (1920, 1080));
        assert!(!manager.resize(1920, 1080), "same extent: nothing to do");
        assert!(!manager.resize(0, 1080), "an empty extent is refused");
    }

    /// #2969 — `drain_calls`' doc says a batch "should be read together with
    /// `dropped_calls` — the batch may not be contiguous", and for a year the
    /// one live consumer read only the batch: a workspace grep for
    /// `dropped_calls` outside this crate returned nothing. Pinned the same
    /// way the `UiFrame::Hidden` contract below is, because the failure is
    /// silent — a hole in the record costs nothing observable until the drain
    /// starts routing calls into quest / inventory / player state, at which
    /// point it is a lost state transition with no signal.
    #[test]
    fn the_frame_driver_reads_the_drop_counter_beside_the_drain() {
        const APP_FRAME: &str = include_str!("../../../byroredux/src/app_frame.rs");
        assert!(
            APP_FRAME.contains("ui.drain_host_calls()"),
            "the engine's per-frame Scaleform drain moved or was removed"
        );
        assert!(
            APP_FRAME.contains("ui.dropped_host_calls()"),
            "the per-frame drain stopped reading the eviction counter, so a \
             non-contiguous batch is silent again (#2969)"
        );
    }

    /// #2972 SIBLING — the frame driver is the only consumer of
    /// `UiManager::render`, and the whole fix depends on it treating the three
    /// states differently. A future edit that collapses `Hidden` back into the
    /// reuse arm would restore the bug silently, because nothing observable
    /// changes until a menu-stack policy first sets `visible = false`.
    #[test]
    fn the_frame_driver_stops_compositing_on_hidden() {
        const APP_FRAME: &str = include_str!("../../../byroredux/src/app_frame.rs");
        assert!(
            APP_FRAME.contains("UiFrame::Hidden => {}"),
            "app_frame.rs no longer has an explicit no-composite arm for \
             UiFrame::Hidden (#2972)"
        );
        assert!(
            APP_FRAME.contains("UiFrame::Unchanged => {"),
            "app_frame.rs no longer distinguishes Unchanged from Hidden (#2972)"
        );
        // The pre-fix shape must not come back.
        assert!(
            !APP_FRAME.contains("} else if self.ui_texture_handle.is_some() {"),
            "app_frame.rs is back to reusing the last texture on a bare `None` \
             — that is the conflation #2972 removed"
        );
    }
}

#[cfg(test)]
mod ui_doc_signature_pin {
    //! #UI-D1-2026-09-09-02 — pin `docs/engine/ui.md`'s documented `profile`
    //! parameters against the real ones.
    //!
    //! The doc carried `profile: ScaleformProfile` on both resource-provider
    //! constructors for the whole window after #3771 changed them to
    //! `Option<ScaleformProfile>`. That is not a cosmetic type slip: the
    //! `Option` *is* #3771's contract, and a caller written against the
    //! documented non-`Option` form has to source a profile from somewhere.
    //! The only source available is a pre-extract of the archive entry plus an
    //! independent `ScaleformProfile::detect` — precisely the second archive
    //! extraction and third whole-stream inflate #3771 deleted. The stale doc
    //! did not merely describe the old shape, it instructed the regression.
    //!
    //! Pinned per-parameter rather than by whole-signature diff: the doc
    //! blocks are hand-maintained prose approximations (elided lifetimes,
    //! trailing comments), so a strict diff would fail on formatting and get
    //! deleted. The `profile` type is the part that carries meaning.
    //!
    //! `load_swf_with_profile` is included deliberately as the negative
    //! control: it legitimately takes a plain `ScaleformProfile`, so a
    //! well-meaning sweep that "fixes" every occurrence to `Option` fails here.

    const DOC: &str = include_str!("../../../docs/engine/ui.md");
    const LIB_RS: &str = include_str!("lib.rs");
    const PLAYER_RS: &str = include_str!("player.rs");

    /// The `profile:` parameter line from the first `pub fn <name>(` in `src`,
    /// whitespace-normalized and stripped of any trailing `//` comment.
    fn profile_param(src: &str, name: &str) -> Option<String> {
        let after = src.split_once(&format!("pub fn {name}("))?.1;
        let body = after.split_once(')')?.0;
        let line = body
            .lines()
            .find(|line| line.trim_start().starts_with("profile:"))?;
        let line = line.split("//").next().unwrap_or(line);
        Some(line.split_whitespace().collect::<Vec<_>>().join(" "))
    }

    /// The return type of every `pub fn <name>(` in `src`, whitespace-normalized
    /// and stripped of any trailing `//` comment.
    fn return_types(src: &str, name: &str) -> Vec<String> {
        src.split(&format!("pub fn {name}("))
            .skip(1)
            .filter_map(|after| {
                let tail = after.split_once(')')?.1;
                let end = tail.find([';', '{'])?;
                let ret = tail[..end].split("//").next()?.trim();
                Some(ret.split_whitespace().collect::<Vec<_>>().join(" "))
            })
            .collect()
    }

    /// #7003 — the doc's `SwfPlayer` block said `invoke_callback` returns
    /// `bool`, contradicting its own `UiManager` block and losing the
    /// `None` (unknown / unregistered) vs `Some(Null)` (registered, returned
    /// null) distinction the Papyrus bridge and menu stack rely on (#5732).
    #[test]
    fn documented_invoke_callback_returns_match_the_real_ones() {
        let expected = "-> Option<ScaleformValue>";
        let documented = return_types(DOC, "invoke_callback");
        assert_eq!(documented.len(), 2, "SwfPlayer and UiManager blocks");
        for (what, returns) in [
            ("docs/engine/ui.md", documented),
            ("player.rs", return_types(PLAYER_RS, "invoke_callback")),
            ("lib.rs", return_types(LIB_RS, "invoke_callback")),
        ] {
            assert!(!returns.is_empty(), "{what} no longer has invoke_callback");
            for ret in returns {
                assert_eq!(ret, expected, "{what} documents invoke_callback as `{ret}`");
            }
        }
    }

    #[test]
    fn documented_profile_parameters_match_the_real_signatures() {
        for (name, code) in [
            ("from_resource_provider", PLAYER_RS),
            ("load_swf_from_resource_provider", LIB_RS),
            // Negative control — must stay non-Option on both sides.
            ("load_swf_with_profile", LIB_RS),
        ] {
            let documented = profile_param(DOC, name)
                .unwrap_or_else(|| panic!("docs/engine/ui.md no longer documents `{name}`"));
            let actual = profile_param(code, name)
                .unwrap_or_else(|| panic!("`{name}` no longer takes a `profile` parameter"));
            assert_eq!(
                documented, actual,
                "docs/engine/ui.md documents `{name}` as taking `{documented}` but it \
                 takes `{actual}`. If this is the Option/non-Option distinction, note \
                 that `None` means \"let prepare_movie detect\" — a caller following \
                 the wrong form re-adds the pre-extract-and-detect step #3771 removed."
            );
        }
    }
}
