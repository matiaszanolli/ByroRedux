//! Character creation: the screens a script opens while the player builds their character, and
//! what the player's answers change.
//!
//! New Vegas's opening (`VCG01`, Doc Mitchell's house) asks for the name (`ShowNameMenu`, the
//! TextEdit menu 1051), the sex and race (`ShowRaceMenu`, RaceSex 1036), the SPECIAL
//! attributes (the Vit-o-matic's `ShowLoveTesterMenuParams 40`, Love Tester 1074), three tag
//! skills (`SetTagSkills 3`, CharGen 1048) and the traits (`ShowTraitMenu`, 1084); the codes
//! are the per-game inventory's (`docs/data/game-ui/menu-inventory-fnv.tsv`). The ObScript
//! commands queue a [`byroredux_scripting::obscript_more::MenuRequest`]; this module turns the
//! character-creation ones into screens, one open at a time on the native page, reports the open
//! one's code to `MenuMode` (GUI-EVT-006), and applies what the player accepts:
//!
//! - name, sex and race to the player's [`CharacterIdentity`] (saved; the race reaches
//!   `Background` through `player_identity_sync_system`, the sex `GetIsSex` directly), and a
//!   rebuild of the player's body;
//! - SPECIAL to the attributes' base values, which the derived stats follow every frame;
//! - tag skills through `SetPlayerTagSkill`'s own path;
//! - traits to the player's `Perks`.
//!
//! Which screen is open is presentation, never saved (GUI-EVT-005).

use std::collections::VecDeque;

use byroredux_core::character::CharacterIdentity;
use byroredux_core::ecs::{Resource, World};
use byroredux_debug_ui::{ChargenChoice, ChargenOption, ChargenScreenView, ChargenSnapshot};
use byroredux_plugin::esm::records::EsmIndex;

/// The SPECIAL attributes' Fallout actor-value codes, Strength (5) to Luck (11).
const SPECIAL_CODES: std::ops::RangeInclusive<u32> = 5..=11;
/// The skills' codes, Barter (32) to Unarmed (45).
const SKILL_CODES: std::ops::RangeInclusive<u32> = 32..=45;
/// Each attribute's range: GECK *SPECIAL*, "each ranging from 1 to 10".
const SPECIAL_MIN: i32 = 1;
const SPECIAL_MAX: i32 = 10;
/// The marker a skill the game retired carries in its description: New Vegas keeps Fallout 3's
/// `AVBigGuns` with the name "Big Guns - OBSOLETE" and the description "-- OBSOLETE -- Please
/// remove.", and offers it nowhere.
const OBSOLETE_MARKER: &str = "-- OBSOLETE --";

/// One character-creation screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChargenScreen {
    Name,
    RaceSex,
    /// Spread `points` over the attributes; `0` keeps the total the attributes have.
    Special { points: u32 },
    TagSkills { count: u32 },
    Traits,
}

impl ChargenScreen {
    /// The screen an ObScript menu request names, if it is a character-creation one
    /// (`obscript_more/tail.rs` names the requests).
    pub(crate) fn for_request(menu: &str, amount: u32) -> Option<Self> {
        Some(match menu {
            "name entry" => Self::Name,
            "race" => Self::RaceSex,
            "vigor tester" | "special points" | "special book" => Self::Special { points: amount },
            "tag skills" => Self::TagSkills { count: amount.max(1) },
            "traits" => Self::Traits,
            _ => return None,
        })
    }

    /// The menu code `MenuMode` blocks and the `MenuMode` function see while it is open.
    pub(crate) fn menu_code(self) -> u32 {
        match self {
            Self::Name => 1051,
            Self::RaceSex => 1036,
            Self::Special { .. } => 1074,
            Self::TagSkills { .. } => 1048,
            Self::Traits => 1084,
        }
    }
}

/// The character-creation screens asked for and the one open. `NOT_SAVED_BY_DESIGN`: a menu.
#[derive(Debug, Default)]
pub(crate) struct ChargenSession {
    queue: VecDeque<ChargenScreen>,
    open: Option<(ChargenScreen, u64)>,
    serial: u64,
    /// An answer to the open screen given outside the page (the `chargen` console command),
    /// applied by the app on its next frame. `None` inside `Some` keeps the screen unchanged.
    answer: Option<Option<ChargenChoice>>,
}

impl Resource for ChargenSession {}

impl ChargenSession {
    pub(crate) fn request(&mut self, screen: ChargenScreen) {
        self.queue.push_back(screen);
    }

