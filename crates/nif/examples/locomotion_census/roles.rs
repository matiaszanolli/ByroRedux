//! Which clips are a game's base locomotion set, and what each one's name says it is for.
//!
//! A role is read from the file name, so the census says what is *named*, not what a behaviour
//! graph plays. Paths here are lower-case with `/` separators.

use super::common::Game;

/// A locomotion role, in the report's column order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Gait,
    Turn,
    Start,
    Stop,
    Reversal,
    Airborne,
}

impl Role {
    pub const ALL: [Role; 6] = [
        Role::Gait,
        Role::Turn,
        Role::Start,
        Role::Stop,
        Role::Reversal,
        Role::Airborne,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Role::Gait => "gait cycle",
            Role::Turn => "turn",
            Role::Start => "start",
            Role::Stop => "stop",
            Role::Reversal => "reversal",
            Role::Airborne => "airborne",
        }
    }
}

/// The base set in words: the adult, third-person, unarmed or relaxed locomotion clips.
pub fn base_set_description(game: Game) -> &'static str {
    match game {
        Game::Oblivion => "`characters/_male/` `walk*`, `jump*`",
        Game::Fallout3 | Game::FalloutNV => {
            "`characters/_male/locomotion/male/mt*`, `locomotion/mtturn*`, `_male/mtjump*`"
        }
        Game::SkyrimLE | Game::SkyrimSE => {
            "`actors/character/animations/` `male/mt_{walk,run,sprint}*`, `male/npc_turn*`, `mt_turn*`, `mt_jump*`"
        }
        Game::Fallout4 | Game::Fallout76 => "`actors/character/animations/mt/neutral/`",
        Game::Starfield => "`actors/human/animations/melee/unarmed/relaxed_*`",
    }
}

fn letters(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_lowercase())
}

/// The file name without its extension, when `path` is `<prefix><name>.<ext>`.
fn named<'a>(path: &'a str, prefix: &str, ext: &str) -> Option<&'a str> {
    path.strip_prefix(prefix)?.strip_suffix(ext)
}

fn in_kf_male_directory(path: &str) -> bool {
    if let Some(name) = named(path, "meshes/characters/_male/locomotion/male/", ".kf") {
        return name
            .strip_prefix("mt")
            .is_some_and(|rest| letters(rest) && !rest.starts_with("idle"));
    }
    if let Some(name) = named(path, "meshes/characters/_male/locomotion/", ".kf") {
        return name.strip_prefix("mtturn").is_some_and(letters);
    }
    named(path, "meshes/characters/_male/", ".kf")
        .and_then(|name| name.strip_prefix("mtjump"))
        .is_some_and(letters)
}

fn in_skyrim_directory(path: &str) -> bool {
    let Some(name) = named(path, "meshes/actors/character/animations/", ".hkx") else {
        return false;
    };
    if let Some(male) = name.strip_prefix("male/") {
        let gait = ["mt_walk", "mt_run", "mt_sprint"]
            .iter()
            .any(|p| male.strip_prefix(p).is_some_and(letters));
        let turn = male.strip_prefix("npc_turn").is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
        });
        return gait || turn;
    }
    let turn = ["mt_turnleft", "mt_turnright"].iter().any(|p| {
        name.strip_prefix(p)
            .is_some_and(|d| !d.is_empty() && d.bytes().all(|b| b.is_ascii_digit()))
    });
    // `mt_jumpforwardtest` is a test clip.
    let jump = name
        .strip_prefix("mt_jump")
        .is_some_and(|rest| (rest.is_empty() || letters(rest)) && rest != "forwardtest");
    turn || jump
}

/// Whether `path` belongs to the game's base set.
pub fn in_base_set(game: Game, path: &str) -> bool {
    match game {
        Game::Oblivion => named(path, "meshes/characters/_male/", ".kf").is_some_and(|name| {
            ["walk", "jump"].iter().any(|p| {
                name.strip_prefix(p)
                    .is_some_and(|rest| rest.is_empty() || letters(rest))
            })
        }),
        Game::Fallout3 | Game::FalloutNV => in_kf_male_directory(path),
        Game::SkyrimLE | Game::SkyrimSE => in_skyrim_directory(path),
        Game::Fallout4 | Game::Fallout76 => named(
            path,
            "meshes/actors/character/animations/mt/neutral/",
            ".hkx",
        )
        .is_some_and(|name| !name.contains('/')),
        Game::Starfield => named(
            path,
            "meshes/actors/human/animations/melee/unarmed/relaxed_",
            ".af",
        )
        // `relaxed_lib_*` are gestures; four of them are farewell waves named "departure".
        .is_some_and(|name| !name.contains('/') && !name.starts_with("lib_")),
    }
}

/// `idle` anywhere it is not the start of `idletrans`.
fn names_an_idle(stem: &str) -> bool {
    stem.match_indices("idle")
        .any(|(i, _)| !stem[i + 4..].starts_with("trans"))
}

/// `turn[_]{left,right}<digit>`, as in `turn_left90` and `turnright180`.
fn names_an_angled_turn(stem: &str) -> bool {
    stem.match_indices("turn").any(|(i, _)| {
        let rest = &stem[i + 4..];
        let rest = rest.strip_prefix('_').unwrap_or(rest);
        rest.strip_prefix("left")
            .or_else(|| rest.strip_prefix("right"))
            .is_some_and(|after| after.starts_with(|c: char| c.is_ascii_digit()))
    })
}

