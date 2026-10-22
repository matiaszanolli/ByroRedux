//! The ahead-of-time check: whether every statement and call of a function
//! (and of what it reaches) is one this VM runs, against typed
//! placeholders (#5800). The per-family native arms live in `natives/`.

use super::*;

/// What the static check knows of an expression's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Num,
    Bool,
    Str,
    Object,
    Quest,
    Player,
    Alias,
    Array,
    /// A Fallout 4 struct.
    Struct,
    None,
}

pub(super) struct Checker<'l> {
    pub(super) library: Option<&'l dyn CheckLibrary>,
    /// Whether the function being checked waits (itself or in a function
    /// it calls as a statement).
    pub(super) waits: bool,
    /// The functions that wait, by lowercased (script, function).
    pub(super) waiting: HashSet<(String, String)>,
    /// The first reason the check failed, for diagnostics.
    pub(super) reason: Option<String>,
    /// Scripts the run will need, by lowercased name.
    pub(super) scripts: HashMap<String, Arc<Script>>,
    pub(super) visiting: HashSet<(String, String)>,
}

pub(super) struct CheckFrame {
    pub(super) script: Arc<Script>,
    /// The script whose body is being checked: the instance's script or one it
    /// extends. `Parent.Fn()` calls past it.
    pub(super) defined_in: Arc<Script>,
    /// What `Self` is: a quest, or a reference for a reference script.
    pub(super) self_kind: Kind,
    pub(super) locals: HashMap<String, Kind>,
    /// Locals' and parameters' declared types (a quest local's script type).
    pub(super) types: HashMap<String, Type>,
}

/// Whether `function` of `script` (a fragment of a quest) runs whole on this
/// VM; `Some` carries every script the run needs.
pub fn check_function(
    script: Arc<Script>,
    function: &str,
    library: Option<&dyn CheckLibrary>,
) -> Option<Vec<Arc<Script>>> {
    check_function_reason(script, function, library).ok()
}

/// [`check_function`] with the first reason it fails.
pub fn check_function_reason(
    script: Arc<Script>,
    function: &str,
    library: Option<&dyn CheckLibrary>,
) -> Result<Vec<Arc<Script>>, String> {
    check_reason(script, function, library, Kind::Quest)
}

/// Whether a reference script runs on this VM: it handles at least one of
/// `events`, and every handler of them — in any state, down the `extends`
/// chain — runs whole. `Ok` carries the events it handles and every script
/// the runs need; `Err` the first reason it does not run.
pub fn check_reference_script(
    script: Arc<Script>,
    events: &[&str],
    library: Option<&dyn CheckLibrary>,
) -> Result<(Vec<String>, Vec<Arc<Script>>), String> {
    // #6000 — the checker walks the decompiler's AST of a mod-supplied `.pex`
    // and runs on the cell-load attach path and a per-frame system, so it sits
    // inside the same panic net as the rest of the `.pex` path (#3948/#1816): a
    // panic declines the script instead of aborting the load or the frame loop.
    crate::translate::catching_panics("check_reference_script", || check_reference_script_unguarded(script, events, library))
        .unwrap_or_else(|| Err("the checker panicked".to_owned()))
}

fn check_reference_script_unguarded(
    script: Arc<Script>,
    events: &[&str],
    library: Option<&dyn CheckLibrary>,
) -> Result<(Vec<String>, Vec<Arc<Script>>), String> {
    let mut checker = Checker::new(library, &script);
    let mut handled = Vec::new();
    for event in events {
        if checker.versions(&script, event).is_empty() || checker.hit_by_registration(&script, event) {
            continue;
        }
        if checker.check_call_into(&script, event, true, Kind::Object).is_none() {
            let reason = checker.reason.take().unwrap_or_else(|| "unsupported".into());
            return Err(format!("{event}: {reason}"));
        }
        handled.push((*event).to_owned());
    }
    if handled.is_empty() {
        return Err("handles no delivered event".into());
    }
    Ok((handled, checker.scripts.into_values().collect()))
}

/// Whether an alias script runs on this VM: [`check_reference_script`] with
/// `Self` a `ReferenceAlias`, over the events an alias script receives.
pub fn check_alias_script(
    script: Arc<Script>,
    events: &[&str],
    library: Option<&dyn CheckLibrary>,
) -> Result<(Vec<String>, Vec<Arc<Script>>), String> {
    // #6000 — the checker walks the decompiler's AST of a mod-supplied `.pex`
    // and runs on the cell-load attach path and a per-frame system, so it sits
    // inside the same panic net as the rest of the `.pex` path (#3948/#1816): a
    // panic declines the script instead of aborting the load or the frame loop.
    crate::translate::catching_panics("check_alias_script", || check_alias_script_unguarded(script, events, library))
        .unwrap_or_else(|| Err("the checker panicked".to_owned()))
}

fn check_alias_script_unguarded(
    script: Arc<Script>,
    events: &[&str],
    library: Option<&dyn CheckLibrary>,
) -> Result<(Vec<String>, Vec<Arc<Script>>), String> {
    let mut checker = Checker::new(library, &script);
    let mut handled = Vec::new();
    for event in events {
        if checker.versions(&script, event).is_empty() || checker.hit_by_registration(&script, event) {
            continue;
        }
        if checker.check_call_into(&script, event, true, Kind::Alias).is_none() {
            let reason = checker.reason.take().unwrap_or_else(|| "unsupported".into());
            return Err(format!("{event}: {reason}"));
        }
        handled.push((*event).to_owned());
    }
    if handled.is_empty() {
        return Err("handles no delivered event".into());
    }
    Ok((handled, checker.scripts.into_values().collect()))
}

/// The installed scripts as a [`CheckLibrary`].
pub(super) struct Installed<'a>(pub(super) &'a HashMap<String, Arc<Script>>);

impl crate::translate::effects::ScriptLibrary for Installed<'_> {
    fn script(&self, name: &str) -> Option<Arc<Script>> {
        self.0.get(&name.to_ascii_lowercase()).cloned()
    }
}

/// What an installed script instance is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(in crate::fragment) enum SelfKind {
    Quest,
    Object,
    Alias,
}

impl SelfKind {
    /// The kind of the instance owned by `owner`: a quest (`on_quest`), a quest
    /// alias, or a reference.
    pub(in crate::fragment) fn of_owner(owner: u32, on_quest: bool) -> Self {
        if on_quest {
            Self::Quest
        } else if crate::fragment::is_alias_owner(owner) {
            Self::Alias
        } else {
            Self::Object
        }
    }
}