    /// The open screen.
    pub(crate) fn open(&self) -> Option<ChargenScreen> {
        self.open.map(|(screen, _)| screen)
    }

    /// Open the next screen asked for when none is open. `true` when one opened.
    pub(crate) fn open_next(&mut self) -> bool {
        if self.open.is_some() {
            return false;
        }
        let Some(screen) = self.queue.pop_front() else {
            return false;
        };
        self.serial += 1;
        self.open = Some((screen, self.serial));
        true
    }

    pub(crate) fn close(&mut self) {
        self.open = None;
    }

    /// Answer the open screen from outside the page; `None` closes it unchanged.
    pub(crate) fn answer(&mut self, choice: Option<ChargenChoice>) {
        self.answer = Some(choice);
    }

    /// The answer given outside the page, once.
    pub(crate) fn take_answer(&mut self) -> Option<Option<ChargenChoice>> {
        self.answer.take()
    }
}

/// Parse a `chargen` console answer for `screen`: `keep`, or the screen's own values —
/// `name <text>`, `race <male|female> <race hex>`, `special <seven values>`,
/// `tags <codes…>`, `traits [perk hex…]`.
pub(crate) fn parse_answer(screen: ChargenScreen, args: &str) -> Result<Option<ChargenChoice>, String> {
    let mut words = args.split_whitespace();
    let kind = words.next().unwrap_or("keep");
    let hex = |text: &str| u32::from_str_radix(text.trim_start_matches("0x"), 16).map_err(|_| format!("not a hex form: {text}"));
    let numbers = |words: std::str::SplitWhitespace| words.map(|word| word.parse::<i64>().map_err(|_| format!("not a number: {word}"))).collect::<Result<Vec<_>, _>>();
    let choice = match (kind, screen) {
        ("keep", _) => return Ok(None),
        ("name", ChargenScreen::Name) => {
            let name = args.trim_start()["name".len()..].trim();
            if name.is_empty() {
                return Err("name: give a name".into());
            }
            ChargenChoice::Name(name.to_string())
        }
        ("race", ChargenScreen::RaceSex) => {
            let female = match words.next() {
                Some("female") => true,
                Some("male") => false,
                _ => return Err("race: male|female <race hex>".into()),
            };
            ChargenChoice::RaceSex { female, race: hex(words.next().ok_or("race: missing race hex")?)? }
        }
        ("special", ChargenScreen::Special { .. }) => {
            let values = numbers(words)?;
            if values.len() != SPECIAL_CODES.clone().count() {
                return Err("special: give seven values, Strength to Luck".into());
            }
            ChargenChoice::Special(SPECIAL_CODES.zip(values).map(|(code, value)| (code, value as i32)).collect())
        }
        ("tags", ChargenScreen::TagSkills { .. }) => ChargenChoice::TagSkills(numbers(words)?.into_iter().map(|code| code as u32).collect()),
        ("traits", ChargenScreen::Traits) => ChargenChoice::Traits(words.map(hex).collect::<Result<_, _>>()?),
        (kind, screen) => return Err(format!("{kind} does not answer the open screen ({screen:?})")),
    };
    Ok(Some(choice))
}

/// The menu code of the open screen, for the `MenuMode` answer.
pub(crate) fn open_menu_code(world: &World) -> Option<u32> {
    world.try_resource::<ChargenSession>()?.open().map(ChargenScreen::menu_code)
}

/// The player's identity as it stands: the saved choices, or the player's base record before
/// any.
fn current_identity(world: &World, index: &EsmIndex, player: byroredux_core::ecs::storage::EntityId) -> CharacterIdentity {
    if let Some(identity) = world.get::<CharacterIdentity>(player) {
        return identity.clone();
    }
    let npc = index.npcs.get(&crate::inventory::player_npc_form_id(index.game));
    CharacterIdentity {
        name: npc.map(|npc| npc.full_name.clone()).unwrap_or_default(),
        female: byroredux_scripting::actor_base_traits::is_female(world, player).unwrap_or(false),
        race_form_id: world
            .get::<byroredux_core::character::Background>(player)
            .map(|background| background.race_form_id)
            .or_else(|| npc.map(|npc| byroredux_plugin::equip::traits_terminal(npc, index).race_form_id))
            .unwrap_or(0),
    }
}

