//! The interpreter: a run, its frames, statements, expressions and
//! assignment, script variables, and the call dispatch (#5800).

use super::*;

/// The run's mutable context: the fragment runtime's own state.
pub(super) struct Vm<'a> {
    pub(super) world: &'a World,
    pub(super) context: QuestFormId,
    pub(super) stages: &'a mut QuestStageState,
    pub(super) objectives: &'a mut QuestObjectiveState,
    pub(super) deferred: &'a mut DeferredFragmentEffects,
    pub(super) steps: usize,
    pub(super) depth: usize,
    pub(super) advances: Vec<QuestStageAdvanced>,
    /// The frame budget this run may spend (statements), the statements it
    /// has spent of it, and whether it suspended because that ran out.
    pub(super) allowance: usize,
    pub(super) spent: usize,
    pub(super) yielded: bool,
    /// Calls for a value in progress, and calling frames holding an array or
    /// struct local: a run cannot suspend while either is on the stack.
    pub(super) value_calls: usize,
    pub(super) blocked_callers: usize,
    /// Whether the native call being resolved is a statement (its result is
    /// unused): a call for effect on a `None` object is ignored, as the engine does.
    pub(super) for_effect: bool,
    /// The arrays and structs script variables hold that this run has read
    /// or written, by (owner, variable). They are reference values: every
    /// read in the run shares one, so a mutation is seen by the next read, and
    /// the run writes them back whole when it ends.
    pub(super) aggregates: std::cell::RefCell<HashMap<(u32, ConditionStringId), Value>>,
    /// The members of the script-filled reference collections this run has
    /// read or changed, written back when it ends.
    pub(super) collections: std::cell::RefCell<HashMap<crate::alias_collections::CollectionKey, Vec<u32>>>,
}

/// Run `script`'s `function` attached to `context` — a quest's fragment, or
/// a reference script's event with its `args` — its properties bound by
/// `vmad`, from the start or from where `resume` says a `Utility.Wait`
/// suspended it. Returns the quest-stage advances its natives produced, and
/// the wait and resume point when it suspends again.
#[allow(clippy::too_many_arguments)]
pub(in crate::fragment) fn run_fragment(
    script: &str,
    function: &str,
    args: &[VmStored],
    resume: Option<&VmResume>,
    context: QuestFormId,
    vmad: Option<&ScriptInstanceData>,
    world: &World,
    stages: &mut QuestStageState,
    objectives: &mut QuestObjectiveState,
    deferred: &mut DeferredFragmentEffects,
) -> (Vec<QuestStageAdvanced>, Option<(f32, VmResume)>) {
    let Some(script) = deferred.vm_scripts.get(&script.to_ascii_lowercase()).cloned() else {
        log::warn!("Papyrus VM: script '{script}' is not installed");
        return (Vec::new(), None);
    };
    let mut vm = Vm {
        world,
        context,
        stages,
        objectives,
        deferred,
        // A run the frame budget suspended counts its earlier statements
        // against the cap; one a wait suspended starts over.
        steps: resume.map_or(0, |state| state.carried_steps as usize),
        depth: 0,
        advances: Vec::new(),
        allowance: world
            .try_resource::<crate::fragment::VmFrameBudget>()
            .map_or(usize::MAX, |budget| budget.remaining()),
        spent: crate::fragment::RUN_START_COST,
        yielded: false,
        value_calls: 0,
        blocked_callers: 0,
        for_effect: false,
        aggregates: Default::default(),
        collections: Default::default(),
    };
    let frame = Frame {
        script: Arc::clone(&script),
        owner: context,
        vmad: vmad.cloned(),
        locals: HashMap::new(),
        defined_in: None,
    };
    let run = match resume.and_then(|state| state.frames.split_first()) {
        Some((top, inner)) => vm.resume_frame(frame, top, inner),
        None => match vm.resolve(&script, context, function) {
            // An event the script does not handle in its current state:
            // nothing runs.
            None if crate::fragment::reference::is_delivered_event(function) => Ok(Flow::Normal),
            None => stop(format!("no function {function}")),
            Some(code) => {
                let mut args: Vec<Value> = args.iter().map(restore).collect();
                // A topic-info fragment's speaker parameter is the line's
                // speaker, as the lowering binds it.
                let speaker_parameter = code.callable(function).is_ok_and(|callable| {
                    callable.params.first().is_some_and(|param| {
                        param
                            .name
                            .node
                            .0
                            .eq_ignore_ascii_case(crate::translate::effects::INFO_SPEAKER_PARAMETER)
                    })
                });
                if args.is_empty() && speaker_parameter {
                    let speaker = vm.deferred.speaker.and_then(|entity| vm.entity_form(entity));
                    args.push(speaker.map_or(Value::None, Value::Form));
                }
                vm.enter(frame, &code, function, args).and_then(|mut frame| {
                    let flow = vm.run_frame(&mut frame, &code, function, None, &[])?;
                    vm.callee_flow(flow, &frame, &code, function)
                })
            }
        },
    }
    .map(|flow| match flow {
        Flow::Suspend(seconds, _, frames) => {
            let carried_steps = if vm.yielded { vm.steps as u32 } else { 0 };
            Some((seconds, VmResume { frames, carried_steps }))
        }
        _ => None,
    });
    let suspended = match run {
        Ok(suspended) => suspended,
        Err(Stop(why)) => {
            log::warn!(
                "Papyrus VM: {}.{function} on {:08X} stopped: {why}",
                script.name.node.0,
                context.0
            );
            None
        }
    };
    // The arrays and structs the run touched go back to the saved store.
    vm.flush_aggregates();
    for (key, members) in std::mem::take(&mut *vm.collections.borrow_mut()) {
        vm.deferred.alias_collections.push((key, members));
    }
    // Settle with the frame's budget: what this run spent, and whether it was
    // cut short by it.
    if let Some(mut budget) = world.try_resource_mut::<crate::fragment::VmFrameBudget>() {
        budget.charge(vm.spent);
        if vm.yielded {
            budget.note_yielded();
        }
    }
    (vm.advances, suspended)
}

