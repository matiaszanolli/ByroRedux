# #6215: SCR-2026-10-21-01: `GetValueInt()` and `x as Int` keep the untruncated float in fragment guards and values; #5974 fixed only `/` and `SetValueInt`

**Labels**: medium,scripting,bug

**Source**: docs/audits/AUDIT_SCRIPTING_2026-10-21.md · **Severity**: MEDIUM · **Dimension**: Recognizer Chain · **Location**: `crates/scripting/src/translate/effects/guards.rs:77-81` (`classify_value` strips every `Cast`), `:357-360` (`guard_reading` strips casts), `:407-412` (`GetValue` and `GetValueInt` → the same `GuardReading::Global`), `:21` (`INT_READS` lists `GetValueInt` as provably integral); evaluated raw at `crates/scripting/src/fragment/effects/evaluate.rs:68-72` and `:263-269`; applied raw at `fragment/effects.rs:162-167`

**Audit status**: NEW (residual of closed #5974)

**Untrusted-Input**: No

## Description

Papyrus `GlobalVariable.GetValueInt()` and `<float> as Int` truncate. The lowering removes the cast and maps `GetValueInt` to the same `ValueExpr::Global` / `FragmentGuard::Global` as `GetValue`. Both evaluate `Globals::get`, which is the stored float. #5974's own doc on `provably_int_valued` says "classify_value strips casts, so the truncation a `Float as Int` performs would be lost". That doc uses the fact only to refuse `SetValueInt` of a cast. It still accepts `SetValueInt(G.GetValueInt())` as exact, and it accepts every guard comparison.

## Evidence

`GameHour.GetValueInt() == 12` at 12.5 lowers to `12.5 == 12`, which is false; Papyrus gives true. `(GameDaysPassed.GetValue() as Int) >= N` is off by the fraction at the boundary. `Counter.SetValueInt(Clock.GetValueInt())` stores 3.7 into the target global; Papyrus stores 3. Integral-typed (`s`/`l`) globals are unaffected. Float globals such as GameHour, GameDaysPassed and mod timers are affected.

## Impact

Stage-fragment `If` branches and set-values on float globals decide differently from the original game, with no decline.

## Related

#5974, SCR-2026-10-21-02 (the same loss on the VM side).

## Suggested Fix

In `classify_value` and `guard_reading`, decline a `Cast { Int }` whose operand is not `provably_int_valued`. Decline `GetValueInt` unless the global is integral-typed. A `ValueExpr::Trunc` would also work: it would let both lower exactly.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other cast-stripping sites in translate/effects; every INT_READS row (GetStage, GetItemCount, GetLevel) used as a guard/value)
- [ ] **TESTS**: A regression test pins this specific fix
