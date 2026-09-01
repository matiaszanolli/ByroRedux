//! Extender-era Papyrus compatibility preflight over decoded PEX calls.

use byroredux_papyrus::{
    ast::{Event, Expr, Function, Script, ScriptItem, StateItem, Stmt, Variable},
    span::{Span, Spanned},
};
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

/// One extender-era call found directly in Papyrus source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceCompatibilityFinding {
    pub scope: String,
    pub span: Span,
    pub source_line: usize,
    pub provider: Option<String>,
    pub function: String,
    pub argument_count: usize,
    pub compatibility: CompatibilityMatch,
}

/// Source-level companion to [`CompatibilityReport`]. Parser diagnostics stay
/// with the parser; this report contains only syntactically recognized calls.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SourceCompatibilityReport {
    pub findings: Vec<SourceCompatibilityFinding>,
}

impl SourceCompatibilityReport {
    pub fn native_count(&self) -> usize {
        self.count(CompatibilityDisposition::Native)
    }

    pub fn mapped_count(&self) -> usize {
        self.count(CompatibilityDisposition::Mapped)
    }

    pub fn unsupported_count(&self) -> usize {
        self.count(CompatibilityDisposition::Unsupported)
    }

    fn count(&self, disposition: CompatibilityDisposition) -> usize {
        self.findings
            .iter()
            .filter(|finding| finding.compatibility.disposition == disposition)
            .count()
    }
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

/// Scan a parsed Papyrus script before compilation. The caller retains parser
/// diagnostics, while this pass reports recognized extender APIs in source
/// order with byte spans and one-based line numbers.
pub fn analyze_source_compatibility(
    script: &Script,
    source_text: &str,
) -> SourceCompatibilityReport {
    let mut findings = Vec::new();
    for item in &script.body {
        match &item.node {
            ScriptItem::Variable(variable) => {
                scan_variable(variable, "<script>", source_text, &mut findings)
            }
            ScriptItem::Property(property) => {
                if let Some(value) = &property.initial_value {
                    scan_expr(value, &property.name.node.0, source_text, &mut findings);
                }
                if let Some(getter) = &property.getter {
                    scan_function(getter, source_text, &mut findings);
                }
                if let Some(setter) = &property.setter {
                    scan_function(setter, source_text, &mut findings);
                }
            }
            ScriptItem::Function(function) => scan_function(function, source_text, &mut findings),
            ScriptItem::Event(event) => scan_event(event, source_text, &mut findings),
            ScriptItem::State(state) => {
                for state_item in &state.body {
                    match &state_item.node {
                        StateItem::Function(function) => {
                            scan_function(function, source_text, &mut findings)
                        }
                        StateItem::Event(event) => scan_event(event, source_text, &mut findings),
                    }
                }
            }
            ScriptItem::Struct(structure) => {
                for member in &structure.members {
                    scan_variable(member, &structure.name.node.0, source_text, &mut findings);
                }
            }
            ScriptItem::Group(group) => {
                for property in &group.properties {
                    if let Some(value) = &property.node.initial_value {
                        scan_expr(
                            value,
                            &property.node.name.node.0,
                            source_text,
                            &mut findings,
                        );
                    }
                    if let Some(getter) = &property.node.getter {
                        scan_function(getter, source_text, &mut findings);
                    }
                    if let Some(setter) = &property.node.setter {
                        scan_function(setter, source_text, &mut findings);
                    }
                }
            }
            ScriptItem::Import(_) | ScriptItem::CustomEvent(_) => {}
        }
    }
    findings.sort_by_key(|finding| finding.span.start);
    SourceCompatibilityReport { findings }
}

fn scan_function(
    function: &Function,
    source_text: &str,
    findings: &mut Vec<SourceCompatibilityFinding>,
) {
    let scope = function.name.node.0.as_str();
    for param in &function.params {
        if let Some(default) = &param.default {
            scan_expr(default, scope, source_text, findings);
        }
    }
    scan_statements(&function.body, scope, source_text, findings);
}

fn scan_event(event: &Event, source_text: &str, findings: &mut Vec<SourceCompatibilityFinding>) {
    let scope = event.name.node.0.as_str();
    for param in &event.params {
        if let Some(default) = &param.default {
            scan_expr(default, scope, source_text, findings);
        }
    }
    scan_statements(&event.body, scope, source_text, findings);
}

fn scan_variable(
    variable: &Variable,
    scope: &str,
    source_text: &str,
    findings: &mut Vec<SourceCompatibilityFinding>,
) {
    if let Some(value) = &variable.initial_value {
        scan_expr(value, scope, source_text, findings);
    }
}

fn scan_statements(
    statements: &[Spanned<Stmt>],
    scope: &str,
    source_text: &str,
    findings: &mut Vec<SourceCompatibilityFinding>,
) {
    for statement in statements {
        match &statement.node {
            Stmt::Assign { target, value, .. } => {
                scan_expr(target, scope, source_text, findings);
                scan_expr(value, scope, source_text, findings);
            }
            Stmt::Return(value) => {
                if let Some(value) = value {
                    scan_expr(value, scope, source_text, findings);
                }
            }
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            } => {
                scan_expr(condition, scope, source_text, findings);
                scan_statements(body, scope, source_text, findings);
                for (condition, body) in elseif_clauses {
                    scan_expr(condition, scope, source_text, findings);
                    scan_statements(body, scope, source_text, findings);
                }
                if let Some(body) = else_body {
                    scan_statements(body, scope, source_text, findings);
                }
            }
            Stmt::While { condition, body } => {
                scan_expr(condition, scope, source_text, findings);
                scan_statements(body, scope, source_text, findings);
            }
            Stmt::ExprStmt(expr) => scan_expr(expr, scope, source_text, findings),
            Stmt::VarDecl(variable) => scan_variable(variable, scope, source_text, findings),
        }
    }
}

