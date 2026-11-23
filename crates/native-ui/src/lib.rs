//! The player-facing native UI: the HUD the game draws every frame and the native pages (pause,
//! settings, inventory, message box, barter, crafting, dialogue, character creation).
//!
//! M48 GUI-PAGE-006 (#6666): these used to live in the operator overlay crate
//! (`byroredux-debug-ui`) beside the F3 panels. They are drawn with egui into the same context
//! and pass (the overlay crate hosts it: input, the frame, the render handoff), but they read
//! only [`PageSnapshot`], the player's state, and hand back only [`PageOutputs`], the player's
//! actions. Every word they draw goes through the load order's `UiLabels` or is listed in the
//! native-page string inventory (GUI-PAGE-005, `page_strings_tests`).

mod chargen;
mod game_menu;
mod hud;
mod mcm;
mod rest;
mod setting_control;
mod views;
#[cfg(test)]
mod mcm_tests;
#[cfg(test)]
mod page_strings_tests;

pub use chargen::ChargenDraft;
pub use mcm::{format_slider_value, scancode as directinput_scancode, McmDraft};
pub use game_menu::{draw_game_menu, GameMenuPage, GameMenuState};
pub use hud::{draw_hud, draw_loading_tip, draw_player_message};
pub use setting_control::draw_setting_control;
pub use views::{
    BarterAction, BarterItemView, BarterSnapshot, ChargenChoice, ChargenOption, ChargenScreenView, ChargenSnapshot,
    CraftingAction, CraftingRecipeView, McmAction, McmCellKind, McmCellView, McmCustomContent, McmDialogView, McmModView,
    McmSnapshot, MCM_FLAG_DISABLED, MCM_FLAG_HIDDEN, MCM_FLAG_WITH_UNMAP, CraftingSnapshot, DialogueTopicEntryView, DialogueTopicSnapshot,
    DialogueUiAction, InteractionPrompt, InventoryAction, InventoryItemView, InventorySnapshot, MessageBoxView,
    ObjectiveView, PageOutputs, PageSnapshot, RestAction, RestAttribute, RestView, StealthIndicatorLook, StealthIndicatorView, VitalBarView,
};

/// The egui these pages draw with, so a host can name the context type.
pub use egui;

/// End the pass and drain its texture deltas: the page tests read shapes
/// and widget state, never upload, and egui 0.36 debug-asserts that a
/// dropped `TexturesDelta` was drained.
#[cfg(test)]
fn end_pass(ctx: &egui::Context) -> egui::FullOutput {
    let mut output = ctx.end_pass();
    output.textures_delta.clear();
    output
}
