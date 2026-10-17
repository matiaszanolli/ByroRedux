//! `Game` / `Utility` / `Weather` / `Math` / `Debug`: the global-function
//! classes a script calls by class name, never on a value (#5800).

use super::*;

/// The `Math` natives the VM computes itself.
pub(in crate::fragment::vm) const MATH_NATIVES: [&str; 8] = ["min", "max", "abs", "floor", "ceiling", "sqrt", "pow", "sin"];

/// `Math.<name>(values)`, by lowercase name (CK *Math Script* pages; `Min` /
/// `Max` are Fallout 4's). `None` arity or kind the VM cannot compute.
pub(in crate::fragment::vm) fn math_native(name: &str, values: &[Value]) -> Run<Option<Value>> {
    let mut numbers = Vec::with_capacity(values.len());
    for value in values {
        let Some(number) = value.number() else {
            return stop(format!("Math.{name} on {}", value.text()));
        };
        numbers.push(number);
    }
    let arity = if matches!(name, "min" | "max" | "pow") { 2 } else { 1 };
    if numbers.len() != arity {
        return stop(format!("Math.{name} takes {arity} argument(s)"));
    }
    let a = numbers[0];
    Ok(Some(match name {
        "min" => Value::Float(a.min(numbers[1])),
        "max" => Value::Float(a.max(numbers[1])),
        "abs" => Value::Float(a.abs()),
        "floor" => Value::Int(a.floor() as i32),
        "ceiling" => Value::Int(a.ceil() as i32),
        "sqrt" => Value::Float(a.sqrt()),
        "pow" => Value::Float(a.powf(numbers[1])),
        // Papyrus trigonometry takes degrees.
        "sin" => Value::Float(a.to_radians().sin()),
        _ => return Ok(None),
    }))
}

