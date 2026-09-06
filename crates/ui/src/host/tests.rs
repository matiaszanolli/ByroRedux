use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use byroredux_bsa::{Ba2Archive, BsaArchive};
use ruffle_core::external::Value as ExternalValue;
use ruffle_core::tag_utils::SwfMovie;
use ruffle_core::{FloatDuration, LoadBehavior, Player, PlayerBuilder};

use super::{ScaleformHostBridge, ScaleformHostDispatch, ScaleformValue};
use crate::{
    ScaleformHostCatalog, ScaleformHostMethodKind, ScaleformHostObjectState, ScaleformProfile,
    UiInputEvent, UiKeyDescriptor, UiKeyLocation, UiLogicalKey, UiMouseButton, UiNamedKey,
    UiPhysicalKey,
};

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
        .with_load_behavior(LoadBehavior::Blocking)
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
    // #3850: an explicitly-set override is BINDING. Returning it unchecked
    // meant a typo'd or DLC-stripped path surfaced much later as a failure
    // against a directory the operator never named.
    if let Some(v) = std::env::var(environment).ok().filter(|s| !s.is_empty()) {
        let p = PathBuf::from(v);
        assert!(
            p.is_dir(),
            "{environment} points to {p:?}, which is not a directory"
        );
        return p;
    }
    PathBuf::from(fallback)
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

fn dispatch_representative_input(player: &Arc<Mutex<Player>>) {
    let key = UiKeyDescriptor {
        physical_key: UiPhysicalKey::Enter,
        logical_key: UiLogicalKey::Named(UiNamedKey::Enter),
        key_location: UiKeyLocation::Standard,
    };
    let events = [
        UiInputEvent::FocusGained,
        UiInputEvent::MouseMove { x: 32.0, y: 16.0 },
        UiInputEvent::MouseDown {
            x: 32.0,
            y: 16.0,
            button: UiMouseButton::Left,
        },
        UiInputEvent::MouseUp {
            x: 32.0,
            y: 16.0,
            button: UiMouseButton::Left,
        },
        UiInputEvent::KeyDown { key },
        UiInputEvent::TextControl {
            code: crate::UiTextControlCode::Enter,
        },
        UiInputEvent::KeyUp { key },
        UiInputEvent::FocusLost,
    ];

    let mut player = player.lock().unwrap();
    for event in events {
        player.handle_event(event.into());
    }
    player.tick(FloatDuration::from_secs(1.0 / 30.0));
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

/// #2965 (UI-D2-02) — the leading-integer heuristic used to fire on ANY
/// `SkyrimAvm1` call, catalog membership be damned, because `SkyrimAvm1` is
/// also the fallback profile for every non-AS3 movie (loose demo SWFs,
/// third-party AVM1 content) that never went through `GameDelegate.call` at
/// all. A call to a method the catalog has never heard of, with no `respond`
/// callback registered, has zero evidence it's a `GameDelegate` request —
/// pre-fix it still silently lost its first argument and got a bogus
/// `request_id` anyway. Both must now survive intact.
#[test]
fn an_uncataloged_call_with_no_respond_callback_keeps_its_leading_argument() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::SkyrimAvm1);

    bridge.record_call(
        "UnmappedMethod",
        &[ExternalValue::from(3_i32), ExternalValue::from("payload")],
    );

    let call = bridge.drain_calls().pop().unwrap();
    assert_eq!(call.dispatch, ScaleformHostDispatch::Unknown);
    assert_eq!(
        call.request_id, None,
        "an uncataloged call must not be assigned a request_id it never carried"
    );
    assert_eq!(
        call.arguments,
        vec![ScaleformValue::from(3.0), ScaleformValue::from("payload")],
        "the leading integer is real call data here, not a GameDelegate \
         request ID — it must not be stripped"
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
    // #2966 — regenerated from the corpus sweep: 138 original F4CF-sourced
    // entries + 131 real call sites the sweep found outside them.
    assert_eq!(fallout.len(), 269);
    assert!(fallout.contains("PlaySound"));
    assert!(!fallout.contains("RequestPlayerInfo"));
    assert!(fallout
        .methods()
        .windows(2)
        .all(|methods| methods[0].name < methods[1].name));
    assert_eq!(
        fallout.host_object().unwrap(),
        crate::ScaleformHostObject {
            property: "BGSCodeObj",
            on_create: "onCodeObjCreate",
            on_destroy: "onCodeObjDestruction",
        }
    );
}

