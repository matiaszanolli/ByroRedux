//! Natives on base forms that are not references: `Sound`, `Weather`, `ConditionForm`.
//!
//! One family of the native hub in [`super`] (#5800): the run-time read
//! and the ahead-of-time check of the same calls.

use super::*;

/// The method names this family reads (gating the receiver's evaluation).
pub(super) const READS: &[&str] = &[
    "getclassification",
    "istrue",
    "play",
];

impl Vm<'_> {
    /// The base-form reads of [`Vm::object_read`].
    pub(in crate::fragment::vm) fn read_forms(
        &mut self,
        frame: &mut Frame,
        owner: &Value,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (owner, name) {
            // CK *Play - Sound*: "Plays the sound from the location of the
            // specified object reference." The call returns an instance id: 1 when
            // the sound was queued, 0 when it could not be (the check's note).
            (Value::Form(sound), "play") if args.len() == 1 => {
                let source = match self.eval(frame, &args[0].value.node)? {
                    Value::Form(form) if form != 0 => Some(form),
                    Value::Alias { quest, alias } => self
                        .deferred
                        .scene_actor_bindings
                        .resolve(QuestFormId(quest), i32::from(alias))
                        .and_then(|entity| self.entity_form(entity)),
                    _ => None,
                };
                if let Some(source) = source {
                    self.deferred.sound_plays.push(crate::SoundPlay { sound: *sound, source });
                }
                Some(Value::Int(i32::from(source.is_some())))
            }
            // CK *GetClassification - Weather*: -1 none, 0 Pleasant, 1 Cloudy,
            // 2 Rainy, 3 Snow.
            (Value::Form(weather), "getclassification") if args.is_empty() => Some(Value::Int(
                self.world
                    .try_resource::<crate::WeatherClassifications>()
                    .map_or(-1, |table| table.of(*weather)),
            )),
            // Starfield `ConditionForm.IsTrue(akRefObject, akTargetObject)`: the
            // form's conditions with the first reference as subject and the
            // second as target; see [`crate::condition_form`].
            (Value::Form(form), "istrue")
                if args.len() == 2
                    && self.world.try_resource::<crate::ConditionForms>().is_some_and(|forms| forms.contains(*form)) =>
            {
                let values = self.eval_args(frame, args)?;
                let entity = |value: &Value| match value {
                    Value::Form(reference) => self.entity_of(*reference),
                    _ => None,
                };
                let Some(subject) = entity(&values[0]) else {
                    return stop("IsTrue without a loaded reference");
                };
                let target = entity(&values[1]);
                match crate::condition_form_is_true(self.world, *form, subject, target) {
                    Some(result) => Some(Value::Bool(result)),
                    None => return stop("IsTrue on a condition form the engine cannot fully evaluate"),
                }
            }
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// The base-form calls [`Checker::check_call`] lets run.
    pub(in crate::fragment::vm) fn check_forms(
        &mut self,
        receiver_kind: Kind,
        name: &str,
        arg_kinds: &[Arg],
        _statement: bool,
    ) -> Option<Kind> {
        match (receiver_kind, name) {
            (Kind::Object, "istrue")
                if matches!(
                    arg_kinds,
                    [Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit), Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit)]
                ) =>
            {
                return Some(Kind::Bool);
            }
            (Kind::Object, "getclassification") if arg_kinds.is_empty() => return Some(Kind::Num),
            _ => {}
        }
        None
    }
}
