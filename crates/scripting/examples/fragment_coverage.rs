//! Empirical b2 coverage — what fraction of the real vanilla fragment
//! corpus the [`lower_fragment`] effect table actually claims.
//!
//! The compositional-scaling design
//! ([`docs/engine/m47-2-recognizer-scaling.md`]) predicted, from the
//! primitive-frequency curve, that a small effect vocabulary covers a
//! large share of the 43,818 behavioral `Fragment_*` functions. This
//! example measures the *current* table's real claim rate end-to-end:
//! decompile every `.pex`, lower every fragment, tally claimed vs
//! declined (with the decline reasons, so the next primitives to add are
//! obvious). `SAMPLE_SHAPE=<shape>` prints example statements behind one
//! decline shape.
//!
//! It is the b2 analog of `pex_corpus_shapes` and doubles as a
//! coverage-regression gate as the effect table grows.
//!
//! ```bash
//! cargo run --release -p byroredux-scripting --example fragment_coverage -- \
//!     "<Skyrim SE>/Data/Skyrim - Misc.bsa" \
//!     "<Fallout 4>/Data/Fallout4 - Misc.ba2" \
//!     "<Starfield>/Data/Starfield - Misc.ba2"
//! ```

use std::collections::{BTreeMap, HashSet};

use byroredux_papyrus::ast::{Expr, ScriptItem, Stmt};
#[path = "common/archives.rs"]
mod archives;
use archives::{Archive, ArchiveLibrary};
use byroredux_pex::{decompile::decompile_script, parse};
use byroredux_scripting::fragment::quest_property_names_with_library;
use byroredux_papyrus::ast::Param;
use byroredux_scripting::translate::effects::{
    lower_fragment_with_quest_properties, lower_info_fragment, lower_quest_fragment,
    lower_scene_fragment, Effect,
    ScriptLibrary, INFO_SPEAKER_PARAMETER,
};

/// Lower a fragment the way production does: a topic-info (`TIF_…`) fragment
/// — the one carrying the speaker parameter — through the INFO entry point
/// with its speaker bound, every other one through the quest/scene path.
/// Which production entry point a fragment script goes through, with the
/// script library every populate path now carries (M47.2).
#[derive(Clone, Copy)]
struct Production<'a> {
    library: &'a dyn ScriptLibrary,
    /// `QF_…` — the quest path.
    quest: bool,
    /// `SF_…` — the scene path.
    scene: bool,
}

fn lower_as_production(
    params: &[Param],
    body: &[byroredux_papyrus::span::Spanned<Stmt>],
    quest_properties: &HashSet<String>,
    quest_library: Production<'_>,
) -> Option<Vec<Effect>> {
    let library = Some(quest_library.library);
    if params
        .iter()
        .any(|p| p.name.node.0.eq_ignore_ascii_case(INFO_SPEAKER_PARAMETER))
    {
        lower_info_fragment(params, body, quest_properties, &HashSet::new(), None, library)
    } else if quest_library.quest {
        // A quest fragment (`QF_…`): production inlines calls into the
        // quest's own script.
        lower_quest_fragment(body, quest_properties, &HashSet::new(), None, library)
    } else if quest_library.scene {
        lower_scene_fragment(body, quest_properties, &HashSet::new(), None, library)
    } else {
        lower_fragment_with_quest_properties(body, quest_properties)
    }
}

/// A short description of a top-level statement's shape, for tallying
/// decline reasons. A method-call `ExprStmt` describes as `method/arity`
/// (e.g. `moveto/5`) — exactly the granularity #3498's suggested fix asks
/// for; every other statement kind describes by its own kind, since
/// `lower_statements` declines several of those unconditionally
/// (assignment to a field/index, an `elseif` clause, …) independent of
/// any particular method.
fn stmt_shape(stmt: &Stmt) -> String {
    match stmt {
        Stmt::ExprStmt(e) => expr_call_shape(&e.node).unwrap_or_else(|| "ExprStmt(other)".into()),
        Stmt::Assign { target, value, .. } => match &target.node {
            Expr::Ident(_) => match expr_call_shape(&value.node) {
                Some(shape) => format!("Assign(= {shape})"),
                None => "Assign(local)".into(),
            },
            _ => "Assign(field/index)".into(),
        },
        Stmt::VarDecl(_) => "VarDecl".into(),
        Stmt::Return(Some(_)) => "Return(value)".into(),
        Stmt::Return(None) => "Return(none)".into(),
        Stmt::If {
            elseif_clauses, ..
        } if !elseif_clauses.is_empty() => "If(elseif)".into(),
        Stmt::If { .. } => "If".into(),
        Stmt::While { .. } => "While".into(),
    }
}