/// The attribute or skill of `code`, named from its `AVIF`, with the player's value — the base one
/// for an attribute the screen spends points on (`base`), else the current one; `None` when the
/// load order has no such record or the game retired it.
fn actor_value_option(world: &World, index: &EsmIndex, values: Option<&byroredux_core::ecs::components::ActorValues>, code: u32, base: bool) -> Option<(ChargenOption, i32)> {
    let form = byroredux_scripting::obscript_actor_values::actor_value_form(world, code as f32)?;
    let record = index.actor_values.get(&form)?;
    if record.description.trim_start().starts_with(OBSOLETE_MARKER) {
        return None;
    }
    let value = values
        .map_or(0.0, |values| if base { values.get(form).map_or(0.0, |value| value.base) } else { values.current(form) })
        .round() as i32;
    Some((ChargenOption { id: code, name: record.full_name.clone(), description: record.description.clone() }, value))
}

/// The open screen and everything it shows, while one is open.
pub(crate) fn snapshot(world: &World) -> Option<ChargenSnapshot> {
    let (screen, serial) = world.try_resource::<ChargenSession>()?.open?;
    let index = world.try_resource::<crate::components::LoadedCellIndex>()?.0.clone();
    let player = crate::components::PlayerEntity::get(world)?;
    let identity = current_identity(world, &index, player);
    let values = world.get::<byroredux_core::ecs::components::ActorValues>(player);
    let attributes: Vec<_> = SPECIAL_CODES.filter_map(|code| actor_value_option(world, &index, values.as_deref(), code, true)).collect();
    let skills: Vec<_> = SKILL_CODES.filter_map(|code| actor_value_option(world, &index, values.as_deref(), code, false)).collect();
    drop(values);
    let mut races: Vec<ChargenOption> = world
        .try_resource::<byroredux_scripting::obscript_more::PlayableRaces>()
        .map(|playable| playable.iter().collect::<Vec<_>>())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|form| index.races.get(&form))
        .map(|race| ChargenOption { id: race.form_id, name: race.full_name.clone(), description: race.description.clone() })
        .collect();
    races.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let mut traits: Vec<ChargenOption> = index
        .perks
        .values()
        .filter(|perk| perk.is_trait && perk.playable)
        .map(|perk| ChargenOption { id: perk.form_id, name: perk.full_name.clone(), description: perk.description.clone() })
        .collect();
    traits.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let chosen_traits = world
        .get::<byroredux_core::character::Perks>(player)
        .map(|perks| traits.iter().filter(|candidate| perks.rank(candidate.id) > 0).map(|candidate| candidate.id).collect())
        .unwrap_or_default();
    let (view, title) = match screen {
        ChargenScreen::Name => (ChargenScreenView::Name, "Name"),
        ChargenScreen::RaceSex => (ChargenScreenView::RaceSex, "Sex and race"),
        ChargenScreen::Special { points } => {
            let current: i32 = attributes.iter().map(|(_, value)| *value).sum();
            let points = if points == 0 { current } else { points as i32 };
            (ChargenScreenView::Special { points, min: SPECIAL_MIN, max: SPECIAL_MAX }, "S.P.E.C.I.A.L.")
        }
        ChargenScreen::TagSkills { count } => (ChargenScreenView::TagSkills { count: count as usize }, "Tag skills"),
        ChargenScreen::Traits => {
            let max = byroredux_scripting::obscript_state::game_setting(world, "iTraitMenuMaxNumTraits").map_or(2, |max| max.max(0.0) as usize);
            (ChargenScreenView::Traits { max }, "Traits")
        }
    };
    Some(ChargenSnapshot {
        screen: view,
        serial,
        title: title.to_string(),
        name: identity.name,
        female: identity.female,
        race: identity.race_form_id,
        races,
        attributes,
        skills,
        tagged: byroredux_scripting::obscript_more::player_tag_skills(world).into_iter().map(u32::from).collect(),
        traits,
        chosen_traits,
    })
}