/// Whether `handler` of the installed `script` runs whole on this VM,
/// against the installed scripts — on a quest, a reference or an alias.
pub(in crate::fragment) fn handler_runs(scripts: &HashMap<String, Arc<Script>>, script: &str, handler: &str, self_kind: SelfKind) -> bool {
    let Some(script) = scripts.get(&script.to_ascii_lowercase()) else {
        return false;
    };
    let kind = match self_kind {
        SelfKind::Quest => Kind::Quest,
        SelfKind::Object => Kind::Object,
        SelfKind::Alias => Kind::Alias,
    };
    let library = Installed(scripts);
    let mut checker = Checker::new(Some(&library), script);
    checker.versions(script, handler).is_empty() || checker.check_call_into(script, handler, true, kind).is_some()
}

pub(super) fn check_reason(
    script: Arc<Script>,
    function: &str,
    library: Option<&dyn CheckLibrary>,
    self_kind: Kind,
) -> Result<Vec<Arc<Script>>, String> {
    let mut checker = Checker::new(library, &script);
    match checker.check_call_into(&script, function, true, self_kind) {
        Some(_) => Ok(checker.scripts.into_values().collect()),
        None => Err(checker.reason.unwrap_or_else(|| "unsupported".into())),
    }
}

pub(super) fn kind_of_type(ty: &Type, library: Option<&dyn CheckLibrary>) -> Kind {
    match ty {
        Type::Int | Type::Float => Kind::Num,
        Type::Bool => Kind::Bool,
        Type::String => Kind::Str,
        Type::Array(_) => Kind::Array,
        Type::Var => Kind::None,
        Type::Object(name) => {
            let lower = name.0.to_ascii_lowercase();
            if lower == "quest" || library.is_some_and(|library| extends_quest(library, &lower, 0)) {
                Kind::Quest
            } else if lower.contains("alias") {
                Kind::Alias
            } else {
                Kind::Object
            }
        }
    }
}

pub(super) fn extends_quest(library: &dyn CheckLibrary, name: &str, depth: usize) -> bool {
    if depth > 16 {
        return false;
    }
    let Some(script) = library.script(name) else {
        return false;
    };
    match &script.parent {
        Some(parent) if parent.node.0.eq_ignore_ascii_case("quest") => true,
        Some(parent) => extends_quest(library, &parent.node.0.to_ascii_lowercase(), depth + 1),
        None => false,
    }
}

impl<'l> Checker<'l> {
    pub(super) fn new(library: Option<&'l dyn CheckLibrary>, script: &Arc<Script>) -> Self {
        let mut checker = Checker {
            library,
            reason: None,
            waits: false,
            waiting: HashSet::new(),
            scripts: HashMap::new(),
            visiting: HashSet::new(),
        };
        checker
            .scripts
            .insert(script.name.node.0.to_ascii_lowercase(), Arc::clone(script));
        checker
    }

    /// The type of `member` of the script an Object receiver is declared as
    /// (not an engine class).
    pub(super) fn reference_member_type(&mut self, frame: &CheckFrame, owner: Kind, receiver: &Expr, member: &str) -> Option<Type> {
        if owner != Kind::Object {
            return None;
        }
        let script = self.declared_script(frame, receiver)?;
        if is_engine_class(&script) {
            return None;
        }
        match self.member_type(&script, member)? {
            Type::Array(_) => None,
            ty => Some(ty),
        }
    }

    /// The check's kind of the declared type `ty` in `script`: a struct type
    /// is a struct, the rest as [`kind_of_type`].
    pub(super) fn kind_of_declared(&mut self, script: &Arc<Script>, ty: &Type) -> Kind {
        if let Type::Object(name) = ty {
            if self.struct_declaration(script, &name.0).is_some() {
                return Kind::Struct;
            }
        }
        kind_of_type(ty, self.library)
    }

    /// The declaration of the struct type `name` from `script`'s chain or,
    /// for `Script#Struct`, that script — registering it for the run.
    pub(super) fn struct_declaration(&mut self, script: &Arc<Script>, name: &str) -> Option<byroredux_papyrus::ast::Struct> {
        let owners = match name.split_once('#') {
            Some((owner, _)) => {
                let owner = self.library?.script(owner)?;
                self.scripts
                    .insert(owner.name.node.0.to_ascii_lowercase(), Arc::clone(&owner));
                vec![owner]
            }
            None => self.chain(script),
        };
        let mut known: HashMap<String, Arc<Script>> = HashMap::new();
        for owner in &owners {
            known.insert(owner.name.node.0.to_ascii_lowercase(), Arc::clone(owner));
        }
        struct_declaration(&owners[0], name, &known)
    }

    /// The declared type of `e`, where the check can know it: a typed local
    /// or member, an array's element, a struct's member, a cast.
    pub(super) fn static_type(&mut self, frame: &CheckFrame, e: &Expr) -> Option<Type> {
        match e {
            Expr::Ident(name) => match frame.types.get(&name.0.to_ascii_lowercase()) {
                Some(ty) => Some(ty.clone()),
                None => {
                    let script = Arc::clone(&frame.script);
                    self.member_type(&script, &name.0)
                }
            },
            Expr::Cast { target_type, .. } => Some(target_type.node.clone()),
            Expr::Index { object, .. } => match self.static_type(frame, &object.node)? {
                Type::Array(element) => Some(*element),
                _ => None,
            },
            Expr::MemberAccess { object, member } => {
                let owner = self.static_type(frame, &object.node)?;
                let Type::Object(owner) = owner else {
                    return None;
                };
                let script = Arc::clone(&frame.script);
                if let Some(declaration) = self.struct_declaration(&script, &owner.0) {
                    return declaration
                        .members
                        .iter()
                        .find(|m| m.name.node.0.eq_ignore_ascii_case(&member.node.0))
                        .map(|m| m.ty.node.clone());
                }
                let declared = self.library?.script(&owner.0)?;
                self.member_type(&declared, &member.node.0)
            }
            _ => None,
        }
    }

    /// An engine class's constant `name`'s type, registering the class.
    pub(super) fn engine_constant_type(&mut self, name: &str) -> Option<Type> {
        CONSTANT_CLASSES.iter().find_map(|class| {
            let script = self.library?.script(class)?;
            let ty = constant_property(&script, name)?.ty.node.clone();
            self.scripts.insert((*class).to_owned(), Arc::clone(&script));
            Some(ty)
        })
    }

