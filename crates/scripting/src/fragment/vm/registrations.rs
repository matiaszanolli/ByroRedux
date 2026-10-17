//! Update, custom-event, distance and hit registrations: the run-time
//! registration and its ahead-of-time check (#5800).

use super::*;

/// An update (un)registration on another object: the receiver and the
/// lowercase name.
pub(super) fn quest_update_call(callee: &Expr) -> Option<(&Expr, String)> {
    let Expr::MemberAccess { object, member } = callee else {
        return None;
    };
    let name = member.node.0.to_ascii_lowercase();
    let on_self = matches!(&object.node, Expr::Ident(id) if id.0.eq_ignore_ascii_case("self"));
    (registration_handler(&name).is_some() && !on_self && static_class(&object.node).is_none())
        .then_some((&object.node, name))
}

/// The handler a registration call delivers to (`""` for a cancellation):
/// an update, timer or distance registration, by lowercase name.
pub(super) fn registration_handler(name: &str) -> Option<&'static str> {
    crate::fragment::UpdateRequest::handler(name)
        .or_else(|| crate::fragment::DistanceRequest::handler(name))
        .or_else(|| crate::fragment::HitRequest::handler(name))
        .or_else(|| crate::fragment::EventRequest::is_call(name).then_some(""))
}

/// An update (un)registration on the script itself, by lowercase name.
pub(super) fn update_call(callee: &Expr) -> Option<String> {
    let name = callee_name(callee).to_ascii_lowercase();
    (registration_handler(&name).is_some() && self_call_named(callee, &name)).then_some(name)
}

/// The aggressor a hit registration filters on, when the filter is one this
/// runtime matches: a reference (the check proved the filter's declared type is
/// one) or an alias.
pub(super) fn target_of_aggressor(value: &Value) -> Option<crate::fragment::DistanceTarget> {
    match value {
        Value::Form(form) if *form != 0 => Some(crate::fragment::DistanceTarget::Reference(*form)),
        Value::Alias { quest, alias } => Some(crate::fragment::DistanceTarget::Alias {
            quest: *quest,
            alias: *alias,
        }),
        _ => None,
    }
}

/// An update or timer call's arguments: numbers, at most as many as it
/// takes, and the interval when it starts one.
pub(super) fn update_args_ok(name: &str, args: &[Arg]) -> bool {
    // A hit registration: the target (a reference, the player or an alias), an
    // aggressor filter that is `None`, the player or an alias, `None` source and
    // projectile filters, four numbers and the `abMatch` Bool.
    if crate::fragment::HitRequest::handler(name).is_some() {
        let object = |arg: &Arg| matches!(arg, Arg::Kind(Kind::Object | Kind::Player | Kind::Alias));
        let unset = |arg: &Arg| matches!(arg, Arg::Literal(Expr::NoneLit));
        let number = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num));
        let flag = |arg: &Arg| matches!(arg, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool));
        return match name {
            "unregisterforallhitevents" => args.is_empty(),
            "unregisterforhitevent" => args.len() == 1 && object(&args[0]),
            _ => {
                !args.is_empty()
                    && args.len() <= 9
                    && object(&args[0])
                    && args.get(1).is_none_or(|a| unset(a) || matches!(a, Arg::Kind(Kind::Player | Kind::Alias)))
                    && args.get(2).is_none_or(unset)
                    && args.get(3).is_none_or(unset)
                    && args.iter().skip(4).take(4).all(number)
                    && args.get(8).is_none_or(flag)
            }
        };
    }
    // A distance registration: two objects (a reference, a player or an alias)
    // and, registering, a number.
    if crate::fragment::DistanceRequest::handler(name).is_some() {
        let object = |arg: &Arg| matches!(arg, Arg::Kind(Kind::Object | Kind::Player | Kind::Alias));
        let number = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num));
        let arity = crate::fragment::DistanceRequest::arity(name);
        return args.len() == arity && args[..2].iter().all(object) && args.get(2).is_none_or(number);
    }
    let numeric = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num));
    let starts = !crate::fragment::UpdateRequest::handler(name).unwrap_or_default().is_empty();
    args.len() <= crate::fragment::UpdateRequest::max_args(name) && (!starts || !args.is_empty()) && args.iter().all(numeric)
}

