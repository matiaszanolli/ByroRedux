//! Oblivion's installed sleep/wait and level-up documents (GUI-XML-004).

use std::hash::{Hash, Hasher};

use byroredux_core::ecs::resources::menu_registry::MenuGame;
use byroredux_menuxml::{MenuRenderer, TileHit};
use byroredux_native_ui::{GameMenuPage, RestAction, RestView};
use byroredux_ui::{UiInputEvent, UiLogicalKey, UiMouseButton, UiNamedKey};

use crate::menuxml_document::MenuDocument;

pub(crate) const SLEEP_WAIT_CODE: u32 = 1012;
pub(crate) const LEVEL_UP_CODE: u32 = 1027;

pub(crate) const fn is_rest_document(code: u32) -> bool {
    matches!(code, SLEEP_WAIT_CODE | LEVEL_UP_CODE)
}

fn row_name(code: usize) -> String {
    format!("LevelAttribute{code}")
}

fn require_tiles(document: &MenuDocument, tiles: &[&str]) -> Result<(), String> {
    for &tile in tiles {
        if document.renderer.tile(tile).is_none() {
            return Err(format!("{} is missing tile {tile}", document.document));
        }
    }
    Ok(())
}

fn prepare_duration(document: &mut MenuDocument) -> Result<(), String> {
    require_tiles(
        document,
        &[
            "SleepWaitMenu",
            "sleep_scroll",
            "rest_time_text",
            "sleep_button_sleep_wait",
            "sleep_button_cancel",
            "horizontal_scroll_left",
            "horizontal_scroll_right",
            "horizontal_scroll_marker",
        ],
    )?;
    let renderer = &mut document.renderer;
    renderer.set_default_font(1);
    // The prefab's thumb formula divides by `(max - min)` and its
    // generic percentage trait divides by `max`. Use zero-based ticks.
    renderer.set_override("sleep_scroll", "user1", 0.0);
    renderer.set_override("sleep_scroll", "user2", 23.0);
    Ok(())
}

fn prepare_level(document: &mut MenuDocument) -> Result<(), String> {
    require_tiles(
        document,
        &["LevelUpMenu", "levelup_pane", "levelup_exit_button"],
    )?;
    document.renderer.set_default_font(1);
    for code in 0..8 {
        let row = row_name(code);
        document
            .renderer
            .instantiate_template("levelup_template", "levelup_pane", &row)
            .map_err(|error| format!("level-up attribute row: {error}"))?;
        document
            .renderer
            .set_override(&row, "listindex", code as f32);
        document
            .renderer
            .set_override(&row, "xdefault", if code == 0 { 2.0 } else { 0.0 });
        document.renderer.set_override_str(&row, "user5", "");
    }
    Ok(())
}

fn feed(
    document: &mut MenuDocument,
    view: &RestView,
    labels: &byroredux_core::ui_labels::UiLabels,
) {
    let mut hasher = rustc_hash::FxHasher::default();
    view.hash(&mut hasher);
    let signature = hasher.finish();
    if document.signature == Some(signature) {
        return;
    }
    document.signature = Some(signature);
    document.touch();
    let renderer = &mut document.renderer;
    let flag = |value| if value { 2.0 } else { 0.0 };
    match view {
        RestView::Duration { sleeping, hours } => {
            // This is the document's _question_1 / _question_2 selector.
            renderer.set_override("SleepWaitMenu", "user0", if *sleeping { 2.0 } else { 1.0 });
            renderer.set_override("sleep_scroll", "user5", f32::from(hours - 1));
            renderer.set_override_str("rest_time_text", "string", &hours.to_string());
            renderer.set_override_str(
                "sleep_button_sleep_wait",
                "user0",
                labels.text(if *sleeping { "Sleep" } else { "Wait" }),
            );
        }
        RestView::LevelUp {
            level,
            attributes,
            ready,
            ..
        } => {
            renderer.set_override_str(
                "LevelUpMenu",
                "user3",
                &format!("{} {level}", labels.text("Level")),
            );
            renderer.set_override("LevelUpMenu", "user1", flag(*ready));
            for (code, attribute) in attributes.iter().enumerate() {
                let row = row_name(code);
                renderer.set_override(&row, "user0", flag(attribute.selected));
                renderer.set_override(&row, "user1", 2.0);
                renderer.set_override_str(&row, "user2", &format!("+{}", attribute.bonus));
                renderer.set_override_str(&row, "user3", &attribute.base);
                renderer.set_override_str(&row, "user4", labels.text(attribute.name));
                renderer.set_override(&row, "user6", flag(attribute.enabled));
            }
        }
    }
}

fn action_of(hit: &TileHit) -> Option<RestAction> {
    match hit.name.as_deref()? {
        "sleep_button_sleep_wait" | "levelup_exit_button" => Some(RestAction::Confirm),
        "sleep_button_cancel" => Some(RestAction::Cancel),
        "horizontal_scroll_left" => Some(RestAction::AdjustHours(-1)),
        "horizontal_scroll_right" => Some(RestAction::AdjustHours(1)),
        row => row
            .strip_prefix("LevelAttribute")?
            .parse()
            .ok()
            .map(RestAction::ToggleAttribute),
    }
}

