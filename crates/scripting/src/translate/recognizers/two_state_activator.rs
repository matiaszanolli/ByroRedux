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

    let is_open = vmad_bool(ctx, "isOpen")
        .or(bool_prop(script, "isOpen")?)
        .unwrap_or(false);
    let is_animating = vmad_bool(ctx, "isAnimating")
        .or(bool_prop(script, "isAnimating")?)
        .unwrap_or(false);
    let do_once = vmad_bool(ctx, "doOnce")
        .or(bool_prop(script, "doOnce")?)
        .unwrap_or(false);
    let state = TwoStateActivator {
        is_open,
        is_animating,
        do_once,
        activated_once: false,
    };

    Some(Recognized::new(
        format!("two_state_activator@{}", script.name.node),
        move |world, entity| {
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

fn vmad_bool(ctx: &RecognizeCtx<'_>, name: &str) -> Option<bool> {
    let property = ctx.script_instance?.script(SCRIPT_NAME)?.property(name)?;
    match property.value {
        PropertyValue::Bool(value) => Some(value),
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
}