impl Vm<'_> {
    /// `<quest>.RegisterForUpdate(…)` and the like on another quest: every
    /// script attached to it that handles the update registers (or all of
    /// them unregister).
    pub(super) fn quest_update(&mut self, frame: &mut Frame, quest: u32, name: &str, args: &[CallArg]) -> Run<()> {
        if crate::fragment::EventRequest::is_call(name) {
            return self.event_registration(frame, Some(quest), name, args);
        }
        if crate::fragment::DistanceRequest::handler(name).is_some() {
            return self.distance_registration(frame, Some(quest), name, args);
        }
        let mut numbers = Vec::with_capacity(args.len());
        for arg in args {
            numbers.push(self.eval(frame, &arg.value.node)?.number());
        }
        let Some(request) = crate::fragment::UpdateRequest::of_call(name, &numbers) else {
            return stop(format!("{name} without its numbers"));
        };
        let handler = crate::fragment::UpdateRequest::handler(name).unwrap_or_default();
        let owner = QuestFormId(quest);
        let attached: Vec<String> = self
            .deferred
            .quest_scripts
            .get(&owner)
            .map(|vmad| vmad.scripts.iter().map(|script| script.name.clone()).collect())
            .unwrap_or_default();
        for name in attached {
            let script = self.deferred.vm_scripts.get(&name.to_ascii_lowercase()).cloned();
            let handles = script.is_some_and(|script| handler.is_empty() || self.resolve(&script, owner, handler).is_some());
            if handles {
                self.deferred.script_updates.push((quest, name, request));
            }
        }
        Ok(())
    }

    /// `RegisterForCustomEvent` / `RegisterForRemoteEvent` and the unregister
    /// calls, on the script itself (`quest` `None`) or on another quest, whose
    /// attached scripts all register.
    pub(super) fn event_registration(&mut self, frame: &mut Frame, quest: Option<u32>, name: &str, args: &[CallArg]) -> Run<()> {
        let values = self.eval_args(frame, args)?;
        let sender = match values.first() {
            Some(Value::Form(form)) if *form != 0 => Some(crate::fragment::DistanceTarget::Reference(*form)),
            Some(Value::Alias { quest, alias }) => Some(crate::fragment::DistanceTarget::Alias {
                quest: *quest,
                alias: *alias,
            }),
            _ => None,
        };
        let event = match values.get(1) {
            Some(Value::Str(event)) => Some(event.clone()),
            _ => None,
        };
        if !name.starts_with("unregisterforall") && (sender.is_none() || event.is_none()) {
            // CK: the sender must be a script object; a None registers nothing.
            return Ok(());
        }
        let Some(request) = crate::fragment::EventRequest::of_call(name, sender, event) else {
            return stop(format!("{name} as written"));
        };
        let scripts: Vec<(u32, String)> = match quest {
            None => vec![(frame.owner.0, frame.script.name.node.0.clone())],
            Some(quest) => self
                .deferred
                .quest_scripts
                .get(&QuestFormId(quest))
                .map(|vmad| vmad.scripts.iter().map(|script| (quest, script.name.clone())).collect())
                .unwrap_or_default(),
        };
        for (owner, script) in scripts {
            self.deferred.event_requests.push((owner, script, request.clone()));
        }
        Ok(())
    }

    /// `SendCustomEvent(name, args)` on the script itself: every registered
    /// receiver's handler is queued to run on its own, after this run.
    pub(super) fn send_custom_event(&mut self, frame: &mut Frame, args: &[CallArg]) -> Run<()> {
        let values = self.eval_args(frame, args)?;
        let Some(Value::Str(event)) = values.first() else {
            return stop("SendCustomEvent without an event name");
        };
        let payload = match values.get(1) {
            None | Some(Value::None) => VmStored::None,
            Some(array @ Value::Array(_)) => match event_arg(array) {
                Some(stored) => stored,
                None => return stop("a custom event argument that cannot be sent"),
            },
            Some(other) => return stop(format!("custom event arguments are an array, not {}", other.text())),
        };
        let stored = self
            .world
            .try_resource::<crate::fragment::EventRegistrations>()
            .map(|registrations| registrations.clone())
            .unwrap_or_default();
        let registrants = crate::fragment::events::registrants(&stored, &self.deferred.event_requests, frame.owner.0, event, false);
        crate::fragment::events::enqueue(self.world, &registrants, frame.owner.0, &[payload]);
        Ok(())
    }

    /// `Register…DistanceEvent` / `UnregisterForDistanceEvents` on the script
    /// itself (`quest` `None`) or on another quest, whose attached scripts that
    /// handle the event all register.
    pub(super) fn distance_registration(&mut self, frame: &mut Frame, quest: Option<u32>, name: &str, args: &[CallArg]) -> Run<()> {
        let values = self.eval_args(frame, args)?;
        let target = |value: &Value| match value {
            Value::Form(form) if *form != 0 => Some(crate::fragment::DistanceTarget::Reference(*form)),
            Value::Alias { quest, alias } => Some(crate::fragment::DistanceTarget::Alias {
                quest: *quest,
                alias: *alias,
            }),
            _ => None,
        };
        let (Some(a), Some(b)) = (values.first().and_then(target), values.get(1).and_then(target)) else {
            // CK: the objects "must be an ObjectReference or ReferenceAlias";
            // a None registers nothing.
            return Ok(());
        };
        let Some(request) = crate::fragment::DistanceRequest::of_call(name, a, b, values.get(2).and_then(Value::number)) else {
            return stop(format!("{name} without a distance"));
        };
        let handler = crate::fragment::DistanceRequest::handler(name).unwrap_or_default();
        let (owner, scripts): (u32, Vec<String>) = match quest {
            None => (frame.owner.0, vec![frame.script.name.node.0.clone()]),
            Some(quest) => {
                let owner = QuestFormId(quest);
                let attached: Vec<String> = self
                    .deferred
                    .quest_scripts
                    .get(&owner)
                    .map(|vmad| vmad.scripts.iter().map(|script| script.name.clone()).collect())
                    .unwrap_or_default();
                let scripts = attached
                    .into_iter()
                    .filter(|name| {
                        let script = self.deferred.vm_scripts.get(&name.to_ascii_lowercase()).cloned();
                        script.is_some_and(|script| handler.is_empty() || self.resolve(&script, owner, handler).is_some())
                    })
                    .collect();
                (quest, scripts)
            }
        };
        for script in scripts {
            self.deferred.distance_requests.push((owner, script, request));
        }
        Ok(())
    }

    /// `RegisterForHitEvent` / `UnregisterForHitEvent` /
    /// `UnregisterForAllHitEvents` on the script itself (see [`crate::fragment::hits`]).
    pub(super) fn hit_registration(&mut self, frame: &mut Frame, name: &str, args: &[CallArg]) -> Run<()> {
        let values = self.eval_args(frame, args)?;
        let target = |value: &Value| match value {
            Value::Form(form) if *form != 0 => Some(crate::fragment::DistanceTarget::Reference(*form)),
            Value::Alias { quest, alias } => Some(crate::fragment::DistanceTarget::Alias {
                quest: *quest,
                alias: *alias,
            }),
            _ => None,
        };
        let request = match name {
            "unregisterforallhitevents" => crate::fragment::HitRequest::UnregisterAll,
            "unregisterforhitevent" => match values.first().and_then(target) {
                Some(target) => crate::fragment::HitRequest::Unregister { target },
                None => return Ok(()),
            },
            _ => {
                // CK: the target is "the ObjectReference, ReferenceAlias or
                // RefCollectionAlias that is being hit"; a None registers nothing.
                let Some(target) = values.first().and_then(target) else {
                    return Ok(());
                };
                let aggressor = match values.get(1) {
                    None | Some(Value::None) => None,
                    Some(value @ Value::Form(form)) if *form != 0 => {
                        // A faction filter matches the aggressor's membership;
                        // any other form the check let through is a reference.
                        if self.declared_object_type(frame, args.get(1)).is_some_and(|ty| ty.eq_ignore_ascii_case("faction")) {
                            Some(crate::fragment::HitAggressor::Faction(*form))
                        } else {
                            target_of_aggressor(value).map(crate::fragment::HitAggressor::Target)
                        }
                    }
                    Some(value) => match target_of_aggressor(value) {
                        Some(aggressor) => Some(crate::fragment::HitAggressor::Target(aggressor)),
                        None => return stop("a hit event aggressor filter beyond None, a reference, an alias and a faction"),
                    },
                };
                if !matches!(values.get(2), None | Some(Value::None)) || !matches!(values.get(3), None | Some(Value::None)) {
                    return stop("a hit event source or projectile filter");
                }
                let tri = |index: usize| values.get(index).and_then(Value::int).unwrap_or(-1);
                crate::fragment::HitRequest::Register {
                    target,
                    filter: crate::fragment::HitFilter {
                        aggressor,
                        power: tri(4),
                        sneak: tri(5),
                        bash: tri(6),
                        block: tri(7),
                        invert: values.get(8).is_some_and(|value| !value.truthy()),
                    },
                }
            }
        };
        let script = frame.script.name.node.0.clone();
        self.deferred.hit_requests.push((frame.owner.0, script, request));
        Ok(())
    }

    /// The declared object type of a call argument written as a property or a
    /// cast of one (`OnHitFaction`, `Alias_Boss as ScriptObject`).
    pub(super) fn declared_object_type(&self, frame: &Frame, arg: Option<&CallArg>) -> Option<String> {
        let mut expr = &arg?.value.node;
        while let Expr::Cast { expr: inner, .. } = expr {
            expr = &inner.node;
        }
        let Expr::Ident(name) = expr else {
            return None;
        };
        match member_type_in(&frame.script, &name.0, &self.deferred.vm_scripts)? {
            Type::Object(ty) => Some(ty.0),
            _ => None,
        }
    }
}

