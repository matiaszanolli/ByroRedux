//! `hud.*` — MenuXml/Scaleform HUD console control (M48.4 Oblivion /
//! M48.5 FO3 / M48.6 Skyrim).
//!
//! The HUD renderer lives in the frame loop (see `hud.rs`'s module docs);
//! these commands mutate the [`HudControl`] World resource it reads every
//! frame. `hud.status` is the smoke-gate observable: it reports whether a
//! HUD is live even when hidden, because the resource exists only after a
//! successful `--hud` launch. Bar count and labels are the launched
//! game's (3 for Oblivion/Skyrim, 2 for FO3/FNV), and `hud.status` /
//! `hud.debug` name the backend that owns the overlay.

use super::shared::*;
use crate::hud::HudControl;
use crate::scaleform_hud::ScaleformHudDiag;

/// `hud.on` — show the HUD overlay.
pub(crate) struct HudOnCommand;

impl ConsoleCommand for HudOnCommand {
    fn name(&self) -> &str {
        "hud.on"
    }
    fn description(&self) -> &str {
        "Show the HUD overlay (MenuXml or Scaleform backend)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        match world.try_resource_mut::<HudControl>() {
            Some(mut control) => {
                control.visible = true;
                CommandOutput::line("hud: visible")
            }
            None => CommandOutput::error("hud: not launched (start with --hud)"),
        }
    }
}

/// `hud.off` — hide the HUD overlay (stops the UI quad entirely).
pub(crate) struct HudOffCommand;

impl ConsoleCommand for HudOffCommand {
    fn name(&self) -> &str {
        "hud.off"
    }
    fn description(&self) -> &str {
        "Hide the HUD overlay (MenuXml or Scaleform backend)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        match world.try_resource_mut::<HudControl>() {
            Some(mut control) => {
                control.visible = false;
                CommandOutput::line("hud: hidden")
            }
            None => CommandOutput::error("hud: not launched (start with --hud)"),
        }
    }
}

/// `hud.values <f0> [f1] [f2]` — pin bar fractions (0..1), one per the
/// game's bars; `hud.values auto` returns them to actor-value
/// derivation.
pub(crate) struct HudValuesCommand;

impl ConsoleCommand for HudValuesCommand {
    fn name(&self) -> &str {
        "hud.values"
    }
    fn description(&self) -> &str {
        "Pin HUD bar fractions: hud.values <0-1>... | auto"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(mut control) = world.try_resource_mut::<HudControl>() else {
            return CommandOutput::error("hud: not launched (start with --hud)");
        };
        let count = control.bar_count as usize;
        let labels = control.bar_labels;
        let tokens: Vec<&str> = args.split_whitespace().collect();
        if tokens.len() == 1 && tokens[0].eq_ignore_ascii_case("auto") {
            control.bars = [None; 3];
            return CommandOutput::line("hud: bars follow actor values");
        }
        if tokens.len() != count {
            let usage = labels[..count].join(" ");
            return CommandOutput::error(format!(
                "usage: hud.values <{usage}> (one 0-1 fraction per bar) | auto"
            ));
        }
        let parse = |t: &str| -> Result<f32, ()> {
            t.parse::<f32>().map(|v| v.clamp(0.0, 1.0)).map_err(|_| ())
        };
        let parsed: Vec<Result<f32, ()>> = tokens.iter().map(|t| parse(t)).collect();
        if parsed.iter().any(|p| p.is_err()) {
            return CommandOutput::error("hud.values: 0-1 fractions or 'auto'");
        }
        let values: Vec<f32> = parsed.into_iter().map(|p| p.unwrap()).collect();
        for (slot, value) in values.iter().enumerate() {
            control.bars[slot] = Some(*value);
        }
        let joined = values
            .iter()
            .map(|v| format!("{v:.2}"))
            .collect::<Vec<_>>()
            .join("/");
        CommandOutput::line(format!("hud: bars pinned to {joined}"))
    }
}

/// `hud.heading <degrees>` — pin the compass; `hud.heading auto` follows
/// the camera.
pub(crate) struct HudHeadingCommand;

impl ConsoleCommand for HudHeadingCommand {
    fn name(&self) -> &str {
        "hud.heading"
    }
    fn description(&self) -> &str {
        "Pin the compass heading: hud.heading <0-360> | auto"
    }
    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(mut control) = world.try_resource_mut::<HudControl>() else {
            return CommandOutput::error("hud: not launched (start with --hud)");
        };
        let tokens: Vec<&str> = args.split_whitespace().collect();
        let Some(first) = tokens.first() else {
            return CommandOutput::error("usage: hud.heading <0-360> | auto");
        };
        if first.eq_ignore_ascii_case("auto") {
            control.heading = None;
            return CommandOutput::line("hud: compass follows the camera");
        }
        match first.parse::<f32>() {
            Ok(deg) => {
                let deg = deg.rem_euclid(360.0);
                control.heading = Some(deg);
                CommandOutput::line(format!("hud: compass pinned to {deg:.0}°"))
            }
            Err(_) => CommandOutput::error("hud.heading: degrees 0-360 or 'auto'"),
        }
    }
}

/// `hud.status` — the smoke-gate observable.
pub(crate) struct HudStatusCommand;

