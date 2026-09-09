===== 3483 =====
OPEN | bug low game:oblivion character 
# CHAR-2026-08-27b-D4-01: flat Fatigue regen is gated behind the `CharacterRuleset` lookup that only Magicka needs

**Severity**: LOW
**Dimension**: Pools, Afflictions & Reputation
**Game**: Oblivion (the only game whose regen config exists)
**Location**: `crates/core/src/character/regen.rs:174-200` (`pool_regen_tick_system`)
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` (CHAR-2026-08-27b-D4-01), HEAD `969d81c8`

## Description

Latent — the system is inert today (`PoolRegenConfig` has no production insertion site).

`pool_regen_tick_system` acquires `CharacterRuleset` with a `let … else { return; }` **before** the actor loop, but the ruleset is used only inside the Magicka branch (to look up the max-Magicka row and check its `DerivedScope`). Fatigue's regen is a flat constant that needs no ruleset at all — `regen.rs:70-73` documents `FATIGUE_REGEN_PER_SEC = 10.0` as *"vanilla Oblivion's Endurance coefficient (`fFatigueReturnMult`) is `0.0`, so this is the whole formula"* (`charal-oblivion-ruleset.md:386-388`), and it reads no ruleset row.

Yet a load with a `PoolRegenConfig` and no `CharacterRuleset` silently regenerates **neither** pool.

## Evidence

```rust
let Some(ruleset) = world.try_resource::<CharacterRuleset>() else {
    return;                       // ← Fatigue never reached
};
let Some(mut avs_q) = world.query_mut::<ActorValues>() else { return; };
for (_entity, avs) in avs_q.iter_mut() {
    if avs.get(config.fatigue_avif).is_some() {
        avs.restore(config.fatigue_avif, FATIGUE_REGEN_PER_SEC * elapsed);
    }
```

The three prior "silent gate" fixes in this file (#2950's two-resource gate, #2932's scope check, #2153's guard scope) all addressed the *documented* preconditions; this fourth gate is undocumented — the system's own docstring enumerates the gates as `PoolRegenConfig` and `PoolRegenAccumulator` and does not mention `CharacterRuleset`.

## Impact

None today. When Oblivion wiring lands, a load order that resolves the regen AVIFs but not a ruleset loses Fatigue regen with no log line — the same "indistinguishable from *no game loaded*" failure mode #2950 was filed for.

## Related

- #2950, #2932 (the earlier silent-gate fixes in this same system)
- #3444 — the concurrently-filed #2153 guard-drop finding at `regen.rs:153-180`. **This is a different defect at an adjacent line** (an over-broad early-return, not a lock hold) and does not overlap; the two fixes touch the same function and should be sequenced.
- #3441 — the `ActorValues ↔ CharacterRuleset` lock-order cycle, which involves the same `CharacterRuleset` acquire; narrowing this acquire's scope interacts with that fix.

## Suggested Fix

Move the `CharacterRuleset` acquire inside the Magicka branch (or make it a `try_resource` whose `None` only disables the scoped-max lookup, falling back to `base_max` as the branch already does for player-only rows), and add `CharacterRuleset` to the docstring's gate list either way.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other `try_resource`-gated character systems — `affliction_tick_system`, and the CHARAL consumers in `crates/scripting/src/condition.rs`)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved — narrowing the `CharacterRuleset` hold interacts with #3441 and #3444
- [ ] **TESTS**: A regression test pins this specific fix (a world with `PoolRegenConfig` + `PoolRegenAccumulator` but no `CharacterRuleset` still regenerates Fatigue)

===== 3487 =====
OPEN | bug medium ai scripting quests game:fo4 game:starfield 
# SCR-D5-2026-08-27-01: three effect primitives guard on hand-authored .psc arity, but the .pex frontend materializes every default argument — MoveTo declines 100% of 3,334 real calls

- **Severity**: MEDIUM
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: No
- **Location**: `crates/scripting/src/translate/effects.rs:790-801` (`prim_move_to`); `crates/scripting/src/translate/effects.rs:1072-1080` (`prim_evaluate_package`); `crates/scripting/src/translate/effects.rs:889-893` (`prim_player_controls`)
- **Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`

## Description

The Papyrus compiler emits **every** parameter of a call into the compiled `.pex`, including ones the author omitted and left at their declared default. Quest fragments only ever reach the effect table through the `.pex` frontend (`populate_quest_fragments_from_pex` → `decompile_script` → `lower_fragment_with_quest_properties`) — the `.psc` route is a test-only path. Several primitives, however, were written against the *authored* call shape, and reject the default-materialized one:

| Primitive | Accepted arity | Arities actually observed in the corpus | Declined |
|---|---|---|---|
| `prim_move_to` | exactly 1 | 5 (Skyrim, ×1742) and 6 (FO4/Starfield, ×1592) | **3,334 / 3,334 = 100%** |
| `prim_evaluate_package` | exactly 0 | 0 (×853) and 1 (FO4/Starfield `abResetAI`, ×2628) | **2,628 / 3,481 = 75%** |
| `prim_player_controls` | `<= 9` | 9 (×118) and 11 (FO4/Starfield, ×61) | 61 / 179 = 34% |

`MoveTo` is the severe case: it has an `Effect::MoveTo` variant, a dispatch arm in `fragment.rs` that resolves both receiver and destination through the alias-aware `resolve_object`, and its own regression tests — and it can **never** fire on production input. `prim_move_to`'s comment justifies the narrowness as refusing to "silently drop [an offset] and misplace the object", which is sound reasoning applied to the wrong input shape: the offsets it is refusing are, overwhelmingly, the compiler's own zeros.

## Evidence

`prim_move_to`, verbatim:

```rust
// effects.rs:790-801
fn prim_move_to(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "MoveTo")?;
    // The conservative 2-arg shape only (receiver + destination) — a 3rd+
    // argument (offsets / match-rotation) declines rather than silently
    // dropping it and misplacing the object.
    if args.len() != 1 {
        return None;
    }
    ...
}
```

Corpus probe over every `Fragment_*` body in `Skyrim - Misc.bsa` + `Fallout4 - Misc.ba2` + `Starfield - Misc.ba2` (26,641 `.pex`, 43,818 behavioral fragments), tallying the literal shape of `MoveTo`'s trailing arguments:

```
moveto  args=5  count=1742
moveto  args=6  count=1592
moveto-tail[f0,f0,f0,btrue]         args=5  count=1668
moveto-tail[f0,f0,f0,btrue,bfalse]  args=6  count=1585
...(43 further distinct tails, 81 calls total, carrying real offsets)
evalpkg-arg[bfalse]  args=1  count=2621
evalpkg-arg[btrue]   args=1  count=7
```

**3,253 of 3,334 `MoveTo` calls (97.6%) carry exactly `(0.0, 0.0, 0.0, matchRotation)` — precisely the "receiver + destination" semantics `Effect::MoveTo { moved, destination }` already models.** Only ~81 calls (2.4%) carry a real offset where the current decline is genuinely protective. Likewise 2,621 of 2,628 one-arg `EvaluatePackage` calls pass the literal `false` default.

## Impact

A shipped, tested, dispatch-wired, alias-aware effect (`MoveTo`) contributes nothing on any real game's content and cannot be observed to be broken by any existing gate — `fragment_coverage` reports a zero for it that reads identically to "authors don't use this". Every fragment containing a `MoveTo` call is guaranteed to decline in full, so this is also a hard ceiling on the whole-fragment claim rate (42.6% Skyrim / 34.9% FO4+SF), not just on one effect. `EvaluatePackage` is game-asymmetric: it works on Skyrim and silently declines on FO4/Starfield, which is exactly the kind of per-game divergence the domain's abstraction rules exist to prevent. This is a *decline*, so nothing is mis-lowered — hence MEDIUM, not HIGH.

## Related

`docs/engine/m47-3-quest-alias-design.md` Phase 2's unchecked "re-measure `AddItem`/`MoveTo` yield" item — this finding is that measurement's result. Same family as #3159 (`Lock`/`Unlock` absent): the effect table is growing faster than anything checks it against real input. See also SCR-D5-2026-08-27-04 (the missing decline-reason tally that concealed this).

## Suggested Fix

Accept the default-materialized tail where its literal value is the documented Papyrus default, and keep declining otherwise — i.e. for `MoveTo`, accept 5/6 args when args 1–3 are numeric-literal `0` and the rotation flags are literals, decline on a non-zero or non-literal offset; for `EvaluatePackage`, accept a literal `abResetAI`; for `prim_player_controls`, widen the bound to the FO4/Starfield parameter count. Then add a corpus-arity assertion to `fragment_coverage` (or a sibling instrument) so the next primitive written against a `.psc` arity fails a gate instead of silently measuring zero.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (all 31 `prim_*` bodies re-checked against real corpus arity, not just these three)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix

===== 3496 =====
OPEN | bug low scripting quests 
# SCR-D5-2026-08-27-03: prim_set_stage and the three objective primitives are the only four of 31 effect primitives with no upper argument-count guard — an over-arity call silently lowers

- **Severity**: LOW
- **Dimension**: Recognizer-Chain Soundness (Dimension 5)
- **Untrusted-Input**: Yes (a modded `.pex` reaches this code)
- **Location**: `crates/scripting/src/translate/effects.rs:588-595` (`prim_set_stage`); `:699-709` (`prim_set_objective_displayed`); `:711-720` (`prim_set_objective_completed`); `:722-731` (`prim_set_objective_failed`)
- **Source**: `docs/audits/AUDIT_SCRIPTING_2026-08-27.md`

## Description

Every other primitive in `EFFECT_PRIMITIVES` bounds its argument count — `prim_add_item` (`args.len() > 3 → None`), `prim_activate` (`> 2`), `prim_disable` (`> 1`), `prim_set_open`, `prim_start_scene` (`!args.is_empty()`), and so on — and #2289 added a decline-path test for each. These four read only positional args 0 and 1 and ignore any further argument silently.

`prim_set_stage` is the highest-traffic effect in the domain (20,322 real calls, all one-argument) and the one whose false-positive lowering has the largest blast radius: a fragment shaped `SomeQuest.SetStage(10, <unmodeled term>)` lowers to a plain `SetStage { stage: 10 }` rather than declining.

## Evidence

```rust
// effects.rs:588-595 — no args.len() bound anywhere in the body
fn prim_set_stage(e: &Expr, scope: &Scope) -> Option<Effect> {
    let (object, args) = method_call(e, "SetStage")?;
    let stage = u16::try_from(int_arg(args, 0)?).ok()?;
    Some(Effect::SetStage {
        quest: receiver_quest(object, scope)?,
        stage,
    })
}
```

The three objective primitives have the same shape (`int_arg(args, 0)` + `bool_arg(args, 1)?.unwrap_or(true)`, no bound). Mechanical sweep of all 31 `fn prim_*` bodies for an `args.len()` / `args.is_empty()` guard: 27 guarded (directly or via a guarded delegate), 4 unguarded — the four above.

## Impact

Not reachable from vanilla content (the compiler emits exactly the declared arity, and all four functions' Papyrus signatures are within the read range), so no shipped game is affected — hence LOW. It is a real hole in the decline discipline for modded/hand-authored input, and an inconsistency a reader of the other 27 primitives would not expect.

## Related

#2289 (CLOSED — added decline tests for 14 primitives, but arg-count declines for these four were not among them); #2540 (CLOSED — added negative-index and i32-overflow declines for the three objective primitives, but not an over-arity decline).

## Suggested Fix

Add `if args.len() > N { return None; }` to each (N = 1 for `SetStage`, 3 for `SetObjectiveDisplayed`, 2 for the other two, matching their real Papyrus signatures), plus one decline test each in the block #2289 already established.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (re-sweep all 31 `prim_*` bodies after the fix so the guarded count is 31/31)
- [ ] **TESTS**: A regression test pins this specific fix

===== 3504 =====
OPEN | bug medium tech-debt 
# REG-2026-08-27-02: regression of #3218 — the traceability fix shipped an advisory tool, the CI gate is still PR-only, and the citation gap is unchanged (31%)

## Description

**Regression of #3218** (CLOSED 2026-08-26, `medium`/`bug`/`tech-debt`) — the fix shipped an advisory tool; the gap it measured is unchanged.

#3218 diagnosed the mechanism precisely — the CI gate is `if: github.event_name == 'pull_request'`, and *"this repo's history is overwhelmingly direct commits to main, so for the dominant workflow it never fires."* The fix added a `--window` mode to `scripts/check-issue-traceability.sh` and a call to it in the `session-close` ritual. **The gate's trigger condition was not changed**, and the `--window` mode is a report, not an enforcement — nothing fails, nothing blocks, and nothing back-fills the citation.

The measured gap has therefore not moved. #3218 was filed against *43 of 134 (32%)* uncited in the 2026-08-16..20 window. Measured 2026-08-27 over the 2026-08-18..28 window: **123 of 400 (31%)**. The rate on 2026-08-26 — the day #3218 itself closed — was **36 of 76 (47%)**.

## Location

`.github/workflows/ci.yml:13-25` · `scripts/check-issue-traceability.sh:34-52` · `.claude/commands/session-close/SKILL.md:80-88`

## Evidence

```yaml
# .github/workflows/ci.yml:13-17 — unchanged trigger
jobs:
  issue-traceability:
    name: Issue/commit traceability
    if: github.event_name == 'pull_request'
    runs-on: ubuntu-latest
```

The workflow itself does fire `on: push: branches: [main]`, so the runner is present — it is the job-level `if:` that excludes every direct-to-main commit.

```
per-day uncited (closing-keyword commit on main, live gh state, measured 2026-08-27):
  2026-08-18: 24/58 (41%)     2026-08-24:  0/3  ( 0%)
  2026-08-19:  2/37 ( 5%)     2026-08-25:  6/19 (32%)
  2026-08-20:  2/11 (18%)     2026-08-26: 36/76 (47%)  <- #3218 closed
  2026-08-21:  2/61 ( 3%)     2026-08-27:  5/47 (11%)
  2026-08-22: 44/64 (69%)     2026-08-28:  2/17 (12%)
  2026-08-23:  0/7  ( 0%)
```

```
# commits on main since 2026-08-20 touching *.rs
246 total — 122 (50%) carry no closing keyword
```

The reverse direction — *commit → issue* — is not checked at all: the script's `closing_issue_numbers` reads a PR body, and `--window` iterates closed issues. A commit that fixes something with no issue attached is invisible to both modes.

## Impact

This is the mechanism that produced the eight-unguarded-walker regression filed alongside this issue. #3237's partial fix was buried in a mega-commit body under a `refactor(...)` heading with no closing keyword; the issue was closed by hand; no gate compared the fix's reach to the issue's stated scope.

As #3218's own script comment says, the degradation is self-concealing: *"a regression audit that cannot find fixes gets quieter, not louder."* At a 31% uncited rate, `/audit-regression`'s Step 2 (`git log --grep="#<N>"`) is a coin flip, and every future sweep pays the cost of re-deriving fix presence by hand.

## Related

- #3218 — the partially-applied fix (CLOSED)
- REG-2026-08-27-01 (filed from the same report) — the concrete regression this gap concealed

## Suggested Fix

Change the gate to run on `push` to `main` over the pushed range (`github.event.before..github.event.after`) rather than only on `pull_request`, so the dominant workflow is actually covered. Separately, add the missing direction — flag pushed commits that touch `*.rs` and cite no issue at all — as a warning-level annotation, so the fix→issue link is recorded while the context is fresh rather than reconstructed by an auditor weeks later.

## Source

`docs/audits/AUDIT_REGRESSION_2026-08-27.md` — REG-2026-08-27-02

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the other CI jobs that are PR-gated but describe main-branch invariants
- [ ] **TESTS**: A regression test pins this specific fix (extend the script's `--self-test` to cover the push-range mode)

