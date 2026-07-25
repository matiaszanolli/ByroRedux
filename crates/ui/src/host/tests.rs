use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use byroredux_bsa::{Ba2Archive, BsaArchive};
use ruffle_core::external::Value as ExternalValue;
use ruffle_core::tag_utils::SwfMovie;
use ruffle_core::{FloatDuration, Player, PlayerBuilder};

use super::{ScaleformHostBridge, ScaleformValue};
use crate::ScaleformProfile;

const AVM1_FIXTURE: &str = include_str!("../../testdata/avm1_external_interface.swf.b64");
const AVM2_FIXTURE: &str = include_str!("../../testdata/avm2_external_interface.swf.b64");

fn decode_base64(input: &str) -> Vec<u8> {
    fn sextet(byte: u8) -> Option<u8> {
        match byte {
            b'A'..=b'Z' => Some(byte - b'A'),
            b'a'..=b'z' => Some(byte - b'a' + 26),
            b'0'..=b'9' => Some(byte - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    }

    let encoded = input.bytes().filter_map(sextet).collect::<Vec<_>>();
    let mut decoded = Vec::with_capacity(encoded.len() * 3 / 4);
    for chunk in encoded.chunks(4) {
        decoded.push((chunk[0] << 2) | (chunk[1] >> 4));
        if chunk.len() > 2 {
            decoded.push((chunk[1] << 4) | (chunk[2] >> 2));
        }
        if chunk.len() > 3 {
            decoded.push((chunk[2] << 6) | chunk[3]);
        }
    }
    decoded
}

fn run_fixture(
    encoded_fixture: &str,
    profile: ScaleformProfile,
) -> (Arc<Mutex<Player>>, ScaleformHostBridge) {
    let bytes = decode_base64(encoded_fixture);
    let bridge = ScaleformHostBridge::new(profile);
    bridge.set_response("ping", ScaleformValue::from("Pong!"));
    bridge.register_method("reentry");
    run_movie(&bytes, bridge)
}

fn run_movie(
    bytes: &[u8],
    bridge: ScaleformHostBridge,
) -> (Arc<Mutex<Player>>, ScaleformHostBridge) {
    let movie = SwfMovie::from_data(bytes, "file:///external-interface.swf".to_string(), None)
        .expect("SWF must parse");
    assert_eq!(ScaleformProfile::from_movie(&movie), bridge.profile());

    let player = PlayerBuilder::new()
        .with_external_interface(bridge.provider())
        .with_movie(movie)
        .with_viewport_dimensions(64, 64, 1.0)
        .build();
    player.lock().unwrap().set_is_playing(true);

    for _ in 0..3 {
        player
            .lock()
            .unwrap()
            .tick(FloatDuration::from_secs(1.0 / 30.0));
    }

    (player, bridge)
}

fn data_dir(environment: &str, fallback: &str) -> PathBuf {
    std::env::var(environment)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(fallback))
}

fn assert_external_interface_round_trip(
    player: &Arc<Mutex<Player>>,
    bridge: &ScaleformHostBridge,
    profile: ScaleformProfile,
) {
    let callbacks = bridge.available_callbacks();
    assert!(callbacks.iter().any(|name| name == "parrot"));
    assert!(callbacks.iter().any(|name| name == "callWith"));

    let result = player
        .lock()
        .unwrap()
        .call_internal_interface("parrot", [ExternalValue::from("ByroRedux")]);
    assert_eq!(result, ExternalValue::from("ByroRedux"));

    let calls = bridge.drain_calls();
    assert!(calls.iter().any(|call| {
        call.profile == profile && call.transport_method == "ping" && call.method == "ping"
    }));
    assert!(calls
        .windows(2)
        .all(|calls| calls[0].sequence < calls[1].sequence));
    assert!(bridge
        .unknown_methods()
        .iter()
        .any(|method| method == "non_existent"));
}

#[test]
fn profile_detection_distinguishes_avm1_and_avm2() {
    assert_eq!(
        ScaleformProfile::detect(&decode_base64(AVM1_FIXTURE)).unwrap(),
        ScaleformProfile::SkyrimAvm1
    );
    assert_eq!(
        ScaleformProfile::detect(&decode_base64(AVM2_FIXTURE)).unwrap(),
        ScaleformProfile::Fallout4Avm2
    );
}

#[test]
fn scaleform_values_round_trip_nested_payloads() {
    let value = ScaleformValue::Object(BTreeMap::from([
        ("enabled".to_string(), ScaleformValue::Bool(true)),
        (
            "items".to_string(),
            ScaleformValue::List(vec![ScaleformValue::Number(42.0), ScaleformValue::Null]),
        ),
    ]));

    let external = ExternalValue::from(value.clone());
    assert_eq!(ScaleformValue::from(&external), value);
}

#[test]
fn skyrim_game_delegate_transport_is_normalized() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::SkyrimAvm1);
    bridge.set_response("Inventory.GetItems", ScaleformValue::Bool(true));

    let response = bridge.record_call(
        "Call",
        &[
            ExternalValue::from("Inventory.GetItems"),
            ExternalValue::List(vec![ExternalValue::from(7_i32)]),
        ],
    );

    assert_eq!(response, ExternalValue::Bool(true));
    let call = bridge.drain_calls().pop().unwrap();
    assert_eq!(call.transport_method, "Call");
    assert_eq!(call.method, "Inventory.GetItems");
    assert_eq!(
        call.arguments,
        vec![ScaleformValue::List(vec![ScaleformValue::Number(7.0)])]
    );
    assert!(bridge.unknown_methods().is_empty());
}

