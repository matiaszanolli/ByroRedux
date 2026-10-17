//! `anim.event` - `ObjectReference.PlayAnimation` from the console.

use super::shared::*;

/// `anim.event <entity|.> <event>`: send an animation event to the reference's behavior
/// graph (a placed prop's, or - on Fallout 4 - an actor's); prints whether the graph
/// accepted it and the animation now playing.
pub(crate) struct AnimEventCommand;

impl ConsoleCommand for AnimEventCommand {
    fn name(&self) -> &str {
        "anim.event"
    }

    fn description(&self) -> &str {
        "PlayAnimation from the console: anim.event <entity|.> <event>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut tokens = args.split_whitespace();
        let (Some(entity_token), Some(event)) = (tokens.next(), tokens.next()) else {
            return CommandOutput::error("usage: anim.event <entity|.> <event>");
        };
        let entity = match resolve_console_entity(world, entity_token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("anim.event: {message}")),
        };
        if !byroredux_scripting::animation_graph::has_graph(world, entity) {
            return CommandOutput::error("anim.event: the reference runs no behavior graph");
        }
        let accepted = byroredux_scripting::animation_graph::play_animation(world, entity, event);
        let clip = world
            .get::<byroredux_core::animation::AnimationPlayer>(entity)
            .map(|player| player.clip_handle);
        let layers = world.get::<byroredux_core::animation::AnimationStack>(entity).map(|stack| stack.layers.len());
        CommandOutput::line(format!(
            "{event}: {} (clip {clip:?}, blended layers {layers:?})",
            if accepted { "accepted" } else { "rejected" }
        ))
    }
}

/// `anim.var <entity|.> [name [value]]`: the reference's behavior-graph variables
/// (`GetAnimationVariable*` / `SetAnimationVariable*` from the console) - all of them, one,
/// or one set.
pub(crate) struct AnimVarCommand;

impl ConsoleCommand for AnimVarCommand {
    fn name(&self) -> &str {
        "anim.var"
    }

    fn description(&self) -> &str {
        "Behavior-graph variables: anim.var <entity|.> [name [value]]"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut tokens = args.split_whitespace();
        let Some(entity_token) = tokens.next() else {
            return CommandOutput::error("usage: anim.var <entity|.> [name [value]]");
        };
        let entity = match resolve_console_entity(world, entity_token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("anim.var: {message}")),
        };
        let (name, value) = (tokens.next(), tokens.next());
        match (name, value) {
            (None, _) => {
                let Some(graphs) = world.query::<byroredux_scripting::ObjectAnimationGraph>() else {
                    return CommandOutput::error("anim.var: the reference runs no behavior graph");
                };
                let Some(graph) = graphs.get(entity) else {
                    return CommandOutput::error("anim.var: the reference runs no behavior graph");
                };
                CommandOutput::lines(graph.variables().map(|(name, value)| format!("{name} = {value}")).collect())
            }
            (Some(name), None) => match byroredux_scripting::animation_graph::variable_value(world, entity, name) {
                Some(value) => CommandOutput::line(format!("{name} = {value}")),
                None => CommandOutput::error(format!("anim.var: no variable `{name}` on that graph")),
            },
            (Some(name), Some(value)) => {
                let Ok(value) = value.parse::<f32>() else {
                    return CommandOutput::error(format!("anim.var: `{value}` is not a number"));
                };
                if byroredux_scripting::animation_graph::set_variable_value(world, entity, name, value) {
                    let now = byroredux_scripting::animation_graph::variable_value(world, entity, name).unwrap_or(value);
                    CommandOutput::line(format!("{name} = {now}"))
                } else {
                    CommandOutput::error(format!("anim.var: no settable variable `{name}` on that graph"))
                }
            }
        }
    }
}

/// `anim.pose <entity|.>`: what the reference's graph is playing - the single clip, or the
/// blended layers with their live weights and speeds.
pub(crate) struct AnimPoseCommand;

impl ConsoleCommand for AnimPoseCommand {
    fn name(&self) -> &str {
        "anim.pose"
    }

    fn description(&self) -> &str {
        "The animation playing on a reference: anim.pose <entity|.>"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let Some(entity_token) = args.split_whitespace().next() else {
            return CommandOutput::error("usage: anim.pose <entity|.>");
        };
        let entity = match resolve_console_entity(world, entity_token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("anim.pose: {message}")),
        };
        if let Some(stack) = world.get::<byroredux_core::animation::AnimationStack>(entity) {
            let mut lines = vec![format!("stack of {} layers", stack.layers.len())];
            for (index, layer) in stack.layers.iter().enumerate() {
                lines.push(format!(
                    "  {index:2} clip {} weight {:.3} (fading in {:.2}s / out {:.2}s) speed {:.3} time {:.3}",
                    layer.clip_handle,
                    layer.effective_weight(),
                    layer.blend_in_remaining,
                    layer.blend_out_remaining,
                    layer.speed,
                    layer.local_time
                ));
            }
            return CommandOutput::lines(lines);
        }
        match world.get::<byroredux_core::animation::AnimationPlayer>(entity) {
            Some(player) => CommandOutput::line(format!(
                "player: clip {} speed {:.3} time {:.3}{}",
                player.clip_handle,
                player.speed,
                player.local_time,
                if player.playing { "" } else { " (held)" }
            )),
            None => CommandOutput::line("nothing playing"),
        }
    }
}