fn scan_expr(
    expr: &Spanned<Expr>,
    scope: &str,
    source_text: &str,
    findings: &mut Vec<SourceCompatibilityFinding>,
) {
    match &expr.node {
        Expr::Call { callee, args } => {
            if let Some((provider, function, compatibility)) = classify_source_callee(callee) {
                findings.push(SourceCompatibilityFinding {
                    scope: scope.to_owned(),
                    span: expr.span,
                    source_line: source_line(source_text, expr.span.start),
                    provider: provider.map(str::to_owned),
                    function: function.to_owned(),
                    argument_count: args.len(),
                    compatibility,
                });
            }
            scan_expr(callee, scope, source_text, findings);
            for arg in args {
                scan_expr(&arg.value, scope, source_text, findings);
            }
        }
        Expr::MemberAccess { object, .. } => scan_expr(object, scope, source_text, findings),
        Expr::Index { object, index } => {
            scan_expr(object, scope, source_text, findings);
            scan_expr(index, scope, source_text, findings);
        }
        Expr::UnaryOp { operand, .. } => scan_expr(operand, scope, source_text, findings),
        Expr::BinaryOp { left, right, .. } => {
            scan_expr(left, scope, source_text, findings);
            scan_expr(right, scope, source_text, findings);
        }
        Expr::Cast { expr, .. } => scan_expr(expr, scope, source_text, findings),
        Expr::New { size, .. } => scan_expr(size, scope, source_text, findings),
        Expr::ArrayLit(values) => {
            for value in values {
                scan_expr(value, scope, source_text, findings);
            }
        }
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::BoolLit(_)
        | Expr::StringLit(_)
        | Expr::NoneLit
        | Expr::Ident(_)
        | Expr::ParentAccess => {}
    }
}

fn classify_source_callee(
    callee: &Spanned<Expr>,
) -> Option<(Option<&str>, &str, CompatibilityMatch)> {
    match &callee.node {
        Expr::MemberAccess { object, member } => {
            let provider = match &object.node {
                Expr::Ident(identifier) => Some(identifier.0.as_str()),
                _ => None,
            };
            if let Some(provider) = provider {
                if let Some(compatibility) = classify_static_call(provider, &member.node.0) {
                    return Some((Some(provider), &member.node.0, compatibility));
                }
            }
            classify_method_call(&member.node.0)
                .map(|compatibility| (provider, member.node.0.as_str(), compatibility))
        }
        Expr::Ident(function) => classify_method_call(&function.0)
            .map(|compatibility| (None, function.0.as_str(), compatibility)),
        _ => None,
    }
}

fn source_line(source_text: &str, byte_offset: usize) -> usize {
    let end = byte_offset.min(source_text.len());
    source_text.as_bytes()[..end]
        .iter()
        .filter(|byte| **byte == b'\n')
        .count()
        + 1
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

    #[test]
    fn source_preflight_finds_nested_calls_in_source_order() {
        let source = r#"Scriptname ExtenderFixture extends Quest

int Function ReadVisits()
    return StorageUtil.GetIntValue(None, "visits", 0)
EndFunction

Event OnInit()
    RegisterForModEvent("byro:ready", "OnByroReady")
    int handle = JsonUtil.Load("unsafe.json")
    Utility.Wait(0.1)
EndEvent
"#;
        let (script, errors) = byroredux_papyrus::parse_script(source).expect("source lexes");
        assert!(errors.is_empty(), "source parse errors: {errors:?}");

        let report = analyze_source_compatibility(&script, source);
        assert_eq!(report.findings.len(), 3);
        assert_eq!(report.mapped_count(), 2);
        assert_eq!(report.unsupported_count(), 1);
        assert_eq!(report.native_count(), 0);

        let storage = &report.findings[0];
        assert_eq!(storage.scope, "ReadVisits");
        assert_eq!(storage.source_line, 4);
        assert_eq!(storage.provider.as_deref(), Some("StorageUtil"));
        assert_eq!(storage.function, "GetIntValue");
        assert_eq!(storage.argument_count, 3);

        let event = &report.findings[1];
        assert_eq!(event.scope, "OnInit");
        assert_eq!(event.source_line, 8);
        assert_eq!(event.provider, None);
        assert_eq!(event.function, "RegisterForModEvent");

        let json = &report.findings[2];
        assert_eq!(json.source_line, 9);
        assert_eq!(json.provider.as_deref(), Some("JsonUtil"));
        assert_eq!(
            json.compatibility.disposition,
            CompatibilityDisposition::Unsupported
        );
    }
}
