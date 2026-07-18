//! `PACK` AI package records — 30-procedure scheduling system (guard
//! patrols, merchant behavior, dialogue triggers, ambient idles).

use super::super::common::read_zstring;
use super::super::condition::{parse_ctda, remap_condition_form_ids, ConditionList};
use crate::esm::reader::{GameKind, SubRecord};
use crate::esm::sub_reader::SubReader;

/// `PACK` AI package record. 30-procedure scheduling system
/// (guard patrols, merchant behavior, dialogue triggers, ambient
/// idles). `NpcRecord.ai_packages` carries PKID form refs; pre-#446
/// those dangled.
///
/// PKDT (package flags + procedure type), PSDT (schedule), and PLDT
/// (location) are captured here. PTDT / PKTG(Skyrim+) / PKCU / PKPA
/// decoding lands with the AI runtime per the `ai_packages_procedures.md`
/// memo. Layout verified against the FO3/FNV xEdit-derived spec:
/// <https://tes5edit.github.io/fopdoc/FalloutNV/Records/PACK.html>.
#[derive(Debug, Clone, Default)]
pub struct PackRecord {
    pub form_id: u32,
    pub editor_id: String,
    /// Flags bitfield from PKDT (schedule / location repeat / weapon
    /// draw / etc.). Low 16 bits on FO3/FNV, u32 on Skyrim+.
    pub package_flags: u32,
    /// Procedure type — the FO3/FNV package-type enum (0..=16):
    /// 0 Find, 1 Follow, 2 Escort, 3 Eat, 4 Sleep, 5 Wander, 6 Travel,
    /// 7 Accompany, 8 UseItemAt, 9 Ambush, 10 FleeNotCombat,
    /// 11 CastMagic, 12 **Sandbox**, 13 Patrol, 14 Guard, 15 Dialogue,
    /// 16 UseWeapon. Read as a single **byte** at PKDT offset 4.
    pub procedure_type: u32,
    /// Schedule from PSDT (FO3/FNV). `None` when the package has no PSDT
    /// (treated as always-active). Drives which package is *active* at a
    /// given game hour — the M42.1 seat-assignment selector.
    pub schedule: Option<PackSchedule>,
    /// Authored activity center from PLDT. `None` when the package has
    /// no PLDT (rare — most FO3/FNV packages carry one). This is the
    /// Sandbox procedure's "Location" parameter (param #1 of 15 per the
    /// `ai_packages_procedures.md` memo) — the real center a Sandbox
    /// package should search around, replacing the v0 actor-position
    /// approximation in `sandbox_seat_system`.
    pub location: Option<PackLocation>,
    /// Target actor/object from PTDT (M42.5). `None` when the package has
    /// no PTDT — most procedures don't need one; Follow/Escort-family
    /// procedures do. `PTD2` (a second target, for two-target procedures
    /// like Escort-someone-to-someone) is not decoded — no implemented
    /// procedure needs it yet.
    pub target: Option<PackTarget>,
    /// `CTDA` conditions gating whether this package is eligible at all
    /// (M42.2). Empty = unconditionally eligible (Bethesda's "no
    /// conditions = always fires"). The plugin crate only *carries* these
    /// — evaluation lives in `byroredux_scripting`'s M47.1 evaluator, so
    /// [`active_package`] and friends take a caller-supplied
    /// `condition_met` predicate rather than reaching across the crate
    /// boundary (scripting depends on plugin, not the reverse).
    pub conditions: ConditionList,
}

/// PACK location data from PLDT — where a package's activity centers.
/// FormIDs in [`PackLocationTarget`] are plugin-local at parse time; the
/// caller must remap them the same way `parse_pack` does internally
/// (via the `FormIdRemap` threaded through `extract_records`, #1666).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackLocation {
    /// Raw Location Type enum (0..=7) — kept alongside `target` so a
    /// consumer can distinguish `Other` variants without re-deriving it.
    pub location_type: u32,
    pub target: PackLocationTarget,
    /// Search radius (game units) around `target`.
    pub radius: i32,
}

/// The `union` half of PLDT — its meaning depends on the Location Type
/// enum. Only types 0/1/4 carry a resolvable FormID per the FO3/FNV spec;
/// types 2/3/6/7 (Near Current Location / Near Editor Location / Near
/// Linked Reference / At Package Location) are self-referential to the
/// runtime actor or package and carry no FormID to look up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PackLocationTarget {
    /// Type 0 — Near Reference. FormID of a REFR/PGRE/PMIS/ACHR/ACRE/PLYR.
    NearReference(u32),
    /// Type 1 — In Cell. FormID of a CELL.
    InCell(u32),
    /// Type 4 — Object ID. FormID of a base-object record (ACTI/DOOR/
    /// STAT/FURN/CREA/SPEL/NPC_/CONT/ARMO/AMMO/MISC/WEAP/BOOK/KEYM/
    /// ALCH/LIGH/…).
    ObjectId(u32),
    /// Types 2 (Near Current Location), 3 (Near Editor Location), 5
    /// (Object Type — an enum value, not a FormID), 6 (Near Linked
    /// Reference), 7 (At Package Location). The raw union bytes are kept
    /// but not interpreted as a FormID.
    Other(u32),
}

/// PACK target data from PTDT (M42.5) — layout verified against
/// <https://tes5edit.github.io/fopdoc/FalloutNV/Records/PACK.html> (the
/// same xEdit-derived reference the PLDT/PSDT arms below already cite),
/// cross-checked against those two *already-implemented* layouts before
/// trusting it for a brand-new sub-record: PSDT's fetched Month/DayOfWeek/
/// Date/Time(@3)/Duration(@4) layout and PLDT's fetched Type/union/Radius
/// (12 bytes) layout both matched this codebase's existing, tested decode
/// exactly. PTDT is the same fixed-width-union shape as PLDT (type plus
/// union plus one more field), 16 bytes: Type u32 @0, Target union u32 @4,
/// Count/Distance i32 @8, and a trailing Unknown f32 @12 that has no known
/// consumer anywhere and isn't stored (same convention PSDT's unused
/// Month/DayOfWeek/Date bytes already established).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PackTarget {
    /// Raw Target Type enum (0..=3) — kept alongside `target` so a
    /// consumer can distinguish `Other` without re-deriving it.
    pub target_type: u32,
    pub target: PackTargetKind,
    /// Count or distance (game units) — meaning is procedure-dependent
    /// per the source spec (undocumented generically); Follow interprets
    /// it as a stand-off distance.
    pub count_or_distance: i32,
}

/// The `union` half of PTDT. Only types 0/1 carry a resolvable FormID,
/// mirroring [`PackLocationTarget`]'s exact precedent of only naming the
/// FormID-carrying variants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PackTargetKind {
    /// Type 0 — Specific Reference. FormID of a REFR/PGRE/PMIS/ACHR/ACRE/PLYR.
    SpecificReference(u32),
    /// Type 1 — Object ID. FormID of a base-object record.
    ObjectId(u32),
    /// Type 2 (Object Type — an enum value, not a FormID) or type 3
    /// (Linked Reference — undocumented, "??" in the source spec). The
    /// raw union bytes are kept but not interpreted as a FormID.
    Other(u32),
}

/// FO3/FNV package procedure-type index for `Sandbox` — idle activities
/// in an area (sit, wander, use furniture). 56 % of vanilla FNV NPCs
/// carry one; it's the dominant ambient idle behavior. See M42.
pub const PROCEDURE_SANDBOX: u32 = 12;

/// FO3/FNV package procedure-type index for `Wander` — walk to random
/// points within a radius, pause, repeat, with no target reference and no
/// scheduling beyond PSDT/CTDA. First non-Sandbox procedure to get a
/// runtime (M42.3), backed by `wander_system`'s straight-line
/// walk-to-point locomotion (no pathing/NAVM). See M42.
pub const PROCEDURE_WANDER: u32 = 5;

