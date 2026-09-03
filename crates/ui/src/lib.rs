//! Scaleform/SWF UI system — Ruffle integration for Bethesda menu rendering.
//!
//! Wraps Ruffle (Rust Flash player) to parse and execute Bethesda's Scaleform GFx
//! menu SWF files. Renders to an RGBA pixel buffer suitable for Vulkan texture upload.
//!
//! Note: UiManager is NOT an ECS Resource because Ruffle's Player is not Send+Sync.
//! It lives in the main loop alongside VulkanContext.

mod avm2_host;
mod catalog;
mod host;
mod input;
mod navigator;
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
    MAX_DISTINCT_HOST_METHOD_NAMES, MAX_QUEUED_CALLS,
};
pub use input::{
    UiImeEvent, UiInputEvent, UiKeyDescriptor, UiKeyLocation, UiLogicalKey, UiMouseButton,
    UiMouseWheelDelta, UiNamedKey, UiPhysicalKey, UiTextControlCode,
};
pub use navigator::{ScaleformResourceLoad, ScaleformResourceProvider};
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

/// Global UI manager. Owns the active Ruffle player and UI state.
///
/// Managed directly by the main loop (not ECS) because Ruffle's Player
/// contains non-Send backends (video, audio).
pub struct UiManager {
    /// Active SWF player (None if no menu is loaded).
    player: Option<SwfPlayer>,
    /// Whether the UI overlay is visible.
    pub visible: bool,
    /// Whether the active menu owns keyboard and pointer input.
    input_focused: bool,
    /// Name of the currently loaded menu (e.g. "startmenu").
    pub menu_name: String,
    /// Viewport dimensions for the UI overlay.
    ///
    /// #2723 (SAFEUI-07) — fixed at [`Self::new`] time, not live-updated on
    /// a window resize. Nothing currently drives Ruffle's
    /// `set_viewport_dimensions` or re-registers the UI texture when the
    /// swapchain is recreated, so the overlay stretches to the new aspect
    /// ratio instead of re-rendering at it (visual only — see
    /// `docs/engine/ui.md`'s "overlay viewport is fixed at load time" note
    /// for why this cannot over-read the texture upload).
    pub width: u32,
    pub height: u32,
}

