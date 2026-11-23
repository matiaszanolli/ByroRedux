//! Recognizer for Skyrim's `default2StateActivator` behavior family.

use byroredux_papyrus::ast::{Expr, Script, ScriptItem};
use byroredux_plugin::esm::records::script_instance::PropertyValue;

use crate::translate::archetype::{RecognizeCtx, Recognized};
use crate::translate::source::ScriptSource;
use crate::vm_state::{ScriptVariables, TwoStateActivator};

const SCRIPT_NAME: &str = "default2StateActivator";

pub fn recognize(ctx: &RecognizeCtx<'_>) -> Option<Recognized> {
    let ScriptSource::PapyrusSource(script) = ctx.source else {
        return None;
    };
    if !script.name.node.eq_ignore_case(SCRIPT_NAME) {
        return None;
    }

    // The `?` on each `vmad_bool` is the decline (#2669): a VMAD property
    // that is PRESENT but carries a non-`Bool` payload must abandon the
    // whole recognizer, not quietly adopt the `.psc`-authored default. Only
    // a genuinely absent property (`Some(None)`) falls through to
    // `bool_prop`.
    // #7306 — an `isOpen` the placement's VMAD authors is that placement's choice; without one, a
    // door whose `attach_door` already seeded its state from `Open by Default` keeps it (resolved at
    // spawn, below), and anything else takes the script's default.
    let authored_open = vmad_bool(ctx, "isOpen")?;
    let default_open = bool_prop(script, "isOpen")?.unwrap_or(false);
    let is_animating = vmad_bool(ctx, "isAnimating")?
        .or(bool_prop(script, "isAnimating")?)
        .unwrap_or(false);
    let do_once = vmad_bool(ctx, "doOnce")?
        .or(bool_prop(script, "doOnce")?)
        .unwrap_or(false);
    Some(Recognized::new(
        format!("two_state_activator@{}", script.name.node),
        move |world, entity| {
            let is_open = authored_open.unwrap_or_else(|| {
                world.get::<TwoStateActivator>(entity).map_or(default_open, |seeded| seeded.is_open)
            });
            let state = TwoStateActivator { is_open, is_animating, do_once, activated_once: false };
            world.insert(entity, state);
            let mut variables = ScriptVariables::default();
            variables.set_by_name("::isOpen_var", f32::from(is_open));
            variables.set_by_name("::isAnimating_var", f32::from(is_animating));
            variables.set_by_name("::doOnce_var", f32::from(do_once));
            world.insert(entity, variables);
        },
    ))
}

fn prop_init<'a>(script: &'a Script, name: &str) -> Option<&'a Expr> {
    script.body.iter().find_map(|item| match &item.node {
        ScriptItem::Property(property) if property.name.node.eq_ignore_case(name) => {
            property.initial_value.as_ref().map(|value| &value.node)
        }
        _ => None,
    })
}

fn bool_prop(script: &Script, name: &str) -> Option<Option<bool>> {
    match prop_init(script, name) {
        None => Some(None),
        Some(Expr::BoolLit(value)) => Some(Some(*value)),
        Some(_) => None,
    }
}

