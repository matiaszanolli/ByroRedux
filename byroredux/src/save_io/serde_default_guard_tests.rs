//! Extracted from `save_io.rs`'s inline `mod tests` (#2407 / TD1-004).
//! Production code there is ~1030 LOC; the test bulk alone pushed the
//! file past 3000. Split by topic, contents unchanged.

/// Source files that define the save-participating types registered in
/// [`build_save_registry`] — top-level columns AND the types nested
/// inside them (an `Inventory`'s `ItemStack`, an `AnimationStack`'s
/// `AnimationLayer`, the `FormIdPair` behind the form-id key column, …).
///
/// KEEP IN LOCKSTEP with `build_save_registry`: registering a new saved
/// type (or nesting a new type inside a saved column) means adding its
/// defining file here so the SAVE-D2-01 guard below scans it.
/// Paths are relative to `CARGO_MANIFEST_DIR` (the `byroredux/` crate).
const SAVE_TYPE_SOURCES: &[&str] = &[
    "../crates/core/src/ecs/packed.rs",                  // Transform
    "../crates/core/src/ecs/components/name.rs",         // Name
    "../crates/core/src/ecs/components/hierarchy.rs",    // Parent, Children
    "../crates/core/src/ecs/components/inventory.rs", // Inventory, EquipmentSlots, ItemStack, InventoryIndex
    "../crates/core/src/ecs/components/light.rs",     // LightSource, LightFlicker
    "../crates/core/src/ecs/components/form_id.rs",   // FormIdComponent
    "../crates/core/src/ecs/components/actor_values.rs", // ActorValues
    "../crates/core/src/form_id.rs",                  // FormIdPair (the serialised key)
    "../crates/core/src/animation/player.rs",         // AnimationPlayer
    "../crates/core/src/animation/stack.rs",          // AnimationStack, AnimationLayer
    "../crates/core/src/ecs/resources/mod.rs",        // ItemInstancePool, ItemInstance
    "../crates/scripting/src/timer.rs",               // ScriptTimer
    "../crates/scripting/src/vm_state.rs",            // TwoStateActivator, ScriptVariables
    "../crates/plugin/src/esm/records/condition.rs",  // ConditionStringId nested in ScriptVariables
    "../crates/scripting/src/quest_stages.rs", // QuestStageState, QuestObjectiveState + nested types
    "../crates/scripting/src/scene.rs",        // QuestAliasInjectionState grant ledger
    "src/cell_loader/transition.rs",           // CurrentCellContext
    "src/save_io.rs",                          // PlayerPose
    "src/components/game_time.rs",             // GameTimeRes
    "../crates/core/src/ecs/components/wander.rs", // WanderState (+ WanderBehavior, WanderPhase)
    "../crates/core/src/ecs/components/travel.rs", // TravelState, Traveled (+ TravelBehavior)
    "../crates/core/src/ecs/components/follow.rs", // FollowState (+ FollowBehavior)
    "../crates/core/src/ecs/components/escort.rs", // EscortState, Escorted (+ EscortBehavior)
    "../crates/core/src/ecs/components/guard.rs", // GuardState (+ GuardBehavior)
    "../crates/core/src/ecs/components/patrol.rs", // PatrolState (+ PatrolBehavior)
    "../crates/core/src/ecs/components/sandbox.rs", // Seated (+ SandboxBehavior)
    // #2537 / SAVE-D2-19 — six files carrying ~23 save-participating
    // serde-derived types, wired into `build_save_registry` by the
    // #2378/#2379/#2380/#2381/#2382/c5202627 commit sequence but never
    // added here. Same failure class as #2015/SAVE-D2-03
    // (actor_values.rs), now recurred once already — see this guard's
    // own doc comment above for why a missing entry here is a silent
    // blind spot, not a build error.
    "../crates/core/src/ecs/components/material.rs", // Material, EffectFalloff, ShaderTypeFields, PbrMaterial, EmissiveSource
    "../crates/core/src/ecs/components/collision.rs", // RigidBodyData (+ MotionType)
    "../crates/scripting/src/papyrus_demo/mod.rs",   // RumbleOnActivate (+ RumbleState)
    "../crates/scripting/src/cinematic.rs", // ActorCinematicState, HorseTetherState, CinematicPresentationState + nested types
    "../crates/scripting/src/player_control.rs", // PlayerControlState, ActorControlState (+ PlayerControlSelection)
    "../crates/scripting/src/fragment.rs",       // FragmentExecutionQueue (+ nested types)
];

