//! M48 Phase 4 (#6666, PRD Q1) — the menus the engine runs on the game's own surfaces, and the
//! drivers that stand in for the native code feeding them:
//!
//! | Menu | Skyrim (S1, movie) | Fallout 3 / New Vegas (S2, document) |
//! |---|---|---|
//! | Dialogue | `scaleform_dialogue` (`main.rs`) | `menuxml_dialogue` |
//! | Script message box | `scaleform_messagebox` | `menuxml_message` |
//!
//! Each opens only when the registry's game targets that surface for the menu and it loads; the
//! native page stays the fallback (S3). A MenuXml document is a [`MenuDocument`] in
//! `UiState::menu_documents`; a movie is a `UiManager` layer whose host calls the frame's drain
//! files under its layer in `UiState::movie_calls`.

use byroredux_core::ecs::resources::menu_registry::{MenuGame, TargetSurface};
use byroredux_core::ecs::resources::MenuRegistry;

use crate::menuxml_document::MenuDocument;
use crate::App;

impl App {
    /// The open document of the menu with `code`, if any.
    pub(crate) fn menu_document_mut(&mut self, code: u32) -> Option<&mut MenuDocument> {
        self.ui.menu_documents.iter_mut().find(|document| document.code == code)
    }

    /// Open a game's menu with `code` on its document (S2) and `prepare` it; `false` when the
    /// game targets no document for it, the session has no menu archives, or it does not load.
    /// An already open document stays.
    pub(crate) fn open_menu_document(&mut self, game: MenuGame, code: u32, purpose: &str, prepare: fn(&mut MenuDocument) -> Result<(), String>) -> bool {
        if self.menu_document_mut(code).is_some() {
            return true;
        }
        let def = self.world.try_resource::<MenuRegistry>().and_then(|registry| {
            (registry.game() == game)
                .then(|| registry.table().by_code(code).cloned())
                .flatten()
                .filter(|def| def.target == TargetSurface::MenuXml)
        });
        let Some((name, document)) = def.and_then(|def| def.document.map(|document| (def.name, document))) else {
            return false;
        };
        let assets = match crate::hud::session_menu_assets(&crate::cli_args::effective_args()) {
            Ok(Some((assets, _, _))) => assets,
            Ok(None) => {
                log::warn!("{name}: the session has no menu archives beside its ESM; {purpose} opens on the native page");
                return false;
            }
            Err(error) => {
                log::warn!("{error}; {purpose} opens on the native page");
                return false;
            }
        };
        let Some(ctx) = self.renderer.as_mut() else {
            return false;
        };
        let opened = MenuDocument::open(ctx, assets, code, name.clone(), document.clone()).and_then(|mut opened| match prepare(&mut opened) {
            Ok(()) => Ok(opened),
            Err(error) => {
                opened.close(ctx);
                Err(error)
            }
        });
        match opened {
            Ok(opened) => {
                log::info!("ui.open: {name} opened as menuxml {document} — {purpose}");
                self.ui.menu_documents.push(opened);
                true
            }
            Err(error) => {
                log::warn!("{name}: {error}; {purpose} opens on the native page");
                false
            }
        }
    }

    /// Close the document of the menu with `code` and give the world its input back when it was
    /// the last surface up; `false` when none was open.
    pub(crate) fn close_menu_document(&mut self, code: u32) -> bool {
        let Some(position) = self.ui.menu_documents.iter().position(|document| document.code == code) else {
            return false;
        };
        let document = self.ui.menu_documents.remove(position);
        if let Some(ctx) = self.renderer.as_mut() {
            document.close(ctx);
        }
        if self.ui.menu_documents.is_empty() && self.ui.dialogue_movie.is_none() && self.ui.message_box_movie.is_none() {
            self.capture_world_input();
        }
        true
    }

    /// Open the conversation on the game's dialogue document (S2, Fallout 3 / New Vegas).
    pub(crate) fn open_dialogue_document(&mut self) -> bool {
        self.open_menu_document(MenuGame::Fallout3NV, crate::menuxml_dialogue::DIALOGUE_MENU_CODE, "the conversation", crate::menuxml_dialogue::prepare)
    }