#[test]
fn avm1_external_interface_is_bidirectional_headlessly() {
    let (player, bridge) = run_fixture(AVM1_FIXTURE, ScaleformProfile::SkyrimAvm1);
    assert_external_interface_round_trip(&player, &bridge, ScaleformProfile::SkyrimAvm1);
}

#[test]
fn avm2_external_interface_is_bidirectional_headlessly() {
    let (player, bridge) = run_fixture(AVM2_FIXTURE, ScaleformProfile::Fallout4Avm2);
    assert_external_interface_round_trip(&player, &bridge, ScaleformProfile::Fallout4Avm2);
}

#[test]
#[ignore = "requires an installed Skyrim Special Edition corpus"]
fn installed_skyrim_hudmenu_loads_with_avm1_profile() {
    let archive_path = data_dir(
        "BYROREDUX_SKYRIMSE_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Skyrim Special Edition/Data",
    )
    .join("Skyrim - Interface.bsa");
    let archive = BsaArchive::open(&archive_path).expect("open Skyrim interface BSA");
    let swf = archive
        .extract("interface\\hudmenu.swf")
        .expect("extract Skyrim HUDMenu.swf");

    let bridge = ScaleformHostBridge::new(ScaleformProfile::SkyrimAvm1);
    let (_player, bridge) = run_movie(&swf, bridge);
    assert_eq!(bridge.profile(), ScaleformProfile::SkyrimAvm1);
}

#[test]
#[ignore = "requires an installed Fallout 4 corpus"]
fn installed_fallout4_hudmenu_loads_with_avm2_profile() {
    let archive_path = data_dir(
        "BYROREDUX_FO4_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
    )
    .join("Fallout4 - Interface.ba2");
    let archive = Ba2Archive::open(&archive_path).expect("open Fallout 4 interface BA2");
    let swf = archive
        .extract("interface\\hudmenu.swf")
        .expect("extract Fallout 4 HUDMenu.swf");

    let bridge = ScaleformHostBridge::new(ScaleformProfile::Fallout4Avm2);
    let (_player, bridge) = run_movie(&swf, bridge);
    assert_eq!(bridge.profile(), ScaleformProfile::Fallout4Avm2);
}
