//! The native game menu and its pages: crafting, barter, message box, dialogue, pause, settings, inventory (#6578, split from `panels.rs`).

use super::*;
use egui::{Align2, Color32, Context, CornerRadius, Frame, Id, Margin, Order, RichText, Stroke, Window};

use byroredux_core::settings::{SettingChange, SettingEntry};

use super::studio::draw_setting_control;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameMenuPage {
    #[default]
    Pause,
    Settings,
    Inventory,
    Dialogue,
    MessageBox,
    Barter,
    Crafting,
    /// A character-creation screen a script opened (name, sex and race, SPECIAL, tag skills,
    /// traits).
    CharGen,
}

/// Persistent navigation state for the player-facing native menu.
#[derive(Debug, Clone, Default)]
pub struct GameMenuState {
    pub visible: bool,
    pub page: GameMenuPage,
    pub selected_section: String,
    pub selected_inventory_category: String,
    pub selected_inventory_index: Option<u32>,
    /// The open character-creation screen's working choices.
    pub chargen: super::ChargenDraft,
}

/// Player-facing pause/settings surface. This deliberately consumes the same
/// `SettingEntry` snapshot and emits the same `SettingChange` values as the F3
/// operator panel, so there is one validation/application path.
pub fn draw_game_menu(
    ctx: &Context,
    snapshot: &PanelSnapshot,
    state: &mut GameMenuState,
    outputs: &mut PanelOutputs,
) {
    let viewport = ctx.content_rect();
    egui::Area::new(Id::new("game_menu_backdrop"))
        .order(Order::Foreground)
        .fixed_pos(viewport.min)
        .interactable(true)
        .show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_black_alpha(205))
                .show(ui, |ui| {
                    ui.set_min_size(viewport.size());
                });
        });

    let title = match state.page {
        GameMenuPage::Pause => "Paused",
        GameMenuPage::Settings => "Settings",
        GameMenuPage::Inventory => "Inventory",
        GameMenuPage::Dialogue => "Conversation",
        GameMenuPage::MessageBox => "Message",
        GameMenuPage::Barter => "Barter",
        GameMenuPage::Crafting => "Crafting",
        GameMenuPage::CharGen => "Character",
    };
    Window::new(title)
        .id(Id::new("game_menu_window"))
        .order(Order::Foreground)
        .anchor(Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .fixed_size(match state.page {
            GameMenuPage::Pause => egui::vec2(360.0, 390.0),
            GameMenuPage::Settings => egui::vec2(760.0, 620.0),
            GameMenuPage::Inventory => egui::vec2(900.0, 620.0),
            GameMenuPage::Dialogue => egui::vec2(760.0, 560.0),
            GameMenuPage::MessageBox => egui::vec2(520.0, 320.0),
            GameMenuPage::Barter => egui::vec2(900.0, 620.0),
            GameMenuPage::Crafting => egui::vec2(900.0, 620.0),
            GameMenuPage::CharGen => egui::vec2(620.0, 560.0),
        })
        .frame(
            Frame::window(&ctx.global_style())
                .fill(Color32::from_rgb(20, 23, 29))
                .stroke(Stroke::new(1.0_f32, Color32::from_gray(75)))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(22)),
        )
        .show(ctx, |ui| match state.page {
            GameMenuPage::Pause => draw_pause_page(ui, state, outputs),
            GameMenuPage::Settings => draw_game_settings(ui, &snapshot.settings, state, outputs),
            GameMenuPage::Inventory => {
                draw_inventory_page(ui, snapshot.inventory.as_ref(), state, outputs)
            }
            GameMenuPage::Dialogue => {
                draw_dialogue_page(ui, snapshot.dialogue.as_ref(), state, outputs)
            }
            GameMenuPage::MessageBox => {
                draw_message_box_page(ui, snapshot.message_box.as_ref(), state, outputs)
            }
            GameMenuPage::Barter => draw_barter_page(ui, snapshot.barter.as_ref(), state, outputs),
            GameMenuPage::Crafting => draw_crafting_page(ui, snapshot.crafting.as_ref(), state, outputs),
            GameMenuPage::CharGen => super::chargen::draw_chargen_page(ui, snapshot.chargen.as_ref(), &mut state.chargen, outputs),
        });
}

