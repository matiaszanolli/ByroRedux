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
//! obvious).
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

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_papyrus::ast::{Expr, ScriptItem, Stmt};
use byroredux_pex::{decompile::decompile_script, parse};
use byroredux_scripting::fragment::quest_property_names;
use byroredux_scripting::translate::effects::{lower_fragment_with_quest_properties, Effect};

enum Archive {
    Bsa(BsaArchive),
    Ba2(Ba2Archive),
}

impl Archive {
    fn open(path: &str) -> std::io::Result<Self> {
        if path.to_ascii_lowercase().ends_with(".ba2") {
            Ok(Archive::Ba2(Ba2Archive::open(path)?))
        } else {
            Ok(Archive::Bsa(BsaArchive::open(path)?))
        }
    }
    fn list_files(&self) -> Vec<&str> {
        match self {
            Archive::Bsa(a) => a.list_files(),
            Archive::Ba2(a) => a.list_files(),
        }
    }
    fn extract(&self, path: &str) -> std::io::Result<Vec<u8>> {
        match self {
            Archive::Bsa(a) => a.extract(path),
            Archive::Ba2(a) => a.extract(path),
        }
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
fn decline_shape(body: &[byroredux_papyrus::span::Spanned<Stmt>], quest_properties: &HashSet<String>) -> String {
    for i in 0..body.len() {
        if lower_fragment_with_quest_properties(&body[..=i], quest_properties).is_none() {
            return stmt_shape(&body[i].node);
        }
    }
    // Every prefix lowered but the whole body still declined — not
    // reachable given `lower_statements`' sequential short-circuit, but
    // keep the histogram honest instead of panicking on a future
    // control-flow shape (e.g. a decline keyed on total effect count).
    "(no single failing statement found)".into()
}

fn effect_kind(e: &Effect) -> &'static str {
    match e {
        Effect::Conditional { .. } => "Conditional",
        Effect::SetGlobalValue { .. } => "SetGlobalValue",
        Effect::Disable { .. } => "Disable",
        Effect::Enable { .. } => "Enable",
        Effect::SetStage { .. } => "SetStage",
        Effect::StartQuest { .. } => "StartQuest",
        Effect::StopQuest { .. } => "StopQuest",
        Effect::CompleteQuest { .. } => "CompleteQuest",
        Effect::ResetQuest { .. } => "ResetQuest",
        Effect::SetQuestActive { .. } => "SetQuestActive",
        Effect::SetObjectiveDisplayed { .. } => "SetObjectiveDisplayed",
        Effect::SetObjectiveCompleted { .. } => "SetObjectiveCompleted",
        Effect::SetObjectiveFailed { .. } => "SetObjectiveFailed",
        Effect::CompleteAllObjectives { .. } => "CompleteAllObjectives",
        Effect::FailAllObjectives { .. } => "FailAllObjectives",
        Effect::AddItem { .. } => "AddItem",
        Effect::EquipItem { .. } => "EquipItem",
        Effect::MoveTo { .. } => "MoveTo",
        Effect::StartScene { .. } => "StartScene",
        Effect::StopScene { .. } => "StopScene",
        Effect::Activate { .. } => "Activate",
        Effect::SetOpen { .. } => "SetOpen",
        Effect::SetPlayerRestrained { .. } => "SetPlayerRestrained",
        Effect::SetPlayerControls { .. } => "SetPlayerControls",
        Effect::SetPlayerAiDriven { .. } => "SetPlayerAiDriven",
        Effect::SetHudCartMode { .. } => "SetHudCartMode",
        Effect::PlayIdle { .. } => "PlayIdle",
        Effect::SetVehicle { .. } => "SetVehicle",
        Effect::TetherToHorse { .. } => "TetherToHorse",
        Effect::SetMotionType { .. } => "SetMotionType",
        Effect::SetSittingRotation { .. } => "SetSittingRotation",
        Effect::ExitCart { .. } => "ExitCart",
        Effect::RegisterPlayerAnimationEvent { .. } => "RegisterPlayerAnimationEvent",
        Effect::EvaluatePackage { .. } => "EvaluatePackage",
        Effect::Wait { .. } => "Wait",
        Effect::WaitForActors3DLoaded { .. } => "WaitForActors3DLoaded",
        Effect::ProviderCall(_) => "ProviderCall",
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
    let mut empty = 0usize; // empty fragments (trivially lowered)
    let mut effect_hist: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut claimed_effects = 0usize;
    // #3498 — the tally the module doc always claimed to produce.
    let mut decline_hist: BTreeMap<String, usize> = BTreeMap::new();

    for path in &args {
        let Ok(arch) = Archive::open(path) else {
            eprintln!("!! could not open {path}");
            continue;
        };
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
            let quest_properties = quest_property_names(&script);
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
                match lower_fragment_with_quest_properties(&func.body, &quest_properties) {
                    Some(effects) => {
                        claimed += 1;
                        claimed_effects += effects.len();
                        for e in &effects {
                            *effect_hist.entry(effect_kind(e)).or_default() += 1;
                        }
                    }
                    None => {
                        *decline_hist
                            .entry(decline_shape(&func.body, &quest_properties))
                            .or_default() += 1;
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
    println!("\n######## b2 fragment-lowerer coverage ########");
    println!("empty fragments (trivial no-op): {empty}");
    println!("behavioral fragments: {behavioral}");
    println!(
        "fully lowered (claimed): {claimed} ({:.1}% of behavioral)",
        pct(claimed, behavioral)
    );
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
    for (shape, n) in declines.into_iter().take(30) {
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
            decline_shape(&body, &HashSet::new()),
            "Assign(field/index)"
        );
    }
}
