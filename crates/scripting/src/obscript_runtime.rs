//! Conservative ECS runtime for engine-native legacy load-order probes.
//!
//! This translator intentionally accepts only exact supported load-order calls,
//! assignments, and bounded `if`/`elseif`/`else` trees. Any other executable
//! statement rejects its handler as a unit, so partial ObScript support cannot
//! silently change the meaning of a real script.

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
const IF: u16 = 0x16;
const ELSE: u16 = 0x17;
const ELSE_IF: u16 = 0x18;
const END_IF: u16 = 0x19;
const REFERENCE_FUNCTION: u16 = 0x1c;
const SCRIPT_NAME: u16 = 0x1d;
const REF: u16 = 0x1f;
const MAX_LEGACY_OBSCRIPT_NESTING: usize = 32;

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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LegacyObscriptStatement {
    Assignment(LegacyObscriptAssignment),
    If {
        condition: LegacyObscriptLoadOrderCall,
        then_branch: Vec<LegacyObscriptStatement>,
        else_branch: Vec<LegacyObscriptStatement>,
    },
}

/// Static translated behavior attached to one legacy scripted entity.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LegacyObscriptProgram {
    handlers: BTreeMap<LegacyObscriptEvent, Vec<LegacyObscriptStatement>>,
}

impl Component for LegacyObscriptProgram {
    type Storage = SparseSetStorage<Self>;
}

