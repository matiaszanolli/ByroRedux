//! M48 Phase 1 (#6653) — the binary side of the menu registry
//! ([`byroredux_core::ecs::resources::MenuRegistry`], design in `docs/engine/ui.md` § *Menu
//! registry, stack and path access*).
//!
//! The surfaces report what they have up and the registry is republished from them once a frame,
//! after the frame's modal pages are presented ([`App::publish_menu_registry`]):
//!
//! - the native pages, under the game-facing name of the menu each stands in for (GUI-PAGE-001),
//!   plus the loading cover, the operator overlay (the console) and the HUD layer;
//! - every open Scaleform movie of the [`UiManager`] stack, named by the Phase 0 inventory's
//!   movie join;
//! - the menus `ui.open` opened with no surface (S4).
//!
//! Every engine-side "is a menu open" answer then reads the registry: the pause
//! ([`simulation_paused`], GUI-REG-010), the movie that receives input (GUI-REG-009), ObScript
//! `MenuMode` and `Begin MenuMode`, Papyrus `Utility.IsInMenuMode` and the extender
//! `UI.IsMenuOpen`. `NOT_SAVED_BY_DESIGN` (GUI-EVT-005): presentation state.

use byroredux_core::ecs::resource::Resource;
use byroredux_core::ecs::resources::menu_registry::{
    AbsentFallback, MenuFlags, MenuGame, MenuRegistry, MenuSurface, NativeMenu, TargetSurface,
};
use byroredux_core::ecs::World;
use byroredux_debug_ui::GameMenuPage;
use byroredux_plugin::esm::reader::GameKind;
use byroredux_ui::UiManager;

use crate::App;

/// The registry table of a load order's game. Fallout 76 and Starfield have no Phase 0
/// inventory (Starfield is out of M48, PRD §9.1); they read the Fallout 4 table, the `IMenu`
/// family both descend from, until Phase 6 inventories them.
pub(crate) fn menu_game(game: GameKind) -> MenuGame {
    match game {
        GameKind::Oblivion => MenuGame::Oblivion,
        GameKind::Fallout3NV => MenuGame::Fallout3NV,
        GameKind::Skyrim => MenuGame::Skyrim,
        GameKind::Fallout4 | GameKind::Fallout76 | GameKind::Starfield => MenuGame::Fallout4,
    }
}

/// Select the load order's menu tables (#6620's per-game menu codes, now the whole registry).
pub(crate) fn install_menu_registry(world: &mut World, game: GameKind) {
    let game = menu_game(game);
    if let Some(mut registry) = world.try_resource_mut::<MenuRegistry>() {
        registry.set_game(game);
        return;
    }
    world.insert_resource(MenuRegistry::new(game));
}

/// What the engine's own layers have up this frame.
#[derive(Debug, Default, Clone)]
pub(crate) struct NativeMenuState {
    /// The native page open, if any.
    pub page: Option<GameMenuPage>,
    /// The character-creation screen's menu code, while that page is up (`chargen.rs`).
    pub chargen_code: Option<u32>,
    /// A script's message box is waiting to be shown.
    pub message_pending: bool,
    /// A barter session is open.
    pub barter_open: bool,
    /// The operator overlay (F3).
    pub overlay: bool,
    /// The loading cover.
    pub loading: bool,
    /// The MenuXml HUD's document, when the original HUD document is up; else the native HUD.
    pub hud_document: Option<String>,
}

fn native(page: NativeMenu) -> MenuSurface {
    MenuSurface::Native { page: page.page().to_owned() }
}