fn has_any(stem: &str, parts: &[&str]) -> bool {
    parts.iter().any(|part| stem.contains(part))
}

/// The role a base-set clip's name gives it, or `None` when it is not locomotion (an idle, a hit
/// reaction, a camera track and so on).
pub fn role_of(stem: &str) -> Option<Role> {
    let not_locomotion = names_an_idle(stem)
        || has_any(
            stem,
            &[
                "lookdirections",
                "camera",
                "posea",
                "bumpreaction",
                "stagger",
                "offset",
                "cower",
                "essential",
                "dooropen",
                "crit",
                "flavor",
                "bleed",
                "death",
                "down",
            ],
        )
        || ["hit", "swim", "to_", "wst"]
            .iter()
            .any(|p| stem.starts_with(p))
        || (stem.starts_with("wpn") && !stem.starts_with("wpnjump"));
    if (not_locomotion && !stem.contains("to_idle")) || stem.starts_with("sneak") {
        return None;
    }
    if stem.contains("jump") {
        return Some(Role::Airborne);
    }
    if stem.contains("turninplace")
        || names_an_angled_turn(stem)
        || [
            "walkturn",
            "mtturn",
            "mt_turn",
            "npc_turnleft",
            "npc_turnright",
        ]
        .iter()
        .any(|p| stem.starts_with(p))
    {
        return Some(Role::Turn);
    }
    if stem.ends_with("_start") || has_any(stem, &["stand_to_", "departure", "turntowalk"]) {
        return Some(Role::Start);
    }
    if has_any(stem, &["_to_stand", "_stop", "arrival", "_to_idle"]) {
        return Some(Role::Stop);
    }
    if has_any(
        stem,
        &[
            "reversal",
            "walk_to_walk",
            "walk_to_run",
            "run_to_walk",
            "run_to_run",
            "forwardtobackward",
            "backwardtoforward",
            "lefttoright",
            "righttoleft",
        ],
    ) {
        return Some(Role::Reversal);
    }
    // Leans are additive poses over a gait, not a gait of their own.
    if stem.contains("lean") {
        return None;
    }
    let kf_cardinal = stem.strip_prefix("mt").is_some_and(|rest| {
        let rest = rest.strip_prefix("fast").unwrap_or(rest);
        ["forward", "backward", "left", "right"].contains(&rest)
    });
    (kf_cardinal || stem.starts_with("mt_") || has_any(stem, &["walk", "run", "jog", "sprint"]))
        .then_some(Role::Gait)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn farewell_gestures_named_departure_are_not_starfield_starts() {
        let dir = "meshes/actors/human/animations/melee/unarmed/";
        assert!(!in_base_set(
            Game::Starfield,
            &format!("{dir}relaxed_lib_departure_s01_wave_rh.af")
        ));
        assert!(in_base_set(
            Game::Starfield,
            &format!("{dir}relaxed_walkdeparture_left90.af")
        ));
        assert_eq!(role_of("relaxed_walkdeparture_left90"), Some(Role::Start));
        assert_eq!(role_of("relaxed_walkforward_start"), Some(Role::Start));
    }

    #[test]
    fn fallout_base_set_spans_three_directories_and_leaves_idles_out() {
        let male = "meshes/characters/_male/";
        for path in [
            "locomotion/male/mtforward.kf",
            "locomotion/mtturnleft.kf",
            "mtjumploop.kf",
        ] {
            assert!(
                in_base_set(Game::FalloutNV, &format!("{male}{path}")),
                "{path}"
            );
        }
        for path in [
            "locomotion/male/mtidle.kf",
            "mtforward.kf",
            "locomotion/female/mtforward.kf",
        ] {
            assert!(
                !in_base_set(Game::FalloutNV, &format!("{male}{path}")),
                "{path}"
            );
        }
    }

    #[test]
    fn skyrim_leaves_out_its_jump_test_clip_and_weapon_gaits() {
        let dir = "meshes/actors/character/animations/";
        assert!(in_base_set(Game::SkyrimSE, &format!("{dir}mt_jump.hkx")));
        assert!(in_base_set(
            Game::SkyrimSE,
            &format!("{dir}male/mt_walkforward.hkx")
        ));
        assert!(!in_base_set(
            Game::SkyrimSE,
            &format!("{dir}mt_jumpforwardtest.hkx")
        ));
        assert!(!in_base_set(
            Game::SkyrimSE,
            &format!("{dir}1hm_walkforward.hkx")
        ));
    }

    #[test]
    fn names_give_roles_in_precedence_order() {
        for (stem, role) in [
            ("mtforward", Some(Role::Gait)),
            ("mtfastleft", Some(Role::Gait)),
            ("walkforward", Some(Role::Gait)),
            ("walkturnleft", Some(Role::Turn)),
            ("turn_left90", Some(Role::Turn)),
            ("mtjumpland", Some(Role::Airborne)),
            ("walk_to_stand", Some(Role::Stop)),
            ("run_to_idle", Some(Role::Stop)),
            ("walk_to_run", Some(Role::Reversal)),
            ("mt_idle", None),
            ("sneakwalkforward", None),
            ("walkforwardleanleft", None),
        ] {
            assert_eq!(role_of(stem), role, "{stem}");
        }
    }
}