impl Checker<'_> {
    /// Whether `event` is Fallout 4's `OnHit(akTarget, akAggressor, akSource,
    /// akProjectile, abPowerAttack, abSneakAttack, abBashAttack, abHitBlocked,
    /// apMaterial)`: unlike Skyrim's seven-parameter `OnHit`, it is delivered only
    /// to the script that registered for it (`RegisterForHitEvent`), so the
    /// engine's automatic delivery leaves it alone and the registration site
    /// checks it.
    pub(super) fn hit_by_registration(&mut self, script: &Arc<Script>, event: &str) -> bool {
        event.eq_ignore_ascii_case("onhit")
            && self
                .versions(script, event)
                .iter()
                .find_map(|(defined_in, state)| callable_in(defined_in, state.as_deref(), event).map(|c| c.params.len()))
                .is_some_and(|params| params > 7)
    }

    /// A custom / remote event registration call on an instance of `script`:
    /// the handler it would deliver to must run, and a remote event must be one
    /// the engine raises.
    pub(super) fn check_event_registration(
        &mut self,
        script: &Arc<Script>,
        name: &str,
        args: &[Arg],
        statement: bool,
        self_kind: Kind,
    ) -> Option<Kind> {
        let object = |arg: &Arg| matches!(arg, Arg::Kind(Kind::Object | Kind::Player | Kind::Alias | Kind::Quest));
        let literal = match args {
            [sender, Arg::Literal(Expr::StringLit(event))] if object(sender) => Some(event.clone()),
            _ => None,
        };
        let shaped = statement
            && args.len() == crate::fragment::EventRequest::arity(name)
            && (name.starts_with("unregisterforall") || literal.is_some());
        if !shaped {
            return self.fail(|| format!("{name} as written"));
        }
        let remote = name.contains("remote");
        if name.starts_with("unregisterfor") {
            return Some(Kind::None);
        }
        let event = literal.unwrap_or_default();
        if remote && !crate::fragment::REMOTE_EVENTS.contains(&event.to_ascii_lowercase().as_str()) {
            return self.fail(|| format!("remote event {event} is not raised by the engine"));
        }
        if let Some(handler) = crate::fragment::events::handler_in(script, &event, remote) {
            self.check_handler(script, &handler, self_kind)?;
        }
        Some(Kind::None)
    }
}
