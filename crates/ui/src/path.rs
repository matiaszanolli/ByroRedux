//! M48 Phase 2 — the path service (GUI-PATH-001…005, design in `docs/engine/ui.md` § *Path
//! access*).
//!
//! Every open movie, AVM1 or AVM2, carries an injected adapter that registers ExternalInterface
//! callbacks to get, set and invoke a member by its dotted path from the movie root (`_root.HUDMovieBaseInstance.SetHealthMeterPercent`, `LeftMeters_mc.HPMeter_mc.MeterBar_mc.Percent`).
//! The AVM1 adapter is a `DoAction` spliced into frame 1 ([`crate::avm1_path`]); the AVM2 one is
//! more methods in the `BGSCodeObj` adapter ([`crate::avm2_host`]), which also attaches a new
//! instance of one of the movie's exported classes ([`PathOp::Attach`], #7036). Both answer one
//! shape —
//! an object `{found: Boolean, value: *, type: String}` — which [`decode_reply`] turns into a
//! [`PathOutcome`]: an absent path is a typed result with a diagnostic, never a silent no-op
//! (GUI-PATH-002).
//!
//! This is the only engine code that reads, writes or invokes a movie member by path
//! (GUI-PATH-005): the HUD drivers feed the vanilla meters through it, and scripts gain no path
//! access.

use crate::ScaleformValue;

/// `__byro_path_get(path)` — read a member.
pub const PATH_GET_CALLBACK: &str = "__byro_path_get";
/// `__byro_path_set(path, value)` — write a member of an existing parent.
pub const PATH_SET_CALLBACK: &str = "__byro_path_set";
/// `__byro_path_call(path, [args])` — invoke a function member with its parent as `this`.
pub const PATH_CALL_CALLBACK: &str = "__byro_path_call";
/// `__byro_path_attach(path, class)` — add a new instance of an exported display class under the
/// path's parent, named by its last segment (AVM2 only, [`PathOp::Attach`]).
pub const PATH_ATTACH_CALLBACK: &str = "__byro_path_attach";

/// Every path-service callback, in either VM's adapter: the service's surface, not a movie's own
/// push target.
pub const PATH_CALLBACKS: [&str; 4] = [PATH_GET_CALLBACK, PATH_SET_CALLBACK, PATH_CALL_CALLBACK, PATH_ATTACH_CALLBACK];

/// One path operation.
#[derive(Clone, Debug, PartialEq)]
pub enum PathOp {
    Get(String),
    Set(String, ScaleformValue),
    Call(String, Vec<ScaleformValue>),
    /// Make `path` exist: when the parent has no child of the last segment's name, construct the
    /// exported display class `class` (its linkage name, `DirectionMarkerWidget`), name it, and
    /// `addChild` it to the parent. A child already there is found as-is, so the operation is
    /// idempotent. #7036 — how the engine stands in for native code that instantiates a movie's
    /// widgets (Fallout 4's compass direction markers). The AVM2 adapter only: an AVM1 movie
    /// answers [`PathOutcome::NotReady`], and reaches the same through
    /// `Call("<parent>.attachMovie", [linkage, name, depth])`.
    Attach(String, String),
}

impl PathOp {
    pub fn path(&self) -> &str {
        match self {
            Self::Get(path) | Self::Set(path, _) | Self::Call(path, _) | Self::Attach(path, _) => path,
        }
    }

    /// The adapter callback and its arguments.
    pub(crate) fn callback(&self) -> (&'static str, Vec<ScaleformValue>) {
        match self {
            Self::Get(path) => (PATH_GET_CALLBACK, vec![ScaleformValue::String(path.clone())]),
            Self::Set(path, value) => (PATH_SET_CALLBACK, vec![ScaleformValue::String(path.clone()), value.clone()]),
            Self::Call(path, args) => {
                (PATH_CALL_CALLBACK, vec![ScaleformValue::String(path.clone()), ScaleformValue::List(args.clone())])
            }
            Self::Attach(path, class) => {
                (PATH_ATTACH_CALLBACK, vec![ScaleformValue::String(path.clone()), ScaleformValue::String(class.clone())])
            }
        }
    }
}

/// What to do with an operation whose target is not there yet (GUI-PATH-003), stated per call.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhenAbsent {
    /// Keep it and retry every tick until the target appears; a newer operation on the same
    /// path replaces it (the last value wins, as a pushed meter value should).
    Queue,
    /// Answer [`PathOutcome::Absent`] / [`PathOutcome::NotReady`] now.
    Refuse,
}