/// `method/arity` for a direct or member-access call expression; `None`
/// for anything else (a bare literal statement, an unsupported operator
/// chain, …).
fn expr_call_shape(expr: &Expr) -> Option<String> {
    let Expr::Call { callee, args } = expr else {
        return None;
    };
    let name = match &callee.node {
        Expr::Ident(id) => id.0.to_ascii_lowercase(),
        Expr::MemberAccess { member, .. } => member.node.0.to_ascii_lowercase(),
        _ => return None,
    };
    Some(format!("{name}/{}", args.len()))
}

/// `lower_statements` (the function under `lower_fragment_with_quest_
/// properties`) declines a fragment on the FIRST statement it can't
/// classify and processes every earlier statement's binding effects
/// first — the same short-circuit-on-`?` shape as a `for` loop with an
/// early return. Re-lowering successive prefixes of `body` from the
/// start therefore reproduces exactly the point of failure: the
/// smallest prefix that still declines names the failing statement.
/// Zero production-code changes — this probes the existing decision
/// boundary from the outside rather than threading a new "why" return
/// value through the translator.
fn decline_shape(
    params: &[Param],
    body: &[byroredux_papyrus::span::Spanned<Stmt>],
    quest_properties: &HashSet<String>,
    quest_library: Production<'_>,
) -> String {
    for i in 0..body.len() {
        if lower_as_production(params, &body[..=i], quest_properties, quest_library).is_none() {
            return if_aware_shape(params, &body[..i], &body[i].node, quest_properties, quest_library);
        }
    }
    // Every prefix lowered but the whole body still declined — not
    // reachable given `lower_statements`' sequential short-circuit, but
    // keep the histogram honest instead of panicking on a future
    // control-flow shape (e.g. a decline keyed on total effect count).
    "(no single failing statement found)".into()
}

