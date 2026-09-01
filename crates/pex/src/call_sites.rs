//! Deterministic Papyrus call-site extraction for compatibility preflight.

use crate::{Function, FunctionType, Instruction, OpCode, Pex, Value};

/// Lexical owner of a bytecode call.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CallScope {
    StateFunction { state: String, function: String },
    PropertyGetter { property: String },
    PropertySetter { property: String },
}

/// Dispatch form encoded by one Papyrus call opcode.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum CallTarget {
    StaticType(String),
    Receiver(Option<String>),
    ParentType(String),
}

/// One structurally valid call found in a decoded PEX instruction stream.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CallSite {
    pub source_file: String,
    pub object: String,
    pub scope: CallScope,
    pub instruction_index: usize,
    pub source_line: Option<u16>,
    pub target: CallTarget,
    pub function: String,
    pub argument_count: usize,
}

/// Malformed call metadata retained as an actionable preflight diagnostic.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CallSiteDiagnostic {
    pub source_file: String,
    pub object: String,
    pub scope: CallScope,
    pub instruction_index: usize,
    pub source_line: Option<u16>,
    pub message: String,
}

/// Complete extraction result. Calls preserve object/property/state order and
/// instruction order from the PEX rather than being alphabetically reordered.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CallSiteScan {
    pub calls: Vec<CallSite>,
    pub diagnostics: Vec<CallSiteDiagnostic>,
}

impl Pex {
    /// Extract every method, parent, and static call from every object,
    /// including full-property getter and setter bodies.
    pub fn call_sites(&self) -> CallSiteScan {
        let mut scan = CallSiteScan::default();
        for object in &self.objects {
            for property in &object.properties {
                if let Some(function) = &property.read_function {
                    let scope = CallScope::PropertyGetter {
                        property: property.name.clone(),
                    };
                    scan_function(self, object, function, scope, &mut scan);
                }
                if let Some(function) = &property.write_function {
                    let scope = CallScope::PropertySetter {
                        property: property.name.clone(),
                    };
                    scan_function(self, object, function, scope, &mut scan);
                }
            }
            for state in &object.states {
                for function in &state.functions {
                    let scope = CallScope::StateFunction {
                        state: state.name.clone(),
                        function: function.name.clone(),
                    };
                    scan_function(self, object, function, scope, &mut scan);
                }
            }
        }
        scan
    }
}

fn scan_function(
    pex: &Pex,
    object: &crate::Object,
    function: &Function,
    scope: CallScope,
    scan: &mut CallSiteScan,
) {
    let line_numbers = debug_lines(pex, &object.name, &scope);
    for (instruction_index, instruction) in function.instructions.iter().enumerate() {
        if !matches!(
            instruction.op,
            OpCode::CallMethod | OpCode::CallParent | OpCode::CallStatic
        ) {
            continue;
        }
        let source_line = line_numbers.and_then(|lines| lines.get(instruction_index).copied());
        match decode_call(instruction, &object.parent_class_name) {
            Ok((target, called_function)) => scan.calls.push(CallSite {
                source_file: pex.header.source_file_name.clone(),
                object: object.name.clone(),
                scope: scope.clone(),
                instruction_index,
                source_line,
                target,
                function: called_function.to_owned(),
                argument_count: instruction.var_args.len(),
            }),
            Err(message) => scan.diagnostics.push(CallSiteDiagnostic {
                source_file: pex.header.source_file_name.clone(),
                object: object.name.clone(),
                scope: scope.clone(),
                instruction_index,
                source_line,
                message: message.to_owned(),
            }),
        }
    }
}

fn decode_call<'a>(
    instruction: &'a Instruction,
    parent_class_name: &str,
) -> Result<(CallTarget, &'a str), &'static str> {
    match instruction.op {
        OpCode::CallStatic => {
            let provider = identifier(
                instruction.args.first(),
                "static call has no identifier provider",
            )?;
            let function = identifier(
                instruction.args.get(1),
                "static call has no identifier function",
            )?;
            Ok((CallTarget::StaticType(provider.to_owned()), function))
        }
        OpCode::CallMethod => {
            let function = identifier(
                instruction.args.first(),
                "method call has no identifier function",
            )?;
            let receiver = instruction
                .args
                .get(1)
                .and_then(Value::as_identifier)
                .map(str::to_owned);
            Ok((CallTarget::Receiver(receiver), function))
        }
        OpCode::CallParent => {
            let function = identifier(
                instruction.args.first(),
                "parent call has no identifier function",
            )?;
            Ok((
                CallTarget::ParentType(parent_class_name.to_owned()),
                function,
            ))
        }
        _ => Err("instruction is not a Papyrus call opcode"),
    }
}

fn identifier<'a>(value: Option<&'a Value>, role: &'static str) -> Result<&'a str, &'static str> {
    value.and_then(Value::as_identifier).ok_or(role)
}

