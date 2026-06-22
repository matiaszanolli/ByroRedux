//! Decompiler fidelity gate — decompile a real vanilla `.pex` and check it
//! against the Champollion-decompiled `.psc` reference fixture (the R5
//! corpus). This is the oracle the whole Phase-2 port is verified against.
//!
//! **Opt-in / `#[ignore]`d**: it needs the Skyrim SE script archive on
//! disk, so it never runs in CI. Run locally with:
//! ```bash
//! cargo test -p byroredux-pex --test r5_fidelity -- --ignored --nocapture
//! ```
//! It skips gracefully (passing) when the game data isn't present.

use byroredux_papyrus::ast::{BinaryOp, Expr, ScriptItem, Stmt};
use byroredux_papyrus::span::Spanned;
use byroredux_pex::decompile::decompile_script;

/// Vanilla Skyrim SE script archive. Adjust if your library lives elsewhere.
const SKYRIM_MISC_BSA: &str = "/mnt/data/SteamLibrary/steamapps/common/\
Skyrim Special Edition/Data/Skyrim - Misc.bsa";

/// Extract a `.pex` from the archive by file-stem (case-insensitive),
/// ignoring its folder. `None` if the archive or the script is missing.
fn extract_pex(stem: &str) -> Option<Vec<u8>> {
    let arch = byroredux_bsa::BsaArchive::open(SKYRIM_MISC_BSA).ok()?;
    let want = format!("{}.pex", stem.to_ascii_lowercase());
    let path = arch
        .list_files()
        .into_iter()
        .find(|f| f.to_ascii_lowercase().ends_with(&want))?
        .to_string();
    arch.extract(&path).ok()
}

/// Recurse a statement body looking for a `*.<method>(...)` call.
fn body_has_call(body: &[Spanned<Stmt>], method: &str) -> bool {
    body.iter().any(|s| stmt_has_call(&s.node, method))
}

fn stmt_has_call(stmt: &Stmt, method: &str) -> bool {
    match stmt {
        Stmt::ExprStmt(e) | Stmt::Assign { value: e, .. } => expr_has_call(&e.node, method),
        Stmt::Return(Some(e)) => expr_has_call(&e.node, method),
        Stmt::Return(None) => false,
        Stmt::If { condition, body, elseif_clauses, else_body } => {
            expr_has_call(&condition.node, method)
                || body_has_call(body, method)
                || elseif_clauses
                    .iter()
                    .any(|(c, b)| expr_has_call(&c.node, method) || body_has_call(b, method))
                || else_body.as_ref().is_some_and(|b| body_has_call(b, method))
        }
        Stmt::While { condition, body } => {
            expr_has_call(&condition.node, method) || body_has_call(body, method)
        }
        Stmt::VarDecl(_) => false,
    }
}

fn expr_has_call(expr: &Expr, method: &str) -> bool {
    match expr {
        Expr::Call { callee, args } => {
            let here = matches!(
                &callee.node,
                Expr::MemberAccess { member, .. } if member.node.0.eq_ignore_ascii_case(method)
            );
            here || expr_has_call(&callee.node, method)
                || args.iter().any(|a| expr_has_call(&a.value.node, method))
        }
        Expr::MemberAccess { object, .. } => expr_has_call(&object.node, method),
        Expr::Index { object, index } => {
            expr_has_call(&object.node, method) || expr_has_call(&index.node, method)
        }
        Expr::BinaryOp { left, right, .. } => {
            expr_has_call(&left.node, method) || expr_has_call(&right.node, method)
        }
        Expr::UnaryOp { operand, .. } => expr_has_call(&operand.node, method),
        Expr::Cast { expr, .. } => expr_has_call(&expr.node, method),
        Expr::New { size, .. } => expr_has_call(&size.node, method),
        _ => false,
    }
}

/// Whether any expression in a statement body uses the given binary op.
fn body_has_binary_op(body: &[Spanned<Stmt>], target: BinaryOp) -> bool {
    body.iter().any(|s| stmt_has_binary_op(&s.node, target))
}