/// The menus the engine's own layers stand in for, by game-facing name (GUI-PAGE-001).
pub(crate) fn native_menus(registry: &MenuRegistry, state: &NativeMenuState) -> Vec<(String, MenuSurface)> {
    let name = |menu: NativeMenu| registry.native_name(menu).to_owned();
    let mut open = Vec::new();
    // The HUD first: it goes under every modal (GUI-REG-003). A movie HUD reports itself with
    // the movie layers, which replaces this surface.
    let hud_surface = match &state.hud_document {
        Some(path) => MenuSurface::MenuXml { path: path.clone() },
        None => native(NativeMenu::Hud),
    };
    open.push((name(NativeMenu::Hud), hud_surface));
    if state.loading {
        open.push((name(NativeMenu::Loading), native(NativeMenu::Loading)));
    }
    let page = match state.page {
        Some(GameMenuPage::Pause) => Some(NativeMenu::Pause),
        Some(GameMenuPage::Settings) => Some(NativeMenu::Settings),
        Some(GameMenuPage::Inventory) => Some(NativeMenu::Inventory),
        Some(GameMenuPage::Dialogue) => Some(NativeMenu::Dialogue),
        Some(GameMenuPage::MessageBox) => Some(NativeMenu::MessageBox),
        Some(GameMenuPage::Barter) => Some(NativeMenu::Barter),
        Some(GameMenuPage::Crafting) => Some(NativeMenu::Crafting),
        Some(GameMenuPage::CharGen) => Some(NativeMenu::CharGen),
        None => None,
    };
    match page {
        // The character-creation page is the screen's own menu (`RaceSexMenu` 1036,
        // `TextEditMenu` 1051, …) on a game with codes.
        Some(NativeMenu::CharGen) => {
            let named = state.chargen_code.and_then(|code| registry.name_for_code(code));
            open.push((named.map_or_else(|| name(NativeMenu::CharGen), str::to_owned), native(NativeMenu::CharGen)));
        }
        Some(page) => open.push((name(page), native(page))),
        None => {}
    }
    // A barter session open without its page, and a message box waiting with no other page up
    // (it opens next): both are open menus to a script asking. A box waiting behind another page
    // is not — registering it would pause the world under a dialogue page that must finish first.
    if state.barter_open && page != Some(NativeMenu::Barter) {
        open.push((name(NativeMenu::Barter), native(NativeMenu::Barter)));
    }
    if state.message_pending && page.is_none() {
        open.push((name(NativeMenu::MessageBox), native(NativeMenu::MessageBox)));
    }
    if state.overlay {
        open.push((name(NativeMenu::Console), native(NativeMenu::Console)));
    }
    open
}

/// The registry name of an open movie: the Phase 0 inventory's menu for its movie file
/// (`interface\hudmenu.swf` is Skyrim's `HUD Menu`), else the name it was loaded under.
pub(crate) fn movie_menu_name(registry: &MenuRegistry, layer: &byroredux_ui::UiLayer) -> String {
    registry
        .table()
        .by_document(layer.path())
        .map_or_else(|| layer.menu_name().to_owned(), |def| def.name.clone())
}

