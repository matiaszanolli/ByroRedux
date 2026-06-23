//! `.pex` corpus **structural-shape** survey — the decision-driving data
//! for the M47.2 recognizer-catalog scaling question.
//!
//! The recognizer catalog (`crates/scripting/src/translate/recognizers/`)
//! is deliberately a *catalog* — each behavior shape is hand-written Rust.
//! That has a linear cost ceiling against tens of thousands of shipping
//! scripts. Before committing to a tiered architecture (hand-written for
//! the head, template-driven for the body, decline for the tail), we need
//! the actual frequency distribution of structural shapes: how many
//! distinct templates exist, and what fraction of the corpus the top-N
//! cover.
//!
//! This pass decompiles every `.pex` in the given archives to the shared
//! `byroredux_papyrus` AST (the same target `.psc` parses to and the same
//! the recognizers consume), then **abstracts each script to a structural
//! fingerprint** — control-flow skeleton + called API names, with all
//! literals and ref identities erased — and tallies the distribution.
//!
//! Three views, each answering a different planning question:
//!   1. **Event-handler frequency** — which events vanilla content actually
//!      handles (informs the 136-event dispatch + OnEquip/OnHit priority).
//!   2. **Handler-body templates** — distinct `(event, arity, body-skeleton)`
//!      shapes + top-N cumulative coverage (the per-handler match unit).
//!   3. **Whole-script templates** — the multiset of a script's handler
//!      skeletons (the unit a recognizer actually claims) + top-N coverage.
//!
//! The fingerprint is intentionally as conservative as the recognizers:
//! two scripts share a template **only** if their control flow, called API
//! names, operators, and argument *structure* match exactly — only literal
//! values and ref identities (the "holes" a template recognizer would
//! bind) are abstracted away. So the template count is a faithful proxy
//! for "how many exact-match recognizers would it take".
//!
//! Usage:
//! ```bash
//! cargo run --release -p byroredux-pex --example pex_corpus_shapes -- \
//!     "/path/Skyrim Special Edition/Data/Skyrim - Misc.bsa" \
//!     "/path/Fallout 4/Data/Fallout4 - Misc.ba2"
//! ```

use std::collections::HashMap;
use std::fmt::Write as _;

use byroredux_bsa::{Ba2Archive, BsaArchive};
use byroredux_papyrus::ast::{
    BinaryOp, Event, Expr, Script, ScriptItem, State, StateItem, Stmt, UnaryOp,
};
use byroredux_papyrus::span::Spanned;
use byroredux_pex::{decompile::decompile_script, parse};

/// Minimal archive abstraction over the two container formats.
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

// ── Structural fingerprinting ────────────────────────────────────────
//
// The abstraction policy — the crux of the whole survey. Keep everything
// that determines *behavior shape*; erase everything a template would bind
// as a hole.
//
//   KEEP:  control flow (if/elseif/else/while), statement kinds, called
//          method / function names, operators, argument arity + structure,
//          and the receiver *namespace* for the handful of native global
//          script-objects (Game/Utility/Debug/Math/…) whose API identity
//          is semantically load-bearing.
//   ERASE: literal values (→ `#`), ref identities — locals, params,
//          properties (→ `$`), and Champollion's decompiler temporaries
//          (also bare idents → `$`). Casts are unwrapped (the recognizers
//          already see through them).

/// Native global script-objects callable as `Namespace.Func(..)`. Their
/// identity is kept (`Game.GetPlayer` ≠ `MyQuest.GetStage`); every other
/// receiver collapses to `$`. Small on purpose — an unlisted namespace
/// still keeps its *function* name, only the receiver collapses.
const GLOBALS: &[&str] = &[
    "game", "utility", "debug", "math", "ui", "input", "wornobject",
];

