//! The in-game HUD the shipped game draws every frame: crosshair, interaction prompt, vitals, objectives, loading tip and player message (#6578, split from `panels.rs`).

use super::*;
use egui::{Align2, Color32, Context, CornerRadius, FontId, Frame, Id, Margin, Order, RichText, Stroke};

/// Draw the small gameplay HUD layer shared with the debug renderer.
pub fn draw_hud(ctx: &Context, snapshot: &PanelSnapshot) {
    if snapshot.show_crosshair {
        let center = ctx.content_rect().center();
        let painter = ctx.layer_painter(egui::LayerId::new(
            Order::Middle,
            Id::new("gameplay_crosshair"),
        ));
        let stroke = Stroke::new(1.5_f32, Color32::from_white_alpha(220));
        for (from, to) in [
            (egui::vec2(-7.0, 0.0), egui::vec2(-2.0, 0.0)),
            (egui::vec2(2.0, 0.0), egui::vec2(7.0, 0.0)),
            (egui::vec2(0.0, -7.0), egui::vec2(0.0, -2.0)),
            (egui::vec2(0.0, 2.0), egui::vec2(0.0, 7.0)),
        ] {
            painter.line_segment([center + from, center + to], stroke);
        }
    }

    if snapshot.show_vitals {
        if let Some(vitals) = snapshot.vitals.as_ref() {
            draw_vitals_bars(ctx, vitals);
        }
    }

    if snapshot.show_objectives {
        if let Some(objectives) = snapshot.objectives.as_ref() {
            draw_objective_lines(ctx, objectives);
        }
    }

    let Some(prompt) = snapshot
        .show_prompts
        .then_some(snapshot.interaction_prompt)
        .flatten()
    else {
        return;
    };

    egui::Area::new(Id::new("interaction_prompt"))
        .anchor(Align2::CENTER_BOTTOM, egui::vec2(0.0, -64.0))
        .interactable(false)
        .show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_black_alpha(190))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(format!("[{}] {}", prompt.binding, prompt.verb))
                            .size(20.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                });
        });
}

/// Fill color for one vitals bar, keyed on its display label. The labels are
/// the per-game vitals vocabulary (`Health`/`Magicka`/`Stamina`,
/// `Fatigue`, `HP`/`AP`), so the match is presentation-only — an unknown
/// label degrades to a neutral color rather than failing to draw.
fn vital_color(label: &str) -> Color32 {
    match label.to_ascii_lowercase().as_str() {
        "magicka" => Color32::from_rgb(72, 110, 220),
        "stamina" | "fatigue" => Color32::from_rgb(74, 168, 88),
        "hp" | "health" => Color32::from_rgb(198, 58, 48),
        "ap" | "actionpoints" | "o2" => Color32::from_rgb(212, 176, 62),
        // The FO3 / FNV rad meter: a radiation-sign green, distinct from AP's gold.
        "rad" | "rads" => Color32::from_rgb(132, 204, 54),
        _ => Color32::from_rgb(170, 170, 170),
    }
}

