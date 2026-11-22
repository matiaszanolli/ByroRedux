//! Oblivion rest and level-up sessions (GUI-XML-004 through 007, #7086).

use byroredux_core::character::{CharacterLevel, SkillUseLevel};
use byroredux_core::ecs::components::{ActorValues, Dead, Seated};
use byroredux_core::ecs::{EntityId, Resource, World};
use byroredux_native_ui::{RestAction, RestAttribute, RestView};
use byroredux_plugin::esm::{
    reader::GameKind, records::oblivion_actor_values::OBLIVION_ACTOR_VALUES,
};
use byroredux_scripting::oblivion_skill_use::{
    LEVEL_PICKS, LevelUp, SleepSource, apply_level_menu_picks,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestMode {
    Bed(EntityId),
    Wait,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RestRequest {
    player: EntityId,
    mode: RestMode,
    hours: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LevelOffer {
    player: EntityId,
    level: u16,
    tallies: [u16; 8],
    picks: [bool; 8],
}

/// A single interactive rest request or the one level it authorizes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RestSession {
    #[default]
    Closed,
    Duration(RestRequest),
    Queued(RestRequest),
    LevelUp(LevelOffer),
}

impl Resource for RestSession {}

pub(crate) fn supports_rest(world: &World) -> bool {
    world
        .try_resource::<byroredux_scripting::ScriptGame>()
        .is_some_and(|game| game.0 == GameKind::Oblivion)
}

fn state(world: &World) -> RestSession {
    world
        .try_resource::<RestSession>()
        .map_or(RestSession::Closed, |session| *session)
}

fn set_state(world: &World, next: RestSession) {
    if let Some(mut session) = world.try_resource_mut::<RestSession>() {
        *session = next;
    }
}

pub(crate) fn cancel(world: &World) {
    if matches!(
        state(world),
        RestSession::Duration(_) | RestSession::LevelUp(_)
    ) {
        set_state(world, RestSession::Closed);
    }
}

fn usable_bed(world: &World, player: EntityId, bed: EntityId) -> bool {
    // Furniture ownership is authored on the reference. Weynon House has a
    // CELL owner and three beds without their own owners.
    crate::crime::is_bed(world, bed)
        && crate::inventory::foreign_owner(world, player, bed).is_none()
        && !world.query::<Seated>().is_some_and(|seats| {
            seats
                .iter()
                .any(|(actor, seat)| actor != player && seat.furniture == bed)
        })
}

fn request(world: &World, player: EntityId, mode: RestMode) {
    if state(world) != RestSession::Closed || world.has::<Dead>(player) {
        return;
    }
    if byroredux_scripting::is_in_combat(world, player) {
        crate::notifications::push(world, "You cannot rest while in combat.");
    } else if let RestMode::Bed(bed) = mode
        && !usable_bed(world, player, bed)
    {
        crate::notifications::push(world, "You cannot sleep in this bed.");
    } else {
        set_state(
            world,
            RestSession::Duration(RestRequest {
                player,
                mode,
                hours: 1,
            }),
        );
    }
}

/// Ordinary beds use the same activation event as other furniture. The jail path
/// still owns beds while the player is serving a sentence.
pub(crate) fn bed_use_system(world: &World, _dt: f32) {
    if !supports_rest(world) || byroredux_scripting::jail::player_in_jail(world) {
        return;
    }
    let Some(player) = crate::systems::PlayerEntity::get(world) else {
        return;
    };
    let targets: Vec<_> = world
        .query::<byroredux_scripting::ActivateEvent>()
        .map(|events| {
            events
                .iter()
                .filter(|(_, event)| event.activator == player)
                .map(|(entity, _)| entity)
                .collect()
        })
        .unwrap_or_default();
    if let Some(bed) = targets
        .into_iter()
        .find(|&target| crate::crime::is_bed(world, target))
    {
        request(world, player, RestMode::Bed(bed));
    }
}

/// Process one confirmed interactive request before scripted sleep. When a
/// hostile enters combat before the clock advances, the sleep ends early.
pub(crate) fn step_request(world: &World) -> bool {
    let RestSession::Queued(request) = state(world) else {
        return false;
    };
    set_state(world, RestSession::Closed);
    if crate::systems::PlayerEntity::get(world) != Some(request.player)
        || world.has::<Dead>(request.player)
    {
        return true;
    }
    let interrupted = byroredux_scripting::is_in_combat(world, request.player)
        || matches!(request.mode, RestMode::Bed(bed) if !usable_bed(world, request.player, bed));
    let hours = if interrupted {
        0.0
    } else {
        f32::from(request.hours)
    };
    if interrupted {
        crate::notifications::push(world, "Your rest was interrupted.");
    }
    match request.mode {
        RestMode::Bed(_) => crate::systems::script_requests::finish_sleep(
            world,
            SleepSource::Bed(request.player),
            hours,
            interrupted,
        ),
        RestMode::Wait if !interrupted => {
            if let Some(mut time) = world.try_resource_mut::<crate::components::GameTimeRes>() {
                time.advance_hours(hours);
            }
        }
        RestMode::Wait => {}
    }
    true
}

/// Authorize the oldest pending level for this completed bed sleep.
pub(crate) fn offer_level(world: &World, player: EntityId) {
    if state(world) != RestSession::Closed {
        return;
    }
    let Some(pending) = world
        .get::<SkillUseLevel>(player)
        .and_then(|state| state.pending.first().cloned())
    else {
        return;
    };
    let level = world
        .get::<CharacterLevel>(player)
        .map_or(1, |value| value.level);
    let mut offer = LevelOffer {
        player,
        level,
        tallies: pending.tallies,
        picks: [false; 8],
    };
    let Some(bases) = attribute_bases(world, player) else {
        return;
    };
    if bases
        .iter()
        .all(|value| value.is_none_or(|base| base >= 100.0))
    {
        crate::skill_use_emit::report_level_result(world, LevelUp::Perfection);
        return;
    }
    for code in pending.picks {
        let code = code as usize;
        if bases
            .get(code)
            .copied()
            .flatten()
            .is_some_and(|value| value < 100.0)
        {
            offer.picks[code] = true;
        }
    }
    set_state(world, RestSession::LevelUp(offer));
}

fn attribute_bases(world: &World, player: EntityId) -> Option<[Option<f32>; 8]> {
    let forms: [_; 8] = std::array::from_fn(|code| {
        byroredux_scripting::obscript_actor_values::actor_value_form(world, code as f32)
    });
    let values = world.get::<ActorValues>(player)?;
    Some(forms.map(|form| form.and_then(|form| values.get_base(form).map(|entry| entry.base))))
}

pub(crate) fn snapshot(world: &World) -> Option<RestView> {
    match state(world) {
        RestSession::Duration(request) => Some(RestView::Duration {
            sleeping: matches!(request.mode, RestMode::Bed(_)),
            hours: request.hours,
        }),
        RestSession::LevelUp(offer) => {
            let bases = attribute_bases(world, offer.player)?;
            // #7196 — the bonus bands are the ruleset's (the load order's `iLevelUpXXMult` settings).
            let leveling = world.try_resource::<byroredux_core::character::CharacterRuleset>()?.leveling;
            let selected = offer
                .picks
                .iter()
                .zip(bases)
                .filter(|(picked, base)| **picked && base.is_some_and(|value| value < 100.0))
                .count();
            let attributes = std::array::from_fn(|code| {
                let available = bases[code].is_some_and(|base| base < 100.0);
                let selected_row = available && offer.picks[code];
                RestAttribute {
                    name: OBLIVION_ACTOR_VALUES[code],
                    base: bases[code]
                        .map_or_else(|| String::from("—"), |base| format!("{base:.0}")),
                    bonus: leveling.attribute_bonus(offer.tallies[code]).unwrap_or(0),
                    selected: selected_row,
                    enabled: available && (selected_row || selected < LEVEL_PICKS),
                }
            });
            Some(RestView::LevelUp {
                level: offer.level.saturating_add(1),
                attributes: Box::new(attributes),
                selected: selected as u8,
                ready: selected == LEVEL_PICKS,
            })
        }
        _ => None,
    }
}

/// Apply a surface action. The world owns the draft, so both surfaces share
/// the eligibility checks, the confirmation transaction, and cancellation.
pub(crate) fn apply(world: &World, action: RestAction) {
    match (state(world), action) {
        (_, RestAction::Cancel) => cancel(world),
        (RestSession::Duration(mut request), RestAction::SetHours(hours)) => {
            request.hours = hours.clamp(1, 24);
            set_state(world, RestSession::Duration(request));
        }
        (RestSession::Duration(request), RestAction::AdjustHours(by)) => {
            apply(
                world,
                RestAction::SetHours(request.hours.saturating_add_signed(by)),
            );
        }
        (RestSession::Duration(request), RestAction::Confirm) => {
            set_state(world, RestSession::Queued(request))
        }
        (RestSession::LevelUp(mut offer), RestAction::ToggleAttribute(code)) => {
            if let Some(RestView::LevelUp { attributes, .. }) = snapshot(world)
                && attributes
                    .get(code as usize)
                    .is_some_and(|attribute| attribute.enabled)
            {
                let picks = &mut offer.picks;
                for (pick, attribute) in picks.iter_mut().zip(attributes.iter()) {
                    *pick = attribute.selected;
                }
                picks[code as usize] = !picks[code as usize];
                set_state(world, RestSession::LevelUp(offer));
            }
        }
        (RestSession::LevelUp(offer), RestAction::Confirm) => {
            if crate::systems::PlayerEntity::get(world) != Some(offer.player)
                || world.has::<Dead>(offer.player)
            {
                cancel(world);
                return;
            }
            let Some(RestView::LevelUp {
                attributes,
                ready: true,
                ..
            }) = snapshot(world)
            else {
                return;
            };
            let mut choices = attributes
                .iter()
                .enumerate()
                .filter_map(|(code, attribute)| attribute.selected.then_some(code as u8));
            let picks = std::array::from_fn(|_| {
                choices
                    .next()
                    .expect("the level offer has three valid picks")
            });
            let result =
                apply_level_menu_picks(world, offer.player, offer.level, offer.tallies, picks);
            if !matches!(result, LevelUp::NotReady { .. }) {
                cancel(world);
                crate::skill_use_emit::report_level_result(world, result);
            }
        }
        _ => {}
    }
}

pub(crate) fn request_wait(world: &World) {
    if supports_rest(world)
        && !byroredux_scripting::jail::player_in_jail(world)
        && let Some(player) = crate::systems::PlayerEntity::get(world)
    {
        request(world, player, RestMode::Wait);
    }
}