/// Normalize an expression to its structural token.
fn norm_expr(e: &Expr, out: &mut String) {
    match e {
        // Every literal is a bindable hole.
        Expr::IntLit(_)
        | Expr::FloatLit(_)
        | Expr::BoolLit(_)
        | Expr::StringLit(_)
        | Expr::NoneLit => out.push('#'),

        // Every ref (local / param / property / temp) is a bindable hole.
        Expr::Ident(_) => out.push('$'),

        Expr::ParentAccess => out.push_str("parent"),

        Expr::MemberAccess { object, member } => {
            norm_receiver(&object.node, out);
            out.push('.');
            out.push_str(&member.node.0.to_ascii_lowercase());
        }
        Expr::Index { object, index } => {
            norm_expr(&object.node, out);
            out.push('[');
            norm_expr(&index.node, out);
            out.push(']');
        }
        Expr::Call { callee, args } => {
            // The function/method name is the load-bearing signal.
            match &callee.node {
                Expr::MemberAccess { object, member } => {
                    norm_receiver(&object.node, out);
                    out.push('.');
                    out.push_str(&member.node.0.to_ascii_lowercase());
                }
                Expr::Ident(id) => out.push_str(&id.0.to_ascii_lowercase()),
                other => norm_expr(other, out),
            }
            out.push('(');
            for (i, a) in args.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                norm_expr(&a.value.node, out);
            }
            out.push(')');
        }
        Expr::UnaryOp { op, operand } => {
            out.push_str(match op {
                UnaryOp::Neg => "-",
                UnaryOp::Not => "!",
            });
            norm_expr(&operand.node, out);
        }
        Expr::BinaryOp { left, op, right } => {
            out.push('(');
            norm_expr(&left.node, out);
            out.push_str(bin_op(*op));
            norm_expr(&right.node, out);
            out.push(')');
        }
        // Casts are transparent to the recognizers — unwrap.
        Expr::Cast { expr, .. } => norm_expr(&expr.node, out),
        Expr::New { .. } => out.push_str("new"),
        Expr::ArrayLit(_) => out.push_str("[..]"),
    }
}

/// A call/member receiver: keep the global namespace identity, collapse
/// everything else (`Self`, properties, refs, chained calls) to `$`.
fn norm_receiver(e: &Expr, out: &mut String) {
    match e {
        Expr::Ident(id) => {
            let lc = id.0.to_ascii_lowercase();
            if lc == "self" {
                out.push_str("self");
            } else if GLOBALS.contains(&lc.as_str()) {
                out.push_str(&lc);
            } else {
                out.push('$');
            }
        }
        Expr::ParentAccess => out.push_str("parent"),
        Expr::Cast { expr, .. } => norm_receiver(&expr.node, out),
        // A call or member as receiver (method chain) — abstract to `$`;
        // its own call shows up as a separate statement skeleton anyway.
        _ => out.push('$'),
    }
}

fn bin_op(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Or => "||",
        BinaryOp::And => "&&",
        BinaryOp::Eq => "==",
        BinaryOp::Ne => "!=",
        BinaryOp::Lt => "<",
        BinaryOp::Le => "<=",
        BinaryOp::Gt => ">",
        BinaryOp::Ge => ">=",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::StrCat => "..",
    }
}

/// Normalize a statement list to a `;`-joined skeleton.
fn norm_body(body: &[Spanned<Stmt>], out: &mut String) {
    out.push('{');
    for (i, s) in body.iter().enumerate() {
        if i > 0 {
            out.push(';');
        }
        norm_stmt(&s.node, out);
    }
    out.push('}');
}

fn norm_stmt(s: &Stmt, out: &mut String) {
    match s {
        Stmt::ExprStmt(e) => norm_expr(&e.node, out),
        Stmt::Assign { target, value, .. } => {
            norm_expr(&target.node, out);
            out.push('=');
            norm_expr(&value.node, out);
        }
        Stmt::Return(None) => out.push_str("ret"),
        Stmt::Return(Some(e)) => {
            out.push_str("ret ");
            norm_expr(&e.node, out);
        }
        Stmt::If {
            condition,
            body,
            elseif_clauses,
            else_body,
        } => {
            out.push_str("if ");
            norm_expr(&condition.node, out);
            norm_body(body, out);
            for (c, b) in elseif_clauses {
                out.push_str("elif ");
                norm_expr(&c.node, out);
                norm_body(b, out);
            }
            if let Some(b) = else_body {
                out.push_str("else");
                norm_body(b, out);
            }
        }
        Stmt::While { condition, body } => {
            out.push_str("while ");
            norm_expr(&condition.node, out);
            norm_body(body, out);
        }
        Stmt::VarDecl(_) => out.push_str("var"),
    }
}

