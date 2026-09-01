//! Conservative ECS runtime for engine-native legacy load-order probes.
//!
//! This translator intentionally accepts only event handlers made entirely of
//! unconditional supported `Set` statements. A handler containing control
//! flow or any other executable statement is rejected as a unit, so partial
//! ObScript support cannot silently change the meaning of a real script.

use std::collections::BTreeMap;
use std::sync::Arc;

use byroredux_core::ecs::sparse_set::SparseSetStorage;
use byroredux_core::ecs::storage::{Component, EntityId};
use byroredux_core::ecs::{Resource, World};
use byroredux_plugin::esm::records::ScriptRecord;
use byroredux_sdk::compatibility::{
    adapt_legacy_obscript_load_order, LegacyObscriptLoadOrderCall, LegacyObscriptLoadOrderResult,
};
use byroredux_sdk::content::ContentCatalog;

use crate::events::{ActivateEvent, OnCellLoadEvent};
use crate::vm_state::ScriptVariables;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum LegacyObscriptEvent {
    GameMode,
    OnLoad,
    OnActivate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacyObscriptAssignment {
    pub target: String,
    pub call: LegacyObscriptLoadOrderCall,
}

/// Static translated behavior attached to one legacy scripted entity.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LegacyObscriptProgram {
    handlers: BTreeMap<LegacyObscriptEvent, Vec<LegacyObscriptAssignment>>,
}

impl Component for LegacyObscriptProgram {
    type Storage = SparseSetStorage<Self>;
}

impl LegacyObscriptProgram {
    pub fn handler(&self, event: LegacyObscriptEvent) -> &[LegacyObscriptAssignment] {
        self.handlers.get(&event).map_or(&[], Vec::as_slice)
    }

    pub fn is_empty(&self) -> bool {
        self.handlers.values().all(Vec::is_empty)
    }
}

/// Live immutable content snapshot used by built-in ObScript compatibility.
#[derive(Clone, Debug, Default)]
pub struct LegacyObscriptContentCatalog(pub Arc<ContentCatalog>);

impl Resource for LegacyObscriptContentCatalog {}

pub fn set_legacy_obscript_content_catalog(world: &World, catalog: Arc<ContentCatalog>) {
    if let Some(mut current) = world.try_resource_mut::<LegacyObscriptContentCatalog>() {
        if !Arc::ptr_eq(&current.0, &catalog) && *current.0 != *catalog {
            current.0 = catalog;
        }
    }
}

pub fn register(world: &mut World) {
    world.register::<LegacyObscriptProgram>();
    world.insert_resource(LegacyObscriptContentCatalog::default());
}

/// Translate supported source handlers and attach their static program plus
/// the existing save-backed numeric variable state.
pub fn attach_legacy_obscript_program(
    world: &mut World,
    entity: EntityId,
    script: &ScriptRecord,
) -> bool {
    let Some(source) = script.source.as_deref() else {
        return false;
    };
    let Some(program) = compile_legacy_obscript_program(script, source) else {
        return false;
    };
    world.insert(entity, program);
    if !world.has::<ScriptVariables>(entity) {
        world.insert(entity, ScriptVariables::default());
    }
    true
}

/// Compile only pure load-order-query handlers. Any unsupported statement or
/// control flow invalidates its enclosing handler rather than being ignored.
pub fn compile_legacy_obscript_program(
    script: &ScriptRecord,
    source: &str,
) -> Option<LegacyObscriptProgram> {
    let mut program = LegacyObscriptProgram::default();
    let mut block: Option<(
        Option<LegacyObscriptEvent>,
        bool,
        Vec<LegacyObscriptAssignment>,
    )> = None;

    for raw_line in source.lines() {
        let line = strip_comment(raw_line).trim();
        if line.is_empty() {
            continue;
        }
        let tokens = tokenize(line)?;
        if tokens.is_empty() {
            continue;
        }

        if tokens[0].eq_ignore_ascii_case("begin") {
            if block.is_some() {
                return None;
            }
            let event = (tokens.len() == 2)
                .then(|| event_from_name(&tokens[1]))
                .flatten();
            block = Some((event, true, Vec::new()));
            continue;
        }
        if tokens[0].eq_ignore_ascii_case("end") {
            let Some((event, valid, assignments)) = block.take() else {
                continue;
            };
            if let Some(event) = event.filter(|_| valid && !assignments.is_empty()) {
                program
                    .handlers
                    .entry(event)
                    .or_default()
                    .extend(assignments);
            }
            continue;
        }

        let Some((event, valid, assignments)) = block.as_mut() else {
            continue;
        };
        if event.is_none() || !*valid {
            continue;
        }
        let Some(assignment) = parse_assignment(script, &tokens) else {
            *valid = false;
            assignments.clear();
            continue;
        };
        assignments.push(assignment);
    }

    if block.is_some() || program.is_empty() {
        None
    } else {
        Some(program)
    }
}