/// The crafting page: the bench's recipes, each with the ingredients held against needed; a recipe
/// the player can afford has a Craft button. Closing is the shared resume path, which ends the
/// session.
fn draw_crafting_page(
    ui: &mut egui::Ui,
    crafting: Option<&CraftingSnapshot>,
    state: &mut GameMenuState,
    outputs: &mut PanelOutputs,
) {
    let Some(crafting) = crafting else {
        ui.label("There is nothing to craft here.");
        ui.add_space(12.0);
        if wide_button(ui, "Close (Esc)").clicked() {
            outputs.resume_game = true;
            state.visible = false;
        }
        return;
    };
    ui.heading(crafting.bench_name.clone());
    ui.add_space(8.0);
    egui::ScrollArea::vertical()
        .id_salt("native_crafting_recipes")
        .max_height(470.0)
        .show(ui, |ui| {
            ui.set_min_width(840.0);
            if crafting.recipes.is_empty() {
                ui.label("No recipes.");
            }
            for recipe in &crafting.recipes {
                ui.horizontal(|ui| {
                    let title = if recipe.count > 1 {
                        format!("{} ×{}", recipe.name, recipe.count)
                    } else {
                        recipe.name.clone()
                    };
                    let colour = if recipe.craftable {
                        Color32::from_rgb(225, 222, 210)
                    } else {
                        Color32::from_gray(120)
                    };
                    ui.label(RichText::new(title).strong().color(colour));
                    let needs: Vec<String> = recipe
                        .ingredients
                        .iter()
                        .map(|(name, need, have)| format!("{name} {have}/{need}"))
                        .collect();
                    ui.label(RichText::new(needs.join(", ")).color(Color32::from_gray(150)));
                    if ui
                        .add_enabled(recipe.craftable, egui::Button::new("Craft"))
                        .clicked()
                    {
                        outputs.crafting_actions.push(CraftingAction::Craft { recipe: recipe.form_id });
                    }
                });
            }
        });
    ui.add_space(12.0);
    if wide_button(ui, "Close (Esc)").clicked() {
        outputs.resume_game = true;
        state.visible = false;
    }
}

/// The barter page: the merchant's goods (buy) beside the player's goods the
/// merchant takes (sell), one unit per click at the listed price. Closing is
/// the shared resume path, which ends the session.
fn draw_barter_page(
    ui: &mut egui::Ui,
    barter: Option<&BarterSnapshot>,
    state: &mut GameMenuState,
    outputs: &mut PanelOutputs,
) {
    let Some(barter) = barter else {
        ui.label("The merchant has nothing to trade.");
        ui.add_space(12.0);
        if wide_button(ui, "Close (Esc)").clicked() {
            outputs.resume_game = true;
            state.visible = false;
        }
        return;
    };
    ui.heading(barter.vendor_name.clone());
    ui.add_space(8.0);
    let column = |ui: &mut egui::Ui,
                  title: &str,
                  gold: u32,
                  items: &[BarterItemView],
                  salt: &str,
                  action: fn(u32) -> BarterAction,
                  outputs: &mut PanelOutputs| {
        ui.vertical(|ui| {
            ui.label(RichText::new(format!("{title} — gold {gold}")).strong());
            egui::ScrollArea::vertical()
                .id_salt(salt)
                .max_height(470.0)
                .show(ui, |ui| {
                    ui.set_min_width(400.0);
                    if items.is_empty() {
                        ui.label("Nothing.");
                    }
                    for item in items {
                        let label =
                            format!("{}  ×{}  —  {} gold", item.name, item.count, item.price);
                        if ui.selectable_label(false, label).clicked() {
                            outputs.barter_actions.push(action(item.form_id));
                        }
                    }
                });
        });
    };
    ui.horizontal_top(|ui| {
        column(
            ui,
            "Merchant's goods (buy)",
            barter.vendor_gold,
            &barter.buy,
            "native_barter_buy",
            |form_id| BarterAction::Buy { form_id },
            outputs,
        );
        ui.separator();
        column(
            ui,
            "Your goods (sell)",
            barter.player_gold,
            &barter.sell,
            "native_barter_sell",
            |form_id| BarterAction::Sell { form_id },
            outputs,
        );
    });
    ui.add_space(12.0);
    if wide_button(ui, "Close (Esc)").clicked() {
        outputs.resume_game = true;
        state.visible = false;
    }
}