impl UiManager {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            player: None,
            visible: false,
            input_focused: false,
            menu_name: String::new(),
            width,
            height,
        }
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
    /// always readable afterward via [`Self::menu_profile`], whichever way
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

    /// The currently-loaded menu's resolved Scaleform profile, or `None`
    /// when no menu is loaded. #3771.
    pub fn menu_profile(&self) -> Option<ScaleformProfile> {
        self.player.as_ref().map(SwfPlayer::profile)
    }

    fn install_player(&mut self, player: SwfPlayer, name: &str) {
        self.set_input_focus(false);
        self.player = Some(player);
        self.menu_name = name.to_string();
        self.visible = true;
        self.set_input_focus(true);
        log::info!(
            "Loaded SWF menu '{}' ({}x{})",
            name,
            self.width,
            self.height
        );
    }

    /// Advance the Ruffle player by dt seconds.
    pub fn tick(&mut self, dt: f64) {
        if let Some(ref mut player) = self.player {
            if self.visible {
                player.tick(dt);
            }
        }
    }

    /// Render the current frame and report what the compositor should do.
    ///
    /// #2972 — this returned `Option<&[u8]>`, and since #2719 the frame driver
    /// reads `None` as "keep showing the texture you already have". That
    /// conflated two opposite meanings: *nothing changed, reuse the last
    /// upload* and *the overlay is hidden, stop drawing*. Setting
    /// `visible = false` therefore froze Ruffle but left the last uploaded
    /// frame composited over the world forever.
    ///
    /// The three-state return makes the ambiguity unrepresentable rather than
    /// relying on the caller to remember to check `visible` as a second field.
    pub fn render(&mut self) -> UiFrame<'_> {
        if !self.visible {
            return UiFrame::Hidden;
        }
        let Some(player) = self.player.as_mut() else {
            // No live player: there is nothing to composite, and a stale
            // texture from a closed menu must not survive.
            return UiFrame::Hidden;
        };
        match player.render() {
            Some(pixels) => UiFrame::Fresh(pixels),
            None => UiFrame::Unchanged,
        }
    }

    /// Host bridge for draining ActionScript calls and inspecting callbacks.
    pub fn host_bridge(&self) -> Option<ScaleformHostBridge> {
        self.player.as_ref().map(SwfPlayer::host_bridge)
    }

    /// The active menu's native-object adapter state (#3427).
    ///
    /// [`SwfPlayer::host_object_state`] had no caller outside `crates/ui` —
    /// an AVM2 movie that reaches [`ScaleformHostObjectState::NotPresent`]
    /// logged identically to one that injected cleanly, since there was no
    /// accessor through which the engine could tell them apart. `None` when
    /// no menu is loaded.
    pub fn host_object_state(&self) -> Option<ScaleformHostObjectState> {
        self.player.as_ref().map(SwfPlayer::host_object_state)
    }

    /// Take the ActionScript→engine calls recorded since the previous drain.
    ///
    /// The main loop calls this once per frame beside [`Self::tick`]. Keeping
    /// the accessor here rather than making every caller go through
    /// [`Self::host_bridge`] is what makes "the engine consumes what the menu
    /// asked for" a single, greppable call site (#2714) — the bridge was
    /// designed drain-based and had no non-test consumer at all.
    ///
    /// Returns an empty vector when no menu is loaded.
    pub fn drain_host_calls(&self) -> Vec<ScaleformHostCall> {
        self.player
            .as_ref()
            .map(|player| player.host_bridge().drain_calls())
            .unwrap_or_default()
    }

    /// Calls the active menu's bridge evicted under [`MAX_QUEUED_CALLS`] since
    /// that menu was loaded.
    ///
    /// #2969 — `drain_calls`' contract says a full batch "should be read
    /// together with `dropped_calls` — the batch may not be contiguous", and
    /// nothing outside this crate read it. The bridge warns once at the
    /// producer when it first evicts, but that says a call was lost, not that
    /// *the batch the engine is holding* has a hole in it. Exposed beside
    /// [`Self::drain_host_calls`] so the consumer can honour the contract
    /// through the same handle rather than reaching for
    /// [`Self::host_bridge`].
    ///
    /// Counts from the active menu's bridge, so loading a new menu resets it —
    /// the caller latches, and must treat a decrease as a menu change rather
    /// than as calls being un-dropped.
    pub fn dropped_host_calls(&self) -> u64 {
        self.player
            .as_ref()
            .map(|player| player.host_bridge().dropped_calls())
            .unwrap_or(0)
    }

    /// Invoke a callback registered by the active ActionScript movie.
    pub fn invoke_callback(
        &mut self,
        name: &str,
        arguments: impl IntoIterator<Item = ScaleformValue>,
    ) -> Option<ScaleformValue> {
        self.player
            .as_mut()
            .and_then(|player| player.invoke_callback(name, arguments))
    }

    /// Whether the visible menu currently owns keyboard and pointer input.
    pub fn has_input_focus(&self) -> bool {
        self.input_focused && self.visible && self.player.is_some()
    }

    /// Transfer keyboard and pointer focus to or from the active menu.
    ///
    /// Focus cannot be granted without a visible player. A transition is
    /// forwarded to Ruffle exactly once and the return value reports whether
    /// the effective focus state changed.
    pub fn set_input_focus(&mut self, focused: bool) -> bool {
        let focused = focused && self.visible && self.player.is_some();
        if self.input_focused == focused {
            return false;
        }

        self.input_focused = focused;
        if let Some(player) = self.player.as_mut() {
            let event = if focused {
                UiInputEvent::FocusGained
            } else {
                UiInputEvent::FocusLost
            };
            player.handle_input(event);
        }
        true
    }

    /// Dispatch an input event to the focused menu.
    ///
    /// `true` means the menu captured the event. Capture is based on focus,
    /// not on whether an individual ActionScript listener returned handled:
    /// modal menus must not leak unhandled keys into world controls.
    pub fn handle_input(&mut self, event: UiInputEvent) -> bool {
        if !self.has_input_focus() {
            return false;
        }
        if let Some(player) = self.player.as_mut() {
            player.handle_input(event);
            true
        } else {
            false
        }
    }

    /// Update Ruffle's native-pointer stage state for the visible menu.
    pub fn set_mouse_in_stage(&mut self, is_in_stage: bool) -> bool {
        if !self.visible {
            return false;
        }
        if let Some(player) = self.player.as_mut() {
            player.set_mouse_in_stage(is_in_stage);
            true
        } else {
            false
        }
    }

    // #2723 (SAFEUI-07) — a `close()` unloading the active menu
    // (`set_input_focus(false)` + `player = None` + `visible = false` +
    // `menu_name.clear()`) used to live here. It had zero callers: nothing
    // in the engine currently drives an interactive menu-dismissal flow
    // (no Escape-closes-menu handler, no menu-switch trigger) — see
    // `docs/engine/ui.md`'s "current manager still owns one active menu...
    // a real menu stack must define which visible layer receives focus"
    // note. Deleted rather than left as unreachable API surface; trivial
    // to re-add once that policy exists and needs it.
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #2714 — the engine's per-frame drain runs whether or not a menu is
    /// loaded, so the no-player case has to be a cheap empty answer rather
    /// than a panic or an `Option` the caller has to unwrap every frame.
    #[test]
    fn draining_host_calls_without_a_menu_is_empty() {
        let manager = UiManager::new(1280, 720);
        assert!(manager.host_bridge().is_none());
        assert!(manager.drain_host_calls().is_empty());
        // #2969 — the drop counter is read on the same frames, so it needs
        // the same cheap no-player answer. Zero, not a panic: "no menu" and
        // "no menu has lost anything" are the same statement.
        assert_eq!(manager.dropped_host_calls(), 0);
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

    /// #2972 — `render()` returned `Option<&[u8]>`, and since #2719 the frame
    /// driver reads `None` as "keep showing the texture you already have". So
    /// `visible = false` froze Ruffle but kept the last uploaded frame
    /// composited over the world forever: "unchanged" and "hidden" shared one
    /// `None`. Three states make that unrepresentable.
    #[test]
    fn hiding_the_overlay_is_distinguishable_from_an_unchanged_frame() {
        let mut manager = UiManager::new(1280, 720);

        // No player: nothing to composite. Must be Hidden, not Unchanged —
        // otherwise a stale texture from a closed menu would survive.
        assert!(matches!(manager.render(), UiFrame::Hidden));

        // And explicitly hidden is Hidden regardless of player state.
        manager.visible = false;
        assert!(matches!(manager.render(), UiFrame::Hidden));
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