/// Does `line` carry a `#[serde(...)]` attribute that declares
/// `default`, in any key position?
///
/// SAVE-D2-NEW-07 (#2181) — the guard below used to test
/// `starts_with("#[serde(default")`, which catches `#[serde(default)]`
/// and `#[serde(default, ...)]` but misses the semantically identical
/// `#[serde(skip_serializing_if = "...", default)]`, a legal and
/// idiomatic serde ordering. That ordering exists nowhere in
/// [`SAVE_TYPE_SOURCES`] today, so this closes a blind spot rather than
/// a live gap.
///
/// A bare `line.contains("default")` would close it too, but it
/// false-positives on a *value* that merely spells the word — e.g.
/// `#[serde(skip_serializing_if = "Option::is_default")]` or
/// `#[serde(rename = "default")]`, neither of which default-fills
/// anything. So this parses the attribute's key list instead: split the
/// parenthesised body on top-level commas (commas inside string
/// literals don't count), take each entry's key (the text before any
/// `=`), and match `default` exactly.
///
/// Residual, deliberately: an attribute rustfmt has broken across lines
/// is only seen one fragment at a time. `default` alone on its own line
/// still matches (the continuation is scanned as its own line and the
/// `#[serde(` fragment opens the body), but a key list wrapped so that
/// `default` shares a line with neither is not reachable by a
/// line-oriented scan. Same class of admitted residual as the
/// new-`Option` half documented on the guard itself.
fn serde_attr_declares_default(line: &str) -> bool {
    let trimmed = line.trim_start();
    // Attribute form only, so a comment or string mention of the
    // attribute (this file has several) doesn't self-trip the scan.
    let Some(rest) = trimmed.strip_prefix("#[serde(") else {
        return false;
    };
    // Body is everything up to the closing `)]`; a wrapped attribute
    // whose first line has no `)]` contributes what it does have.
    let body = match rest.rfind(")]") {
        Some(end) => &rest[..end],
        None => rest,
    };
    let mut in_string = false;
    for key in body
        .split(|c| {
            if c == '"' {
                in_string = !in_string;
            }
            c == ',' && !in_string
        })
        // The key is the text left of `=`; a valueless key is the whole
        // entry. `rename = "default"` yields `rename`, not `default`.
        .map(|entry| entry.split('=').next().unwrap_or("").trim())
    {
        if key == "default" {
            return true;
        }
    }
    false
}

