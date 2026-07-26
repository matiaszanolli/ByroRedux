use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use byroredux_bsa::{Ba2Archive, BsaArchive};
use ruffle_core::external::Value as ExternalValue;
use ruffle_core::tag_utils::SwfMovie;
use ruffle_core::{FloatDuration, Player, PlayerBuilder};

use super::{ScaleformHostBridge, ScaleformHostDispatch, ScaleformValue};
use crate::{ScaleformHostCatalog, ScaleformHostMethodKind, ScaleformProfile};

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
    bridge.set_response_handler("RequestPlayerInfo", |arguments| {
        vec![ScaleformValue::Bool(
            arguments == [ScaleformValue::from("inventory")],
        )]
    });

    let outcome = bridge.record_call(
        "RequestPlayerInfo",
        &[ExternalValue::from(7_i32), ExternalValue::from("inventory")],
    );

    assert_eq!(outcome.return_value, ExternalValue::Null);
    assert_eq!(
        outcome.callback_response,
        Some(vec![ExternalValue::from(7_i32), ExternalValue::Bool(true)])
    );
    let call = bridge.drain_calls().pop().unwrap();
    assert_eq!(call.transport_method, "RequestPlayerInfo");
    assert_eq!(call.method, "RequestPlayerInfo");
    assert_eq!(call.request_id, Some(7));
    assert_eq!(call.arguments, vec![ScaleformValue::from("inventory")]);
    assert_eq!(call.dispatch, ScaleformHostDispatch::GameDelegateResponse);
    assert!(bridge.unknown_methods().is_empty());
    assert!(bridge.unanswered_methods().is_empty());
}

#[test]
fn skyrim_catalog_distinguishes_commands_requests_and_unknowns() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::SkyrimAvm1);

    let command = bridge.record_call(
        "PlaySound",
        &[ExternalValue::from(1_i32), ExternalValue::from("UIMenuOK")],
    );
    assert_eq!(command.return_value, ExternalValue::Null);
    assert!(command.callback_response.is_none());

    let request = bridge.record_call("RequestItemCardInfo", &[ExternalValue::from(2_i32)]);
    assert_eq!(request.return_value, ExternalValue::Null);
    assert!(request.callback_response.is_none());

    bridge.record_call("UnmappedMethod", &[ExternalValue::from(3_i32)]);

    let calls = bridge.drain_calls();
    assert_eq!(calls[0].dispatch, ScaleformHostDispatch::Queued);
    assert_eq!(calls[0].request_id, Some(1));
    assert_eq!(calls[0].arguments, vec![ScaleformValue::from("UIMenuOK")]);
    assert_eq!(calls[1].dispatch, ScaleformHostDispatch::MissingResponse);
    assert_eq!(calls[2].dispatch, ScaleformHostDispatch::Unknown);
    assert_eq!(
        bridge.unanswered_methods(),
        vec!["RequestItemCardInfo".to_string()]
    );
    assert_eq!(bridge.unknown_methods(), vec!["UnmappedMethod".to_string()]);

    // Registering a queued engine handler does not satisfy GameDelegate's
    // synchronous callback contract; only a configured response does.
    bridge.register_method("RequestItemCardInfo");
    assert_eq!(
        bridge.unanswered_methods(),
        vec!["RequestItemCardInfo".to_string()]
    );
}

#[test]
fn skyrim_catalog_is_pinned_sorted_and_profile_specific() {
    let catalog = ScaleformHostCatalog::for_profile(ScaleformProfile::SkyrimAvm1);
    assert_eq!(catalog.len(), 74);
    assert!(catalog.contains("PlaySound"));
    assert_eq!(
        catalog.find("RequestPlayerInfo").unwrap().kind,
        ScaleformHostMethodKind::Request
    );
    assert_eq!(
        catalog
            .methods()
            .iter()
            .filter(|method| method.kind == ScaleformHostMethodKind::Request)
            .count(),
        12
    );
    assert!(catalog
        .methods()
        .windows(2)
        .all(|methods| methods[0].name < methods[1].name));

    let fallout = ScaleformHostCatalog::for_profile(ScaleformProfile::Fallout4Avm2);
    assert!(fallout.is_empty());
    assert!(!fallout.contains("PlaySound"));
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