/// `actor.walk <entity|.> <speed> <seconds> [turns]`: walk an actor in a straight line at
/// `speed` units a second for `seconds`, heading `turns` of a turn clockwise from where it
/// faces (default 0, straight ahead). A stand-in mover: the graph's locomotion inputs
/// (`Speed`, `Direction`, `moveStart`/`moveStop`) follow the motion like any other.
pub(crate) struct ActorWalkCommand;

const MAX_ACTOR_WALK_SPEED: f32 = 10_000.0;
const MAX_ACTOR_WALK_SECONDS: f32 = 60.0;

fn parse_actor_walk_parameters(
    speed: &str,
    seconds: &str,
    turns: Option<&str>,
) -> Result<(f32, f32, f32), &'static str> {
    let speed = speed.parse::<f32>().map_err(|_| "speed must be a number")?;
    let seconds = seconds.parse::<f32>().map_err(|_| "seconds must be a number")?;
    if !speed.is_finite() || !seconds.is_finite() {
        return Err("speed and seconds must be finite");
    }
    if speed.abs() > MAX_ACTOR_WALK_SPEED {
        return Err("speed must be between -10000 and 10000");
    }
    let turns = match turns {
        Some(value) => {
            let turns = value.parse::<f32>().map_err(|_| "turns must be a number")?;
            if !turns.is_finite() {
                return Err("turns must be finite");
            }
            turns.rem_euclid(1.0)
        }
        None => 0.0,
    };
    Ok((speed, seconds.clamp(0.0, MAX_ACTOR_WALK_SECONDS), turns))
}

impl ConsoleCommand for ActorWalkCommand {
    fn name(&self) -> &str {
        "actor.walk"
    }

    fn description(&self) -> &str {
        "Walk an actor in a straight line: actor.walk <entity|.> <speed> <seconds> [turns]"
    }

    fn execute(&self, world: &World, args: &str) -> CommandOutput {
        let mut tokens = args.split_whitespace();
        let (Some(entity_token), Some(speed), Some(seconds)) = (tokens.next(), tokens.next(), tokens.next()) else {
            return CommandOutput::error("usage: actor.walk <entity|.> <speed> <seconds> [turns]");
        };
        let entity = match resolve_console_entity(world, entity_token) {
            Ok(entity) => entity,
            Err(message) => return CommandOutput::error(format!("actor.walk: {message}")),
        };
        let Ok((speed, seconds, turns)) =
            parse_actor_walk_parameters(speed, seconds, tokens.next())
        else {
            return CommandOutput::error(
                "actor.walk: speed, seconds, and turns must be finite numbers; speed is capped at 10000 and duration at 60s",
            );
        };
        let Some(rotation) = world
            .query::<byroredux_core::ecs::components::Transform>()
            .and_then(|transforms| transforms.get(entity).map(|transform| transform.rotation))
        else {
            return CommandOutput::error("actor.walk: the entity has no Transform");
        };
        // Clockwise from above, as `heading_fraction` measures.
        let angle = turns * std::f32::consts::TAU;
        let forward = rotation * crate::systems::GRAPH_MODEL_FORWARD;
        let right = rotation * crate::systems::GRAPH_MODEL_RIGHT;
        let velocity = (forward * angle.cos() + right * angle.sin()) * speed;
        if !velocity.is_finite() {
            return CommandOutput::error("actor.walk: computed velocity is not finite");
        }
        let Some(mut walks) = world.query_mut::<crate::systems::ConsoleWalk>() else {
            return CommandOutput::error("actor.walk: ConsoleWalk is not registered");
        };
        walks.insert(entity, crate::systems::ConsoleWalk { velocity, remaining: seconds });
        CommandOutput::line(format!("walking {entity} at {speed} for {seconds}s"))
    }
}

#[cfg(test)]
mod actor_walk_tests {
    use super::*;

    #[test]
    fn actor_walk_rejects_non_finite_values_and_caps_duration() {
        assert!(parse_actor_walk_parameters("NaN", "1", None).is_err());
        assert!(parse_actor_walk_parameters("1", "inf", None).is_err());
        assert!(parse_actor_walk_parameters("1", "1", Some("-inf")).is_err());
        assert_eq!(parse_actor_walk_parameters("2", "120", Some("1.25")), Ok((2.0, 60.0, 0.25)));
    }
}
