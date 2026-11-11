//! Concrete debug-UI panels (Phase 4b of the debug-UI plan).
//!
//! Drawn inside the egui closure passed to `DebugUiState::run` by the
//! binary's main loop. Each panel reads from a pre-built
//! [`PanelSnapshot`] (a frozen view of the World resources the
//! panels need) and writes any actions back through
//! [`PanelOutputs`]; the binary applies those to the World after
//! `run` returns, sidestepping the borrow-checker conflict between
//! `&mut DebugUiState` and the world references the panels would
//! otherwise need.
//!
//! Split by job (#6578): `snapshots` holds the data types both sides share,
//! `hud` and `game_menu` are what the shipped game draws, and `studio` is the
//! developer overlay. The public names are re-exported here, so
//! `panels::draw_hud` and `byroredux_debug_ui::PanelSnapshot` keep their paths.

mod chargen;
mod game_menu;
mod hud;
mod snapshots;
mod studio;

pub use chargen::ChargenDraft;
pub use game_menu::{draw_game_menu, GameMenuPage, GameMenuState};
pub use hud::{draw_hud, draw_loading_tip, draw_player_message};
pub use snapshots::{
    BarterAction, BarterItemView, BarterSnapshot, ChargenChoice, ChargenOption, ChargenScreenView,
    ChargenSnapshot, CraftingAction, CraftingRecipeView, CraftingSnapshot,
    DialogueTopicEntryView, DialogueTopicSnapshot, DialogueUiAction, InteractionPrompt, InventoryAction,
    InventoryItemView, InventorySnapshot, MessageBoxView, ObjectiveView, PanelOutputs, PanelSnapshot,
    QueuedLoad, VitalBarView,
};
pub use studio::{draw, PanelTab};

/// End the pass and drain its texture deltas: the panel tests read shapes
/// and widget state, never upload, and egui 0.36 debug-asserts that a
/// dropped `TexturesDelta` was drained.
#[cfg(test)]
fn end_pass(ctx: &egui::Context) -> egui::FullOutput {
    let mut output = ctx.end_pass();
    output.textures_delta.clear();
    output
}