/// The failing statement's shape, looking inside a plain `If` (no `elseif`):
/// its branches are re-lowered inline after the same prefix, so a branch
/// statement that fails reports as `If>shape`; when every branch lowers, the
/// condition is what failed — `If(condition)`. An `If` is otherwise one
/// opaque bucket hiding which statement the effect table is missing.
fn if_aware_shape(
    params: &[Param],
    prefix: &[byroredux_papyrus::span::Spanned<Stmt>],
    stmt: &Stmt,
    quest_properties: &HashSet<String>,
    quest_library: Production<'_>,
) -> String {
    let Stmt::If {
        body,
        elseif_clauses,
        else_body,
        ..
    } = stmt
    else {
        return stmt_shape(stmt);
    };
    if !elseif_clauses.is_empty() {
        return stmt_shape(stmt);
    }
    for branch in std::iter::once(body).chain(else_body.iter()) {
        let mut inlined = prefix.to_vec();
        for (j, inner) in branch.iter().enumerate() {
            inlined.push(inner.clone());
            if lower_as_production(params, &inlined, quest_properties, quest_library).is_none() {
                let mut before = prefix.to_vec();
                before.extend(branch[..j].iter().cloned());
                let inner_shape =
                    if_aware_shape(params, &before, &inner.node, quest_properties, quest_library);
                return format!("If>{inner_shape}");
            }
        }
    }
    "If(condition)".into()
}
/// `SAMPLE_SHAPE=<shape>` (e.g. `additem/3`) prints the failing statement of
/// every `SAMPLE_EVERY`-th (default 150) fragment declined on that shape, so
/// the receiver/argument forms behind a decline count can be read directly.
fn sample_decline(
    file: &str,
    params: &[Param],
    body: &[byroredux_papyrus::span::Spanned<Stmt>],
    quest_properties: &HashSet<String>,
    quest_library: Production<'_>,
    shape: &str,
) {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static SEEN: AtomicUsize = AtomicUsize::new(0);
    if std::env::var("SAMPLE_SHAPE").ok().as_deref() != Some(shape) {
        return;
    }
    let every: usize = std::env::var("SAMPLE_EVERY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(150)
        .max(1);
    if !SEEN.fetch_add(1, Ordering::Relaxed).is_multiple_of(every) {
        return;
    }
    let Some(stmt) = (0..body.len())
        .find(|&i| {
            lower_as_production(params, &body[..=i], quest_properties, quest_library).is_none()
        })
        .map(|i| &body[i].node)
    else {
        return;
    };
    let text = format!("{stmt:?}")
        .replace("Spanned { node: ", "")
        .replace(", span: Span { start: 0, end: 0 } }", "");
    eprintln!("SAMPLE {file}: {}", &text[..text.len().min(900)]);
}

/// Every called member name in `body` (lowercased), descending into
/// branches, loops and nested expressions.
fn collect_call_names(
    body: &[byroredux_papyrus::span::Spanned<Stmt>],
    out: &mut std::collections::BTreeSet<String>,
) {
    fn expr(e: &Expr, out: &mut std::collections::BTreeSet<String>) {
        match e {
            Expr::Call { callee, args } => {
                if let Expr::MemberAccess { object, member } = &callee.node {
                    out.insert(member.node.0.to_ascii_lowercase());
                    expr(&object.node, out);
                } else {
                    expr(&callee.node, out);
                }
                for arg in args {
                    expr(&arg.value.node, out);
                }
            }
            Expr::MemberAccess { object, .. } => expr(&object.node, out),
            Expr::Cast { expr: inner, .. } => expr(&inner.node, out),
            Expr::BinaryOp { left, right, .. } => {
                expr(&left.node, out);
                expr(&right.node, out);
            }
            Expr::UnaryOp { operand, .. } => expr(&operand.node, out),
            Expr::Index { object, index } => {
                expr(&object.node, out);
                expr(&index.node, out);
            }
            _ => {}
        }
    }
    for stmt in body {
        match &stmt.node {
            Stmt::ExprStmt(e) | Stmt::Return(Some(e)) => expr(&e.node, out),
            Stmt::Assign { target, value, .. } => {
                expr(&target.node, out);
                expr(&value.node, out);
            }
            Stmt::VarDecl(variable) => {
                if let Some(init) = &variable.initial_value {
                    expr(&init.node, out);
                }
            }
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            } => {
                expr(&condition.node, out);
                collect_call_names(body, out);
                for clause in elseif_clauses {
                    expr(&clause.0.node, out);
                    collect_call_names(&clause.1, out);
                }
                if let Some(else_body) = else_body {
                    collect_call_names(else_body, out);
                }
            }
            Stmt::While { condition, body } => {
                expr(&condition.node, out);
                collect_call_names(body, out);
            }
            _ => {}
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: fragment_coverage <archive.bsa|archive.ba2> [more...]");
        std::process::exit(2);
    }

    let mut behavioral = 0usize; // non-empty fragments
    let mut claimed = 0usize; // fully lowered

    // #4328 — claimed fragments whose effects include a placeholder
    // (`Effect::is_placeholder`): lowered, but part of the call is a stub.
    let mut claimed_with_placeholder = 0usize;
    let mut empty = 0usize; // empty fragments (trivially lowered)
    let mut effect_hist: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut claimed_effects = 0usize;
    // #3498 — the tally the module doc always claimed to produce.
    let mut decline_hist: BTreeMap<String, usize> = BTreeMap::new();
    // `EVENT_COVERAGE=1`: how many quest scripts' timer events
    // (`OnUpdate`, `OnTimer`, …) lower on the quest path, by event.
    let event_coverage = std::env::var("EVENT_COVERAGE").is_ok();
    // `CALL_CENSUS=1`: across declined fragments, how many call each name
    // (anywhere in the body, branches included) — what a native or a
    // recognizer for that name could reach.
    let call_census = std::env::var("CALL_CENSUS").is_ok();
    let mut call_hist: BTreeMap<String, usize> = BTreeMap::new();
    // M47.2 — declined fragments the Papyrus VM's ahead-of-time check
    // accepts (production installs them as `Effect::Interpret`).
    let mut interpretable = 0usize;
    let mut vm_reasons: BTreeMap<String, usize> = BTreeMap::new();
    let mut event_hist: BTreeMap<String, (usize, usize)> = BTreeMap::new();

    let archives: Vec<Archive> = args
        .iter()
        .filter_map(|path| match Archive::open(path) {
            Ok(archive) => Some(archive),
            Err(_) => {
                eprintln!("!! could not open {path}");
                None
            }
        })
        .collect();
    let library = ArchiveLibrary::new(&archives);
    for (path, arch) in args.iter().zip(&archives) {
        let pex_files: Vec<String> = arch
            .list_files()
            .into_iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(".pex"))
            .map(|s| s.to_string())
            .collect();
        eprintln!("{path}: {} .pex", pex_files.len());

        for f in pex_files {
            let Ok(data) = arch.extract(&f) else { continue };
            let Ok(pex) = parse(&data) else { continue };
            // Catch a decompiler panic so one bad script can't abort the sweep.
            let Ok(Ok(script)) =
                std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decompile_script(&pex)))
            else {
                continue;
            };
            // #2658 (SCR-D5-NEW11-03) — computed once per script, matching
            // the production caller (`populate_quest_fragments_from_script`),
            // and fed to `lower_fragment_with_quest_properties` below so
            // this measures the SAME lowering path production runs.
            // M47.2 — production classifies quest-script-typed properties
            // through the script library on every populate path.
            let quest_properties = quest_property_names_with_library(&script, Some(&library));
            let script_arc = std::sync::Arc::new(script.clone());
            // Quest-fragment scripts: `QF_…`, `DLC1_QF_…`, FO4's
            // `Fragments:Quests:QF_…` — the ones production populates through
            // the quest path, which inlines calls into the quest's script.
            let is_quest_fragment_script = script.name.node.0.to_ascii_lowercase().contains("qf_");
            let script_name = script.name.node.0.to_ascii_lowercase();
            let quest_library = Production {
                library: &library,
                quest: is_quest_fragment_script,
                scene: script_name.contains("sf_"),
            };
            let extends_quest = script
                .parent
                .as_ref()
                .is_some_and(|parent| parent.node.0.eq_ignore_ascii_case("quest"));
            if event_coverage && extends_quest {
                for item in &script.body {
                    let ScriptItem::Event(event) = &item.node else {
                        continue;
                    };
                    let name = event.name.node.0.to_ascii_lowercase();
                    if !name.contains("update") && !name.contains("timer") {
                        continue;
                    }
                    let lowered = lower_quest_fragment(
                        &event.body,
                        &quest_properties,
                        &HashSet::new(),
                        None,
                        Some(&library),
                    )
                    .is_some();
                    let row = event_hist.entry(name).or_default();
                    row.0 += 1;
                    row.1 += usize::from(lowered);
                }
            }
            for item in &script.body {
                let ScriptItem::Function(func) = &item.node else {
                    continue;
                };
                if !func
                    .name
                    .node
                    .0
                    .to_ascii_lowercase()
                    .starts_with("fragment")
                {
                    continue;
                }
                if func.body.is_empty() {
                    empty += 1;
                    continue;
                }
                behavioral += 1;
                match lower_as_production(&func.params, &func.body, &quest_properties, quest_library) {
                    Some(effects) => {
                        claimed += 1;
                        claimed_effects += effects.len();
                        if effects.iter().any(Effect::is_placeholder) {
                            claimed_with_placeholder += 1;
                        }
                        for e in &effects {
                            *effect_hist.entry((e).kind()).or_default() += 1;
                        }
                    }
                    None => {
                        let shape =
                            decline_shape(&func.params, &func.body, &quest_properties, quest_library);
                        sample_decline(
                            &f,
                            &func.params,
                            &func.body,
                            &quest_properties,
                            quest_library,
                            &shape,
                        );
                        *decline_hist.entry(shape).or_default() += 1;
                        // Production runs the VM fallback on the quest,
                        // scene, topic-info and package paths.
                        let info = func
                            .params
                            .iter()
                            .any(|p| p.name.node.0.eq_ignore_ascii_case(INFO_SPEAKER_PARAMETER));
                        // Package fragments (`PF_…`) run on package transitions.
                        let package = script.name.node.0.to_ascii_lowercase().contains("pf_");
                        // Terminal fragments (`TERM_…`) run on a menu pick.
                        let terminal = script.name.node.0.to_ascii_lowercase().contains("term_");
                        let perk = script.name.node.0.to_ascii_lowercase().contains("prkf_");
                        let wired = quest_library.quest || quest_library.scene || info || package || terminal || perk;
                        let checked = if wired {
                            byroredux_scripting::fragment::vm::check_function_reason(
                                std::sync::Arc::clone(&script_arc),
                                &func.name.node.0,
                                Some(&library),
                            )
                        } else {
                            // The kind, by the script name's prefix, for the census.
                            let name = &script.name.node.0;
                            let prefix: String = name.rsplit(':').next().unwrap_or(name).chars().take_while(|c| *c != '_').collect();
                            Err(format!("not a wired fragment kind: {prefix}_"))
                        };
                        match checked {
                            Ok(_) => interpretable += 1,
                            Err(reason) => {
                                // `VM_REASON_SCRIPTS=<text>` names the fragments behind a reason.
                                if std::env::var("VM_REASON_SCRIPTS").is_ok_and(|text| reason.contains(&text)) {
                                    println!("  {} :: {} — {reason}", script.name.node.0, func.name.node.0);
                                }
                                *vm_reasons.entry(reason).or_default() += 1
                            }
                        }
                        if call_census {
                            let mut names = std::collections::BTreeSet::new();
                            collect_call_names(&func.body, &mut names);
                            for name in names {
                                *call_hist.entry(name).or_default() += 1;
                            }
                        }
                    }
                }
            }
        }
    }

    let pct = |n: usize, d: usize| {
        if d == 0 {
            0.0
        } else {
            100.0 * n as f64 / d as f64
        }
    };
    if call_census {
        let mut calls: Vec<_> = call_hist.iter().collect();
        calls.sort_by(|a, b| b.1.cmp(a.1));
        println!("calls in declined fragments (top 60):");
        for (name, count) in calls.into_iter().take(60) {
            println!("  {name:32} {count}");
        }
    }
    for (event, (total, lowered)) in &event_hist {
        println!("event {event}: {lowered}/{total} lower on the quest path");
    }
    println!("\n######## b2 fragment-lowerer coverage ########");
    println!("empty fragments (trivial no-op): {empty}");
    println!("behavioral fragments: {behavioral}");
    println!(
        "fully lowered (claimed): {claimed} ({:.1}% of behavioral)",
        pct(claimed, behavioral)
    );
    println!(
        "  of which rely on a placeholder effect: {claimed_with_placeholder} \
         (lowered, but a call in them is a counter-only stub)"
    );
    println!(
        "  of the declined, the Papyrus VM runs: {interpretable} — lowered + interpreted {:.1}%",
        pct(claimed + interpretable, behavioral)
    );
    if std::env::var("VM_REASONS").is_ok() {
        let mut reasons: Vec<_> = vm_reasons.iter().collect();
        reasons.sort_by(|a, b| b.1.cmp(a.1));
        for (reason, count) in reasons.into_iter().take(50) {
            println!("    vm: {reason:48} {count}");
        }
    }
    println!(
        "declined: {} ({:.1}%)",
        behavioral - claimed,
        pct(behavioral - claimed, behavioral)
    );
    println!("\ncanonical effects emitted: {claimed_effects}");
    for (k, n) in &effect_hist {
        println!("  {k:<24} {n}");
    }

    // #3498 — the decline-reason tally the module doc always promised.
    // Sorted by count descending so the next primitive to add is the
    // first line, not buried in a BTreeMap's alphabetical order.
    let mut declines: Vec<(&String, &usize)> = decline_hist.iter().collect();
    declines.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    println!(
        "\ndeclined by first unmodeled statement (top {}):",
        declines.len().min(30)
    );
    let top: usize = std::env::var("DECLINE_TOP")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(30);
    for (shape, n) in declines.into_iter().take(top) {
        println!("  {shape:<32} {n}");
    }
}

