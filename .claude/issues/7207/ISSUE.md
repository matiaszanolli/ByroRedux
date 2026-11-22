# #7207 CHAR-2026-11-21-L-10: the bow's Fatigue term is clamped to at most 1 and measured against the fortified maximum, contradicting the capture

**Labels**: low,character,combat,bug,game:oblivion

From `docs/audits/AUDIT_CHARACTER_2026-11-21.md` (`/audit-character`, HEAD `a06d5f9db`).

- **Reported as**: CHAR-D2-07
- **Severity**: LOW / **Dimension**: Derived Formulas / **Game**: oblivion / **Location**: `crates/core/src/character/bow.rs:218` (`traits.fatigue_fraction.clamp(0.0, 1.0)`), `byroredux/src/bow.rs:286-295` (`current() / undamaged()`) / **Status**: NEW
- **Source**: `docs/engine/charal-oblivion-ruleset.md:642-646` ("the `(Fatigue/MaxFatigue+1)/2` term is explicitly *not* clamped to 1, so Fortify Fatigue effects pushing current above max keep paying off past the nominal ceiling", citing UESP *Oblivion:Fatigue* "if your current Fatigue is higher than your normal maximum ... you'll do more damage"); CS wiki *Fatigue Game Settings* (`FatigueFactor` has no clamp). The offline UESP dump (2022) does not carry the "higher than your normal maximum" sentence, so the capture line is the only citation.
- **Description**: `shot_damage` clamps the Fatigue fraction to `[0, 1]`, and the consumer divides current Fatigue by `undamaged()`, which includes the temporary and permanent modifiers (Fortify Fatigue raises numerator and denominator together), so the ratio can never exceed 1 even without the clamp. The documented edge (more damage above the normal maximum) is unreachable; the lower clamp at 0 (a collapsed actor, Fatigue below 0) is also undocumented.
- **Evidence**: `bow.rs` `(traits.fatigue_fraction.clamp(0.0, 1.0) + 1.0) / 2.0`; `actor_values.rs:88` `undamaged()` "adds every layer but damage".
- **Impact**: a Fortify-Fatigue archer does up to 25% less damage than documented (ratio 1.5 -> factor 1.25 versus 1.0); small and rare.
- **Related**: CHAR-D2-06.
- **Suggested Fix**: read the denominator from the unfortified maximum (base layer sum of the four attributes) and drop the upper clamp; keep or document the lower one.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other CHARAL rule rows / overlays / profiles)
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
- [ ] **TESTS**: A regression test pins this specific fix