/// FO3/FNV package procedure-type index for `Patrol` — authored real
/// Bethesda Patrol packages walk a route defined by linked patrol-idle
/// markers, none of which this codebase decodes anywhere (that data lives
/// outside `PACK`'s own sub-records). Absent that, v0 Patrol reduces to
/// exactly `Wander`'s random-point-in-`PLDT`-radius algorithm — not a
/// distinct runtime, just a second procedure type routed onto the same
/// oscillating-walk core (`systems::wander`'s shared helper). See
/// `systems::patrol` module docs. See M42.
pub const PROCEDURE_PATROL: u32 = 13;

/// FO3/FNV package procedure-type index for `Travel` — walk once to the
/// package's PLDT location and stop (no repeat, unlike Wander). Second
/// procedure to reuse `wander_system`'s locomotion primitive (M42.4, via
/// the shared `locomotion::step_toward` helper), and the first to attempt
/// resolving its PLDT target to a real live entity's position (a
/// `NearReference` FormID, via `resolve_entity_by_global_form_id`) rather
/// than only ever approximating with the actor's own spawn position. See
/// M42.
pub const PROCEDURE_TRAVEL: u32 = 6;

/// FO3/FNV package procedure-type index for `Follow` — continuously
/// follow a target actor, closing to (and holding) a stand-off distance.
/// Third procedure to reuse `wander_system`'s locomotion primitive (M42.5,
/// via `systems::locomotion::step_toward`), and the first to track a
/// **live** target position every tick rather than a frozen destination
/// (Travel resolves once and stops; Follow keeps re-reading the target's
/// `GlobalTransform`). Needs `PTDT` (target data), decoded for the first
/// time in this codebase for this procedure. See M42.
pub const PROCEDURE_FOLLOW: u32 = 1;

/// FO3/FNV package procedure-type index for `Escort` — walk to (and
/// collect) a PTDT target, then lead it once to the package's PLDT
/// location and stop. Fourth procedure to reuse `wander_system`'s
/// locomotion primitive (M42.6, via `systems::locomotion::step_toward`),
/// and the first to combine two already-decoded sub-records (`PTDT` from
/// Follow, `PLDT` from Travel) rather than needing new decode work — see
/// `systems::escort` module docs for the two-phase collect-then-lead
/// runtime. See M42.
pub const PROCEDURE_ESCORT: u32 = 2;

/// FO3/FNV package procedure-type index for `Guard` — walk once to the
/// package's PLDT location and hold that post, returning if displaced
/// beyond the authored radius. Needs only `PLDT`, mirroring `Travel`'s
/// read exactly; unlike Travel it never reaches a terminal state (guarding
/// is indefinite, like Wander) — see `systems::guard` module docs. See M42.
pub const PROCEDURE_GUARD: u32 = 14;

/// PACK schedule window from PSDT (FO3/FNV). `start_hour = None` when the raw
/// `time` byte is -1 (0xFF) = "any time". `duration_hours` is the PSDT
/// duration (in hours for FO3/FNV — verified against FalloutNV.esm: a
/// bartender's `8x12` = 08:00 for 12 h, an evening idle `20x2` = 20:00 for 2 h,
/// a `22x10` sleep = 22:00 for 10 h wrapping to 08:00).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PackSchedule {
    pub start_hour: Option<u8>,
    pub duration_hours: u32,
}

impl PackSchedule {
    /// True when `hour` (0..24) falls in `[start, start + duration)` mod 24.
    /// Any-time (`start_hour == None`) is always active.
    pub fn active_at(&self, hour: f32) -> bool {
        let Some(start) = self.start_hour else {
            return true;
        };
        let start = start as f32;
        let end = start + self.duration_hours as f32;
        let h = hour.rem_euclid(24.0);
        if end <= 24.0 {
            h >= start && h < end
        } else {
            h >= start || h < end - 24.0 // wraps past midnight
        }
    }
}

impl PackRecord {
    /// True when this package's procedure is `Sandbox` (the idle-in-area
    /// behavior that drives furniture use).
    pub fn is_sandbox(&self) -> bool {
        self.procedure_type == PROCEDURE_SANDBOX
    }

    /// True when this package's procedure is `Wander` (walk to random
    /// points within a radius, pause, repeat).
    pub fn is_wander(&self) -> bool {
        self.procedure_type == PROCEDURE_WANDER
    }

    /// True when this package's procedure is `Travel` (walk once to the
    /// PLDT location and stop).
    pub fn is_travel(&self) -> bool {
        self.procedure_type == PROCEDURE_TRAVEL
    }

    /// True when this package's procedure is `Follow` (continuously
    /// follow a target actor).
    pub fn is_follow(&self) -> bool {
        self.procedure_type == PROCEDURE_FOLLOW
    }

    /// True when this package's procedure is `Escort` (collect a target,
    /// then lead it to the PLDT location and stop).
    pub fn is_escort(&self) -> bool {
        self.procedure_type == PROCEDURE_ESCORT
    }

    /// True when this package's procedure is `Guard` (hold the PLDT
    /// location indefinitely, returning if displaced).
    pub fn is_guard(&self) -> bool {
        self.procedure_type == PROCEDURE_GUARD
    }

    /// True when this package's procedure is `Patrol` (v0: identical to
    /// Wander's random-point-in-radius algorithm — see
    /// [`PROCEDURE_PATROL`]'s doc for why).
    pub fn is_patrol(&self) -> bool {
        self.procedure_type == PROCEDURE_PATROL
    }

    /// Whether this package's schedule includes `hour`. No PSDT → always
    /// active (the package is condition/location-gated, not time-gated).
    pub fn scheduled_active_at(&self, hour: f32) -> bool {
        self.schedule.is_none_or(|s| s.active_at(hour))
    }
}

/// Selection rule (M42.2): an NPC's *active* package at `hour` — the first
/// package, in priority order (`NpcRecord.ai_packages` order), whose
/// schedule includes `hour` **and** whose CTDA conditions pass. This keeps
/// day-shift workers from being treated as idle sandboxers — e.g. a
/// bartender whose 08:00–20:00 `AtBar` package outranks an evening `Sandbox`
/// package is *not* seated at 10:00 — and now also skips a package whose
/// author-set conditions (e.g. `GetIsID`, `GetActorValue`) don't hold.
///
/// `condition_met` is caller-supplied because the M47.1 condition evaluator
/// lives in `byroredux_scripting`, which depends on this crate — so the
/// evaluation is injected rather than called across the boundary. Pass
/// `|_| true` for the schedule-only behavior (M42.1). Empty condition lists
/// must map to `true` in the caller's predicate (Bethesda's "no conditions
/// = always fires"). Unresolved packages are skipped by the caller (pass
/// only resolved records, in order).
///
/// `pub` (#2031 / PERF-D7-01) so a caller that needs more than one of the
/// `active_package_is_*`/`active_*_location`/`active_*_target` projections
/// below can resolve the winning package once and read `procedure_type` /
/// `location` / `target` directly, instead of re-running this walk (which
/// re-evaluates every rejected package's CTDA conditions) once per
/// projection. An NPC's active package is a single winning `PackRecord` by
/// construction, so every `active_package_is_*` call below against the
/// same `(packages, hour, condition_met)` resolves to the same package.
pub fn active_package<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> Option<&'a PackRecord> {
    packages
        .into_iter()
        .find(|pk| pk.scheduled_active_at(hour) && condition_met(pk))
}

