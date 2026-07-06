# #1850 (FNV-D7-02, MEDIUM) + #1851 (FNV-D7-03, LOW) — ragdoll edge/count guards

## #1850 — bhkBreakableConstraint invisible to the ragdoll graph
`extract_ragdoll`'s constraint loop only downcasts `BhkConstraint`. A
`bhkBreakableConstraint` decodes into its own struct (`BhkBreakableConstraint`)
and fell through the `continue` with **no warning** — unlike the #1539 `Other`
arm which warns before dropping. A breakable-wrapped Ragdoll(7)/LimitedHinge(2)
is a real articulation joint, so the silent drop can detach a limb.

**Key finding**: `BhkBreakableConstraint::parse` (`constraints.rs:607`) *skips*
the wrapped CInfo bytes (`stream.skip(size)`) — it retains only `wrapped_type`,
not the joint geometry (twist/pivot/limits). So the inner joint **cannot** be
rebuilt from what we keep; fully surfacing it would need a byte-accurate parser
change to retain the wrapped CInfo (risky on sizeless Oblivion, and no vanilla
FNV skeleton even uses it — malleable dominates). The issue offers the
#1539-style warn as the minimal fix; that is what closes the actual defect
(the *silent* drop).

**Fix** (`crates/nif/src/import/collision/ragdoll.rs`): in the loop's
downcast-fail arm, also downcast `BhkBreakableConstraint` and, when it bridges
two distinct ragdoll bodies, `log::warn!` naming the two bones (mirroring
#1539). Logic extracted into a pure `breakable_dropped_edge` helper so the drop
is unit-testable without a logger.

**Tests**: `breakable_dropped_edge_names_the_two_bones` (helper: real edge →
Some(names); self-loop / unmapped-body → None) + `breakable_wrapped_ragdoll_is_dropped_not_surfaced`
(end-to-end: a breakable-only 2-body scene yields `None`, not a fabricated joint).

## #1851 — real-data test does not pin FNV body/joint counts
**Premise partly STALE**: the FNV arm already had exact `assert_eq!(bodies==18,
constraints==17)` pins since commit `0a0bc3ce` (2026-06-14), 18 days *before*
the issue was filed (2026-07-02) — strictly stronger than the requested floor.
The issue's Location cited `assert_structural` in isolation and missed the
per-arm pins in `fnv_humanoid_skeleton_threads_ragdoll`.

**Live part** = the sibling check: the Oblivion and Skyrim arms had **no** count
pin, so a joint-drop there would pass silently. Resolved with *measured* data
(no guessing): ran the ignored real-data tests 2026-07-05 →
- Oblivion: 18 bodies / 17 joints (10 Ragdoll + 7 LimitedHinge)
- FNV: 18 / 17 (9 + 8)
- Skyrim SE: 18 / 17 (9 + 8)

**Fix** (`crates/nif/tests/ragdoll_import.rs`): shared `assert_reference_counts`
helper using `>=` floors at the measured count (catches any drop; stays
future-proof to a #1850-style improvement that would *increase* joint count,
which a brittle `==` would false-fail). Applied to all three arms (FNV
converted exact→floor for uniformity). Added two CI-runnable tests (no game
data): floor passes at/above measured, and `#[should_panic]` trips on a
synthetic 16-joint drop.

Note: the issue's suggested `>= 16` floor would have *missed* a single-joint
drop (17→16 passes `>= 16`); the floor is placed at the measured value instead.

## Domain / verification
nif → `byroredux-nif`. Scoped `cargo test -p byroredux-nif` green (869 lib,
+2); ignored real-data arms green with the new floors; full workspace green,
no new warnings.
