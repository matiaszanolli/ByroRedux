//! Scaleform/SWF UI system — Ruffle integration for Bethesda menu rendering.
//!
//! Wraps Ruffle (Rust Flash player) to parse and execute Bethesda's Scaleform GFx
//! menu SWF files. Renders to an RGBA pixel buffer suitable for Vulkan texture upload.
//!
//! Note: UiManager is NOT an ECS Resource because Ruffle's Player is not Send+Sync.
//! It lives in the main loop alongside VulkanContext.

mod catalog;
mod host;
mod player;
mod profile;

pub use catalog::{ScaleformHostCatalog, ScaleformHostMethod, ScaleformHostMethodKind};
pub use host::{ScaleformHostBridge, ScaleformHostCall, ScaleformHostDispatch, ScaleformValue};
pub use player::SwfPlayer;
pub use profile::ScaleformProfile;

/// Global UI manager. Owns the active Ruffle player and UI state.
///
/// Managed directly by the main loop (not ECS) because Ruffle's Player
/// contains non-Send backends (video, audio).
pub struct UiManager {
    /// Active SWF player (None if no menu is loaded).
    player: Option<SwfPlayer>,
    /// Whether the UI overlay is visible.
    pub visible: bool,
    /// Name of the currently loaded menu (e.g. "startmenu").
    pub menu_name: String,
    /// Viewport dimensions for the UI overlay.
    pub width: u32,
    pub height: u32,
}

impl UiManager {
    pub fn new(width: u32, height: u32) -> Self {
        Self {
            player: None,
            visible: false,
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

    fn install_player(&mut self, player: SwfPlayer, name: &str) {
        self.player = Some(player);
        self.menu_name = name.to_string();
        self.visible = true;
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

    /// Render the current frame and return the RGBA pixel buffer if dirty.
    pub fn render(&mut self) -> Option<&[u8]> {
        if !self.visible {
            return None;
        }
        if let Some(ref mut player) = self.player {
            player.render()
        } else {
            None
        }
    }

    /// Host bridge for draining ActionScript calls and inspecting callbacks.
    pub fn host_bridge(&self) -> Option<ScaleformHostBridge> {
        self.player.as_ref().map(SwfPlayer::host_bridge)
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

    /// Close the current menu.
    pub fn close(&mut self) {
        self.player = None;
        self.visible = false;
        self.menu_name.clear();
    }
}
