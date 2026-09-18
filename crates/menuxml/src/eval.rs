//! Per-frame trait evaluation for a parsed menu document.
//!
//! Semantics follow the CS Wiki "Oblivion XML Reference / Operators"
//! page: a trait's operator chain is a **fold**, not a stack — each
//! operator sees the previous sibling's returned value as its "current
//! working value" and combines it with its *argument* (inline literal,
//! `src`/`trait` selection, or the value of its child chain). Booleans
//! are numbers, `&true;` == 2; comparisons return 2 / 0; `onlyif`
//! returns the working value when its argument is true, else zero.
//!
//! Trait reads resolve through [`Selector`]s (`me()`, `parent()`,
//! `sibling(name)`, `child(name)`, `screen()`, `strings()`, or a bare
//! tile name). Evaluation is memoised per frame in document order —
//! parents precede children in the arena, and vanilla expressions only
//! reference tiles "up" the document (parents, earlier siblings, the
//! menu root), so a single forward pass converges without a dependency
//! graph. A reference *backwards* to a not-yet-evaluated tile falls back
//! to the trait's literal prefix (ops evaluate to 0 on first touch) —
//! the same one-frame lag the source engine's dirty-propagation showed
//! for freshly created tiles.

use std::collections::HashMap;

use crate::parse::{Document, Op, OpArg, OpKind, RawTrait, Scalar};

/// The pseudo-tile traits `screen()` answers for. `cropx`/`cropy` are the
/// 4:3-safe insets: Oblivion's UI coordinate math was authored for 4:3,
/// so widescreen screens crop horizontally (`(w - 4h/3) / 2`) and taller
/// aspects crop vertically (`(h - 3w/4) / 2`). This matches the vanilla
/// `safe_zone.xml` usage, which fills bars of `cropy` height at the top
/// and bottom of the screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenTraits {
    pub width: f32,
    pub height: f32,
    pub cropx: f32,
    pub cropy: f32,
}

impl ScreenTraits {
    pub fn new(width: f32, height: f32) -> Self {
        let cropx = ((width - height * 4.0 / 3.0) / 2.0).max(0.0);
        let cropy = ((height - width * 3.0 / 4.0) / 2.0).max(0.0);
        Self {
            width,
            height,
            cropx,
            cropy,
        }
    }
}

/// Engine-side overrides layered over authored trait values — the channel
/// through which gameplay state (actor values, compass heading) reaches
/// the menu (e.g. `hudmain_health_full`'s `user0` fraction).
pub type Overrides = HashMap<(String, String), Scalar>;

