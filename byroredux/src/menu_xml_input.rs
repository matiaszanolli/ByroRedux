//! M48 GUI-XML-002 (#6665) — MenuXml input: the engine's `UiInputEvent`s onto a MenuXml menu's
//! targets (`byroredux_menuxml::MenuRenderer::hit_test` and its activations), the same event type
//! the Scaleform track routes, so one input-owner rule covers both tracks (GUI-REG-009).
//!
//! - `MouseMove` / `MouseDown` / `MouseUp` (left button) are pointer moves, presses and releases
//!   at UI pixels; a press and release on one target activates it once.
//! - `KeyDown`: Down / Tab focus the next target, Up the previous (the list items' `xdefault`
//!   one first), Enter / Space activate the focused one.
//!
//! The menu's own code then reads [`MenuRenderer::take_activations`] — the activated tiles with
//! their `<id>`, the handle a native page's control is keyed by.

use byroredux_core::ecs::resources::MenuRegistry;
use byroredux_menuxml::{MenuKey, MenuRenderer};
use byroredux_ui::{UiInputEvent, UiLogicalKey, UiMouseButton, UiNamedKey};

/// Whether the MenuXml menu `menu` owns input: it is the registry's input owner (GUI-REG-009).
// Self-expiring (#2732's pattern): no live MenuXml menu surface (S2) routes input yet; the
// frame's input path calls these when it opens one, and the expectation then fails the build.
#[cfg_attr(not(test), expect(dead_code, reason = "no live S2 MenuXml menu surface yet"))]
pub(crate) fn owns_input(registry: &MenuRegistry, menu: &str) -> bool {
    registry.input_owner().is_some_and(|owner| owner.name.eq_ignore_ascii_case(menu))
}

