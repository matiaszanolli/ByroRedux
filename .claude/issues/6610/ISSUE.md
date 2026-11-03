# RT-2026-10-31-03: `m47-3-obscript` red: `inv.use` of NukaCola is refused, so its ScriptEffectStart never runs

Labels: medium,gameplay,inventory,scripting,bug,game:fnv
Source: docs/audits/AUDIT_RUNTIME_2026-10-31.md

**Source**: `docs/audits/AUDIT_RUNTIME_2026-10-31.md` RT-2026-10-31-03 (report HEAD `670f81917`).
**Re-verified at HEAD `7f2d98944`**: `byroredux/src/inventory/` is unchanged since the report; the warning at `inventory/mod.rs:183` still names no precondition.

- **Severity**: MEDIUM (a live behavioural gate for the M47.3 chem path is red; not attributed)
- **Dimension**: Gate matrix / gameplay runtime
- **Location**: gate `docs/smoke-tests/m47-3-obscript.sh:169-177`; consumer `byroredux/src/inventory/actions.rs:88` (`consume_item`), warning at `byroredux/src/inventory/mod.rs:183`
- **Status**: NEW
- **Game / Cell**: fnv / GSProspectorSaloonInterior (`--player`)
- **Baseline**: PASS when the gate landed (`cd8744718`, 2026-10-18)
- **Current**: `inv.add 0x0001519E 1` succeeds, `inv.use 0x0001519E` reports "queued use of row N", then the engine logs `WARN byroredux::inventory: queued inventory action was unavailable for the current player/item`. No `Used …` notification, no `ObScript effects`. Every earlier leg of the gate (OnLoad, OnTriggerEnter, OnDeath, Resurrect, getters, item OnAdd) PASSes. Reproduced by hand on a clean engine.
- **Description**: `consume_item` returns `Unavailable` on a long list of preconditions: `Dead`, equipped row, no `InventoryCatalog` entry or no effects, `ActorVitals`/`ActorValues` missing, a non-finite health, an effect actor value the player's `ActorValues` lacks, a stale `ItemInstancePool` slot. I did not isolate which one fails for NukaCola. The consume path changed in M43's crafting / improved-items work and in `258c6d9b2` (#6356 split), after the gate was green.
- **Evidence**: `/tmp/byro-m47-3.tGgDpB/{use.log,effects.log,session.stderr}` and `/tmp/audit/runtime/nuka/err.log`.
- **Impact**: A player using a chem or food with a script effect gets no feedback and no effect, and the item stays in the inventory (the refusal happens before the count drops).
- **Suggested Fix**: Log the refusing precondition in `consume_item` (the current warning does not name it), then fix the one that fires.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related call sites
- [ ] **TESTS**: A regression test pins this specific fix
- [ ] **TESTS**: The gate is re-run live and passes (or SKIPs with 77 on missing data)