    /// `name`'s declared type in `script` or its parents (registering the
    /// parents the run will need).
    pub(super) fn member_type(&mut self, script: &Script, name: &str) -> Option<Type> {
        if let Some(ty) = script_member_type(script, name) {
            return Some(ty);
        }
        let mut parent = script.parent.as_ref().map(|p| p.node.0.clone());
        for _ in 0..16 {
            let name_of_parent = parent?;
            let lower = name_of_parent.to_ascii_lowercase();
            if matches!(lower.as_str(), "quest" | "topicinfo" | "scene" | "objectreference" | "actor") {
                // An engine class lends only its constants.
                let class = self.library?.script(&name_of_parent)?;
                let ty = constant_property(&class, name)?.ty.node.clone();
                self.scripts.insert(lower, Arc::clone(&class));
                return Some(ty);
            }
            let script = self.library?.script(&name_of_parent)?;
            self.scripts.insert(lower, Arc::clone(&script));
            if let Some(ty) = script_member_type(&script, name) {
                return Some(ty);
            }
            parent = script.parent.as_ref().map(|p| p.node.0.clone());
        }
        None
    }

    pub(super) fn fail<T>(&mut self, reason: impl FnOnce() -> String) -> Option<T> {
        if self.reason.is_none() {
            self.reason = Some(reason());
        }
        None
    }

    /// `script` and the scripts it extends, registering each for the run.
    pub(super) fn chain(&mut self, script: &Arc<Script>) -> Vec<Arc<Script>> {
        let mut chain = vec![Arc::clone(script)];
        while chain.len() < 16 {
            let Some(parent) = chain.last().and_then(|last| last.parent.as_ref()).map(|p| p.node.0.clone()) else {
                break;
            };
            let Some(parent) = self.library.and_then(|library| library.script(&parent)) else {
                break;
            };
            self.scripts
                .insert(parent.name.node.0.to_ascii_lowercase(), Arc::clone(&parent));
            chain.push(parent);
        }
        chain
    }

    /// Every body a call of `name` on an instance of `script` may run — in
    /// any state, down the `extends` chain to the engine classes — as
    /// (script, state).
    pub(super) fn versions(&mut self, script: &Arc<Script>, name: &str) -> Vec<(Arc<Script>, Option<String>)> {
        let mut versions = Vec::new();
        for script in self.chain(script).into_iter().take_while(|script| !is_engine_class(script)) {
            let states = std::iter::once(None).chain(state_names(&script).map(|state| Some(state.to_owned())));
            for state in states.collect::<Vec<_>>() {
                if callable_in(&script, state.as_deref(), name).is_some() {
                    versions.push((Arc::clone(&script), state));
                }
            }
        }
        versions
    }

    /// Check a call of `function` on an instance of `script` whose `Self`
    /// is `self_kind`: every version of it. `statement`: whether the call is
    /// a statement — a function that waits may not be called for its value.
    pub(super) fn check_call_into(&mut self, script: &Arc<Script>, function: &str, statement: bool, self_kind: Kind) -> Option<Kind> {
        let key = (script.name.node.0.to_ascii_lowercase(), function.to_ascii_lowercase());
        let versions = self.versions(script, function);
        let returns = versions
            .iter()
            .find_map(|(defined_in, state)| callable_in(defined_in, state.as_deref(), function)?.returns.cloned())
            .map_or(Kind::None, |ty| kind_of_type(&ty.node, self.library));
        if versions.is_empty() {
            return self.fail(|| format!("no function {function}"));
        }
        if !self.visiting.insert(key.clone()) {
            // Checked already, or recursion: assume the declared return.
            return self.call_waits(self.waiting.contains(&key), statement).map(|_| returns);
        }
        let outer = std::mem::replace(&mut self.waits, false);
        for (defined_in, state) in &versions {
            let callable = callable_in(defined_in, state.as_deref(), function)?;
            let mut frame = CheckFrame {
                script: Arc::clone(script),
                defined_in: Arc::clone(defined_in),
                self_kind,
                locals: HashMap::new(),
                types: HashMap::new(),
            };
            for param in callable.params {
                let key = param.name.node.0.to_ascii_lowercase();
                let kind = self.kind_of_declared(script, &param.ty.node);
                frame.locals.insert(key.clone(), kind);
                frame.types.insert(key, param.ty.node.clone());
            }
            callable
                .body
                .iter()
                .try_for_each(|statement| self.check_stmt(&mut frame, &statement.node))?;
        }
        let waits = std::mem::replace(&mut self.waits, outer);
        if waits {
            self.waiting.insert(key);
        }
        self.call_waits(waits, statement).map(|_| returns)
    }

    /// Check an event handler a registration delivers to later, installing
    /// the scripts it needs when it runs. A handler the VM cannot run does
    /// not fail the registering code — the delivery checks it again and
    /// skips it — so its failed check is rolled back. It may wait; the
    /// registering call does not.
    pub(super) fn check_handler(&mut self, script: &Arc<Script>, handler: &str, self_kind: Kind) -> Option<()> {
        let saved = (
            self.waits,
            self.reason.clone(),
            self.visiting.clone(),
            self.waiting.clone(),
            self.scripts.clone(),
        );
        if self.check_call_into(script, handler, true, self_kind).is_none() {
            (self.reason, self.visiting, self.waiting, self.scripts) = (saved.1, saved.2, saved.3, saved.4);
        }
        self.waits = saved.0;
        Some(())
    }

    /// A call to a function that `waits` makes its caller wait too.
    pub(super) fn call_waits(&mut self, waits: bool, statement: bool) -> Option<()> {
        if waits {
            if !statement {
                return self.fail(|| "a wait in a function called for its value".into());
            }
            self.waits = true;
        }
        Some(())
    }

    pub(super) fn check_block(&mut self, frame: &mut CheckFrame, body: &[Spanned<Stmt>]) -> Option<()> {
        for statement in body {
            self.check_stmt(frame, &statement.node)?;
        }
        Some(())
    }