    /// M48 GUI-PAGE-004 — the dialogue document's frame: apply what its input asked, then present
    /// the conversation's snapshot when it changed; a conversation that ended closes it.
    pub(crate) fn step_dialogue_document(&mut self) {
        let code = crate::menuxml_dialogue::DIALOGUE_MENU_CODE;
        let Some(document) = self.menu_document_mut(code) else {
            return;
        };
        let actions: Vec<_> = document.renderer.take_activations().iter().filter_map(crate::menuxml_dialogue::action_of).collect();
        let npc = self.world.try_resource::<crate::systems::npc_dialogue::DialogueSurfaceState>().and_then(|surface| surface.npc);
        let snapshot = crate::systems::npc_dialogue::dialogue_snapshot(&self.world);
        for action in actions {
            let call = match action {
                crate::menuxml_dialogue::DocumentAction::Skip => crate::scaleform_dialogue::DialogueCall::Skip,
                crate::menuxml_dialogue::DocumentAction::SelectTopic(index) => crate::scaleform_dialogue::DialogueCall::SelectTopic(index),
            };
            crate::scaleform_dialogue::apply(&self.world, npc, snapshot.as_ref(), call);
        }
        if npc.is_none() {
            self.close_menu_document(code);
            return;
        }
        let snapshot = crate::systems::npc_dialogue::dialogue_snapshot(&self.world);
        if let (Some(snapshot), Some(document)) = (snapshot.as_ref(), self.menu_document_mut(code)) {
            crate::menuxml_dialogue::feed(document, snapshot);
        }
    }

    /// Q1 — open the front script message box on the game's own surface: Skyrim's
    /// `interface\messagebox.swf` (S1) or Fallout 3 / New Vegas's `menus\message_menu.xml` (S2);
    /// `false` when the game has neither or it does not load (the native page takes the box).
    pub(crate) fn open_message_box_surface(&mut self) -> bool {
        if self.ui.message_box_movie.is_some() || self.menu_document_mut(crate::menuxml_message::MESSAGE_MENU_CODE).is_some() {
            return true;
        }
        self.open_message_box_movie()
            || self.open_menu_document(MenuGame::Fallout3NV, crate::menuxml_message::MESSAGE_MENU_CODE, "the message box", crate::menuxml_message::prepare)
    }

    fn open_message_box_movie(&mut self) -> bool {
        let target = self.world.try_resource::<MenuRegistry>().and_then(|registry| {
            (registry.game() == MenuGame::Skyrim)
                .then(|| registry.table().by_name("MessageBoxMenu").cloned())
                .flatten()
                .filter(|def| def.target == TargetSurface::Movie)
                .map(|def| (def, registry.game()))
        });
        let Some((def, game)) = target else {
            return false;
        };
        match self.open_movie_menu(&def.name, def.document.as_deref(), game) {
            Ok(line) => {
                log::info!("{line} — the message box's surface (Q1)");
                let layer = def.document.as_deref().map(byroredux_ui::UiManager::installed_menu_name).unwrap_or(def.name);
                if let Some(movie) = self.ui.manager.as_ref().and_then(|manager| manager.layer(&layer)) {
                    let bridge = movie.host_bridge();
                    for method in crate::scaleform_messagebox::SERVED_METHODS {
                        bridge.register_method(method);
                    }
                }
                self.ui.movie_calls.insert(layer.clone(), Vec::new());
                self.ui.message_box_movie = Some(crate::scaleform_messagebox::MessageBoxMovie::new(layer));
                true
            }
            Err(error) => {
                log::warn!("{error}; the message box opens on the native page");
                false
            }
        }
    }

    /// The message box surface's frame: answer the front box with the button the player chose,
    /// present the next one, and close the surface once none waits (answered here, by `ui.answer`
    /// or by a perk's activate choice).
    pub(crate) fn step_message_box(&mut self) {
        let code = crate::menuxml_message::MESSAGE_MENU_CODE;
        let choices: Vec<usize> = if let Some(movie) = self.ui.message_box_movie.as_ref() {
            let calls = self.ui.movie_calls.get_mut(&movie.layer).map(std::mem::take).unwrap_or_default();
            calls.iter().filter_map(crate::scaleform_messagebox::decode).collect()
        } else if let Some(document) = self.menu_document_mut(code) {
            document.renderer.take_activations().iter().filter_map(crate::menuxml_message::choice_of).collect()
        } else {
            return;
        };
        // One answer per box: a second click in the same frame landed on the box it answered.
        if let Some(choice) = choices.first() {
            crate::notifications::answer_message_box(&self.world, Some(*choice));
            if let Some(movie) = self.ui.message_box_movie.as_mut() {
                movie.answered();
            }
            if let Some(document) = self.menu_document_mut(code) {
                document.signature = None;
            }
        }
        let Some(message) = crate::notifications::front_message_box(&self.world) else {
            self.close_message_box_surface();
            return;
        };
        let labels = crate::ui_labels(&self.world);
        let ok = labels.text("Ok");
        if let Some(movie) = self.ui.message_box_movie.as_mut() {
            if let Some(layer) = self.ui.manager.as_mut().and_then(|manager| manager.layer_mut(&movie.layer)) {
                movie.feed(layer, &message, ok);
            }
        } else if let Some(document) = self.menu_document_mut(code) {
            crate::menuxml_message::feed(document, &message, ok);
        }
    }