/// Regression for #2726 / #2727. Both catalogs are scraped from decompiled
/// ActionScript, and the FO4 one shipped `functiononGPSModeButtonClicked` —
/// `function onGPSModeButtonClicked` with the space collapsed. Nothing could
/// detect it: the sortedness window passed (a mangled name still sorts), the
/// length assertion counted it as valid, and the only corpus guard asserts
/// `referenced ⊆ catalog`, which is silent about catalog entries no menu
/// references. A malformed entry costs a dead forwarder plus two dead pool
/// strings in every patched SWF, and makes the *real* method fall through to
/// `ScaleformHostDispatch::Unknown`. Every genuine Scaleform host method is a
/// plain Camel/camelCase ASCII identifier, so well-formedness alone catches the
/// whole artifact class — cheaply, in the default suite, with no game install.
#[test]
fn catalog_names_are_well_formed_actionscript_identifiers() {
    for profile in [ScaleformProfile::SkyrimAvm1, ScaleformProfile::Fallout4Avm2] {
        for method in ScaleformHostCatalog::for_profile(profile).methods() {
            let name = method.name;
            assert!(
                name.starts_with(|first: char| first.is_ascii_alphabetic())
                    && name
                        .chars()
                        .all(|character| character.is_ascii_alphanumeric()),
                "{profile:?} catalog entry {name:?} is not a plain ActionScript identifier"
            );
            // #2966 — bare `.contains(keyword)` also flagged `ReturnFromDLC`
            // (a real, measured method name), because "return" is a
            // legitimate camelCase word there, not a collapsed space. The
            // artifact this test actually guards against — `function` +
            // `on...` glued together with no case transition — only shows up
            // when the keyword match ISN'T its own camelCase word: bounded on
            // both sides by the string's edges or an uppercase letter. A
            // match with a lowercase neighbor on either side is the collapse
            // signature; a match sitting cleanly between two camelCase word
            // boundaries (or the identifier's ends) is just an English word.
            let lowercased = name.to_ascii_lowercase();
            let chars: Vec<char> = name.chars().collect();
            for keyword in ["function", "var", "return", "const", "class"] {
                for (start, _) in lowercased.match_indices(keyword) {
                    let end = start + keyword.len();
                    let boundary_before = start == 0 || chars[start - 1].is_ascii_uppercase();
                    let boundary_after = end == chars.len() || chars[end].is_ascii_uppercase();
                    assert!(
                        boundary_before && boundary_after,
                        "{profile:?} catalog entry {name:?} embeds the ActionScript keyword \
                         {keyword:?} without a camelCase word boundary around it — likely a \
                         whitespace-collapse scraping artifact"
                    );
                }
            }
        }
    }
}

#[test]
fn fallout4_object_transport_is_normalized_and_cataloged() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::Fallout4Avm2);
    let outcome = bridge.record_call(
        "BGSCodeObj.PlaySound",
        &[ExternalValue::from("UIGeneralFocus")],
    );

    assert_eq!(outcome.return_value, ExternalValue::Null);
    assert!(outcome.callback_response.is_none());
    let call = bridge.drain_calls().pop().unwrap();
    assert_eq!(call.transport_method, "BGSCodeObj.PlaySound");
    assert_eq!(call.method, "PlaySound");
    assert_eq!(call.host_object.as_deref(), Some("BGSCodeObj"));
    assert_eq!(call.arguments, vec![ScaleformValue::from("UIGeneralFocus")]);
    assert_eq!(call.dispatch, ScaleformHostDispatch::Queued);
    assert!(bridge.unknown_methods().is_empty());
}

