//! Extender-era Papyrus compatibility preflight over decoded PEX calls.

use byroredux_pex::{CallScope, CallSite, CallSiteDiagnostic, CallTarget, Pex};
use byroredux_sdk::compatibility::{
    classify_method_call, classify_static_call, CompatibilityDisposition, CompatibilityMatch,
};

/// One recognized extender-era call and its engine-level disposition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilityFinding {
    pub call: CallSite,
    pub compatibility: CompatibilityMatch,
}

/// Complete preflight result for one compiled script.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibilityReport {
    pub findings: Vec<CompatibilityFinding>,
    pub malformed_calls: Vec<CallSiteDiagnostic>,
}

impl CompatibilityReport {
    pub fn native_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.compatibility.disposition == CompatibilityDisposition::Native)
            .count()
    }

    pub fn mapped_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.compatibility.disposition == CompatibilityDisposition::Mapped)
            .count()
    }

    pub fn unsupported_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|finding| {
                finding.compatibility.disposition == CompatibilityDisposition::Unsupported
            })
            .count()
    }
}

/// Scan one already-decoded script without decompiling it. Vanilla and unknown
/// mod calls are omitted; known extender calls and malformed call metadata are
/// retained in source/instruction order.
pub fn analyze_pex_compatibility(pex: &Pex) -> CompatibilityReport {
    let scan = pex.call_sites();
    let findings = scan
        .calls
        .into_iter()
        .filter_map(|call| {
            let compatibility = match &call.target {
                CallTarget::StaticType(provider) => classify_static_call(provider, &call.function),
                CallTarget::Receiver(_) | CallTarget::ParentType(_) => {
                    classify_method_call(&call.function)
                }
            }?;
            Some(CompatibilityFinding {
                call,
                compatibility,
            })
        })
        .collect();
    CompatibilityReport {
        findings,
        malformed_calls: scan.diagnostics,
    }
}

/// Emit attributed, actionable preflight diagnostics before translation.
pub(crate) fn log_compatibility_report(report: &CompatibilityReport) {
    for finding in &report.findings {
        let location = call_location(&finding.call);
        let provider = match &finding.call.target {
            CallTarget::StaticType(provider) | CallTarget::ParentType(provider) => {
                provider.as_str()
            }
            CallTarget::Receiver(Some(receiver)) => receiver.as_str(),
            CallTarget::Receiver(None) => "<dynamic receiver>",
        };
        match finding.compatibility.disposition {
            CompatibilityDisposition::Native => log::debug!(
                "extension compatibility: {location}: {provider}.{} is engine-native via {}",
                finding.call.function,
                finding.compatibility.service.unwrap_or("the core runtime")
            ),
            CompatibilityDisposition::Mapped => log::warn!(
                "extension compatibility: {location}: {provider}.{} maps to {} but needs a source adapter/migration: {}",
                finding.call.function,
                finding.compatibility.service.unwrap_or("an engine service"),
                finding.compatibility.guidance
            ),
            CompatibilityDisposition::Unsupported => log::warn!(
                "extension compatibility: {location}: {provider}.{} is unsupported: {}",
                finding.call.function,
                finding.compatibility.guidance
            ),
        }
    }
    for diagnostic in &report.malformed_calls {
        let location = diagnostic_location(diagnostic);
        log::warn!(
            "extension compatibility: {location}: malformed call metadata: {}",
            diagnostic.message
        );
    }
}

fn call_location(call: &CallSite) -> String {
    format!(
        "{} [{}::{}]",
        source_location(&call.source_file, call.source_line, call.instruction_index),
        call.object,
        scope_label(&call.scope)
    )
}

fn diagnostic_location(diagnostic: &CallSiteDiagnostic) -> String {
    format!(
        "{} [{}::{}]",
        source_location(
            &diagnostic.source_file,
            diagnostic.source_line,
            diagnostic.instruction_index,
        ),
        diagnostic.object,
        scope_label(&diagnostic.scope)
    )
}

fn scope_label(scope: &CallScope) -> String {
    match scope {
        CallScope::StateFunction { state, function } if state.is_empty() => function.clone(),
        CallScope::StateFunction { state, function } => format!("{state}::{function}"),
        CallScope::PropertyGetter { property } => format!("get:{property}"),
        CallScope::PropertySetter { property } => format!("set:{property}"),
    }
}

fn source_location(source_file: &str, source_line: Option<u16>, instruction: usize) -> String {
    match source_line {
        Some(line) => format!("{source_file}:{line}"),
        None => format!("{source_file}:instruction-{instruction}"),
    }
}

#[cfg(test)]
mod tests {
    use byroredux_pex::{
        DebugInfo, Function, Header, Instruction, Object, OpCode, ScriptType, State, Value,
    };
    use byroredux_sdk::service::{EVENT_SERVICE, PRINCIPAL_STORAGE_SERVICE};

    use super::*;

    fn static_call(provider: &str, function: &str) -> Instruction {
        Instruction {
            op: OpCode::CallStatic,
            args: vec![
                Value::Identifier(provider.to_owned()),
                Value::Identifier(function.to_owned()),
                Value::Identifier("::nonevar".to_owned()),
            ],
            var_args: Vec::new(),
        }
    }

    fn method_call(function: &str) -> Instruction {
        Instruction {
            op: OpCode::CallMethod,
            args: vec![
                Value::Identifier(function.to_owned()),
                Value::Identifier("Self".to_owned()),
                Value::Identifier("::nonevar".to_owned()),
            ],
            var_args: Vec::new(),
        }
    }

    #[test]
    fn preflight_reports_mapped_unsupported_and_ignores_vanilla_calls() {
        let pex = Pex {
            script_type: ScriptType::Skyrim,
            header: Header {
                source_file_name: "ExtenderFixture.psc".to_owned(),
                ..Default::default()
            },
            string_table: Vec::new(),
            debug_info: DebugInfo::default(),
            user_flags: Vec::new(),
            objects: vec![Object {
                name: "ExtenderFixture".to_owned(),
                states: vec![State {
                    functions: vec![Function {
                        name: "OnInit".to_owned(),
                        instructions: vec![
                            static_call("StorageUtil", "GetIntValue"),
                            method_call("RegisterForModEvent"),
                            static_call("JsonUtil", "Load"),
                            static_call("Utility", "Wait"),
                        ],
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };

        let report = analyze_pex_compatibility(&pex);
        assert_eq!(report.findings.len(), 3);
        assert_eq!(report.mapped_count(), 2);
        assert_eq!(report.unsupported_count(), 1);
        assert_eq!(
            report.findings[0].compatibility.service,
            Some(PRINCIPAL_STORAGE_SERVICE)
        );
        assert_eq!(
            report.findings[1].compatibility.service,
            Some(EVENT_SERVICE)
        );
        assert!(report.malformed_calls.is_empty());
    }

    #[test]
    fn locations_prefer_source_lines_and_fall_back_to_instruction_offsets() {
        assert_eq!(source_location("A.psc", Some(8), 3), "A.psc:8");
        assert_eq!(source_location("A.psc", None, 3), "A.psc:instruction-3");
    }
}
