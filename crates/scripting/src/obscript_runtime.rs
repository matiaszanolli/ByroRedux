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
use crate::obscript::{decode_exact_load_order_expression, ObscriptDialect};
use crate::vm_state::ScriptVariables;

const BEGIN: u16 = 0x10;
const END: u16 = 0x11;
const SHORT: u16 = 0x12;
const LONG: u16 = 0x13;
const FLOAT: u16 = 0x14;
const SET_TO: u16 = 0x15;
const REFERENCE_FUNCTION: u16 = 0x1c;
const SCRIPT_NAME: u16 = 0x1d;
const REF: u16 = 0x1f;

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
    dialect: Option<ObscriptDialect>,
) -> bool {
    let program = if let Some(source) = script.source.as_deref() {
        compile_legacy_obscript_program(script, source)
    } else {
        dialect.and_then(|dialect| compile_legacy_obscript_bytecode_program(script, dialect))
    };
    let Some(program) = program else {
        return false;
    };
    world.insert(entity, program);
    if !world.has::<ScriptVariables>(entity) {
        world.insert(entity, ScriptVariables::default());
    }
    true
}

/// Compile source-less `SCDA` only when every statement in a supported event
/// block is a local assignment whose right-hand side is one exact supported
/// load-order call. General expressions and control flow are declined.
pub fn compile_legacy_obscript_bytecode_program(
    script: &ScriptRecord,
    dialect: ObscriptDialect,
) -> Option<LegacyObscriptProgram> {
    let mut program = LegacyObscriptProgram::default();
    let mut block: Option<(
        Option<LegacyObscriptEvent>,
        bool,
        Vec<LegacyObscriptAssignment>,
    )> = None;
    let mut offset = 0usize;

    while offset < script.compiled.len() {
        let (opcode, payload, next) = compiled_line(&script.compiled, offset)?;
        offset = next;

        if opcode == BEGIN {
            if block.is_some() {
                return None;
            }
            if payload.len() < 6 {
                return None;
            }
            let event = compiled_event(read_u16(payload, 0)?).filter(|_| payload.len() == 6);
            block = Some((event, true, Vec::new()));
            continue;
        }
        if opcode == END {
            if !payload.is_empty() {
                return None;
            }
            let (event, valid, assignments) = block.take()?;
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
            if !matches!(opcode, SHORT | LONG | FLOAT | SCRIPT_NAME | REF) {
                return None;
            }
            continue;
        };
        if event.is_none() || !*valid {
            continue;
        }
        if opcode != SET_TO {
            *valid = false;
            assignments.clear();
            continue;
        }
        let Some(assignment) = parse_compiled_assignment(script, payload, dialect) else {
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

fn compiled_line(bytes: &[u8], offset: usize) -> Option<(u16, &[u8], usize)> {
    let opcode = read_u16(bytes, offset)?;
    let (actual_opcode, length_offset, payload_offset) = if opcode == REFERENCE_FUNCTION {
        (read_u16(bytes, offset + 4)?, offset + 6, offset + 8)
    } else {
        (opcode, offset + 2, offset + 4)
    };
    let payload_len = usize::from(read_u16(bytes, length_offset)?);
    let payload_end = payload_offset.checked_add(payload_len)?;
    Some((
        actual_opcode,
        bytes.get(payload_offset..payload_end)?,
        payload_end,
    ))
}

fn compiled_event(opcode: u16) -> Option<LegacyObscriptEvent> {
    match opcode {
        0 => Some(LegacyObscriptEvent::GameMode),
        2 => Some(LegacyObscriptEvent::OnActivate),
        21 => Some(LegacyObscriptEvent::OnLoad),
        _ => None,
    }
}

fn parse_compiled_assignment(
    script: &ScriptRecord,
    payload: &[u8],
    dialect: ObscriptDialect,
) -> Option<LegacyObscriptAssignment> {
    if !matches!(payload.first(), Some(b's' | b'f')) {
        return None;
    }
    let local_index = u32::from(read_u16(payload, 1)?);
    let expression_len = usize::from(read_u16(payload, 3)?);
    let expression_end = 5usize.checked_add(expression_len)?;
    if expression_end != payload.len() {
        return None;
    }
    let local = script
        .locals
        .iter()
        .find(|local| local.index == local_index)?;
    let call = decode_exact_load_order_expression(&payload[5..expression_end], dialect)?;
    Some(LegacyObscriptAssignment {
        target: local.name.clone(),
        call,
    })
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    Some(u16::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
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

    fn framed(opcode: u16, payload: &[u8]) -> Vec<u8> {
        let mut bytes = opcode.to_le_bytes().to_vec();
        bytes.extend_from_slice(&(payload.len() as u16).to_le_bytes());
        bytes.extend_from_slice(payload);
        bytes
    }

    fn compiled_assignment(event: u16, local_index: u16, command: u16, plugin: &str) -> Vec<u8> {
        let mut begin = event.to_le_bytes().to_vec();
        begin.extend_from_slice(&0u32.to_le_bytes());

        let mut arguments = 1u16.to_le_bytes().to_vec();
        arguments.extend_from_slice(&(plugin.len() as u16).to_le_bytes());
        arguments.extend_from_slice(plugin.as_bytes());
        let mut expression = vec![b'X'];
        expression.extend_from_slice(&command.to_le_bytes());
        expression.extend_from_slice(&(arguments.len() as u16).to_le_bytes());
        expression.extend_from_slice(&arguments);
        let mut assignment = vec![b's'];
        assignment.extend_from_slice(&local_index.to_le_bytes());
        assignment.extend_from_slice(&(expression.len() as u16).to_le_bytes());
        assignment.extend_from_slice(&expression);

        let mut compiled = framed(BEGIN, &begin);
        compiled.extend(framed(SET_TO, &assignment));
        compiled.extend(framed(END, &[]));
        compiled
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
            &script(source),
            None,
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

    #[test]
    fn compiled_pure_handler_lowers_but_control_flow_declines() {
        let mut record = script("");
        record.source = None;
        record.locals[1].index = 7;
        record.compiled = compiled_assignment(21, 7, 0x14af, "Companion Pack.esp");

        let program =
            compile_legacy_obscript_bytecode_program(&record, ObscriptDialect::Xnvse).unwrap();
        assert_eq!(program.handler(LegacyObscriptEvent::OnLoad).len(), 1);
        assert_eq!(
            program.handler(LegacyObscriptEvent::OnLoad)[0].target,
            "index"
        );

        let begin_len = framed(BEGIN, &[21, 0, 0, 0, 0, 0]).len();
        record
            .compiled
            .splice(begin_len..begin_len, framed(0x16, &[0, 0, 0, 0]));
        assert!(
            compile_legacy_obscript_bytecode_program(&record, ObscriptDialect::Xnvse).is_none()
        );
    }

    #[test]
    fn source_less_compiled_handler_attaches_and_executes() {
        let mut record = script("");
        record.source = None;
        record.locals[1].index = 7;
        record.compiled = compiled_assignment(21, 7, 0x14af, "Companion Pack.esp");

        let mut world = World::new();
        crate::register(&mut world);
        set_legacy_obscript_content_catalog(&world, catalog());
        let entity = world.spawn();
        assert!(attach_legacy_obscript_program(
            &mut world,
            entity,
            &record,
            Some(ObscriptDialect::Xnvse),
        ));
        world.insert(entity, OnCellLoadEvent);

        legacy_obscript_load_order_system(&world, 0.0);
        assert_eq!(
            world
                .get::<ScriptVariables>(entity)
                .unwrap()
                .get_by_name("index"),
            Some(1.0)
        );
    }

    #[test]
    fn preserved_source_rejection_does_not_fall_back_to_compiled_bytes() {
        let source = r#"
            begin OnLoad
                if IsModLoaded "Companion Pack.esp"
                    set index to GetModIndex "Companion Pack.esp"
                endif
            end
        "#;
        let mut record = script(source);
        record.locals[1].index = 7;
        record.compiled = compiled_assignment(21, 7, 0x14af, "Companion Pack.esp");

        let mut world = World::new();
        crate::register(&mut world);
        let entity = world.spawn();
        assert!(!attach_legacy_obscript_program(
            &mut world,
            entity,
            &record,
            Some(ObscriptDialect::Xnvse),
        ));
        assert!(!world.has::<LegacyObscriptProgram>(entity));
    }
}