// Run with `cargo test -p byroredux-scripting --example fragment_coverage`
// (matching `mq101_conformance`'s own embedded-test convention) — not swept
// by a bare `cargo test -p byroredux-scripting`, since a `[[example]]`
// target's tests aren't part of the crate's default test set.
#[cfg(test)]
mod tests {
    use super::*;
    use byroredux_papyrus::ast::{AssignOp, CallArg, Identifier, Variable};
    use byroredux_papyrus::span::{Span, Spanned};

    fn spanned<T>(node: T) -> Spanned<T> {
        Spanned::new(node, Span::empty(0))
    }

    fn ident(name: &str) -> Identifier {
        Identifier(name.to_string())
    }

    fn call(callee: Expr, args: Vec<Expr>) -> Expr {
        Expr::Call {
            callee: Box::new(spanned(callee)),
            args: args
                .into_iter()
                .map(|value| CallArg {
                    name: None,
                    value: spanned(value),
                })
                .collect(),
        }
    }

    fn member_call(receiver: &str, method: &str, args: Vec<Expr>) -> Expr {
        call(
            Expr::MemberAccess {
                object: Box::new(spanned(Expr::Ident(ident(receiver)))),
                member: spanned(ident(method)),
            },
            args,
        )
    }

    #[test]
    fn expr_call_shape_reports_method_and_arity() {
        let e = member_call("Player", "MoveTo", vec![Expr::IntLit(0); 5]);
        assert_eq!(expr_call_shape(&e).as_deref(), Some("moveto/5"));

        let e = call(Expr::Ident(ident("Enable")), vec![]);
        assert_eq!(expr_call_shape(&e).as_deref(), Some("enable/0"));

        assert_eq!(expr_call_shape(&Expr::IntLit(0)), None);
    }