/// Read a VMAD-authored `Bool` property, with the crate's three-case
/// contract (#2669, mirroring [`bool_prop`] above and
/// `translate::effects::bool_arg`):
///
/// * `Some(Some(v))` — the property is present and really is a `Bool`;
/// * `Some(None)` — no such property (no VMAD, no instance of this script,
///   or the script instance authors no property by this name), so the
///   caller should fall through to the `.psc` default;
/// * `None` — the property IS present but its `PropertyValue` is not
///   `Bool`, so its real value is unreadable and the recognizer must
///   **decline**.
///
/// Pre-fix this returned `Option<bool>` and collapsed the last two cases
/// into one `None`, which the caller turned into the authored default. That
/// is the same two-case collapse #2023 fixed in `bool_arg` and #1909 fixed
/// in `rumble::bool_prop`: a `default2StateActivator` whose VMAD carries
/// `isOpen` under a non-`Bool` type tag would spawn in the *wrong state* —
/// a door or gate open when it should be shut — with no diagnostic. A
/// partial lowering is worse than none.
fn vmad_bool(ctx: &RecognizeCtx<'_>, name: &str) -> Option<Option<bool>> {
    let Some(instance) = ctx.script_instance else {
        return Some(None);
    };
    let Some(script) = instance.script(SCRIPT_NAME) else {
        return Some(None);
    };
    let Some(property) = script.property(name) else {
        return Some(None);
    };
    match property.value {
        PropertyValue::Bool(value) => Some(Some(value)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_core::ecs::world::World;
    use byroredux_papyrus::parse_script;
    use byroredux_plugin::esm::reader::GameKind;
    use byroredux_plugin::esm::records::script_instance::{
        ScriptInstance, ScriptInstanceData, ScriptProperty,
    };

    #[test]
    fn vmad_do_once_overrides_script_default() {
        let source = "ScriptName default2StateActivator extends ObjectReference\n\
                      Bool Property isOpen = False Auto\n\
                      Bool Property doOnce = False Auto\n";
        let (script, errors) = parse_script(source).expect("source parses");
        assert!(errors.is_empty());
        let vmad = ScriptInstanceData {
            scripts: vec![ScriptInstance {
                name: SCRIPT_NAME.to_owned(),
                status: 1,
                properties: vec![ScriptProperty {
                    name: "doOnce".to_owned(),
                    status: 1,
                    value: PropertyValue::Bool(true),
                }],
            }],
            ..Default::default()
        };
        let recognized = crate::translate_script(
            &ScriptSource::PapyrusSource(&script),
            GameKind::Skyrim,
            Some(&vmad),
            None,
        )
        .expect("recognized");
        let mut world = World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);

        let state = world.get::<TwoStateActivator>(entity).unwrap();
        assert!(state.do_once);
        assert!(!state.is_open);
    }

    /// Build a `default2StateActivator` VMAD carrying one property under an
    /// arbitrary `PropertyValue` payload.
    fn vmad_with(name: &str, value: PropertyValue) -> ScriptInstanceData {
        ScriptInstanceData {
            scripts: vec![ScriptInstance {
                name: SCRIPT_NAME.to_owned(),
                status: 1,
                properties: vec![ScriptProperty {
                    name: name.to_owned(),
                    status: 1,
                    value,
                }],
            }],
            ..Default::default()
        }
    }

    const SOURCE: &str = "ScriptName default2StateActivator extends ObjectReference\n\
                          Bool Property isOpen = True Auto\n\
                          Bool Property isAnimating = False Auto\n\
                          Bool Property doOnce = False Auto\n";

    /// Regression for #2669. A VMAD property that is PRESENT but whose
    /// `PropertyValue` is not `Bool` has a real authored value this
    /// recognizer cannot read. Pre-fix `vmad_bool` collapsed that into the
    /// same `None` as "absent", and the caller turned it into the
    /// `.psc`-authored default — so a gate whose VMAD really said "open"
    /// under an unexpected type tag spawned shut, silently.
    ///
    /// A partial lowering is worse than none: the recognizer must decline
    /// and leave the entity un-lowered rather than commit a wrong state.
    #[test]
    fn non_bool_vmad_property_declines_instead_of_taking_the_script_default() {
        let (script, errors) = parse_script(SOURCE).expect("source parses");
        assert!(errors.is_empty());

        for value in [
            PropertyValue::Int32(1),
            PropertyValue::Float(1.0),
            PropertyValue::String("True".to_owned()),
            PropertyValue::Unknown(9),
        ] {
            let vmad = vmad_with("isOpen", value.clone());
            assert!(
                crate::translate_script(
                    &ScriptSource::PapyrusSource(&script),
                    GameKind::Skyrim,
                    Some(&vmad),
                    None,
                )
                .is_none(),
                "a present-but-unreadable `isOpen` ({value:?}) must decline \
                 the recognizer, not silently adopt the script default"
            );
        }
    }

    /// The other half of the contract: a VMAD that simply authors no
    /// property by that name is NOT an error — it falls through to the
    /// `.psc` default, exactly as before. Without this, #2669's fix would
    /// have turned every partially-authored VMAD into a decline.
    #[test]
    fn absent_vmad_property_still_falls_through_to_the_script_default() {
        let (script, errors) = parse_script(SOURCE).expect("source parses");
        assert!(errors.is_empty());

        // A VMAD for this script that mentions only `doOnce`; `isOpen` is
        // absent and must come from the `.psc` (`= True`).
        let vmad = vmad_with("doOnce", PropertyValue::Bool(true));
        let recognized = crate::translate_script(
            &ScriptSource::PapyrusSource(&script),
            GameKind::Skyrim,
            Some(&vmad),
            None,
        )
        .expect("an absent property must not decline");
        let mut world = World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);

        {
            let state = world.get::<TwoStateActivator>(entity).unwrap();
            assert!(state.do_once, "the authored VMAD value wins");
            assert!(
                state.is_open,
                "the absent one falls back to the .psc default"
            );
        }

        // No VMAD at all is the same case.
        let recognized = crate::translate_script(
            &ScriptSource::PapyrusSource(&script),
            GameKind::Skyrim,
            None,
            None,
        )
        .expect("no VMAD must not decline");
        let entity = world.spawn();
        (recognized.spawn)(&mut world, entity);
        let state = world.get::<TwoStateActivator>(entity).unwrap();
        assert!(state.is_open);
        assert!(!state.do_once);
    }

    /// #7306 — a door's `attach_door` seeds its state from `Open by Default` before the script
    /// attaches. With no `isOpen` on the placement's VMAD the seed stays (the `.psc` default does not
    /// override the placement's flag); an `isOpen` the VMAD authors is the placement's own choice and
    /// wins; with no seed the script default applies, as before.
    #[test]
    fn a_seeded_open_state_survives_unless_the_placement_authors_is_open() {
        let source = "ScriptName default2StateActivator extends ObjectReference\n\
                      Bool Property isOpen = False Auto\n";
        let (script, errors) = parse_script(source).expect("source parses");
        assert!(errors.is_empty());
        let spawn = |vmad: ScriptInstanceData, seeded: Option<bool>| {
            let recognized = crate::translate_script(&ScriptSource::PapyrusSource(&script), GameKind::Skyrim, Some(&vmad), None)
                .expect("recognized");
            let mut world = World::new();
            crate::register(&mut world);
            let entity = world.spawn();
            if let Some(is_open) = seeded {
                world.insert(entity, TwoStateActivator { is_open, is_animating: false, do_once: false, activated_once: false });
            }
            (recognized.spawn)(&mut world, entity);
            let open = world.get::<TwoStateActivator>(entity).unwrap().is_open;
            let variable = world.get::<ScriptVariables>(entity).unwrap().get_by_name("::isOpen_var");
            assert_eq!(variable, Some(f32::from(open)), "the script variable mirrors the state");
            open
        };
        let silent = || vmad_with("doOnce", PropertyValue::Bool(false));
        assert!(spawn(silent(), Some(true)), "Open by Default survives a script that authors no isOpen");
        assert!(!spawn(vmad_with("isOpen", PropertyValue::Bool(false)), Some(true)), "an authored isOpen wins");
        assert!(!spawn(silent(), None), "no seed: the script default");
    }
}