    /// Close whichever message box surface is up.
    fn close_message_box_surface(&mut self) {
        if let Some(movie) = self.ui.message_box_movie.take() {
            self.ui.movie_calls.remove(&movie.layer);
            self.drop_movie_layer(&movie.layer);
            if self.ui.menu_documents.is_empty() && self.ui.dialogue_movie.is_none() {
                self.capture_world_input();
            }
        }
        self.close_menu_document(crate::menuxml_message::MESSAGE_MENU_CODE);
    }
}

/// The movies the engine feeds (S1): the HUD driver's, and the drivers above.
const FED_MOVIES: [&str; 3] = ["HUD Menu", "Dialogue Menu", "MessageBoxMenu"];
/// The documents the engine feeds (S2), by `MenuMode` code: the MenuXml HUD (`--hud`), the
/// dialogue document and the message box document.
const FED_DOCUMENTS: [u32; 3] = [1004, crate::menuxml_dialogue::DIALOGUE_MENU_CODE, crate::menuxml_message::MESSAGE_MENU_CODE];
/// The native pages the engine opens from a game action or the player (S3); barter needs the
/// game's barter rules (`BarterRules`), which only some games carry.
const NATIVE_PAGES: [&str; 5] = ["Inventory", "Pause", "Dialogue", "MessageBox", "Crafting"];

/// PRD §8 *Menu surface coverage*: per surface class, how many of the game's menus (the
/// registry's table, built from the Phase 0 inventory) the engine can put up, and how.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SurfaceCoverage {
    /// S1: the movie loads when the menu opens (`ui.open`, a script request).
    pub(crate) movie: usize,
    /// …of which the engine feeds (the HUD, dialogue and message box drivers).
    pub(crate) movie_fed: usize,
    /// S2: a document the engine opens and feeds.
    pub(crate) document: usize,
    /// S3: a native page stands in.
    pub(crate) native: usize,
    /// S4: no surface (the menu is recorded).
    pub(crate) absent: usize,
}

impl SurfaceCoverage {
    /// The menus with a surface (S1–S3), and every menu.
    pub(crate) fn covered(&self) -> (usize, usize) {
        let covered = self.movie + self.document + self.native;
        (covered, covered + self.absent)
    }
}

/// Measure [`SurfaceCoverage`] over `registry`'s table; `barter` says whether the game has barter
/// rules (its barter page can open).
pub(crate) fn surface_coverage(registry: &MenuRegistry, barter: bool) -> SurfaceCoverage {
    let page_opens = |surface: &TargetSurface| match surface {
        TargetSurface::Native(page) => NATIVE_PAGES.contains(&page.as_str()) || (barter && page == "Barter"),
        _ => false,
    };
    let chargen = registry.game() == MenuGame::Fallout3NV;
    let mut coverage = SurfaceCoverage::default();
    for def in registry.table().menus() {
        let code = def.code.unwrap_or(0);
        match &def.target {
            TargetSurface::Movie if def.document.is_some() => {
                coverage.movie += 1;
                coverage.movie_fed += usize::from(FED_MOVIES.contains(&def.name.as_str()));
            }
            TargetSurface::MenuXml if FED_DOCUMENTS.contains(&code)
                || (registry.game() == MenuGame::Oblivion && crate::menuxml_rest::is_rest_document(code)) => coverage.document += 1,
            target if page_opens(target) || page_opens(&def.fallback) => coverage.native += 1,
            _ if chargen && crate::chargen::ChargenScreen::MENU_CODES.contains(&code) => coverage.native += 1,
            _ => coverage.absent += 1,
        }
    }
    coverage
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The coverage ROADMAP's compatibility matrix publishes for wave 1 (Skyrim SE, New Vegas).
    /// A menu gaining or losing a surface moves these numbers: re-measure and update the matrix.
    #[test]
    fn wave_one_menu_surface_coverage_is_the_published_one() {
        let skyrim = surface_coverage(&MenuRegistry::new(MenuGame::Skyrim), true);
        assert_eq!(skyrim, SurfaceCoverage { movie: 24, movie_fed: 3, document: 0, native: 3, absent: 19 });
        assert_eq!(skyrim.covered(), (27, 46));
        // New Vegas has no barter rules: its barter page cannot open.
        let new_vegas = surface_coverage(&MenuRegistry::new(MenuGame::Fallout3NV), false);
        assert_eq!(new_vegas, SurfaceCoverage { movie: 0, movie_fed: 0, document: 3, native: 7, absent: 39 });
        assert_eq!(new_vegas.covered(), (10, 49));
    }
}