    #[test]
    fn stmt_shape_describes_every_kind() {
        assert_eq!(
            stmt_shape(&Stmt::ExprStmt(spanned(member_call(
                "obj",
                "DoThing",
                vec![Expr::IntLit(1)]
            )))),
            "dothing/1"
        );
        assert_eq!(
            stmt_shape(&Stmt::ExprStmt(spanned(Expr::IntLit(0)))),
            "ExprStmt(other)"
        );
        assert_eq!(
            stmt_shape(&Stmt::Assign {
                target: spanned(Expr::MemberAccess {
                    object: Box::new(spanned(Expr::Ident(ident("self")))),
                    member: spanned(ident("Field")),
                }),
                op: AssignOp::Eq,
                value: spanned(Expr::IntLit(1)),
            }),
            "Assign(field/index)"
        );
        assert_eq!(
            stmt_shape(&Stmt::VarDecl(Variable {
                ty: spanned(byroredux_papyrus::ast::Type::Int),
                name: spanned(ident("x")),
                initial_value: None,
                is_conditional: false,
                is_const: false,
                is_hidden: false,
            })),
            "VarDecl"
        );
        assert_eq!(stmt_shape(&Stmt::Return(None)), "Return(none)");
        assert_eq!(
            stmt_shape(&Stmt::Return(Some(spanned(Expr::IntLit(1))))),
            "Return(value)"
        );
        assert_eq!(
            stmt_shape(&Stmt::While {
                condition: spanned(Expr::BoolLit(true)),
                body: vec![],
            }),
            "While"
        );
    }

    /// #3498 — the prefix-search must land on the exact statement that
    /// breaks lowering, not merely on "the fragment declined". A leading
    /// `Return(None)` (always lowers fine — Champollion's terminator) is
    /// followed by an assignment to a field, which `lower_statements`
    /// explicitly declines (`effects.rs`: "assignment to a field/index —
    /// unmodeled"). The failing index is 1, not 0.
    #[test]
    fn decline_shape_finds_the_first_failing_statement_not_just_any_failure() {
        let body = vec![
            spanned(Stmt::Return(None)),
            spanned(Stmt::Assign {
                target: spanned(Expr::MemberAccess {
                    object: Box::new(spanned(Expr::Ident(ident("self")))),
                    member: spanned(ident("Field")),
                }),
                op: AssignOp::Eq,
                value: spanned(Expr::IntLit(1)),
            }),
        ];
        assert!(
            lower_fragment_with_quest_properties(&body, &HashSet::new()).is_none(),
            "sanity: the full body must actually decline"
        );
        assert_eq!(
            decline_shape(&[], &body, &HashSet::new(), None),
            "Assign(field/index)"
        );
    }
}