    pub(super) fn check_stmt(&mut self, frame: &mut CheckFrame, statement: &Stmt) -> Option<()> {
        match statement {
            Stmt::VarDecl(variable) => {
                if let Some(init) = &variable.initial_value {
                    self.kind(frame, &init.node)?;
                }
                let key = variable.name.node.0.to_ascii_lowercase();
                let kind = self.kind_of_declared(&frame.script, &variable.ty.node);
                frame.locals.insert(key.clone(), kind);
                frame.types.insert(key, variable.ty.node.clone());
            }
            Stmt::Assign { target, value, .. } => {
                let value_kind = self.kind(frame, &value.node)?;
                match &target.node {
                    Expr::Ident(_) => {}
                    Expr::MemberAccess { object, member } => {
                        let owner = self.kind(frame, &object.node)?;
                        if owner == Kind::Struct {
                            if self.static_type(frame, &target.node).is_none() {
                                let member = member.node.0.clone();
                                return self.fail(|| format!("assignment to .{member} of an untyped struct"));
                            }
                        } else if owner == Kind::Object && member.node.0.eq_ignore_ascii_case("value") && value_kind == Kind::Num {
                            // `Global.Value = n`.
                            let (callee, args) = set_value_call(object, value);
                            self.check_call(frame, &callee.node, &args, true)?;
                        } else if value_kind != Kind::Array
                            && self.reference_member_type(frame, owner, &object.node, &member.node.0).is_some()
                        {
                            // A variable of a script on another reference.
                        } else if owner != Kind::Quest || matches!(value_kind, Kind::Str | Kind::Array) {
                            return self.fail(|| format!("{value_kind:?} assignment to a member of {owner:?}"));
                        }
                    }
                    Expr::Index { object, index } => {
                        self.kind(frame, &object.node)?;
                        self.kind(frame, &index.node)?;
                    }
                    _ => return None,
                }
            }
            Stmt::Return(value) => {
                if let Some(value) = value {
                    self.kind(frame, &value.node)?;
                }
            }
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            } => {
                self.kind(frame, &condition.node)?;
                self.check_block(frame, body)?;
                for (condition, body) in elseif_clauses {
                    self.kind(frame, &condition.node)?;
                    self.check_block(frame, body)?;
                }
                if let Some(body) = else_body {
                    self.check_block(frame, body)?;
                }
            }
            Stmt::While { condition, body } => {
                self.kind(frame, &condition.node)?;
                self.check_block(frame, body)?;
            }
            Stmt::ExprStmt(expression) => match &expression.node {
                Expr::Call { callee, args } => {
                    self.check_call(frame, &callee.node, args, true)?;
                }
                other => {
                    self.kind(frame, other)?;
                }
            },
        }
        Some(())
    }

    pub(super) fn kind(&mut self, frame: &mut CheckFrame, e: &Expr) -> Option<Kind> {
        Some(match e {
            Expr::IntLit(_) | Expr::FloatLit(_) => Kind::Num,
            Expr::BoolLit(_) => Kind::Bool,
            Expr::StringLit(_) => Kind::Str,
            Expr::NoneLit => Kind::None,
            Expr::Ident(name) => {
                let key = name.0.to_ascii_lowercase();
                if let Some(kind) = frame.locals.get(&key) {
                    *kind
                } else if key == "self" {
                    frame.self_kind
                } else if is_state_variable(&name.0) {
                    Kind::Str
                } else {
                    let script = Arc::clone(&frame.script);
                    let Some(ty) = self.member_type(&script, &name.0) else {
                        let name = name.0.clone();
                        return self.fail(|| format!("unknown name {name}"));
                    };
                    self.kind_of_declared(&script, &ty)
                }
            }
            Expr::Cast { expr, target_type } => {
                let inner = self.kind(frame, &expr.node)?;
                match kind_of_type(&target_type.node, self.library) {
                    Kind::Object if inner == Kind::Quest => Kind::Quest,
                    Kind::Object if inner == Kind::Player => Kind::Player,
                    kind => kind,
                }
            }
            Expr::UnaryOp { op, operand } => {
                let inner = self.kind(frame, &operand.node)?;
                match op {
                    UnaryOp::Not => Kind::Bool,
                    UnaryOp::Neg => {
                        if inner != Kind::Num {
                            return None;
                        }
                        Kind::Num
                    }
                }
            }
            Expr::BinaryOp { left, op, right } => {
                let l = self.kind(frame, &left.node)?;
                let r = self.kind(frame, &right.node)?;
                match op {
                    BinaryOp::And | BinaryOp::Or => Kind::Bool,
                    BinaryOp::Eq | BinaryOp::Ne => Kind::Bool,
                    BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge => {
                        if !matches!(l, Kind::Num | Kind::Bool | Kind::Str) || !matches!(r, Kind::Num | Kind::Bool | Kind::Str) {
                            return None;
                        }
                        Kind::Bool
                    }
                    BinaryOp::StrCat => Kind::Str,
                    BinaryOp::Add if l == Kind::Str || r == Kind::Str => Kind::Str,
                    _ => {
                        if !matches!(l, Kind::Num | Kind::Bool) || !matches!(r, Kind::Num | Kind::Bool) {
                            return None;
                        }
                        Kind::Num
                    }
                }
            }
            Expr::Index { object, index } => {
                if self.kind(frame, &object.node)? != Kind::Array {
                    return None;
                }
                self.kind(frame, &index.node)?;
                // The element's declared type when the array's is known;
                // else an untracked value.
                match self.static_type(frame, e) {
                    Some(ty) => {
                        let script = Arc::clone(&frame.script);
                        self.kind_of_declared(&script, &ty)
                    }
                    None => Kind::Object,
                }
            }
            Expr::NewStruct { ty } => {
                let script = Arc::clone(&frame.script);
                match &ty.node {
                    Type::Object(name) if self.struct_declaration(&script, &name.0).is_some() => Kind::Struct,
                    _ => return self.fail(|| "new of an unknown struct".into()),
                }
            }
            Expr::New { size, .. } => {
                self.kind(frame, &size.node)?;
                Kind::Array
            }
            Expr::ArrayLit(items) => {
                for item in items {
                    self.kind(frame, &item.node)?;
                }
                Kind::Array
            }
            Expr::MemberAccess { object, member } => {
                let owner = self.kind(frame, &object.node)?;
                if member.node.0.eq_ignore_ascii_case("length") && owner == Kind::Array {
                    return Some(Kind::Num);
                }
                if owner == Kind::Struct {
                    let Some(ty) = self.static_type(frame, e) else {
                        let member = member.node.0.clone();
                        return self.fail(|| format!("member .{member} of an untyped struct"));
                    };
                    let script = Arc::clone(&frame.script);
                    return Some(self.kind_of_declared(&script, &ty));
                }
                if owner == Kind::Quest {
                    // Another quest script's member: its declared type when
                    // the quest's script is known (an `ActorValue Property`
                    // is no number), else a numeric variable, what the VM
                    // reads from an untyped quest.
                    let declared = self
                        .declared_script(frame, &object.node)
                        .and_then(|script| self.member_type(&script, &member.node.0));
                    match declared {
                        Some(ty) => {
                            let script = Arc::clone(&frame.script);
                            self.kind_of_declared(&script, &ty)
                        }
                        None => Kind::Num,
                    }
                } else if member.node.0.eq_ignore_ascii_case("value") {
                    Kind::Num
                } else if let Some(ty) = self.engine_constant_type(&member.node.0).filter(|_| owner != Kind::None) {
                    kind_of_type(&ty, self.library)
                } else if let Some(ty) = self.reference_member_type(frame, owner, &object.node, &member.node.0) {
                    kind_of_type(&ty, self.library)
                } else {
                    let member = member.node.0.clone();
                    return self.fail(|| format!("member .{member} of {owner:?}"));
                }
            }
            Expr::Call { callee, args } => self.check_call(frame, &callee.node, args, false)?,
            Expr::Is { expr, .. } => {
                self.kind(frame, &expr.node)?;
                Kind::Bool
            }
            Expr::ParentAccess => return self.fail(|| "Parent".into()),
        })
    }

    /// A call's result kind, or `None` when the VM cannot run it.
    pub(super) fn check_call(&mut self, frame: &mut CheckFrame, callee: &Expr, args: &[CallArg], statement: bool) -> Option<Kind> {
        // Starfield guards: the decompiler lifts `LockGuards` / `UnlockGuards` /
        // `TryLockGuards` to `Guard.lock(...)` / `.unlock(...)` / `.trylock(...)`
        // whose arguments name guards, which are not values (see `guard_call`).
        if let Some(call) = guard_call(callee) {
            return match call {
                GuardCall::Lock | GuardCall::Unlock if statement => Some(Kind::None),
                GuardCall::TryLock => Some(Kind::Bool),
                _ => self.fail(|| "a guard call used for its value".into()),
            };
        }
        let mut arg_kinds = Vec::with_capacity(args.len());
        for arg in args {
            let kind = self.kind(frame, &arg.value.node)?;
            // A literal argument is checked as written: the run sees it too;
            // so is an engine constant, which the run passes by name.
            let constant = constant_name(&arg.value.node)
                .filter(|name| !frame.locals.contains_key(&name.to_ascii_lowercase()))
                .filter(|name| self.engine_constant_type(name).is_some());
            arg_kinds.push(match (constant, literal_of(&arg.value.node)) {
                (Some(name), _) => Arg::Constant(name.to_owned()),
                (None, Some(literal)) => Arg::Literal(literal),
                (None, None) => Arg::Kind(kind),
            });
        }
        let (receiver, name) = match callee {
            Expr::Ident(name) => (None, name.0.to_ascii_lowercase()),
            Expr::MemberAccess { object, member } => (Some(&object.node), member.node.0.to_ascii_lowercase()),
            _ => return None,
        };
        // A hit registration's aggressor filter is matched against a reference
        // or a faction, so a property or local declared as one (an `Actor`, an
        // `ObjectReference`, a `ReferenceAlias`, a `Faction`) counts as one; a
        // form of any other type (a keyword, a base object) is a filter this
        // runtime does not match.
        if name == "registerforhitevent" {
            if let (Some(arg), Some(Arg::Kind(Kind::Object))) = (args.get(1), arg_kinds.get(1)) {
                // A cast to `ScriptObject` hides the declared type: look through it.
                let mut declared = &arg.value.node;
                while let Expr::Cast { expr, .. } = declared {
                    declared = &expr.node;
                }
                let is_reference = self.static_type(frame, declared).is_some_and(|ty| match ty {
                    Type::Object(name) => {
                        let lower = name.0.to_ascii_lowercase();
                        lower == "objectreference" || lower == "actor" || lower == "faction" || lower.contains("alias")
                    }
                    _ => false,
                });
                if is_reference {
                    arg_kinds[1] = Arg::Kind(Kind::Player);
                }
            }
        }
        // Help-message housekeeping (see [`help_message_housekeeping`]).
        if statement {
            if let Some(arity) = help_message_housekeeping(callee) {
                if arg_kinds.len() != arity {
                    return self.fail(|| format!("{name} as written"));
                }
                return Some(Kind::None);
            }
        }
        // A function of an `InputEnableLayer` the receiver is declared as.
        if let (Some(native), Some(receiver)) = (layer_native(&name), receiver) {
            let mut declared = receiver;
            while let Expr::Cast { expr, .. } = declared {
                declared = &expr.node;
            }
            if matches!(
                self.static_type(frame, declared),
                Some(Type::Object(ty)) if ty.0.eq_ignore_ascii_case("inputenablelayer")
            ) {
                let flag = |arg: &Arg| matches!(arg, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool | Kind::Num));
                let shaped = match native {
                    LayerNative::Disable | LayerNative::Enable => arg_kinds.len() <= 11 && arg_kinds.iter().all(flag),
                    LayerNative::Set(_) => arg_kinds.len() <= 1 && arg_kinds.iter().all(flag),
                    LayerNative::Reset | LayerNative::Delete | LayerNative::Query(_) => arg_kinds.is_empty(),
                };
                if !shaped {
                    return self.fail(|| format!("InputEnableLayer.{name} as written"));
                }
                return Some(if matches!(native, LayerNative::Query(_)) { Kind::Bool } else { Kind::None });
            }
        }
        // `<Sound>.Play(akSource)`: the receiver must be declared a `Sound`
        // (`VisualEffect.Play` has the same name and a different meaning). The
        // call returns the playback instance id (0: "the sound failed to play");
        // here that is 1 when the sound was queued, which is right for the scripts
        // that keep it or test it and moot for the ones that would steer the
        // instance - `StopInstance` and `SetInstanceVolume` stay unsupported, so a
        // script that does is declined whole.
        if name == "play" && matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Object | Kind::Player | Kind::Alias)]) {
            if let Some(receiver) = receiver {
                let mut declared = receiver;
                while let Expr::Cast { expr, .. } = declared {
                    declared = &expr.node;
                }
                if matches!(
                    self.static_type(frame, declared),
                    Some(Type::Object(ty)) if ty.0.eq_ignore_ascii_case("sound")
                ) {
                    return Some(if statement { Kind::None } else { Kind::Num });
                }
            }
        }
        // `<MusicType>.Add()` / `.Remove()` as a statement (see the runtime arm).
        if matches!(name.as_str(), "add" | "remove") && statement && arg_kinds.is_empty()
            && receiver.is_some_and(|receiver| declares_music_type(&frame.script, receiver)) {
                return Some(Kind::None);
            }
        // The frame's own script function.
        if receiver.is_none()
            || receiver.is_some_and(|r| matches!(r, Expr::Ident(id) if id.0.eq_ignore_ascii_case("self")))
        {
            let script = Arc::clone(&frame.script);
            if !self.versions(&script, &name).is_empty() {
                return self.check_call_into(&script, &name, statement, frame.self_kind);
            }
            if is_state_event(callee) {
                return Some(Kind::None);
            }
            if name == "sendcustomevent" {
                let ok = statement
                    && matches!(arg_kinds.first(), Some(Arg::Literal(Expr::StringLit(_))))
                    && match arg_kinds.get(1) {
                        None | Some(Arg::Kind(Kind::Array | Kind::None)) | Some(Arg::Literal(Expr::NoneLit)) => arg_kinds.len() <= 2,
                        _ => false,
                    };
                if !ok {
                    return self.fail(|| "SendCustomEvent as written".into());
                }
                return Some(Kind::None);
            }
            if name == "getstate" {
                return Some(Kind::Str);
            }
            // An update registration: the handler it delivers to must run.
            if crate::fragment::EventRequest::is_call(&name) {
                return self.check_event_registration(&script, &name, &arg_kinds, statement, frame.self_kind);
            }
            if let Some(handler) = registration_handler(&name) {
                let interval_ok = update_args_ok(&name, &arg_kinds);
                if !statement || !interval_ok {
                    return self.fail(|| format!("{name} as written"));
                }
                if !handler.is_empty() && !self.versions(&script, handler).is_empty() {
                    self.check_handler(&script, handler, frame.self_kind)?;
                }
                return Some(Kind::None);
            }
            // CK *RegisterForAnimationEvent - Form* on the script itself: the
            // `OnAnimationEvent` it delivers must run whole (see the runtime arm).
            if matches!(name.as_str(), "registerforanimationevent" | "unregisterforanimationevent")
                && matches!(
                    arg_kinds.as_slice(),
                    [Arg::Kind(Kind::Object | Kind::Player), Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]
                )
            {
                if name == "registerforanimationevent" && !self.versions(&script, "onanimationevent").is_empty() {
                    self.check_handler(&script, "onanimationevent", frame.self_kind)?;
                }
                return Some(if name == "registerforanimationevent" { Kind::Bool } else { Kind::None });
            }
            if name == "gotostate" && statement {
                // The switch runs the state events, called for no value.
                for event in ["onendstate", "onbeginstate"] {
                    if !self.versions(&script, event).is_empty() {
                        self.check_call_into(&script, event, false, frame.self_kind)?;
                    }
                }
                return Some(Kind::None);
            }
        }
        let Some(receiver) = receiver else {
            return self.fail(|| format!("global {name}"));
        };
        // `Parent.Fn()`: the version past the script this body is defined in.
        if matches!(receiver, Expr::ParentAccess) {
            let defined = Arc::clone(&frame.defined_in);
            let parent = defined
                .parent
                .as_ref()
                .and_then(|parent| self.library.and_then(|library| library.script(&parent.node.0)));
            if let Some(parent) = parent {
                self.scripts.insert(parent.name.node.0.to_ascii_lowercase(), Arc::clone(&parent));
                if !is_engine_class(&parent) && !self.versions(&parent, &name).is_empty() {
                    return self.check_call_into(&parent, &name, statement, frame.self_kind);
                }
            }
            // An engine class's event has no body to run.
            if statement && is_event_name(&callee_name(callee)) {
                return Some(Kind::None);
            }
            return self.fail(|| format!("Parent.{name}"));
        }
        // `ScriptName.Fn()`: a global function of a script the library has.
        if let Expr::Ident(script_name) = receiver {
            let key = script_name.0.to_ascii_lowercase();
            let is_value = key == "self"
                || frame.locals.contains_key(&key)
                || static_class(receiver).is_some()
                || {
                    let script = Arc::clone(&frame.script);
                    self.member_type(&script, &script_name.0).is_some()
                };
            if !is_value {
                if let Some(script) = self.library.and_then(|library| library.script(&script_name.0)) {
                    if !is_engine_class(&script) && !self.versions(&script, &name).is_empty() {
                        self.scripts.insert(key, Arc::clone(&script));
                        return self.check_call_into(&script, &name, statement, Kind::None);
                    }
                }
            }
        }
        if let Some(class) = static_class(receiver) {
            return self.check_static_class(receiver, class, &name, &arg_kinds, statement);
        }
        // A static native on an *engine* class the corpus spells inline —
        // `InputEnableLayer.GetOrCreate(3)` (the FO4+/SF player-controls
        // idiom), `Message.Show(…)`. `static_class` above covers only the
        // four global-function classes; the engine's other builtin types
        // ride the archive as native-only stubs, so the Ident names the
        // CLASS, not a value: it has no local/member binding, and `kind`
        // below would fail the whole check as `unknown name <class>` —
        // even though the stub being an engine class is exactly what says
        // "no script body to run; this is a native". Route it through the
        // same placeholder-native classification any other engine receiver
        // gets; the effect table decides, exactly as for `Game.*`.
        if let Expr::Ident(id) = receiver {
            let key = id.0.to_ascii_lowercase();
            if key == "inputenablelayer" && name == "create" && arg_kinds.is_empty() && !frame.locals.contains_key(&key) {
                return Some(Kind::Object);
            }
            let names_a_value = frame.locals.contains_key(&key)
                || id.0.eq_ignore_ascii_case("self")
                || is_state_variable(&id.0)
                || self.member_type(&frame.script, &id.0).is_some();
            if !names_a_value
                && self
                    .library
                    .and_then(|library| library.script(&key))
                    .is_some_and(|script| is_engine_class(&script))
            {
                let result =
                    self.check_native(receiver.clone(), &name, &arg_kinds, statement);
                if result.is_none() {
                    return self.fail(|| format!("static native {name}/{}", arg_kinds.len()));
                }
                return result;
            }
        }
        let receiver_kind = self.kind(frame, receiver)?;
        if receiver_kind == Kind::Array {
            return self.check_array_call(&name, &arg_kinds, statement);
        }
        // A script-filled `RefCollectionAlias` (declared as one).
        if receiver_kind == Kind::Alias && COLLECTION_NATIVES.contains(&name.as_str()) {
            let collection = self
                .static_type(frame, receiver)
                .is_some_and(|ty| matches!(&ty, Type::Object(n) if n.0.eq_ignore_ascii_case("RefCollectionAlias")));
            if collection {
                let object = |arg: &Arg| matches!(arg, Arg::Kind(Kind::Object | Kind::Player | Kind::Alias));
                let ok = match name.as_str() {
                    "getcount" | "getarray" | "getactorarray" => arg_kinds.is_empty(),
                    "enableall" | "disableall" => {
                        statement && matches!(arg_kinds.as_slice(), [] | [Arg::Literal(Expr::BoolLit(_))])
                    }
                    "evaluateall" => statement && arg_kinds.is_empty(),
                    "getat" => matches!(arg_kinds.as_slice(), [Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num)]),
                    "find" => arg_kinds.len() == 1 && object(&arg_kinds[0]),
                    "addref" | "removeref" => statement && arg_kinds.len() == 1 && object(&arg_kinds[0]),
                    "addarray" => statement && matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Array)]),
                    "addrefcollection" => statement && matches!(arg_kinds.as_slice(), [Arg::Kind(Kind::Alias)]),
                    "removeall" => statement && arg_kinds.is_empty(),
                    _ => false,
                };
                if !ok {
                    return self.fail(|| format!("collection {name} as written"));
                }
                return Some(match name.as_str() {
                    "getcount" | "find" => Kind::Num,
                    "getat" => Kind::Object,
                    "getarray" | "getactorarray" => Kind::Array,
                    _ => Kind::None,
                });
            }
        }
        // The per-family natives (`natives/*.rs`); each claims only its own
        // method names, so the order they are tried in is moot.
        let families = [
            Self::check_alias,
            Self::check_quest,
            Self::check_form_list,
            Self::check_favor,
            Self::check_actor,
            Self::check_ownership_crime,
            Self::check_reference,
            Self::check_linked_refs,
            Self::check_location,
            Self::check_forms,
            Self::check_animation,
            Self::check_combat,
            Self::check_destruction,
            Self::check_scanning,
        ];
        for check in families {
            if let Some(kind) = check(self, receiver_kind, &name, &arg_kinds, statement) {
                return Some(kind);
            }
        }
        // Another quest's update registration: its handler must run.
        if receiver_kind == Kind::Quest {
            if crate::fragment::EventRequest::is_call(&name) {
                let Some(script) = self.declared_script(frame, receiver) else {
                    return self.fail(|| format!("{name} on a quest of unknown script"));
                };
                return self.check_event_registration(&script, &name, &arg_kinds, statement, Kind::Quest);
            }
            if let Some(handler) = registration_handler(&name) {
                let interval_ok = update_args_ok(&name, &arg_kinds);
                if !statement || !interval_ok {
                    return self.fail(|| format!("{name} as written"));
                }
                if !handler.is_empty() {
                    let Some(script) = self.declared_script(frame, receiver) else {
                        return self.fail(|| format!("{name} on a quest of unknown script"));
                    };
                    if !self.versions(&script, handler).is_empty() {
                        self.check_handler(&script, handler, Kind::Quest)?;
                    }
                }
                return Some(Kind::None);
            }
        }
        // A function of a script on another reference, through a cast or a
        // typed variable: it runs as that reference's.
        if receiver_kind == Kind::Object {
            if let Some(script) = self.declared_script(frame, receiver) {
                if !is_engine_class(&script) && !self.versions(&script, &name).is_empty() {
                    // A script on an alias (`Alias_X as MyAliasScript`) runs with an
                    // alias as `Self`, so its `GetActorRef()` and the like read.
                    let self_kind = if self.script_extends_alias(&script) { Kind::Alias } else { Kind::Object };
                    return self.check_call_into(&script, &name, statement, self_kind);
                }
            }
        }
        // A quest script's function, through a cast or a typed property.
        if receiver_kind == Kind::Quest {
            if let Some(script) = self.quest_script_for(frame, receiver, &name) {
                return self.check_call_into(&script, &name, statement, Kind::Quest);
            }
        }
        let Some(placeholder) = spanned_placeholder(receiver_kind) else {
            return self.fail(|| format!("{name} on {receiver_kind:?}"));
        };
        let result = self.check_native(placeholder, &name, &arg_kinds, statement);
        if result.is_none() {
            return self.fail(|| {
                let shapes: Vec<String> = arg_kinds
                    .iter()
                    .map(|arg| match arg {
                        Arg::Literal(_) => "lit".to_string(),
                        Arg::Kind(kind) => format!("{kind:?}"),
                        Arg::Constant(_) => "const".to_string(),
                    })
                    .collect();
                format!("native {name}/{} on {receiver_kind:?} ({})", arg_kinds.len(), shapes.join(","))
            });
        }
        result
    }

    /// The script a quest-typed receiver's `name` lives in: a cast's target
    /// type, or the declared type of a property / local.
    pub(super) fn quest_script_for(&mut self, frame: &CheckFrame, receiver: &Expr, name: &str) -> Option<Arc<Script>> {
        let script = self.declared_script(frame, receiver)?;
        if self.versions(&script, name).is_empty() {
            return None;
        }
        Some(script)
    }

    /// The script a receiver is declared as — a cast's target type, or a
    /// property's / local's type — registered for the run.
    /// Whether `script` extends an alias class through its chain.
    pub(super) fn script_extends_alias(&mut self, script: &Arc<Script>) -> bool {
        let mut parent = script.parent.as_ref().map(|p| p.node.0.clone());
        for _ in 0..16 {
            let Some(name) = parent else {
                return false;
            };
            let lower = name.to_ascii_lowercase();
            if lower.contains("alias") {
                return true;
            }
            parent = self
                .library
                .and_then(|library| library.script(&lower))
                .and_then(|script| script.parent.as_ref().map(|p| p.node.0.clone()));
        }
        false
    }

    pub(super) fn declared_script(&mut self, frame: &CheckFrame, receiver: &Expr) -> Option<Arc<Script>> {
        let type_name = match receiver {
            Expr::Cast { target_type, .. } => match &target_type.node {
                Type::Object(ty) => ty.0.clone(),
                _ => return None,
            },
            Expr::Ident(id) if id.0.eq_ignore_ascii_case("self") => frame.script.name.node.0.clone(),
            Expr::Ident(id) => match frame
                .types
                .get(&id.0.to_ascii_lowercase())
                .cloned()
                .or_else(|| self.member_type(&frame.script, &id.0))
            {
                Some(Type::Object(ty)) => ty.0.clone(),
                _ => return None,
            },
            // `Owner.Member`: the script type `Member` is declared as in
            // `Owner`'s script (`(GetOwningQuest() as REScript).REParent`).
            Expr::MemberAccess { object, member } => {
                let owner = self.declared_script(frame, &object.node)?;
                match self.member_type(&owner, &member.node.0)? {
                    Type::Object(ty) => ty.0,
                    _ => return None,
                }
            }
            _ => return None,
        };
        let script = self.library?.script(&type_name)?;
        self.scripts
            .insert(script.name.node.0.to_ascii_lowercase(), Arc::clone(&script));
        Some(script)
    }

    /// Whether the primitives take this native call, over placeholders.
    pub(super) fn check_native(
        &mut self,
        receiver: Expr,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        let mut bindings = NativeBindings::default();
        let name_is_help_message = name.eq_ignore_ascii_case("showashelpmessage");
        let placeholder = |kind: Kind, index: usize, bindings: &mut NativeBindings| -> Option<Spanned<Expr>> {
            let name = format!("__vm{index}");
            Some(match kind {
                Kind::Num => spanned(Expr::IntLit(1)),
                Kind::Bool => spanned(Expr::BoolLit(false)),
                // `ShowAsHelpMessage`'s context is shown only when empty or
                // `"NoMenu"`; a run-time one is checked as the unrestricted
                // `""` (a menu name stops the run when it arrives).
                Kind::Str if name_is_help_message && index == 5 => spanned(Expr::StringLit(String::new())),
                Kind::Str => spanned(Expr::StringLit("x".into())),
                Kind::None => spanned(Expr::NoneLit),
                Kind::Player => {
                    bindings.players.insert(name.clone());
                    ident(&name)
                }
                Kind::Quest => {
                    bindings.quests.insert(name.clone());
                    ident(&name)
                }
                Kind::Object | Kind::Alias => ident(&name),
                Kind::Array | Kind::Struct => return None,
            })
        };
        let receiver = match receiver {
            Expr::Ident(id) if id.0.starts_with("__vm_kind_") => {
                let kind = match id.0.as_str() {
                    "__vm_kind_quest" => Kind::Quest,
                    "__vm_kind_player" => Kind::Player,
                    _ => Kind::Object,
                };
                placeholder(kind, 0, &mut bindings)?
            }
            other => spanned(other),
        };
        let mut args = Vec::with_capacity(arg_kinds.len());
        for (index, arg) in arg_kinds.iter().enumerate() {
            args.push(CallArg {
                name: None,
                value: match arg {
                    // Skyrim's raw motion type is checked as the constant it
                    // names; the run maps it by the game (`synthesize`).
                    Arg::Literal(Expr::IntLit(raw))
                        if index == 0 && name.eq_ignore_ascii_case("setmotiontype") && skyrim_motion_constant(*raw as i32).is_some() =>
                    {
                        spanned(Expr::MemberAccess {
                            object: Box::new(receiver.clone()),
                            member: spanned(Identifier(skyrim_motion_constant(*raw as i32).unwrap_or_default().to_owned())),
                        })
                    }
                    Arg::Literal(literal) => spanned(literal.clone()),
                    Arg::Kind(kind) => placeholder(*kind, index + 1, &mut bindings)?,
                    Arg::Constant(name) => spanned(Expr::MemberAccess {
                        object: Box::new(receiver.clone()),
                        member: spanned(Identifier(name.clone())),
                    }),
                },
            });
        }
        let call = Expr::Call {
            callee: Box::new(spanned(Expr::MemberAccess {
                object: Box::new(receiver),
                member: spanned(Identifier(name.to_owned())),
            })),
            args,
        };
        if statement {
            if let Some(effect) = classify_native_effect(&call, &bindings) {
                return (!crate::translate::effects::is_latent(&effect)).then_some(Kind::None);
            }
        }
        if classify_native_value(&call, &bindings).is_some() {
            return Some(Kind::Num);
        }
        if classify_native_guard(&call, &bindings).is_some() {
            return Some(Kind::Bool);
        }
        None
    }
}