#[test]
fn fallout4_destruction_acknowledgement_is_observable_but_not_a_host_call() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::Fallout4Avm2);

    let outcome = bridge.record_call(crate::avm2_host::DESTROYED_EVENT, &[]);

    assert_eq!(outcome.return_value, ExternalValue::Null);
    assert!(outcome.callback_response.is_none());
    assert_eq!(bridge.code_object_destruction_count(), 1);
    assert!(bridge.drain_calls().is_empty());
    assert!(bridge.unknown_methods().is_empty());
}

/// #2714 — the queue was a plain `VecDeque` with no bound and no consumer, so
/// an undrained bridge grew for the life of the menu. Pushing past the cap
/// must now hold at the cap rather than keep growing.
#[test]
fn an_undrained_call_queue_stops_growing_at_the_cap() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::Fallout4Avm2);
    let overflow = 50usize;
    for i in 0..crate::MAX_QUEUED_CALLS + overflow {
        bridge.record_call("BGSCodeObj.PlaySound", &[ExternalValue::from(i as f64)]);
    }

    assert_eq!(bridge.queued_call_count(), crate::MAX_QUEUED_CALLS);
    assert_eq!(bridge.dropped_calls(), overflow as u64);
}

/// Overflow drops the oldest, so a consumer that finally runs sees the most
/// recent calls — the ones it still has a chance of acting on.
#[test]
fn call_queue_overflow_evicts_the_oldest_entries() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::Fallout4Avm2);
    let total = crate::MAX_QUEUED_CALLS + 10;
    for i in 0..total {
        bridge.record_call("BGSCodeObj.PlaySound", &[ExternalValue::from(i as f64)]);
    }

    let calls = bridge.drain_calls();
    assert_eq!(calls.len(), crate::MAX_QUEUED_CALLS);
    // Sequence numbers are assigned before the bound is applied, so the
    // surviving window is the tail of the stream.
    assert_eq!(calls.first().unwrap().sequence, 10);
    assert_eq!(calls.last().unwrap().sequence, total as u64 - 1);
    assert_eq!(
        calls.last().unwrap().arguments,
        vec![ScaleformValue::Number((total - 1) as f64)]
    );
}

/// #2964 — `unknown_methods` is keyed by whatever method name untrusted
/// ActionScript content calls, unlike `calls` (bounded by #2714/#2714's
/// `MAX_QUEUED_CALLS`, a *count*). A movie running
/// `ExternalInterface.call("m" + i++)` inside `onEnterFrame` chooses a fresh
/// key every frame, so pushing past the cap must hold at the cap rather than
/// grow the set forever.
#[test]
fn unknown_methods_set_stops_growing_at_the_cap() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::Fallout4Avm2);
    let overflow = 50usize;
    for i in 0..crate::MAX_DISTINCT_HOST_METHOD_NAMES + overflow {
        bridge.record_call(&format!("BGSCodeObj.NotARealMethod{i}"), &[]);
    }

    assert_eq!(
        bridge.unknown_methods().len(),
        crate::MAX_DISTINCT_HOST_METHOD_NAMES
    );
}

/// #2964 — `known_methods` is populated only by trusted `register_method`
/// callers today, not movie content, but is capped for the same
/// defense-in-depth reason `resource_errors` (#2720) caps a channel nothing
/// currently drives hard. A registration past the cap is silently dropped
/// (same shape as the other three sets): the name behaves exactly like one
/// that was never registered, which this proves by observing `record_call`
/// classify a late registration as `Unknown` rather than `Queued`.
#[test]
fn known_methods_cap_makes_a_late_registration_behave_unregistered() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::Fallout4Avm2);
    let overflow = 5usize;
    for i in 0..crate::MAX_DISTINCT_HOST_METHOD_NAMES + overflow {
        bridge.register_method(format!("CustomMethod{i}"));
    }

    // Registered well within the cap: genuinely known.
    bridge.record_call("BGSCodeObj.CustomMethod0", &[]);
    assert_eq!(
        bridge.drain_calls().pop().unwrap().dispatch,
        ScaleformHostDispatch::Queued
    );

    // Registered past the cap: the insert was dropped, so this name falls
    // through to `Unknown` exactly like an unregistered name would.
    let late_index = crate::MAX_DISTINCT_HOST_METHOD_NAMES + overflow - 1;
    bridge.record_call(&format!("BGSCodeObj.CustomMethod{late_index}"), &[]);
    assert_eq!(
        bridge.drain_calls().pop().unwrap().dispatch,
        ScaleformHostDispatch::Unknown
    );
}