/// A handler's `(event, arity)` signature + its body skeleton.
fn handler_fingerprint(e: &Event) -> String {
    let mut s = format!("{}/{}=>", e.name.node.0.to_ascii_lowercase(), e.params.len());
    norm_body(&e.body, &mut s);
    s
}

// ── Compositional primitives ─────────────────────────────────────────
//
// The whole-body templates have a heavy tail, but they are compositions
// of a much smaller vocabulary. A *primitive* is the atomic unit a
// compositional recognizer would have to understand:
//   - `S:<stmt>` — a leaf effect statement (a call / assign / return),
//     with its body abstracted as usual.
//   - `G:<pred>` — an atomic guard predicate: an `If`/`While` condition
//     split across `&&` / `||` into its atomic comparison terms (exactly
//     how `quest_stage_gate` walks the And-tree, declining per leaf).
//
// A handler is "fully covered at vocabulary size K" iff *every* primitive
// it contains is among the K most common primitives — i.e. a compositional
// recognizer that knows those K primitives can bind the whole handler and
// would decline on anything outside them. This is the decisive metric.

/// Decompose a guard expression into atomic predicates (split &&/||).
fn guard_atoms(e: &Expr, out: &mut Vec<String>) {
    match e {
        Expr::BinaryOp {
            op: BinaryOp::And | BinaryOp::Or,
            left,
            right,
        } => {
            guard_atoms(&left.node, out);
            guard_atoms(&right.node, out);
        }
        Expr::UnaryOp { op: UnaryOp::Not, operand } => guard_atoms(&operand.node, out),
        other => {
            let mut s = String::from("G:");
            norm_expr(other, &mut s);
            out.push(s);
        }
    }
}

/// Collect every primitive in a statement list (recurses into control flow).
fn collect_primitives(body: &[Spanned<Stmt>], out: &mut Vec<String>) {
    for s in body {
        match &s.node {
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            } => {
                guard_atoms(&condition.node, out);
                collect_primitives(body, out);
                for (c, b) in elseif_clauses {
                    guard_atoms(&c.node, out);
                    collect_primitives(b, out);
                }
                if let Some(b) = else_body {
                    collect_primitives(b, out);
                }
            }
            Stmt::While { condition, body } => {
                guard_atoms(&condition.node, out);
                collect_primitives(body, out);
            }
            leaf => {
                let mut s = String::from("S:");
                norm_stmt(leaf, &mut s);
                out.push(s);
            }
        }
    }
}

/// Walk a script's event handlers (top-level + inside states).
fn for_each_handler<'a>(script: &'a Script, mut f: impl FnMut(&'a Event)) {
    for item in &script.body {
        match &item.node {
            ScriptItem::Event(e) => f(e),
            ScriptItem::State(State { body, .. }) => {
                for si in body {
                    if let StateItem::Event(e) = &si.node {
                        f(e);
                    }
                }
            }
            _ => {}
        }
    }
}

#[derive(Default)]
struct Survey {
    archives: usize,
    pex_total: usize,
    parsed: usize,
    decompiled: usize,
    decompile_failed: usize,
    panicked: usize,

    /// Scripts with at least one event handler (the recognizable population).
    with_handlers: usize,
    /// Scripts with zero event handlers (pure data/function libs — inert).
    no_handlers: usize,
    /// Of the no-handler scripts: those carrying ≥1 `Fragment_*` function
    /// (quest/scene/dialogue/perk fragments — behavior invoked directly by
    /// the quest system, a *separate* dispatch model from event handlers).
    no_handler_with_fragment: usize,
    /// Of the no-handler scripts: those with ≥1 non-fragment function
    /// (callable libraries) — the rest are pure property/data holders.
    no_handler_with_fn: usize,

    /// event name → number of *scripts* that define a handler of that name.
    event_freq: HashMap<String, usize>,
    /// handler fingerprint → count (across all handlers in the corpus).
    handler_templates: HashMap<String, usize>,
    /// whole-script fingerprint → count.
    script_templates: HashMap<String, (usize, String)>, // (count, example name)

    /// Behavioral handlers = those with a non-empty body. Empties are
    /// stub overrides (no-ops) — excluded from the compositional metric.
    behavioral_handlers: usize,
    /// primitive → number of behavioral handlers that contain it.
    primitive_freq: HashMap<String, usize>,
    /// Per behavioral handler: its set of distinct primitives. Kept so we
    /// can compute coverage-at-K after ranking primitives by frequency.
    handler_prim_sets: Vec<Vec<String>>,