impl Vm<'_> {
    /// A static-class read (`Game.GetPlayer()`, `Utility.RandomFloat()`).
    pub(in crate::fragment::vm) fn read_static_class(
        &mut self,
        frame: &mut Frame,
        class: &str,
        name: &str,
        args: &[CallArg],
    ) -> Run<Option<Value>> {
        Ok(match (class, name) {
            ("game", "getplayer") => Some(Value::Form(PLAYER_REF)),
            // CK *ShakeCamera - Game*: shakes the camera "from the specified
            // object's location" (None: the player's) with `afStrength`
            // (default 0.5, clamped to 0..1) for `afDuration` seconds, where 0
            // (the default) means the `fCameraShakeTime` game setting - 1.25 in
            // SkyrimSE.exe's table (no plugin sets it). The camera subsystem
            // drains the player's `CameraShakeCommand`; the source's distance
            // is not modelled.
            ("game", "shakecamera") if args.len() <= 3 => {
                let values = self.eval_args(frame, args)?;
                let strength = values.get(1).and_then(Value::number).unwrap_or(0.5).clamp(0.0, 1.0);
                let duration = values.get(2).and_then(Value::number).unwrap_or(0.0);
                self.deferred.camera_shakes.push(crate::papyrus_demo::CameraShakeCommand {
                    intensity: strength,
                    duration_secs: if duration > 0.0 { duration } else { 1.25 },
                });
                Some(Value::None)
            }
            // CK *ShakeController - Game*: both motors clamped to 0..1, for the
            // duration; the input subsystem drains `ControllerRumbleCommand`.
            ("game", "shakecontroller") if args.len() == 3 => {
                let values = self.eval_args(frame, args)?;
                let motor = |i: usize| values.get(i).and_then(Value::number).unwrap_or(0.0).clamp(0.0, 1.0);
                self.deferred.controller_shakes.push(crate::papyrus_demo::ControllerRumbleCommand {
                    left: motor(0),
                    right: motor(1),
                    duration_secs: values.get(2).and_then(Value::number).unwrap_or(0.0).max(0.0),
                });
                Some(Value::None)
            }
            // CK *GetCaps - Game*: "the MiscObject representing caps".
            // Fallout 4's is `Caps001`, form `0x0F` of `Fallout4.esm`
            // (checked against the record); no other game's page or
            // record is the one asked for.
            ("game", "getcaps") if args.is_empty() => {
                let fallout4 = self
                    .world
                    .try_resource::<crate::ScriptGame>()
                    .is_some_and(|game| game.0 == byroredux_plugin::esm::reader::GameKind::Fallout4);
                if !fallout4 {
                    return stop("Game.GetCaps outside Fallout 4");
                }
                Some(Value::Form(FO4_CAPS))
            }
            // `Game.GetCredits()` (Starfield, no CK page): the corpus uses it
            // as the currency *form* — `Player.RemoveItem(Game.GetCredits() as
            // Form, cost)` in `DialogueShipServicesScript` — the Starfield
            // twin of `GetCaps`. The form is `Credits`, `0x0F` of
            // `Starfield.esm` (checked against the record).
            ("game", "getcredits") if args.is_empty() => {
                let starfield = self
                    .world
                    .try_resource::<crate::ScriptGame>()
                    .is_some_and(|game| game.0 == byroredux_plugin::esm::reader::GameKind::Starfield);
                if !starfield {
                    return stop("Game.GetCredits outside Starfield");
                }
                Some(Value::Form(STARFIELD_CREDITS))
            }
            // CK *GetForm - Game*: the form of that ID, or None when the
            // ID names no loaded plugin.
            ("game", "getform") => {
                let values = self.eval_args(frame, args)?;
                let Some(Value::Int(id)) = values.first() else {
                    return stop("GetForm without an Int form id");
                };
                let id = *id as u32;
                let valid = id != 0
                    && self
                        .world
                        .try_resource::<crate::load_order::LoadOrderIdentity>()
                        .is_some_and(|identity| identity.pair(id).is_some());
                Some(if valid { Value::Form(id) } else { Value::None })
            }
            // CK *GetCurrentWeather - Weather*.
            ("weather", "getcurrentweather") => Some(
                self.world
                    .try_resource::<crate::CurrentWeather>()
                    .and_then(|weather| weather.0)
                    .map_or(Value::None, Value::Form),
            ),
            ("math", function) if MATH_NATIVES.contains(&function) => {
                let values = self.eval_args(frame, args)?;
                math_native(function, &values)?
            }
            // CK *RandomFloat - Utility*: "a random floating point number
            // between the minimum and maximum values (inclusive)"; the
            // defaults are 0.0 and 1.0.
            ("utility", "randomfloat") => {
                let values = self.eval_args(frame, args)?;
                let bound = |index: usize, default: f32| values.get(index).and_then(Value::number).unwrap_or(default);
                let (low, high) = (bound(0, 0.0), bound(1, 1.0));
                let Some(draw) = self
                    .world
                    .try_resource_mut::<crate::story_manager::StoryManagerRng>()
                    .map(|mut rng| rng.unit_inclusive())
                else {
                    return stop("Utility.RandomFloat without the random generator");
                };
                Some(Value::Float(low + draw * (high - low)))
            }
            // CK *GetCurrentGameTime - Utility*: "game days passed".
            // CK *GetCurrentRealTime - Utility*: "the number of real-world
            // seconds that have elapsed since the game has launched" (time in
            // a menu counts). `TotalTime` is the engine's wall-clock since
            // start.
            ("utility", "getcurrentrealtime") => {
                let seconds = self.world.try_resource::<byroredux_core::ecs::TotalTime>().map_or(0.0, |t| t.0);
                Some(Value::Float(seconds))
            }
            ("utility", "getcurrentgametime") => {
                let hours = self
                    .world
                    .try_resource::<crate::fragment::ScriptUpdates>()
                    .and_then(|updates| updates.game_hours())
                    .unwrap_or(0.0);
                Some(Value::Float((hours / 24.0) as f32))
            }
            ("game", "getformfromfile") => {
                let values = self.eval_args(frame, args)?;
                // The id exactly: an f32 (`Value::number`) cannot hold
                // every 32-bit form id.
                let form_id = match values.first() {
                    Some(Value::Int(id)) => *id as u32,
                    _ => return stop("GetFormFromFile without an Int form id"),
                };
                let Some(Value::Str(file)) = values.get(1) else {
                    return stop("GetFormFromFile without a file");
                };
                let plugin = byroredux_core::form_id::PluginId::from_filename(file);
                let form = self
                    .world
                    .try_resource::<crate::load_order::LoadOrderIdentity>()
                    .and_then(|identity| identity.form_from_file(form_id as u32, plugin));
                Some(form.map_or(Value::None, Value::Form))
            }
            _ => None,
        })
    }
}