/// The open movies, bottom to top.
pub(crate) fn movie_menus(registry: &MenuRegistry, manager: Option<&UiManager>) -> Vec<(String, MenuSurface)> {
    manager
        .map(|manager| {
            manager
                .layers()
                .iter()
                .map(|layer| {
                    let surface = MenuSurface::Movie { path: layer.path().to_owned(), profile: format!("{:?}", layer.profile()) };
                    (movie_menu_name(registry, layer), surface)
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Whether the simulation is paused this frame (GUI-REG-010): an open registry menu pauses the
/// game, whatever its surface. The loading cover is the exception in mechanism, not in answer: it
/// holds the game its own way (the scheduler runs at a held clock, which the load's own steps
/// need), both while it is up and on the frame it drops, before the registry republishes it gone.
pub(crate) fn simulation_paused(world: &World, cover_holds_game: bool) -> bool {
    if cover_holds_game {
        return false;
    }
    let cover = native(NativeMenu::Loading);
    world.try_resource::<MenuRegistry>().is_some_and(|registry| {
        registry
            .entries()
            .iter()
            .any(|open| open.flags.contains(MenuFlags::PAUSES_GAME) && open.surface != cover)
    })
}

/// The script work a paused frame still runs (GUI-EVT-006): the original runs `Begin MenuMode`
/// blocks exactly while a menu holds the game, and the result scripts of the stages they set
/// run with them, as a `SetStage` runs its stage's script at once in the original.
pub(crate) fn run_menu_mode_frame(world: &World, dt: f32) {
    byroredux_scripting::obscript_quests::obscript_quest_menu_mode_system(world, dt);
    byroredux_scripting::obscript_objects::obscript_object_menu_mode_system(world, dt);
    byroredux_scripting::obscript_results::obscript_stage_result_system(world, dt);
}

/// How `ui.close` closes a menu (GUI-OPS-005).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuResult {
    Accept,
    Cancel,
}

/// A `ui.open` / `ui.close` waiting for the frame to apply it: the movies and native pages are
/// main-thread state the console cannot reach.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MenuRequest {
    Open(String),
    Close(String, MenuResult),
}

/// The console's menu requests, and the menus `ui.open` opened with no surface (S4, PRD §3.0).
/// `NOT_SAVED_BY_DESIGN`: operator requests and presentation state.
#[derive(Debug, Default)]
pub(crate) struct MenuRequests {
    pub queue: Vec<MenuRequest>,
    pub absent: Vec<String>,
}

impl Resource for MenuRequests {}

/// Queue a request; the frame applies it before the registry publishes.
pub(crate) fn request(world: &World, request: MenuRequest) {
    if let Some(mut requests) = world.try_resource_mut::<MenuRequests>() {
        requests.queue.push(request);
    }
}

impl App {
    /// The frame's menu step, after the modal pages and before the UI renders: follow the
    /// swapchain extent, apply the console's requests, republish the registry.
    pub(crate) fn step_menus(&mut self) {
        self.sync_ui_extent();
        self.apply_menu_requests();
        self.publish_menu_registry();
    }

    /// GUI-REG-008 — every open movie and the MenuXml HUD follow the swapchain extent: after a
    /// resize they re-render at the new size instead of stretching (design § *Resize*). Read off
    /// the swapchain each frame, so every path that recreates it (a window resize, an out-of-date
    /// present) is covered by one check.
    fn sync_ui_extent(&mut self) {
        let Some(ctx) = self.renderer.as_mut() else { return };
        let (width, height) = ctx.swapchain_extent();
        if let Some(manager) = self.ui.manager.as_mut() {
            if manager.resize(width, height) {
                log::info!("ui: {} movie(s) resized to {width}x{height}", manager.layers().len());
            }
        }
        if let Some(hud) = self.ui.hud.as_mut() {
            if hud.resize(ctx, width, height) {
                log::info!("hud: resized to {width}x{height}");
            }
        }
    }

    /// Republish the registry from the surfaces (once a frame, after the modal pages), then hand
    /// input to the movie the registry names, if a movie owns it (GUI-REG-009). A closed or
    /// replaced menu's input therefore reaches the next owner in the frame it closed
    /// (GUI-REG-007).
    pub(crate) fn publish_menu_registry(&mut self) {
        let ui = self.debug_ui.as_ref();
        let state = NativeMenuState {
            page: ui.and_then(byroredux_debug_ui::DebugUiState::open_game_menu_page),
            chargen_code: ui
                .is_some_and(byroredux_debug_ui::DebugUiState::chargen_menu_visible)
                .then(|| crate::chargen::open_menu_code(&self.world))
                .flatten(),
            message_pending: self
                .world
                .try_resource::<crate::notifications::MessageBoxQueue>()
                .is_some_and(|boxes| !boxes.0.is_empty()),
            barter_open: self.world.try_resource::<crate::barter::ActiveBarter>().is_some_and(|barter| barter.is_open()),
            overlay: ui.is_some_and(|ui| ui.visible),
            loading: self.loading_screen.active(),
            hud_document: self.ui.hud.as_ref().map(|hud| hud.document_path().to_owned()),
        };
        let absent: Vec<String> = self.world.try_resource::<MenuRequests>().map(|r| r.absent.clone()).unwrap_or_default();
        let owner = {
            let Some(mut registry) = self.world.try_resource_mut::<MenuRegistry>() else { return };
            let mut open = native_menus(&registry, &state);
            open.extend(movie_menus(&registry, self.ui.manager.as_ref()));
            open.extend(absent.into_iter().map(|name| (name, MenuSurface::Absent { fallback: AbsentFallback::Recorded })));
            registry.publish(open);
            let movies = self.ui.manager.as_ref().is_some_and(|manager| !manager.is_empty());
            registry.set_movie_devices(if movies { UiManager::device_count() } else { 0 });
            registry.input_owner().and_then(|owner| match &owner.surface {
                MenuSurface::Movie { path, .. } => Some(path.clone()),
                _ => None,
            })
        };
        if let Some(manager) = self.ui.manager.as_mut() {
            let layer = owner.and_then(|path| manager.layers().iter().find(|layer| layer.path() == path).map(|layer| layer.menu_name().to_owned()));
            if manager.set_input_owner(layer.as_deref()) && manager.has_input_focus() {
                self.release_world_input_for_ui();
            }
        }
    }

    /// The Inventory action pressed through the gameplay-input path (`input.press inventory`, an
    /// injected pulse the scheduler turned into an `ActionState` edge) opens the inventory page as
    /// the Tab key's window event does, under the same rule: no page up and no movie owning input.
    /// A real key press never reaches `ActionState` here — the window path consumes it.
    pub(crate) fn open_inventory_on_injected_press(&mut self) {
        let pressed = self
            .world
            .try_resource::<crate::interaction::ActionState>()
            .is_some_and(|actions| actions.was_pressed(crate::interaction::InputAction::Inventory));
        let page_up = self.debug_ui.as_ref().is_some_and(byroredux_debug_ui::DebugUiState::game_menu_visible);
        let movie_owns_input = self.ui.manager.as_ref().is_some_and(UiManager::has_input_focus);
        if pressed && !page_up && !movie_owns_input {
            self.open_inventory_menu();
        }
    }

    /// Apply the console's `ui.open` / `ui.close` requests (GUI-OPS-005), before this frame's
    /// registry publish so the next `ui.menus` shows the change.
    pub(crate) fn apply_menu_requests(&mut self) {
        let queue = match self.world.try_resource_mut::<MenuRequests>() {
            Some(mut requests) => std::mem::take(&mut requests.queue),
            None => return,
        };
        for request in queue {
            let outcome = match request {
                MenuRequest::Open(name) => self.open_menu(&name),
                MenuRequest::Close(name, result) => self.close_menu(&name, result),
            };
            match outcome {
                Ok(line) => log::info!("{line}"),
                Err(line) => log::warn!("{line}"),
            }
        }
    }

    /// Open `name` on the surface Phase 0 targets for it: its original movie (S1), its native
    /// page (S3, or the fallback page of a MenuXml menu, whose documents beyond the HUD are
    /// Phase 3), or no surface (S4, recorded).
    fn open_menu(&mut self, name: &str) -> Result<String, String> {
        let (def, game) = {
            let registry = self.world.try_resource::<MenuRegistry>().ok_or("ui.open: no menu registry")?;
            let def = registry.table().by_name(name).cloned().ok_or_else(|| format!("ui.open: {name} is not a {} menu", registry.game().label()))?;
            (def, registry.game())
        };
        let page = match (&def.target, &def.fallback) {
            (TargetSurface::Native(page), _) | (TargetSurface::MenuXml, TargetSurface::Native(page)) => Some(page.clone()),
            _ => None,
        };
        match (&def.target, page) {
            (TargetSurface::Movie, _) => self.open_movie_menu(&def.name, def.document.as_deref(), game),
            (_, Some(page)) => self.open_native_page(&def.name, &page),
            _ => {
                if let Some(mut requests) = self.world.try_resource_mut::<MenuRequests>() {
                    if !requests.absent.iter().any(|open| open.eq_ignore_ascii_case(&def.name)) {
                        requests.absent.push(def.name.clone());
                    }
                }
                Ok(format!("ui.open: {} opened with no surface (absent fallback=recorded)", def.name))
            }
        }
    }

    /// Load `name`'s original movie from the game's interface archive as a new layer on top.
    fn open_movie_menu(&mut self, name: &str, document: Option<&str>, game: MenuGame) -> Result<String, String> {
        let path = document.ok_or_else(|| format!("ui.open: {name} has no movie"))?;
        let args = crate::cli_args::effective_args();
        let (_, archive_path) = crate::scaleform_hud::interface_archive(&args)?
            .ok_or_else(|| format!("ui.open: no {} interface archive beside the ESM", game.label()))?;
        let archive = crate::asset_provider::Archive::open(&archive_path).map_err(|e| format!("ui.open: {archive_path}: {e}"))?;
        let ctx = self.renderer.as_mut().ok_or("ui.open: no renderer")?;
        let (w, h) = ctx.swapchain_extent();
        let manager = self.ui.manager.get_or_insert_with(|| UiManager::new(w, h));
        // GUI-REG-007 — a movie that fails to load registers nothing; input stays with its owner.
        manager
            .load_swf_from_resource_provider(std::sync::Arc::new(archive), path, path, None)
            .map_err(|e| format!("ui.open: {name}: {path} failed to load: {e:#}"))?;
        let menu = UiManager::installed_menu_name(path);
        let handle = {
            let upload_ctx = byroredux_renderer::upload_ctx!(ctx);
            ctx.texture_registry.register_rgba(upload_ctx, w, h, &vec![0u8; (w * h * 4) as usize])
        };
        let layer = manager.layer_mut(&menu).expect("menu loaded above");
        match handle {
            Ok(handle) => {
                if let Some(old) = layer.texture().filter(|old| *old != handle) {
                    ctx.texture_registry.drop_texture(&ctx.device, old);
                }
                layer.set_texture(Some(handle));
            }
            Err(e) => log::error!("ui.open: {name}: UI texture registration failed: {e:#}"),
        }
        Ok(format!("ui.open: {name} opened as movie {path} profile={:?}", layer.profile()))
    }

    /// Open a native page by its Phase 0 name. Only the pages the player opens directly can be
    /// opened bare; the others need the game action that feeds them.
    fn open_native_page(&mut self, name: &str, page: &str) -> Result<String, String> {
        match page {
            "Inventory" => self.open_inventory_menu(),
            "Pause" if !self.debug_ui.as_ref().is_some_and(byroredux_debug_ui::DebugUiState::game_menu_visible) => self.toggle_game_menu(),
            "Pause" => {}
            _ => return Err(format!("ui.open: {name}'s native page ({page}) opens from its game action, not bare")),
        }
        Ok(format!("ui.open: {name} opened on the native {page} page"))
    }

    /// Close `name` with `result`, whatever its surface.
    fn close_menu(&mut self, name: &str, result: MenuResult) -> Result<String, String> {
        let entry = self
            .world
            .try_resource::<MenuRegistry>()
            .and_then(|registry| registry.entries().iter().find(|open| open.name.eq_ignore_ascii_case(name)).cloned())
            .ok_or_else(|| format!("ui.close: {name} is not open"))?;
        let verdict = match result {
            MenuResult::Accept => "accept",
            MenuResult::Cancel => "cancel",
        };
        match &entry.surface {
            MenuSurface::Movie { path, .. } => {
                let manager = self.ui.manager.as_mut().ok_or("ui.close: no movie stack")?;
                let menu = manager.layers().iter().find(|layer| layer.path() == path).map(|layer| layer.menu_name().to_owned());
                let layer = menu.and_then(|menu| manager.close(&menu)).ok_or_else(|| format!("ui.close: {name}'s movie is gone"))?;
                if let (Some(handle), Some(ctx)) = (layer.texture(), self.renderer.as_mut()) {
                    ctx.texture_registry.drop_texture(&ctx.device, handle);
                }
            }
            MenuSurface::Native { page } if page == NativeMenu::MessageBox.page() => {
                // Accept takes the box's first button; cancel dismisses it.
                let choice = (result == MenuResult::Accept).then_some(0);
                crate::notifications::answer_message_box(&self.world, choice);
            }
            MenuSurface::Native { page } if page == NativeMenu::Console.page() => {
                if let Some(ui) = self.debug_ui.as_mut() {
                    ui.set_visible(false);
                }
            }
            MenuSurface::Native { page } if [NativeMenu::Hud.page(), NativeMenu::Loading.page()].contains(&page.as_str()) => {
                return Err(format!("ui.close: {name} is an engine layer, not a closable menu"));
            }
            MenuSurface::Native { .. } => self.resume_from_game_menu(),
            MenuSurface::MenuXml { .. } => return Err(format!("ui.close: {name} is the MenuXml HUD; hide it with hud.off")),
            MenuSurface::Absent { .. } => {
                if let Some(mut requests) = self.world.try_resource_mut::<MenuRequests>() {
                    requests.absent.retain(|open| !open.eq_ignore_ascii_case(&entry.name));
                }
            }
        }
        Ok(format!("ui.close: {} closed result={verdict}", entry.name))
    }
}

#[cfg(test)]
mod tests;