/// Route one input event to `renderer`; `true` when the event is one a MenuXml menu takes.
#[cfg_attr(not(test), expect(dead_code, reason = "no live S2 MenuXml menu surface yet"))]
pub(crate) fn route(renderer: &mut MenuRenderer, event: &UiInputEvent) -> bool {
    match event {
        UiInputEvent::MouseMove { .. } => true,
        UiInputEvent::MouseDown { x, y, button: UiMouseButton::Left } => {
            renderer.pointer_down(*x as f32, *y as f32);
            true
        }
        UiInputEvent::MouseUp { x, y, button: UiMouseButton::Left } => {
            renderer.pointer_up(*x as f32, *y as f32);
            true
        }
        UiInputEvent::KeyDown { key } => {
            let menu_key = match key.logical_key {
                UiLogicalKey::Named(UiNamedKey::ArrowDown | UiNamedKey::Tab) => MenuKey::Next,
                UiLogicalKey::Named(UiNamedKey::ArrowUp) => MenuKey::Previous,
                UiLogicalKey::Named(UiNamedKey::Enter) | UiLogicalKey::Character(' ') => MenuKey::Activate,
                _ => return false,
            };
            renderer.key(menu_key);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hud::{HudAssets, HudGameProfile};
    use byroredux_menuxml::ScreenTraits;
    use byroredux_ui::{UiKeyDescriptor, UiKeyLocation, UiPhysicalKey};

    fn down(x: f32, y: f32) -> UiInputEvent {
        UiInputEvent::MouseDown { x: x as f64, y: y as f64, button: UiMouseButton::Left }
    }

    fn up(x: f32, y: f32) -> UiInputEvent {
        UiInputEvent::MouseUp { x: x as f64, y: y as f64, button: UiMouseButton::Left }
    }

    fn key(named: UiNamedKey) -> UiInputEvent {
        UiInputEvent::KeyDown {
            key: UiKeyDescriptor {
                physical_key: UiPhysicalKey::Unknown,
                logical_key: UiLogicalKey::Named(named),
                key_location: UiKeyLocation::Standard,
            },
        }
    }

    /// The Phase 3 exit's corpus check (ui.md § *MenuXml input*): `menus\message_menu.xml`, a
    /// non-HUD document, loads through the game's `MenuProfile`; two buttons instantiated the way
    /// the engine builds them (`MM_ButtonTemplate` under `MM_ButtonList`, with the traits native
    /// code sets: the menu's io traits and the list's height, then per button `listindex`, `_y`,
    /// `string`, and the height it computes from the text) take
    /// pointer input through `UiInputEvent`: `hit_test` at a button's centre returns that tile, a
    /// down / up pair on it yields exactly one activation for it, and a pair outside every tile
    /// yields none.
    fn menu_document_takes_pointer_input(profile: HudGameProfile, data: std::path::PathBuf) {
        let assets = HudAssets::open_in(profile, &data).unwrap_or_else(|| panic!("the menu archives under {data:?}"));
        let mut menu = MenuRenderer::load_with_profile(&assets, "menus\\message_menu.xml", ScreenTraits::new(1280.0, 720.0), profile.menu)
            .expect("message_menu.xml loads through the game's MenuProfile");
        // What native code sets before it shows the menu: the menu's io traits the layout reads
        // (`io()`: its maximum width and buffers) and the list's size (`list_box.xml`: width
        // "REQUIRED OVERRIDE", height "recommended override"; the document's own width is only a
        // cap). Test values.
        menu.set_override("MessageMenu", "_MaxMenuWidth", 600.0);
        menu.set_override("MessageMenu", "_horbuf", 25.0);
        menu.set_override("MessageMenu", "_verbuf", 10.0);
        menu.set_override("MM_ButtonList", "width", 550.0);
        menu.set_override("MM_ButtonList", "height", 200.0);
        for (index, (name, label)) in [("MM_ButtonYes", "Yes"), ("MM_ButtonNo", "No")].into_iter().enumerate() {
            menu.instantiate_template("MM_ButtonTemplate", "MM_ButtonList", name).expect("a button instantiates");
            menu.set_override(name, "listindex", index as f32);
            menu.set_override(name, "_y", index as f32 * 60.0);
            menu.set_override(name, "height", 50.0);
            menu.set_override_str(name, "string", label);
        }
        let targets = menu.hit_targets();
        let centre = |name: &str| {
            let tile = menu.tile(name).expect("instantiated");
            let target = targets.iter().find(|target| target.tile == tile).unwrap_or_else(|| panic!("{name} is a hit target: {targets:?}"));
            (target.rect.x + target.rect.w / 2.0, target.rect.y + target.rect.h / 2.0)
        };
        let (yes, no) = (centre("MM_ButtonYes"), centre("MM_ButtonNo"));
        assert_eq!(menu.hit_test(yes.0, yes.1).and_then(|hit| hit.name).as_deref(), Some("MM_ButtonYes"));
        assert_eq!(menu.hit_test(no.0, no.1).and_then(|hit| hit.name).as_deref(), Some("MM_ButtonNo"));

        for event in [down(no.0, no.1), up(no.0, no.1)] {
            assert!(route(&mut menu, &event));
        }
        let activated: Vec<_> = menu.take_activations().into_iter().filter_map(|hit| hit.name).collect();
        assert_eq!(activated, ["MM_ButtonNo"], "exactly one activation, for the button pressed");

        for event in [down(2.0, 2.0), up(2.0, 2.0)] {
            route(&mut menu, &event);
        }
        assert!(menu.take_activations().is_empty(), "a press outside every tile activates nothing");
    }

    #[test]
    #[ignore = "needs FNV game data on disk"]
    fn fnv_menu_document_takes_pointer_input() {
        use byroredux_plugin::esm::test_paths::{data_dir, FNV_DEFAULT, FNV_ENV};
        menu_document_takes_pointer_input(HudGameProfile::fallout_nv(), data_dir(FNV_ENV, FNV_DEFAULT));
    }

    #[test]
    #[ignore = "needs FO3 game data on disk"]
    fn fo3_menu_document_takes_pointer_input() {
        use byroredux_plugin::esm::test_paths::{data_dir, FO3_DEFAULT, FO3_ENV};
        menu_document_takes_pointer_input(HudGameProfile::fallout3(), data_dir(FO3_ENV, FO3_DEFAULT));
    }

    /// The keyboard half, and the input-owner rule, without game data.
    #[test]
    fn keys_map_to_menu_navigation_and_input_goes_to_the_owner() {
        struct Doc;
        impl byroredux_menuxml::MenuAssets for Doc {
            fn menu_xml(&self, _path: &str) -> Option<Vec<u8>> {
                Some(
                    br#"<menu name="M"><locus> &true; </locus>
                    <rect name="A"> <target> &true; </target> <width> 10 </width> <height> 10 </height> </rect>
                    <rect name="B"> <target> &true; </target> <y> 20 </y> <width> 10 </width> <height> 10 </height> </rect>
                    </menu>"#
                        .to_vec(),
                )
            }
            fn texture(&self, _path: &str) -> Option<Vec<u8>> {
                None
            }
            fn font(&self, _index: u8) -> Option<Vec<u8>> {
                None
            }
            fn font_texture(&self, _path: &str) -> Option<Vec<u8>> {
                None
            }
        }
        let mut menu = MenuRenderer::load(&Doc, "menus\\m.xml", ScreenTraits::new(100.0, 100.0)).unwrap();
        for event in [key(UiNamedKey::ArrowDown), key(UiNamedKey::ArrowDown), key(UiNamedKey::Enter)] {
            assert!(route(&mut menu, &event));
        }
        assert_eq!(menu.take_activations().into_iter().filter_map(|hit| hit.name).collect::<Vec<_>>(), ["B"]);
        assert!(!route(&mut menu, &key(UiNamedKey::Escape)), "not a navigation key");

        use byroredux_core::ecs::resources::menu_registry::{MenuGame, MenuSurface};
        let mut registry = MenuRegistry::new(MenuGame::Fallout3NV);
        registry.open("MessageMenu", MenuSurface::MenuXml { path: "menus\\message_menu.xml".into() });
        assert!(owns_input(&registry, "messagemenu"));
        assert!(!owns_input(&registry, "HUDMainMenu"));
    }
}