fn event_from_name(name: &str) -> Option<LegacyObscriptEvent> {
    if name.eq_ignore_ascii_case("gamemode") {
        Some(LegacyObscriptEvent::GameMode)
    } else if name.eq_ignore_ascii_case("onload") {
        Some(LegacyObscriptEvent::OnLoad)
    } else if name.eq_ignore_ascii_case("onactivate") {
        Some(LegacyObscriptEvent::OnActivate)
    } else {
        None
    }
}

fn parse_assignment(script: &ScriptRecord, tokens: &[String]) -> Option<LegacyObscriptAssignment> {
    if tokens.len() < 4
        || !tokens[0].eq_ignore_ascii_case("set")
        || !tokens[2].eq_ignore_ascii_case("to")
    {
        return None;
    }
    let local = script
        .locals
        .iter()
        .find(|local| local.name.eq_ignore_ascii_case(&tokens[1]))?;
    let command = &tokens[3];
    let call = if command.eq_ignore_ascii_case("IsModLoaded") && tokens.len() == 5 {
        LegacyObscriptLoadOrderCall::IsModLoaded {
            plugin: tokens[4].clone(),
        }
    } else if command.eq_ignore_ascii_case("GetModIndex") && tokens.len() == 5 {
        LegacyObscriptLoadOrderCall::GetModIndex {
            plugin: tokens[4].clone(),
        }
    } else if command.eq_ignore_ascii_case("GetNumLoadedMods") && tokens.len() == 4 {
        LegacyObscriptLoadOrderCall::GetNumLoadedMods
    } else if command.eq_ignore_ascii_case("GetNumLoadedPlugins") && tokens.len() == 4 {
        LegacyObscriptLoadOrderCall::GetNumLoadedPlugins
    } else {
        return None;
    };
    Some(LegacyObscriptAssignment {
        target: local.name.clone(),
        call,
    })
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    for (index, byte) in line.bytes().enumerate() {
        match byte {
            b'"' => quoted = !quoted,
            b';' if !quoted => return &line[..index],
            _ => {}
        }
    }
    line
}

fn tokenize(line: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut quoted = false;
    for character in line.chars() {
        match character {
            '"' => quoted = !quoted,
            character if character.is_whitespace() && !quoted => {
                if !token.is_empty() {
                    tokens.push(std::mem::take(&mut token));
                }
            }
            _ => token.push(character),
        }
    }
    if quoted {
        return None;
    }
    if !token.is_empty() {
        tokens.push(token);
    }
    Some(tokens)
}

/// Execute translated load-order handlers against the live engine catalog.
pub fn legacy_obscript_load_order_system(world: &World, _dt: f32) {
    let catalog = match world.try_resource::<LegacyObscriptContentCatalog>() {
        Some(catalog) => Arc::clone(&catalog.0),
        None => return,
    };
    let loaded = world
        .query::<OnCellLoadEvent>()
        .map(|events| events.iter().map(|(entity, _)| entity).collect::<Vec<_>>())
        .unwrap_or_default();
    let activated = world
        .query::<ActivateEvent>()
        .map(|events| events.iter().map(|(entity, _)| entity).collect::<Vec<_>>())
        .unwrap_or_default();

    let mut writes = Vec::new();
    let Some(programs) = world.query::<LegacyObscriptProgram>() else {
        return;
    };
    for (entity, program) in programs.iter() {
        collect_writes(
            &catalog,
            entity,
            program.handler(LegacyObscriptEvent::GameMode),
            &mut writes,
        );
        if loaded.contains(&entity) {
            collect_writes(
                &catalog,
                entity,
                program.handler(LegacyObscriptEvent::OnLoad),
                &mut writes,
            );
        }
        if activated.contains(&entity) {
            collect_writes(
                &catalog,
                entity,
                program.handler(LegacyObscriptEvent::OnActivate),
                &mut writes,
            );
        }
    }
    drop(programs);

    let Some(mut variables) = world.query_mut::<ScriptVariables>() else {
        return;
    };
    for (entity, target, value) in writes {
        if let Some(variables) = variables.get_mut(entity) {
            variables.set_by_name(&target, value);
        }
    }
}

