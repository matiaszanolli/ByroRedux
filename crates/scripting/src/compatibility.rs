//! Extender-era Papyrus compatibility preflight over decoded PEX calls.

use std::collections::{BTreeMap, BTreeSet};

use byroredux_core::ecs::Resource;
use byroredux_papyrus::{
    ast::{Event, Expr, Function, Script, ScriptItem, StateItem, Stmt, Variable},
    span::{Span, Spanned},
};
use byroredux_pex::{CallScope, CallSite, CallSiteDiagnostic, CallTarget, Pex};
pub use byroredux_sdk::compatibility::{
    classify_method_call, classify_obscript_command, classify_static_call, obscript_source_alias,
    source_alias, CompatibilityDisposition, CompatibilityMatch, ExtenderFamily, SourceAlias,
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

pub const MAX_COMPATIBILITY_SCRIPTS: usize = 65_536;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct CompatibilityScriptKey {
    source_file: String,
    fingerprint: u64,
}

/// Deduplicated compatibility evidence for every relevant compiled script
/// observed in the active world generation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CompatibilityRegistry {
    scripts: BTreeMap<CompatibilityScriptKey, CompatibilityReport>,
    truncated: bool,
}

impl Resource for CompatibilityRegistry {}

/// One provider/function aggregate across unique compiled scripts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompatibilitySummaryEntry {
    pub provider: String,
    pub function: String,
    pub compatibility: CompatibilityMatch,
    pub occurrences: usize,
    pub scripts: usize,
}

impl CompatibilityRegistry {
    /// Record one parsed PEX report. Returns true only for a new relevant
    /// script; repeated attachment of the same bytes is a no-op.
    pub fn record(&mut self, fingerprint: u64, report: CompatibilityReport) -> bool {
        let source_file = report
            .findings
            .first()
            .map(|finding| finding.call.source_file.clone())
            .or_else(|| {
                report
                    .malformed_calls
                    .first()
                    .map(|diagnostic| diagnostic.source_file.clone())
            });
        let Some(source_file) = source_file else {
            return false;
        };
        let key = CompatibilityScriptKey {
            source_file,
            fingerprint,
        };
        if self.scripts.contains_key(&key) {
            return false;
        }
        if self.scripts.len() == MAX_COMPATIBILITY_SCRIPTS {
            self.truncated = true;
            return false;
        }
        self.scripts.insert(key, report);
        true
    }

    pub fn clear(&mut self) {
        self.scripts.clear();
        self.truncated = false;
    }

    pub fn script_count(&self) -> usize {
        self.scripts.len()
    }

    pub fn finding_count(&self) -> usize {
        self.scripts
            .values()
            .map(|report| report.findings.len())
            .sum()
    }

    pub fn malformed_count(&self) -> usize {
        self.scripts
            .values()
            .map(|report| report.malformed_calls.len())
            .sum()
    }

    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    pub fn summary(&self) -> Vec<CompatibilitySummaryEntry> {
        #[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
        struct SummaryKey {
            provider: String,
            function: String,
            disposition: CompatibilityDisposition,
            service: Option<&'static str>,
        }

        let mut entries =
            BTreeMap::<SummaryKey, (CompatibilityMatch, usize, BTreeSet<(&str, u64)>)>::new();
        for (script, report) in &self.scripts {
            for finding in &report.findings {
                let provider = match &finding.call.target {
                    CallTarget::StaticType(provider) | CallTarget::ParentType(provider) => {
                        provider.clone()
                    }
                    CallTarget::Receiver(_) => "<instance>".to_owned(),
                };
                let key = SummaryKey {
                    provider: provider.to_ascii_lowercase(),
                    function: finding.call.function.to_ascii_lowercase(),
                    disposition: finding.compatibility.disposition,
                    service: finding.compatibility.service,
                };
                let entry = entries
                    .entry(key)
                    .or_insert_with(|| (finding.compatibility, 0, BTreeSet::new()));
                entry.1 += 1;
                entry.2.insert((&script.source_file, script.fingerprint));
            }
        }
        entries
            .into_iter()
            .map(
                |(key, (compatibility, occurrences, scripts))| CompatibilitySummaryEntry {
                    provider: key.provider,
                    function: key.function,
                    compatibility,
                    occurrences,
                    scripts: scripts.len(),
                },
            )
            .collect()
    }
}

/// Publish one detailed translation result into the world's compatibility
/// registry. Missing registries and reports are clean no-ops so standalone
/// translation users do not need engine setup.
pub fn record_compatibility_report(
    world: &byroredux_core::ecs::World,
    fingerprint: u64,
    report: Option<CompatibilityReport>,
) -> bool {
    let Some(report) = report else {
        return false;
    };
    world
        .try_resource_mut::<CompatibilityRegistry>()
        .is_some_and(|mut registry| registry.record(fingerprint, report))
}