/// Draw the player's vitals bars, bottom-left — the presentation consumer of
/// the player's canonical `ActorValues`. Purely presentational: the producer
/// already composed `current`/`max`; zero-`max` bars never arrive here.
fn draw_vitals_bars(ctx: &Context, vitals: &[VitalBarView]) {
    if vitals.is_empty() {
        return;
    }
    const BAR_WIDTH: f32 = 240.0;
    const BAR_HEIGHT: f32 = 17.0;
    const GAP: f32 = 7.0;
    egui::Area::new(Id::new("vitals_bars"))
        .anchor(Align2::LEFT_BOTTOM, egui::vec2(18.0, -18.0))
        .interactable(false)
        .show(ctx, |ui| {
            for bar in vitals {
                let fraction = if bar.max > 0.0 {
                    (bar.current / bar.max).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let (rect, _) =
                    ui.allocate_exact_size(egui::vec2(BAR_WIDTH, BAR_HEIGHT), egui::Sense::hover());
                let painter = ui.painter();
                painter.rect_filled(rect, CornerRadius::same(4), Color32::from_black_alpha(170));
                let inset = egui::vec2(2.0, 2.0);
                let fill_width = (rect.width() - 2.0 * inset.x) * fraction;
                if fill_width > 0.0 {
                    painter.rect_filled(
                        egui::Rect::from_min_size(rect.min + inset, egui::vec2(fill_width, BAR_HEIGHT - 2.0 * inset.y)),
                        CornerRadius::same(3),
                        vital_color(bar.label),
                    );
                }
                painter.text(
                    rect.left_center() + egui::vec2(7.0, 0.0),
                    Align2::LEFT_CENTER,
                    format!("{}  {:.0}/{:.0}", bar.label, bar.current, bar.max),
                    FontId::proportional(11.5),
                    Color32::from_white_alpha(235),
                );
                ui.add_space(GAP);
            }
        });
}

/// Draw the active quest objective lines, top-left — the presentation
/// consumer of `QuestStageState` + `QuestObjectiveState` +
/// `QuestDefinitionRegistry`. The producer already resolved names, flattened
/// text, ordered, and bounded the list.
fn draw_objective_lines(ctx: &Context, objectives: &[ObjectiveView]) {
    if objectives.is_empty() {
        return;
    }
    egui::Area::new(Id::new("objective_lines"))
        .anchor(Align2::LEFT_TOP, egui::vec2(18.0, 18.0))
        .interactable(false)
        .show(ctx, |ui| {
            for objective in objectives {
                ui.label(
                    RichText::new(&objective.quest)
                        .size(13.0)
                        .strong()
                        .color(Color32::from_rgb(235, 220, 160)),
                );
                ui.label(
                    RichText::new(&objective.text)
                        .size(12.5)
                        .color(Color32::WHITE),
                );
                ui.add_space(4.0);
            }
        });
}

pub fn draw_loading_tip(ctx: &Context, tip: &str) {
    // No Area fade or first-frame sizing pass: even a one-frame loading
    // presentation must show the tip at full opacity.
    let rect = ctx.content_rect();
    let painter = ctx.layer_painter(egui::LayerId::new(Order::Foreground, Id::new("game_loading_tip")));
    let text = painter.layout(tip.to_owned(), egui::FontId::proportional(22.0),
        Color32::WHITE, (rect.width() - 96.0).max(1.0));
    let size = text.size() + egui::vec2(48.0, 32.0);
    let position = egui::pos2(rect.center().x - size.x * 0.5, rect.bottom() - size.y - 35.0);
    painter.rect_filled(egui::Rect::from_min_size(position, size), 0.0, Color32::from_black_alpha(235));
    painter.galley(position + egui::vec2(24.0, 16.0), text, Color32::WHITE);
}

pub fn draw_player_message(ctx: &Context, message: &str) {
    egui::Area::new(Id::new("player_save_load_message"))
        .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 28.0))
        .interactable(false)
        .show(ctx, |ui| {
            Frame::new()
                .fill(Color32::from_black_alpha(210))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::symmetric(14, 8))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(message)
                            .size(18.0)
                            .strong()
                            .color(Color32::WHITE),
                    );
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::panels::end_pass;

    #[test]
    fn original_loading_tip_draws_without_debug_panels() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_loading_tip(&ctx, "An original game loading tip");
        assert!(!end_pass(&ctx).shapes.is_empty());
    }

    #[test]
    fn gameplay_feedback_draws_without_debug_panels() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_player_message(&ctx, "Unlocked with key\nTook 9 items");
        let output = end_pass(&ctx);
        assert!(
            !output.shapes.is_empty(),
            "gameplay feedback must produce renderable geometry"
        );
    }

    #[test]
    fn interaction_prompt_produces_hud_shapes_without_debug_panels() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_hud(
            &ctx,
            &PanelSnapshot {
                interaction_prompt: Some(InteractionPrompt {
                    binding: "E",
                    verb: "Open",
                }),
                show_prompts: true,
                ..Default::default()
            },
        );
        let output = end_pass(&ctx);
        assert!(
            !output.shapes.is_empty(),
            "a gameplay prompt must generate renderable HUD geometry"
        );
    }

    #[test]
    fn crosshair_produces_hud_shapes_without_an_interaction_target() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_hud(
            &ctx,
            &PanelSnapshot {
                show_crosshair: true,
                ..Default::default()
            },
        );
        let output = end_pass(&ctx);
        assert!(!output.shapes.is_empty());
    }

    #[test]
    fn vitals_bars_draw_during_gameplay_without_debug_panels() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_hud(
            &ctx,
            &PanelSnapshot {
                show_vitals: true,
                vitals: Some(vec![
                    VitalBarView {
                        label: "Health",
                        current: 65.0,
                        max: 100.0,
                    },
                    VitalBarView {
                        label: "Magicka",
                        current: 50.0,
                        max: 50.0,
                    },
                ]),
                ..Default::default()
            },
        );
        let output = end_pass(&ctx);
        assert!(
            !output.shapes.is_empty(),
            "vitals bars must generate renderable HUD geometry"
        );
    }

    #[test]
    fn vitals_setting_off_draws_no_bar_geometry_of_its_own() {
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_hud(
            &ctx,
            &PanelSnapshot {
                show_vitals: false,
                vitals: Some(vec![VitalBarView {
                    label: "Health",
                    current: 65.0,
                    max: 100.0,
                }]),
                ..Default::default()
            },
        );
        let output = end_pass(&ctx);
        assert!(
            output.shapes.is_empty(),
            "show_vitals=false must suppress the bars entirely, not merely their fill"
        );
    }

    #[test]
    fn objective_lines_draw_during_gameplay_without_debug_panels() {
        let ctx = Context::default();
        let snapshot = PanelSnapshot {
            show_objectives: true,
            objectives: Some(vec![ObjectiveView {
                quest: "A Testable Errand".to_owned(),
                text: "Recover the golden claw".to_owned(),
            }]),
            ..Default::default()
        };
        ctx.begin_pass(egui::RawInput::default());
        draw_hud(&ctx, &snapshot);
        // egui sizes a newly opened area before painting its text contents
        // (same sizing pass the Use-button test below works around), so the
        // label geometry only appears on the second pass.
        let _ = end_pass(&ctx);
        ctx.begin_pass(egui::RawInput::default());
        draw_hud(&ctx, &snapshot);
        let output = end_pass(&ctx);
        assert!(
            !output.shapes.is_empty(),
            "objective text must generate renderable HUD geometry"
        );
    }
}
