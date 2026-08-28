# #3482 — CHAR-2026-08-27b-D2-01: six `stealth.rs` sub-coefficient groups are supported by no capture document, and three prior reports each credit a predecessor that never verified them

**Labels**: bug, medium, game:fnv, game:fo3, character, test-gap
**Filed from**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` via `/audit-publish`

---

**Severity**: MEDIUM
**Dimension**: Derived Formulas (CHARAL-adjacent siblings, CHAR-D6-05 / #2962)
**Game**: FNV, FO3
**Location**: `crates/core/src/stealth.rs:93-98` (`ActionSound::value`), `:114-118` (`ArmorClass::penalty`), `:211-238` (`detection_score`'s `Sound` and `Visual` terms)
**Source report**: `docs/audits/AUDIT_CHARACTER_2026-08-27b.md` (CHAR-2026-08-27b-D2-01), HEAD `969d81c8`

## Description

`crates/core/src/stealth.rs` carries roughly a dozen numeric sub-coefficients inside `detection_score`'s `Sound` and `Visual` terms. **None of them appears in any `charal-*-ruleset.md` capture document.**

`docs/engine/charal-fnv-fo3-ruleset.md:224-258` is the whole "Sneak Detection (FNV)" capture. It records the top-level `Detection`, `TargetSkill`, `DetectorSkill` and `Attenuation` expressions — all four verified PASS this pass — and then says only that *"`Sound` and `Visual` fold in movement/weapon-noise level, light level + night-eye, and armor class (heavy/medium/light)"*. It states no coefficient for any of them.

That alone makes them `UNSOURCED` under this audit's own rule. What makes it worth filing rather than tabulating is the **audit trail**: the three most recent CHARAL reports each record the file as verified, and each points at a predecessor that did not verify it. The chain terminates in nothing.

Not a re-file: `AUDIT_CHARACTER_2026-08-15.md` filed the *ownership* gap (CHAR-D6-05 / #2962, since closed by adding the siblings to `/audit-character`'s scope); no report has ever filed the *constants* gap.

## Evidence

Unsourced constant groups, all confirmed present at HEAD:

| Constant | Code | Capture-document value |
|---|---|---|
| LOS sound multiplier `1.6` / `0.16` | `stealth.rs:212` | **absent** |
| Movement sound `12.0 + weight/2` and `×1.5 / ×1.0 / ×0` | `stealth.rs:213-219` | **absent** |
| Action-sound values `0 / 10 / 50 / 100` and the `×2.0` weight | `stealth.rs:95-98,220` | **absent** |
| Visual `1.4 × min(light × nightEye, 100)`, night-eye `3.0` | `stealth.rs:225-231` | **absent** |
| Visual movement `0.21 / 0.01 / 0.0` | `stealth.rs:231-234` | **absent** |
| Armour-class penalty `0 / 10 / 20` | `stealth.rs:116-118` | **absent** (doc says only "armor class (heavy/medium/light)") |

The circular attribution chain, quoted from the reports themselves:

- `docs/audits/AUDIT_CHARACTER_2026-08-15.md:2256` — *"Dimension 2 verified 26 constants; none of them are these."* (`stealth.rs` is named two lines later as having "the same status" as `combat.rs`.)
- The 2026-08-16 report mentions `stealth.rs` only twice, both times as an *ownership* reference (`:201`, `:381`); it verifies nothing in it.
- `docs/audits/AUDIT_CHARACTER_2026-08-20.md:570-572` — *"`crates/core/src/stealth.rs` was re-read but not re-verified line-by-line against `charal-fnv-fo3-ruleset.md`'s 'Sneak Detection (FNV)' section — it is unchanged since the **2026-08-16 sweep verified it**"*.
- `docs/audits/AUDIT_CHARACTER_2026-08-24.md` § Verification honesty — *"`crates/core/src/stealth.rs` (unchanged since the **2026-08-16 sweep verified it**)"*.

The module is honest about its own limits (`stealth.rs:41-50`, "## No-guessing caveat" — it discloses that the source gives no worked numeric example and that the tests are structural / monotonicity-only). The gap is that the *capture* never recorded what the transcription was made from, so nothing in the repository can now falsify it.

## Impact

~12 gameplay constants that this audit exists to check are unverifiable from the repository. No live impact today — `detection_score` and `classify` have zero non-test callers (the `HitEvent.sneak_attack` hook is hardcoded `false`) — but the deferral is exactly the condition under which the trail rotted unnoticed for four sweeps, and the numbers will be believed the day an AI/perception system consumes them.

## Related

- #2962 (the ownership half, closed)
- `feedback_no_guessing`
- the analogous still-open UNSOURCED row: `SKYRIM_SKILL_USE_CURVE = 1.95`, recorded only at `docs/engine/charal.md:147` and absent from `charal-skyrim-ruleset.md`

## Suggested Fix

Extend `charal-fnv-fo3-ruleset.md`'s "Sneak Detection (FNV)" section with the `Sound` and `Visual` sub-expressions and the armour / action tables from the cited fandom *Sneak (Fallout: New Vegas)* page, so each coefficient has a line to be checked against. Then correct the "verified" attribution in that file's Known-Open register rather than in a future report. Non-structural monotonicity tests are not a substitute for a sourced capture.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the other CHARAL-adjacent sibling (`crates/core/src/combat.rs`) and in every `charal-*-ruleset.md` capture
- [ ] **TESTS**: A regression test pins this specific fix (per-coefficient assertions against the newly captured document values, replacing the structural-only coverage)