/// Apply what the player accepted on the open screen and close it. Returns `true` when the
/// player's body must be rebuilt (the sex or race changed).
pub(crate) fn apply(world: &mut World, choice: ChargenChoice) -> bool {
    if let Some(mut session) = world.try_resource_mut::<ChargenSession>() {
        session.close();
    }
    let Some(player) = crate::components::PlayerEntity::get(world) else {
        return false;
    };
    let Some(index) = world.try_resource::<crate::components::LoadedCellIndex>().map(|index| index.0.clone()) else {
        return false;
    };
    let mut identity = current_identity(world, &index, player);
    match choice {
        ChargenChoice::Name(name) => {
            let interned = world.resource_mut::<byroredux_core::string::StringPool>().intern(&name);
            world.insert(player, byroredux_core::ecs::components::Name(interned));
            identity.name = name;
            world.insert(player, identity);
            false
        }
        ChargenChoice::RaceSex { female, race } => {
            let changed = identity.female != female || identity.race_form_id != race;
            identity.female = female;
            identity.race_form_id = race;
            world.insert(player, identity);
            crate::systems::player_identity_sync_system(world, 0.0);
            changed
        }
        ChargenChoice::Special(values) => {
            let forms: Vec<(u32, f32)> = values
                .into_iter()
                .filter(|(code, _)| SPECIAL_CODES.contains(code))
                .filter_map(|(code, value)| {
                    let form = byroredux_scripting::obscript_actor_values::actor_value_form(world, code as f32)?;
                    Some((form, value.clamp(SPECIAL_MIN, SPECIAL_MAX) as f32))
                })
                .collect();
            if let Some(mut actor_values) = world.query_mut::<byroredux_core::ecs::components::ActorValues>() {
                if let Some(actor_values) = actor_values.get_mut(player) {
                    for (form, value) in forms {
                        actor_values.set_base(form, value);
                    }
                }
            }
            false
        }
        ChargenChoice::TagSkills(skills) => {
            for (slot, skill) in skills.into_iter().take(3).enumerate() {
                byroredux_scripting::obscript_more::set_player_tag_skill(world, skill as u8, slot as u8);
            }
            false
        }
        ChargenChoice::Traits(chosen) => {
            let traits: Vec<u32> = index.perks.values().filter(|perk| perk.is_trait && perk.playable).map(|perk| perk.form_id).collect();
            if let Some(mut perks) = world.query_mut::<byroredux_core::character::Perks>() {
                if perks.get_mut(player).is_none() {
                    perks.insert(player, byroredux_core::character::Perks::default());
                }
                let owned = perks.get_mut(player).expect("present or inserted");
                for perk in traits {
                    if chosen.contains(&perk) {
                        if owned.rank(perk) == 0 {
                            owned.set_rank(perk, 1);
                        }
                    } else {
                        owned.remove(perk);
                    }
                }
            }
            false
        }
    }
}

/// The console's answer to the open screen, if one was given: `Some(rebuild body)` when it
/// was applied (or the screen closed unchanged), `None` when there was none.
pub(crate) fn apply_console_answer(world: &mut World) -> Option<bool> {
    let answer = world.try_resource_mut::<ChargenSession>()?.take_answer()?;
    Some(match answer {
        Some(choice) => apply(world, choice),
        None => {
            dismiss(world);
            false
        }
    })
}

/// Close the open screen without changing anything (the page was dismissed).
pub(crate) fn dismiss(world: &World) {
    if let Some(mut session) = world.try_resource_mut::<ChargenSession>() {
        session.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every character-creation request the ObScript commands raise maps to its screen and
    /// the menu code the per-game inventory gives it; other requests are not chargen.
    #[test]
    fn requests_map_to_screens_and_menu_codes() {
        assert_eq!(ChargenScreen::for_request("race", 0).map(ChargenScreen::menu_code), Some(1036));
        assert_eq!(ChargenScreen::for_request("name entry", 0).map(ChargenScreen::menu_code), Some(1051));
        assert_eq!(ChargenScreen::for_request("vigor tester", 40), Some(ChargenScreen::Special { points: 40 }));
        assert_eq!(ChargenScreen::for_request("vigor tester", 40).map(ChargenScreen::menu_code), Some(1074));
        assert_eq!(ChargenScreen::for_request("tag skills", 3), Some(ChargenScreen::TagSkills { count: 3 }));
        assert_eq!(ChargenScreen::for_request("traits", 0).map(ChargenScreen::menu_code), Some(1084));
        assert_eq!(ChargenScreen::for_request("map", 0x1234), None);
    }

    /// One screen at a time, in the order asked; each new one gets a new serial.
    #[test]
    fn screens_open_one_at_a_time_in_order() {
        let mut session = ChargenSession::default();
        session.request(ChargenScreen::Name);
        session.request(ChargenScreen::RaceSex);
        assert!(session.open_next());
        assert_eq!(session.open(), Some(ChargenScreen::Name));
        assert!(!session.open_next(), "the open screen stays until answered");
        let first = session.open.unwrap().1;
        session.close();
        assert!(session.open_next());
        assert_eq!(session.open(), Some(ChargenScreen::RaceSex));
        assert_ne!(session.open.unwrap().1, first);
        session.close();
        assert!(!session.open_next());
    }
}
