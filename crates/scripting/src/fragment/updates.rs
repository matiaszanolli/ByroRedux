//! M47.2 — Papyrus update registrations: `RegisterForUpdate`,
//! `RegisterForSingleUpdate`, their `GameTime` twins and the `Unregister`
//! pair, delivering `OnUpdate` / `OnUpdateGameTime` to the script that
//! registered — and Fallout 4's timers, `StartTimer(afInterval, aiTimerID)`
//! / `StartTimerGameTime` and their `Cancel` pair, delivering
//! `OnTimer(aiTimerID)` / `OnTimerGameTime(aiTimerID)`.
//!
//! The rules follow the CK's *RegisterForUpdate - Form* family:
//! a periodic registration replaces the script's earlier periodic one, a
//! single one its earlier single one, and the two do not interfere;
//! `UnregisterForUpdate` cancels both; a quest's registrations end when the
//! quest stops. Real-time intervals count the frames' `dt` (the simulation
//! does not advance in menu mode); game-time intervals count game hours on
//! the absolute clock the host reports, so sleeping and waiting deliver the
//! update they pass.
//!
//! The FO4 CK's *StartTimer - ScriptObject* family: a timer is one-shot; its
//! ID is unique per script, and real-time and game-time IDs do not conflict;
//! starting a timer whose ID is already ticking resets it; a game-time
//! interval under 0.033 hours (2 minutes) is rounded up to it.
//!
//! Registrations are per script instance — the object the script is
//! attached to and the script — and the update goes to that script only.
//! The Papyrus VM check accepts a registration only when the script's
//! handler runs whole, so a delivery never meets an unchecked body. A
//! reference's update waits while the reference is unloaded and is
//! delivered when it loads again.

use super::*;

/// Which clock an update registration counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub enum UpdateClock {
    /// Seconds of simulation (`OnUpdate`).
    RealTime,
    /// Game hours (`OnUpdateGameTime`).
    GameTime,
}

impl UpdateClock {
    fn event(self, timer: bool) -> &'static str {
        match (self, timer) {
            (UpdateClock::RealTime, false) => "OnUpdate",
            (UpdateClock::GameTime, false) => "OnUpdateGameTime",
            (UpdateClock::RealTime, true) => "OnTimer",
            (UpdateClock::GameTime, true) => "OnTimerGameTime",
        }
    }
}

/// FO4 *StartTimerGameTime*: "Any value smaller then 0.033 (2 minutes) will
/// be rounded up to that value."
const MIN_GAME_TIMER_HOURS: f32 = 0.033;

/// One script instance's registration.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct UpdateRegistration {
    /// The object the script is attached to: a quest or a reference.
    pub owner: u32,
    pub script: String,
    pub clock: UpdateClock,
    /// `RegisterForUpdate` (periodic) rather than `RegisterForSingleUpdate`.
    pub recurring: bool,
    /// A Fallout 4 timer's ID (`StartTimer`); `None` for an update
    /// registration.
    pub timer: Option<i32>,
    /// Seconds or game hours between updates.
    pub interval: f32,
    /// When the next update is due, on `clock`; `None` until the game clock
    /// is first reported, when it is armed from there.
    pub due: Option<f64>,
}

/// A registration change a script run asks for, applied after the quest
/// guards drop.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UpdateRequest {
    Register {
        clock: UpdateClock,
        interval: f32,
        recurring: bool,
    },
    Unregister(UpdateClock),
    /// `StartTimer` / `StartTimerGameTime`.
    StartTimer { clock: UpdateClock, interval: f32, id: i32 },
    /// `CancelTimer` / `CancelTimerGameTime`.
    CancelTimer { clock: UpdateClock, id: i32 },
}