/// A script message box: the CK *Message* page's "dialog box that pauses the
/// game and waits for a response". Each authored button emits its index; a
/// box without buttons gets one "Ok". The binary pops the box on the choice
/// and reopens the page while more are queued.
fn draw_message_box_page(
    ui: &mut egui::Ui,
    message: Option<&MessageBoxView>,
    state: &mut GameMenuState,
    outputs: &mut PanelOutputs,
) {
    ui.vertical_centered(|ui| {
        ui.add_space(4.0);
        let Some(message) = message else {
            if wide_button(ui, "Ok").clicked() {
                outputs.resume_game = true;
                state.visible = false;
            }
            return;
        };
        if !message.title.is_empty() {
            ui.heading(message.title.clone());
            ui.add_space(8.0);
        }
        egui::ScrollArea::vertical()
            .id_salt("native_message_box_text")
            .max_height(170.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                ui.label(
                    RichText::new(message.text.clone())
                        .size(16.0)
                        .color(Color32::from_rgb(225, 222, 210)),
                );
            });
        ui.add_space(12.0);
        if message.buttons.is_empty() {
            if wide_button(ui, "Ok").clicked() {
                outputs.message_box_choice = Some(0);
            }
            return;
        }
        for (index, button) in message.buttons.iter().enumerate() {
            if wide_button(ui, button).clicked() {
                outputs.message_box_choice = Some(index);
            }
            ui.add_space(4.0);
        }
    });
}

/// P4 blocker 2 — the native dialogue response surface: the selected INFO's
/// response text above the NPC's owned-topic list. Selecting a topic emits
/// [`DialogueUiAction::SelectTopic`]; the binary lowers it through the same
/// selection the activation path uses, and the next frame's snapshot
/// carries the new response. Closing is the pause menu's own resume path —
/// the same Escape / Resume handling every other page shares.
fn draw_dialogue_page(
    ui: &mut egui::Ui,
    dialogue: Option<&DialogueTopicSnapshot>,
    state: &mut GameMenuState,
    outputs: &mut PanelOutputs,
) {
    ui.add_space(4.0);
    if let Some(dialogue) = dialogue {
        ui.horizontal(|ui| {
            if !dialogue.speaker_name.is_empty() {
                ui.label(
                    egui::RichText::new(dialogue.speaker_name.clone())
                        .size(20.0)
                        .strong()
                        .color(Color32::from_rgb(235, 220, 170)),
                );
            }
            // The speaking cue: shown while the line is being presented, and says whether real voice plays.
            if dialogue.speaking {
                ui.label(
                    egui::RichText::new(if dialogue.voiced { "(speaking)" } else { "(speaking, no voice)" })
                        .italics()
                        .color(Color32::from_rgb(150, 200, 235)),
                );
            }
        });
        ui.heading(dialogue.topic_name.clone());
        ui.add_space(10.0);
        // The response: a fixed-height, scrolling read-only panel so long
        // authored text never resizes the window.
        egui::ScrollArea::vertical()
            .id_salt("native_dialogue_response")
            .max_height(180.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                // The subtitle: full strength while the line is presented, dimmed once it has finished.
                let subtitle = if dialogue.speaking {
                    Color32::from_rgb(240, 238, 225)
                } else {
                    Color32::from_rgb(170, 168, 158)
                };
                ui.label(egui::RichText::new(dialogue.response_text.clone()).size(16.0).color(subtitle));
            });
        ui.add_space(12.0);
        ui.separator();
        ui.label(if dialogue.choices { "Replies" } else { "Topics" });
        egui::ScrollArea::vertical()
            .id_salt("native_dialogue_topics")
            .max_height(180.0)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());
                for (position, topic) in dialogue.topics.iter().enumerate() {
                    let selected = topic.topic_form_id == dialogue.topic_form_id;
                    // Replies are numbered; a topic already said is marked, the one being spoken is arrowed.
                    let mut text = if dialogue.choices { format!("{}. {}", position + 1, topic.name) } else { topic.name.clone() };
                    if selected {
                        text = format!("> {text}");
                    } else if topic.said {
                        text.push_str("  (said)");
                    }
                    let label = ui.selectable_label(selected, text);
                    if label.clicked() && !selected {
                        outputs.dialogue_actions.push(DialogueUiAction::SelectTopic {
                            npc: dialogue.npc,
                            topic_form_id: topic.topic_form_id,
                        });
                    }
                }
            });
        ui.add_space(12.0);
        if ui
            .add_sized([220.0, 40.0], egui::Button::new("Close (Esc)"))
            .clicked()
        {
            outputs.resume_game = true;
            state.visible = false;
        }
    } else {
        // Opened without a selection to show (a stale open across a cell
        // transition): say so instead of rendering an empty shell.
        ui.label("The conversation has moved on.");
        ui.add_space(12.0);
        if ui
            .add_sized([220.0, 40.0], egui::Button::new("Close (Esc)"))
            .clicked()
        {
            outputs.resume_game = true;
            state.visible = false;
        }
    }
}