impl Vm<'_> {
    pub(super) fn is_quest_form(&self, value: &Value) -> bool {
        matches!(value, Value::Form(form) if self.is_quest(*form))
    }

    pub(super) fn is_quest(&self, form: u32) -> bool {
        let quest = QuestFormId(form);
        self.deferred.quest_scripts.contains_key(&quest) || self.stages.is_started(quest)
    }

    pub(super) fn tick(&mut self) -> Run<()> {
        self.steps += 1;
        self.spent += 1;
        if self.steps > MAX_STEPS {
            return stop("step limit");
        }
        Ok(())
    }

    /// The run has used its frame allowance and may stop here, between two
    /// statements: nothing on the stack needs a value back or holds a local
    /// the saved continuation cannot store.
    pub(super) fn should_yield(&self, frame: &Frame) -> bool {
        self.spent >= self.allowance
            && self.value_calls == 0
            && self.blocked_callers == 0
            && locals_storable(frame)
    }

    /// The body a call of `name` on `owner`'s instance of `script` runs, in
    /// the instance's current state.
    pub(super) fn resolve(&self, script: &Arc<Script>, owner: QuestFormId, name: &str) -> Option<Code> {
        let state = self.script_state(owner, script);
        resolve_in(script, &state, name, &self.deferred.vm_scripts)
    }

    /// `GotoState(state)` for a script without the compiler's generated
    /// one: `OnEndState` in the old state, the switch, `OnBeginState` in the
    /// new one (CK *OnBeginState*: also when re-entering the same state).
    pub(super) fn goto_state(&mut self, frame: &Frame, state: String) -> Run<()> {
        for (event, switch) in [("OnEndState", false), ("OnBeginState", true)] {
            if switch {
                let key = state_key(&frame.script);
                self.write_object(frame.owner, key, &Value::Str(state.clone()))?;
            }
            if let Some(code) = self.resolve(&frame.script, frame.owner, event) {
                let handler = Frame {
                    script: Arc::clone(&frame.script),
                    owner: frame.owner,
                    vmad: frame.vmad.clone(),
                    locals: HashMap::new(),
                    defined_in: None,
                };
                self.call_function(handler, &code, event, Vec::new())?;
            }
        }
        Ok(())
    }

    /// `owner`'s instance of `script`'s state: the one `GotoState` last set,
    /// else the auto state.
    pub(super) fn script_state(&self, owner: QuestFormId, script: &Arc<Script>) -> String {
        match self.written_object(owner, state_key(script)) {
            Some(Value::Str(state)) => state,
            _ => auto_state(script, &self.deferred.vm_scripts),
        }
    }

    /// Run `function`'s body `code` in `frame` — from the start, or along
    /// `path`, with `inner` the suspended frames of the call the path ends in.
    pub(super) fn run_frame(
        &mut self,
        frame: &mut Frame,
        code: &Code,
        function: &str,
        path: Option<&[VmStep]>,
        inner: &[VmFrame],
    ) -> Run<Flow> {
        if self.depth >= MAX_DEPTH {
            return stop("call depth");
        }
        let callable = code.callable(function)?;
        frame.defined_in = Some(Arc::clone(&code.script));
        self.depth += 1;
        let flow = self.exec_block(frame, callable.body, path, inner, true);
        self.depth -= 1;
        flow
    }

    /// `frame` with the parameters of `name` (in `code`) bound to `args`.
    pub(super) fn enter(&mut self, mut frame: Frame, code: &Code, name: &str, args: Vec<Value>) -> Run<Frame> {
        let callable = code.callable(name)?;
        let mut args = args.into_iter();
        for param in callable.params {
            let value = match args.next() {
                Some(value) => value,
                None => match &param.default {
                    Some(default) => self.eval(&mut frame, &default.node)?,
                    None => Value::default_for(&param.ty.node),
                },
            };
            frame
                .locals
                .insert(param.name.node.0.to_ascii_lowercase(), value);
        }
        Ok(frame)
    }

    /// A function called for its value; it may not wait.
    pub(super) fn call_function(&mut self, frame: Frame, code: &Code, name: &str, args: Vec<Value>) -> Run<Value> {
        let mut frame = self.enter(frame, code, name, args)?;
        self.value_calls += 1;
        let flow = self.run_frame(&mut frame, code, name, None, &[]);
        self.value_calls -= 1;
        Ok(match flow? {
            Flow::Return(value) => value,
            Flow::Normal => Value::None,
            Flow::Suspend(..) => return stop("a wait in a function called for its value"),
        })
    }

    /// A called function's flow as its call statement's: a suspension adds
    /// the callee's frame to the chain.
    pub(super) fn callee_flow(&mut self, flow: Flow, frame: &Frame, code: &Code, function: &str) -> Run<Flow> {
        Ok(match flow {
            Flow::Suspend(seconds, path, deeper) => {
                let mut frames = vec![frame_state(frame, code, function, path)?];
                frames.extend(deeper);
                Flow::Suspend(seconds, Vec::new(), frames)
            }
            _ => Flow::Normal,
        })
    }

    /// Continue the suspended frame `state` as `frame` (its script, owner
    /// and properties), with `inner` the frames of the call it waits in.
    pub(super) fn resume_frame(&mut self, mut frame: Frame, state: &VmFrame, inner: &[VmFrame]) -> Run<Flow> {
        let Some(defined_in) = self
            .deferred
            .vm_scripts
            .get(&state.defined_in.to_ascii_lowercase())
            .cloned()
        else {
            return stop(format!("script {} is not installed", state.defined_in));
        };
        let code = Code {
            script: defined_in,
            state: state.state.clone(),
        };
        frame.locals = restore_locals(state);
        let flow = self.run_frame(&mut frame, &code, &state.function, Some(&state.path), inner)?;
        self.callee_flow(flow, &frame, &code, &state.function)
    }

    /// Continue the suspended callee chain `inner` (outermost first) — none
    /// when the path ended at the wait itself — called from `caller`.
    pub(super) fn resume_callee(&mut self, caller: &Frame, inner: &[VmFrame]) -> Run<Flow> {
        let Some((callee, deeper)) = inner.split_first() else {
            return Ok(Flow::Normal);
        };
        let Some(script) = self
            .deferred
            .vm_scripts
            .get(&callee.script.to_ascii_lowercase())
            .cloned()
        else {
            return stop(format!("script {} is not installed", callee.script));
        };
        let owner = QuestFormId(callee.owner);
        // The properties the call bound (`script_call_target`): the caller's
        // own for a call into its own script, else the quest's.
        let vmad = if owner == caller.owner && Arc::ptr_eq(&script, &caller.script) {
            caller.vmad.clone()
        } else if let Some(vmad) = self.deferred.quest_scripts.get(&owner) {
            Some(vmad.clone())
        } else {
            // A script on another reference: its own properties.
            self.reference_scripts(owner.0)
                .into_iter()
                .find(|(attached, _)| Arc::ptr_eq(attached, &script))
                .map(|(_, vmad)| (*vmad).clone())
        };
        let frame = Frame {
            script,
            owner,
            vmad,
            locals: HashMap::new(),
            defined_in: None,
        };
        self.resume_frame(frame, callee, deeper)
    }

    /// Run `body`, or — `resume` — continue it from the position a wait
    /// suspended it at.
    pub(super) fn exec_block(
        &mut self,
        frame: &mut Frame,
        body: &[Spanned<Stmt>],
        resume: Option<&[VmStep]>,
        inner: &[VmFrame],
        // `body` is a function's own body, not a branch inside one: nothing of
        // this function follows its last statement.
        ends_function: bool,
    ) -> Run<Flow> {
        let mut start = 0;
        if let Some(path) = resume {
            let Some((VmStep::Statement(index), rest)) = path.split_first() else {
                return stop("a malformed resume path");
            };
            let index = *index as usize;
            let Some(statement) = body.get(index) else {
                return stop("a resume path past its block");
            };
            // The path ends at the wait itself or at the call it waits in.
            let flow = if rest.is_empty() {
                self.resume_callee(frame, inner)?
            } else {
                self.resume_statement(frame, &statement.node, rest, inner)?
            };
            match flow {
                Flow::Normal => {}
                flow => return Ok(suspend_at(VmStep::Statement(index as u32), flow)),
            }
            start = index + 1;
        }
        for (index, statement) in body.iter().enumerate().skip(start) {
            match self.exec(frame, &statement.node)? {
                Flow::Normal => {
                    // The frame budget ran out: suspend after this statement,
                    // as a zero-second `Utility.Wait` here would.
                    if !(ends_function && index + 1 == body.len()) && self.should_yield(frame) {
                        self.yielded = true;
                        return Ok(suspend_at(
                            VmStep::Statement(index as u32),
                            Flow::Suspend(0.0, Vec::new(), Vec::new()),
                        ));
                    }
                }
                flow => return Ok(suspend_at(VmStep::Statement(index as u32), flow)),
            }
        }
        Ok(Flow::Normal)
    }

    /// Continue inside the `If` branch or `While` body a wait suspended.
    pub(super) fn resume_statement(
        &mut self,
        frame: &mut Frame,
        statement: &Stmt,
        path: &[VmStep],
        inner: &[VmFrame],
    ) -> Run<Flow> {
        let Some((VmStep::Branch(branch), rest)) = path.split_first() else {
            return stop("a malformed resume path");
        };
        match statement {
            Stmt::If {
                body,
                elseif_clauses,
                else_body,
                ..
            } => {
                let block = match *branch {
                    0 => body.as_slice(),
                    u32::MAX => match else_body {
                        Some(body) => body.as_slice(),
                        None => return stop("a resume path into a missing Else"),
                    },
                    k => match elseif_clauses.get(k as usize - 1) {
                        Some((_, body)) => body.as_slice(),
                        None => return stop("a resume path into a missing ElseIf"),
                    },
                };
                let flow = self.exec_block(frame, block, Some(rest), inner, false)?;
                Ok(suspend_at(VmStep::Branch(*branch), flow))
            }
            Stmt::While { condition, body } => {
                match self.exec_block(frame, body, Some(rest), inner, false)? {
                    Flow::Normal => self.exec_while(frame, &condition.node, body),
                    flow => Ok(suspend_at(VmStep::Branch(0), flow)),
                }
            }
            _ => stop("a resume path into a statement without a block"),
        }
    }

    pub(super) fn exec_while(&mut self, frame: &mut Frame, condition: &Expr, body: &[Spanned<Stmt>]) -> Run<Flow> {
        while self.eval(frame, condition)?.truthy() {
            self.tick()?;
            match self.exec_block(frame, body, None, &[], false)? {
                Flow::Normal => {}
                flow => return Ok(suspend_at(VmStep::Branch(0), flow)),
            }
        }
        Ok(Flow::Normal)
    }

    pub(super) fn exec(&mut self, frame: &mut Frame, statement: &Stmt) -> Run<Flow> {
        self.tick()?;
        match statement {
            Stmt::VarDecl(variable) => {
                let value = match &variable.initial_value {
                    Some(init) => self.eval(frame, &init.node)?,
                    None => Value::default_for(&variable.ty.node),
                };
                frame
                    .locals
                    .insert(variable.name.node.0.to_ascii_lowercase(), value);
            }
            Stmt::Assign { target, op, value } => {
                let value = self.eval(frame, &value.node)?;
                let value = match op {
                    AssignOp::Eq => value,
                    op => {
                        let current = self.eval(frame, &target.node)?;
                        let binary = match op {
                            AssignOp::PlusEq => BinaryOp::Add,
                            AssignOp::MinusEq => BinaryOp::Sub,
                            AssignOp::MulEq => BinaryOp::Mul,
                            AssignOp::DivEq => BinaryOp::Div,
                            AssignOp::ModEq => BinaryOp::Mod,
                            AssignOp::Eq => unreachable!(),
                        };
                        arithmetic(binary, &current, &value)?
                    }
                };
                self.assign(frame, &target.node, value)?;
            }
            Stmt::Return(value) => {
                let value = match value {
                    Some(value) => self.eval(frame, &value.node)?,
                    None => Value::None,
                };
                return Ok(Flow::Return(value));
            }
            Stmt::If {
                condition,
                body,
                elseif_clauses,
                else_body,
            } => {
                if self.eval(frame, &condition.node)?.truthy() {
                    let flow = self.exec_block(frame, body, None, &[], false)?;
                    return Ok(suspend_at(VmStep::Branch(0), flow));
                }
                for (k, (condition, body)) in elseif_clauses.iter().enumerate() {
                    if self.eval(frame, &condition.node)?.truthy() {
                        let flow = self.exec_block(frame, body, None, &[], false)?;
                        return Ok(suspend_at(VmStep::Branch(k as u32 + 1), flow));
                    }
                }
                if let Some(body) = else_body {
                    let flow = self.exec_block(frame, body, None, &[], false)?;
                    return Ok(suspend_at(VmStep::Branch(u32::MAX), flow));
                }
            }
            Stmt::While { condition, body } => {
                return self.exec_while(frame, &condition.node, body);
            }
            Stmt::ExprStmt(expression) => {
                return self.call_statement(frame, &expression.node);
            }
        }
        Ok(Flow::Normal)
    }

    pub(super) fn assign(&mut self, frame: &mut Frame, target: &Expr, value: Value) -> Run<()> {
        match target {
            Expr::Ident(name) => {
                let key = name.0.to_ascii_lowercase();
                if let Some(slot) = frame.locals.get_mut(&key) {
                    *slot = value;
                    return Ok(());
                }
                if is_state_variable(&name.0) {
                    let key = state_key(&frame.script);
                    return self.write_object(frame.owner, key, &value);
                }
                // A script variable or property of the frame's quest script.
                self.write_quest_variable(frame.owner, &name.0, &value)
            }
            Expr::MemberAccess { object, member } => {
                let owner = self.eval(frame, &object.node)?;
                if let Value::Struct(members) = &owner {
                    let mut members = members.borrow_mut();
                    return match Value::member(&members, &member.node.0) {
                        Some(index) => {
                            members[index].1 = value;
                            Ok(())
                        }
                        None => stop(format!("struct member {}", member.node.0)),
                    };
                }
                if member.node.0.eq_ignore_ascii_case("value") && matches!(owner, Value::Form(_)) && !self.is_quest_form(&owner) {
                    let Some(number) = value.number() else {
                        return stop("a non-number GlobalVariable.Value");
                    };
                    let (callee, args) = set_value_call(object, &spanned(Expr::FloatLit(f64::from(number))));
                    return self.call_statement(frame, &Expr::Call { callee, args }).map(|_| ());
                }
                match owner {
                    Value::Form(quest) if self.is_quest(quest) => {
                        self.write_quest_variable(QuestFormId(quest), &member.node.0, &value)
                    }
                    // A variable of a script on another reference, kept
                    // under that reference.
                    Value::Form(reference) if reference != 0 && reference != PLAYER_REF => {
                        self.write_quest_variable(QuestFormId(reference), &member.node.0, &value)
                    }
                    _ => stop(format!("assignment to {}'s {}", owner.text(), member.node.0)),
                }
            }
            Expr::Index { object, index } => {
                let array = self.eval(frame, &object.node)?;
                let index = self.eval(frame, &index.node)?.number().unwrap_or(-1.0) as i64;
                let Value::Array(array) = array else {
                    return stop("index assignment to a non-array");
                };
                let mut items = array.borrow_mut();
                let Some(slot) = usize::try_from(index).ok().and_then(|i| items.get_mut(i)) else {
                    return stop("array index out of range");
                };
                *slot = value;
                Ok(())
            }
            _ => stop("unsupported assignment target"),
        }
    }

    /// A quest script variable: a number in the store `GetVMQuestVariable`
    /// and the lowered `SetQuestVariable` share, an object in
    /// [`crate::QuestScriptObjects`].
    pub(super) fn write_quest_variable(&mut self, quest: QuestFormId, name: &str, value: &Value) -> Run<()> {
        let key = crate::QuestScriptVariables::property_key(name);
        if let Some(number) = value.number() {
            self.deferred.quest_variables.push((quest, key, number));
            return Ok(());
        }
        self.write_object(quest, key, value)
    }

    /// A non-numeric script variable write under `key`.
    pub(super) fn write_object(&mut self, owner: QuestFormId, key: ConditionStringId, value: &Value) -> Run<()> {
        // An array or struct is a reference value kept for the run and
        // written back at its end.
        if matches!(value, Value::Array(_) | Value::Struct(_)) {
            if to_stored(value).is_none() {
                return stop("an array or struct that cannot be stored in a script variable");
            }
            self.aggregates.borrow_mut().insert((owner.0, key), value.clone());
            return Ok(());
        }
        let Some(object) = to_stored(value) else {
            return stop(format!("a {} write to a script variable", value.text()));
        };
        // A scalar replaces whatever aggregate the run held there.
        self.aggregates.borrow_mut().remove(&(owner.0, key));
        self.deferred.quest_objects.push((owner, key, object));
        Ok(())
    }

    /// Write every array and struct the run touched back as a stored object.
    pub(super) fn flush_aggregates(&mut self) {
        let aggregates = std::mem::take(&mut *self.aggregates.borrow_mut());
        for ((owner, key), value) in aggregates {
            if let Some(object) = to_stored(&value) {
                self.deferred.quest_objects.push((QuestFormId(owner), key, object));
            }
        }
    }

    /// An object or string a script wrote to `quest`'s variable `name`, if
    /// any.
    pub(super) fn written_quest_object(&self, quest: QuestFormId, name: &str) -> Option<Value> {
        self.written_object(quest, crate::QuestScriptVariables::property_key(name))
    }

    pub(super) fn written_object(&self, owner: QuestFormId, key: ConditionStringId) -> Option<Value> {
        if let Some(held) = self.aggregates.borrow().get(&(owner.0, key)) {
            return Some(held.clone());
        }
        let pending = self
            .deferred
            .quest_objects
            .iter()
            .rev()
            .find(|(q, k, _)| *q == owner && *k == key)
            .map(|(_, _, value)| value.clone());
        let stored = pending.or_else(|| {
            self.world
                .try_resource::<crate::QuestScriptObjects>()
                .and_then(|objects| objects.get(owner, key))
        });
        let value = from_stored(&stored?);
        // A stored array or struct becomes the run's one shared value.
        if matches!(value, Value::Array(_) | Value::Struct(_)) {
            self.aggregates.borrow_mut().insert((owner.0, key), value.clone());
        }
        Some(value)
    }

    pub(super) fn read_quest_variable(&self, quest: QuestFormId, name: &str) -> Option<f32> {
        let key = crate::QuestScriptVariables::property_key(name);
        self.deferred
            .quest_variables
            .iter()
            .rev()
            .find(|(q, k, _)| *q == quest && *k == key)
            .map(|(_, _, value)| *value)
            .or_else(|| crate::quest_variable(self.world, quest, key))
    }

    pub(super) fn property(vmad: Option<&ScriptInstanceData>, name: &str) -> Option<Value> {
        let bare = vmad_property_name(name);
        vmad?
            .scripts
            .iter()
            .find_map(|script| script.property(bare))
            .map(|property| Value::from_property(&property.value))
    }

    pub(super) fn eval(&mut self, frame: &mut Frame, e: &Expr) -> Run<Value> {
        Ok(match e {
            Expr::IntLit(i) => Value::Int(*i as i32),
            Expr::FloatLit(f) => Value::Float(*f as f32),
            Expr::BoolLit(b) => Value::Bool(*b),
            Expr::StringLit(s) => Value::Str(s.clone()),
            Expr::NoneLit => Value::None,
            Expr::Ident(name) => self.read_ident(frame, &name.0)?,
            Expr::Cast { expr, target_type } => {
                let value = self.eval(frame, &expr.node)?;
                cast(value, &target_type.node)
            }
            Expr::UnaryOp { op, operand } => {
                let value = self.eval(frame, &operand.node)?;
                match op {
                    UnaryOp::Not => Value::Bool(!value.truthy()),
                    UnaryOp::Neg => match value {
                        Value::Int(i) => Value::Int(-i),
                        Value::Float(f) => Value::Float(-f),
                        other => return stop(format!("negating {}", other.text())),
                    },
                }
            }
            Expr::BinaryOp { left, op, right } => match op {
                BinaryOp::And => {
                    let l = self.eval(frame, &left.node)?;
                    if !l.truthy() {
                        Value::Bool(false)
                    } else {
                        Value::Bool(self.eval(frame, &right.node)?.truthy())
                    }
                }
                BinaryOp::Or => {
                    let l = self.eval(frame, &left.node)?;
                    if l.truthy() {
                        Value::Bool(true)
                    } else {
                        Value::Bool(self.eval(frame, &right.node)?.truthy())
                    }
                }
                op => {
                    let l = self.eval(frame, &left.node)?;
                    let r = self.eval(frame, &right.node)?;
                    binary(*op, &l, &r)?
                }
            },
            Expr::Index { object, index } => {
                let array = self.eval(frame, &object.node)?;
                let index = self.eval(frame, &index.node)?.number().unwrap_or(-1.0) as i64;
                let Value::Array(array) = array else {
                    return stop("indexing a non-array");
                };
                let items = array.borrow();
                match usize::try_from(index).ok().and_then(|i| items.get(i)) {
                    Some(value) => value.clone(),
                    None => return stop("array index out of range"),
                }
            }
            Expr::NewStruct { ty } => {
                let name = match &ty.node {
                    Type::Object(name) => name.0.clone(),
                    _ => String::new(),
                };
                match self.new_struct(frame, &name) {
                    Some(value) => value,
                    None => return stop(format!("no struct {name}")),
                }
            }
            Expr::New { ty, size } => {
                let size = self.eval(frame, &size.node)?.number().unwrap_or(0.0).max(0.0) as usize;
                if size > MAX_ARRAY_ELEMENTS {
                    return stop(format!("array size {size} over {MAX_ARRAY_ELEMENTS}"));
                }
                Value::Array(Rc::new(RefCell::new(vec![Value::default_for(&ty.node); size])))
            }
            Expr::ArrayLit(items) => {
                if items.len() > MAX_ARRAY_ELEMENTS {
                    return stop(format!("array literal over {MAX_ARRAY_ELEMENTS} elements"));
                }
                let mut values = Vec::with_capacity(items.len());
                for item in items {
                    values.push(self.eval(frame, &item.node)?);
                }
                Value::Array(Rc::new(RefCell::new(values)))
            }
            Expr::MemberAccess { object, member } => {
                let owner = self.eval(frame, &object.node)?;
                let name = &member.node.0;
                if name.eq_ignore_ascii_case("length") {
                    if let Value::Array(array) = &owner {
                        return Ok(Value::Int(array.borrow().len() as i32));
                    }
                }
                if let Value::Struct(members) = &owner {
                    let members = members.borrow();
                    return match Value::member(&members, name) {
                        Some(index) => Ok(members[index].1.clone()),
                        None => stop(format!("struct member {name}")),
                    };
                }
                self.read_member(&owner, name)?
            }
            Expr::Call { callee, args } => self.call_value(frame, &callee.node, args)?,
            Expr::Is { .. } | Expr::ParentAccess => return stop("unsupported expression"),
        })
    }

    pub(super) fn read_ident(&mut self, frame: &mut Frame, name: &str) -> Run<Value> {
        let key = name.to_ascii_lowercase();
        if let Some(value) = frame.locals.get(&key) {
            return Ok(value.clone());
        }
        if key == "self" {
            return Ok(self.self_value(frame.owner));
        }
        if is_state_variable(name) {
            let script = Arc::clone(&frame.script);
            return Ok(Value::Str(self.script_state(frame.owner, &script)));
        }
        let script = Arc::clone(&frame.script);
        match self.read_script_member(frame.owner, &script, frame.vmad.as_ref(), name) {
            Some(value) => Ok(value),
            None => stop(format!("unknown name {name}")),
        }
    }

    /// What `Self` is for the instance `owner` owns: the alias an alias script
    /// is attached to, else the quest or reference itself.
    pub(super) fn self_value(&self, owner: QuestFormId) -> Value {
        match self.deferred.alias_owners.get(&owner.0) {
            Some(&(quest, alias)) => Value::Alias { quest, alias },
            None => Value::Form(owner.0),
        }
    }

    /// `owner`'s instance of `script` (its properties `vmad`): the property
    /// or script variable `name` — what a script wrote, else the VMAD's
    /// value, else what the script declares. `None` when the script has no
    /// such member.
    pub(super) fn read_script_member(
        &self,
        owner: QuestFormId,
        script: &Arc<Script>,
        vmad: Option<&ScriptInstanceData>,
        name: &str,
    ) -> Option<Value> {
        if let Some(value) = Self::property(vmad, name) {
            // A numeric property a script wrote reads the written value.
            if value.number().is_some() {
                if let Some(written) = self.read_quest_variable(owner, name) {
                    return Some(if value.is_float() {
                        Value::Float(written)
                    } else {
                        Value::Int(written as i32)
                    });
                }
            }
            // An object property a script re-pointed reads the new object; an
            // array or struct property is one shared value across the run,
            // starting from what a script last wrote or else the VMAD's.
            if matches!(value, Value::Array(_) | Value::Struct(_)) {
                return Some(self.aggregate(owner, name, value));
            }
            return Some(self.written_quest_object(owner, name).unwrap_or(value));
        }
        let declared = || {
            script_chain(script, &self.deferred.vm_scripts)
                .iter()
                .find_map(|script| declared_default(script, name))
        };
        Some(match member_type_in(script, name, &self.deferred.vm_scripts)? {
            ty @ (Type::Int | Type::Float | Type::Bool) => {
                let value = match self.read_quest_variable(owner, name) {
                    Some(value) => value,
                    None => declared().and_then(|value| value.number()).unwrap_or(0.0),
                };
                match ty {
                    Type::Int => Value::Int(value as i32),
                    Type::Bool => Value::Bool(value != 0.0),
                    _ => Value::Float(value),
                }
            }
            Type::String => self
                .written_quest_object(owner, name)
                .or_else(declared)
                .unwrap_or_else(|| Value::Str(String::new())),
            Type::Object(_) | Type::Var => self.written_quest_object(owner, name).unwrap_or(Value::None),
            // An array nothing has set yet is empty.
            Type::Array(_) => {
                let empty = Value::Array(Rc::new(RefCell::new(Vec::new())));
                self.aggregate(owner, name, empty)
            }
        })
    }

    /// The array or struct held by `owner`'s variable `name`: this run's
    /// shared value, else what a script last wrote, else `default`.
    pub(super) fn aggregate(&self, owner: QuestFormId, name: &str, default: Value) -> Value {
        let key = crate::QuestScriptVariables::property_key(name);
        if let Some(held) = self.written_object(owner, key) {
            if matches!(held, Value::Array(_) | Value::Struct(_)) {
                return held;
            }
        }
        self.aggregates.borrow_mut().insert((owner.0, key), default.clone());
        default
    }

    /// The scripts the VM runs on the reference `form`, with their
    /// properties: (script, its VMAD), loaded references only.
    pub(super) fn reference_scripts(&self, form: u32) -> Vec<(Arc<Script>, Arc<ScriptInstanceData>)> {
        let Some(entity) = crate::fragment::ReferenceScriptIndex::lookup(self.world, form) else {
            return Vec::new();
        };
        let Some(attached) = self.world.get::<crate::fragment::PapyrusReferenceScripts>(entity) else {
            return Vec::new();
        };
        attached
            .0
            .iter()
            .filter_map(|r| {
                let script = self.deferred.vm_scripts.get(&r.script.to_ascii_lowercase())?;
                Some((Arc::clone(script), Arc::clone(&r.vmad)))
            })
            .collect()
    }

    /// `quest.Variable` — another quest script's property or variable.
    pub(super) fn read_member(&mut self, owner: &Value, name: &str) -> Run<Value> {
        let Value::Form(form) = owner else {
            return stop(format!("member {name} of {}", owner.text()));
        };
        if !self.is_quest(*form) {
            if let Some(value) = self.engine_constant(name) {
                return Ok(value);
            }
            // A member of a script the VM runs on that reference.
            let owner = QuestFormId(*form);
            for (script, vmad) in self.reference_scripts(*form) {
                if let Some(value) = self.read_script_member(owner, &script, Some(&vmad), name) {
                    return Ok(value);
                }
            }
        }
        let quest = QuestFormId(*form);
        let vmad = self.deferred.quest_scripts.get(&quest).cloned();
        // A member of a script attached to the quest the VM has installed:
        // what a script wrote, the VMAD's value, or what it declares.
        if let Some(vmad) = &vmad {
            for attached in &vmad.scripts {
                let Some(script) = self.deferred.vm_scripts.get(&attached.name.to_ascii_lowercase()).cloned() else {
                    continue;
                };
                if let Some(value) = self.read_script_member(quest, &script, Some(vmad), name) {
                    return Ok(value);
                }
            }
        }
        if let Some(value) = Self::property(vmad.as_ref(), name) {
            return Ok(value);
        }
        // `GlobalVariable.Value`.
        if name.eq_ignore_ascii_case("value") {
            return Ok(self
                .world
                .try_resource::<crate::Globals>()
                .and_then(|globals| globals.get(*form))
                .map_or(Value::None, Value::Float));
        }
        match self.read_quest_variable(quest, name) {
            Some(value) => Ok(Value::Float(value)),
            None => stop(format!("member {name} of {form:08X}")),
        }
    }

    /// A new instance of the struct type `name` (`Script#Struct`, or a
    /// struct of the running script's chain), its members at their defaults.
    pub(super) fn new_struct(&self, frame: &Frame, name: &str) -> Option<Value> {
        let declaration = struct_declaration(&frame.script, name, &self.deferred.vm_scripts)?;
        let members = declaration
            .members
            .iter()
            .map(|member| {
                let value = member
                    .initial_value
                    .as_ref()
                    .and_then(|initial| literal_value(&initial.node))
                    .unwrap_or_else(|| Value::default_for(&member.ty.node));
                (member.name.node.0.clone(), value)
            })
            .collect();
        Some(Value::Struct(Rc::new(RefCell::new(members))))
    }

    /// An engine class's constant `name`, from the installed classes.
    pub(super) fn engine_constant(&self, name: &str) -> Option<Value> {
        CONSTANT_CLASSES.iter().find_map(|class| {
            let script = self.deferred.vm_scripts.get(*class)?;
            constant_property(script, name).and_then(property_constant)
        })
    }

    /// The quest script that defines `function` for the quest `quest`, if
    /// any, and the body it runs.
    pub(super) fn quest_script_with(&self, quest: QuestFormId, function: &str) -> Option<(Arc<Script>, Code)> {
        let vmad = self.deferred.quest_scripts.get(&quest)?;
        vmad.scripts.iter().find_map(|attached| {
            let script = self.deferred.vm_scripts.get(&attached.name.to_ascii_lowercase())?;
            let code = self.resolve(script, quest, function)?;
            Some((Arc::clone(script), code))
        })
    }

    /// The reference FormID a per-actor script state is keyed by.
    pub(super) fn actor_reference(&self, actor: u32) -> Option<u32> {
        let entity = self.entity_of(actor)?;
        if actor == PLAYER_REF {
            Some(PLAYER_REF)
        } else {
            self.entity_form(entity)
        }
    }

    pub(super) fn eval_args(&mut self, frame: &mut Frame, args: &[CallArg]) -> Run<Vec<Value>> {
        let mut values = Vec::with_capacity(args.len());
        for arg in args {
            values.push(self.eval(frame, &arg.value.node)?);
        }
        Ok(values)
    }

    /// A call to a script function, when `callee` names one: the frame's own
    /// script (`Fn()`, `Self.Fn()`) or a quest's attached script.
    pub(super) fn script_call(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Option<Value>> {
        let Some((callee_frame, code)) = self.script_call_target(frame, callee)? else {
            return Ok(None);
        };
        let values = self.eval_args(frame, args)?;
        self.call_function(callee_frame, &code, &callee_name(callee), values).map(Some)
    }

    /// The fresh frame a call to a script function runs in, and the body it
    /// runs, when `callee` names one.
    pub(super) fn script_call_target(&mut self, frame: &mut Frame, callee: &Expr) -> Run<Option<(Frame, Code)>> {
        let (receiver, name) = match callee {
            Expr::Ident(name) => (None, name.0.as_str()),
            Expr::MemberAccess { object, member } => (Some(&object.node), member.node.0.as_str()),
            _ => return Ok(None),
        };
        let own = |vm: &Self, frame: &Frame| {
            vm.resolve(&frame.script, frame.owner, name)
                .map(|code| (Arc::clone(&frame.script), frame.owner, frame.vmad.clone(), code))
        };
        let target = match receiver {
            None => own(self, frame),
            Some(object) if static_class(object).is_some() => None,
            // `Parent.Fn()`: the version of the script the running function is
            // defined in extends; an engine class's event (`Parent.OnInit()`)
            // has no body to run.
            Some(Expr::ParentAccess) => {
                let parent = frame
                    .defined_in
                    .as_ref()
                    .and_then(|defined| defined.parent.as_ref())
                    .and_then(|parent| self.deferred.vm_scripts.get(&parent.node.0.to_ascii_lowercase()).cloned());
                let state = self.script_state(frame.owner, &frame.script);
                parent
                    .and_then(|parent| resolve_in(&parent, &state, name, &self.deferred.vm_scripts))
                    .map(|code| (Arc::clone(&frame.script), frame.owner, frame.vmad.clone(), code))
            }
            // `ScriptName.Fn()`: a global function, which has no `Self`.
            Some(Expr::Ident(script_name)) if self.global_script(frame, &script_name.0).is_some() => {
                let script = self.global_script(frame, &script_name.0).expect("checked above");
                resolve_in(&script, "", name, &self.deferred.vm_scripts)
                    .map(|code| (script, QuestFormId(0), None, code))
            }
            Some(object) => match self.eval(frame, object)? {
                Value::Form(form) if form == frame.owner.0 && own(self, frame).is_some() => own(self, frame),
                // An alias script calling itself through `Self`.
                value @ Value::Alias { .. }
                    if self.self_value(frame.owner).same_alias(&value) && own(self, frame).is_some() =>
                {
                    own(self, frame)
                }
                Value::Form(form) if self.is_quest(form) => {
                    self.quest_script_with(QuestFormId(form), name).map(|(script, code)| {
                        let vmad = self.deferred.quest_scripts.get(&QuestFormId(form)).cloned();
                        (script, QuestFormId(form), vmad, code)
                    })
                }
                // A function of a script the VM runs on another reference.
                Value::Form(form) if form != 0 => self.reference_scripts(form).into_iter().find_map(|(script, vmad)| {
                    let code = self.resolve(&script, QuestFormId(form), name)?;
                    Some((script, QuestFormId(form), Some((*vmad).clone()), code))
                }),
                _ => None,
            },
        };
        Ok(target.map(|(script, owner, vmad, code)| {
            (
                Frame {
                    script,
                    owner,
                    vmad,
                    locals: HashMap::new(),
                    defined_in: None,
                },
                code,
            )
        }))
    }

    /// The script `name` names, when it is not a value in `frame`: the
    /// receiver of a global call.
    pub(super) fn global_script(&self, frame: &Frame, name: &str) -> Option<Arc<Script>> {
        let key = name.to_ascii_lowercase();
        if key == "self" || frame.locals.contains_key(&key) {
            return None;
        }
        if Self::property(frame.vmad.as_ref(), name).is_some()
            || member_type_in(&frame.script, name, &self.deferred.vm_scripts).is_some()
        {
            return None;
        }
        self.deferred.vm_scripts.get(&key).cloned()
    }

    pub(super) fn call_value(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Value> {
        if matches!(guard_call(callee), Some(GuardCall::TryLock)) {
            return Ok(Value::Bool(true));
        }
        if let Some(value) = self.array_call(frame, callee, args)? {
            return Ok(value);
        }
        if let Some(value) = self.collection_call(frame, callee, args)? {
            return Ok(value);
        }
        if let Some(value) = self.layer_call(frame, callee, args)? {
            return Ok(value);
        }
        // (A call whose value is used is not "for effect", whatever encloses it.)
        let saved = std::mem::replace(&mut self.for_effect, false);
        let read = self.object_read(frame, callee, args);
        self.for_effect = saved;
        if let Some(value) = read? {
            return Ok(value);
        }
        if let Some(value) = self.script_call(frame, callee, args)? {
            return Ok(value);
        }
        if self_call_named(callee, "getstate") {
            let script = Arc::clone(&frame.script);
            return Ok(Value::Str(self.script_state(frame.owner, &script)));
        }
        let (call, bindings, vmad) = self.synthesize(frame, callee, args)?;
        if let Some(value) = classify_native_value(&call, &bindings) {
            let number = evaluate_value(&value, self.context, Some(&vmad), self.world, self.stages, self.deferred);
            return match number {
                Some(number) => Ok(Value::Float(number)),
                None => stop("a numeric read could not be resolved"),
            };
        }
        if let Some(guards) = classify_native_guard(&call, &bindings) {
            let mut all = true;
            for guard in &guards {
                match evaluate_guard(guard, self.context, Some(&vmad), self.world, self.stages, self.objectives, self.deferred) {
                    Some(pass) => all &= pass,
                    None => return stop("a Bool read could not be resolved"),
                }
            }
            return Ok(Value::Bool(all));
        }
        stop(format!("unsupported call {}", callee_name(callee)))
    }

    pub(super) fn call_statement(&mut self, frame: &mut Frame, expression: &Expr) -> Run<Flow> {
        let Expr::Call { callee, args } = expression else {
            // A bare expression statement has no effect.
            self.eval(frame, expression)?;
            return Ok(Flow::Normal);
        };
        // `Utility.Wait(seconds)` suspends the run, every frame of it.
        if let Expr::MemberAccess { object, member } = &callee.node {
            if static_class(&object.node) == Some("utility") && member.node.0.eq_ignore_ascii_case("wait") {
                let seconds = match args.first() {
                    Some(arg) => self.eval(frame, &arg.value.node)?.number().unwrap_or(0.0),
                    None => 0.0,
                };
                // #5681 — the seconds are a run-time value here, so a literal
                // the lowerer already declined can still reach the VM. A wait
                // past the shared cap would park an immortal continuation in
                // `FragmentExecutionQueue`; stopping the run is the
                // decline-safe outcome, same choice the provider lowerer
                // makes for the same literal.
                if seconds > MAX_PROVIDER_WAIT_SECONDS {
                    return stop("Utility.Wait beyond MAX_PROVIDER_WAIT_SECONDS");
                }
                return Ok(Flow::Suspend(seconds.max(0.0), Vec::new(), Vec::new()));
            }
        }
        // CK *WaitForAnimationEvent*: "waits for the specified animation event from
        // this object to happen before returning". The graph's schedule is
        // deterministic, so the wait is computed now; with no graph (or an event it
        // never raises) the call returns false at once, which the CK gives as "the
        // animation graph unloaded before the event was sent".
        if let Expr::MemberAccess { object, member } = &callee.node {
            if member.node.0.eq_ignore_ascii_case("waitforanimationevent")
                && args.len() == 1
                && static_class(&object.node).is_none()
            {
                if let Value::Form(reference) = self.eval(frame, &object.node)? {
                    let values = self.eval_args(frame, args)?;
                    let event = values[0].text();
                    let seconds = self
                        .entity_of(reference)
                        .and_then(|entity| crate::animation_graph::wait_for_animation_event(self.world, entity, &event));
                    return match seconds {
                        Some(seconds) if seconds > MAX_PROVIDER_WAIT_SECONDS => {
                            stop("WaitForAnimationEvent beyond MAX_PROVIDER_WAIT_SECONDS")
                        }
                        Some(seconds) => Ok(Flow::Suspend(seconds.max(0.0), Vec::new(), Vec::new())),
                        None => Ok(Flow::Normal),
                    };
                }
            }
        }
        // CK *PlayAnimationAndWait*: send the event and wait until the graph raises
        // the named one (the schedule is deterministic, so the wait is computed
        // now). A graph that rejects the event, or never raises the other, makes
        // the call return false at once: no wait.
        if let Expr::MemberAccess { object, member } = &callee.node {
            if member.node.0.eq_ignore_ascii_case("playanimationandwait")
                && args.len() == 2
                && static_class(&object.node).is_none()
            {
                if let Value::Form(reference) = self.eval(frame, &object.node)? {
                    let values = self.eval_args(frame, args)?;
                    let (event, wait_event) = (values[0].text(), values[1].text());
                    let seconds = self.entity_of(reference).and_then(|entity| {
                        crate::animation_graph::play_animation_and_wait(self.world, entity, &event, &wait_event)
                    });
                    return match seconds {
                        Some(seconds) if seconds > MAX_PROVIDER_WAIT_SECONDS => {
                            stop("PlayAnimationAndWait beyond MAX_PROVIDER_WAIT_SECONDS")
                        }
                        Some(seconds) => Ok(Flow::Suspend(seconds.max(0.0), Vec::new(), Vec::new())),
                        None => Ok(Flow::Normal),
                    };
                }
            }
        }
        // Help-message housekeeping names the `Message` class or a message: the
        // receiver is never read.
        // CK *Add - MusicType* / *Remove - MusicType*: "Adds this music type to
        // the music stack, causing it to start playing if it's the highest
        // priority item". There is no music player (the music is xWMA, which the
        // engine cannot decode), so the stack has no observable state and the
        // call is a no-op. Only a receiver declared a `MusicType` is one (a
        // script's own `Add()` function is a different call).
        if let Expr::MemberAccess { object, member } = &callee.node {
            let name = member.node.0.to_ascii_lowercase();
            if matches!(name.as_str(), "add" | "remove") && args.is_empty() && declares_music_type(&frame.script, &object.node) {
                return Ok(Flow::Normal);
            }
        }
        if matches!(guard_call(&callee.node), Some(GuardCall::Lock | GuardCall::Unlock)) {
            return Ok(Flow::Normal);
        }
        // CK *StopDialogueCamera - Game*: "Stops all dialogue cameras". The
        // engine never starts one (a conversation here has no camera of its
        // own), so there is nothing to stop.
        if let Expr::MemberAccess { object, member } = &callee.node {
            if static_class(&object.node) == Some("game") && member.node.0.eq_ignore_ascii_case("stopdialoguecamera") {
                self.eval_args(frame, args)?;
                return Ok(Flow::Normal);
            }
        }
        if help_message_housekeeping(&callee.node).is_some() {
            self.eval_args(frame, args)?;
            return Ok(Flow::Normal);
        }
        if self.array_call(frame, &callee.node, args)?.is_some() {
            return Ok(Flow::Normal);
        }
        if self.collection_call(frame, &callee.node, args)?.is_some() {
            return Ok(Flow::Normal);
        }
        if self.layer_call(frame, &callee.node, args)?.is_some() {
            return Ok(Flow::Normal);
        }
        self.for_effect = true;
        let read = self.object_read(frame, &callee.node, args);
        self.for_effect = false;
        if read?.is_some() {
            return Ok(Flow::Normal);
        }
        // CK *UpdateCurrentInstanceGlobal - Quest*: "Attempts to update the
        // stored value of the given global in the instance data for the quest's
        // current instance", i.e. refreshes a cached copy of the global. This
        // engine keeps no cached copy (quest text reads the global live), so
        // the statement has nothing to refresh; only the Bool result (failure
        // with no active instance) is unmodelled, and the checker only lets
        // the call through with that result unused.
        if let Expr::MemberAccess { object, member } = &callee.node {
            if member.node.0.eq_ignore_ascii_case("updatecurrentinstanceglobal")
                && static_class(&object.node).is_none()
            {
                if let Value::Form(quest) = self.eval(frame, &object.node)? {
                    if self.is_quest(quest) {
                        self.eval_args(frame, args)?;
                        return Ok(Flow::Normal);
                    }
                }
            }
        }
        // A script function called as a statement may wait.
        if let Some((callee_frame, code)) = self.script_call_target(frame, &callee.node)? {
            let name = callee_name(&callee.node);
            let values = self.eval_args(frame, args)?;
            let mut callee_frame = self.enter(callee_frame, &code, &name, values)?;
            // The caller stays on the stack while the callee runs: if it holds
            // a local a suspension cannot store, the callee must not suspend.
            let blocked = !locals_storable(frame);
            self.blocked_callers += usize::from(blocked);
            let flow = self.run_frame(&mut callee_frame, &code, &name, None, &[]);
            self.blocked_callers -= usize::from(blocked);
            return self.callee_flow(flow?, &callee_frame, &code, &name);
        }
        // `Parent.OnInit()` and the like past the scripts to an engine class:
        // its events have no body, so the call does nothing.
        if let Expr::MemberAccess { object, member } = &callee.node {
            if matches!(object.node, Expr::ParentAccess) && is_event_name(&member.node.0) {
                self.eval_args(frame, args)?;
                return Ok(Flow::Normal);
            }
        }
        // `GotoState`'s `OnEndState` / `OnBeginState` the script does not
        // handle: the empty events every script inherits.
        if is_state_event(&callee.node) {
            return Ok(Flow::Normal);
        }
        if let Some((object, name)) = quest_update_call(&callee.node) {
            if let Value::Form(quest) = self.eval(frame, object)? {
                if self.is_quest(quest) {
                    return self.quest_update(frame, quest, &name, args).map(|_| Flow::Normal);
                }
            }
        }
        if self_call_named(&callee.node, "sendcustomevent") {
            return self.send_custom_event(frame, args).map(|_| Flow::Normal);
        }
        if let Some(name) = update_call(&callee.node) {
            if crate::fragment::EventRequest::is_call(&name) {
                return self.event_registration(frame, None, &name, args).map(|_| Flow::Normal);
            }
            if crate::fragment::DistanceRequest::handler(&name).is_some() {
                return self.distance_registration(frame, None, &name, args).map(|_| Flow::Normal);
            }
            if crate::fragment::HitRequest::handler(&name).is_some() {
                return self.hit_registration(frame, &name, args).map(|_| Flow::Normal);
            }
            let mut numbers = Vec::with_capacity(args.len());
            for arg in args {
                numbers.push(self.eval(frame, &arg.value.node)?.number());
            }
            let Some(request) = crate::fragment::UpdateRequest::of_call(&name, &numbers) else {
                return stop(format!("{name} without its numbers"));
            };
            let script = frame.script.name.node.0.clone();
            self.deferred.script_updates.push((frame.owner.0, script, request));
            return Ok(Flow::Normal);
        }
        if self_call_named(&callee.node, "gotostate") {
            let state = match args.first() {
                Some(arg) => self.eval(frame, &arg.value.node)?.text(),
                None => String::new(),
            };
            self.goto_state(frame, state)?;
            return Ok(Flow::Normal);
        }
        if is_trace(&callee.node) {
            return Ok(Flow::Normal);
        }
        let (call, bindings, vmad) = self.synthesize(frame, &callee.node, args)?;
        let Some(effect) = classify_native_effect(&call, &bindings) else {
            return stop(format!("unsupported statement {}", callee_name(&callee.node)));
        };
        if crate::translate::effects::is_latent(&effect) {
            return stop("a latent call");
        }
        let advances = apply_effects(
            std::slice::from_ref(&effect),
            self.context,
            Some(&vmad),
            self.world,
            self.stages,
            self.objectives,
            self.deferred,
        );
        self.advances.extend(advances);
        Ok(Flow::Normal)
    }
}
