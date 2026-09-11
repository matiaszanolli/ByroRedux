# CHAR-2026-09-11-D3-03: `tes.rs`'s module docstring still calls the Oblivion per-level Health accrual "the deferred TES leveling-efficiency mechanic" — the same file implements it 200 lines below

GitHub: https://github.com/matiaszanolli/ByroRedux/issues/4101
Filed: 2026-09-11 from `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`)

> Immutable snapshot of the issue as filed (TD10-001/#1156). GitHub is
> authoritative for current state: `gh issue view 4101 --json state`.

---

Reported by `/audit-character` — see `docs/audits/AUDIT_CHARACTER_2026-09-11.md` (HEAD `8151cded`).

- **Severity**: LOW
- **Dimension**: Leveling & Progression Models
- **Game**: oblivion
- **Location**: `crates/core/src/character/tes.rs:16-18` (module doc) vs `crates/core/src/character/tes.rs:200-220` (`oblivion_health_gain_per_level`) and `docs/engine/charal.md:336-341`
- **Source**: `docs/engine/charal-oblivion-ruleset.md:455-459` — the per-level accrual `0.1×Endurance` is captured and LOCKED, and `docs/engine/charal.md:336-341` (edited in this delta) states "The level-up leveling-efficiency mechanics are shipped too: `oblivion_attribute_bonus(governed_skill_ups)` → +1/+2/+3/+4/+5 … and `oblivion_health_gain_per_level(endurance)` = 10 % of Endurance accrued (and stored) each level".

## Description

This is the exact defect class of #2946 ("`leveling.rs` docstring calls `SkillXp` 'a future third variant' and the Oblivion attribute bonus 'deferred' — both shipped"), which is CLOSED. Its fix rewrote `leveling.rs`'s module docstring — which now correctly says the level-up attribute bonuses "are implemented by the TES leveling helpers" — but did not touch `tes.rs`'s, which still reads: "Health's per-level accrual (≈10 % of Endurance each level) is **not** part of the base formula — it is **the deferred TES leveling-efficiency mechanic** (`docs/engine/charal.md` §5), a leveling concern, not a derived pool." The mechanic is not deferred: `oblivion_health_gain_per_level` is a `#[must_use] pub fn` in the same file at `:218`, exported from `mod.rs`, with two tests including the UESP anchor. The file contradicts itself — `:200-201` calls the same thing "the stateful half of the Health pool (leveling-efficiency mechanic, `charal.md` §5)". The pointed-at `charal.md` §5 was updated in *this delta* to say the opposite of what the docstring claims it says.

## Evidence

`tes.rs:17` — "it is the deferred TES leveling-efficiency mechanic"; `tes.rs:218-220` — `pub fn oblivion_health_gain_per_level(endurance: u16) -> f32 { 0.1 * f32::from(endurance) }`; `tes.rs:384-389` — `health_gain_per_level_is_ten_percent_of_endurance` asserts the `Endurance 100 → 10.0` UESP anchor and passes (verified in this session's run). `charal.md:340-341` — "The level-up leveling-efficiency mechanics are shipped too".

## Impact

Documentation only, but of the kind that causes duplicated logic: the module docstring is the first thing a contributor implementing Oblivion leveling reads, and it tells them the per-level Health accrual is *not yet built*, 200 lines above the function that builds it. That is precisely the re-implementation the project's standing "improve existing code rather than duplicating logic" instruction guards against, and it is the reason #2946 was filed in the first place.

## Related

#2946 (CLOSED — same class; its fix reached `leveling.rs` but not `tes.rs`); #3848

## Suggested Fix

Replace "the deferred TES leveling-efficiency mechanic" in `tes.rs:17` with a pointer to the sibling that implements it — "the TES leveling-efficiency mechanic, implemented by [`oblivion_health_gain_per_level`] below" — keeping the true half of the sentence ("a leveling concern, not a derived pool") intact.

## Completeness Checks

- [ ] **SIBLING**: every other copy of this fact swept repo-wide (this class has repeatedly had one copy fixed and another missed — grep the symbol/number across `docs/`, `crates/`, `.claude/`, not just the named file)
- [ ] **TESTS**: where the doc states a pinned number or symbol, a test or `_audit-validate.sh` rule keeps it honest