fn debug_lines<'a>(pex: &'a Pex, object: &str, scope: &CallScope) -> Option<&'a [u16]> {
    let (state, function, function_type) = match scope {
        CallScope::StateFunction { state, function } => {
            (state.as_str(), function.as_str(), FunctionType::Method)
        }
        CallScope::PropertyGetter { property } => ("", property.as_str(), FunctionType::Getter),
        CallScope::PropertySetter { property } => ("", property.as_str(), FunctionType::Setter),
    };
    pex.debug_info
        .function_infos
        .iter()
        .find(|info| {
            info.object_name.eq_ignore_ascii_case(object)
                && info.state_name.eq_ignore_ascii_case(state)
                && info.function_name.eq_ignore_ascii_case(function)
                && info.function_type == Some(function_type)
        })
        .map(|info| info.line_numbers.as_slice())
}

#[cfg(test)]
mod tests {
    use crate::{DebugInfo, FunctionInfo, Header, Object, Property, ScriptType, State};

    use super::*;

    fn call(op: OpCode, args: &[&str], argument_count: usize) -> Instruction {
        Instruction {
            op,
            args: args
                .iter()
                .map(|value| Value::Identifier((*value).to_owned()))
                .collect(),
            var_args: vec![Value::None; argument_count],
        }
    }

    #[test]
    fn scans_static_method_parent_and_property_calls_with_lines() {
        let getter = Function {
            instructions: vec![call(
                OpCode::CallStatic,
                &["StorageUtil", "GetIntValue", "::temp0"],
                2,
            )],
            ..Default::default()
        };
        let event = Function {
            name: "OnInit".to_owned(),
            instructions: vec![
                call(
                    OpCode::CallMethod,
                    &["RegisterForModEvent", "Self", "::nonevar"],
                    2,
                ),
                call(OpCode::CallParent, &["OnInit", "::nonevar"], 0),
            ],
            ..Default::default()
        };
        let pex = Pex {
            script_type: ScriptType::Skyrim,
            header: Header {
                source_file_name: "ExtenderFixture.psc".to_owned(),
                ..Default::default()
            },
            string_table: Vec::new(),
            debug_info: DebugInfo {
                present: true,
                function_infos: vec![
                    FunctionInfo {
                        object_name: "ExtenderFixture".to_owned(),
                        state_name: "".to_owned(),
                        function_name: "Count".to_owned(),
                        function_type: Some(FunctionType::Getter),
                        line_numbers: vec![7],
                    },
                    FunctionInfo {
                        object_name: "ExtenderFixture".to_owned(),
                        state_name: "".to_owned(),
                        function_name: "OnInit".to_owned(),
                        function_type: Some(FunctionType::Method),
                        line_numbers: vec![12, 13],
                    },
                ],
                ..Default::default()
            },
            user_flags: Vec::new(),
            objects: vec![Object {
                name: "ExtenderFixture".to_owned(),
                parent_class_name: "Quest".to_owned(),
                properties: vec![Property {
                    name: "Count".to_owned(),
                    read_function: Some(getter),
                    ..Default::default()
                }],
                states: vec![State {
                    functions: vec![event],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };

        let scan = pex.call_sites();
        assert!(scan.diagnostics.is_empty());
        assert_eq!(scan.calls.len(), 3);
        assert_eq!(scan.calls[0].source_line, Some(7));
        assert_eq!(
            scan.calls[0].target,
            CallTarget::StaticType("StorageUtil".to_owned())
        );
        assert_eq!(scan.calls[0].function, "GetIntValue");
        assert_eq!(scan.calls[0].argument_count, 2);
        assert_eq!(scan.calls[1].source_line, Some(12));
        assert_eq!(
            scan.calls[1].target,
            CallTarget::Receiver(Some("Self".to_owned()))
        );
        assert_eq!(
            scan.calls[2].target,
            CallTarget::ParentType("Quest".to_owned())
        );
    }

    #[test]
    fn malformed_calls_are_diagnostics_instead_of_silent_omissions() {
        let pex = Pex {
            script_type: ScriptType::Fallout4,
            header: Header {
                source_file_name: "Broken.psc".to_owned(),
                ..Default::default()
            },
            string_table: Vec::new(),
            debug_info: DebugInfo::default(),
            user_flags: Vec::new(),
            objects: vec![Object {
                name: "Broken".to_owned(),
                states: vec![State {
                    functions: vec![Function {
                        name: "OnInit".to_owned(),
                        instructions: vec![Instruction {
                            op: OpCode::CallStatic,
                            args: vec![Value::Integer(1)],
                            var_args: Vec::new(),
                        }],
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };
        let scan = pex.call_sites();
        assert!(scan.calls.is_empty());
        assert_eq!(scan.diagnostics.len(), 1);
        assert_eq!(scan.diagnostics[0].instruction_index, 0);
        assert_eq!(
            scan.diagnostics[0].message,
            "static call has no identifier provider"
        );
    }
}