    // ── Fragment population (the 69.5% the recognizer chain ignores) ──
    /// Total `Fragment_*` functions across the corpus (one script may hold
    /// several — one per quest stage / scene action).
    fragment_fns: usize,
    /// Non-empty fragment functions (carry behavior).
    fragment_behavioral: usize,
    /// Fragment body skeleton → count (the fragment-template distribution).
    fragment_templates: HashMap<String, usize>,
    /// primitive → number of behavioral fragments that contain it.
    fragment_freq: HashMap<String, usize>,
    fragment_prim_sets: Vec<Vec<String>>,
}

fn survey_script(script: &Script, file: &str, sv: &mut Survey) {
    // Fragment functions (quest/scene/dialogue/perk) — collected from
    // every script, independent of whether it also has event handlers.
    for item in &script.body {
        if let ScriptItem::Function(f) = &item.node {
            if !f.name.node.0.to_ascii_lowercase().starts_with("fragment") {
                continue;
            }
            sv.fragment_fns += 1;
            if f.body.is_empty() {
                continue;
            }
            sv.fragment_behavioral += 1;
            let mut tpl = String::new();
            norm_body(&f.body, &mut tpl);
            *sv.fragment_templates.entry(tpl).or_default() += 1;
            let mut prims = Vec::new();
            collect_primitives(&f.body, &mut prims);
            prims.sort();
            prims.dedup();
            for p in &prims {
                *sv.fragment_freq.entry(p.clone()).or_default() += 1;
            }
            sv.fragment_prim_sets.push(prims);
        }
    }

    let mut handler_keys: Vec<String> = Vec::new();
    let mut seen_events = std::collections::HashSet::new();
    for_each_handler(script, |e| {
        let name = e.name.node.0.to_ascii_lowercase();
        if seen_events.insert(name.clone()) {
            *sv.event_freq.entry(name).or_default() += 1;
        }
        let fp = handler_fingerprint(e);
        *sv.handler_templates.entry(fp.clone()).or_default() += 1;
        handler_keys.push(fp);

        // Compositional view: behavioral (non-empty) handlers only.
        if !e.body.is_empty() {
            sv.behavioral_handlers += 1;
            let mut prims = Vec::new();
            collect_primitives(&e.body, &mut prims);
            prims.sort();
            prims.dedup();
            for p in &prims {
                *sv.primitive_freq.entry(p.clone()).or_default() += 1;
            }
            sv.handler_prim_sets.push(prims);
        }
    });

    if handler_keys.is_empty() {
        sv.no_handlers += 1;
        // Classify the inert majority: fragment-bearing vs library vs data.
        let mut has_fragment = false;
        let mut has_fn = false;
        for item in &script.body {
            if let ScriptItem::Function(f) = &item.node {
                if f.name.node.0.to_ascii_lowercase().starts_with("fragment") {
                    has_fragment = true;
                } else {
                    has_fn = true;
                }
            }
        }
        if has_fragment {
            sv.no_handler_with_fragment += 1;
        } else if has_fn {
            sv.no_handler_with_fn += 1;
        }
        return;
    }
    sv.with_handlers += 1;

    // Whole-script template = the sorted multiset of handler skeletons.
    handler_keys.sort();
    let mut script_fp = String::new();
    for (i, k) in handler_keys.iter().enumerate() {
        if i > 0 {
            script_fp.push('\n');
        }
        script_fp.push_str(k);
    }
    let entry = sv
        .script_templates
        .entry(script_fp)
        .or_insert((0, script.name.node.0.clone()));
    entry.0 += 1;
    let _ = file; // (file available for richer examples if needed)
}

