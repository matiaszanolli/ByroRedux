# SCR-2026-10-27-01: Oblivion's caller-scoped `GetCrimeGold` / `SetCrimeGold` / `ModCrimeGold` / `GetGold` ignore the calling reference and read or write the player's state

Labels: high, scripting, legacy-compat, game:oblivion, bug
Source: docs/audits/AUDIT_SCRIPTING_2026-10-27.md

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-10-27.md` finding `SCR-2026-10-27-01` (HIGH)
**Location (re-mapped to current tree)**: `crates/scripting/src/obscript_more/state.rs` (`get_crime_gold`, `cmd_1075`, `cmd_1076`, all taking `_ctx`, ~:35-47; rows ~:638-640 with `needs_caller: false`); `crates/scripting/src/obscript_more/tail.rs` (`get_gold` ~:573-581, row ~:831)

**Severity**: HIGH (vanilla quest path; corrupts saved state; drives guard aggression). Dimension: Legacy ObScript. Game: Oblivion. Untrusted input: No. The handlers predate the window; they became harmful when e161c45f3/73924db73/1a67c9f30 made the bounty drive guards, jail and dialogue.

## Description
The CS wiki defines all four as actor functions: *SetCrimeGold* "Sets the current crime gold on an actor"; *ModCrimeGold* "Modifies the current crime gold on an actor"; *GetCrimeGold* "Returns the current crime gold on an actor"; *GetGold* `[ActorID.]GetGold` "Returns the actor's gold".

The handlers never read `ctx.caller`. Each call reads/writes `ObScriptLedger.crime_gold` (the player's bounty) or counts the player's `Gold001`. The command table is consulted before the CTDA-getter fallback (`ObScriptCommandHost::condition_getter`), so for id 0x1074 the row shadows the condition evaluator's arm, which *does* respect the Run-On subject (`ConditionFunction::GetCrimeGold => guards::bounty_of(world, entity)`, 0 for anyone but the player).

## Evidence
An Oblivion.esm `SCTX` census found these non-player callers:
- `GlarthirRef.modcrimegold 5000` x4, all MS38 "Paranoia": three are `begin ondeath glarthirref` blocks on his victims' scripts; the fourth is the stage item "I showed Glarthir's note to a town guard...", whose result script reads `set ms38.arrestInProgress to 1 / modpcfame 1 / glarthirref.modav aggression 20 / glarthirref.modcrimegold 5000`.
- `martinref.setcrimegold 0` x2 and `OcatoRef.setcrimegold 0` (main quest).
- `OrumGangCourier1Ref.getgold` and `DulfishgroOrumRef.getgold` (an Imperial City ambient package script).
Verified at HEAD: all three crime-gold handlers take `_ctx` and the rows are `needs_caller: false`; `get_gold` ignores `_ctx` and uses `player(world)`.

## Impact
- Reporting Glarthir to the guard adds 5000 to the **player's** bounty; `crime.rs::rush_wanted_player` (bounty >= `rush_bounty`, 500) then force-greets every guard who detects the player, and `IsTurnArrest` and the arrest dialogue follow. `ObScriptLedger` is saved, so the bounty survives a save.
- The main-quest result scripts wipe the player's real bounty.
- The Orum courier's `goldCount` is the player's gold, and its `removeitem gold001 goldCount` acts on that number.

## Related
`GetCrimeGold` condition arm (`condition/eval.rs`); #6187 (the same handlers' saturation). Audit note: of 377 command rows, 272 have `needs_caller: false`; each row whose CS/GECK page shows `[ActorID.]` syntax should use `caller(...)` or prove it is player-only - this is the first miss found.

## Suggested Fix
Resolve the caller in each of the four handlers: player (or no caller) keeps today's ledger/player-inventory behaviour; any other actor, `GetGold` reads that actor's own `Inventory`; any other actor, the crime-gold trio declines (no NPC bounty is modelled; `bounty_of` already answers 0 on the condition side, and the two should agree). Add a regression test that runs `GlarthirRef.ModCrimeGold 5000` and asserts the player's ledger is unchanged.

## Completeness Checks
- [ ] **SIBLING**: Other `needs_caller: false` rows whose CS/GECK syntax is `[ActorID.]` audited (mechanical caller audit of the command table)
- [ ] **TESTS**: A regression test pins this specific fix