impl ConsoleCommand for HudStatusCommand {
    fn name(&self) -> &str {
        "hud.status"
    }
    fn description(&self) -> &str {
        "Report the HUD state (MenuXml or Scaleform backend)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        match world.try_resource::<HudControl>() {
            Some(control) => {
                // #4675 — the auto arm reports the LIVE derived fraction
                // (the driver writes `live` every frame) instead of a
                // constant "1.00 (auto)" no observable could read.
                let fmt = |pinned: Option<f32>, live: Option<f32>| match pinned {
                    Some(pinned) => format!("{pinned:.2} (pinned)"),
                    None => match live {
                        Some(live) => format!("{live:.2} (auto)"),
                        None => "1.00 (auto)".to_string(),
                    },
                };
                let bars = (0..control.bar_count as usize)
                    .map(|i| {
                        format!(
                            "{}={}",
                            control.bar_labels[i],
                            fmt(control.bars[i], control.live[i])
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                // `backend` goes last so the m48-4/m48-5 gates' grep
                // prefixes (`hud: launched visible=true…`) survive the
                // M48.6 addition.
                CommandOutput::line(format!(
                    "hud: launched visible={} {bars} heading={} backend={}",
                    control.visible,
                    match control.heading {
                        Some(deg) => format!("{deg:.0} (pinned)"),
                        None => "auto".to_string(),
                    },
                    control.backend.as_str(),
                ))
            }
            None => CommandOutput::error("hud: not launched (start with --hud)"),
        }
    }
}

/// `hud.debug` — Scaleform HUD bridge diagnostics: the callbacks the
/// movie registered (legal push targets), the host methods it called
/// with no handler, and the last pushed frame's values. MenuXml-only
/// runs have no bridge to inspect.
pub(crate) struct HudDebugCommand;

impl ConsoleCommand for HudDebugCommand {
    fn name(&self) -> &str {
        "hud.debug"
    }
    fn description(&self) -> &str {
        "Dump the Scaleform HUD bridge diagnostics (Skyrim --hud)"
    }
    fn execute(&self, world: &World, _args: &str) -> CommandOutput {
        match world.try_resource::<ScaleformHudDiag>() {
            Some(diag) => {
                let list = |name: &str, items: &[String]| -> String {
                    if items.is_empty() {
                        format!("{name}: (none)")
                    } else {
                        format!("{name}: {}", items.join(", "))
                    }
                };
                let bars = diag
                    .last_push
                    .map(|(a, b, c, deg)| {
                        let count = diag.bar_count as usize;
                        let values = [a, b, c];
                        let labels = (0..count)
                            .map(|i| {
                                format!("{}={:.2}", diag.bar_labels[i], values[i])
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        format!("{labels} heading={deg:.1}")
                    })
                    .unwrap_or_else(|| "(never)".to_string());
                CommandOutput::line(format!(
                    "hud.debug (scaleform, {}): {}\n{}\n{}\nlast_push: {}\nrender_passes: {}",
                    diag.game,
                    list("callbacks", &diag.callbacks),
                    list("unknown_methods", &diag.unknown_methods),
                    list("unanswered_methods", &diag.unanswered_methods),
                    bars,
                    diag.render_passes,
                ))
            }
            None => {
                if world.try_resource::<HudControl>().is_some() {
                    CommandOutput::error("hud.debug: the Scaleform HUD has no host bridge to mirror")
                } else {
                    CommandOutput::error("hud: not launched (start with --hud)")
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hud_world() -> World {
        let mut world = World::new();
        world.insert_resource(HudControl::default());
        world
    }

    /// `CommandOutput::error` is a "Error: "-prefixed line, not a variant.
    fn assert_error(output: CommandOutput, needle: &str) {
        let first = output
            .lines
            .first()
            .unwrap_or_else(|| panic!("expected an error line, got empty output"));
        assert!(
            first.starts_with("Error: ") && first.contains(needle),
            "`{first}` must be an error naming {needle}"
        );
    }

    /// #4724 — `hud.values` parsing: correct-count pins land, `auto`
    /// clears, the count and the 0-1 domain are enforced, and the
    /// not-launched arm still reports.
    #[test]
    fn hud_values_pins_autos_and_rejects_bad_input() {
        let world = hud_world();
        let cmd = HudValuesCommand;

        // Default control carries three bars (Oblivion labels). Reads are
        // scoped so the lock tracker never sees a read guard across the
        // next execute's write.
        let out = cmd.execute(&world, "0.25 0.5 0.75");
        assert!(!out.lines.is_empty() && !out.lines[0].starts_with("Error: "));
        {
            let bars = world.resource::<HudControl>().bars;
            assert_eq!(
                bars,
                [Some(0.25), Some(0.5), Some(0.75)],
                "three pins land in slot order"
            );
        }

        cmd.execute(&world, "auto");
        {
            let bars = world.resource::<HudControl>().bars;
            assert_eq!(bars, [None; 3], "`auto` returns the bars to actor-value derivation");
        }

        // Wrong count.
        assert_error(cmd.execute(&world, "0.5"), "usage");
        assert_error(cmd.execute(&world, "0.1 0.2 0.3 0.4"), "usage");
        // Out of the fraction domain.
        assert_error(cmd.execute(&world, "0.5 oops 0.5"), "0-1 fractions");
        // Not launched.
        let bare = World::new();
        assert_error(cmd.execute(&bare, "0.5 0.5 0.5"), "not launched");
    }

    /// #4724 — `hud.heading` parsing: pins wrap into 0..360, `auto`
    /// clears, junk is rejected.
    #[test]
    fn hud_heading_pins_wraps_and_rejects_junk() {
        let world = hud_world();
        let cmd = HudHeadingCommand;

        cmd.execute(&world, "450");
        assert_eq!(
            world.resource::<HudControl>().heading,
            Some(90.0),
            "degrees wrap into 0..360"
        );
        cmd.execute(&world, "auto");
        assert_eq!(world.resource::<HudControl>().heading, None);

        assert_error(cmd.execute(&world, ""), "usage");
        assert_error(cmd.execute(&world, "north"), "degrees 0-360");
    }
}