fn draw_pause_page(ui: &mut egui::Ui, state: &mut GameMenuState, outputs: &mut PanelOutputs) {
    ui.vertical_centered(|ui| {
        ui.add_space(12.0);
        ui.label(
            RichText::new("BYROREDUX")
                .size(13.0)
                .strong()
                .color(Color32::from_rgb(145, 175, 210)),
        );
        ui.heading(RichText::new("Paused").size(34.0));
        ui.add_space(24.0);
        if wide_button(ui, "Continue").clicked() {
            outputs.resume_game = true;
        }
        ui.add_space(8.0);
        if wide_button(ui, "Settings").clicked() {
            state.page = GameMenuPage::Settings;
        }
        ui.add_space(8.0);
        if wide_button(ui, "Inventory").clicked() {
            state.page = GameMenuPage::Inventory;
        }
        ui.add_space(8.0);
        if wide_button(ui, "Quicksave").clicked() {
            outputs.quicksave = true;
        }
        ui.add_space(8.0);
        if wide_button(ui, "Quickload").clicked() {
            outputs.quickload = true;
        }
        ui.add_space(8.0);
        if wide_button(ui, "Quit to desktop").clicked() {
            outputs.quit_game = true;
        }
        ui.add_space(20.0);
        ui.label(RichText::new("Esc  Continue").small().color(Color32::GRAY));
    });
}

fn wide_button(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add_sized(
        [280.0, 42.0],
        egui::Button::new(RichText::new(label).size(18.0)),
    )
}

fn draw_game_settings(
    ui: &mut egui::Ui,
    settings: &[SettingEntry],
    state: &mut GameMenuState,
    outputs: &mut PanelOutputs,
) {
    ui.horizontal(|ui| {
        if ui.button("← Back").clicked() {
            state.page = GameMenuPage::Pause;
        }
        ui.heading("Settings");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("Changes are saved automatically")
                    .small()
                    .color(Color32::GRAY),
            );
        });
    });
    ui.separator();

    let mut sections: Vec<&str> = settings
        .iter()
        .map(|entry| entry.section.as_str())
        .collect();
    sections.sort_unstable_by_key(|section| section_rank(section));
    sections.dedup();
    if !sections
        .iter()
        .any(|section| *section == state.selected_section)
    {
        state.selected_section = sections.first().copied().unwrap_or_default().to_owned();
    }

    ui.horizontal_wrapped(|ui| {
        for section in &sections {
            ui.selectable_value(&mut state.selected_section, (*section).to_owned(), *section);
        }
    });
    ui.separator();

    if sections.is_empty() {
        ui.label("No settings are registered yet.");
        return;
    }

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for entry in settings
                .iter()
                .filter(|entry| entry.section == state.selected_section)
            {
                draw_player_setting(ui, entry, outputs);
                ui.add_space(7.0);
            }
        });
}