impl UpdateRequest {
    /// The request a `Self.<name>(args)` call makes, by lowercase name, from
    /// its numeric arguments (`None` for one that is not a number).
    pub(super) fn of_call(name: &str, args: &[Option<f32>]) -> Option<Self> {
        let interval = || args.first().copied().flatten();
        // A timer ID: the given argument, or the default 0.
        let id = |index: usize| -> Option<i32> {
            match args.get(index) {
                None => Some(0),
                Some(value) => value.map(|value| value as i32),
            }
        };
        let register = |clock, recurring| {
            Some(UpdateRequest::Register {
                clock,
                interval: interval()?,
                recurring,
            })
        };
        let start = |clock| {
            Some(UpdateRequest::StartTimer {
                clock,
                interval: interval()?,
                id: id(1)?,
            })
        };
        let cancel = |clock| Some(UpdateRequest::CancelTimer { clock, id: id(0)? });
        match name {
            "registerforupdate" => register(UpdateClock::RealTime, true),
            "registerforsingleupdate" => register(UpdateClock::RealTime, false),
            "registerforupdategametime" => register(UpdateClock::GameTime, true),
            "registerforsingleupdategametime" => register(UpdateClock::GameTime, false),
            "unregisterforupdate" => Some(UpdateRequest::Unregister(UpdateClock::RealTime)),
            "unregisterforupdategametime" => Some(UpdateRequest::Unregister(UpdateClock::GameTime)),
            "starttimer" => start(UpdateClock::RealTime),
            "starttimergametime" => start(UpdateClock::GameTime),
            "canceltimer" => cancel(UpdateClock::RealTime),
            "canceltimergametime" => cancel(UpdateClock::GameTime),
            _ => None,
        }
    }

    /// The handler a registration delivers to: what the check must accept
    /// (`""` for a cancellation).
    pub(super) fn handler(name: &str) -> Option<&'static str> {
        match name {
            "registerforupdate" | "registerforsingleupdate" => Some("OnUpdate"),
            "registerforupdategametime" | "registerforsingleupdategametime" => Some("OnUpdateGameTime"),
            "starttimer" => Some("OnTimer"),
            "starttimergametime" => Some("OnTimerGameTime"),
            "unregisterforupdate" | "unregisterforupdategametime" | "canceltimer" | "canceltimergametime" => {
                Some("")
            }
            _ => None,
        }
    }

    /// How many numeric arguments the call takes, at most.
    pub(super) fn max_args(name: &str) -> usize {
        match name {
            "starttimer" | "starttimergametime" => 2,
            "canceltimer" | "canceltimergametime" | "registerforupdate" | "registerforsingleupdate"
            | "registerforupdategametime" | "registerforsingleupdategametime" => 1,
            _ => 0,
        }
    }
}

/// Every script instance's update registrations, and the clocks they count.
/// Saved.
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct ScriptUpdates {
    registrations: Vec<UpdateRegistration>,
    /// Simulation seconds counted so far.
    seconds: f64,
    /// The game clock (hours since the runtime began) last reported.
    game_hours: Option<f64>,
}

impl Resource for ScriptUpdates {}

impl ScriptUpdates {
    fn now(&self, clock: UpdateClock) -> Option<f64> {
        match clock {
            UpdateClock::RealTime => Some(self.seconds),
            UpdateClock::GameTime => self.game_hours,
        }
    }

    /// Apply `request` from `owner`'s instance of `script`.
    pub fn request(&mut self, owner: u32, script: &str, request: UpdateRequest) {
        let mine = |r: &UpdateRegistration, clock: UpdateClock| {
            r.owner == owner && r.clock == clock && r.script.eq_ignore_ascii_case(script)
        };
        match request {
            UpdateRequest::Register {
                clock,
                interval,
                recurring,
            } => {
                self.registrations
                    .retain(|r| !(mine(r, clock) && r.timer.is_none() && r.recurring == recurring));
                let interval = interval.max(0.0);
                self.registrations.push(UpdateRegistration {
                    owner,
                    script: script.to_owned(),
                    clock,
                    recurring,
                    timer: None,
                    interval,
                    due: self.now(clock).map(|now| now + f64::from(interval)),
                });
            }
            UpdateRequest::Unregister(clock) => self.registrations.retain(|r| !(mine(r, clock) && r.timer.is_none())),
            UpdateRequest::StartTimer { clock, interval, id } => {
                self.registrations.retain(|r| !(mine(r, clock) && r.timer == Some(id)));
                let interval = match clock {
                    UpdateClock::RealTime => interval.max(0.0),
                    UpdateClock::GameTime => interval.max(MIN_GAME_TIMER_HOURS),
                };
                self.registrations.push(UpdateRegistration {
                    owner,
                    script: script.to_owned(),
                    clock,
                    recurring: false,
                    timer: Some(id),
                    interval,
                    due: self.now(clock).map(|now| now + f64::from(interval)),
                });
            }
            UpdateRequest::CancelTimer { clock, id } => {
                self.registrations.retain(|r| !(mine(r, clock) && r.timer == Some(id)))
            }
        }
    }