/// #2964 — direct coverage of the shared bound `BridgeState::insert_bounded`
/// gives all four sets (`callbacks` and `unanswered_methods` can't
/// practically be driven past 1024 distinct *real* entries through the
/// public API — a Ruffle fixture would need 1024 distinct `addCallback`
/// names, and `unanswered_methods` only ever admits cataloged `Request`
/// methods, of which there are a few dozen). Exercising the mechanism
/// directly proves the same guarantee without a synthetic corpus.
#[test]
fn insert_bounded_caps_and_logs_once() {
    let mut set = std::collections::BTreeSet::new();
    let mut capped = false;

    for i in 0..crate::MAX_DISTINCT_HOST_METHOD_NAMES + 10 {
        super::BridgeState::insert_bounded(&mut set, &mut capped, "test_set", format!("n{i}"));
    }

    assert_eq!(set.len(), crate::MAX_DISTINCT_HOST_METHOD_NAMES);
    assert!(capped);
    assert!(set.contains("n0"));
    assert!(!set.contains(&format!("n{}", crate::MAX_DISTINCT_HOST_METHOD_NAMES)));

    // A name already present is still a no-op success, not a second drop —
    // re-observing a known name must never itself be blocked by the cap.
    let len_before = set.len();
    super::BridgeState::insert_bounded(&mut set, &mut capped, "test_set", "n0".to_string());
    assert_eq!(set.len(), len_before);
}

/// The drop counter is evidence that a gap happened, so it must survive the
/// drain that clears the backlog.
#[test]
fn dropped_call_count_is_monotonic_across_drains() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::Fallout4Avm2);
    for _ in 0..crate::MAX_QUEUED_CALLS + 3 {
        bridge.record_call("BGSCodeObj.PlaySound", &[]);
    }
    assert_eq!(bridge.dropped_calls(), 3);

    bridge.drain_calls();
    assert_eq!(bridge.queued_call_count(), 0);
    assert_eq!(
        bridge.dropped_calls(),
        3,
        "a drain clears the backlog, not the record that entries were lost"
    );

    bridge.record_call("BGSCodeObj.PlaySound", &[]);
    assert_eq!(bridge.dropped_calls(), 3);
    assert_eq!(bridge.queued_call_count(), 1);
}

/// #2969 — the pairing `drain_calls`' doc requires, from the consumer's side.
///
/// The survivors of an overflow are internally contiguous, so the batch alone
/// cannot show that anything is missing; only `dropped_calls` says the record
/// has a hole. That is exactly why the engine's per-frame drain reads both
/// (`byroredux/src/app_frame.rs`, via `UiManager::dropped_host_calls`) — it
/// used to read only the batch, which meant a lost call was invisible to
/// everything downstream of it.
#[test]
fn a_full_batch_hides_its_gap_unless_dropped_calls_is_read_with_it() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::Fallout4Avm2);
    let recorded = crate::MAX_QUEUED_CALLS + 5;
    for _ in 0..recorded {
        bridge.record_call("BGSCodeObj.PlaySound", &[]);
    }

    let batch = bridge.drain_calls();
    assert_eq!(batch.len(), crate::MAX_QUEUED_CALLS);
    for pair in batch.windows(2) {
        assert_eq!(
            pair[1].sequence,
            pair[0].sequence + 1,
            "the survivors are contiguous among themselves — nothing inside \
             the batch betrays the eviction"
        );
    }

    let first = batch.first().expect("a full batch is not empty").sequence;
    assert_eq!(
        first, 5,
        "the five evicted calls are the OLDEST, so the batch starts partway \
         through the menu's call history"
    );
    assert_eq!(bridge.dropped_calls(), 5);
    assert_eq!(
        batch.len() as u64 + bridge.dropped_calls(),
        recorded as u64,
        "batch + dropped is the complete record; either half alone is not"
    );
}