fn stmt_has_binary_op(stmt: &Stmt, target: BinaryOp) -> bool {
    match stmt {
        Stmt::ExprStmt(e) | Stmt::Assign { value: e, .. } => expr_has_binary_op(&e.node, target),
        Stmt::Return(Some(e)) => expr_has_binary_op(&e.node, target),
        Stmt::Return(None) => false,
        Stmt::If { condition, body, elseif_clauses, else_body } => {
            expr_has_binary_op(&condition.node, target)
                || body_has_binary_op(body, target)
                || elseif_clauses.iter().any(|(c, b)| {
                    expr_has_binary_op(&c.node, target) || body_has_binary_op(b, target)
                })
                || else_body.as_ref().is_some_and(|b| body_has_binary_op(b, target))
        }
        Stmt::While { condition, body } => {
            expr_has_binary_op(&condition.node, target) || body_has_binary_op(body, target)
        }
        Stmt::VarDecl(_) => false,
    }
}

fn expr_has_binary_op(expr: &Expr, target: BinaryOp) -> bool {
    match expr {
        Expr::BinaryOp { left, op, right } => {
            *op == target
                || expr_has_binary_op(&left.node, target)
                || expr_has_binary_op(&right.node, target)
        }
        Expr::UnaryOp { operand, .. } => expr_has_binary_op(&operand.node, target),
        Expr::Cast { expr, .. } => expr_has_binary_op(&expr.node, target),
        Expr::Call { callee, args } => {
            expr_has_binary_op(&callee.node, target)
                || args.iter().any(|a| expr_has_binary_op(&a.value.node, target))
        }
        Expr::MemberAccess { object, .. } => expr_has_binary_op(&object.node, target),
        Expr::Index { object, index } => {
            expr_has_binary_op(&object.node, target) || expr_has_binary_op(&index.node, target)
        }
        _ => false,
    }
}

/// Find the `OnActivate` event body in a decompiled script (top level).
fn on_activate_body(script: &byroredux_papyrus::ast::Script) -> Option<&[Spanned<Stmt>]> {
    script.body.iter().find_map(|item| match &item.node {
        ScriptItem::Event(e) if e.name.node.0.eq_ignore_ascii_case("OnActivate") => {
            Some(e.body.as_slice())
        }
        _ => None,
    })
}

#[test]
#[ignore = "needs Skyrim SE game data on disk"]
fn da10_main_door_decompiles_to_the_r5_reference_shape() {
    let Some(bytes) = extract_pex("DA10MainDoorScript") else {
        eprintln!("SKIP: DA10MainDoorScript.pex not found (no game data?)");
        return;
    };

    let pex = byroredux_pex::parse(&bytes).expect("DA10 .pex parses");
    let script = decompile_script(&pex).expect("DA10 decompiles");

    // Header matches the .psc reference (ScriptName … Extends ReferenceAlias).
    assert!(script.name.node.0.eq_ignore_ascii_case("DA10MainDoorScript"));
    assert_eq!(
        script.parent.as_ref().map(|p| p.node.0.to_ascii_lowercase()),
        Some("referencealias".to_string()),
    );

    // The OnActivate handler is an Event whose body carries the stage-gate
    // logic: GetStageDone reads guarding a SetStage(40) write.
    let body = on_activate_body(&script).expect("OnActivate event present");
    assert!(body_has_call(body, "GetStageDone"), "guards on GetStageDone");
    assert!(body_has_call(body, "SetStage"), "writes via SetStage");

    // The two GetStageDone checks are joined by `&&` in the source — the
    // boolean pass must collapse the short-circuit into one `&&` condition
    // (rather than nested ifs). This is the real-bytecode `&&` test.
    assert!(
        body_has_binary_op(body, BinaryOp::And),
        "the two stage checks collapse into a single && condition",
    );

    // Cross-check the reference parses to the same header (sanity on the
    // fixture itself).
    let psc = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../docs/r5/source/DA10MainDoorScript.psc"
    ))
    .expect("R5 fixture readable");
    let (reference, _errs) = byroredux_papyrus::parse_script(&psc).expect("fixture parses");
    assert!(reference.name.node.0.eq_ignore_ascii_case("DA10MainDoorScript"));
    assert!(on_activate_body(&reference).is_some(), "reference has OnActivate");
}
