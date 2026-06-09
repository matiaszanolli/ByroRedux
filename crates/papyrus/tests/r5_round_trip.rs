//! M30.2 Phase 4 — round-trip the four R5 source `.psc` files through
//! the full parser to validate end-to-end shape: lex → preprocess →
//! parse_script → typed AST with the expected items.
//!
//! These are the same scripts the R5 prototype hand-translated, so
//! they exercise every grammar feature that M47.2's transpiler will
//! need to handle: `ScriptName Extends`, `Auto State` / `State`
//! blocks, typed Properties with initializers, Events with parameter
//! lists + bodies, If/EndIf control flow, member-access call chains
//! (`Self.GetOwningQuest().SetStage(40)`), `as` casts, comparison
//! operators, line comments, doc comments.
//!
//! Sources live at `docs/r5/source/*.psc` for reference next to the
//! Rust translations. Including them via `include_str!` keeps the
//! fixture path stable across CI checkouts.

use byroredux_papyrus::ast::*;
use byroredux_papyrus::parse_script;

fn parse_or_panic(label: &str, source: &str) -> Script {
    match parse_script(source) {
        Ok((script, recovered)) => {
            assert!(
                recovered.is_empty(),
                "{label} parsed with {} recovered errors: {:#?}",
                recovered.len(),
                recovered,
            );
            script
        }
        Err(errors) => panic!(
            "{label} parse failed with {} hard errors: {:#?}",
            errors.len(),
            errors,
        ),
    }
}

