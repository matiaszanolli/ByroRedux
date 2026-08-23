# Stream-boundary state continuity

**Status**: PROPOSED (2026-08-23). No code lands from this document by
itself — it's the design authority EX-16 item 4 (#2372) and EX-14/15 item
C2's "reconcile instead of re-spawning" half (#2369) were both correctly
flagged-but-not-attempted for, per `docs/engine/exterior-readiness-plan.md`'s
own investigation notes for each.

## 1. The one problem, two populations

Both flagged gaps are the same underlying fact wearing two different
costumes: **this engine's streaming model has exactly one way to leave a
cell and exactly one way to enter one — full teardown, full rebuild from
authored ESM data — with no path for anything that happened in between to
survive the round trip.** For most content that's correct (an unmodified
REFR should render identically every time it's spawned). It stops being
correct the moment something *changed* after spawn: a script moved an
object, an actor picked a seat, an actor started walking toward a Travel
destination.

The two items differ only in *which* despawn/respawn cycle they're about,
and that difference matters enough to need two different fixes, not one:

| | EX-14/15 item C2 | EX-16 item 4 |
|---|---|---|
| **Population** | Persistent-flagged refs, resident in a worldspace's dedicated `persistent_root` (never evicted by radius streaming) | Temporary (non-persistent) refs, resident in an ordinary grid-tile `CellRoot` (routinely evicted/reloaded by radius streaming) |
| **When the cycle happens** | A *worldspace-level* crossing — `drain_streaming_state` tears the whole `WorldStreamingState` down, including `persistent_root`, and `begin_worldspace_persistent_cell` rebuilds it from scratch | An *ordinary grid-tile* crossing — ~every few seconds of player movement, ordinary radius hysteresis (`compute_streaming_deltas`) evicts a tile and later reloads it |
| **Is the teardown avoidable?** | Often yes — if the source and destination worldspace crossing resolves to the **same** persistent CELL (leaving a child worldspace back to its parent, or between siblings sharing one ancestor's persistent CELL via the WNAM chain `resolve_persistent_cell` already walks), the rebuild is pure waste | No — the tile genuinely left the load radius; re-spawning it is the correct, necessary behavior, not a bug to optimize away |
| **Right fix shape** | **Skip the unnecessary work.** Compare resolved persistent-CELL identity across the crossing *before* draining; skip the persistent-CELL drain+rebuild specifically when it's unchanged, while still fully draining the grid tiles (which always change with the worldspace) | **Snapshot before despawn, restore after respawn.** The despawn is correct and necessary; what's missing is a place for the entity's accumulated runtime state to survive it |

Two fixes, one design doc, because they share every hard part below: how to
identify "the same logical actor" across a despawn/respawn boundary, how to
avoid stale `EntityId` references inside restored state, and how to bound
the cost/lifetime of whatever's being retained. Building them separately
risks solving the shared part twice, differently, and inconsistently.

---

## 2. The concrete failure, traced through real component types

Not hypothetical — traced against the actual runtime state types
(`crates/core/src/ecs/components/{sandbox,wander,travel}.rs`,
`byroredux/src/components.rs`):

- **Position.** `spawn_npc_entity`/ordinary REFR respawn places every entity
  back at its **authored REFR position** — there is no "last known position"
  concept anywhere in the respawn path. An actor that walked away from its
  spawn point (Wander, Travel, or just NPC AI in general) snaps back to
  exactly where the ESM placed it, the instant its owning cell round-trips
  through a despawn/respawn.
- **`AmbientPackageRuntime`** (`byroredux/src/components.rs`) — `
  active_package_form_id: Option<u32>`, `last_evaluated_game_minute:
  Option<u16>`. Both FormID/scalar, no `EntityId` inside — the *easy* case,
  see §3.
- **`Seated`** (`crates/core/src/ecs/components/sandbox.rs`) — `{ furniture:
  EntityId }`. **The hard case.** `EntityId` allocation is monotonic (#372);
  a respawned furniture entity gets a *new* `EntityId`, never the one a
  stale `Seated` snapshot would name. Restoring this field verbatim after a
  despawn/respawn round trip would point at a dead or (worse) a *different,
  wrong* live entity, since IDs are never reused. This is the concrete
  reason a snapshot can't just be "serialize the component, deserialize it
  back" — see §4.
- **`TravelState`** (`crates/core/src/ecs/components/travel.rs`) — `{
  destination: Vec3 }`, resolved once and frozen (unlike `WanderState`,
  Travel never re-picks). Plus the terminal marker `Traveled` (unit struct,
  no fields). An actor that already reached `Traveled` and stopped, after
  any cell-boundary cycle affecting its *spawn* tile — irrespective of the
  actor's *current* position, since ownership is entity-range/`CellRoot`-
  based, assigned once at spawn, not tracked by live location — restarts
  its entire walk from the original spawn point. This is the single most
  visibly wrong symptom the missing mechanism produces: a completed
  errand un-completing itself.
- **`WanderState`** — deliberately **not** in scope for preservation.
  `WanderBehavior`'s own doc already establishes the opposite philosophy on
  purpose: `form_id` feeds a deterministic desync hash so a re-roll on
  respawn is *intended* behavior, not a gap. Confirmed correct as-is; no
  design work needed here.

---

## 3. FormID-keyed state, not EntityId-keyed state

The `Seated.furniture: EntityId` problem generalizes: **any snapshot that
might outlive the entities it references must store those references as
FormIDs, and re-resolve them to live `EntityId`s at restore time — never
store a raw `EntityId` across a despawn/respawn boundary.** This isn't a
new idea to invent; it's the exact pattern `PersistentRefIndex`
(`cell_loader::persistent_ref_index`) already demonstrates for a different
population (globally-persistent actors, resolved within `persistent_root`):
an `O(1)`-after-rebuild `FormId → EntityId` map scoped to one cell root,
invalidated when that root's content changes.

What's missing is the **ordinary-cell-root** counterpart: the same
`FormId → EntityId` resolution shape, scoped to whichever grid-tile
`CellRoot` is being respawned, not only to `persistent_root`. This is
additive to `PersistentRefIndex`, not a replacement for it — likely a
sibling index keyed the same way (`CellRoot` identity as the invalidation
signal, `resolve_entity_by_global_form_id`'s key space reused rather than
duplicated), built lazily the moment a snapshot restore needs to resolve a
reference rather than proactively for every ordinary cell (that would be
pure overhead for the overwhelming majority of REFRs that never accumulate
any state worth snapshotting).

---

## 4. Snapshot scope — what to keep, what to deliberately drop

Not "snapshot every component," which would balloon into a second save
system with none of the real save system's validation/versioning
discipline (`docs/engine/save_invariants.md`). Scope it the same way the
save-registry completeness guard already forces every `Component`/`Resource`
to be classified — explicit allow-list, not a blanket rule:

**Keep** (concrete, from §2's trace):
- Live position/orientation, if it has diverged from the authored REFR
  placement by more than a small epsilon (so an actor that never moved
  costs nothing to snapshot — the common case for the vast majority of
  ordinary REFRs).
- `AmbientPackageRuntime.active_package_form_id` (FormID, trivially safe).
- `TravelState.destination` + `Traveled` presence (the "already arrived"
  fact is exactly what must survive, per §2).
- `Seated.furniture`, snapshotted as a **FormID** via §3's index, not the
  raw `EntityId`.

**Deliberately drop, re-roll on respawn** (confirmed safe, not just
assumed):
- `WanderState` — per §2, re-roll is the *intended* behavior already.
- Any animation-phase/pose state — a fresh spawn re-entering its default
  pose for whatever package it resumes into is not visually distinct from
  today's respawn behavior, and snapshotting mid-animation state is a much
  larger surface (interpolation continuity, clip-transition validity) for
  a benefit nobody has asked for.
- Script-local Papyrus VM state — canonical save/load already owns this
  boundary (`ScriptVariables`/`ScriptTimer` are registered in the real save
  registry); a stream-boundary snapshot duplicating that would be a second,
  competing source of truth for the same data. Out of scope here entirely.

## 5. Lifetime and bounding

The snapshot store itself needs the same `OwnershipTracker`
`Exact`/`Bounded`/`Monotonic` discipline every other cross-cutting resource
in this codebase gets (`crates/core/src/ecs/resources/ownership.rs`):

- Scoped per `CellRoot`, populated at despawn time (`unload_cell_inner`),
  consulted and cleared at the corresponding respawn (`spawn_npc_entity`/
  the ordinary REFR spawn path) for that same tile's re-entry.
- A snapshot that's never claimed (the tile never comes back — worldspace
  changed, save loaded elsewhere, session ended) must not accumulate
  forever. Bound it the simple way first: clear on worldspace drain
  (`drain_streaming_state`'s existing single choke point every exterior
  teardown already funnels through) rather than inventing a TTL — a
  snapshot is only ever meaningful for "the player might walk back to this
  exact tile in this exact worldspace session," and drain is precisely the
  point that's no longer true.
- New `OwnershipTracker` class (`stream_snapshot_rows`, `Exact` policy,
  same posture EX-16 item 6 already established for `navm_tiles_resident`)
  once this lands — it's exactly the kind of "generic reclaim path might
  silently leak" surface that tracker exists to catch.

---

## 6. What this document does NOT decide

- **The exact snapshot data structure** (a `HashMap<u32, ActorSnapshot>`
  keyed by FormID vs. something else) — an implementation choice for
  whoever picks up the follow-up, not a design-authority decision.
- **Whether the position-divergence epsilon belongs in this system or is a
  general "how far can a REFR wander from its authored placement" concept
  useful elsewhere** — flagged, not resolved.
- **Persistent-CELL identity-comparison mechanics for item C2's half** —
  `resolve_persistent_cell` already exists and is exactly the tool item
  C2's fix needs (see the table in §1); wiring it into
  `step_cell_transition`'s Exterior arm / `execute_pending_save_loads`'s
  exterior reload / `begin_exterior_streaming` is real, sequencing-
  sensitive work against already-working transition code, deliberately not
  attempted in a design document.
- **Follow/Guard/Escort package state** — §2's trace only covered
  Sandbox/Wander/Travel, the three package systems with landed runtime
  components today. Whichever lands next should extend §4's keep/drop
  table with the same "trace the real component, don't assume" discipline,
  not copy an assumption from this document.

## 7. Recommended sequencing

1. **§3's ordinary-cell-root `FormId → EntityId` index** first — it's the
   one piece both C2's and item 4's fixes need, and it's buildable and
   testable in isolation (mirrors `persistent_ref_index.rs`'s existing test
   shape exactly: resolve/miss/cross-root-exclusion/rebuild/invalidate).
2. **EX-14/15 item C2's reconcile half** next — smaller diff, higher
   confidence (compares identity, skips work; doesn't invent a new runtime
   state store), and real regression risk to already-working transition
   code either way, so land the lower-risk half first and validate it holds
   before building the larger snapshot mechanism on the same index.
3. **EX-16 item 4's snapshot/restore** last — depends on both of the above,
   and is the larger surface (§4's keep/drop table, §5's lifetime
   management) most likely to need iteration once real actors are observed
   crossing real boundaries.

Each step should land as its own scoped change with its own tests, not as
one large patch — consistent with how every other multi-part item in
`exterior-readiness-plan.md` has been sequenced this session.