/// Print a frequency table + cumulative-coverage curve for a template map.
fn report_distribution(title: &str, map: &HashMap<String, usize>, total: usize, show: usize) {
    let mut v: Vec<(&String, usize)> = map.iter().map(|(k, &c)| (k, c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));

    println!("\n==== {title} ====");
    println!("distinct templates: {}   population: {}", v.len(), total);

    // Cumulative coverage at milestones.
    let milestones = [10usize, 25, 50, 100, 200, 500, 1000];
    let mut cum = 0usize;
    let mut next = 0usize;
    println!("cumulative coverage:");
    for (i, (_, c)) in v.iter().enumerate() {
        cum += c;
        if next < milestones.len() && i + 1 == milestones[next] {
            println!(
                "  top {:>4} templates → {:>6} / {} ({:5.1}%)",
                milestones[next],
                cum,
                total,
                100.0 * cum as f64 / total as f64
            );
            next += 1;
        }
    }
    // Also report how many templates to reach 50/80/90/95/99%.
    let thresholds = [0.50, 0.80, 0.90, 0.95, 0.99];
    let mut ti = 0;
    let mut run = 0usize;
    println!("templates needed to cover:");
    for (i, (_, c)) in v.iter().enumerate() {
        run += c;
        while ti < thresholds.len() && run as f64 >= thresholds[ti] * total as f64 {
            println!("  {:>3.0}% → {} templates", thresholds[ti] * 100.0, i + 1);
            ti += 1;
        }
    }

    println!("top {show} templates:");
    for (k, c) in v.iter().take(show) {
        let pct = 100.0 * *c as f64 / total as f64;
        let disp: String = k.chars().take(160).collect();
        let disp = disp.replace('\n', " ⏎ ");
        println!("  {c:>6} ({pct:4.1}%)  {disp}");
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        eprintln!("usage: pex_corpus_shapes <archive.bsa|archive.ba2> [more...]");
        std::process::exit(2);
    }

    let mut sv = Survey::default();

    // Loose `.psc` source files are accepted too (parsed via the same AST)
    // — handy for validating the fingerprint on the R5 reference scripts
    // without an archive. Mixed `.psc` + archive args are fine.
    let psc_args: Vec<&String> = args
        .iter()
        .filter(|a| a.to_ascii_lowercase().ends_with(".psc"))
        .collect();
    for path in &psc_args {
        let Ok(text) = std::fs::read_to_string(path) else {
            eprintln!("!! could not read {path}");
            continue;
        };
        sv.pex_total += 1;
        match byroredux_papyrus::parse_script(&text) {
            Ok((script, _errs)) => {
                sv.parsed += 1;
                sv.decompiled += 1;
                survey_script(&script, path, &mut sv);
            }
            Err(_) => sv.decompile_failed += 1,
        }
    }

    for path in args.iter().filter(|a| !a.to_ascii_lowercase().ends_with(".psc")) {
        let arch = match Archive::open(path) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("!! could not open {path}: {e}");
                continue;
            }
        };
        sv.archives += 1;
        let pex_files: Vec<String> = arch
            .list_files()
            .into_iter()
            .filter(|f| f.to_ascii_lowercase().ends_with(".pex"))
            .map(|s| s.to_string())
            .collect();
        eprintln!("{path}: {} .pex files", pex_files.len());

        for f in pex_files {
            sv.pex_total += 1;
            let Ok(data) = arch.extract(&f) else { continue };
            let Ok(pex) = parse(&data) else { continue };
            sv.parsed += 1;
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decompile_script(&pex)))
            {
                Ok(Ok(script)) => {
                    sv.decompiled += 1;
                    survey_script(&script, &f, &mut sv);
                }
                Ok(Err(_)) => sv.decompile_failed += 1,
                Err(_) => sv.panicked += 1,
            }
        }
    }

    println!("\n######## .pex corpus structural-shape survey ########");
    println!(
        "archives {}  .pex {}  parsed {}  decompiled {}  (decompile-fail {}  panic {})",
        sv.archives, sv.pex_total, sv.parsed, sv.decompiled, sv.decompile_failed, sv.panicked
    );
    println!(
        "scripts with ≥1 event handler: {}   no handlers: {}",
        sv.with_handlers, sv.no_handlers
    );
    println!(
        "  of no-handler: {} carry Fragment_* fns (quest/scene fragments), {} are libraries (other fns), {} pure data/property",
        sv.no_handler_with_fragment,
        sv.no_handler_with_fn,
        sv.no_handlers - sv.no_handler_with_fragment - sv.no_handler_with_fn
    );

    // ── View 1: which events the corpus actually handles ──
    let mut ev: Vec<(&String, usize)> = sv.event_freq.iter().map(|(k, &c)| (k, c)).collect();
    ev.sort_by(|a, b| b.1.cmp(&a.1));
    println!("\n==== event-handler frequency (scripts defining each) ====");
    let mut line = String::new();
    for (name, c) in ev.iter().take(40) {
        let _ = write!(line, "  {name} {c}");
        if line.len() > 90 {
            println!("{line}");
            line.clear();
        }
    }
    if !line.is_empty() {
        println!("{line}");
    }

    // ── View 2: handler-body templates ──
    let handler_pop: usize = sv.handler_templates.values().sum();
    report_distribution(
        "handler-body templates (event + arity + body skeleton)",
        &sv.handler_templates,
        handler_pop,
        30,
    );

    // ── View 3: whole-script templates (the recognizer's claim unit) ──
    let script_map: HashMap<String, usize> = sv
        .script_templates
        .iter()
        .map(|(k, (c, _))| (k.clone(), *c))
        .collect();
    report_distribution(
        "whole-script templates (multiset of handler skeletons)",
        &script_map,
        sv.with_handlers,
        30,
    );

    // ── View 4: compositional primitive vocabulary + coverage-at-K ──
    compositional_report(
        "EVENT-HANDLER primitives (guard atoms + effect statements)",
        &sv.primitive_freq,
        &sv.handler_prim_sets,
        sv.behavioral_handlers,
        "behavioral handlers",
    );

    // ── View 5: the fragment population (69.5% of the corpus) ──
    println!("\n==== fragment population (quest/scene/dialogue/perk) ====");
    println!(
        "Fragment_* functions: {}   behavioral (non-empty): {}",
        sv.fragment_fns, sv.fragment_behavioral
    );
    report_distribution(
        "FRAGMENT body templates",
        &sv.fragment_templates,
        sv.fragment_behavioral,
        20,
    );
    compositional_report(
        "FRAGMENT primitives (guard atoms + effect statements)",
        &sv.fragment_freq,
        &sv.fragment_prim_sets,
        sv.fragment_behavioral,
        "behavioral fragments",
    );
}