/// SAVE-D2-01 (#1714) — a save-participating struct must not gain a
/// `#[serde(default)]` field without a [`FORMAT_MAJOR`] bump.
///
/// `schema_fingerprint` hashes column *type keys*, not field layout, so
/// an intra-type field change slips past it. serde's required-field
/// backstop only rejects an old save when the new field is *required*; a
/// `#[serde(default)]` field default-fills a missing column entry on an
/// old save, loading it **silently downgraded**. Until a versioned
/// migrator chain exists, the only safe shape change is a `FORMAT_MAJOR`
/// bump (which `decode` rejects across).
///
/// This guard trips on the explicit-`#[serde(default)]` half of the
/// footgun. The new-`Option` half can't be caught statically (legitimate
/// `Option`s already exist in saved structs — e.g.
/// `EquipmentSlots::occupants`, `AnimationStack::root_entity`); it rides
/// the doc rule on [`byroredux_save::FORMAT_MAJOR`]. Static source scan,
/// mirroring the `texture.rs` / `draw.rs` `include_str!` ordering checks.
#[test]
fn serde_default_on_saved_struct_requires_format_major_bump() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let mut offenders = Vec::new();
    for rel in SAVE_TYPE_SOURCES {
        let path = std::path::Path::new(manifest).join(rel);
        let src = std::fs::read_to_string(&path).unwrap_or_else(|e| {
            panic!(
                "SAVE-D2-01 guard can't read {} ({e}); a save-participating \
                 type's file moved — update SAVE_TYPE_SOURCES.",
                path.display()
            )
        });
        for (i, line) in src.lines().enumerate() {
            // #2181 — key-position-independent; see the helper.
            if serde_attr_declares_default(line) {
                offenders.push(format!("{rel}:{}", i + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "SAVE-D2-01 (#1714): `#[serde(default)]` on a save-participating \
         struct masks an intra-type change at load — schema_fingerprint is \
         type-key-only, so an old save loads silently default-filled. Bump \
         byroredux_save::FORMAT_MAJOR (+ add a migrator) or drop the \
         default. Offenders: {offenders:?}",
    );
}

/// SAVE-D2-NEW-07 (#2181) — the ordering the old line-prefix match
/// missed. `default` as a non-first key is exactly as dangerous as
/// `default` as the first one: both silently default-fill a missing
/// column on an old save.
#[test]
fn serde_guard_catches_default_in_any_key_position() {
    // The reported blind spot, verbatim from the issue.
    assert!(serde_attr_declares_default(
        r#"#[serde(skip_serializing_if = "Vec::is_empty", default)]"#
    ));
    // …and with a value, still not first.
    assert!(serde_attr_declares_default(
        r#"    #[serde(rename = "n", default = "Vec::new")]"#
    ));
    // Third position, after two other keys.
    assert!(serde_attr_declares_default(
        r#"#[serde(rename = "n", skip_serializing_if = "Vec::is_empty", default)]"#
    ));
}

/// The forms the original prefix match already caught must keep
/// tripping — broadening the check must not narrow it.
#[test]
fn serde_guard_still_catches_the_original_first_key_forms() {
    assert!(serde_attr_declares_default("#[serde(default)]"));
    assert!(serde_attr_declares_default(
        r#"#[serde(default = "default_layers")]"#
    ));
    assert!(serde_attr_declares_default(
        r#"    #[serde(default, rename = "n")]"#
    ));
}

/// The reason this parses keys instead of using `contains("default")`:
/// a *value* that merely spells the word default-fills nothing, and a
/// guard that trips on it would be noise the next maintainer silences.
#[test]
fn serde_guard_ignores_default_appearing_only_as_a_value() {
    assert!(!serde_attr_declares_default(
        r#"#[serde(skip_serializing_if = "Option::is_default")]"#
    ));
    assert!(!serde_attr_declares_default(
        r#"#[serde(rename = "default")]"#
    ));
    assert!(!serde_attr_declares_default(
        r#"#[serde(with = "crate::default_codec")]"#
    ));
}

/// Attribute form only — prose and string mentions of the attribute
/// (this file is full of them, including the assert message below)
/// must not self-trip the scan.
#[test]
fn serde_guard_ignores_non_attribute_mentions() {
    assert!(!serde_attr_declares_default(
        "/// a `#[serde(default)]` field default-fills a missing column"
    ));
    assert!(!serde_attr_declares_default("// #[serde(default)]"));
    assert!(!serde_attr_declares_default(
        r##"    let s = "#[serde(default)]";"##
    ));
    // A different attribute that happens to name a `default` key.
    assert!(!serde_attr_declares_default("#[builder(default)]"));
}

/// A comma inside a string literal is not a key separator — the split
/// must not mistake the tail of a quoted path for a new key.
#[test]
fn serde_guard_does_not_split_on_commas_inside_string_literals() {
    assert!(!serde_attr_declares_default(
        r#"#[serde(rename = "a,default")]"#
    ));
    assert!(serde_attr_declares_default(
        r#"#[serde(rename = "a,b", default)]"#
    ));
}