fn duration_at(renderer: &MenuRenderer, x: f32) -> Option<RestAction> {
    let targets = renderer.hit_targets();
    let bounds = |name| {
        let tile = renderer.tile(name)?;
        targets
            .iter()
            .find(|target| target.tile == tile)
            .map(|target| target.rect)
    };
    let left = bounds("horizontal_scroll_left")?;
    let right = bounds("horizontal_scroll_right")?;
    let start = left.x + left.w;
    let fraction = ((x - start) / (right.x - start).max(1.0)).clamp(0.0, 1.0);
    Some(RestAction::SetHours(1 + (fraction * 23.0).round() as u8))
}

fn slider(target: Option<TileHit>) -> bool {
    target.is_some_and(|hit| {
        matches!(
            hit.name.as_deref(),
            Some(
                "horizontal_scroll_marker"
                    | "horizontal_scroll_leftside"
                    | "horizontal_scroll_rightside"
                    | "sleep_scroll_marker"
            )
        )
    })
}

/// Input the generic MenuXml navigator cannot interpret: Escape and the duration slider.
pub(crate) fn direct_input(
    document: &mut MenuDocument,
    event: &UiInputEvent,
) -> Option<RestAction> {
    let duration = document.code == SLEEP_WAIT_CODE;
    match event {
        UiInputEvent::FocusLost => {
            document.renderer.cancel_pointer();
            None
        }
        UiInputEvent::KeyDown { key } => match key.logical_key {
            UiLogicalKey::Named(UiNamedKey::Escape) => Some(RestAction::Cancel),
            UiLogicalKey::Named(UiNamedKey::ArrowLeft) if duration => {
                Some(RestAction::AdjustHours(-1))
            }
            UiLogicalKey::Named(UiNamedKey::ArrowRight) if duration => {
                Some(RestAction::AdjustHours(1))
            }
            _ => None,
        },
        UiInputEvent::MouseDown {
            x,
            y,
            button: UiMouseButton::Left,
        } if duration && slider(document.renderer.hit_test(*x as f32, *y as f32)) => {
            duration_at(&document.renderer, *x as f32)
        }
        UiInputEvent::MouseMove { x, .. }
        | UiInputEvent::MouseUp {
            x,
            button: UiMouseButton::Left,
            ..
        } if duration && slider(document.renderer.pressed()) => {
            duration_at(&document.renderer, *x as f32)
        }
        _ => None,
    }
}

impl crate::App {
    pub(crate) fn close_rest_surface(&mut self) {
        self.close_menu_document(SLEEP_WAIT_CODE);
        self.close_menu_document(LEVEL_UP_CODE);
        if let Some(ui) = self.debug_ui.as_mut()
            && ui.rest_menu_visible()
        {
            self.resume_from_game_menu();
        }
    }

