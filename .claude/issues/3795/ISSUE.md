# #3795: LC-2026-08-30-D6-01: per-game-translation-survey.md §5 'Pattern A' prescribes migrating onto seven NifVariant helpers that were deliberately deleted as an 'architectural foot-gun' — the doc's highest-leverage starter is the exact inverse of enforced doctrine

**Labels**: documentation, nif-parser, medium, legacy-compat, doc-rot
**Filed**: 2026-08-30 · HEAD `64f64480`

---

**Source**: `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-30.md` — LC-2026-08-30-D6-01 (MEDIUM)
**Dimension**: 6 — per-game translation leak patterns
**Location**: `docs/engine/per-game-translation-survey.md:266-278` (§5 "Pattern A"), cascading into §4.1, §8 task 5, §9; and `.claude/commands/audit-legacy-compat/SKILL.md` Dimension 6

## Description

`per-game-translation-survey.md` §5 "Pattern A: hardcoded BSVER constants where a helper exists" states:

> *"`NifVariant` exposes `has_effects_list`, `has_properties_list`, `has_material_crc`, `has_shader_alpha_refs`, `uses_bs_tri_shape`, `uses_fo4_shader_flags`, `uses_fo76_shader_flags` — and the parser calls `stream.bsver() < 130` or `stream.bsver() > 34` directly instead. Fix is mechanical: every raw `bsver()` comparison gets rewritten to call the named helper … **Highest-leverage starter** … a clippy lint or custom test can enforce 'no raw `bsver()` comparison outside `version.rs`'."*

Against HEAD (`64f64480`), **every clause of that is false or inverted.**

## Evidence

**1. Those helpers do not exist.** `uses_fo4_shader_flags`, `uses_fo76_shader_flags` and `has_dynamic_effect_fields` (cited in §4.1) have **zero** occurrences anywhere in the tree. The other four survive only as *prose in comments explaining why the call site does **not** use them*:

```
crates/nif/src/blocks/node.rs:107   "Use raw bsver rather than `variant().has_effects_list()` so …"
crates/nif/src/blocks/base.rs:101   "bsver() directly rather than `variant().has_properties_list()`"
crates/nif/src/blocks/tri_shape_nigeometry_data_version_tests.rs:344
crates/nif/src/blocks/collision/shape_compound_tests.rs:29
```

**2. They were removed on purpose, and the reasoning is recorded in the very file the survey points at.** `crates/nif/src/version.rs:699-718`:

> *"#938 … removed three predicates; #1511 removed six more; #1840 removed seven more (`has_material_crc`, `has_properties_list`, `avobject_flags_u32`, `has_shader_alpha_refs`, `has_effects_list`, `uses_bs_tri_shape`, `has_culling_mode`); #1897 removed the last survivor … Keeping a call-site-less predicate as an 'approved helper' alongside the raw-bsver path was an **architectural foot-gun**: a contributor adopting one … reintroduces the one-bsver-step transitional-export mis-parse those call sites were fixed to avoid. **No feature-flag predicates remain on `NifVariant` — this doctrine is fully enforced now.**"*

**3. The bare-comparison problem it describes is already solved** — by the *opposite* move: named **constants** (`version::bsver::*`), not named **predicates**.

**4. The false premise cascades into three more sections.**

- **§4.1's 14-row table** is headed *"Hardcoded threshold constants scattered across 30+ sites"* with a *"Helper available? Yes — bypassed"* column. Every cited site now reads a named constant; the column names APIs that no longer exist.
- **§4.1's closing paragraph** calls `BSLightingShaderProperty::parse` *"the textbook candidate for splitting into `BsLightingShaderVariant::{Skyrim, Fo4, Fo76Plus}`"*. **That split landed**: `shader.rs:903 parse_skyrim`, `:1009 parse_fo4`, `:1159 parse_fo76_plus`, plus `parse_shader_type_data_fo4` / `_fo76`.
- **§8 task 5** ("Migrate raw `bsver()` comparisons to `NifVariant` helpers … add a clippy lint to prevent regression") and **§9's progress row** for it (*"landed `2bd447d5` — 6 sites migrated, 3 new helpers added"*) are a record of work that #1840 / #1897 then deliberately **reverted**, with no note. §9 also still records task 2 as *"deferred to a dedicated session"* — it shipped.

## Impact — why this is not merely doc rot

This audit's own skill file (`.claude/commands/audit-legacy-compat/SKILL.md`, Dimension 6) instructs the auditor to *"audit by the survey's three leak patterns: Pattern A — hardcoded BSVER constants where a named helper already exists but call sites bypass it (`per-game-translation-survey.md` §5 Pattern A)"*.

An auditor following that literally searches for a class of leak that was **designed out** — and, worse, the survey's stated *"highest-leverage starter"* is an instruction to **reintroduce an abstraction the tree records as a foot-gun that causes a mis-parse**. That is an actionable wrong instruction sitting in the engine's own design docs, not a stale number. It re-seeds the same misdirection on every sweep.

Severity MEDIUM: `_audit-severity.md`'s LOW bucket is dead code / missing docs / naming / test-coverage gaps; an architectural prescription that is the documented inverse of the enforced doctrine is not in it, and the decision tree's terminal rule is "Otherwise → MEDIUM". No runtime behaviour is wrong today, which is why it is not HIGH.

**Confidence: CERTAIN.** Every claim above is a grep or a verbatim quote from HEAD; the removal rationale is in the codebase, signed by four issue numbers.

## Suggested Fix

1. **Rewrite §5 Pattern A** to state the doctrine the tree actually enforces — *raw `stream.bsver()` compared against a named `version::bsver::*` constant, with the nif.xml `vercond` quoted at the site; **no** `NifVariant` feature-flag predicates* — and cite `version.rs:699-718` for why.
2. **Strike the seven dead helper names** from §4.1's "Helper available?" column and re-title the table (the constants are named; the *thresholds* are what is scattered).
3. **Mark §9's task-5 row REVERTED** (#1840 / #1897) and its task-2 row LANDED. Update §4.1's closing paragraph to record that the `BSLightingShaderProperty` variant split shipped.
4. **Update the skill's Dimension 6 Pattern A bullet** to match, or it will keep re-seeding the same misdirection every sweep.

## Related

- **#3537** (LC-2026-08-27-D6-01) — §7 item 7 still restates, unmarked, the `classify_pbr_keyword`-collapses-everything claim that §2 retracts eight lines into itself. Confirmed unchanged (`survey.md:56-68` vs `:427-430`). **Batch the two fixes**: same document, same failure class.
- #938, #1511, #1840, #1897 — the four issues that removed the helpers

## Completeness Checks
- [ ] **SIBLING**: `.claude/commands/audit-legacy-compat/SKILL.md` Dimension 6 updated in the same pass, or the doc fix does not stick
- [ ] **SIBLING**: Batched with #3537 — same document, same class
- [ ] **TESTS**: If a guard is wanted, it is the inverse of the one §5 proposes — a test asserting `NifVariant` exposes **no** feature-flag predicates, matching the doctrine `version.rs:699-718` declares enforced
