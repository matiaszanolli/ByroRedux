//! Game clock inspection and controls.

use super::shared::*;

use crate::components::{GameTimeRes, SkyParamsRes, WeatherDataRes};
use crate::systems::weather::DEFAULT_TOD_HOURS;

fn no_args(args: &str, usage: &str) -> Result<(), CommandOutput> {
    if args.trim().is_empty() {
        Ok(())
    } else {
        Err(CommandOutput::error(format!("usage: {usage}")))
    }
}

fn one_arg<'a>(args: &'a str, usage: &str) -> Result<&'a str, CommandOutput> {
    let mut tokens = args.split_whitespace();
    let Some(value) = tokens.next() else {
        return Err(CommandOutput::error(format!("usage: {usage}")));
    };
    if tokens.next().is_some() {
        return Err(CommandOutput::error(format!("usage: {usage}")));
    }
    Ok(value)
}

fn parse_hour(value: &str) -> Option<f32> {
    let hour = if let Some((hour, minute)) = value.split_once(':') {
        let hour = hour.parse::<u32>().ok()?;
        let minute = minute.parse::<u32>().ok()?;
        if hour >= 24 || minute >= 60 {
            return None;
        }
        hour as f32 + minute as f32 / 60.0
    } else {
        value.parse::<f32>().ok()?
    };
    (hour.is_finite() && (0.0..24.0).contains(&hour)).then_some(hour)
}

fn format_clock(hour: f32) -> String {
    let hour = hour.rem_euclid(24.0);
    let whole_hour = hour.floor() as u32;
    let minute = ((hour - hour.floor()) * 60.0).floor() as u32;
    format!("{whole_hour:02}:{minute:02}")
}

fn phase(hour: f32, tod_hours: [f32; 4]) -> &'static str {
    let [sunrise_begin, sunrise_end, sunset_begin, sunset_end] = tod_hours;
    if hour < sunrise_begin || hour >= sunset_end {
        "night"
    } else if hour < sunrise_end {
        "sunrise"
    } else if hour < sunset_begin {
        "day"
    } else {
        "sunset"
    }
}

fn current_tod_hours(world: &World) -> [f32; 4] {
    world
        .try_resource::<WeatherDataRes>()
        .map_or(DEFAULT_TOD_HOURS, |weather| weather.tod_hours)
}

fn resample_lighting(world: &World) {
    crate::systems::weather_system(world, 0.0);
}

fn mutation_output(world: &World, action: &str) -> CommandOutput {
    let time = *world.resource::<GameTimeRes>();
    let tod_hours = current_tod_hours(world);
    CommandOutput::line(format!(
        "{action}: day={} hour={:.3} clock={} phase={} scale={:.3}x paused={}",
        time.day,
        time.hour,
        format_clock(time.hour),
        phase(time.hour, tod_hours),
        time.time_scale,
        time.is_paused()
    ))
}

pub(crate) struct TimeShowCommand;

impl ConsoleCommand for TimeShowCommand {
    fn name(&self) -> &str {
        "time.show"
    }

    fn description(&self) -> &str {
        "Show the persistent game clock, climate schedule, phase, rate, and sun state"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        if let Err(error) = no_args(args, "time.show") {
            return error;
        }
        let time = match world.try_resource::<GameTimeRes>() {
            Some(time) => *time,
            None => {
                return CommandOutput::error(
                    "game clock unavailable — scene runtime not initialized",
                )
            }
        };
        let tod_hours = current_tod_hours(world);
        let [sunrise_begin, sunrise_end, sunset_begin, sunset_end] = tod_hours;
        let mut lines = vec!["Game time:".to_string()];
        lines.push(format!("  day: {}", time.day));
        lines.push(format!(
            "  clock: {} (hour={:.3}, phase={})",
            format_clock(time.hour),
            time.hour,
            phase(time.hour, tod_hours)
        ));
        lines.push(format!(
            "  scale: {:.3}x game-sec/real-sec ({})",
            time.time_scale,
            if time.is_paused() {
                "paused"
            } else {
                "running"
            }
        ));
        lines.push(format!(
            "  climate: sunrise={sunrise_begin:.2}-{sunrise_end:.2} sunset={sunset_begin:.2}-{sunset_end:.2}"
        ));
        match world.try_resource::<SkyParamsRes>() {
            Some(sky) => lines.push(format!(
                "  sun: intensity={:.3} direction=[{:.3}, {:.3}, {:.3}]",
                sky.sun_intensity, sky.sun_direction[0], sky.sun_direction[1], sky.sun_direction[2]
            )),
            None => lines.push("  sun: unavailable (no exterior sky)".to_string()),
        }
        CommandOutput::lines(lines)
    }
}

pub(crate) struct TimeSetCommand;

impl ConsoleCommand for TimeSetCommand {
    fn name(&self) -> &str {
        "time.set"
    }