/// What a path operation found.
#[derive(Clone, Debug, PartialEq)]
pub enum PathOutcome {
    /// The path resolved: the value read, the invoked function's return value, or `Undefined`
    /// for a set.
    Found(ScaleformValue),
    /// The movie has no member at that path (GUI-PATH-002).
    Absent,
    /// The movie's adapter is not registered yet (frame 1 has not run, or the AVM2 lifecycle
    /// class has not been constructed) or the movie carries none.
    NotReady,
    /// Held for a retry ([`WhenAbsent::Queue`]).
    Queued,
}

/// Decode an adapter reply. `None` (the callback is not registered) is [`PathOutcome::NotReady`];
/// a reply that is not the adapter's object shape is treated as absent.
pub(crate) fn decode_reply(reply: Option<ScaleformValue>) -> PathOutcome {
    let Some(reply) = reply else {
        return PathOutcome::NotReady;
    };
    let ScaleformValue::Object(fields) = reply else {
        return PathOutcome::Absent;
    };
    match fields.get("found") {
        Some(ScaleformValue::Bool(true)) => {
            PathOutcome::Found(fields.get("value").cloned().unwrap_or(ScaleformValue::Undefined))
        }
        _ => PathOutcome::Absent,
    }
}

/// Operations held under [`WhenAbsent::Queue`], at most this many per movie; the oldest goes when
/// a new path would exceed it.
pub(crate) const MAX_QUEUED_PATH_OPS: usize = 64;

/// Distinct absent paths reported per movie before the diagnostic goes quiet (the path string is
/// the engine's, but a driver can ask about many).
pub(crate) const MAX_REPORTED_ABSENT_PATHS: usize = 256;

/// A movie's queued operations and absent-path diagnostics.
#[derive(Debug, Default)]
pub(crate) struct PathQueue {
    pending: Vec<PathOp>,
    reported_absent: std::collections::HashSet<String>,
}

impl PathQueue {
    /// Hold `op`, replacing a pending operation on the same path.
    pub(crate) fn hold(&mut self, op: PathOp) {
        self.pending.retain(|pending| pending.path() != op.path());
        if self.pending.len() >= MAX_QUEUED_PATH_OPS {
            self.pending.remove(0);
        }
        self.pending.push(op);
    }

    pub(crate) fn take(&mut self) -> Vec<PathOp> {
        std::mem::take(&mut self.pending)
    }

    pub(crate) fn len(&self) -> usize {
        self.pending.len()
    }

    /// Report an absent path once (GUI-PATH-002's diagnostic).
    pub(crate) fn report_absent(&mut self, menu: &str, op: &PathOp) {
        if self.reported_absent.len() >= MAX_REPORTED_ABSENT_PATHS || !self.reported_absent.insert(op.path().to_owned()) {
            return;
        }
        log::warn!("Scaleform menu '{menu}': path '{}' is absent ({})", op.path(), match op {
            PathOp::Get(_) => "get",
            PathOp::Set(..) => "set",
            PathOp::Call(..) => "call",
            PathOp::Attach(..) => "attach",
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_decode_to_typed_outcomes() {
        let object = |found: bool, value: Option<ScaleformValue>| {
            let mut fields = std::collections::BTreeMap::new();
            fields.insert("found".to_owned(), ScaleformValue::Bool(found));
            if let Some(value) = value {
                fields.insert("value".to_owned(), value);
            }
            Some(ScaleformValue::Object(fields))
        };
        assert_eq!(decode_reply(None), PathOutcome::NotReady);
        assert_eq!(decode_reply(object(false, None)), PathOutcome::Absent);
        assert_eq!(decode_reply(object(true, Some(ScaleformValue::Number(7.0)))), PathOutcome::Found(ScaleformValue::Number(7.0)));
        assert_eq!(decode_reply(object(true, None)), PathOutcome::Found(ScaleformValue::Undefined));
        assert_eq!(decode_reply(Some(ScaleformValue::Null)), PathOutcome::Absent, "not the adapter's shape");
    }

    #[test]
    fn a_queued_path_keeps_only_its_latest_operation() {
        let mut queue = PathQueue::default();
        queue.hold(PathOp::Set("a.b".into(), ScaleformValue::Number(1.0)));
        queue.hold(PathOp::Set("a.c".into(), ScaleformValue::Number(2.0)));
        queue.hold(PathOp::Set("a.b".into(), ScaleformValue::Number(3.0)));
        assert_eq!(queue.take(), vec![PathOp::Set("a.c".into(), ScaleformValue::Number(2.0)), PathOp::Set("a.b".into(), ScaleformValue::Number(3.0))]);
        for i in 0..(MAX_QUEUED_PATH_OPS + 5) {
            queue.hold(PathOp::Get(format!("p{i}")));
        }
        assert_eq!(queue.len(), MAX_QUEUED_PATH_OPS);
    }
}
