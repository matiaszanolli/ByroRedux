//! `DOOR` base records (PRD door-runtime DRT-REC-001, -002); the REFR `Open by Default` test is in `refr.rs`.

use super::super::super::reader::{EsmReader, GameKind};
use super::super::support::parse_modl_group;
use super::super::*;

fn sub(buf: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    buf.extend_from_slice(kind);
    buf.extend_from_slice(&(data.len() as u16).to_le_bytes());
    buf.extend_from_slice(data);
}

/// A `DOOR` record with a model, an `FNAM` flag byte and the three sounds, in a one-record group.
fn door_group(form_id: u32, fnam: Option<u8>, sounds: [u32; 3]) -> Vec<u8> {
    let mut subs = Vec::new();
    sub(&mut subs, b"EDID", b"TestDoor\0");
    sub(&mut subs, b"MODL", b"architecture\\door01.nif\0");
    sub(&mut subs, b"SNAM", &sounds[0].to_le_bytes());
    sub(&mut subs, b"ANAM", &sounds[1].to_le_bytes());
    sub(&mut subs, b"BNAM", &sounds[2].to_le_bytes());
    if let Some(fnam) = fnam {
        sub(&mut subs, b"FNAM", &[fnam]);
    }
    let mut record = Vec::new();
    record.extend_from_slice(b"DOOR");
    record.extend_from_slice(&(subs.len() as u32).to_le_bytes());
    record.extend_from_slice(&0u32.to_le_bytes());
    record.extend_from_slice(&form_id.to_le_bytes());
    record.extend_from_slice(&[0u8; 8]);
    record.extend_from_slice(&subs);
    let mut group = Vec::new();
    group.extend_from_slice(b"GRUP");
    group.extend_from_slice(&((24 + record.len()) as u32).to_le_bytes());
    group.extend_from_slice(b"DOOR");
    group.extend_from_slice(&0u32.to_le_bytes());
    group.extend_from_slice(&[0u8; 8]);
    group.extend_from_slice(&record);
    group
}

fn parse_door(group: &[u8], form_id: u32) -> StaticObject {
    parse_door_for(group, form_id, None)
}

fn parse_door_for(group: &[u8], form_id: u32, game: Option<GameKind>) -> StaticObject {
    let mut reader = EsmReader::new(group);
    if let Some(game) = game {
        reader.set_game(game);
    }
    let header = reader.read_group_header().unwrap();
    let end = reader.group_content_end(&header);
    let mut statics = std::collections::HashMap::new();
    parse_modl_group(&mut reader, end, &mut statics).unwrap();
    statics.remove(&form_id).expect("the DOOR is indexed")
}

/// The flag byte and the three sounds reach the base object, and a record with no `FNAM` reads no flags.
#[test]
fn a_door_record_decodes_its_flags_and_sounds() {
    let door = parse_door(&door_group(0x55, Some(0b0000_1000), [0x10, 0x11, 0x12]), 0x55)
        .door
        .expect("a DOOR carries door data");
    assert_eq!(door.fnam, 0b0000_1000);
    assert_eq!((door.open_sound, door.close_sound, door.loop_sound), (Some(0x10), Some(0x11), Some(0x12)));

    let bare = parse_door(&door_group(0x56, None, [0, 0, 0]), 0x56).door.unwrap();
    assert_eq!((bare.fnam, bare.open_sound, bare.close_sound, bare.loop_sound), (0, None, None, None));
}

/// Starfield's `ANAM` is the Facing Axis Override enum and its `BFCB` component blocks carry string
/// `ANAM` / `BNAM`; its sounds are `DOSH` / `DCSH` / `DLSH` (`wbDefinitionsSF1.pas:11575-11626`). None of
/// the three tags may become a sound there, while the same bytes on FO4 still do (#7098).
#[test]
fn starfield_door_tags_are_not_sounds() {
    // Axis 2 ("Based on Bounds") in ANAM, an ASCII-led component string's first word in BNAM.
    let group = door_group(0x58, Some(0b10), [0x7468_6973, 2, u32::from_le_bytes(*b"ship")]);
    let starfield = parse_door_for(&group, 0x58, Some(GameKind::Starfield)).door.unwrap();
    assert_eq!(starfield.fnam, 0b10, "the flag byte still decodes");
    assert_eq!((starfield.open_sound, starfield.close_sound, starfield.loop_sound), (None, None, None));

    let fo4 = parse_door_for(&group, 0x58, Some(GameKind::Fallout4)).door.unwrap();
    assert_eq!(fo4.close_sound, Some(2), "FO4 ANAM is still the close sound");
}

/// Only a `DOOR` carries door data: the same sub-records on a `STAT` read nothing.
#[test]
fn only_a_door_record_carries_door_data() {
    let mut group = door_group(0x57, Some(1), [1, 2, 3]);
    group[20..24].copy_from_slice(b"\0\0\0\0"); // keep the group header's label field intact
    let mut as_stat = group.clone();
    as_stat[24..28].copy_from_slice(b"STAT");
    assert!(parse_door(&as_stat, 0x57).door.is_none());
}

/// Each game's bit layout (xEdit `wbDefinitions*.pas`): Oblivion bits 0 to 3, Fallout 3 / New Vegas bits 1 to 4, Skyrim bits 1 to 5.
#[test]
fn door_flags_follow_each_games_bit_layout() {
    let at = |bit: u8| DoorData { fnam: 1 << bit, ..DoorData::default() };
    let named = |game: GameKind| {
        (0..6)
            .map(|bit| {
                let f = at(bit).flags(game);
                [f.oblivion_gate, f.automatic, f.hidden, f.minimal_use, f.sliding, f.no_open_in_combat_search]
                    .iter()
                    .position(|set| *set)
            })
            .collect::<Vec<_>>()
    };
    // Oblivion: bit 0 gate, 1 automatic, 2 hidden, 3 minimal use; bits 4 and 5 name nothing.
    assert_eq!(named(GameKind::Oblivion), vec![Some(0), Some(1), Some(2), Some(3), None, None]);
    // Fallout 3 / New Vegas: bit 0 unnamed, 1 automatic, 2 hidden, 3 minimal use, 4 sliding.
    assert_eq!(named(GameKind::Fallout3NV), vec![None, Some(1), Some(2), Some(3), Some(4), None]);
    // Skyrim adds bit 5, do not open in combat search.
    assert_eq!(named(GameKind::Skyrim), vec![None, Some(1), Some(2), Some(3), Some(4), Some(5)]);
    assert!(at(3).flags(GameKind::Oblivion).minimal_use && at(3).flags(GameKind::Skyrim).minimal_use);
}
