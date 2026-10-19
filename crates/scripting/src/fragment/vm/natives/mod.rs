//! The natives the VM reads itself — the object reads no effect models —
//! by Papyrus API family (#5800).
//!
//! [`Vm::object_read`] is the hub of the run-time reads and
//! [`Checker::check_call`] (in `checker.rs`) the hub of the ahead-of-time
//! check; each family file holds both halves of its natives — the
//! `read_<family>` arm the run takes and the `check_<family>` arm that says
//! the VM can run the call — and its `READS`, the names whose receiver the
//! hub evaluates. A new native is one family file's change.
//!
//! **Dispatch order.** The families are what used to be one first-match
//! `match` on `(receiver, method name)`, cut by method name: every arm that
//! names a method lives in that method's one family, in its original order,
//! and no arm a family holds can match a name another family holds. So the
//! first family to claim a call is the only one that could, and the order
//! the hubs try them in cannot change which arm answers. Keep it so: an arm
//! for a method a family already holds goes into that family, after the
//! arms it must not shadow. Arms that match every name (a call on `None`)
//! stay in the hub, after the families.

use super::*;

mod actor;
mod alias;
mod animation;
mod collections;
mod combat;
mod destruction;
mod favor;
mod form_list;
mod forms;
mod game_utility;
mod linked_refs;
mod location;
mod ownership_crime;
mod quest;
mod reference;
mod scanning;

pub(super) use collections::{layer_native, LayerNative, COLLECTION_NATIVES};

/// Every family's `READS`: a name none of them reads is not an object read,
/// and its receiver is not evaluated.
const FAMILY_READS: [&[&str]; 14] = [
    alias::READS,
    quest::READS,
    form_list::READS,
    favor::READS,
    actor::READS,
    ownership_crime::READS,
    reference::READS,
    linked_refs::READS,
    location::READS,
    forms::READS,
    animation::READS,
    combat::READS,
    destruction::READS,
    scanning::READS,
];

/// Builds one synthetic native call: evaluated values bound to names.
#[derive(Default)]
pub(in crate::fragment::vm) struct Synth {
    pub(in crate::fragment::vm) bindings: NativeBindings,
    pub(in crate::fragment::vm) properties: Vec<ScriptProperty>,
}

impl Synth {
    pub(in crate::fragment::vm) fn bind(&mut self, vm: &Vm<'_>, value: &Value) -> Run<Spanned<Expr>> {
        let name = format!("__vm{}", self.properties.len() + self.bindings.players.len() + 1);
        let object = |form_id: u32, alias: i16| PropertyValue::Object { form_id, alias };
        Ok(match value {
            Value::None => spanned(Expr::NoneLit),
            Value::Bool(b) => spanned(Expr::BoolLit(*b)),
            Value::Int(i) => spanned(Expr::IntLit(i64::from(*i))),
            // An integral number binds as an Int literal: the primitives read
            // Int parameters with `int_arg`, Float ones with `as_num`, which
            // takes either.
            Value::Float(f) if f.fract() == 0.0 && f.abs() < 2_147_483_648.0 => {
                spanned(Expr::IntLit(*f as i64))
            }
            Value::Float(f) => spanned(Expr::FloatLit(f64::from(*f))),
            Value::Str(s) => spanned(Expr::StringLit(s.clone())),
            Value::Form(PLAYER_REF) => {
                self.bindings.players.insert(name.clone());
                ident(&name)
            }
            Value::Form(form) => {
                if vm.is_quest(*form) {
                    self.bindings.quests.insert(name.clone());
                }
                self.properties.push(ScriptProperty {
                    name: name.clone(),
                    status: 1,
                    value: object(*form, -1),
                });
                ident(&name)
            }
            // The effect runtime resolves an alias property against the
            // fragment's own quest.
            Value::Alias { quest, alias } if *quest == vm.context.0 => {
                self.properties.push(ScriptProperty {
                    name: name.clone(),
                    status: 1,
                    value: object(*quest, *alias),
                });
                ident(&name)
            }
            Value::Alias { .. } => return stop("another quest's alias as a native argument"),
            Value::Array(_) => return stop("an array as a native argument"),
            Value::Struct(_) => return stop("a struct as a native argument"),
        })
    }

