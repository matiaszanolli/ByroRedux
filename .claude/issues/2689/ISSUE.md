# #2689 — SAFE-D8-01: AnimationClipRegistry slot vector grows monotonically — release() never returns a slot, stranding one clip header per evict/reload cycle

**Severity**: LOW · **Dimension**: NPC / Animation spawn safety
**Location**: `crates/core/src/animation/registry.rs`

## Premise correction (verified before implementing)

The issue's suggested fix offered two options: a free list (reuse a
released slot in `add()`) or observability
(`stub_slot_count()` surfaced to debug stats). Analyzing the free-list
option against the registry's own documented invariant before
implementing it: **rejected it** — reuse is not actually safe.

The registry's own doc for `release` states live
`AnimationPlayer`/`AnimationLayer` consumers "still resolve via `Self::get`
but read an empty clip" — i.e. a released handle can genuinely still be
held by a live consumer indefinitely, reading the stub by design. If
`add()` recycled that same slot index for a *new*, unrelated clip, any
consumer still holding the old (released) handle value would suddenly
start reading the new clip's content instead of the empty stub the
moment it's pushed — exactly the aliasing bug the registry's own "no
stale handle ever resolves to a different clip" invariant exists to rule
out. The issue's own free-list argument ("reuse cannot alias a live
consumer to different content") doesn't hold given that documented
invariant, so implementing it would trade a slow, bounded-severity leak
for a real handle-aliasing correctness bug. Went with the issue's second
option instead.

## Fix

Added `AnimationClipRegistry::stub_slot_count() -> usize`, backed by a
`stub_count: usize` field incremented once per successful (populated-slot)
`release()` call — monotonic, matching the slots it counts.

Documented `add()`'s "never reuse" decision explicitly (it previously had
no doc at all), citing the aliasing hazard above, so the next person who
reaches for the free-list option sees why it was rejected before
re-proposing it.

Wired the two counts through to the `stats` console command, matching the
existing `mesh_count`/`meshes_in_use` "registry-wide vs in-use" pairing
convention already established on `DebugStats`:
- `DebugStats::anim_clip_count` / `anim_clip_stub_count` — new fields,
  populated in `app_events.rs`'s per-frame stats-refresh block (read
  before the `DebugStats` write-lock opens, not nested inside it, to
  keep this a plain sequential pair of resource acquisitions).
- `stats` command's output gained an `AnimClips: N clips / M stub
  (stranded, never reused)` line.

## SIBLING (issue's own checklist item)

Checked `#2524` (LRU eviction dropping freed handles) and `#863` (the
eviction path this counter's growth is driven by) — both already-closed,
unrelated to this specific counter; no other registry in the codebase
shares this exact "documented no-reuse + no observability" shape (mesh/
texture registries are refcounted and genuinely free their slots; the
skin-slot pool has its own `overflow_attempt_count` telemetry already).

## TESTS (issue's own checklist item)

`stub_slot_count_tracks_released_slots` — zero before any release, +1
per successful release, unaffected by an idempotent re-release or an
out-of-range handle (both already return `false` and strand nothing new).
`stub_slot_count_grows_with_repeated_evict_reload_cycles` — five
evict/reload cycles on the same path key produce `stub_slot_count() ==
len() == 5`, matching the issue's own framing ("one clip header per
evict/reload cycle, permanently").

**Reintroduce-and-revert verification**: temporarily removed the
`self.stub_count += 1;` line from `release()` — confirmed both new tests
failed (`left: 0, right: 1`). Restored the fix and reran — all 11 tests
in `animation::registry::tests` pass again.

## Verification

- `cargo check -p byroredux-core --tests`: clean, zero warnings.
- `cargo check -p byroredux --tests`: clean.
- `cargo test -p byroredux-core --lib animation::registry::`: 11 tests
  passing, 0 failing (+2 new).
- `cargo test -q -p byroredux-core`: 736 tests passing, 0 failing.
- `cargo test -q -p byroredux`: passing.
- `cargo test -q --no-fail-fast` (full workspace): **7153 passing, 0
  failing**.
