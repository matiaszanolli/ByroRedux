# Issue #3489
title:	SCR-D5-2026-08-27-02: Effect::Disable shipped without an Enable counterpart over a save-persisted resource — a latent one-way door, and 3,005 real Enable() calls decline
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, medium, quests, save-load, scripting
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3489
--
- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs:476-513` (`EFFECT_PRIMITIVES` — no `prim_enable`); `:803-812` (`prim_disable`); `crates/scripting/src/fragment.rs:63-84` (`ReferenceEnableState`, whose `set_enabled(form_id, bool)` API already supports both directions)
- **Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`

## Description

`5f38402e` added `Effect::Disable` and the save-serialized `ReferenceEnableState` resource it writes into, but no `Enable` primitive. The resource's own API is symmetric (`set_enabled` takes a `bool` and removes from the `disabled` set when `true`), and it is registered for save persistence (`byroredux/src/save_io.rs:438`, `.register_resource::<ReferenceEnableState>("ReferenceEnableState")`), so the *state model* is complete — only the lowering half is one-directional.

In the real corpus, `Enable()` is the **more common** of the pair:

```
disable  args=1  count=2587
enable   args=1  count=3005
```

Every fragment containing an `Enable()` call therefore declines in full today (the whole-fragment lowering contract), and once `ReferenceEnableState` gains the runtime consumer #3278 asks for, a reference a script disables can never be re-enabled by script — the disable survives save/load by design.

## Evidence

`EFFECT_PRIMITIVES` contains `prim_disable` at `effects.rs:493`; a grep for an `Enable` sibling finds only `prim_enable_player_controls` (`:500`, an unrelated `Game.EnablePlayerControls` primitive) — there is no `ObjectReference.Enable` lowering, and no `Effect::Enable` variant anywhere in `crates/scripting`. `ReferenceEnableState::set_enabled` (`fragment.rs:76-82`) has the `enabled == true` branch that no caller ever reaches:

```rust
pub fn set_enabled(&mut self, form_id: u32, enabled: bool) {
    if enabled {
        self.disabled.remove(&form_id);   // no production caller
    } else {
        self.disabled.insert(form_id);
    }
}
```

The single production caller is `fragment.rs:577` (`state.set_enabled(form_id, enabled)` draining `reference_enable_changes`), fed only by the `Disable` arm.

## Impact

