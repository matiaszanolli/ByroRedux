## #4136 [OPEN] SAVE-D1-2026-09-11-02: Locked became runtime-mutable (#3159) but was never registered for save, and the cell-load stamp overwrites it

### SAVE-D1-2026-09-11-02: `Locked` became a runtime-mutable component this cycle (#3159's `SetLocked`/`SetLockLevel` fragment effects) but was never registered as save state, and the cell-load stamp unconditionally overwrites it

- **Severity**: HIGH
- **Dimension**: 1 — Snapshot Completeness & Determinism
- **Data-Loss Class**: silent-drop
- **Location**: `crates/core/src/ecs/components/lock.rs` (the `Locked` component: `lock_level: u8`, `key_form_id: Option<u32>` — no `FixedString`/`EntityId`, delta-safe by the same bar `MUTABLE_DELTA_COLUMNS`'s other entries pass); `crates/scripting/src/fragment/effects.rs:814-852` (`Effect::SetLocked`/`Effect::SetLockLevel`); `byroredux/src/cell_loader/spawn.rs:919-932` (the unconditional ESM-authored `Locked` stamp on every cell load/reload); `byroredux/src/save_io/registry_completeness_tests.rs:428` (the `NOT_SAVED_BY_DESIGN` entry that names the gap but does not close it); `byroredux/src/save_io.rs:324-508` (`build_save_registry` — `Locked` absent)
- **Status**: NEW. `Locked`'s mutability is new this cycle: `e26579c1` (Fix #3159) added `SetLocked`/`SetLockLevel` specifically because an authored lock was previously "a one-way door for the session." Before #3159, `Locked` was correctly write-once-from-ESM and its `NOT_SAVED_BY_DESIGN` classification was accurate.
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: The registry-completeness guard's own allowlist entry states the gap explicitly rather than hiding it, and explicitly declines to be treated as a justification:

> `("Locked", "XLOC lock data, rederived from the plugin's parsed REFR every cell load (#3098). NO LONGER rederived *identically*: #3159 added Effect::SetLocked, so a script can now unlock a door mid-session and a reload re-stamps the authored lock over it. **This is a real save-fidelity gap, not a justification** — the component needs registering once a scripted lock change is expected to survive a save...")

No tracking issue covers it. Concretely, `Effect::SetLocked` inserts/removes `Locked` and `Effect::SetLockLevel` mutates its `lock_level` field. Nothing persists that change: it is absent from `build_save_registry`, so a full save/load silently reverts to whatever the ESM authored. The *live* reload path has the identical exposure without even needing a save — `cell_loader/spawn.rs:919-932` unconditionally re-inserts `Locked` from the placement's authored `XLOC` on every cell (re)load, with no check for an already-present (scripted) value:

```rust
if let Some(l) = lock {
    world.insert(placement_root, Locked { lock_level: l.lock_level, key_form_id: l.key_form_id });
}
```

A player who picks a lock, then triggers any cell reload — a live `load <slot>`, or simply leaving and re-entering the cell in the same session — finds the door re-locked, with no record anywhere of what happened.

**Evidence**: `lock.rs` struct fields, `fragment/effects.rs:814-852`, `cell_loader/spawn.rs:919-932`, and `registry_completeness_tests.rs:428` all directly confirmed during publish. `grep -n "Locked" byroredux/src/save_io.rs crates/save/src/validate.rs` returns nothing — no registration, no validation gate.

**Impact**: Any scripted lock/unlock — vanilla content routinely uses `Lock()`/`Unlock()`/`SetLockLevel()` on quest-gating doors and containers — does not survive a save/load and does not even survive an in-session cell revisit. A door meant to stay open after a quest event re-locking itself can strand a player or re-block content the quest already unlocked.

**Related**: #3159 (introduced the mutation with no persistence companion); #3098 (the original write-once XLOC stamp, correct at the time); the analogous, already-fixed pattern for `EquippedWeapon` (#3488) and `ReferenceEnableState` (#3278/#3789) — both show the project's established fix shape for exactly this class of gap.

**Suggested Fix**: Register `Locked` in `build_save_registry` (plain `u8`/`Option<u32>`, no session-local hazard) and add it to `MUTABLE_DELTA_COLUMNS`. Gate the cell-load stamp at `spawn.rs:919-932` on the placement root not already carrying a `Locked` component from a prior overlay/restore. Remove the `NOT_SAVED_BY_DESIGN` entry once both land.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — save with a scripted `SetLocked`/`SetLockLevel` change, load, assert the change survives; also assert a live in-session cell revisit doesn't re-stamp over it
- [ ] **SIBLING**: Any other component that gained a runtime mutator recently (grep new `Effect::Set*` variants) for the same "mutable but never registered" gap


## #4138 [OPEN] SAVE-D5-2026-09-11-01: Quicksave during a mid-flight interior-cell transition writes an unloadable save

### SAVE-D5-2026-09-11-01: Quicksave during a mid-flight interior-cell transition writes a save that can never be loaded back

- **Severity**: HIGH
- **Dimension**: 5 — Frame-Boundary Capture & Off-Frame Apply
- **Data-Loss Class**: corruption-on-load (a save that appears to succeed, consumes a ring slot, but is permanently unloadable)
- **Location**: `byroredux/src/cell_loader/transition.rs:386-396` (`unload_current_interior` clears `CurrentCellRoot`/`CurrentCellContext`); `byroredux/src/app_step.rs:904-921` (`step_cell_transition`'s Interior arm: teardown → `InteriorCellApply::begin`, budgeted `advance()` after); `byroredux/src/cell_loader/load.rs:920-984` (`InteriorCellApplyJob::advance`/`finish` — context inserted only inside `finish()`, i.e. only on `Complete`); `byroredux/src/app_step.rs:33` (`STREAMING_APPLY_BUDGET = 16ms`); `byroredux/src/app_events.rs:348-368` (F5/F9 handler queues a save unconditionally, no gate on an in-flight transition); `byroredux/src/save_io.rs:787-841` (`SaveCommand::execute`'s only pre-write gates — none inspect `CurrentCellContext`/`CurrentExteriorContext` presence); `byroredux/src/save_io.rs:1128-1141` (`LoadCommand::execute` — the symmetric gate that DOES exist, but only on the load side)
- **Status**: NEW (not found in any of the 12 prior save audit cycles, and no matching GitHub issue title in a fresh keyword sweep)
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: An interior-cell transition is a resumable, budgeted, multi-frame App-owned job (`self.interior_transition`, deliberately kept off the ECS `World`). `step_cell_transition`'s Interior arm first calls `unload_current_interior` (clearing both `CurrentCellRoot` and `CurrentCellContext`), then starts `InteriorCellApplyJob::advance` against a 16ms/frame budget. For any cell whose reference load doesn't fit in one slice — the entire reason the resumable design exists — `advance()` returns `Pending` and is resumed next frame. `CurrentCellContext`/`CurrentCellRoot` are inserted **only** inside `finish()`, on the terminal `Complete` arm. For the entire `Pending` window (which can span many frames for a real interior), the live `World` has **neither** cell nor exterior context installed — indistinguishable from genuine loose-NIF mode.

`step_player_save_actions` runs every frame **before** `step_cell_transition` and has no visibility into `self.interior_transition`. The F5/F9 handler queues a quicksave unconditionally — there is no check against an in-flight transition (contrast with `step_save_loads`, which explicitly guards `self.interior_transition.is_some()` on the *load* side — no equivalent exists for *save*).

If a player quicksaves while a large interior cell is still streaming in, `SaveCommand::execute`'s three referential-integrity gates (none of which check context presence) all pass, and the write succeeds and is reported as success. The resulting snapshot has neither context in its resources map. `LoadCommand::execute` correctly refuses to queue it, but reports it with the same message used for the legitimate loose-NIF case — there is no way to distinguish the two, and no way to recover the location.

**Evidence**: Full call-site trace re-confirmed during publish — `cell_loader/transition.rs:386-396`, `app_step.rs:904-921`, `cell_loader/load.rs` (`CurrentCellContext`/`CurrentCellRoot` inserted only at the two `finish`-adjacent sites, `load.rs:698,703` and `:1042-1043`), and the `about_to_wait` ordering (`capture_player_pose` → `step_player_save_actions` → `step_save_loads` → `step_cell_transition`, same tick). `step_save_loads`'s `interior_transition.is_some()` guard confirmed at `app_step.rs:742`; no equivalent gate exists in `SaveCommand::execute` (`save_io.rs:780-845`) or the F5/F9 handler (`app_events.rs:348-368`).

**Impact**: A player who quicksaves during any door transition into a cell heavy enough to exceed 16ms of reference-load work in one frame gets a save that reports success, occupies a ring slot (evicting whatever it would otherwise have kept), and can never be loaded again. The failure surfaces only later, at load, as an ambiguous "loose save" message.

**Related**: Symmetric to the already-correct `LoadCommand::execute` gate and to `step_save_loads`'s existing `interior_transition`-awareness on the load side — the fix is the missing third leg of a pattern the codebase already applies twice.

**Suggested Fix**: Give `SaveCommand::execute` (and/or `quicksave`) the same context-presence gate `LoadCommand::execute` already has. Since `self.interior_transition` is App-only, either (a) surface a lightweight `TransitionInFlight` marker resource that `step_cell_transition` sets/clears, checked by both `SaveCommand::execute` and the F5/F9 handler, or (b) have the F5/F9 handler's caller check `self.interior_transition.is_none()` before queuing, with `step_player_save_actions` re-checking before draining. Distinguish the resulting rejection message from the genuine loose-NIF-save case.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — simulate a `Pending` interior transition, attempt a quicksave, assert it is refused (or deferred) rather than writing an unloadable save
- [ ] **SIBLING**: Check the exterior streaming path for the analogous window (a save taken mid-`FullRadius` bootstrap before delta remap settles)


## #4139 [OPEN] SAVE-D4-2026-09-11-01: PapyrusProviderContinuationQueue persists unvalidated, unrebound EntityRef locals

### SAVE-D4-2026-09-11-01: `PapyrusProviderContinuationQueue` persists raw session-local `EntityRef` handles inside `ScriptValue::Entity` locals — a newly-registered save column carrying an inter-entity reference type none of the nine gates inspect, and whose own registration comment's safety claim is false

- **Severity**: MEDIUM
- **Dimension**: 4 — Validation Gates
- **Data-Loss Class**: latent silent-drop (common path: `entity_resolver` unavailable → continuation dropped with a warning) escalating to **corruption-on-load** under a specific condition: a stale `EntityRef` misresolves to a *different* live entity rather than failing to resolve.
- **Location**: `byroredux/src/save_io.rs:495-500` (registration + the false claim); `crates/scripting/src/papyrus_provider/ir.rs:98-113`; `crates/scripting/src/papyrus_provider/execute.rs:256-273,299-315,340-355`; `crates/sdk/src/identity.rs:219-227,296-306` (`EntityRef`, scoped to a `world_generation`); `crates/sdk/src/script_function.rs:56` (`ScriptValue::Entity(EntityRef)`); `byroredux/src/extensions/mod.rs:301-347` (`EntityHandleRegistry`); `byroredux/src/extensions/persist.rs:341-405` (`preflight_extension_state`/`restore_extension_state`'s early-`Ok` short-circuit when the saved `ExtensionStateSnapshot` is empty)
- **Status**: NEW. `PapyrusProviderContinuationQueue` was registered in this delta window (`3a0ce9bb`), so this could not have been reported by any prior audit.
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: The registration comment states *"All nested identities are stable manifest strings; no EntityId or process-local handles cross the save boundary."* This is false: `pending: Vec<PendingPapyrusProviderContinuation>`'s `locals: BTreeMap<String, ScriptValue>` can hold `ScriptValue::Entity(EntityRef)` — the SDK's own doc comment calls `EntityRef` "never a raw ECS slot... scoped to a world_generation so a stale one is rejectable," i.e. a process-local handle by definition. `execute.rs` inserts `self`/entity-typed locals at handler dispatch, and if that handler hits `Utility.Wait(...)`, the same map is captured verbatim into a suspended continuation with no rebinding at save time.

Whether a stale post-load `EntityRef` is safe depends on `EntityHandleRegistry::begin_world_generation()`, called once inside `ExtensionHost::restore_saved_state`, reached via `restore_extension_state`. But `preflight_extension_state`/`restore_extension_state` **both short-circuit to `Ok` before calling into the host** when the save's `ExtensionStateSnapshot` has `rows.is_empty() && principal_storage.is_empty()` — a condition entirely orthogonal to whether `PapyrusProviderContinuationQueue` itself is non-empty. A session using the Papyrus-provider path but with no persisted SDK extension rows hits this short-circuit on every load: `begin_world_generation` never runs, and a stale `EntityRef` in the just-restored queue resolves via the untouched `by_handle` map to whatever `EntityId` it mapped to *before* the reload — which, in a freshly reloaded world with comparable spawn order/count, likely now names a **different, currently-live entity**. The resumed continuation then acts on the wrong object.

**Evidence**: `save_io.rs:495-500`'s comment vs. `script_function.rs:56`'s `ScriptValue::Entity(EntityRef)` variant, both confirmed present verbatim during publish. `persist.rs:348,356,376,384` confirmed as the four early-return sites gated purely on `rows.is_empty() && principal_storage.is_empty()`. The only existing round-trip test for this column (`round_trip_tests.rs:872-983`) uses a fixture that touches no `self`/entity-typed local and sets no `entity_resolver` — it proves the string/form half survives a round trip and says nothing about the `Entity`-valued-local half.

**Impact**: Under a real, reachable condition (a Papyrus-provider `Utility.Wait` pending on a handler using `self`, at a save taken while the SDK extension host is active but has zero persisted rows), a resumed continuation after load can silently execute against the wrong live entity. In the more common "no ExtensionHost" case, the continuation is simply dropped with a log warning — a milder, silent feature drop.

**Related**: Same class as the now-fixed `SAVE-D4-2026-08-30-03` (a saved column carrying a reference type validation doesn't know about) and `SAVE-D6-2026-08-30-01` (a load-time consumer outrunning its protecting restore/rebind step) — distinct from both: a new column, in the SDK's `EntityRef` (not a core `EntityId`), whose own existing rebind mechanism is bypassed by an early-return rather than absent outright.

**Suggested Fix**: Either (a) drop pending continuations carrying entity-typed locals across a save/load with a diagnostic, since a stale receiver handle can't be safely honored without the rebind this column lacks; or (b) persist entity-typed locals by stable `FormRef` and rebind through `entities_by_form` unconditionally (not gated on `saved.rows`/`principal_storage` non-emptiness). At minimum, correct the registration comment, and add a round-trip test exercising a `self`-referencing `Wait()` continuation across the live load path (not just `restore_world`).

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix — a `self`-referencing `Utility.Wait()` continuation saved with an empty `ExtensionStateSnapshot`, loaded, and asserted to either rebind correctly or be safely dropped (not silently misresolve)
- [ ] **SIBLING**: Any other saved column carrying `ScriptValue::Entity`/`EntityRef` payloads should be checked against the same early-return gap


## #4140 [OPEN] SAVE-D2-2026-09-11-01: v22 FORMAT_MAJOR bump rationale (discriminant shift) is factually wrong for serde_json

### SAVE-D2-2026-09-11-01: v22's `FORMAT_MAJOR` bump justification ("discriminant shift") is factually wrong for this codebase's serde_json format — a load-bearing precedent now teaches an incorrect model of what actually requires a bump

- **Severity**: MEDIUM
- **Dimension**: 2 — Registry & (De)serialization Fidelity
- **Data-Loss Class**: None directly (the bump is conservative, not permissive) — but the incorrect stated mechanism is now the citable precedent for future bump/no-bump calls, where the same error in the opposite direction would be a real compatibility bug.
- **Location**: `crates/scripting/src/translate/effects.rs:100` (`enum Effect`, plain `#[derive(Serialize, Deserialize)]`, no tag/repr override); commit `e26579c1` (Fix #3159); baseline comment at `byroredux/src/save_io/serde_default_guard_tests.rs:440-447`; `crates/save/src/snapshot.rs:212` (payload is `serde_json` of `Snapshot`)
- **Status**: NEW
- **Source**: `docs/audits/AUDIT_SAVE_2026-09-11.md`

**Description**: `e26579c1`'s commit message states that adding enum variants "shifts later discriminants under serde's index-based representation" so a pre-v22 snapshot would deserialize as the wrong effect. This is incorrect for the actual on-disk format: `Effect` uses serde's default *externally tagged* JSON representation (confirmed at `effects.rs:99-100`: `#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]`, no tag/repr attribute), which serializes a data-carrying variant as `{"<VariantName>": {…}}` keyed by the Rust variant **name**, never by ordinal. `serde_json`'s `Serializer` ignores the `variant_index` serde_derive passes it — only non-self-describing binary formats care about it, and this codebase doesn't use one for saves. Inserting `SetLocked`/`SetLockLevel` at any position is therefore backward-compatible for deserializing a pre-v22 tail exactly like `Effect::Enable` (#3489, correctly *not* bumped, with the commit's own correct reasoning: "serde only has to recognize the tags actually present"). The two commits give directly contradictory technical justifications for structurally identical changes on the same enum, three commits apart in the same file. Independently corroborated by `docs/audits/AUDIT_INCREMENTAL_2026-09-09.md` §4 for the unrelated `FloatTarget`/`ColorTarget` twin-deletion change, which states the correct principle for the identical format.

**Evidence**: `effects.rs` enum declaration (no tag/repr attribute) confirmed during publish; `snapshot.rs:212`; `#3489`'s own (correct) commit message for the same enum three bumps earlier.

**Impact**: No data loss — the bump fails closed (a clean `UnsupportedVersion` rejection, not silent corruption), consistent with the subsystem's "refuse rather than corrupt" thesis. The cost is (a) every pre-#3159 save was unnecessarily invalidated, working against the subsystem's own "don't make players lose progress" goal, and (b) the precedent-bank comment at `serde_default_guard_tests.rs` — explicitly written for future contributors deciding whether their own change needs a bump — now contains one entry with a wrong stated mechanism, risking either an unnecessary future bump or, more dangerously, an over-generalized "variant insertions are always safe" applied to a case that actually changes an *existing* variant's field shape.

**Related**: `#3489` (the correct precedent this contradicts); `#3159`/`e26579c1` (the commit under review); `docs/audits/AUDIT_INCREMENTAL_2026-09-09.md` §4.

**Suggested Fix**: Correct the `serde_default_guard_tests.rs:440-447` comment and the `e26579c1` record to state the real reason the bump was conservatively taken (not worth reverting now that it shipped). If a genuinely index-sensitive save format is ever adopted, this whole "no bump needed for variant insertion" precedent class needs re-auditing together.

## Completeness Checks
- [ ] **TESTS**: No test change needed — this is a comment-only fix; confirm the existing `saved_type_shape_changes_require_format_major_bump` guard's baseline stays green after the comment edit
- [ ] **SIBLING**: Scan `serde_default_guard_tests.rs`'s other bump-history comments for the same "discriminant shift" framing on an externally-tagged enum


