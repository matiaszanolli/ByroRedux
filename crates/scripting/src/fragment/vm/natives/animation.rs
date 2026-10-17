//! Animation-graph natives: events, variables, archetypes and the latent waits.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "changeanimarchetype",
    "playanimation",
    "registerforanimationevent",
    "unregisterforanimationevent",
    "setanimationvariablefloat",
    "setanimationvariablebool",
    "setanimationvariableint",
    "getanimationvariablefloat",
    "getanimationvariablebool",
    "getanimationvariableint",
];

impl Vm<'_> {
    /// The animation reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_animation(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
            // Animation-graph natives (CK *PlayAnimation*, *RegisterForAnimationEvent*,
            // *SetAnimationVariable\**): see [`crate::animation_graph`]. `PlayAnimation`
            // sends the event to the object's behavior graph; the registrations
            // deliver `OnAnimationEvent` (see [`crate::fragment::animation_event_system`]) and
            // a variable set has no graph variable to write.
            (Value::Form(reference), "playanimation") if args.len() == 1 => {
                let values = self.eval_args(frame, args)?;
                let event = values[0].text();
                let accepted = self
                    .entity_of(*reference)
                    .is_some_and(|entity| crate::animation_graph::play_animation(self.world, entity, &event));
                Some(Value::Bool(accepted))
            }
            // CK *RegisterForAnimationEvent - Form*: true "if it was successfully able
            // to register", false when "the animation graph may not be loaded". A
            // reference has a graph here when its prop attached one; the registrant
            // is the script that makes the call (a call on another form is not a
            // registration this script can own), and the event name is never
            // checked: "the game will allow you to register ANY animation event".
            (Value::Form(registrant), "registerforanimationevent") if args.len() == 2 => {
                let values = self.eval_args(frame, args)?;
                let sender = match values.first() {
                    Some(Value::Form(sender)) if *sender != 0 => Some(*sender),
                    _ => None,
                };
                let registered = sender.filter(|_| *registrant == frame.owner.0).filter(|sender| {
                    self.entity_of(*sender)
                        .is_some_and(|entity| crate::animation_graph::has_graph(self.world, entity))
                });
                match registered {
                    Some(sender) => {
                        let script = frame.script.name.node.0.clone();
                        let event = values[1].text();
                        self.deferred.animation_event_requests.push((
                            frame.owner.0,
                            script,
                            crate::fragment::AnimationEventRequest::Register { sender, event },
                        ));
                        Some(Value::Bool(true))
                    }
                    None => Some(Value::Bool(false)),
                }
            }
            (Value::Form(registrant), "unregisterforanimationevent") if args.len() == 2 => {
                let values = self.eval_args(frame, args)?;
                if let (true, Some(Value::Form(sender))) = (*registrant == frame.owner.0, values.first()) {
                    let script = frame.script.name.node.0.clone();
                    let event = values[1].text();
                    self.deferred.animation_event_requests.push((
                        frame.owner.0,
                        script,
                        crate::fragment::AnimationEventRequest::Unregister { sender: *sender, event },
                    ));
                }
                Some(Value::None)
            }
            // CK *SetAnimationVariable\** / *GetAnimationVariable\**: "the value of a variable
            // on the reference's animation graph". A reference with no graph, or a graph
            // without that variable, takes the write for nothing and reads as zero.
            (Value::Form(reference), "setanimationvariablefloat" | "setanimationvariablebool" | "setanimationvariableint")
                if args.len() == 2 =>
            {
                let values = self.eval_args(frame, args)?;
                let value = match &values[1] {
                    Value::Bool(flag) => f32::from(*flag),
                    Value::Int(whole) => *whole as f32,
                    Value::Float(real) => *real,
                    _ => return stop(format!("{name} with a non-number")),
                };
                if let Some(entity) = self.entity_of(*reference) {
                    crate::animation_graph::set_variable_value(self.world, entity, &values[0].text(), value);
                }
                Some(Value::None)
            }
            (Value::Form(reference), "getanimationvariablefloat" | "getanimationvariablebool" | "getanimationvariableint")
                if args.len() == 1 =>
            {
                let values = self.eval_args(frame, args)?;
                let value = self
                    .entity_of(*reference)
                    .and_then(|entity| crate::animation_graph::variable_value(self.world, entity, &values[0].text()))
                    .unwrap_or(0.0);
                Some(match name {
                    "getanimationvariablebool" => Value::Bool(value != 0.0),
                    "getanimationvariableint" => Value::Int(value as i32),
                    _ => Value::Float(value),
                })
            }
            // CK *ChangeAnimArchetype - Actor* (Fallout 4): "adds the specified
            // AnimArchetype keyword to this actor"; none "removes any existing
            // AnimArchetype keyword". Returns true on success.
            (Value::Form(actor), "changeanimarchetype") if args.len() <= 1 => {
                let values = self.eval_args(frame, args)?;
                let keyword = match values.first() {
                    None | Some(Value::None) => 0,
                    Some(Value::Form(keyword)) => *keyword,
                    _ => return stop("ChangeAnimArchetype without a keyword"),
                };
                let Some(reference) = self.actor_reference(*actor) else {
                    return stop("ChangeAnimArchetype on an actor that is not loaded");
                };
                self.deferred.anim_archetype_changes.push((reference, keyword));
                Some(Value::Bool(true))
            }
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The animation calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_animation(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Object | Kind::Player, "changeanimarchetype")
                if arg_kinds.len() <= 1
                    && arg_kinds.iter().all(|a| {
                        matches!(a, Arg::Literal(Expr::NoneLit) | Arg::Kind(Kind::Object | Kind::None))
                    }) =>
            {
                return Some(Kind::Bool);
            }
            // The animation-graph natives (see `animation_graph`): the CK's
            // failure results, since no behavior graph runs.
            (Kind::Object | Kind::Player, "playanimation") if matches!(arg_kinds, [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]) => {
                return Some(Kind::Bool);
            }
            (Kind::Object | Kind::Player, "playanimationandwait")
                if statement
                    && matches!(
                        arg_kinds,
                        [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str), Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]
                    ) =>
            {
                // Latent: it waits for the graph's event, so only as a statement.
                self.waits = true;
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "waitforanimationevent")
                if statement && matches!(arg_kinds, [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]) =>
            {
                // Latent, like PlayAnimationAndWait: only as a statement.
                self.waits = true;
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "registerforanimationevent" | "unregisterforanimationevent")
                if matches!(
                    arg_kinds,
                    [Arg::Kind(Kind::Object | Kind::Player), Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]
                ) =>
            {
                return Some(if name == "registerforanimationevent" { Kind::Bool } else { Kind::None });
            }
            (Kind::Object | Kind::Player, "setanimationvariablefloat" | "setanimationvariableint")
                if statement
                    && matches!(
                        arg_kinds,
                        [
                            Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str),
                            Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num)
                        ]
                    ) =>
            {
                return Some(Kind::None);
            }
            (Kind::Object | Kind::Player, "getanimationvariablefloat" | "getanimationvariableint")
                if matches!(arg_kinds, [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]) =>
            {
                return Some(Kind::Num);
            }
            (Kind::Object | Kind::Player, "getanimationvariablebool")
                if matches!(arg_kinds, [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)]) =>
            {
                return Some(Kind::Bool);
            }
            (Kind::Object | Kind::Player, "setanimationvariablebool")
                if statement
                    && matches!(
                        arg_kinds,
                        [Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str), Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool)]
                    ) =>
            {
                return Some(Kind::None);
            }
            _ => {}
        }
        None
    }
}
