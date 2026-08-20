# #3219 — SKY-2026-08-20-D4-01: TES5 RACE.DATA starting Magicka (@40) and Stamina (@44) are never parsed — no Skyrim actor ever gets a Magicka or Stamina actor value

**Issue**: #3219 — https://github.com/matiaszanolli/ByroRedux/issues/3219
**Finding ID**: `SKY-2026-08-20-D4-01`
**Severity**: MEDIUM
**Dimension**: 4 — this title's data through the shared parser
**Audit**: `/audit-skyrim` — `docs/audits/AUDIT_SKYRIM_2026-08-20.md` (HEAD `bb0b92f2`, 2026-08-20 comprehensive suite)
**Labels**: medium, legacy-compat, import-pipeline, bug
**Filed**: 2026-08-20 · `/audit-publish`

---

**Audit**: `/audit-skyrim` — `docs/audits/AUDIT_SKYRIM_2026-08-20.md` (Dim 4 — this title's data through the shared parser), HEAD `bb0b92f2`
**Finding ID**: `SKY-2026-08-20-D4-01`

- **Severity**: MEDIUM
- **Status**: NEW — residual of CLOSED **#2455** (which named the whole Skyrim+ `RACE.DATA` gap; the fix that landed reads only `starting_health`)

## Location

- `crates/plugin/src/esm/records/actor/mod.rs:441` — `pub starting_health: Option<f32>` is the **only** resource read from `RACE.DATA` (populated at `:1238-1240`)
- Consumer: `crates/plugin/src/esm/records/actor_value_derive.rs` — `derive_skyrim_actor_values`

## Description

`NpcStatModel::RaceBaseOffsets` is documented as "race resource bases plus signed NPC offsets" (**plural**), and the NPC side holds all three:

- `magicka_offset` (`ACBS i16 @ 4`) — `actor/mod.rs:250`
- `stamina_offset` (`i16 @ 6`) — `:253`
- `health_offset` (`i16 @ 20`) — `:256`

`magicka_offset`'s own docstring says it is *"parsed alongside Health … so the three TES5 resource offsets stay one verified wire-layout unit"*. The **race** side never got the other two: only `RACE.DATA f32 @ 36` is read. `derive_skyrim_actor_values` therefore returns a one-element vector:

```rust
let health = starting_health + f32::from(npc.health_offset);
…
vec![(health_key, health)]
```

## Evidence

All 99 `Skyrim.esm` `RACE` records carry a 164-byte `DATA`, and the three floats at 36/40/44 are the resource triple:

```
ManakinRace       @36,40,44 = [ 50.0,  50.0,  50.0]
UndeadDragonRace              [500.0, 150.0, 100.0]
DraugrMagicRace               [ 50.0,   0.0,  80.0]
FoxRace                       [ 12.0,   0.0, 200.0]
distinct values across 99 races:  @36: 21   @40: 8   @44: 13
```

`@40` and `@44` vary independently of `@36` and take only plausible resource magnitudes (0/4/5/15/50/100/150/200 and 0/10/15/20/25/50/75/80/…), which is what identifies them.

`AVMagicka 0x3E9` and `AVStamina 0x3EA` are both authored in `Skyrim.esm` and resolve fine — **nothing ever asks for them.**

(Standing fact, not in dispute: `Skyrim.esm` *does* author `AVHealth 0x3E8`; the Health path is correct.)

## Impact

**100 % of Skyrim actors** (5 118 `NPC_` records plus the player) carry an `ActorValues` map containing exactly one entry. Every consumer that keys off Magicka or Stamina is silently inert on Skyrim:

- the CTDA evaluator's `GetActorValue` (`crates/scripting/src/condition.rs:419-431`, a direct `ActorValues` lookup)
- `setav` / `modav` (`byroredux/src/commands/actor_value.rs`)
- `pool_regen_tick_system`
- the Skyrim ruleset's own `CarryWeight = f(Stamina)` derivation (`crates/core/src/character/skyrim.rs:134`)

Not a regression — the data has never had a reader — but it is the **load-bearing prerequisite** for wiring the Skyrim CHARAL ruleset, so it will block that work.

## Related

- **#2455** (CLOSED) — "Skyrim+/FO4/FO76/Starfield RACE DATA sub-record is never decoded". The Skyrim arm now exists but reads one of three resource floats; this is that issue's residual.
- **#3170** — the Skyrim CHARAL ruleset is unreachable (`RulesetBuilder::None`); wiring it makes this gap live rather than dormant.
- **#3169** — the `Illusion` / `AVMysticism` roster key, same dormant-until-wired subsystem.
- `/audit-character` owns `CharacterRulesProfile::SKYRIM`; **this finding is the parser half only.**

## Suggested Fix

Add `starting_magicka` / `starting_stamina: Option<f32>` to `RaceRecord`, reading `DATA f32 @ 40` / `@ 44` with the same finite/positive guard `starting_health` uses. Extend `derive_skyrim_actor_values` to emit all three `(AVIF, base + offset)` pairs through `actor_value_form_id("Magicka")` / `("Stamina")`.

Guard with a fixture `RACE` carrying a **distinct** triple so a future edit cannot re-collapse them onto Health.

## Completeness Checks
- [ ] **SIBLING**: FO4/FO76/Starfield `RACE.DATA` — #2455 named them too; confirm whether their resource fields are read or still fall through
- [ ] **CANONICAL-BOUNDARY**: the base+offset composition stays in `actor_value_derive.rs`, not re-derived per consumer
- [ ] **TESTS**: a fixture `RACE` with a distinct `(health, magicka, stamina)` triple asserts three `ActorValues` entries, so the three cannot silently re-collapse onto one