fn collect_writes(
    catalog: &ContentCatalog,
    entity: EntityId,
    assignments: &[LegacyObscriptAssignment],
    writes: &mut Vec<(EntityId, String, f32)>,
) {
    for assignment in assignments {
        let Ok(result) = adapt_legacy_obscript_load_order(catalog, assignment.call.clone()) else {
            continue;
        };
        let value = match result {
            LegacyObscriptLoadOrderResult::Bool(value) => f32::from(value),
            LegacyObscriptLoadOrderResult::Integer(value) => value as f32,
            LegacyObscriptLoadOrderResult::String(_) => continue,
        };
        writes.push((entity, assignment.target.clone(), value));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_plugin::esm::records::ScriptLocalVar;
    use byroredux_sdk::content::{PluginInfo, PluginKind};

    fn script(source: &str) -> ScriptRecord {
        ScriptRecord {
            source: Some(source.to_owned()),
            locals: vec![
                ScriptLocalVar {
                    index: 0,
                    var_type: 2,
                    name: "loaded".to_owned(),
                },
                ScriptLocalVar {
                    index: 1,
                    var_type: 2,
                    name: "index".to_owned(),
                },
            ],
            ..Default::default()
        }
    }

    fn catalog() -> Arc<ContentCatalog> {
        Arc::new(
            ContentCatalog::new(vec![
                PluginInfo::new("FalloutNV.esm", [1; 16], PluginKind::Regular).unwrap(),
                PluginInfo::new("Companion Pack.esp", [2; 16], PluginKind::Regular).unwrap(),
            ])
            .unwrap(),
        )
    }

    #[test]
    fn compiler_accepts_only_pure_supported_handlers() {
        let source = r#"
            scn CompatibilityGate
            begin GameMode
                set loaded to IsModLoaded "Companion Pack.esp" ; exact basename
                set index to GetModIndex "Companion Pack.esp"
            end
        "#;
        let program = compile_legacy_obscript_program(&script(source), source).unwrap();
        assert_eq!(program.handler(LegacyObscriptEvent::GameMode).len(), 2);

        let conditional = r#"
            begin GameMode
                if IsModLoaded "Companion Pack.esp"
                    set index to GetModIndex "Companion Pack.esp"
                endif
            end
        "#;
        assert!(compile_legacy_obscript_program(&script(conditional), conditional).is_none());

        let filtered = r#"
            begin OnActivate Player
                set index to GetModIndex "Companion Pack.esp"
            end
        "#;
        assert!(compile_legacy_obscript_program(&script(filtered), filtered).is_none());
    }

    #[test]
    fn live_handlers_write_existing_save_backed_script_variables() {
        let source = r#"
            begin OnLoad
                set loaded to IsModLoaded "Companion Pack.esp"
            end
            begin OnActivate
                set index to GetModIndex "Companion Pack.esp"
            end
        "#;
        let mut world = World::new();
        crate::register(&mut world);
        set_legacy_obscript_content_catalog(&world, catalog());
        let entity = world.spawn();
        assert!(attach_legacy_obscript_program(
            &mut world,
            entity,
            &script(source)
        ));
        world.insert(entity, OnCellLoadEvent);

        legacy_obscript_load_order_system(&world, 0.0);
        assert_eq!(
            world
                .get::<ScriptVariables>(entity)
                .unwrap()
                .get_by_name("loaded"),
            Some(1.0)
        );
        assert_eq!(
            world
                .get::<ScriptVariables>(entity)
                .unwrap()
                .get_by_name("index"),
            None
        );

        world.insert(entity, ActivateEvent { activator: entity });
        legacy_obscript_load_order_system(&world, 0.0);
        assert_eq!(
            world
                .get::<ScriptVariables>(entity)
                .unwrap()
                .get_by_name("index"),
            Some(1.0)
        );
    }
}