    /// The game clock (hours since the runtime began) the host last
    /// reported; `None` before the first report.
    pub fn game_hours(&self) -> Option<f64> {
        self.game_hours
    }

    pub fn registrations(&self) -> &[UpdateRegistration] {
        &self.registrations
    }

    /// Advance the clocks and return the indices of the registrations now due.
    /// The registrations themselves are untouched (a due one has its first
    /// deadline fixed), so the caller can decide each one's [`Hold`] without
    /// this resource locked, then [`settle`](Self::settle) them.
    fn advance(&mut self, dt: f32, game_hours: Option<f64>) -> Vec<usize> {
        self.seconds += f64::from(dt.max(0.0));
        if let Some(hours) = game_hours.filter(|hours| hours.is_finite()) {
            self.game_hours = Some(hours);
        }
        let (seconds, game_hours) = (self.seconds, self.game_hours);
        let mut due = Vec::new();
        for (index, r) in self.registrations.iter_mut().enumerate() {
            let now = match r.clock {
                UpdateClock::RealTime => seconds,
                UpdateClock::GameTime => match game_hours {
                    Some(hours) => hours,
                    None => continue,
                },
            };
            let due_at = *r.due.get_or_insert(now + f64::from(r.interval));
            if due_at <= now {
                due.push(index);
            }
        }
        due
    }

    /// Apply each due registration's `hold` (indices from [`advance`](Self::advance),
    /// ascending): a dropped one is removed, a waiting one stays due for a later
    /// tick, and a delivered one is returned — removed when one-shot, re-armed
    /// when periodic.
    fn settle(&mut self, holds: &[(usize, Hold)]) -> Vec<UpdateRegistration> {
        let (seconds, game_hours) = (self.seconds, self.game_hours);
        let mut delivered = Vec::new();
        let mut removed = std::collections::HashSet::new();
        for &(index, hold) in holds {
            let Some(r) = self.registrations.get_mut(index) else {
                continue;
            };
            match hold {
                Hold::Drop => {
                    removed.insert(index);
                }
                Hold::Wait => {}
                Hold::Deliver => {
                    delivered.push(r.clone());
                    if !r.recurring {
                        removed.insert(index);
                        continue;
                    }
                    let now = match r.clock {
                        UpdateClock::RealTime => seconds,
                        UpdateClock::GameTime => game_hours.unwrap_or(seconds),
                    };
                    // One update per tick, however far the clock jumped.
                    r.due = Some(now + f64::from(r.interval.max(f32::EPSILON)));
                }
            }
        }
        if !removed.is_empty() {
            let mut index = 0;
            self.registrations.retain(|_| {
                let keep = !removed.contains(&index);
                index += 1;
                keep
            });
        }
        delivered
    }
}

/// What a due registration does this tick.
#[derive(Debug, Clone, Copy)]
enum Hold {
    Deliver,
    /// Its reference is unloaded: deliver when it loads.
    Wait,
    /// Its quest stopped.
    Drop,
}

