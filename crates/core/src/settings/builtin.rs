//! The settings the engine always registers.
//!
//! Lives in `core` rather than in the debug-UI crate that used to own it, so
//! the launcher can build the identical registry without linking
//! `egui-ash-renderer` — and therefore Vulkan. That is the whole mechanism
//! behind "one model, two skins" (`docs/engine/launcher.md` §4): the launcher
//! and the in-game menu render the same [`SettingEntry`] values and submit the
//! same [`SettingChange`](super::SettingChange) back, differing only in widgets.
//!
//! Nothing here may reference a renderer, window, or UI type. The values are
//! plain data; the ID constants are the stable contract between whoever
//! registers a setting and whoever applies it.

use super::{SettingChoice, SettingEntry, SettingsError, SettingsRegistry};

/// Stable registry key for the overlay's own scale control. Other engine
/// modules can register settings beside it without depending on this crate.
pub const OVERLAY_SCALE_SETTING_ID: &str = "interface.overlay_scale";

/// Stable keys for the player-facing HUD and camera settings.
pub const SHOW_CROSSHAIR_SETTING_ID: &str = "interface.show_crosshair";
pub const SHOW_PROMPTS_SETTING_ID: &str = "interface.show_prompts";
pub const FOV_SETTING_ID: &str = "gameplay.field_of_view";

/// Stable registry key for the temporal reconstruction path. The value is the
/// same spec string the `r.upscaler` console command and the `--upscaler` /
/// `--fsr-quality` CLI pair accept, so all three routes share one grammar and
/// the binary parses it in exactly one place.
pub const UPSCALER_SETTING_ID: &str = "render.upscaler";

/// Register settings owned by the overlay itself. The binary calls this while
/// assembling the universal [`SettingsRegistry`]; renderer, audio, input, and
/// gameplay modules can add their own entries through the same API over time.
pub fn register_builtin_settings(registry: &mut SettingsRegistry) -> Result<(), SettingsError> {
    registry.register(SettingEntry::slider(
        FOV_SETTING_ID,
        "Gameplay",
        "Field of view",
        "Vertical camera field of view. Applies immediately without rebuilding the renderer.",
        // Match `Camera::default()` so merely loading the settings registry
        // does not change the established view or benchmark framing.
        45.0,
        45.0,
        110.0,
        1.0,
        "°",
    ))?;
    registry.register(SettingEntry::slider(
        OVERLAY_SCALE_SETTING_ID,
        "Interface",
        "UI scale",
        "Scale the HUD, pause menu, settings, and developer overlay.",
        1.0,
        0.75,
        2.0,
        0.05,
        "×",
    ))?;
    registry.register(SettingEntry::toggle(
        SHOW_CROSSHAIR_SETTING_ID,
        "Interface",
        "Show crosshair",
        "Keep a small reticle at screen center while controlling the world.",
        true,
    ))?;
    registry.register(SettingEntry::toggle(
        SHOW_PROMPTS_SETTING_ID,
        "Interface",
        "Show interaction prompts",
        "Show the active key and action when an object can be used.",
        true,
    ))?;
    registry.register(SettingEntry::choice(
        UPSCALER_SETTING_ID,
        "Rendering",
        "Upscaler",
        "Temporal reconstruction path. FSR renders the scene below output \
         resolution and reconstructs it; TAA renders at native resolution. \
         Switching rebuilds every render-resolution target and resets \
         temporal history.",
        // Must match `UpscalerMode::default()` and the CLI's no-flag default;
        // `App::new` overwrites this from the parsed config at startup anyway,
        // but a divergent literal here would show the wrong entry for the one
        // frame before that happens.
        "fsr3/quality",
        vec![
            SettingChoice::new("taa", "TAA (native resolution)"),
            SettingChoice::new("fsr3/native-aa", "FSR 3.1 — Native AA"),
            SettingChoice::new("fsr3/quality", "FSR 3.1 — Quality"),
            SettingChoice::new("fsr3/balanced", "FSR 3.1 — Balanced"),
            SettingChoice::new("fsr3/performance", "FSR 3.1 — Performance"),
        ],
    ))
}
