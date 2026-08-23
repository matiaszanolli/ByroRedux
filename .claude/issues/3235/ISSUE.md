# 3235: NIFAL-D9: translation_completeness metalness/roughness floors are stale against #2707

**Severity**: MEDIUM · **Dimension**: NIFAL Completeness/cross-cutting · **Report**: `docs/audits/AUDIT_NIFAL_2026-08-23.md` (NIFAL-D9-2026-08-23-01)

## Description

The cross-game completeness harness's `metalness_override`/`roughness_override` `>= 99.9%` floors (`crates/nif/tests/translation_completeness.rs`, 7 per-game assertions) were added 2026-08-06 (`66f0775e`, #2304-#2307) on the premise, stated in the test's own comment block: *"the sole production `ImportedMaterial` constructor sets both unconditionally to `Some(..)`... so they are exactly 100% by construction on every game."*

That premise was invalidated a week later by `593ab134` (#2705-#2708/#2707): the constructor now correctly gates both fields on `has_no_pbr_classifier_signal()` (`crates/nif/src/import/material/mod.rs:1415-1424`) — a deliberate no-fabrication improvement that stops inventing a metalness/roughness guess for content with no PBR-relevant signal at all, deferring instead to `Material::resolve_pbr`'s NaN-sentinel backstop.

The 2026-08-06 test floors were never updated to match, and the comment block still asserts the pre-#2707 premise as current fact.

## Evidence

Live run output (`cargo test -p byroredux-nif --test translation_completeness -- --ignored --nocapture`), all 7 game data directories present:

```
game         imported   metO%   rghO%
Oblivion     567        100.0%  100.0%
FO3          687         94.3%   94.3%
FNV          806         96.7%   96.7%
SkyrimSE     515         93.8%   93.8%
FO4          644         99.4%   99.4%
FO76         697         18.2%   18.2%
Starfield    811          5.1%    5.1%

thread 'cross_game_translation_completeness' panicked at
crates/nif/tests/translation_completeness.rs:483:17:
[FO3] metalness_override fill < 100% (got 94.3%)
```

The panic aborts the test before later games' closures run, but their printed `metO`/`rghO` values (all < 99.9%) show every one of them would fail the same assertion in turn. Only Oblivion passes.

## Impact

The harness is this layer's only automated, output-based regression signal (see #2532 for its narrower Material-only sibling) and is currently unusable as a gate for 6 of 7 games, for a reason unrelated to any translation regression. Two risks: (1) it's opt-in/`--ignored`, so this can go unnoticed for a long time and produce a false "translation broke" alarm when someone does run it; (2) worse, a well-intentioned "fix" that reverts `metalness_override`/`roughness_override` to unconditional `Some(..)` to make the assertion pass again would silently **undo the #2707 no-fabrication correctness fix**.

## Suggested Fix

Either (a) lower the `metO`/`rghO` floors per-game to the measured post-#2707 values with the same ~10-15pp margin convention already used for every other metric in this file, or (b) replace the flat floor with an explicit assertion tied to `has_no_pbr_classifier_signal`'s actual definition. Either way, correct the "100% by construction" comment block to cite #2707.

## Completeness Checks
- [ ] **TESTS**: New floors (or the relational assertion) actually run green against current data for all 7 games before merge