/// Coverage-at-K: how large a primitive vocabulary a compositional,
/// decline-on-any-unknown recognizer needs to FULLY bind X% of a
/// behavioral population.
fn compositional_report(
    title: &str,
    freq: &HashMap<String, usize>,
    prim_sets: &[Vec<String>],
    population: usize,
    unit: &str,
) {
    println!("\n==== {title} ====");
    println!("distinct primitives: {}   {unit}: {}", freq.len(), population);

    let mut prims: Vec<(&String, usize)> = freq.iter().map(|(k, &c)| (k, c)).collect();
    prims.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    let rank: HashMap<&str, usize> = prims
        .iter()
        .enumerate()
        .map(|(i, (k, _))| (k.as_str(), i))
        .collect();

    let max_rank: Vec<usize> = prim_sets
        .iter()
        .map(|set| set.iter().map(|p| rank[p.as_str()]).max().unwrap_or(0))
        .collect();
    let total = population.max(1);
    println!("vocabulary size K → {unit} FULLY covered:");
    for k in [10usize, 25, 50, 100, 150, 200, 300, 500, 750, 1000, 1500, 2000] {
        let covered = max_rank.iter().filter(|&&r| r < k).count();
        println!(
            "  K={:>4} → {:>5} / {} ({:5.1}%)",
            k,
            covered,
            total,
            100.0 * covered as f64 / total as f64
        );
    }
    let mut sorted_max = max_rank.clone();
    sorted_max.sort_unstable();
    println!("coverage milestone → vocabulary size needed:");
    for pct in [0.50f64, 0.80, 0.90, 0.95, 0.99] {
        let idx = ((pct * total as f64).ceil() as usize).min(sorted_max.len());
        if idx == 0 {
            continue;
        }
        println!("  {:>3.0}% → K={}", pct * 100.0, sorted_max[idx - 1] + 1);
    }
    println!("top 30 primitives (frequency):");
    for (k, c) in prims.iter().take(30) {
        let pct = 100.0 * *c as f64 / total as f64;
        let disp: String = k.chars().take(150).collect();
        println!("  {c:>5} ({pct:4.1}%)  {disp}");
    }
}