impl Checker<'_> {
    /// A static-class call: the ones the VM computes, else the primitives'
    /// classification over placeholders.
    pub(in crate::fragment::vm) fn check_static_class(
        &mut self,
        receiver: &Expr,
        class: &str,
        name: &str,
        arg_kinds: &[Arg],
        statement: bool,
    ) -> Option<Kind> {
        if class == "game" && name == "getplayer" {
            return Some(Kind::Player);
        }
        let numeric_arg = |a: &Arg| matches!(a, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num));
        if class == "game"
            && name == "shakecamera"
            && statement
            && arg_kinds.len() <= 3
            && arg_kinds.first().is_none_or(|a| {
                matches!(a, Arg::Kind(Kind::Object | Kind::Player | Kind::None) | Arg::Literal(Expr::NoneLit))
            })
            && arg_kinds.iter().skip(1).all(numeric_arg)
        {
            return Some(Kind::None);
        }
        if class == "game" && name == "shakecontroller" && statement && arg_kinds.len() == 3 && arg_kinds.iter().all(numeric_arg) {
            return Some(Kind::None);
        }
        if class == "game"
            && name == "stopdialoguecamera"
            && statement
            && arg_kinds.len() <= 2
            && arg_kinds.iter().all(|a| matches!(a, Arg::Literal(Expr::BoolLit(_)) | Arg::Kind(Kind::Bool)))
        {
            return Some(Kind::None);
        }
        if class == "game" && matches!(name, "getcaps" | "getcredits") && arg_kinds.is_empty() {
            return Some(Kind::Object);
        }
        if class == "game"
            && name == "getformfromfile"
            && matches!(arg_kinds, [Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num), Arg::Literal(Expr::StringLit(_)) | Arg::Kind(Kind::Str)])
        {
            return Some(Kind::Object);
        }
        if class == "debug" && (name.starts_with("trace") || name == "openuserlog" || name == "closeuserlog") && statement {
            return Some(Kind::None);
        }
        if class == "game"
            && name == "getform"
            && matches!(arg_kinds, [Arg::Literal(Expr::IntLit(_)) | Arg::Kind(Kind::Num)])
        {
            return Some(Kind::Object);
        }
        if class == "weather" && name == "getcurrentweather" && arg_kinds.is_empty() {
            return Some(Kind::Object);
        }
        if class == "math" && MATH_NATIVES.contains(&name) {
            let numeric = |arg: &Arg| matches!(arg, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num));
            let arity = if matches!(name, "min" | "max" | "pow") { 2 } else { 1 };
            if arg_kinds.len() == arity && arg_kinds.iter().all(numeric) {
                return Some(Kind::Num);
            }
            return self.fail(|| format!("Math.{name} as written"));
        }
        if class == "utility"
            && name == "randomfloat"
            && arg_kinds.len() <= 2
            && arg_kinds.iter().all(|a| matches!(a, Arg::Literal(Expr::IntLit(_) | Expr::FloatLit(_)) | Arg::Kind(Kind::Num)))
        {
            return Some(Kind::Num);
        }
        if class == "utility" && matches!(name, "getcurrentgametime" | "getcurrentrealtime") && arg_kinds.is_empty() {
            return Some(Kind::Num);
        }
        if class == "utility" && name == "wait" {
            if statement && matches!(arg_kinds, [Arg::Literal(_)] | [Arg::Kind(Kind::Num)]) {
                self.waits = true;
                return Some(Kind::None);
            }
            return self.fail(|| "Utility.Wait called for its value".into());
        }
        let result = self.check_native(receiver.clone(), Kind::None, &name, &arg_kinds, statement);
        if result.is_none() {
            return self.fail(|| {
                // A literal string argument names what was asked for
                // (`Game.QueryStat("Bribes")`), which the census groups by.
                let named = arg_kinds.iter().find_map(|arg| match arg {
                    Arg::Literal(Expr::StringLit(text)) => Some(format!(" \"{text}\"")),
                    _ => None,
                });
                format!("native {class}.{name}/{}{}", arg_kinds.len(), named.unwrap_or_default())
            });
        }
        result
    }
}
