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

    /// Open the FO3 / FNV menu with `code` on its document (S2) and `prepare` it; `false` when the
    /// game targets no document for it, the session has no menu archives, or it does not load.
    /// An already open document stays.
    pub(crate) fn open_menu_document(&mut self, code: u32, purpose: &str, prepare: fn(&mut MenuDocument) -> Result<(), String>) -> bool {
        if self.menu_document_mut(code).is_some() {
            return true;
        }
        let def = self.world.try_resource::<MenuRegistry>().and_then(|registry| {
            (registry.game() == MenuGame::Fallout3NV)
                .then(|| registry.table().by_code(code).cloned())
                .flatten()
                .filter(|def| def.target == TargetSurface::MenuXml)
        });
        let Some((name, document)) = def.and_then(|def| def.document.map(|document| (def.name, document))) else {
            return false;
        };
        let assets = match crate::hud::session_menu_assets(&crate::cli_args::effective_args()) {
            Ok(Some((assets, _, _))) => assets,
            Ok(None) => return false,
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
                log::info!("ui.open: {name} opened as menuxml {document} — {purpose} (GUI-PAGE-004, Q1)");
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
        self.open_menu_document(crate::menuxml_dialogue::DIALOGUE_MENU_CODE, "the conversation", crate::menuxml_dialogue::prepare)
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
            || self.open_menu_document(crate::menuxml_message::MESSAGE_MENU_CODE, "the message box", crate::menuxml_message::prepare)
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