fn draw_inventory_page(
    ui: &mut egui::Ui,
    inventory: Option<&InventorySnapshot>,
    state: &mut GameMenuState,
    outputs: &mut PanelOutputs,
) {
    ui.horizontal(|ui| {
        if ui.button("← Back").clicked() {
            state.page = GameMenuPage::Pause;
        }
        ui.heading("Inventory");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                RichText::new("Tab / Esc  Close")
                    .small()
                    .color(Color32::GRAY),
            );
        });
    });
    ui.separator();

    let Some(inventory) = inventory else {
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            ui.heading("Inventory unavailable");
            ui.label("Enter character mode in a loaded game cell to use the player inventory.");
        });
        return;
    };
    if inventory.items.is_empty() {
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            ui.heading("Your inventory is empty");
        });
        return;
    }

    let mut categories: Vec<&str> = inventory.items.iter().map(|item| item.category).collect();
    categories.sort_unstable();
    categories.dedup();
    if state.selected_inventory_category.is_empty()
        || (state.selected_inventory_category != "All"
            && !categories.contains(&state.selected_inventory_category.as_str()))
    {
        state.selected_inventory_category = "All".to_owned();
    }
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(
            &mut state.selected_inventory_category,
            "All".to_owned(),
            "All",
        );
        for category in &categories {
            ui.selectable_value(
                &mut state.selected_inventory_category,
                (*category).to_owned(),
                *category,
            );
        }
    });
    ui.separator();

    let selected_category = state.selected_inventory_category.clone();
    let category_matches =
        |item: &InventoryItemView| selected_category == "All" || item.category == selected_category;

    if !inventory
        .items
        .iter()
        .filter(|item| category_matches(item))
        .any(|item| Some(item.index) == state.selected_inventory_index)
    {
        state.selected_inventory_index = inventory
            .items
            .iter()
            .find(|item| category_matches(item))
            .map(|item| item.index);
    }

    let visible_count = inventory
        .items
        .iter()
        .filter(|item| category_matches(item))
        .count();

    ui.columns(2, |columns| {
        columns[0].set_width(430.0);
        columns[0].label(
            RichText::new(format!(
                "{} of {} stacks  ·  {:.1} total weight",
                visible_count,
                inventory.items.len(),
                inventory.total_weight
            ))
            .small()
            .color(Color32::GRAY),
        );
        columns[0].add_space(6.0);
        egui::ScrollArea::vertical()
            .id_salt("native_inventory_items")
            .auto_shrink([false, false])
            .show(&mut columns[0], |ui| {
                for item in inventory.items.iter().filter(|item| category_matches(item)) {
                    let suffix = if item.count > 1 {
                        format!(" ×{}", item.count)
                    } else {
                        String::new()
                    };
                    let equipped = if item.equipped { "  ◆" } else { "" };
                    let response = ui.selectable_label(
                        state.selected_inventory_index == Some(item.index),
                        format!("{}{}{}", item.name, suffix, equipped),
                    );
                    if response.clicked() {
                        state.selected_inventory_index = Some(item.index);
                    }
                    response.on_hover_text(format!("{} · {:08X}", item.category, item.form_id));
                }
            });

        let selected = state
            .selected_inventory_index
            .and_then(|index| inventory.items.iter().find(|item| item.index == index));
        let Some(item) = selected else {
            return;
        };
        columns[1].heading(&item.name);
        columns[1].label(
            RichText::new(item.category)
                .strong()
                .color(Color32::from_rgb(145, 175, 210)),
        );
        columns[1].add_space(12.0);
        if !item.details.is_empty() {
            columns[1].label(&item.details);
        }
        egui::Grid::new("native_inventory_details")
            .num_columns(2)
            .spacing([24.0, 8.0])
            .show(&mut columns[1], |ui| {
                ui.label("Count");
                ui.monospace(item.count.to_string());
                ui.end_row();
                ui.label("Weight");
                ui.monospace(format!("{:.1}", item.weight));
                ui.end_row();
                ui.label("Value");
                ui.monospace(item.value.to_string());
                ui.end_row();
                ui.label("Form ID");
                ui.monospace(format!("{:08X}", item.form_id));
                ui.end_row();
            });
        columns[1].add_space(24.0);
        if !item.equipped
            && columns[1]
                .add_sized([220.0, 32.0], egui::Button::new("Drop"))
                .clicked()
        {
            outputs.inventory_actions.push(InventoryAction::Drop {
                index: item.index,
                form_id: item.form_id,
            });
        }
        columns[1].add_space(8.0);
        if item.consumable {
            if columns[1]
                .add_sized([220.0, 42.0], egui::Button::new("Use"))
                .clicked()
            {
                outputs.inventory_actions.push(InventoryAction::Consume {
                    index: item.index,
                    form_id: item.form_id,
                });
            }
        } else if item.equippable {
            let label = if item.equipped { "Unequip" } else { "Equip" };
            if columns[1]
                .add_sized(
                    [220.0, 42.0],
                    egui::Button::new(RichText::new(label).size(18.0)),
                )
                .clicked()
            {
                outputs
                    .inventory_actions
                    .push(InventoryAction::ToggleEquip { index: item.index });
            }
        } else {
            columns[1].add_enabled(
                false,
                egui::Button::new(if item.category == "Aid" {
                    "Use unavailable"
                } else {
                    "Equip unavailable"
                }),
            );
            columns[1].label(
                RichText::new(if item.category == "Aid" {
                    "This item's effects are not fully supported yet."
                } else {
                    "This item type has no runtime equipment contract yet."
                })
                .small()
                .color(Color32::GRAY),
            );
        }
    });
}