#[test]
fn parse_default_rumble_on_activate() {
    let src = include_str!("../../../docs/r5/source/defaultRumbleOnActivate.psc");
    let script = parse_or_panic("defaultRumbleOnActivate.psc", src);

    assert_eq!(script.name.node.0, "defaultRumbleOnActivate");
    assert_eq!(script.parent.as_ref().unwrap().node.0, "objectreference");

    // 5 properties + 3 states.
    let props: Vec<&Property> = script
        .body
        .iter()
        .filter_map(|i| match &i.node {
            ScriptItem::Property(p) => Some(p.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(
        props.len(),
        5,
        "expected 5 Auto properties (cameraIntensity, duration, repeatable, shakeLeft, shakeRight)"
    );
    let prop_names: Vec<&str> = props.iter().map(|p| p.name.node.0.as_str()).collect();
    assert!(prop_names.contains(&"cameraIntensity"));
    assert!(prop_names.contains(&"duration"));
    assert!(prop_names.contains(&"repeatable"));
    assert!(prop_names.contains(&"shakeLeft"));
    assert!(prop_names.contains(&"shakeRight"));
    // Every property must be Auto (matches the .psc source).
    for p in &props {
        assert!(
            p.flags.contains(PropertyFlags::AUTO),
            "{} must be flagged Auto",
            p.name.node.0
        );
    }

    let states: Vec<&State> = script
        .body
        .iter()
        .filter_map(|i| match &i.node {
            ScriptItem::State(s) => Some(s),
            _ => None,
        })
        .collect();
    assert_eq!(
        states.len(),
        3,
        "expected 3 states: active / busy / inactive"
    );
    let state_names: Vec<&str> = states.iter().map(|s| s.name.node.0.as_str()).collect();
    assert!(state_names.contains(&"active"));
    assert!(state_names.contains(&"busy"));
    assert!(state_names.contains(&"inactive"));

    // `active` is the Auto state.
    let active = states.iter().find(|s| s.name.node.0 == "active").unwrap();
    assert!(active.is_auto, "'active' must be the Auto state");
}

#[test]
fn parse_da10_main_door_script() {
    let src = include_str!("../../../docs/r5/source/DA10MainDoorScript.psc");
    let script = parse_or_panic("DA10MainDoorScript.psc", src);

    assert_eq!(script.name.node.0, "DA10MainDoorScript");
    assert_eq!(script.parent.as_ref().unwrap().node.0, "ReferenceAlias");

    // Single OnActivate event with the stage-gated SetStage body.
    let events: Vec<&Event> = script
        .body
        .iter()
        .filter_map(|i| match &i.node {
            ScriptItem::Event(e) => Some(e),
            _ => None,
        })
        .collect();
    assert_eq!(events.len(), 1, "expected one OnActivate event");
    let ev = events[0];
    assert_eq!(ev.name.node.0, "OnActivate");
    assert_eq!(ev.params.len(), 1);
    assert_eq!(ev.params[0].name.node.0, "akActionRef");

    // Body has one If statement.
    assert_eq!(ev.body.len(), 1, "expected one If statement in the body");
    assert!(
        matches!(ev.body[0].node, Stmt::If { .. }),
        "expected If statement, got {:?}",
        ev.body[0].node
    );
}

#[test]
fn parse_mg07_labyrinthian_door_script() {
    let src = include_str!("../../../docs/r5/source/MG07LabyrinthianDoorScript.psc");
    let script = parse_or_panic("MG07LabyrinthianDoorScript.psc", src);

    assert_eq!(script.name.node.0, "MG07LabyrinthianDoorSCRIPT");
    assert_eq!(script.parent.as_ref().unwrap().node.0, "ObjectReference");

    // Variables: beenOpened
    let vars: Vec<&Variable> = script
        .body
        .iter()
        .filter_map(|i| match &i.node {
            ScriptItem::Variable(v) => Some(v),
            _ => None,
        })
        .collect();
    let var_names: Vec<&str> = vars.iter().map(|v| v.name.node.0.as_str()).collect();
    assert!(
        var_names.contains(&"beenOpened"),
        "expected `beenOpened` script-level variable, found {var_names:?}"
    );

    // Properties: MG07, MG07Keystone, delayAfterInsert,
    // dunLabyrinthianDenialMSG, myDoor.
    let prop_names: Vec<&str> = script
        .body
        .iter()
        .filter_map(|i| match &i.node {
            ScriptItem::Property(p) => Some(p.name.node.0.as_str()),
            _ => None,
        })
        .collect();
    assert!(prop_names.contains(&"MG07"));
    assert!(prop_names.contains(&"MG07Keystone"));
    assert!(prop_names.contains(&"delayAfterInsert"));
    assert!(prop_names.contains(&"dunLabyrinthianDenialMSG"));
    assert!(prop_names.contains(&"myDoor"));

    // Two states: inactive (empty), waiting (with onActivate).
    let states: Vec<&State> = script
        .body
        .iter()
        .filter_map(|i| match &i.node {
            ScriptItem::State(s) => Some(s),
            _ => None,
        })
        .collect();
    let state_names: Vec<&str> = states.iter().map(|s| s.name.node.0.as_str()).collect();
    assert!(state_names.contains(&"inactive"));
    assert!(state_names.contains(&"waiting"));
    let waiting = states.iter().find(|s| s.name.node.0 == "waiting").unwrap();
    assert_eq!(
        waiting.body.len(),
        1,
        "waiting state should hold one Event (onActivate)"
    );

    // Top-level onLoad event.
    let events: Vec<&Event> = script
        .body
        .iter()
        .filter_map(|i| match &i.node {
            ScriptItem::Event(e) => Some(e),
            _ => None,
        })
        .collect();
    assert!(
        events.iter().any(|e| e.name.node.0 == "onLoad"),
        "top-level onLoad event missing; got {:?}",
        events
            .iter()
            .map(|e| e.name.node.0.as_str())
            .collect::<Vec<_>>()
    );
}

#[test]
fn parse_dlc2_ttr4a_player_script() {
    let src = include_str!("../../../docs/r5/source/DLC2TTR4aPlayerScript.psc");
    let script = parse_or_panic("DLC2TTR4aPlayerScript.psc", src);

    assert_eq!(script.name.node.0, "DLC2TTR4aPlayerScript");
    assert_eq!(script.parent.as_ref().unwrap().node.0, "ReferenceAlias");

    // This script's interesting axis is "RecurringUpdate / OnUpdate"
    // — the polled-stat pattern. We don't introspect deeper here
    // (specific function names are translation-detail, not grammar
    // pins), just verify the body is non-empty and parsed without
    // errors.
    assert!(!script.body.is_empty(), "script body must have items");
}