    pub(crate) fn present_oblivion_rest(&mut self) {
        use crate::interaction::{ActionState, InputAction};
        use crate::oblivion_rest::{apply, request_wait, snapshot};

        for code in [SLEEP_WAIT_CODE, LEVEL_UP_CODE] {
            let activations = self
                .menu_document_mut(code)
                .map(|document| document.renderer.take_activations())
                .unwrap_or_default();
            for action in activations.iter().filter_map(action_of) {
                apply(&self.world, action);
            }
        }
        if self.cover_holds_game() {
            return;
        }
        let other_owns_input =
            self.ui
                .menu_documents
                .iter()
                .any(|document| !is_rest_document(document.code))
                || self
                    .ui
                    .manager
                    .as_ref()
                    .is_some_and(byroredux_ui::UiManager::has_input_focus)
                || self.debug_ui.as_ref().is_some_and(|ui| {
                    ui.visible || ui.game_menu_visible() && !ui.rest_menu_visible()
                });
        let wait = crate::oblivion_rest::supports_rest(&self.world)
            && self
                .world
                .try_resource_mut::<ActionState>()
                .is_some_and(|mut actions| actions.take_pressed(InputAction::Wait));
        if wait && !other_owns_input {
            request_wait(&self.world);
        }
        let Some(view) = snapshot(&self.world) else {
            self.close_rest_surface();
            return;
        };
        if other_owns_input {
            return;
        }
        type Prepare = fn(&mut MenuDocument) -> Result<(), String>;
        let (code, other_code, page, prepare) = match view {
            RestView::Duration { .. } => (
                SLEEP_WAIT_CODE,
                LEVEL_UP_CODE,
                GameMenuPage::SleepWait,
                prepare_duration as Prepare,
            ),
            RestView::LevelUp { .. } => (
                LEVEL_UP_CODE,
                SLEEP_WAIT_CODE,
                GameMenuPage::LevelUp,
                prepare_level as Prepare,
            ),
        };
        self.close_menu_document(other_code);
        if self
            .debug_ui
            .as_ref()
            .and_then(byroredux_debug_ui::DebugUiState::open_game_menu_page)
            == Some(page)
        {
            return;
        }
        let opened = self.menu_document_mut(code).is_some();
        if opened || self.open_menu_document(MenuGame::Oblivion, code, "Oblivion rest", prepare) {
            let labels = crate::ui_labels(&self.world);
            if let Some(document) = self.menu_document_mut(code) {
                feed(document, &view, &labels);
            }
            if !opened {
                self.release_world_input_for_ui();
            }
        } else if let Some(ui) = self.debug_ui.as_mut() {
            ui.open_rest_menu(page);
            self.release_world_input_for_ui();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::{HudAssets, HudGameProfile};
    use byroredux_core::ui_labels::UiLabels;
    use byroredux_native_ui::RestAttribute;

    fn click(document: &mut MenuDocument, name: &str) -> Option<RestAction> {
        let tile = document.renderer.tile(name).unwrap();
        let targets = document.renderer.hit_targets();
        let rect = targets.iter().find(|target| target.tile == tile)?.rect;
        let (x, y) = (
            f64::from(rect.x + rect.w / 2.0),
            f64::from(rect.y + rect.h / 2.0),
        );
        let (width, height) = document.extent();
        assert!(
            (0.0..f64::from(width)).contains(&x) && (0.0..f64::from(height)).contains(&y),
            "{name} is outside the screen at {x}, {y}"
        );
        for event in [
            UiInputEvent::MouseDown {
                x,
                y,
                button: UiMouseButton::Left,
            },
            UiInputEvent::MouseUp {
                x,
                y,
                button: UiMouseButton::Left,
            },
        ] {
            crate::menu_xml_input::route(&mut document.renderer, &event);
        }
        document
            .renderer
            .take_activations()
            .iter()
            .find_map(action_of)
    }

    #[test]
    #[ignore = "needs the installed Oblivion MenuXml documents and art"]
    fn installed_oblivion_rest_documents_display_and_route_their_actions() {
        let data = byroredux_plugin::esm::test_paths::oblivion_data_dir();
        let assets =
            || HudAssets::open_in(HudGameProfile::oblivion(), &data).expect("Oblivion UI archives");
        let labels = UiLabels::default();
        let mut sleep = MenuDocument::headless(
            SLEEP_WAIT_CODE,
            "SleepWaitMenu",
            "menus\\sleep_wait_menu.xml",
            assets(),
            (1280, 720),
        );
        prepare_duration(&mut sleep).unwrap();
        feed(
            &mut sleep,
            &RestView::Duration {
                sleeping: true,
                hours: 1,
            },
            &labels,
        );
        assert_eq!(
            click(&mut sleep, "horizontal_scroll_right"),
            Some(RestAction::AdjustHours(1))
        );
        assert_eq!(
            click(&mut sleep, "sleep_button_sleep_wait"),
            Some(RestAction::Confirm)
        );
        feed(
            &mut sleep,
            &RestView::Duration {
                sleeping: false,
                hours: 24,
            },
            &labels,
        );
        assert_eq!(
            sleep
                .renderer
                .value("rest_time_text", "string")
                .unwrap()
                .as_num(),
            24.0
        );
        assert_eq!(
            click(&mut sleep, "sleep_button_cancel"),
            Some(RestAction::Cancel)
        );
        assert!(sleep.render().chunks_exact(4).any(|pixel| pixel[3] != 0));

        let mut level = MenuDocument::headless(
            LEVEL_UP_CODE,
            "LevelUpMenu",
            "menus\\levelup_menu.xml",
            assets(),
            (1280, 720),
        );
        prepare_level(&mut level).unwrap();
        let mut attributes: [RestAttribute; 8] = std::array::from_fn(|code| RestAttribute {
            name: byroredux_plugin::esm::records::oblivion_actor_values::OBLIVION_ACTOR_VALUES
                [code],
            base: "40".to_owned(),
            bonus: if code == 0 { 5 } else { 1 },
            selected: false,
            enabled: true,
        });
        feed(
            &mut level,
            &RestView::LevelUp {
                level: 2,
                attributes: Box::new(attributes.clone()),
                selected: 0,
                ready: false,
            },
            &labels,
        );
        for code in [0, 5, 7] {
            assert_eq!(
                click(&mut level, &row_name(code)),
                Some(RestAction::ToggleAttribute(code as u8))
            );
            attributes[code].selected = true;
        }
        assert_eq!(click(&mut level, "levelup_exit_button"), None);
        feed(
            &mut level,
            &RestView::LevelUp {
                level: 2,
                attributes: Box::new(attributes),
                selected: 3,
                ready: true,
            },
            &labels,
        );
        assert_eq!(
            click(&mut level, "levelup_exit_button"),
            Some(RestAction::Confirm)
        );
        assert!(level.render().chunks_exact(4).any(|pixel| pixel[3] != 0));
    }
}