fn section_rank(section: &str) -> (u8, &str) {
    let rank = match section {
        "Gameplay" => 0,
        "Controls" => 1,
        "Interface" => 2,
        "Rendering" => 3,
        "Audio" => 4,
        _ => 5,
    };
    (rank, section)
}

fn draw_player_setting(ui: &mut egui::Ui, entry: &SettingEntry, outputs: &mut PanelOutputs) {
    ui.group(|ui| {
        ui.set_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                let mut title = RichText::new(&entry.label).strong().size(16.0);
                if entry.restart_required {
                    title = title.color(Color32::YELLOW);
                }
                ui.label(title);
                if !entry.description.is_empty() {
                    ui.label(
                        RichText::new(&entry.description)
                            .small()
                            .color(Color32::GRAY),
                    );
                }
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if entry.value != entry.default && ui.small_button("Reset").clicked() {
                    outputs
                        .setting_changes
                        .push(SettingChange::new(&entry.id, entry.default.clone()));
                }
                if let Some(value) = draw_setting_control(ui, entry) {
                    outputs
                        .setting_changes
                        .push(SettingChange::new(&entry.id, value));
                }
            });
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::end_pass;

    #[test]
    fn native_pause_menu_draws_without_the_debug_overlay() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        let mut state = GameMenuState {
            visible: true,
            ..Default::default()
        };
        let mut outputs = PanelOutputs::default();
        draw_game_menu(&ctx, &PanelSnapshot::default(), &mut state, &mut outputs);
        let output = end_pass(&ctx);
        assert!(!output.shapes.is_empty());
        assert!(!outputs.resume_game);
        assert!(!outputs.quit_game);
    }

    #[test]
    fn native_inventory_page_draws_real_item_details() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        let snapshot = PanelSnapshot {
            inventory: Some(InventorySnapshot {
                total_weight: 30.0,
                items: vec![InventoryItemView {
                    index: 0,
                    form_id: 0x1234,
                    name: "Iron Armor".to_owned(),
                    category: "Armor",
                    details: "Armor rating 25.0".to_owned(),
                    count: 1,
                    value: 125,
                    weight: 30.0,
                    equipped: true,
                    equippable: true,
                    consumable: false,
                }],
            }),
            ..Default::default()
        };
        let mut state = GameMenuState {
            visible: true,
            page: GameMenuPage::Inventory,
            ..Default::default()
        };
        let mut outputs = PanelOutputs::default();
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let output = end_pass(&ctx);
        assert!(!output.shapes.is_empty());
        assert_eq!(state.selected_inventory_category, "All");
        assert_eq!(state.selected_inventory_index, Some(0));
        assert!(outputs.inventory_actions.is_empty());
    }

    #[test]
    fn restorative_use_button_emits_identity_checked_action() {
        let ctx = Context::default();
        let snapshot = PanelSnapshot {
            inventory: Some(InventorySnapshot {
                total_weight: 0.5,
                items: vec![InventoryItemView {
                    index: 7,
                    form_id: 0x3EADD,
                    name: "Healing Potion".into(),
                    category: "Aid",
                    details: "Restores Health".into(),
                    count: 2,
                    value: 20,
                    weight: 0.5,
                    equipped: false,
                    equippable: false,
                    consumable: true,
                }],
            }),
            ..Default::default()
        };
        let mut state = GameMenuState {
            visible: true,
            page: GameMenuPage::Inventory,
            ..Default::default()
        };
        let mut outputs = PanelOutputs::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        // egui sizes a newly opened window before painting its contents.
        let _ = end_pass(&ctx);
        ctx.begin_pass(egui::RawInput::default());
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let frame = end_pass(&ctx);
        let pos = frame
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    if text.galley.text() == "Use" {
                        return Some(text.pos + text.galley.size() * 0.5);
                    }
                }
                None
            })
            .expect("Use button must be visible");
        ctx.begin_pass(egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        });
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let _ = end_pass(&ctx);
        assert_eq!(
            outputs.inventory_actions,
            vec![InventoryAction::Consume {
                index: 7,
                form_id: 0x3EADD
            }]
        );
    }

    /// M43 — the page reads the line's timing, the reply-vs-topic mode and the said-markers; a click on a
    /// reply still emits the selection, and the reply that is being spoken does not.
    #[test]
    fn dialogue_page_marks_speaking_replies_and_said_topics() {
        let ctx = Context::default();
        let snapshot = PanelSnapshot {
            dialogue: Some(DialogueTopicSnapshot {
                npc: 7,
                speaker_name: "Eltrys".into(),
                topic_form_id: 0x2010,
                topic_name: "Greeting".into(),
                info_form_id: 0x2011,
                response_text: "Well met.".into(),
                topics: vec![
                    DialogueTopicEntryView { topic_form_id: 0x2010, name: "Hello".into(), said: false },
                    DialogueTopicEntryView { topic_form_id: 0x2012, name: "Leave".into(), said: true },
                ],
                speaking: true,
                voiced: true,
                choices: true,
            }),
            ..Default::default()
        };
        let mut state = GameMenuState { visible: true, page: GameMenuPage::Dialogue, ..Default::default() };
        let mut outputs = PanelOutputs::default();
        // egui sizes a newly opened window before painting its contents: assert on the second pass.
        ctx.begin_pass(egui::RawInput::default());
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let _ = end_pass(&ctx);
        ctx.begin_pass(egui::RawInput::default());
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let output = end_pass(&ctx);
        let text: String = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::epaint::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(text.contains("(speaking)"), "the speaking cue with real voice: {text}");
        assert!(text.contains("Replies"), "the list is the line's replies");
        assert!(text.contains("> 1. Hello"), "the spoken reply is arrowed and numbered");
        assert!(text.contains("2. Leave  (said)"), "a said reply is marked");
        assert!(outputs.dialogue_actions.is_empty());
    }

    #[test]
    fn dialogue_page_shows_the_response_and_emits_topic_clicks() {
        let ctx = Context::default();
        let snapshot = PanelSnapshot {
            dialogue: Some(DialogueTopicSnapshot {
                npc: 7,
                speaker_name: "Eltrys".into(),
                topic_form_id: 0x2010,
                topic_name: "MS01EltrysTopic".into(),
                info_form_id: 0x2011,
                response_text: "You took the note, then.".into(),
                speaking: false,
                voiced: false,
                choices: false,
                topics: vec![
                    DialogueTopicEntryView {
                        said: false,
                        topic_form_id: 0x2010,
                        name: "Eltrys".into(),
                    },
                    DialogueTopicEntryView {
                        said: false,
                        topic_form_id: 0x2012,
                        name: "Rumors".into(),
                    },
                ],
            }),
            ..Default::default()
        };
        let mut state = GameMenuState {
            visible: true,
            page: GameMenuPage::Dialogue,
            ..Default::default()
        };
        let mut outputs = PanelOutputs::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        // egui sizes a newly opened window before painting its contents;
        // the response text asserts on the painted second pass.
        let _ = end_pass(&ctx);
        ctx.begin_pass(egui::RawInput::default());
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let frame = end_pass(&ctx);
        assert!(
            frame.shapes.iter().any(|shape| {
                matches!(
                    shape.shape,
                    egui::Shape::Text(ref text) if text.galley.text().contains("You took the note")
                )
            }),
            "the selected INFO response text must present"
        );

        // The selected topic must not emit a click for its own row.
        let pos = frame
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    if text.galley.text() == "Rumors" {
                        return Some(text.pos + text.galley.size() * 0.5);
                    }
                }
                None
            })
            .expect("the owned-topic list must render every entry");
        ctx.begin_pass(egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        });
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let _ = end_pass(&ctx);
        assert_eq!(
            outputs.dialogue_actions,
            vec![DialogueUiAction::SelectTopic {
                npc: 7,
                topic_form_id: 0x2012
            }]
        );
    }

    #[test]
    fn dialogue_page_close_returns_through_the_shared_resume_path() {
        let ctx = Context::default();
        let snapshot = PanelSnapshot {
            dialogue: Some(DialogueTopicSnapshot {
                npc: 7,
                speaker_name: "Eltrys".into(),
                topic_form_id: 0x2010,
                topic_name: "MS01EltrysTopic".into(),
                info_form_id: 0x2011,
                response_text: "Watch yourself.".into(),
                topics: Vec::new(),
                speaking: false,
                voiced: false,
                choices: false,
            }),
            ..Default::default()
        };
        let mut state = GameMenuState {
            visible: true,
            page: GameMenuPage::Dialogue,
            ..Default::default()
        };
        let mut outputs = PanelOutputs::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let _ = end_pass(&ctx);
        ctx.begin_pass(egui::RawInput::default());
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let frame = end_pass(&ctx);
        let pos = frame
            .shapes
            .iter()
            .find_map(|shape| {
                if let egui::Shape::Text(text) = &shape.shape {
                    if text.galley.text().starts_with("Close") {
                        return Some(text.pos + text.galley.size() * 0.5);
                    }
                }
                None
            })
            .expect("the Close button must be visible");
        ctx.begin_pass(egui::RawInput {
            events: vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            ..Default::default()
        });
        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let _ = end_pass(&ctx);
        assert!(
            outputs.resume_game,
            "Close rides the pause menu's shared resume path"
        );
        assert!(
            outputs.dialogue_actions.is_empty(),
            "closing must not emit a topic click"
        );
    }

    #[test]
    fn inventory_category_filter_selects_a_visible_item() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        let item = |index: u32, name: &str, category: &'static str| InventoryItemView {
            index,
            form_id: 0x1000 + index,
            name: name.to_owned(),
            category,
            details: String::new(),
            count: 1,
            value: 1,
            weight: 1.0,
            equipped: false,
            equippable: false,
            consumable: false,
        };
        let snapshot = PanelSnapshot {
            inventory: Some(InventorySnapshot {
                total_weight: 2.0,
                items: vec![item(0, "Long Barrel", "Mods"), item(1, "Desk Fan", "Junk")],
            }),
            ..Default::default()
        };
        let mut state = GameMenuState {
            visible: true,
            page: GameMenuPage::Inventory,
            selected_inventory_category: "Junk".to_owned(),
            selected_inventory_index: Some(0),
            ..Default::default()
        };
        let mut outputs = PanelOutputs::default();

        draw_game_menu(&ctx, &snapshot, &mut state, &mut outputs);
        let output = end_pass(&ctx);

        assert!(!output.shapes.is_empty());
        assert_eq!(state.selected_inventory_category, "Junk");
        assert_eq!(state.selected_inventory_index, Some(1));
    }
}