/// Scan preserved Oblivion/FO3/FNV `SCTX` text for recognized extender
/// commands and express the result through the common compatibility report.
pub fn analyze_obscript_compatibility(source_file: &str, source_text: &str) -> CompatibilityReport {
    let mut findings = Vec::new();
    let mut byte_offset = 0usize;
    for (line_index, raw_line) in source_text.split_inclusive('\n').enumerate() {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let bytes = line.as_bytes();
        let mut index = 0usize;
        let mut quoted = false;
        while index < bytes.len() {
            match bytes[index] {
                b'"' => {
                    quoted = !quoted;
                    index += 1;
                }
                b';' if !quoted => break,
                byte if !quoted && (byte.is_ascii_alphabetic() || byte == b'_') => {
                    let start = index;
                    index += 1;
                    while index < bytes.len()
                        && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                    {
                        index += 1;
                    }
                    let command = &line[start..index];
                    let Some(compatibility) = classify_obscript_command(command) else {
                        continue;
                    };
                    let provider = match compatibility.family {
                        ExtenderFamily::Xnvse => "xNVSE",
                        ExtenderFamily::Obse => "OBSE",
                        ExtenderFamily::Shared => "xNVSE/OBSE",
                        _ => unreachable!("ObScript classifier returned a non-ObScript family"),
                    };
                    findings.push(CompatibilityFinding {
                        call: CallSite {
                            source_file: source_file.to_owned(),
                            object: "<obscript>".to_owned(),
                            scope: CallScope::StateFunction {
                                state: "ObScript".to_owned(),
                                function: "<script>".to_owned(),
                            },
                            instruction_index: byte_offset + start,
                            source_line: u16::try_from(line_index + 1).ok(),
                            target: CallTarget::StaticType(provider.to_owned()),
                            function: command.to_owned(),
                            argument_count: 0,
                        },
                        compatibility,
                    });
                }
                _ => index += 1,
            }
        }
        byte_offset = byte_offset.saturating_add(raw_line.len());
    }
    CompatibilityReport {
        findings,
        malformed_calls: Vec::new(),
    }
}

/// Stable fingerprint for preserved legacy script source/bytecode evidence.
pub fn legacy_script_fingerprint(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
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

    #[test]
    fn legacy_obscript_scanner_finds_probes_but_ignores_comments_and_strings() {
        let source = r#"scn VersionGate
Begin GameMode
  if GetNVSEVersion >= 6
    let revision := GetNVSERevision
  endif
  ; GetOBSEVersion in a comment
  Print "GetNVSEBeta in a string"
  if GetOBSERevision > 0
  endif
End"#;
        let report = analyze_obscript_compatibility("VersionGate", source);
        assert_eq!(report.findings.len(), 3);
        assert_eq!(report.findings[0].call.function, "GetNVSEVersion");
        assert_eq!(report.findings[0].call.source_line, Some(3));
        assert_eq!(report.findings[1].call.function, "GetNVSERevision");
        assert_eq!(report.findings[2].call.function, "GetOBSERevision");
        assert!(report.findings.iter().all(|finding| {
            finding.compatibility.disposition == CompatibilityDisposition::Mapped
                && finding.compatibility.service == Some(byroredux_sdk::service::CONTEXT_SERVICE)
        }));
        assert_eq!(
            legacy_script_fingerprint(source.as_bytes()),
            legacy_script_fingerprint(source.as_bytes())
        );

        let crlf =
            analyze_obscript_compatibility("CrlfGate", "if GetNVSEVersion\r\n  GetOBSEVersion\r\n");
        assert_eq!(crlf.findings[1].call.source_line, Some(2));
        assert_eq!(
            crlf.findings[1].call.instruction_index,
            "if GetNVSEVersion\r\n".len() + 2
        );
    }

    #[test]
    fn legacy_obscript_scanner_reports_shared_load_order_recipes() {
        let report = analyze_obscript_compatibility(
            "LoadOrderGate",
            "if IsModLoaded \"Companion.esp\"\n  set index to GetModIndex \"Companion.esp\"\nendif\n",
        );
        assert_eq!(report.findings.len(), 2);
        assert!(report.findings.iter().all(|finding| {
            finding.compatibility.family == ExtenderFamily::Shared
                && finding.compatibility.service
                    == Some(byroredux_sdk::service::CONTENT_CATALOG_SERVICE)
                && finding.call.target == CallTarget::StaticType("xNVSE/OBSE".to_owned())
        }));
    }

    #[test]
    fn registry_deduplicates_attachments_and_aggregates_script_variants() {
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
                            static_call("JsonUtil", "Load"),
                        ],
                        ..Default::default()
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
        };
        let report = analyze_pex_compatibility(&pex);
        let mut registry = CompatibilityRegistry::default();

        assert!(registry.record(10, report.clone()));
        assert!(!registry.record(10, report.clone()));
        assert!(registry.record(11, report));
        assert_eq!(registry.script_count(), 2);
        assert_eq!(registry.finding_count(), 4);
        assert_eq!(registry.malformed_count(), 0);

        let summary = registry.summary();
        assert_eq!(summary.len(), 2);
        let storage = summary
            .iter()
            .find(|entry| entry.provider == "storageutil")
            .unwrap();
        assert_eq!(storage.function, "getintvalue");
        assert_eq!(storage.occurrences, 2);
        assert_eq!(storage.scripts, 2);
        assert_eq!(
            storage.compatibility.disposition,
            CompatibilityDisposition::Mapped
        );
    }
}