/// Per (script, handler, on a quest): whether the Papyrus VM runs the
/// handler, checked at its first delivery against the installed scripts.
/// Session-only.
#[derive(Debug, Default)]
pub struct UpdateHandlerChecks(HashMap<(String, &'static str, super::vm::SelfKind), bool>);

impl Resource for UpdateHandlerChecks {}

/// Deliver the update events now due. `game_hours` is the host's absolute
/// game clock in hours, when it has one.
///
/// Idle cost is the clock advance plus one pass over the registrations
/// (#5763): nothing else is read until a registration is due, and a due one
/// finds its reference through [`ReferenceScriptIndex`] instead of a map of
/// every attached script rebuilt each frame.
pub fn script_update_system(world: &World, dt: f32, game_hours: Option<f64>) {
    // The registrations that are due, copied out so no lock on `ScriptUpdates`
    // is held while their owners are looked up.
    let due_now: Vec<(usize, UpdateRegistration)> = {
        let Some(mut updates) = world.try_resource_mut::<ScriptUpdates>() else {
            return;
        };
        let due = updates.advance(dt, game_hours);
        due.into_iter().map(|index| (index, updates.registrations[index].clone())).collect()
    };
    if due_now.is_empty() {
        return;
    }
    let owners = super::owners::ScriptOwners::new(world);
    // Each owner's standing: a quest delivers while it runs, a reference while
    // it is loaded. Quest state is read first and released.
    let quest_standing: Vec<Option<bool>> = {
        let stages = world.resource::<QuestStageState>();
        due_now.iter().map(|(_, r)| owners.standing(world, &stages, r.owner)).collect()
    };
    let holds: Vec<(usize, Hold)> = due_now
        .iter()
        .zip(&quest_standing)
        .map(|((index, r), standing)| {
            let hold = match standing {
                Some(true) => Hold::Deliver,
                Some(false) => Hold::Drop,
                None if ReferenceScriptIndex::lookup(world, r.owner).is_some() => Hold::Deliver,
                None => Hold::Wait,
            };
            (*index, hold)
        })
        .collect();
    // A registration the frame's interpreter budget cannot start stays due
    // rather than queueing: a periodic one has already re-armed by the time a
    // queued run would start, so queueing would pile runs up behind a budget
    // the demand exceeds. Held back, it is delivered on the first frame with
    // allowance left, once however many ticks passed — the same coalescing a
    // clock jump gets.
    let mut starts = super::budget::runs_startable(world);
    let holds: Vec<(usize, Hold)> = holds
        .into_iter()
        .map(|(index, hold)| match hold {
            Hold::Deliver if starts == 0 => (index, Hold::Wait),
            Hold::Deliver => {
                starts -= 1;
                (index, Hold::Deliver)
            }
            other => (index, other),
        })
        .collect();
    let due = world.resource_mut::<ScriptUpdates>().settle(&holds);
    if due.is_empty() {
        return;
    }
    let mut advances = Vec::new();
    for registration in due {
        let owner = QuestFormId(registration.owner);
        let event = registration.clock.event(registration.timer.is_some());
        let kind = owners.kind(registration.owner);
        let key = (registration.script.to_ascii_lowercase(), event, kind);
        let cached = world
            .try_resource::<UpdateHandlerChecks>()
            .and_then(|checks| checks.0.get(&key).copied());
        let runs = cached.unwrap_or_else(|| {
            let runs = super::vm::handler_runs(&owners.scripts(), &registration.script, event, kind);
            if let Some(mut checks) = world.try_resource_mut::<UpdateHandlerChecks>() {
                checks.0.insert(key, runs);
            }
            runs
        });
        if !runs {
            log::debug!(
                "{:08X}: {}.{event} skipped: the Papyrus VM does not run it",
                registration.owner,
                registration.script
            );
            continue;
        }
        let vmad = owners.vmad(world, registration.owner, &registration.script);
        let effect = Effect::Interpret {
            script: registration.script.clone(),
            function: event.to_owned(),
            args: registration
                .timer
                .map(|id| vec![crate::translate::effects::VmStored::Int(id)])
                .unwrap_or_default(),
            resume: None,
        };
        log::debug!("{:08X}: {}.{event}", registration.owner, registration.script);
        // The frame-budget gate and the advance flush live in one place
        // (`dispatch`) — this used to be a hand-copied 25-line tail.
        advances.extend(run_papyrus_call(world, owner, vmad.as_ref(), effect));
    }
    flush_advances(world, advances);
}