/// Whether an NPC's active package at `hour` (see [`active_package`]) is a
/// Sandbox package. `condition_met` injects M47.1 CTDA evaluation (M42.2);
/// pass `|_| true` for schedule-only selection.
pub fn active_package_is_sandbox<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> bool {
    active_package(packages, hour, condition_met).is_some_and(PackRecord::is_sandbox)
}

/// The PLDT location of an NPC's active package at `hour` (see
/// [`active_package`]), when that package is Sandbox-type. `None` when the
/// active package isn't Sandbox, carries no PLDT, or nothing is scheduled
/// active. M42.1's seat system uses this to size its search radius around
/// the authored center instead of a fixed guess. `condition_met` injects
/// M47.1 CTDA evaluation (M42.2); pass `|_| true` for schedule-only.
pub fn active_sandbox_location<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> Option<PackLocation> {
    active_package(packages, hour, condition_met)
        .filter(|pk| pk.is_sandbox())
        .and_then(|pk| pk.location)
}

/// Whether an NPC's active package at `hour` (see [`active_package`]) is a
/// Wander package (M42.3). `condition_met` injects M47.1 CTDA evaluation;
/// pass `|_| true` for schedule-only selection. Mirrors
/// [`active_package_is_sandbox`] — an NPC's active package is always a
/// single winning `PackRecord`, so this and `active_package_is_sandbox`
/// are naturally mutually exclusive for the same package list/hour.
pub fn active_package_is_wander<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> bool {
    active_package(packages, hour, condition_met).is_some_and(PackRecord::is_wander)
}

/// The PLDT location of an NPC's active package at `hour` (see
/// [`active_package`]), when that package is Wander-type. `None` when the
/// active package isn't Wander, carries no PLDT, or nothing is scheduled
/// active. `wander_system` uses this to size its wander radius around the
/// authored center instead of a fixed default. Mirrors
/// [`active_sandbox_location`]. `condition_met` injects M47.1 CTDA
/// evaluation; pass `|_| true` for schedule-only.
pub fn active_wander_location<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> Option<PackLocation> {
    active_package(packages, hour, condition_met)
        .filter(|pk| pk.is_wander())
        .and_then(|pk| pk.location)
}

/// Whether an NPC's active package at `hour` (see [`active_package`]) is a
/// Travel package. Mirrors [`active_package_is_wander`]. `condition_met`
/// injects M47.1 CTDA evaluation; pass `|_| true` for schedule-only.
pub fn active_package_is_travel<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> bool {
    active_package(packages, hour, condition_met).is_some_and(PackRecord::is_travel)
}

/// The PLDT location of an NPC's active package at `hour` (see
/// [`active_package`]), when that package is Travel-type. `None` when the
/// active package isn't Travel, carries no PLDT, or nothing is scheduled
/// active. `travel_system` uses `PackLocation.radius` as its
/// no-target-resolved fallback pick radius, and `PackLocation.target`
/// (when it's `NearReference`) as the FormID to resolve to a live
/// destination. Mirrors [`active_wander_location`]. `condition_met`
/// injects M47.1 CTDA evaluation; pass `|_| true` for schedule-only.
pub fn active_travel_location<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> Option<PackLocation> {
    active_package(packages, hour, condition_met)
        .filter(|pk| pk.is_travel())
        .and_then(|pk| pk.location)
}

/// Whether an NPC's active package at `hour` (see [`active_package`]) is a
/// Follow package. Mirrors [`active_package_is_travel`]. `condition_met`
/// injects M47.1 CTDA evaluation; pass `|_| true` for schedule-only.
pub fn active_package_is_follow<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> bool {
    active_package(packages, hour, condition_met).is_some_and(PackRecord::is_follow)
}

/// The PTDT target of an NPC's active package at `hour` (see
/// [`active_package`]), when that package is Follow-type. `None` when the
/// active package isn't Follow, carries no PTDT, or nothing is scheduled
/// active. Mirrors [`active_travel_location`]. `condition_met` injects
/// M47.1 CTDA evaluation; pass `|_| true` for schedule-only.
pub fn active_follow_target<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> Option<PackTarget> {
    active_package(packages, hour, condition_met)
        .filter(|pk| pk.is_follow())
        .and_then(|pk| pk.target)
}

/// Whether an NPC's active package at `hour` (see [`active_package`]) is an
/// Escort package (M42.6). Mirrors [`active_package_is_follow`].
/// `condition_met` injects M47.1 CTDA evaluation; pass `|_| true` for
/// schedule-only selection.
pub fn active_package_is_escort<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> bool {
    active_package(packages, hour, condition_met).is_some_and(PackRecord::is_escort)
}

/// The PTDT target (who to collect) of an NPC's active package at `hour`
/// (see [`active_package`]), when that package is Escort-type. Mirrors
/// [`active_follow_target`]. `condition_met` injects M47.1 CTDA evaluation;
/// pass `|_| true` for schedule-only.
pub fn active_escort_target<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> Option<PackTarget> {
    active_package(packages, hour, condition_met)
        .filter(|pk| pk.is_escort())
        .and_then(|pk| pk.target)
}

/// The PLDT location (where to lead the target) of an NPC's active package
/// at `hour` (see [`active_package`]), when that package is Escort-type.
/// Mirrors [`active_travel_location`]. `condition_met` injects M47.1 CTDA
/// evaluation; pass `|_| true` for schedule-only.
pub fn active_escort_location<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> Option<PackLocation> {
    active_package(packages, hour, condition_met)
        .filter(|pk| pk.is_escort())
        .and_then(|pk| pk.location)
}

/// Whether an NPC's active package at `hour` (see [`active_package`]) is a
/// Guard package (M42.7). Mirrors [`active_package_is_travel`].
/// `condition_met` injects M47.1 CTDA evaluation; pass `|_| true` for
/// schedule-only selection.
pub fn active_package_is_guard<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> bool {
    active_package(packages, hour, condition_met).is_some_and(PackRecord::is_guard)
}

/// The PLDT location (the post to hold) of an NPC's active package at
/// `hour` (see [`active_package`]), when that package is Guard-type.
/// Mirrors [`active_travel_location`]. `condition_met` injects M47.1 CTDA
/// evaluation; pass `|_| true` for schedule-only.
pub fn active_guard_location<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> Option<PackLocation> {
    active_package(packages, hour, condition_met)
        .filter(|pk| pk.is_guard())
        .and_then(|pk| pk.location)
}

/// Whether an NPC's active package at `hour` (see [`active_package`]) is a
/// Patrol package (M42.8). Mirrors [`active_package_is_wander`] — v0
/// Patrol reduces to the exact same algorithm (see [`PROCEDURE_PATROL`]'s
/// doc). `condition_met` injects M47.1 CTDA evaluation; pass `|_| true`
/// for schedule-only selection.
pub fn active_package_is_patrol<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> bool {
    active_package(packages, hour, condition_met).is_some_and(PackRecord::is_patrol)
}

/// The PLDT location of an NPC's active package at `hour` (see
/// [`active_package`]), when that package is Patrol-type. Mirrors
/// [`active_wander_location`]. `condition_met` injects M47.1 CTDA
/// evaluation; pass `|_| true` for schedule-only.
pub fn active_patrol_location<'a>(
    packages: impl IntoIterator<Item = &'a PackRecord>,
    hour: f32,
    condition_met: impl Fn(&PackRecord) -> bool,
) -> Option<PackLocation> {
    active_package(packages, hour, condition_met)
        .filter(|pk| pk.is_patrol())
        .and_then(|pk| pk.location)
}

/// Remap a raw plugin-local FormID to global space, leaving 0 (no
/// FormID / null ref) untouched. Mirrors the null-guard in
/// `remap_condition_form_ids` for the single-field PLDT case.
fn remap_fid(raw: u32, remap: &Option<crate::esm::reader::FormIdRemap>) -> u32 {
    if raw == 0 {
        return 0;
    }
    remap.as_ref().map_or(raw, |r| r.remap(raw))
}