/// A draining consumer never reaches the bound — the case the engine's
/// per-frame drain puts us in.
#[test]
fn draining_every_frame_never_drops() {
    let bridge = ScaleformHostBridge::new(ScaleformProfile::Fallout4Avm2);
    for _ in 0..10 {
        for _ in 0..64 {
            bridge.record_call("BGSCodeObj.PlaySound", &[]);
        }
        assert_eq!(bridge.drain_calls().len(), 64);
    }
    assert_eq!(bridge.dropped_calls(), 0);
    assert_eq!(bridge.queued_call_count(), 0);
}

#[test]
fn avm1_input_and_external_interface_are_bidirectional_headlessly() {
    let (player, bridge) = run_fixture(AVM1_FIXTURE, ScaleformProfile::SkyrimAvm1);
    dispatch_representative_input(&player);
    assert_external_interface_round_trip(&player, &bridge, ScaleformProfile::SkyrimAvm1);
}

#[test]
fn avm2_input_and_external_interface_are_bidirectional_headlessly() {
    let (player, bridge) = run_fixture(AVM2_FIXTURE, ScaleformProfile::Fallout4Avm2);
    dispatch_representative_input(&player);
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
fn installed_fallout4_representative_menus_obey_host_object_lifecycle() {
    let archive_path = data_dir(
        "BYROREDUX_FO4_DATA",
        "/mnt/data/SteamLibrary/steamapps/common/Fallout 4/Data",
    )
    .join("Fallout4 - Interface.ba2");
    let archive = Arc::new(Ba2Archive::open(&archive_path).expect("open Fallout 4 interface BA2"));
    let cases = [
        (
            "HUD",
            "interface\\hudmenu.swf",
            ScaleformHostObjectState::AdapterInjected,
        ),
        (
            "Pip-Boy",
            "interface\\pipboymenu.swf",
            ScaleformHostObjectState::AdapterInjected,
        ),
        (
            "Atomic Command holotape",
            "programs\\atomiccommand.swf",
            ScaleformHostObjectState::NotPresent,
        ),
    ];

    for (label, path, expected_state) in cases {
        let mut player = crate::SwfPlayer::from_resource_provider(
            archive.clone(),
            path,
            64,
            64,
            Some(ScaleformProfile::Fallout4Avm2),
        )
        .unwrap_or_else(|error| panic!("load {label} and its archive imports: {error}"));
        assert_eq!(
            player.host_object_state(),
            expected_state,
            "{label} host-object contract"
        );

        for _ in 0..3 {
            player.tick(1.0 / 30.0);
        }
        assert!(player.current_frame().is_some(), "{label} did not start");
        assert_eq!(player.resource_error(), None, "{label} resource loading");
        assert_eq!(player.profile(), ScaleformProfile::Fallout4Avm2);

        let bridge = player.host_bridge();
        if expected_state.adapter_injected() {
            assert!(bridge.has_callback(crate::avm2_host::LOADED_CALLBACK));
            assert!(
                bridge.has_callback(crate::avm2_host::READY_CALLBACK),
                "{label} installer did not finish; callbacks={:?}, calls={:?}",
                bridge.available_callbacks(),
                bridge.drain_calls()
            );
            assert_eq!(
                player.invoke_callback(crate::avm2_host::READY_CALLBACK, []),
                Some(ScaleformValue::Bool(true)),
                "{label} readiness callback"
            );
            assert_eq!(
                bridge.has_callback(crate::avm2_host::DESTROY_CALLBACK),
                expected_state.has_destroy_hook()
            );
        } else {
            assert!(!bridge.has_callback(crate::avm2_host::READY_CALLBACK));
            assert!(!bridge.has_callback(crate::avm2_host::DESTROY_CALLBACK));
        }

        drop(player);
        assert_eq!(
            bridge.code_object_destruction_count(),
            u64::from(expected_state.has_destroy_hook()),
            "{label} destruction hook"
        );
        assert!(
            bridge.unknown_methods().is_empty(),
            "{label} unknown methods: {:?}",
            bridge.unknown_methods()
        );
    }
}
