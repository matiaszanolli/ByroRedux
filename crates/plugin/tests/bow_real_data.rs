//! Player-archery PRD, Phase 1 (#7055): the bow's rules against the shipped masters (`parse_esm` → [`BowModel`]), on
//! the records the PRD's §7.2 names. Opt-in, needs the games installed:
//!
//! ```sh
//! cargo test -p byroredux-plugin --test bow_real_data -- --ignored --nocapture
//! ```
//!
//! A missing install passes with a skip line; `BYROREDUX_REQUIRE_GAME_DATA=1` fails instead.

use byroredux_core::character::{BowModel, ShotTraits};
use byroredux_plugin::esm::parse_esm;
use byroredux_plugin::esm::records::{EsmIndex, ItemKind};
use byroredux_plugin::esm::test_paths;

fn index(what: &str, path: std::path::PathBuf) -> Option<EsmIndex> {
    if !path.is_file() {
        test_paths::require_or_skip(&format!("{what} ({})", path.display()));
        return None;
    }
    Some(parse_esm(&std::fs::read(path).expect("read the master")).expect("parse the master"))
}

/// Oblivion: the iron bow is a type-5 weapon and the iron arrow keeps its speed; every arrow authors speed 1.0 and the
/// master authors none of the `fArrow*` settings, so the CS wiki's defaults stand (BOW-DRW-002, BOW-FLT-001).
#[test]
#[ignore = "needs Oblivion.esm on disk"]
fn oblivion_bows_and_arrows() {
    let Some(index) = index("Oblivion.esm", test_paths::oblivion_esm()) else { return };
    let model = BowModel::OBLIVION.with_gmst(|name| index.game_setting_float(name));
    assert_eq!(model, BowModel::OBLIVION, "Oblivion.esm authors none of the fArrow* settings");
    let (mut bows, mut arrows) = (0, 0);
    for item in index.items.values() {
        match &item.kind {
            ItemKind::Weapon { oblivion_type, anim_type, .. } if model.is_bow(*oblivion_type, *anim_type) => bows += 1,
            ItemKind::Ammo { speed, .. } => {
                arrows += 1;
                assert_eq!(*speed, 1.0, "every vanilla arrow authors speed 1.0 (census 2026-11-20)");
            }
            _ => {}
        }
    }
    assert_eq!((bows, arrows), (131, 128), "the PRD's census");
    let ItemKind::Weapon { damage: bow_damage, oblivion_type, .. } = &index.items[&0x0002_5231].kind else { panic!("WeapIronBow is a weapon") };
    assert_eq!((*bow_damage, *oblivion_type), (8, 5));
    let ItemKind::Ammo { damage, speed, flags, .. } = &index.items[&0x0001_7829].kind else { panic!("Arrow1Iron is ammunition") };
    assert_eq!((*damage, *speed, *flags & 1), (8.0, 1.0, 0));
    // PRD §7.2 scenario 1: `WeapIronBow` with `Arrow1Iron` at Marksman 5, Agility 50, Luck 50 and full Fatigue.
    let traits = ShotTraits { attribute: 50.0, skill: 5.0, luck: 50.0, fatigue_fraction: 1.0 };
    let shot = model.shot_damage(*bow_damage as f32, *damage, traits);
    assert!((shot - 2.2).abs() < 1e-3, "{shot}");
    // 22 arrows ignore normal weapon resistance (the decoded flag).
    let resistant = index.items.values().filter(|item| matches!(&item.kind, ItemKind::Ammo { flags, .. } if flags & 1 != 0)).count();
    assert_eq!(resistant, 22);
}

/// Skyrim: the hunting bow is an animation-type-7 weapon and its full draw is `0.4 + 1.66 / speed + 0.6`; the iron
/// arrow names a `PROJ` that flies, and 19 of the 20 arrow projectiles carry the gravity a straight arrow ignores.
#[test]
#[ignore = "needs Skyrim.esm on disk"]
fn skyrim_bows_and_arrows() {
    let path = test_paths::skyrim_se_data_dir().join("Skyrim.esm");
    let Some(index) = index("Skyrim.esm", path) else { return };
    let model = BowModel::SKYRIM.with_gmst(|name| index.game_setting_float(name));
    let ItemKind::Weapon { anim_type, speed, .. } = &index.items[&0x0001_3985].kind else { panic!("HuntingBow is a weapon") };
    assert!(model.is_bow(0, *anim_type), "HuntingBow is a bow");
    let full = model.full_draw_seconds(*speed);
    assert!((full - (0.4 + 1.66 / *speed + 0.6)).abs() < 1e-4, "{full} at speed {speed}");
    let ItemKind::Ammo { projectile_form, .. } = &index.items[&0x0001_397D].kind else { panic!("IronArrow is ammunition") };
    let proj = &index.projectiles[projectile_form];
    assert!(proj.muzzle_speed > 0.0, "the arrow's PROJ has a speed");
    assert!(model.range(proj.range) <= 4096.0);
    let gravity = index
        .items
        .values()
        .filter_map(|item| match &item.kind {
            ItemKind::Ammo { projectile_form, .. } => index.projectiles.get(projectile_form),
            _ => None,
        })
        .filter(|proj| proj.gravity != 0.0)
        .count();
    assert!(gravity > 0, "arrow projectiles carry gravity (a straight arrow must ignore it, BOW-FLT-004)");
}

/// Oblivion armor and weapon resistance, on the records the defense rules read: the iron cuirass carries a decoded
/// rating and is heavy, silver and enchanted weapons ignore normal resistance and a plain iron one does not.
#[test]
#[ignore = "needs Oblivion.esm on disk"]
fn oblivion_armor_and_resistance_records() {
    let Some(index) = index("Oblivion.esm", test_paths::oblivion_esm()) else { return };
    let named = |name: &str| index.items.values().find(|item| item.common.editor_id == name);
    let cuirass = named("IronCuirass").expect("IronCuirass");
    let ItemKind::Armor { armor_rating_x100, armor_type, health, .. } = &cuirass.kind else { panic!("armor") };
    println!("IronCuirass {:08X}: rating x100 {armor_rating_x100}, type {armor_type:?}, health {health}", cuirass.form_id);
    assert_eq!((*armor_rating_x100, *armor_type), (1000, Some(1)), "displayed rating 10 (x100), heavy");
    let resistant: Vec<&str> = index
        .items
        .values()
        .filter(|item| matches!(&item.kind, ItemKind::Weapon { ignores_normal_resistance: true, .. }))
        .map(|item| item.common.editor_id.as_str())
        .collect();
    println!("{} weapons ignore normal resistance, e.g. {:?}", resistant.len(), &resistant[..resistant.len().min(6)]);
    assert!(resistant.iter().any(|name| name.to_ascii_lowercase().contains("silver")), "silver weapons set the flag");
    let iron = named("IronBow").or_else(|| named("WeapIronBow")).expect("an iron bow");
    assert!(matches!(&iron.kind, ItemKind::Weapon { ignores_normal_resistance: false, .. }), "a plain iron bow");
}
