# #3768 — CHAR-2026-08-30-D6-01: charal.md declares Oblivion "CHARAL-complete end-to-end" for a ruleset that has no construction site and could not resolve a single AVIF if it did

**Repo**: matiaszanolli/ByroRedux · **Filed**: 2026-08-30 · **HEAD**: `64f64480`
**Labels**: low, character, doc-rot, documentation, game:oblivion

---

**Audit**: `/audit-character` — `docs/audits/AUDIT_CHARACTER_2026-08-30.md` (Dimension 6 — Coverage, Documentation & Doctrine Drift), HEAD `64f64480`
**Finding ID**: `CHAR-2026-08-30-D6-01`

- **Severity**: LOW
- **Status**: NEW

## Location

- `docs/engine/charal.md:343`
- echoed at `docs/engine/charal-oblivion-ruleset.md:7`

## Description

§5 ends its Oblivion paragraph:

> *"**Oblivion is now CHARAL-complete** end-to-end."*

Two independent facts contradict it:

1. `CharacterRulesProfile::OBLIVION` carries `ruleset: RulesetBuilder::None`, so `build_ruleset` returns `None` and no Oblivion `CharacterRuleset` is ever constructed at load.
2. More fundamentally, `Oblivion.esm` authors **no `AVIF` records at all** — Oblivion predates the record type — so every one of `oblivion_ruleset`'s eight resolve-or-skip rows would skip and both rosters would resolve empty even if arm (1) were added.

## Evidence

- `crates/core/src/character/profile.rs:82-87` — `OBLIVION` → `RulesetBuilder::None`
- `crates/plugin/tests/parse_real_esm.rs:286-295` — the Oblivion `RosterCase` carries `authors_actor_values: false`, and `assert_rosters_resolve` asserts `index.actor_values.is_empty()` with the comment *"if it now does, its rosters became falsifiable and this case should assert them"*
- `docs/feature-matrix.md:250` gets it right ("~ built, unwired"), so the design doc is the outlier, not the matrix

Re-verified at HEAD: `grep -n 'CHARAL-complete' docs/engine/charal.md docs/engine/charal-oblivion-ruleset.md` returns both sites.

## Impact

"end-to-end" is the phrase a future contributor greps for when deciding what is left to do on Oblivion. It hides the *actual* blocker — a legacy actor-value index resolver for a pre-`AVIF` game — behind a completion claim, and the child capture repeats it, so cross-checking the two documents confirms rather than corrects it.

This is `feedback_audit_findings`' stale-premise class at the doc layer: a future audit that trusts §5 would mark Oblivion done and stop looking.

## Related

- #3170 (Skyrim's parallel unwired-ruleset issue)

## Suggested Fix

Reword to: "the Oblivion **ruleset builder** is complete; it is unwired (`RulesetBuilder::None`) and additionally blocked on a pre-`AVIF` legacy actor-value resolver, since `Oblivion.esm` authors no `AVIF` group." Correct the echo in `charal-oblivion-ruleset.md:7` the same way.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the Skyrim paragraph in the same §5, which has the same `RulesetBuilder::None` shape)
- [ ] **TESTS**: N/A (documentation) — but confirm `parse_real_esm.rs`'s Oblivion `RosterCase` comment still states the falsifiability condition