    fn description(&self) -> &str {
        "Set time of day: time.set <0..24|HH:MM>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let value = match one_arg(args, "time.set <0..24|HH:MM>") {
            Ok(value) => value,
            Err(error) => return error,
        };
        let Some(hour) = parse_hour(value) else {
            return CommandOutput::error(format!(
                "invalid time `{value}` (expected 0..24 or HH:MM)"
            ));
        };
        let Some(mut time) = world.try_resource_mut::<GameTimeRes>() else {
            return CommandOutput::error("game clock unavailable — scene runtime not initialized");
        };
        time.set_hour(hour);
        drop(time);
        resample_lighting(world);
        mutation_output(world, "time.set")
    }
}

pub(crate) struct TimeScaleCommand;

impl ConsoleCommand for TimeScaleCommand {
    fn name(&self) -> &str {
        "time.scale"
    }

    fn description(&self) -> &str {
        "Set game seconds per real second; zero pauses: time.scale <factor>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let value = match one_arg(args, "time.scale <factor>") {
            Ok(value) => value,
            Err(error) => return error,
        };
        let Some(scale) = value
            .parse::<f32>()
            .ok()
            .filter(|scale| scale.is_finite() && *scale >= 0.0)
        else {
            return CommandOutput::error(format!("invalid time scale `{value}`"));
        };
        let Some(mut time) = world.try_resource_mut::<GameTimeRes>() else {
            return CommandOutput::error("game clock unavailable — scene runtime not initialized");
        };
        time.set_time_scale(scale);
        drop(time);
        mutation_output(world, "time.scale")
    }
}

pub(crate) struct TimePauseCommand;

impl ConsoleCommand for TimePauseCommand {
    fn name(&self) -> &str {
        "time.pause"
    }

    fn description(&self) -> &str {
        "Pause the game clock while retaining its prior rate"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        if let Err(error) = no_args(args, "time.pause") {
            return error;
        }
        let Some(mut time) = world.try_resource_mut::<GameTimeRes>() else {
            return CommandOutput::error("game clock unavailable — scene runtime not initialized");
        };
        time.pause();
        drop(time);
        mutation_output(world, "time.pause")
    }
}

pub(crate) struct TimeResumeCommand;

impl ConsoleCommand for TimeResumeCommand {
    fn name(&self) -> &str {
        "time.resume"
    }

    fn description(&self) -> &str {
        "Resume the game clock at its last non-zero rate"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        if let Err(error) = no_args(args, "time.resume") {
            return error;
        }
        let Some(mut time) = world.try_resource_mut::<GameTimeRes>() else {
            return CommandOutput::error("game clock unavailable — scene runtime not initialized");
        };
        time.resume();
        drop(time);
        mutation_output(world, "time.resume")
    }
}

pub(crate) struct TimeAdvanceCommand;

impl ConsoleCommand for TimeAdvanceCommand {
    fn name(&self) -> &str {
        "time.advance"
    }

    fn description(&self) -> &str {
        "Advance by game hours, carrying whole days: time.advance <hours>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let value = match one_arg(args, "time.advance <hours>") {
            Ok(value) => value,
            Err(error) => return error,
        };
        let Some(hours) = value
            .parse::<f32>()
            .ok()
            .filter(|hours| hours.is_finite() && *hours >= 0.0)
        else {
            return CommandOutput::error(format!("invalid hour delta `{value}`"));
        };
        let Some(mut time) = world.try_resource_mut::<GameTimeRes>() else {
            return CommandOutput::error("game clock unavailable — scene runtime not initialized");
        };
        time.advance_hours(hours);
        drop(time);
        resample_lighting(world);
        mutation_output(world, "time.advance")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with_time() -> World {
        let mut world = World::new();
        world.insert_resource(GameTimeRes::default());
        world
    }

    #[test]
    fn set_accepts_clock_syntax_and_advance_carries_days() {
        let world = world_with_time();
        let set = TimeSetCommand.execute(&world, "23:30").lines.join("\n");
        assert!(set.contains("hour=23.500 clock=23:30 phase=night"));

        let advanced = TimeAdvanceCommand.execute(&world, "25").lines.join("\n");
        assert!(advanced.contains("day=2 hour=0.500 clock=00:30"));
    }

    #[test]
    fn pause_resume_retains_the_last_explicit_scale() {
        let world = world_with_time();
        TimeScaleCommand.execute(&world, "120");
        let paused = TimePauseCommand.execute(&world, "").lines.join("\n");
        assert!(paused.contains("scale=0.000x paused=true"));
        let resumed = TimeResumeCommand.execute(&world, "").lines.join("\n");
        assert!(resumed.contains("scale=120.000x paused=false"));
    }

    #[test]
    fn show_and_mutators_reject_missing_runtime_or_bad_input() {
        let empty = World::new();
        assert!(TimeShowCommand.execute(&empty, "").lines[0].contains("unavailable"));

        let world = world_with_time();
        assert!(TimeSetCommand.execute(&world, "24:00").lines[0].contains("invalid time"));
        assert!(TimeScaleCommand.execute(&world, "-1").lines[0].contains("invalid time scale"));
        assert!(TimeAdvanceCommand.execute(&world, "-2").lines[0].contains("invalid hour delta"));
    }
}
