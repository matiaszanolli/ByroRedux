//! The in-game HUD the shipped game draws every frame: crosshair, interaction prompt, vitals, objectives, detection indicator, loading tip and player message (#6578, split from `panels.rs`).

use super::*;
use egui::{Align2, Color32, Context, CornerRadius, FontId, Frame, Id, Margin, Order, RichText, Stroke};

/// Draw the small gameplay HUD layer shared with the debug renderer.
pub fn draw_hud(ctx: &Context, snapshot: &PageSnapshot) {
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

    if let Some(indicator) = snapshot.stealth_indicator.as_ref() {
        draw_stealth_indicator(ctx, indicator);
    }

    let Some(prompt) = snapshot
        .show_prompts
        .then_some(snapshot.interaction_prompt.as_ref())
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
/// A bar's colour by its actor value (#6825: not by the label, which is the game's word).
fn vital_color(actor_value: &str) -> Color32 {
    match actor_value {
        "Magicka" => Color32::from_rgb(72, 110, 220),
        "Stamina" | "Fatigue" => Color32::from_rgb(74, 168, 88),
        "Health" => Color32::from_rgb(198, 58, 48),
        "ActionPoints" | "Oxygen" => Color32::from_rgb(212, 176, 62),
        // The FO3 / FNV rad meter: a radiation-sign green, distinct from AP's gold.
        "RadiationRads" => Color32::from_rgb(132, 204, 54),
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
                        vital_color(bar.actor_value),
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

/// The danger state's flash: `Fallout3.exe` restarts a 0.5 s alpha animation on
/// the meter while it shows `sSneakDanger` (`0x937bd2`).
const DANGER_FLASH_SECS: f64 = 0.5;

/// Draw the detection indicator just above the crosshair (SNK-HUD-001) — the
/// presentation consumer of the detection tick's folded state. A label in the
/// game's vocabulary, or Oblivion's eye.
fn draw_stealth_indicator(ctx: &Context, indicator: &StealthIndicatorView) {
    use byroredux_core::stealth::IndicatorState;
    let alpha = if indicator.state == IndicatorState::Danger {
        let phase = (ctx.input(|i| i.time) / DANGER_FLASH_SECS) * std::f64::consts::PI;
        ctx.request_repaint();
        (0.35 + 0.65 * phase.sin().abs()) as f32
    } else {
        1.0
    };
    let center = ctx.content_rect().center() + egui::vec2(0.0, -58.0);
    let painter = ctx.layer_painter(egui::LayerId::new(Order::Middle, Id::new("stealth_indicator")));
    match &indicator.look {
        StealthIndicatorLook::Label(label) => {
            let color = match indicator.state {
                IndicatorState::Hidden => Color32::from_rgb(225, 225, 225),
                IndicatorState::Detected => Color32::from_rgb(238, 214, 120),
                IndicatorState::Caution => Color32::from_rgb(240, 156, 60),
                IndicatorState::Danger => Color32::from_rgb(232, 64, 52),
            };
            let galley = painter.layout_no_wrap(label.to_string(), FontId::proportional(18.0), color.gamma_multiply(alpha));
            let size = galley.size() + egui::vec2(20.0, 8.0);
            let rect = egui::Rect::from_center_size(center, size);
            painter.rect_filled(rect, CornerRadius::same(5), Color32::from_black_alpha(150));
            painter.galley(rect.min + egui::vec2(10.0, 4.0), galley, Color32::WHITE);
        }
        StealthIndicatorLook::Eye => {
            let lit = indicator.state > IndicatorState::Hidden;
            let tone = if lit { Color32::from_white_alpha(240) } else { Color32::from_white_alpha(70) };
            let stroke = Stroke::new(2.0, tone);
            // An almond outline: upper and lower lids as polylines.
            let lid = |sign: f32| -> Vec<egui::Pos2> {
                (0..=16)
                    .map(|i| {
                        let t = i as f32 / 16.0 * 2.0 - 1.0;
                        center + egui::vec2(18.0 * t, sign * 9.0 * (1.0 - t * t))
                    })
                    .collect()
            };
            painter.add(egui::Shape::line(lid(-1.0), stroke));
            painter.add(egui::Shape::line(lid(1.0), stroke));
            if lit {
                painter.circle_filled(center, 5.0, tone);
            }
        }
    }
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
    use crate::end_pass;

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
            &PageSnapshot {
                interaction_prompt: Some(InteractionPrompt {
                    binding: "E",
                    verb: "Open".into(),
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
            &PageSnapshot {
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
            &PageSnapshot {
                show_vitals: true,
                vitals: Some(vec![
                    VitalBarView {
                        label: "Health".into(),
                        actor_value: "Health",
                        current: 65.0,
                        max: 100.0,
                    },
                    VitalBarView {
                        label: "Magicka".into(),
                        actor_value: "Magicka",
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
            &PageSnapshot {
                show_vitals: false,
                vitals: Some(vec![VitalBarView {
                    label: "Health".into(),
                        actor_value: "Health",
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
    fn the_stealth_indicator_draws_in_each_look_and_not_without_a_view() {
        use byroredux_core::stealth::IndicatorState;
        for look in [StealthIndicatorLook::Label("[HIDDEN]".into()), StealthIndicatorLook::Eye] {
            for state in IndicatorState::ALL {
                let ctx = Context::default();
                ctx.begin_pass(egui::RawInput::default());
                draw_hud(
                    &ctx,
                    &PageSnapshot {
                        stealth_indicator: Some(StealthIndicatorView { state, look: look.clone() }),
                        ..Default::default()
                    },
                );
                assert!(!end_pass(&ctx).shapes.is_empty(), "{look:?} {state:?}");
            }
        }
        let ctx = Context::default();
        ctx.begin_pass(egui::RawInput::default());
        draw_hud(&ctx, &PageSnapshot::default());
        assert!(end_pass(&ctx).shapes.is_empty(), "no view outside the stance draws nothing");
    }

    #[test]
    fn objective_lines_draw_during_gameplay_without_debug_panels() {
        let ctx = Context::default();
        let snapshot = PageSnapshot {
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