/// Where a `src` reference points.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Selector<'a> {
    /// `me()`
    Me,
    /// `parent()`
    Parent,
    /// `sibling(name)`
    Sibling(&'a str),
    /// `child(name)`
    Child(&'a str),
    /// `screen()` / `strings()` pseudo-tiles.
    Pseudo(&'a str),
    /// Bare tile name, resolved through the document's name index.
    Named(&'a str),
}

fn parse_selector(src: &str) -> Selector<'_> {
    let trimmed = src.trim();
    if let Some(inner) = trimmed.strip_suffix("()") {
        match inner {
            "me" => return Selector::Me,
            "parent" => return Selector::Parent,
            "screen" => return Selector::Pseudo("screen"),
            "strings" => return Selector::Pseudo("strings"),
            _ => {}
        }
    }
    if let Some(rest) = trimmed.strip_prefix("sibling(") {
        if let Some(name) = rest.strip_suffix(')') {
            return Selector::Sibling(name.trim());
        }
    }
    if let Some(rest) = trimmed.strip_prefix("child(") {
        if let Some(name) = rest.strip_suffix(')') {
            return Selector::Child(name.trim());
        }
    }
    // `screen`/`strings` also appear without parens in some menus.
    match trimmed {
        "screen" => Selector::Pseudo("screen"),
        "strings" => Selector::Pseudo("strings"),
        _ => Selector::Named(trimmed),
    }
}

/// Live evaluation state for one document: the trait-value memo for the
/// current frame plus the screen/strings pseudo-tiles.
pub struct EvalState<'a> {
    doc: &'a Document,
    pub screen: ScreenTraits,
    /// `strings.xml` traits (`_name` → string), the `strings()` source.
    pub strings: &'a HashMap<String, Scalar>,
    /// Engine overrides, keyed `(tile-name-lower, trait-lower)`; the
    /// empty tile name addresses the menu root.
    overrides: &'a Overrides,
    /// `(tile, trait) -> value` memo for this frame.
    memo: HashMap<(usize, String), Scalar>,
    /// Reference-depth guard: a self-referential op chain (possible in
    /// third-party menus) must not recurse forever.
    depth: u32,
}

/// Hard cap on chained trait reads. Vanilla chains are ≤ 3 deep; 32
/// leaves generous room for mods before we cut a pathological loop off.
const MAX_READ_DEPTH: u32 = 32;

impl<'a> EvalState<'a> {
    pub fn new(
        doc: &'a Document,
        screen: ScreenTraits,
        strings: &'a HashMap<String, Scalar>,
        overrides: &'a Overrides,
    ) -> Self {
        Self {
            doc,
            screen,
            strings,
            overrides,
            memo: HashMap::new(),
            depth: 0,
        }
    }

    /// Evaluate every trait of every tile, in arena (document) order.
    /// The memo then holds this frame's resolved values for layout.
    pub fn resolve_all(&mut self) {
        for tile in 0..self.doc.tiles.len() {
            let keys: Vec<String> = self.doc.tiles[tile].traits.keys().cloned().collect();
            for key in keys {
                self.trait_value(tile, &key);
            }
        }
    }

    /// Read a trait on a tile, evaluating on demand and memoising.
    pub fn trait_value(&mut self, tile: usize, name: &str) -> Scalar {
        if self.depth > MAX_READ_DEPTH {
            log::warn!("menuxml: trait reference chain deeper than {MAX_READ_DEPTH}; cutting off");
            return Scalar::Num(0.0);
        }
        let key = name.to_lowercase();
        if let Some(v) = self.memo.get(&(tile, key.clone())) {
            return v.clone();
        }
        // Engine override wins over anything authored.
        if let Some(owner) = self.doc.tiles[tile].name.as_deref() {
            if let Some(v) = self.overrides.get(&(owner.to_lowercase(), key.clone())) {
                self.memo.insert((tile, key), v.clone());
                return v.clone();
            }
        }
        let raw = self.doc.tiles[tile].traits.get(&key).cloned();
        let value = match raw {
            None => default_trait_value(&key),
            Some(RawTrait::Num(n)) => Scalar::Num(n),
            Some(RawTrait::Str(s)) => Scalar::Str(expand_class_entity(&s)),
            Some(RawTrait::Ops(ops)) => {
                self.depth += 1;
                let v = self.eval_ops(tile, &ops);
                self.depth -= 1;
                v
            }
        };
        self.memo.insert((tile, key), value.clone());
        value
    }

    /// Fold an operator chain, per the wiki's working-value semantics.
    fn eval_ops(&mut self, tile: usize, ops: &[Op]) -> Scalar {
        let mut working: Option<Scalar> = None;
        for op in ops {
            let arg = self.eval_arg(tile, op, working.as_ref());
            working = Some(self.apply(tile, op.kind, working, arg));
        }
        working.unwrap_or(Scalar::Num(0.0))
    }

    /// Resolve an operator's argument. `working` feeds `copy`'s
    /// switch-case form: `copy` selecting a `_name_` trait appends the
    /// working value to pick the concrete case (`_name_23`).
    fn eval_arg(&mut self, tile: usize, op: &Op, working: Option<&Scalar>) -> Scalar {
        match &op.arg {
            OpArg::None => Scalar::Num(0.0),
            OpArg::Lit(s) => s.clone(),
            OpArg::Children(ops) => self.eval_ops(tile, ops),
            OpArg::Src { src, trait_name } => {
                let trait_name = trait_name.as_deref().unwrap_or("");
                // Switch-case: a `copy` (or any op) selecting a trait whose
                // name starts AND ends with `_` picks `_prefix_working`.
                let dynamic = trait_name.starts_with('_')
                    && trait_name.ends_with('_')
                    && trait_name.len() > 1;
                if dynamic {
                    let w = working.map(|w| w.as_num()).unwrap_or(0.0);
                    // Case 0 never fires (wiki); fall to the bare prefix.
                    let concrete = format!("{}{}", trait_name, w as i32);
                    return self.read_selected(tile, src, &concrete);
                }
                self.read_selected(tile, src, trait_name)
            }
        }
    }

    /// Resolve a selector + trait read to a value.
    fn read_selected(&mut self, tile: usize, src: &str, trait_name: &str) -> Scalar {
        match parse_selector(src) {
            Selector::Me => self.trait_value(tile, trait_name),
            Selector::Parent => match self.doc.tiles[tile].parent {
                Some(p) => self.trait_value(p, trait_name),
                // Root's parent: read against the root itself (harmless
                // 0 for unknown traits).
                None => self.trait_value(0, trait_name),
            },
            Selector::Sibling(name) => {
                let parent = self.doc.tiles[tile].parent;
                let sibling = parent.and_then(|p| {
                    self.doc.tiles[p]
                        .children
                        .iter()
                        .copied()
                        .find(|&c| {
                            self.doc.tiles[c]
                                .name
                                .as_deref()
                                .is_some_and(|n| n.eq_ignore_ascii_case(name))
                        })
                });
                match sibling {
                    Some(s) => self.trait_value(s, trait_name),
                    None => {
                        log::debug!("menuxml: sibling('{name}') not found");
                        Scalar::Num(0.0)
                    }
                }
            }
            Selector::Child(name) => {
                let child = self.doc.tiles[tile].children.iter().copied().find(|&c| {
                    self.doc.tiles[c]
                        .name
                        .as_deref()
                        .is_some_and(|n| n.eq_ignore_ascii_case(name))
                });
                match child {
                    Some(c) => self.trait_value(c, trait_name),
                    None => {
                        log::debug!("menuxml: child('{name}') not found");
                        Scalar::Num(0.0)
                    }
                }
            }
            Selector::Pseudo("screen") => match trait_name.to_lowercase().as_str() {
                "width" => Scalar::Num(self.screen.width),
                "height" => Scalar::Num(self.screen.height),
                "cropx" => Scalar::Num(self.screen.cropx),
                "cropy" => Scalar::Num(self.screen.cropy),
                _ => Scalar::Num(0.0),
            },
            Selector::Pseudo("strings") => {
                let key = trait_name.to_lowercase();
                self.strings
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| {
                        log::debug!("menuxml: strings() trait '{trait_name}' missing");
                        Scalar::Num(0.0)
                    })
            }
            Selector::Pseudo(other) => {
                log::debug!("menuxml: unknown pseudo-selector {other}()");
                Scalar::Num(0.0)
            }
            Selector::Named(name) => match self.doc.name_index.get(&name.to_lowercase()) {
                Some(&t) => self.trait_value(t, trait_name),
                // Prefab include files appear as `src="button_long.xml"`
                // references in vanilla; there is nothing to read from a
                // file selector — answer 0 quietly.
                None if name.ends_with(".xml") => Scalar::Num(0.0),
                None => {
                    log::debug!("menuxml: src tile '{name}' not found");
                    Scalar::Num(0.0)
                }
            },
        }
    }

    /// Apply one operator to (working, argument).
    fn apply(&self, tile: usize, kind: OpKind, working: Option<Scalar>, arg: Scalar) -> Scalar {
        use OpKind::*;
        // String flow: `copy` passes strings through untouched (region
        // text, filenames). Everything else coerces to numbers.
        if matches!(kind, Copy) {
            if let Scalar::Str(_) = arg {
                return arg;
            }
        }
        let w = working.as_ref().map(|s| s.as_num()).unwrap_or(0.0);
        let a = arg.as_num();
        let truthy_2 = |b: bool| Scalar::Num(if b { 2.0 } else { 0.0 });
        match kind {
            Copy => Scalar::Num(a),
            Add => Scalar::Num(w + a),
            Sub => Scalar::Num(w - a),
            Mul => Scalar::Num(w * a),
            Div => {
                if a == 0.0 {
                    Scalar::Num(0.0)
                } else {
                    Scalar::Num(w / a)
                }
            }
            Mod => {
                if a == 0.0 {
                    Scalar::Num(0.0)
                } else {
                    Scalar::Num(w.rem_euclid(a))
                }
            }
            Min => Scalar::Num(w.min(a)),
            Max => Scalar::Num(w.max(a)),
            And => truthy_2(arg.truthy() && working.as_ref().map(|s| s.truthy()).unwrap_or(true)),
            Or => truthy_2(arg.truthy() || working.as_ref().map(|s| s.truthy()).unwrap_or(false)),
            Eq => {
                // `eq` "always returns false for strings" (wiki); numeric
                // equality otherwise.
                if matches!(arg, Scalar::Str(_)) || matches!(working, Some(Scalar::Str(_))) {
                    match (working, &arg) {
                        (Some(Scalar::Str(l)), Scalar::Str(r)) => {
                            truthy_2(l.trim() == r.trim())
                        }
                        _ => Scalar::Num(0.0),
                    }
                } else {
                    truthy_2(w == a)
                }
            }
            Neq => match self.apply(tile, Eq, working, arg.clone()) {
                Scalar::Num(n) => truthy_4(n == 0.0),
                _ => Scalar::Num(2.0),
            },
            Lt => truthy_2(w < a),
            Lte => truthy_2(w <= a),
            Gt => truthy_2(w > a),
            Gte => truthy_2(w >= a),
            OnlyIf => {
                if arg.truthy() {
                    working.unwrap_or(Scalar::Num(0.0))
                } else {
                    Scalar::Num(0.0)
                }
            }
            OnlyIfNot => {
                if !arg.truthy() {
                    working.unwrap_or(Scalar::Num(0.0))
                } else {
                    Scalar::Num(0.0)
                }
            }
            Ceil => Scalar::Num((w + a).ceil()),
            Floor => Scalar::Num((w + a).floor()),
            // "If the working value is negative, the argument is added to
            // it, and the absolute value of that result is returned."
            Abs => Scalar::Num(if w < 0.0 { (w + a).abs() } else { w }),
            // `not` replaces the working value: casts the *argument* to
            // boolean, inverts it.
            Not => truthy_2(!arg.truthy()),
        }
    }
}

/// Neq helper: true (2) when the Eq application came out false (0).
fn truthy_4(b: bool) -> Scalar {
    Scalar::Num(if b { 2.0 } else { 0.0 })
}

/// Default values for well-known traits the dialect reads without
/// authoring (Gamebryo's TiMenuItem defaults).
pub fn default_trait_value(name: &str) -> Scalar {
    match name {
        // Booleans default true (&true; == 2).
        "visible" | "locus" | "clips" | "clipwindow" | "target" => Scalar::Num(2.0),
        "alpha" | "red" | "green" | "blue" => Scalar::Num(255.0),
        // Colors default to white; alpha fully opaque.
        _ => Scalar::Num(0.0),
    }
}

/// A `<class>` trait value may still carry an unexpanded class entity in
/// a string that came from a nested `copy` (e.g. `_filename_` case
/// traits authored as `&SomeMenu;`). Expand bare identifiers that match
/// the entity form so comparisons behave.
fn expand_class_entity(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.len() > 2 && trimmed.starts_with('&') && trimmed.ends_with(';') {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        s.to_string()
    }
}