pub fn parse_pack(
    form_id: u32,
    subs: &[SubRecord],
    remap: &Option<crate::esm::reader::FormIdRemap>,
    game: GameKind,
) -> PackRecord {
    let mut out = PackRecord {
        form_id,
        ..Default::default()
    };
    for sub in subs {
        match &sub.sub_type {
            b"EDID" => out.editor_id = read_zstring(&sub.data),
            b"PKDT" if sub.data.len() >= 8 => {
                let mut r = SubReader::new(&sub.data);
                out.package_flags = r.u32_or_default();
                // FO3/FNV PKDT: the procedure type is a single BYTE at
                // offset 4, followed by type-specific / flags2 bytes.
                // Reading it as a u32 (the pre-M42 bug) polluted the
                // type with the next 3 bytes — e.g. a Sandbox (12)
                // package parsed as 3452816652 / 268 / 65292. Masking to
                // the byte restores the clean 0..=16 enum (verified
                // against a full FalloutNV.esm sweep).
                out.procedure_type = r.u8_or_default() as u32;
            }
            b"PSDT" if sub.data.len() >= 8 => {
                // FO3/FNV PSDT (8 bytes): month i8, dayOfWeek i8, date u8,
                // time i8 (hour; -1/0xFF = any), duration i32 (hours).
                // Verified vs FalloutNV.esm (AtBar 8x12, Evening 20x2,
                // Sleep 22x10). `time` sits at offset 3 in both eras.
                //
                // #2012 / LC0716-01 — Skyrim+ PSDT grew to 12 bytes: the
                // same month/day/date/hour i8 quartet, plus a new
                // `minute` i8 (offset 4, not decoded here — no consumer
                // needs sub-hour precision yet) and 3 bytes of padding,
                // pushing `duration` from offset 4 to offset 8. Reading
                // offset 4 unconditionally on a Skyrim+ record misreads
                // `minute` + padding as `duration`. Cross-checked against
                // wrye-bash's MelPackSchedule (new, 12 B) vs
                // MelPackScheduleOld (old, 8 B). `uses_prebaked_facegen`
                // is Redux's existing "post-Skyrim" predicate (Skyrim /
                // FO4 / FO76 / Starfield) — reused here rather than
                // inventing a second one for the same era split.
                let time = sub.data[3] as i8;
                let duration_offset = if game.uses_prebaked_facegen() { 8 } else { 4 };
                let duration = if sub.data.len() >= duration_offset + 4 {
                    i32::from_le_bytes([
                        sub.data[duration_offset],
                        sub.data[duration_offset + 1],
                        sub.data[duration_offset + 2],
                        sub.data[duration_offset + 3],
                    ])
                } else {
                    0
                };
                out.schedule = Some(PackSchedule {
                    start_hour: if time < 0 { None } else { Some(time as u8) },
                    duration_hours: duration.max(0) as u32,
                });
            }
            // FO3/FNV PLDT: Location Type u32, Location union u32
            // (FormID or raw value depending on type), Radius i32. Per
            // https://tes5edit.github.io/fopdoc/FalloutNV/Records/PACK.html.
            // Only types 0 (Near Reference) / 1 (In Cell) / 4 (Object ID)
            // carry a FormID that needs remapping to global space; the
            // others are self-referential and pass through raw.
            b"PLDT" if sub.data.len() >= 12 => {
                let mut r = SubReader::new(&sub.data);
                let location_type = r.u32_or_default();
                let raw = r.u32_or_default();
                let radius = r.i32_or_default();
                let target = match location_type {
                    0 => PackLocationTarget::NearReference(remap_fid(raw, remap)),
                    1 => PackLocationTarget::InCell(remap_fid(raw, remap)),
                    4 => PackLocationTarget::ObjectId(remap_fid(raw, remap)),
                    _ => PackLocationTarget::Other(raw),
                };
                out.location = Some(PackLocation {
                    location_type,
                    target,
                    radius,
                });
            }
            // FO3/FNV PTDT (M42.5): Target Type u32, Target union u32
            // (FormID or raw value depending on type), Count/Distance i32.
            // A trailing Unknown f32 (offset 12) exists but has no known
            // consumer and isn't read. Per
            // https://tes5edit.github.io/fopdoc/FalloutNV/Records/PACK.html,
            // cross-checked against this file's own PLDT/PSDT arms (see
            // `PackTarget`'s doc comment). Only types 0 (Specific
            // Reference) / 1 (Object ID) carry a FormID that needs
            // remapping to global space; the rest pass through raw.
            b"PTDT" if sub.data.len() >= 16 => {
                let mut r = SubReader::new(&sub.data);
                let target_type = r.u32_or_default();
                let raw = r.u32_or_default();
                let count_or_distance = r.i32_or_default();
                let target = match target_type {
                    0 => PackTargetKind::SpecificReference(remap_fid(raw, remap)),
                    1 => PackTargetKind::ObjectId(remap_fid(raw, remap)),
                    _ => PackTargetKind::Other(raw),
                };
                out.target = Some(PackTarget {
                    target_type,
                    target,
                    count_or_distance,
                });
            }
            // Package eligibility conditions (M42.2). A PACK carries a flat
            // CTDA list (no per-block nesting like QUST stages), combined
            // with the standard OR-precedence rule. FormID params are
            // remapped to global load-order space here, same as PLDT above.
            b"CTDA" => {
                if let Some(mut cond) = parse_ctda(sub) {
                    remap_condition_form_ids(&mut cond, remap);
                    out.conditions.push(cond);
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::esm::records::condition::Condition;

    fn sub(typ: &[u8; 4], data: &[u8]) -> SubRecord {
        SubRecord {
            sub_type: *typ,
            data: data.to_vec(),
        }
    }

    #[test]
    fn parse_pack_picks_pkdt_flags_and_procedure() {
        let mut pkdt = Vec::new();
        pkdt.extend_from_slice(&0x0000_0421u32.to_le_bytes()); // flags
        pkdt.extend_from_slice(&6u32.to_le_bytes()); // procedure 6 = Travel
        let subs = vec![sub(b"EDID", b"TravelToWork\0"), sub(b"PKDT", &pkdt)];
        let p = parse_pack(0xA1A1, &subs, &None, GameKind::default());
        assert_eq!(p.editor_id, "TravelToWork");
        assert_eq!(p.package_flags, 0x0000_0421);
        assert_eq!(p.procedure_type, 6);
        assert!(!p.is_sandbox());
    }

    /// The procedure type is a single BYTE at PKDT offset 4. Real FNV
    /// PKDTs carry type-specific data in the 3 bytes after it; the
    /// pre-M42 u32 read polluted the type with them (a Sandbox package
    /// parsed as e.g. 0xCC…0C instead of 12). Masking to the byte must
    /// recover 12.
    #[test]
    fn parse_pack_reads_procedure_as_byte_not_polluted_u32() {
        let mut pkdt = Vec::new();
        pkdt.extend_from_slice(&0x0000_1234u32.to_le_bytes()); // flags
        pkdt.push(12); // procedure byte = Sandbox
        pkdt.extend_from_slice(&[0xAB, 0xCD, 0xEF]); // type-specific junk
        let subs = vec![sub(b"EDID", b"DefaultSandbox\0"), sub(b"PKDT", &pkdt)];
        let p = parse_pack(0xB2B2, &subs, &None, GameKind::default());
        assert_eq!(
            p.procedure_type, 12,
            "procedure must be the byte value, not the polluted u32"
        );
        assert!(p.is_sandbox());
    }

    fn pack(procedure: u32, schedule: Option<PackSchedule>) -> PackRecord {
        PackRecord {
            procedure_type: procedure,
            schedule,
            ..Default::default()
        }
    }

    fn sched(start_hour: Option<u8>, duration_hours: u32) -> Option<PackSchedule> {
        Some(PackSchedule {
            start_hour,
            duration_hours,
        })
    }

    #[test]
    fn parse_pack_reads_psdt_schedule() {
        // AtBar `8x12`: time byte = 8, duration i32 = 12 → 08:00 for 12 h.
        let psdt = [0xff, 0xff, 0x00, 0x08, 0x0c, 0, 0, 0];
        assert_eq!(
            parse_pack(0x1, &[sub(b"PSDT", &psdt)], &None, GameKind::default()).schedule,
            sched(Some(8), 12)
        );
        // Any-time sandbox: time byte = -1 (0xFF) → start_hour None.
        let any = [0xff, 0xff, 0x00, 0xff, 0, 0, 0, 0];
        assert_eq!(
            parse_pack(0x2, &[sub(b"PSDT", &any)], &None, GameKind::default()).schedule,
            sched(None, 0)
        );
    }

    /// #2012 / LC0716-01 — Skyrim+ PSDT is 12 bytes: the same
    /// month/day/date/hour i8 quartet at offsets 0-3, a new `minute` i8
    /// at offset 4, 3 bytes of padding, then `duration` i32 at offset 8
    /// (not offset 4, where the pre-fix code read unconditionally).
    /// Pins that a Skyrim+ `GameKind` reads `duration` from the correct
    /// offset instead of misreading `minute` + padding as the duration.
    #[test]
    fn parse_pack_reads_skyrim_plus_psdt_schedule_from_offset_8() {
        // month=0xff day=0xff date=0x00 hour=0x08 minute=0x1e(30, unused)
        // pad=[0,0,0] duration=12 (0x0c000000 LE) — AtBar-equivalent
        // schedule (08:00 for 12h) under the 12-byte Skyrim+ layout.
        let psdt = [0xff, 0xff, 0x00, 0x08, 0x1e, 0, 0, 0, 0x0c, 0, 0, 0];
        assert_eq!(
            parse_pack(0x1, &[sub(b"PSDT", &psdt)], &None, GameKind::Skyrim).schedule,
            sched(Some(8), 12),
            "Skyrim+ PSDT must read duration from offset 8, not offset 4 \
             (which holds `minute` + padding under the 12-byte layout)"
        );

        // Same bytes read under Fallout3NV (the old 8-byte-layout era)
        // must reinterpret offset 4 (`minute`=0x1e=30) as `duration`,
        // proving the two branches genuinely diverge on this input
        // rather than coincidentally agreeing.
        assert_eq!(
            parse_pack(0x2, &[sub(b"PSDT", &psdt)], &None, GameKind::Fallout3NV).schedule,
            sched(Some(8), 30),
        );
    }

    fn pldt(location_type: u32, raw: u32, radius: i32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&location_type.to_le_bytes());
        data.extend_from_slice(&raw.to_le_bytes());
        data.extend_from_slice(&radius.to_le_bytes());
        data
    }

    #[test]
    fn parse_pack_no_pldt_leaves_location_none() {
        let p = parse_pack(0x1, &[sub(b"EDID", b"NoLocation\0")], &None, GameKind::default());
        assert!(p.location.is_none());
    }

    #[test]
    fn parse_pack_reads_pldt_near_reference() {
        // Type 0 = Near Reference, radius 512 (the FNV DefaultSandbox
        // package radius).
        let data = pldt(0, 0x0001_2345, 512);
        let p = parse_pack(0x1, &[sub(b"PLDT", &data)], &None, GameKind::default());
        let loc = p.location.expect("PLDT should populate location");
        assert_eq!(loc.location_type, 0);
        assert_eq!(loc.target, PackLocationTarget::NearReference(0x0001_2345));
        assert_eq!(loc.radius, 512);
    }

    #[test]
    fn parse_pack_reads_pldt_in_cell_and_object_id() {
        let cell = pldt(1, 0x0002_ABCD, 0);
        let p = parse_pack(0x1, &[sub(b"PLDT", &cell)], &None, GameKind::default());
        assert_eq!(
            p.location.unwrap().target,
            PackLocationTarget::InCell(0x0002_ABCD)
        );

        let obj = pldt(4, 0x0003_1111, 256);
        let p = parse_pack(0x1, &[sub(b"PLDT", &obj)], &None, GameKind::default());
        assert_eq!(
            p.location.unwrap().target,
            PackLocationTarget::ObjectId(0x0003_1111)
        );
    }

    /// Types other than 0/1/4 (Near Current Location, Near Editor
    /// Location, Object Type, Near Linked Reference, At Package
    /// Location) carry no FormID — the raw union value passes through
    /// unremapped as `Other`.
    #[test]
    fn parse_pack_reads_pldt_other_types_pass_through_raw() {
        for location_type in [2u32, 3, 5, 6, 7] {
            let data = pldt(location_type, 0x0009_9999, 128);
            let p = parse_pack(0x1, &[sub(b"PLDT", &data)], &None, GameKind::default());
            let loc = p.location.unwrap();
            assert_eq!(loc.location_type, location_type);
            assert_eq!(loc.target, PackLocationTarget::Other(0x0009_9999));
        }
    }

    /// PLDT's Near Reference FormID is plugin-local at parse time; a
    /// self-reference (top byte == master count) must remap to the
    /// plugin's own global slot, mirroring `remap_condition_form_ids`'s
    /// contract for the same #1666 pattern.
    #[test]
    fn parse_pack_pldt_near_reference_remaps_form_id() {
        let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
        // mod_index 1 == master_slots.len() → self-reference.
        let raw = (1u32 << 24) | 0x0000_5678;
        let data = pldt(0, raw, 512);
        let p = parse_pack(0x1, &[sub(b"PLDT", &data)], &Some(remap), GameKind::default());
        assert_eq!(
            p.location.unwrap().target,
            PackLocationTarget::NearReference((2u32 << 24) | 0x0000_5678)
        );
    }

    fn ptdt(target_type: u32, raw: u32, count_or_distance: i32) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&target_type.to_le_bytes());
        data.extend_from_slice(&raw.to_le_bytes());
        data.extend_from_slice(&count_or_distance.to_le_bytes());
        data.extend_from_slice(&0.0f32.to_le_bytes()); // trailing Unknown f32, not read
        data
    }

    #[test]
    fn parse_pack_no_ptdt_leaves_target_none() {
        let p = parse_pack(0x1, &[sub(b"EDID", b"NoTarget\0")], &None, GameKind::default());
        assert!(p.target.is_none());
    }

    #[test]
    fn parse_pack_reads_ptdt_specific_reference() {
        // Type 0 = Specific Reference, distance 256.
        let data = ptdt(0, 0x0001_2345, 256);
        let p = parse_pack(0x1, &[sub(b"PTDT", &data)], &None, GameKind::default());
        let target = p.target.expect("PTDT should populate target");
        assert_eq!(target.target_type, 0);
        assert_eq!(target.target, PackTargetKind::SpecificReference(0x0001_2345));
        assert_eq!(target.count_or_distance, 256);
    }

    #[test]
    fn parse_pack_reads_ptdt_object_id_and_other_types() {
        let obj = ptdt(1, 0x0003_1111, 128);
        let p = parse_pack(0x1, &[sub(b"PTDT", &obj)], &None, GameKind::default());
        assert_eq!(p.target.unwrap().target, PackTargetKind::ObjectId(0x0003_1111));

        for target_type in [2u32, 3] {
            let data = ptdt(target_type, 0x0009_9999, 0);
            let p = parse_pack(0x1, &[sub(b"PTDT", &data)], &None, GameKind::default());
            assert_eq!(p.target.unwrap().target, PackTargetKind::Other(0x0009_9999));
        }
    }

    #[test]
    fn parse_pack_ptdt_specific_reference_remaps_form_id() {
        let remap = crate::esm::reader::FormIdRemap::regular(2, vec![0]);
        let raw = (1u32 << 24) | 0x0000_5678; // mod_index 1 == master_slots.len() → self-reference
        let data = ptdt(0, raw, 0);
        let p = parse_pack(0x1, &[sub(b"PTDT", &data)], &Some(remap), GameKind::default());
        assert_eq!(
            p.target.unwrap().target,
            PackTargetKind::SpecificReference((2u32 << 24) | 0x0000_5678)
        );
    }

    #[test]
    fn pack_schedule_active_at_windows() {
        let bar = PackSchedule {
            start_hour: Some(8),
            duration_hours: 12,
        }; // 08:00–20:00
        assert!(bar.active_at(10.0));
        assert!(!bar.active_at(21.0));
        assert!(!bar.active_at(7.9));
        let sleep = PackSchedule {
            start_hour: Some(22),
            duration_hours: 10,
        }; // 22:00–08:00 (wraps midnight)
        assert!(sleep.active_at(23.0));
        assert!(sleep.active_at(2.0));
        assert!(!sleep.active_at(10.0));
        let any = PackSchedule {
            start_hour: None,
            duration_hours: 0,
        };
        assert!(any.active_at(0.0) && any.active_at(15.0));
    }

    #[test]
    fn active_package_selector_respects_priority_and_schedule() {
        // Bartender's daytime package outranks an evening Sandbox fallback.
        let bartender = pack(6, sched(Some(8), 12)); // Travel/AtBar 08–20
        let evening = pack(PROCEDURE_SANDBOX, sched(Some(20), 2)); // sandbox 20–22
        // 10:00 → bartender active → NOT treated as sandbox (the Trudy bug).
        assert!(!active_package_is_sandbox([&bartender, &evening], 10.0, |_| true));
        // 21:00 → bartender off-shift, evening sandbox active.
        assert!(active_package_is_sandbox([&bartender, &evening], 21.0, |_| true));
        // Any-time saloon sandbox behind an inactive sleep package → sandbox.
        let sleep = pack(4, sched(Some(22), 10));
        let anytime_sandbox = pack(PROCEDURE_SANDBOX, None);
        assert!(active_package_is_sandbox([&sleep, &anytime_sandbox], 10.0, |_| true));
        // No resolvable packages → not sandbox.
        assert!(!active_package_is_sandbox(
            std::iter::empty::<&PackRecord>(),
            10.0,
            |_| true
        ));
    }

    #[test]
    fn active_package_selector_gates_on_conditions() {
        // Two packages both scheduled-active at 10:00: a higher-priority
        // Sandbox whose condition fails, and a lower-priority Sandbox with no
        // conditions. The condition predicate must skip the first and pick
        // the second — proving CTDA gating changes the selection, not just
        // the boolean.
        let mut gated = pack(PROCEDURE_SANDBOX, None);
        gated.editor_id = "GatedSandbox".into();
        gated.conditions = vec![Condition {
            function_index: 72, // GetIsID (arbitrary — the predicate decides)
            ..Default::default()
        }];
        let fallback = pack(PROCEDURE_SANDBOX, None); // no conditions
        // Predicate: a package passes iff it has no conditions (models the
        // caller's fail path for the gated one).
        let cond_met = |pk: &PackRecord| pk.conditions.is_empty();
        // Still sandbox overall — the fallback wins.
        assert!(active_package_is_sandbox([&gated, &fallback], 10.0, cond_met));
        // With only the gated package, its failing condition drops it → no
        // active package → not sandbox. Contrast `|_| true` which passes it.
        assert!(!active_package_is_sandbox([&gated], 10.0, cond_met));
        assert!(active_package_is_sandbox([&gated], 10.0, |_| true));
    }

    #[test]
    fn active_package_is_wander_selector_respects_priority_and_schedule() {
        // Mirrors active_package_selector_respects_priority_and_schedule
        // with Wander swapped in for Sandbox.
        let bartender = pack(6, sched(Some(8), 12)); // Travel/AtBar 08–20
        let evening = pack(PROCEDURE_WANDER, sched(Some(20), 2)); // wander 20–22
        // 10:00 → bartender active → NOT treated as wander.
        assert!(!active_package_is_wander([&bartender, &evening], 10.0, |_| true));
        // 21:00 → bartender off-shift, evening wander active.
        assert!(active_package_is_wander([&bartender, &evening], 21.0, |_| true));
        // Any-time wander behind an inactive sleep package → wander.
        let sleep = pack(4, sched(Some(22), 10));
        let anytime_wander = pack(PROCEDURE_WANDER, None);
        assert!(active_package_is_wander([&sleep, &anytime_wander], 10.0, |_| true));
        // No resolvable packages → not wander.
        assert!(!active_package_is_wander(
            std::iter::empty::<&PackRecord>(),
            10.0,
            |_| true
        ));
    }

    #[test]
    fn active_package_is_wander_selector_gates_on_conditions() {
        // Mirrors active_package_selector_gates_on_conditions with Wander.
        let mut gated = pack(PROCEDURE_WANDER, None);
        gated.editor_id = "GatedWander".into();
        gated.conditions = vec![Condition {
            function_index: 72, // GetIsID (arbitrary — the predicate decides)
            ..Default::default()
        }];
        let fallback = pack(PROCEDURE_WANDER, None); // no conditions
        let cond_met = |pk: &PackRecord| pk.conditions.is_empty();
        assert!(active_package_is_wander([&gated, &fallback], 10.0, cond_met));
        assert!(!active_package_is_wander([&gated], 10.0, cond_met));
        assert!(active_package_is_wander([&gated], 10.0, |_| true));
    }

    #[test]
    fn active_package_is_sandbox_and_is_wander_are_mutually_exclusive() {
        // A single winning PackRecord can only satisfy one procedure check
        // — Sandbox and Wander selection over the same package list/hour
        // must never both report true.
        let sandbox_only = pack(PROCEDURE_SANDBOX, None);
        assert!(active_package_is_sandbox([&sandbox_only], 10.0, |_| true));
        assert!(!active_package_is_wander([&sandbox_only], 10.0, |_| true));

        let wander_only = pack(PROCEDURE_WANDER, None);
        assert!(!active_package_is_sandbox([&wander_only], 10.0, |_| true));
        assert!(active_package_is_wander([&wander_only], 10.0, |_| true));
    }

    #[test]
    fn active_package_is_travel_selector_respects_priority_and_schedule() {
        // Mirrors active_package_is_wander_selector_respects_priority_and_schedule
        // with Travel swapped in for Wander. Bartender uses Follow (1) here
        // — procedure 6 is now Travel itself, so it can't stand in as the
        // generic "some other procedure" placeholder anymore.
        let bartender = pack(1, sched(Some(8), 12)); // Follow/AtBar 08–20
        let evening = pack(PROCEDURE_TRAVEL, sched(Some(20), 2)); // travel 20–22
        // 10:00 → bartender active → NOT treated as travel.
        assert!(!active_package_is_travel([&bartender, &evening], 10.0, |_| true));
        // 21:00 → bartender off-shift, evening travel active.
        assert!(active_package_is_travel([&bartender, &evening], 21.0, |_| true));
        // Any-time travel behind an inactive sleep package → travel.
        let sleep = pack(4, sched(Some(22), 10));
        let anytime_travel = pack(PROCEDURE_TRAVEL, None);
        assert!(active_package_is_travel([&sleep, &anytime_travel], 10.0, |_| true));
        // No resolvable packages → not travel.
        assert!(!active_package_is_travel(
            std::iter::empty::<&PackRecord>(),
            10.0,
            |_| true
        ));
    }

    #[test]
    fn active_package_is_travel_selector_gates_on_conditions() {
        // Mirrors active_package_is_wander_selector_gates_on_conditions with Travel.
        let mut gated = pack(PROCEDURE_TRAVEL, None);
        gated.editor_id = "GatedTravel".into();
        gated.conditions = vec![Condition {
            function_index: 72, // GetIsID (arbitrary — the predicate decides)
            ..Default::default()
        }];
        let fallback = pack(PROCEDURE_TRAVEL, None); // no conditions
        let cond_met = |pk: &PackRecord| pk.conditions.is_empty();
        assert!(active_package_is_travel([&gated, &fallback], 10.0, cond_met));
        assert!(!active_package_is_travel([&gated], 10.0, cond_met));
        assert!(active_package_is_travel([&gated], 10.0, |_| true));
    }

    #[test]
    fn active_package_is_sandbox_wander_and_travel_are_mutually_exclusive() {
        // A single winning PackRecord can only satisfy one procedure check
        // — Sandbox, Wander, Travel, and Follow selection over the same
        // package list/hour must never report true for more than one.
        let sandbox_only = pack(PROCEDURE_SANDBOX, None);
        assert!(active_package_is_sandbox([&sandbox_only], 10.0, |_| true));
        assert!(!active_package_is_wander([&sandbox_only], 10.0, |_| true));
        assert!(!active_package_is_travel([&sandbox_only], 10.0, |_| true));
        assert!(!active_package_is_follow([&sandbox_only], 10.0, |_| true));

        let wander_only = pack(PROCEDURE_WANDER, None);
        assert!(!active_package_is_sandbox([&wander_only], 10.0, |_| true));
        assert!(active_package_is_wander([&wander_only], 10.0, |_| true));
        assert!(!active_package_is_travel([&wander_only], 10.0, |_| true));
        assert!(!active_package_is_follow([&wander_only], 10.0, |_| true));

        let travel_only = pack(PROCEDURE_TRAVEL, None);
        assert!(!active_package_is_sandbox([&travel_only], 10.0, |_| true));
        assert!(!active_package_is_wander([&travel_only], 10.0, |_| true));
        assert!(active_package_is_travel([&travel_only], 10.0, |_| true));
        assert!(!active_package_is_follow([&travel_only], 10.0, |_| true));

        let follow_only = pack(PROCEDURE_FOLLOW, None);
        assert!(!active_package_is_sandbox([&follow_only], 10.0, |_| true));
        assert!(!active_package_is_wander([&follow_only], 10.0, |_| true));
        assert!(!active_package_is_travel([&follow_only], 10.0, |_| true));
        assert!(active_package_is_follow([&follow_only], 10.0, |_| true));
    }

    #[test]
    fn active_package_is_follow_selector_respects_priority_and_schedule() {
        // Mirrors active_package_is_travel_selector_respects_priority_and_schedule
        // with Follow swapped in. Bartender uses Escort (2) here — 1 is now
        // Follow itself, so it can't stand in as the generic placeholder.
        let bartender = pack(2, sched(Some(8), 12)); // Escort/AtBar 08–20
        let evening = pack(PROCEDURE_FOLLOW, sched(Some(20), 2)); // follow 20–22
        assert!(!active_package_is_follow([&bartender, &evening], 10.0, |_| true));
        assert!(active_package_is_follow([&bartender, &evening], 21.0, |_| true));
        let sleep = pack(4, sched(Some(22), 10));
        let anytime_follow = pack(PROCEDURE_FOLLOW, None);
        assert!(active_package_is_follow([&sleep, &anytime_follow], 10.0, |_| true));
        assert!(!active_package_is_follow(
            std::iter::empty::<&PackRecord>(),
            10.0,
            |_| true
        ));
    }

    #[test]
    fn active_package_is_follow_selector_gates_on_conditions() {
        let mut gated = pack(PROCEDURE_FOLLOW, None);
        gated.editor_id = "GatedFollow".into();
        gated.conditions = vec![Condition {
            function_index: 72,
            ..Default::default()
        }];
        let fallback = pack(PROCEDURE_FOLLOW, None);
        let cond_met = |pk: &PackRecord| pk.conditions.is_empty();
        assert!(active_package_is_follow([&gated, &fallback], 10.0, cond_met));
        assert!(!active_package_is_follow([&gated], 10.0, cond_met));
        assert!(active_package_is_follow([&gated], 10.0, |_| true));
    }

    #[test]
    fn active_package_is_escort_selector_respects_priority_and_schedule() {
        // Mirrors active_package_is_follow_selector_respects_priority_and_schedule
        // with Escort swapped in. Bartender uses Wander (5) here — 2 is now
        // Escort itself, so it can't stand in as the generic placeholder.
        let bartender = pack(PROCEDURE_WANDER, sched(Some(8), 12)); // Wander/AtBar 08–20
        let evening = pack(PROCEDURE_ESCORT, sched(Some(20), 2)); // escort 20–22
        assert!(!active_package_is_escort([&bartender, &evening], 10.0, |_| true));
        assert!(active_package_is_escort([&bartender, &evening], 21.0, |_| true));
        let sleep = pack(4, sched(Some(22), 10));
        let anytime_escort = pack(PROCEDURE_ESCORT, None);
        assert!(active_package_is_escort([&sleep, &anytime_escort], 10.0, |_| true));
        assert!(!active_package_is_escort(
            std::iter::empty::<&PackRecord>(),
            10.0,
            |_| true
        ));
    }

    #[test]
    fn active_package_is_escort_selector_gates_on_conditions() {
        let mut gated = pack(PROCEDURE_ESCORT, None);
        gated.editor_id = "GatedEscort".into();
        gated.conditions = vec![Condition {
            function_index: 72,
            ..Default::default()
        }];
        let fallback = pack(PROCEDURE_ESCORT, None);
        let cond_met = |pk: &PackRecord| pk.conditions.is_empty();
        assert!(active_package_is_escort([&gated, &fallback], 10.0, cond_met));
        assert!(!active_package_is_escort([&gated], 10.0, cond_met));
        assert!(active_package_is_escort([&gated], 10.0, |_| true));
    }

    #[test]
    fn active_package_is_sandbox_wander_travel_follow_and_escort_are_mutually_exclusive() {
        // A single winning PackRecord can only satisfy one procedure check.
        let escort_only = pack(PROCEDURE_ESCORT, None);
        assert!(!active_package_is_sandbox([&escort_only], 10.0, |_| true));
        assert!(!active_package_is_wander([&escort_only], 10.0, |_| true));
        assert!(!active_package_is_travel([&escort_only], 10.0, |_| true));
        assert!(!active_package_is_follow([&escort_only], 10.0, |_| true));
        assert!(active_package_is_escort([&escort_only], 10.0, |_| true));

        let follow_only = pack(PROCEDURE_FOLLOW, None);
        assert!(!active_package_is_escort([&follow_only], 10.0, |_| true));
    }

    #[test]
    fn active_package_is_guard_selector_respects_priority_and_schedule() {
        // Mirrors active_package_is_travel_selector_respects_priority_and_schedule
        // with Guard swapped in. Bartender uses Follow (1) here — 14 is now
        // Guard itself, so it can't stand in as the generic placeholder.
        let bartender = pack(PROCEDURE_FOLLOW, sched(Some(8), 12)); // Follow/AtBar 08–20
        let evening = pack(PROCEDURE_GUARD, sched(Some(20), 2)); // guard 20–22
        assert!(!active_package_is_guard([&bartender, &evening], 10.0, |_| true));
        assert!(active_package_is_guard([&bartender, &evening], 21.0, |_| true));
        let sleep = pack(4, sched(Some(22), 10));
        let anytime_guard = pack(PROCEDURE_GUARD, None);
        assert!(active_package_is_guard([&sleep, &anytime_guard], 10.0, |_| true));
        assert!(!active_package_is_guard(
            std::iter::empty::<&PackRecord>(),
            10.0,
            |_| true
        ));
    }

    #[test]
    fn active_package_is_guard_selector_gates_on_conditions() {
        let mut gated = pack(PROCEDURE_GUARD, None);
        gated.editor_id = "GatedGuard".into();
        gated.conditions = vec![Condition {
            function_index: 72,
            ..Default::default()
        }];
        let fallback = pack(PROCEDURE_GUARD, None);
        let cond_met = |pk: &PackRecord| pk.conditions.is_empty();
        assert!(active_package_is_guard([&gated, &fallback], 10.0, cond_met));
        assert!(!active_package_is_guard([&gated], 10.0, cond_met));
        assert!(active_package_is_guard([&gated], 10.0, |_| true));
    }

    #[test]
    fn active_package_is_patrol_selector_respects_priority_and_schedule() {
        // Mirrors active_package_is_wander_selector_respects_priority_and_schedule
        // with Patrol swapped in. Bartender uses Guard (14) here — 13 is
        // now Patrol itself, so it can't stand in as the generic placeholder.
        let bartender = pack(PROCEDURE_GUARD, sched(Some(8), 12)); // Guard/AtBar 08–20
        let evening = pack(PROCEDURE_PATROL, sched(Some(20), 2)); // patrol 20–22
        assert!(!active_package_is_patrol([&bartender, &evening], 10.0, |_| true));
        assert!(active_package_is_patrol([&bartender, &evening], 21.0, |_| true));
        let sleep = pack(4, sched(Some(22), 10));
        let anytime_patrol = pack(PROCEDURE_PATROL, None);
        assert!(active_package_is_patrol([&sleep, &anytime_patrol], 10.0, |_| true));
        assert!(!active_package_is_patrol(
            std::iter::empty::<&PackRecord>(),
            10.0,
            |_| true
        ));
    }

    #[test]
    fn active_package_is_patrol_selector_gates_on_conditions() {
        let mut gated = pack(PROCEDURE_PATROL, None);
        gated.editor_id = "GatedPatrol".into();
        gated.conditions = vec![Condition {
            function_index: 72,
            ..Default::default()
        }];
        let fallback = pack(PROCEDURE_PATROL, None);
        let cond_met = |pk: &PackRecord| pk.conditions.is_empty();
        assert!(active_package_is_patrol([&gated, &fallback], 10.0, cond_met));
        assert!(!active_package_is_patrol([&gated], 10.0, cond_met));
        assert!(active_package_is_patrol([&gated], 10.0, |_| true));
    }

    #[test]
    fn active_package_is_sandbox_wander_travel_follow_escort_guard_and_patrol_are_mutually_exclusive(
    ) {
        // A single winning PackRecord can only satisfy one procedure check.
        let guard_only = pack(PROCEDURE_GUARD, None);
        assert!(!active_package_is_sandbox([&guard_only], 10.0, |_| true));
        assert!(!active_package_is_wander([&guard_only], 10.0, |_| true));
        assert!(!active_package_is_travel([&guard_only], 10.0, |_| true));
        assert!(!active_package_is_follow([&guard_only], 10.0, |_| true));
        assert!(!active_package_is_escort([&guard_only], 10.0, |_| true));
        assert!(active_package_is_guard([&guard_only], 10.0, |_| true));
        assert!(!active_package_is_patrol([&guard_only], 10.0, |_| true));

        let patrol_only = pack(PROCEDURE_PATROL, None);
        assert!(!active_package_is_sandbox([&patrol_only], 10.0, |_| true));
        assert!(!active_package_is_wander([&patrol_only], 10.0, |_| true));
        assert!(!active_package_is_travel([&patrol_only], 10.0, |_| true));
        assert!(!active_package_is_follow([&patrol_only], 10.0, |_| true));
        assert!(!active_package_is_escort([&patrol_only], 10.0, |_| true));
        assert!(!active_package_is_guard([&patrol_only], 10.0, |_| true));
        assert!(active_package_is_patrol([&patrol_only], 10.0, |_| true));
    }

    /// #2031 / PERF-D7-01 — `npc_spawn::spawn_npc_entity` collapsed 14
    /// separate `active_package_is_*`/`active_*_location`/`active_*_target`
    /// calls into a single `active_package(...)` resolve, then reads
    /// `procedure_type`/`location`/`target` directly off the one resolved
    /// package instead of re-deriving them through the per-procedure
    /// getters. This pins the equivalence that refactor depends on: for a
    /// Travel package (PLDT-only, mirrors Sandbox/Wander/Guard/Patrol's
    /// location-only shape) and a Follow package (PTDT-only, mirrors
    /// Escort's target-plus-location shape), a single `active_package` call
    /// exposes the exact same `location`/`target` data the old
    /// `active_travel_location`/`active_follow_target` getters would have
    /// returned — so the refactor is not just "same procedure selected" but
    /// "same location/target payload available from one resolve".
    #[test]
    fn active_package_single_resolve_exposes_same_location_as_travel_getter() {
        let mut p = pack(PROCEDURE_TRAVEL, None);
        p.location = Some(PackLocation {
            location_type: 0,
            target: PackLocationTarget::NearReference(0xDEAD_BEEF),
            radius: 512,
        });
        let via_getter = active_travel_location([&p], 10.0, |_| true);
        let resolved = active_package([&p], 10.0, |_| true);
        assert_eq!(
            resolved.and_then(|pk| pk.location),
            via_getter,
            "single active_package resolve's .location must match active_travel_location"
        );
        assert!(resolved.is_some_and(PackRecord::is_travel));
    }

    #[test]
    fn active_package_single_resolve_exposes_same_target_as_follow_getter() {
        let mut p = pack(PROCEDURE_FOLLOW, None);
        p.target = Some(PackTarget {
            target_type: 0,
            target: PackTargetKind::SpecificReference(0xCAFE_F00D),
            count_or_distance: 128,
        });
        let via_getter = active_follow_target([&p], 10.0, |_| true);
        let resolved = active_package([&p], 10.0, |_| true);
        assert_eq!(
            resolved.and_then(|pk| pk.target),
            via_getter,
            "single active_package resolve's .target must match active_follow_target"
        );
        assert!(resolved.is_some_and(PackRecord::is_follow));
    }

    #[test]
    fn parse_pack_captures_ctda_conditions() {
        // PKDT (sandbox) + one CTDA → the condition lands on the record so
        // the caller can evaluate it. 28-byte FO3/FNV CTDA: type byte at 0,
        // function_index (u32) at offset 8.
        let mut pkdt = Vec::new();
        pkdt.extend_from_slice(&0u32.to_le_bytes()); // flags
        pkdt.push(PROCEDURE_SANDBOX as u8); // procedure byte
        pkdt.extend_from_slice(&[0u8; 3]); // pad to 8
        let mut ctda = vec![0u8; 28];
        ctda[8..12].copy_from_slice(&72u32.to_le_bytes()); // function_index
        let p = parse_pack(
            0x1,
            &[sub(b"PKDT", &pkdt), sub(b"CTDA", &ctda)],
            &None,
            GameKind::default(),
        );
        assert!(p.is_sandbox());
        assert_eq!(p.conditions.len(), 1);
        assert_eq!(p.conditions[0].function_index, 72);
    }
}