impl LegacyObscriptProgram {
    pub fn handler(&self, event: LegacyObscriptEvent) -> &[LegacyObscriptStatement] {
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
/// block is an exact supported assignment or bounded conditional statement.
pub fn compile_legacy_obscript_bytecode_program(
    script: &ScriptRecord,
    dialect: ObscriptDialect,
) -> Option<LegacyObscriptProgram> {
    let mut program = LegacyObscriptProgram::default();
    let mut block: Option<(Option<LegacyObscriptEvent>, Vec<CompiledLine<'_>>)> = None;
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
            block = Some((event, Vec::new()));
            continue;
        }
        if opcode == END {
            if !payload.is_empty() {
                return None;
            }
            let (event, lines) = block.take()?;
            if let Some(event) = event {
                let statements = parse_compiled_statements(script, dialect, &lines)?;
                if !statements.is_empty() {
                    program
                        .handlers
                        .entry(event)
                        .or_default()
                        .extend(statements);
                }
            }
            continue;
        }

        let Some((event, lines)) = block.as_mut() else {
            if !matches!(opcode, SHORT | LONG | FLOAT | SCRIPT_NAME | REF) {
                return None;
            }
            continue;
        };
        if event.is_some() {
            lines.push(CompiledLine { opcode, payload });
        }
    }

    if block.is_some() || program.is_empty() {
        None
    } else {
        Some(program)
    }
}

#[derive(Clone, Copy)]
struct CompiledLine<'a> {
    opcode: u16,
    payload: &'a [u8],
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum StatementTerminator {
    End,
    Else,
    ElseIf(LegacyObscriptLoadOrderCall),
    EndIf,
}

fn parse_compiled_statements(
    script: &ScriptRecord,
    dialect: ObscriptDialect,
    lines: &[CompiledLine<'_>],
) -> Option<Vec<LegacyObscriptStatement>> {
    let mut cursor = 0;
    let (statements, terminator) = parse_compiled_sequence(script, dialect, lines, &mut cursor, 0)?;
    (matches!(terminator, StatementTerminator::End) && cursor == lines.len()).then_some(statements)
}

fn parse_compiled_sequence(
    script: &ScriptRecord,
    dialect: ObscriptDialect,
    lines: &[CompiledLine<'_>],
    cursor: &mut usize,
    depth: usize,
) -> Option<(Vec<LegacyObscriptStatement>, StatementTerminator)> {
    let mut statements = Vec::new();
    while let Some(line) = lines.get(*cursor).copied() {
        *cursor += 1;
        match line.opcode {
            SET_TO => statements.push(LegacyObscriptStatement::Assignment(
                parse_compiled_assignment(script, line.payload, dialect)?,
            )),
            IF => statements.push(parse_compiled_conditional(
                script,
                dialect,
                lines,
                cursor,
                depth,
                parse_compiled_condition(line.payload, dialect)?,
            )?),
            ELSE if line.payload.len() == 2 => {
                return Some((statements, StatementTerminator::Else));
            }
            END_IF if line.payload.is_empty() => {
                return Some((statements, StatementTerminator::EndIf));
            }
            ELSE_IF => {
                return Some((
                    statements,
                    StatementTerminator::ElseIf(parse_compiled_condition(line.payload, dialect)?),
                ));
            }
            _ => return None,
        }
    }
    Some((statements, StatementTerminator::End))
}

fn parse_compiled_conditional(
    script: &ScriptRecord,
    dialect: ObscriptDialect,
    lines: &[CompiledLine<'_>],
    cursor: &mut usize,
    depth: usize,
    condition: LegacyObscriptLoadOrderCall,
) -> Option<LegacyObscriptStatement> {
    if depth >= MAX_LEGACY_OBSCRIPT_NESTING {
        return None;
    }
    let (then_branch, terminator) =
        parse_compiled_sequence(script, dialect, lines, cursor, depth + 1)?;
    let else_branch = match terminator {
        StatementTerminator::EndIf => Vec::new(),
        StatementTerminator::Else => {
            let (else_branch, terminator) =
                parse_compiled_sequence(script, dialect, lines, cursor, depth + 1)?;
            matches!(terminator, StatementTerminator::EndIf).then_some(else_branch)?
        }
        StatementTerminator::ElseIf(next_condition) => vec![parse_compiled_conditional(
            script,
            dialect,
            lines,
            cursor,
            depth + 1,
            next_condition,
        )?],
        StatementTerminator::End => return None,
    };
    Some(LegacyObscriptStatement::If {
        condition,
        then_branch,
        else_branch,
    })
}

fn parse_compiled_condition(
    payload: &[u8],
    dialect: ObscriptDialect,
) -> Option<LegacyObscriptLoadOrderCall> {
    let expression_len = usize::from(read_u16(payload, 2)?);
    let expression_end = 4usize.checked_add(expression_len)?;
    if expression_end != payload.len() {
        return None;
    }
    decode_exact_load_order_expression(&payload[4..expression_end], dialect)
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

/// Compile exact load-order-query assignments and bounded conditional trees.
/// Any unsupported statement invalidates its enclosing handler.
pub fn compile_legacy_obscript_program(
    script: &ScriptRecord,
    source: &str,
) -> Option<LegacyObscriptProgram> {
    let mut program = LegacyObscriptProgram::default();
    let mut block: Option<(Option<LegacyObscriptEvent>, Vec<Vec<String>>)> = None;

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
            block = Some((event, Vec::new()));
            continue;
        }
        if tokens[0].eq_ignore_ascii_case("end") {
            if tokens.len() != 1 {
                return None;
            }
            let Some((event, lines)) = block.take() else {
                continue;
            };
            if let Some(event) = event {
                let statements = parse_source_statements(script, &lines)?;
                if !statements.is_empty() {
                    program
                        .handlers
                        .entry(event)
                        .or_default()
                        .extend(statements);
                }
            }
            continue;
        }

        let Some((event, lines)) = block.as_mut() else {
            continue;
        };
        if event.is_some() {
            lines.push(tokens);
        }
    }

    if block.is_some() || program.is_empty() {
        None
    } else {
        Some(program)
    }
}

fn parse_source_statements(
    script: &ScriptRecord,
    lines: &[Vec<String>],
) -> Option<Vec<LegacyObscriptStatement>> {
    let mut cursor = 0;
    let (statements, terminator) = parse_source_sequence(script, lines, &mut cursor, 0)?;
    (matches!(terminator, StatementTerminator::End) && cursor == lines.len()).then_some(statements)
}

fn parse_source_sequence(
    script: &ScriptRecord,
    lines: &[Vec<String>],
    cursor: &mut usize,
    depth: usize,
) -> Option<(Vec<LegacyObscriptStatement>, StatementTerminator)> {
    let mut statements = Vec::new();
    while let Some(tokens) = lines.get(*cursor) {
        *cursor += 1;
        let keyword = tokens.first()?;
        if keyword.eq_ignore_ascii_case("if") {
            statements.push(parse_source_conditional(
                script,
                lines,
                cursor,
                depth,
                parse_source_call(&tokens[1..])?,
            )?);
        } else if keyword.eq_ignore_ascii_case("else") && tokens.len() == 1 {
            return Some((statements, StatementTerminator::Else));
        } else if keyword.eq_ignore_ascii_case("endif") && tokens.len() == 1 {
            return Some((statements, StatementTerminator::EndIf));
        } else if keyword.eq_ignore_ascii_case("elseif") {
            return Some((
                statements,
                StatementTerminator::ElseIf(parse_source_call(&tokens[1..])?),
            ));
        } else {
            statements.push(LegacyObscriptStatement::Assignment(parse_assignment(
                script, tokens,
            )?));
        }
    }
    Some((statements, StatementTerminator::End))
}

fn parse_source_conditional(
    script: &ScriptRecord,
    lines: &[Vec<String>],
    cursor: &mut usize,
    depth: usize,
    condition: LegacyObscriptLoadOrderCall,
) -> Option<LegacyObscriptStatement> {
    if depth >= MAX_LEGACY_OBSCRIPT_NESTING {
        return None;
    }
    let (then_branch, terminator) = parse_source_sequence(script, lines, cursor, depth + 1)?;
    let else_branch = match terminator {
        StatementTerminator::EndIf => Vec::new(),
        StatementTerminator::Else => {
            let (else_branch, terminator) =
                parse_source_sequence(script, lines, cursor, depth + 1)?;
            matches!(terminator, StatementTerminator::EndIf).then_some(else_branch)?
        }
        StatementTerminator::ElseIf(next_condition) => vec![parse_source_conditional(
            script,
            lines,
            cursor,
            depth + 1,
            next_condition,
        )?],
        StatementTerminator::End => return None,
    };
    Some(LegacyObscriptStatement::If {
        condition,
        then_branch,
        else_branch,
    })
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
    let call = parse_source_call(&tokens[3..])?;
    Some(LegacyObscriptAssignment {
        target: local.name.clone(),
        call,
    })
}

fn parse_source_call(tokens: &[String]) -> Option<LegacyObscriptLoadOrderCall> {
    let command = tokens.first()?;
    if command.eq_ignore_ascii_case("IsModLoaded") && tokens.len() == 2 {
        Some(LegacyObscriptLoadOrderCall::IsModLoaded {
            plugin: tokens[1].clone(),
        })
    } else if command.eq_ignore_ascii_case("GetModIndex") && tokens.len() == 2 {
        Some(LegacyObscriptLoadOrderCall::GetModIndex {
            plugin: tokens[1].clone(),
        })
    } else if command.eq_ignore_ascii_case("GetNumLoadedMods") && tokens.len() == 1 {
        Some(LegacyObscriptLoadOrderCall::GetNumLoadedMods)
    } else if command.eq_ignore_ascii_case("GetNumLoadedPlugins") && tokens.len() == 1 {
        Some(LegacyObscriptLoadOrderCall::GetNumLoadedPlugins)
    } else {
        None
    }
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
    statements: &[LegacyObscriptStatement],
    writes: &mut Vec<(EntityId, String, f32)>,
) {
    for statement in statements {
        match statement {
            LegacyObscriptStatement::Assignment(assignment) => {
                let Some(value) = evaluate_load_order_call(catalog, &assignment.call)
                    .and_then(load_order_result_number)
                else {
                    continue;
                };
                writes.push((entity, assignment.target.clone(), value));
            }
            LegacyObscriptStatement::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let branch = if evaluate_load_order_call(catalog, condition)
                    .is_some_and(load_order_result_truthy)
                {
                    then_branch
                } else {
                    else_branch
                };
                collect_writes(catalog, entity, branch, writes);
            }
        }
    }
}

fn evaluate_load_order_call(
    catalog: &ContentCatalog,
    call: &LegacyObscriptLoadOrderCall,
) -> Option<LegacyObscriptLoadOrderResult> {
    adapt_legacy_obscript_load_order(catalog, call.clone()).ok()
}

fn load_order_result_number(result: LegacyObscriptLoadOrderResult) -> Option<f32> {
    match result {
        LegacyObscriptLoadOrderResult::Bool(value) => Some(f32::from(value)),
        LegacyObscriptLoadOrderResult::Integer(value) => Some(value as f32),
        LegacyObscriptLoadOrderResult::String(_) => None,
    }
}

fn load_order_result_truthy(result: LegacyObscriptLoadOrderResult) -> bool {
    match result {
        LegacyObscriptLoadOrderResult::Bool(value) => value,
        LegacyObscriptLoadOrderResult::Integer(value) => value != 0,
        LegacyObscriptLoadOrderResult::String(value) => !value.is_empty(),
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

    fn compiled_expression(command: u16, plugin: &str) -> Vec<u8> {
        let mut arguments = 1u16.to_le_bytes().to_vec();
        arguments.extend_from_slice(&(plugin.len() as u16).to_le_bytes());
        arguments.extend_from_slice(plugin.as_bytes());
        let mut expression = vec![b'X'];
        expression.extend_from_slice(&command.to_le_bytes());
        expression.extend_from_slice(&(arguments.len() as u16).to_le_bytes());
        expression.extend_from_slice(&arguments);
        expression
    }

    fn compiled_assignment_payload(local_index: u16, command: u16, plugin: &str) -> Vec<u8> {
        let expression = compiled_expression(command, plugin);
        let mut assignment = vec![b's'];
        assignment.extend_from_slice(&local_index.to_le_bytes());
        assignment.extend_from_slice(&(expression.len() as u16).to_le_bytes());
        assignment.extend_from_slice(&expression);
        assignment
    }

    fn compiled_assignment(event: u16, local_index: u16, command: u16, plugin: &str) -> Vec<u8> {
        let mut begin = event.to_le_bytes().to_vec();
        begin.extend_from_slice(&0u32.to_le_bytes());

        let mut compiled = framed(BEGIN, &begin);
        compiled.extend(framed(
            SET_TO,
            &compiled_assignment_payload(local_index, command, plugin),
        ));
        compiled.extend(framed(END, &[]));
        compiled
    }

    fn compiled_condition_payload(plugin: &str) -> Vec<u8> {
        let expression = compiled_expression(0x14ae, plugin);
        let mut condition = 0u16.to_le_bytes().to_vec();
        condition.extend_from_slice(&(expression.len() as u16).to_le_bytes());
        condition.extend_from_slice(&expression);
        condition
    }

    fn compiled_conditional(event: u16, condition_plugin: &str) -> Vec<u8> {
        let mut begin = event.to_le_bytes().to_vec();
        begin.extend_from_slice(&0u32.to_le_bytes());

        let mut compiled = framed(BEGIN, &begin);
        compiled.extend(framed(IF, &compiled_condition_payload(condition_plugin)));
        compiled.extend(framed(
            SET_TO,
            &compiled_assignment_payload(7, 0x14af, "Missing.esp"),
        ));
        compiled.extend(framed(
            ELSE_IF,
            &compiled_condition_payload("Companion Pack.esp"),
        ));
        compiled.extend(framed(
            SET_TO,
            &compiled_assignment_payload(7, 0x14af, "Companion Pack.esp"),
        ));
        compiled.extend(framed(ELSE, &[0, 0]));
        compiled.extend(framed(
            SET_TO,
            &compiled_assignment_payload(7, 0x14af, "Missing.esp"),
        ));
        compiled.extend(framed(END_IF, &[]));
        compiled.extend(framed(END, &[]));
        compiled
    }

    #[test]
    fn source_compiler_accepts_assignments_and_bounded_conditionals() {
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
                else
                    set index to GetModIndex "Missing.esp"
                endif
            end
        "#;
        let conditional_program =
            compile_legacy_obscript_program(&script(conditional), conditional).unwrap();
        assert!(matches!(
            conditional_program.handler(LegacyObscriptEvent::GameMode),
            [LegacyObscriptStatement::If { .. }]
        ));

        let else_if = r#"
            begin GameMode
                if IsModLoaded "Missing.esp"
                    set index to GetModIndex "Missing.esp"
                elseif IsModLoaded "Companion Pack.esp"
                    set index to GetModIndex "Companion Pack.esp"
                endif
            end
        "#;
        let else_if_program = compile_legacy_obscript_program(&script(else_if), else_if).unwrap();
        assert!(matches!(
            else_if_program.handler(LegacyObscriptEvent::GameMode),
            [LegacyObscriptStatement::If { else_branch, .. }]
                if matches!(else_branch.as_slice(), [LegacyObscriptStatement::If { .. }])
        ));

        let unsupported_else_if = else_if.replace(
            "elseif IsModLoaded \"Companion Pack.esp\"",
            "elseif loaded == 1",
        );
        assert!(compile_legacy_obscript_program(
            &script(&unsupported_else_if),
            &unsupported_else_if
        )
        .is_none());

        let filtered = r#"
            begin OnActivate Player
                set index to GetModIndex "Companion Pack.esp"
            end
        "#;
        assert!(compile_legacy_obscript_program(&script(filtered), filtered).is_none());
    }

    #[test]
    fn source_compiler_rejects_excessive_nesting() {
        let mut source = "begin GameMode\n".to_owned();
        for _ in 0..=MAX_LEGACY_OBSCRIPT_NESTING {
            source.push_str("if IsModLoaded \"Companion Pack.esp\"\n");
        }
        source.push_str("set index to GetModIndex \"Companion Pack.esp\"\n");
        for _ in 0..=MAX_LEGACY_OBSCRIPT_NESTING {
            source.push_str("endif\n");
        }
        source.push_str("end\n");

        assert!(compile_legacy_obscript_program(&script(&source), &source).is_none());

        let mut chain = "begin GameMode\nif IsModLoaded \"Missing.esp\"\n".to_owned();
        chain.push_str("set index to GetModIndex \"Missing.esp\"\n");
        for _ in 0..MAX_LEGACY_OBSCRIPT_NESTING {
            chain.push_str("elseif IsModLoaded \"Missing.esp\"\n");
            chain.push_str("set index to GetModIndex \"Missing.esp\"\n");
        }
        chain.push_str("endif\nend\n");
        assert!(compile_legacy_obscript_program(&script(&chain), &chain).is_none());
    }

    #[test]
    fn source_conditionals_execute_only_the_selected_branch() {
        let source = r#"
            begin OnLoad
                if IsModLoaded "Missing.esp"
                    set index to GetModIndex "Missing.esp"
                elseif IsModLoaded "Companion Pack.esp"
                    set index to GetModIndex "Companion Pack.esp"
                else
                    set index to GetModIndex "Missing.esp"
                endif
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
                .get_by_name("index"),
            Some(1.0)
        );
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
    fn compiled_handler_lowers_assignments_and_conditionals() {
        let mut record = script("");
        record.source = None;
        record.locals[1].index = 7;
        record.compiled = compiled_assignment(21, 7, 0x14af, "Companion Pack.esp");

        let program =
            compile_legacy_obscript_bytecode_program(&record, ObscriptDialect::Xnvse).unwrap();
        assert_eq!(program.handler(LegacyObscriptEvent::OnLoad).len(), 1);
        assert!(matches!(
            program.handler(LegacyObscriptEvent::OnLoad),
            [LegacyObscriptStatement::Assignment(LegacyObscriptAssignment {
                target,
                ..
            })] if target == "index"
        ));

        record.compiled = compiled_conditional(21, "Missing.esp");
        let program =
            compile_legacy_obscript_bytecode_program(&record, ObscriptDialect::Xnvse).unwrap();
        assert!(matches!(
            program.handler(LegacyObscriptEvent::OnLoad),
            [LegacyObscriptStatement::If { .. }]
        ));

        let begin_len = framed(BEGIN, &[21, 0, 0, 0, 0, 0]).len();
        record
            .compiled
            .splice(begin_len..begin_len, framed(ELSE_IF, &[0, 0, 0, 0]));
        assert!(
            compile_legacy_obscript_bytecode_program(&record, ObscriptDialect::Xnvse).is_none()
        );

        record.compiled = compiled_conditional(21, "Missing.esp");
        record.compiled[begin_len + 6] += 1;
        assert!(
            compile_legacy_obscript_bytecode_program(&record, ObscriptDialect::Xnvse).is_none()
        );
    }

    #[test]
    fn compiled_conditionals_execute_only_the_selected_branch() {
        let mut record = script("");
        record.source = None;
        record.locals[1].index = 7;
        record.compiled = compiled_conditional(21, "Missing.esp");

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
                if loaded == 1
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