/// A native call's argument as the check sees it.
pub(super) enum Arg {
    Literal(Expr),
    Kind(Kind),
    /// An engine constant, passed as the receiver's named member.
    Constant(String),
}

/// The name an argument reads an engine constant by, if it is written as
/// one: `X.Motion_Keyframed` or a bare `Motion_Keyframed`.
pub(super) fn constant_name(e: &Expr) -> Option<&str> {
    match e {
        Expr::MemberAccess { member, .. } => Some(member.node.0.as_str()),
        Expr::Ident(name) if !name.0.eq_ignore_ascii_case("self") => Some(name.0.as_str()),
        _ => None,
    }
}

/// `e` as a literal (through casts and negation), if it is one.
pub(super) fn literal_of(e: &Expr) -> Option<Expr> {
    match e {
        Expr::IntLit(_) | Expr::FloatLit(_) | Expr::BoolLit(_) | Expr::StringLit(_) | Expr::NoneLit => {
            Some(e.clone())
        }
        Expr::Cast { expr, target_type } => match (literal_of(&expr.node)?, &target_type.node) {
            (Expr::IntLit(i), Type::Float) => Some(Expr::FloatLit(i as f64)),
            (Expr::FloatLit(f), Type::Int) => Some(Expr::IntLit(f as i64)),
            (literal, Type::Int | Type::Float | Type::Bool | Type::String) => Some(literal),
            _ => None,
        },
        Expr::UnaryOp { op: UnaryOp::Neg, operand } => match literal_of(&operand.node)? {
            Expr::IntLit(i) => Some(Expr::IntLit(-i)),
            Expr::FloatLit(f) => Some(Expr::FloatLit(-f)),
            _ => None,
        },
        _ => None,
    }
}

/// A receiver placeholder the native check binds by kind.
pub(super) fn spanned_placeholder(kind: Kind) -> Option<Expr> {
    let name = match kind {
        Kind::Quest => "__vm_kind_quest",
        Kind::Player => "__vm_kind_player",
        Kind::Object | Kind::Alias => "__vm_kind_object",
        _ => return None,
    };
    Some(Expr::Ident(Identifier(name.to_owned())))
}