    pub(in crate::fragment::vm) fn vmad(self) -> (NativeBindings, ScriptInstanceData) {
        (
            self.bindings,
            ScriptInstanceData {
                version: 5,
                object_format: 2,
                scripts: vec![ScriptInstance {
                    name: "<Papyrus VM>".into(),
                    status: 0,
                    properties: self.properties,
                }],
            },
        )
    }
}

/// The `Motion_*` constant Skyrim's raw `SetMotionType` number names (CK
/// *SetMotionType - ObjectReference*: Dynamic 1, SphereInertia 2, BoxInertia 3,
/// Keyframed 4, Fixed 5, ThinBoxInertia 6, Character 7). The inertia variants
/// are softened simulations the physics layer has no kind for, so they are not
/// mapped. Fallout 4's numbers differ, and no page here lists them.
pub(in crate::fragment::vm) fn skyrim_motion_constant(raw: i32) -> Option<&'static str> {
    Some(match raw {
        1 => "Motion_Dynamic",
        4 => "Motion_Keyframed",
        5 => "Motion_Fixed",
        7 => "Motion_Character",
        _ => return None,
    })
}

impl Vm<'_> {
    /// The object reads no effect models.
    ///
    /// A hub (#5800): the static classes go to [`Vm::read_static_class`]; an
    /// instance native is offered to each family in turn (see the module
    /// docs for why their order cannot matter), and a call on `None` is
    /// answered here, after every family has declined it.
    pub(in crate::fragment::vm) fn object_read(&mut self, frame: &mut Frame, callee: &Expr, args: &[CallArg]) -> Run<Option<Value>> {
        // The Form natives a script calls on itself without a receiver
        // (`RegisterForAnimationEvent(Self, "Reset")`) read as `Self.<name>(...)`.
        if let Expr::Ident(called) = callee {
            if matches!(
                called.0.to_ascii_lowercase().as_str(),
                "registerforanimationevent" | "unregisterforanimationevent"
            ) {
                let on_self = Expr::MemberAccess {
                    object: Box::new(ident("self")),
                    member: spanned(called.clone()),
                };
                return self.object_read(frame, &on_self, args);
            }
        }
        let Expr::MemberAccess { object, member } = callee else {
            return Ok(None);
        };
        let name = member.node.0.to_ascii_lowercase();
        // CK *Create - InputEnableLayer*: a new layer, as its handle. The class
        // is an engine type the script names, not a value.
        if name == "create"
            && matches!(&object.node, Expr::Ident(class) if class.0.eq_ignore_ascii_case("inputenablelayer"))
            && !frame.locals.contains_key("inputenablelayer")
        {
            let Some(mut layers) = self.world.try_resource_mut::<crate::InputLayers>() else {
                return stop("InputEnableLayer.Create without the layer store");
            };
            return Ok(Some(Value::Int(layers.create() as i32)));
        }
        if let Some(class) = static_class(&object.node) {
            return self.read_static_class(frame, class, &name, args);
        }
        if !FAMILY_READS.iter().any(|family| family.contains(&name.as_str())) {
            return Ok(None);
        }
        let owner = self.eval(frame, &object.node)?;
        let families = [
            Self::read_alias,
            Self::read_quest,
            Self::read_form_list,
            Self::read_favor,
            Self::read_actor,
            Self::read_ownership_crime,
            Self::read_reference,
            Self::read_linked_refs,
            Self::read_location,
            Self::read_forms,
            Self::read_animation,
            Self::read_combat,
            Self::read_destruction,
            Self::read_scanning,
        ];
        for read in families {
            if let Some(value) = read(self, frame, &owner, &name, args)? {
                return Ok(Some(value));
            }
        }
        Ok(match (&owner, name.as_str()) {
            // CK *Papyrus Runtime Errors*: 'Cannot call X() on a None object, aborting
            // function call' - "since you cannot call a non-global function on
            // nothing, the call is ignored and the error printed out". A call for
            // effect is skipped and the script goes on; one whose result is used has
            // no result to give, so the run stops.
            (Value::None, _) if self.for_effect => {
                log::debug!("Papyrus: {name}() called on a None object, ignored");
                Some(Value::None)
            }
            (Value::None, _) => return stop(format!("{name} on None")),
            _ => None,
        })
    }

    pub(in crate::fragment::vm) fn entity_form(&self, entity: EntityId) -> Option<u32> {
        crate::condition::entity_global_form_id(self.world, entity)
    }

    pub(in crate::fragment::vm) fn entity_of(&self, form: u32) -> Option<EntityId> {
        crate::condition::resolve_entity_by_global_form_id(self.world, form)
    }

    pub(in crate::fragment::vm) fn base_of(&self, form: u32) -> Option<u32> {
        let entity = self.entity_of(form)?;
        crate::relationships::actor_base(self.world, entity)
    }

    /// Re-write `callee(args)` over bound values for the primitives.
    pub(in crate::fragment::vm) fn synthesize(
        &mut self,
        frame: &mut Frame,
        callee: &Expr,
        args: &[CallArg],
    ) -> Run<(Expr, NativeBindings, ScriptInstanceData)> {
        let mut synth = Synth::default();
        let mut bound_receiver = None;
        let callee = match callee {
            Expr::MemberAccess { object, member } => {
                let receiver = if static_class(&object.node).is_some() {
                    (**object).clone()
                } else {
                    let value = self.eval(frame, &object.node)?;
                    if matches!(value, Value::None) {
                        return stop(format!("{} on None", member.node.0));
                    }
                    let receiver = synth.bind(self, &value)?;
                    bound_receiver = Some(receiver.clone());
                    receiver
                };
                Expr::MemberAccess {
                    object: Box::new(receiver),
                    member: member.clone(),
                }
            }
            // A bare global native (`GetCurrentGameTime()` style).
            other => other.clone(),
        };
        let mut bound = Vec::with_capacity(args.len());
        for arg in args {
            // An engine constant passes as the named member it was written
            // as (`Self.Motion_Keyframed`): its number differs per game, the
            // name does not.
            let constant = constant_name(&arg.value.node).filter(|name| self.engine_constant(name).is_some());
            // `SetMotionType(4, …)`: the raw number is the game's (see
            // [`skyrim_motion_constant`]); it passes as the constant it names.
            if bound.is_empty() && callee_member_is(&callee, "setmotiontype") {
                if let Some(receiver) = &bound_receiver {
                    let raw = self.eval(frame, &arg.value.node)?;
                    let skyrim = self
                        .world
                        .try_resource::<crate::ScriptGame>()
                        .is_some_and(|game| game.0 == byroredux_plugin::esm::reader::GameKind::Skyrim);
                    if let (Value::Int(raw), true) = (&raw, skyrim) {
                        let Some(name) = skyrim_motion_constant(*raw) else {
                            return stop(format!("SetMotionType with the motion type {raw}"));
                        };
                        bound.push(CallArg {
                            name: None,
                            value: spanned(Expr::MemberAccess {
                                object: Box::new(receiver.clone()),
                                member: spanned(Identifier(name.to_owned())),
                            }),
                        });
                        continue;
                    }
                }
            }
            let value = match (constant, &bound_receiver) {
                (Some(name), Some(receiver)) => spanned(Expr::MemberAccess {
                    object: Box::new(receiver.clone()),
                    member: spanned(Identifier(name.to_owned())),
                }),
                _ => {
                    let value = self.eval(frame, &arg.value.node)?;
                    synth.bind(self, &value)?
                }
            };
            bound.push(CallArg { name: None, value });
        }
        let call = Expr::Call {
            callee: Box::new(spanned(callee)),
            args: bound,
        };
        let (bindings, vmad) = synth.vmad();
        Ok((call, bindings, vmad))
    }
}
