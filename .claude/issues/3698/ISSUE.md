# #3698 — ECS-P2-02: collect_candidates holds the scratch write guard across five component reads, every frame

**Severity**: MEDIUM · **Dimension**: P2 Gameplay Slice / Lock Ordering
**Location**: `byroredux/src/interaction.rs` (`collect_candidates`; helper `populate_candidates`)

## Fix

Flagged as the one exception found during #3697's SIBLING sweep of
`interaction.rs`. Implemented the issue's own suggested fix: take the map
OUT of `InteractionCandidateScratch` (via the same `mem::take` the code
already used, just moved earlier) before `populate_candidates` runs its
five component queries, instead of populating it in place while the
write guard is held. Unified the `Some`/`None` branches into one `match`
that either takes the resource's map or builds a fresh default, then runs
`.clear()` + `populate_candidates` identically either way — no behavior
branch left between "scratch present" and "scratch absent" beyond where
the map's storage comes from.

The capacity-reuse contract (#3059's whole reason this resource exists)
is unchanged: `select_interaction_target` still hands the used map back
into `scratch.candidates = candidates;` after consuming it (line ~865,
untouched), so next frame's `collect_candidates` still takes out whatever
capacity was left there — confirmed by the existing
`candidate_scratch_capacity_survives_across_calls` test passing unchanged.

## SIBLING (issue's own checklist item — "the `else` branch and the rest of `interaction.rs` checked for the same shape")

The `else` branch is now folded into the same `match` arm structure as
the `Some` branch, so there's no separate behavior to diverge. The rest
of `interaction.rs`'s resource-write sites were already swept during
#3697 (filed moments before this one, in the same session) — only this
site had the nested-read-inside-write-guard shape; `InteractionState`,
`InteractionTrace`, and the other `InteractionCandidateScratch` site
(line ~865, `scratch.candidates = candidates;`) all only assign
already-resolved locals.

## LOCK_ORDER (issue's own checklist item)

The write guard's scope narrowed (it now covers only the `mem::take`
call, not the five-query populate pass) but nothing changed about
TypeId-sorted acquisition — this was always a single-resource guard, not
a multi-component paired acquisition.

## TESTS (issue's own checklist item)

Added `collect_candidates_does_not_close_scratch_component_lock_cycle`,
mirroring #3697's live-detector pattern exactly: guarded on
`BYRO_LOCK_ORDER_CHECK=1`, establishes the canonical reverse edge
(`DoorTeleport` read, then `InteractionCandidateScratch` write) on a real
door fixture, then drives `select_interaction_target`.

Verified the guard actually catches the regression (this session's
established quality bar): reverted to the pre-fix nested-in-place
version, reran under `BYRO_LOCK_ORDER_CHECK=1` — the test failed with the
exact expected `DoorTeleport ↔ InteractionCandidateScratch` cycle
message, then restored the fix and confirmed a clean pass again.

## Verification

- `cargo check -p byroredux --tests`: clean.
- `cargo test -q -p byroredux --bin byroredux`: 1,871 tests passing, 0
  failing (+1 new); `candidate_scratch_capacity_survives_across_calls`
  still passes unchanged.
- `BYRO_LOCK_ORDER_CHECK=1 cargo test -p byroredux --bin byroredux
  collect_candidates_does_not_close`: passes with the detector live.
- `cargo test -q --no-fail-fast` (full workspace): **7092 passing, 0
  failing**.