Today, inert — nothing consumes `ReferenceEnableState` (#3278), so neither half does anything observable. **This finding's severity is conditional on #3278 being fixed**: the moment a consumer lands, disabling becomes permanent and unrecoverable across saves, and a `Disable`/`Enable` pair authored to hide a reference for one quest stage will hide it forever. Fixing #3278 without fixing this would ship a strictly worse state than either fix alone. Also caps fragment coverage: 3,005 guaranteed declines.

## Related

#3278 (`Effect::Disable` has no production consumer, and its receiver resolution is narrower than its siblings) — same commit, same effect, must be fixed together. Structurally identical to #3159 (a `Lock` with no `Unlock`), which the 08-20 pass already named as a one-way door.

## Suggested Fix

Add `prim_enable` mirroring `prim_disable` (same `receiver_object` treatment, same optional literal `abFadeIn` argument) and an `Effect::Enable` variant dispatching to `deferred.reference_enable_changes.push((form_id, true))`. Land it in the same change as #3278's consumer, not after.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other one-way effect pairs — `Lock`/`Unlock` per #3159 — and other `EFFECT_PRIMITIVES` entries whose state resource is symmetric)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix


---

# Issue #3491
title:	SAVE-D1-2026-08-27-02: the `Perks` completeness-allowlist reason cites a `validate_progression_state` guard that never inspects `Perks`
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, character, medium, save-load, test-gap
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3491
--
Audit: `docs/audits/AUDIT_SAVE_2026-08-27.md` (SAVE-D1-2026-08-27-02)
Severity: **MEDIUM** · Dimension 1 — Snapshot Completeness & Determinism
Data-Loss Class: latent silent-drop (no loss today — `Perks` has zero production mutators)

## Location
- `byroredux/src/save_io/registry_completeness_tests.rs:108` — the allowlist reason
- `byroredux/src/save_io/round_trip_tests.rs:757`, `:781` — the `REDERIVED_NOT_SAVED` preamble repeating the claim
- `crates/save/src/validate.rs:410-442` — `validate_progression_state`, which never mentions `Perks`

## Description
The SAVE-D1-12 allowlist entry reads:

```rust
("Perks", "known progression gap guarded by validate_progression_state: saves are refused once perks exist (#2947)"),
```

and `round_trip_tests.rs`'s `REDERIVED_NOT_SAVED` preamble repeats the claim for both types: *"`crates/save/src/validate.rs::validate_progression_state` aborts any save where a `CharacterLevel.xp != 0` slips through with these two still unregistered, so the exemption fails loudly rather than silently discarding progress."*

`validate_progression_state` reads `Perks` nowhere. Its whole body is:

```rust
fn validate_progression_state(world: &World, errors: &mut Vec<ValidationError>) {
    let Some(q_level) = world.query::<CharacterLevel>() else { return; };
    for (entity, level) in q_level.iter() {
        if level.xp != 0 { … }
    }
}
```

Its own doc comment (`validate.rs:410-422`) is scrupulously accurate and only ever discusses `CharacterLevel.xp` — the over-claim exists solely in the two allowlist reasons. The stated trigger ("once perks exist") is also already false in the component sense: `scene.rs:1393` inserts `Perks::default()` on the player body unconditionally (#3158), and `npc_spawn.rs:178-186` inserts a populated `Perks` for any NPC with `PRKR` entries. Saves are not refused, correctly, because `Perks` is genuinely write-once from ESM today — but that is a *different* argument from the one the allowlist makes.

## Evidence
- `crates/save/src/validate.rs:424-442` — the complete function body, quoted above; `grep -rn "Perks" crates/save/src/` returns **zero** hits, so the only `Perks` reference anywhere in `crates/save` is absent.
- `grep -rn "get_mut::<Perks>\|query_mut::<Perks>\|resource_mut::<Perks>" byroredux/src crates/` returns zero hits, confirming the exemption is *substantively* safe today.
- `byroredux/src/scene.rs:1393` — `world.insert(body, byroredux_core::character::Perks::default());`.

## Impact
The completeness ledger is this subsystem's primary silent-drop defence, and the audit skill's own Dimension 1 instruction is to consume its reasons rather than re-derive them — so a wrong reason propagates directly into every future audit and every future reviewer's mental model. The day an `AddPerk` effect or a perk-selection UI lands (`docs/engine/charal.md`'s perk work, #3004/#2986), the author will read "guarded by `validate_progression_state`", conclude the gate will catch them, and ship a runtime that silently discards every granted perk on save — with the guard test still green, because a green `NOT_SAVED_BY_DESIGN` entry only asserts that *a* reason exists, not that it is still true.

## Related
#2947 (the `CharacterLevel` half, which is correctly implemented); #3158 (the unconditional player `Perks` stub); the audit skill's own note that the guard "enforces a reason exists, not that it's still true".

## Suggested Fix
Either extend `validate_progression_state` to also flag a non-empty `Perks` on any entity (making the claim true and giving the future perk runtime the loud failure the ledger promises), or rewrite both reasons to state the real justification — "`Perks` is stamped verbatim from `NPC_.PRKR` at spawn with no production mutator (`grep` for `get_mut::<Perks>`); register it the moment an `AddPerk` effect lands." Prefer the former: it costs four lines and makes the ledger self-enforcing rather than self-describing.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every other `NOT_SAVED_BY_DESIGN` / `REDERIVED_NOT_SAVED` reason that names a guard, checked against what that guard actually inspects
- [ ] **TESTS**: A regression test pins this specific fix


---

# Issue #3497
title:	SAVE-D1-2026-08-27-03: the SAVE-D1-12 completeness guard's `SCAN_ROOTS` cannot notice a new crate — `crates/sdk` is unscanned
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, medium, save-load, test-gap
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3497
--
Audit: `docs/audits/AUDIT_SAVE_2026-08-27.md` (SAVE-D1-2026-08-27-03)
Severity: **MEDIUM** · Dimension 1 — Snapshot Completeness & Determinism
Data-Loss Class: latent silent-drop (no loss today — `StudioSession` is authoring-tool state, correctly not save-worthy)

## Location
- `byroredux/src/save_io/registry_completeness_tests.rs:362-369` — `SCAN_ROOTS`
- `crates/sdk/src/studio.rs:120` — `impl Resource for StudioSession {}`

## Status
NEW — made possible by `21a840d5` ("feat: introduce byroredux-sdk"), the first new workspace crate to define ECS state since the guard was written.

## Description
The guard's scan set is:

```rust
const SCAN_ROOTS: &[&str] = &[
    "../crates/core/src",
    "../crates/scripting/src",
    "../crates/physics/src",
    "../crates/audio/src",
    "../crates/plugin/src",
    "../byroredux/src",
];
```

It has a strong self-defence against a root *moving* (`collect_rs_files` panics on an unreadable directory — *"moved — update SCAN_ROOTS"* — and a `!found.is_empty()` assert catches the impl-line shape changing) but none at all against a root that was never added. `crates/sdk/src/studio.rs:120` declares `impl Resource for StudioSession {}`, and `StudioSession` is neither registered in `build_save_registry` nor listed in `NOT_SAVED_BY_DESIGN`. The guard is green because it simply never looks there (`grep -c sdk byroredux/src/save_io/registry_completeness_tests.rs` → 0).

`StudioSession` itself is correctly excluded on the merits — it is a Studio authoring document holding `Vec<EntityId>` / `Option<EntityId>` / a `BTreeMap<EntityId, TransformValue>`, all session-local identity, installed only when the Studio host is active (`byroredux/src/app_events.rs:163-168` opens the Studio panel when it is present). So there is no live data loss. The defect is that the ledger's *coverage* silently shrank relative to the workspace, in exactly the way the ledger exists to prevent.

## Evidence
`grep -rn --include='*.rs' "^impl Component for \|^impl Resource for " crates/ | grep -vE "^crates/(core|scripting|physics|audio|plugin|save)/"` returns four hits: `crates/sdk/src/studio.rs:120` (`StudioSession`), `crates/debug-ui/src/lib.rs:179` (`DebugUiState`), and `crates/renderer/src/vulkan/allocator.rs:49,70` (`AllocatorResource`, `GpuMemoryBudget`). The last three are unambiguously renderer/overlay infrastructure and predate the guard; `StudioSession` is the new one, and it is the only one of the four that carries a *document* rather than a device handle. `_audit-common.md`'s crate list is 25 entries against the guard's six roots.

## Impact
The SDK is described in `_audit-common.md` as *"the first tooling API surface"* and has no owner audit skill of its own. If Studio grows a document field that is genuinely game state (a persisted scene edit, a per-asset material override the engine should reload), it will land unnoticed by the one guard whose job is to notice exactly that. The cost of the miss compounds: the guard's green run is cited in this report and every prior one as "the completeness ledger", so an unscanned crate is not merely unchecked, it is affirmatively reported as checked.

## Related
#2295 / #3166 (the guard and its last `SCAN_ROOTS` widening); `21a840d5`; the "ByroRedux SDK — no dedicated owner" row in `_audit-common.md`'s un-owned-subsystems table; #3457 (the sibling doc-rot instance — `_audit-common.md`'s Project Layout gives `crates/sdk` no row).

## Suggested Fix
Replace the hardcoded list with a discovery step — enumerate `crates/*/src` from the workspace root and subtract an explicit, reasoned `NOT_SCANNED` set (`renderer`, `debug-ui`, `ui`, `save` itself, the parser-only crates) — so adding a crate forces a deliberate classification instead of silently widening the blind spot. Failing that, add `"../crates/sdk/src"` now and give `StudioSession` a `NOT_SAVED_BY_DESIGN` entry ("Studio authoring document holding session-local `EntityId`s; the edited world state it describes is saved through the normal component columns").

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — the other three out-of-root `impl Resource` sites (`DebugUiState`, `AllocatorResource`, `GpuMemoryBudget`) get an explicit classification too
- [ ] **TESTS**: A regression test pins this specific fix (a new crate defining ECS state must fail the guard until classified)


---

# Issue #3500
title:	SAVE-D3-2026-08-27-05: `save.info` still reports every exterior save as `<none — loose/exterior save>` and never prints its `CurrentExteriorContext`
state:	OPEN
author:	matiaszanolli (Matias Zanolli)
labels:	bug, low, save-load
comments:	0
assignees:	
projects:	
milestone:	
issue-type:	
parent:	
sub-issues:	
sub-issues-completed:	
blocked-by:	
blocking:	
number:	3500
--
Audit: `docs/audits/AUDIT_SAVE_2026-08-27.md` (SAVE-D3-2026-08-27-05)
Severity: **LOW** · Dimension 3 — Disk Format & Durability (operator diagnostics)
Data-Loss Class: none

## Location
`byroredux/src/save_io.rs:869-878` (`SaveInfoCommand::execute`)

## Description
`SaveInfoCommand::execute` was not updated when exterior save/load shipped (`0a847910`, EX-09/17 item 4):

```rust
match snapshot_cell_context(&snap) {
    Some(ctx) => lines.push(format!("  cell: {} (esm {}, {} master(s))", …)),
    None => lines.push("  cell: <none — loose/exterior save>".to_string()),
}
```

`snapshot_exterior_context` exists (`save_io.rs:464-471`) and `LoadCommand` already uses it to build a `"worldspace '{}' @ ({},{})"` destination label — `save.info` is the one consumer that never got the second arm. An operator inspecting an exterior quicksave is told it is a loose save that cannot be live-loaded, which is the opposite of the truth.

## Evidence
`byroredux/src/save_io.rs:869-878` (quoted) versus `:1022-1037` (`LoadCommand`'s three-arm `match (snapshot_cell_context(…), snapshot_exterior_context(…))`). `grep -rn "snapshot_exterior_context" byroredux/src/save_io.rs` returns three sites — the definition at `:464`, `LoadCommand` at `:1027`, and the load drain at `:1343` — none in `SaveInfoCommand`. The resource *is* listed later by the generic `for name in snap.resources.keys()` loop, but only as a bare `resource CurrentExteriorContext` line with no worldspace or grid.

## Impact
Diagnostic only — no save or load behaviour changes. It matters because `save.info` is the operator's only pre-load inspection tool over `byro-dbg`, and it now actively contradicts `load`'s own classification of the same file.

## Related
`0a847910` (EX-09/17 item 4); SAVE-D6-2026-08-24-02 / #3028, the doc-side instance of the same omission, now fixed in `5458522d`.

## Suggested Fix
Mirror `LoadCommand`'s three-arm match — print the worldspace key, grid, and load radius for the exterior arm, and reserve `<none — loose save>` for a snapshot carrying neither context.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files — every other `snapshot_cell_context` consumer that predates exterior save/load
- [ ] **TESTS**: A regression test pins this specific fix (`save.info` on an exterior snapshot prints the worldspace/grid)


---

