# #3537 — LC-2026-08-27-D6-01: the translation survey's §7 still states, unmarked, the exact classify_pbr_keyword claim §2 now retracts

Labels: low, documentation, doc-rot, legacy-compat, nifal, game:fnv
Source: docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md (base 969d81c8)
Filed: 2026-08-27 via /audit-publish

---

**From:** `docs/audits/AUDIT_LEGACY_COMPAT_2026-08-27.md` (LC-2026-08-27-D6-01) · base `969d81c8`

- **Severity**: LOW
- **Dimension**: 6 — per-game translation survey currency
- **Location**: `docs/engine/per-game-translation-survey.md:427-430` (§7 item 7), contradicting `:53-64` (§2's correction note) in the same file
- **Note**: residual of LC-D6-2026-08-24-01, whose main body was fixed by #3281 / `a924244e`

## Description

The 2026-08-25 correction pass fixed §2 and §4.3 but left §7's numbered list untouched. Item 7 still reads:

> "7. **FNV `classify_pbr_keyword` collapses everything to matte 0.8 roughness** — already documented in `material-abstraction.md` Leak B. This single fact accounts for the 'Fallout looks like a different engine' perception more than any other."

§2 of the same document now says the opposite ~370 lines earlier — that this "specific bug was fixed by #1873 (commit `634873db`)" and that the classifier "now runs an evidence-cited keyword + `specular_authored` gate rather than a blanket matte default". §7 carries no correction marker of any kind.

## Evidence

Both passages quoted above are verbatim from HEAD (§2 at `:53-64`, §7 item 7 at `:427-430`). `crates/core/src/ecs/components/material.rs:663+` is the current classifier; `docs/engine/material-abstraction.md:10-13` carries its own corrective banner for the same fix, so §7's cross-reference to "Leak B" now points at a passage that is itself annotated as superseded.

## Impact

Documentation-only, but §7 is precisely the section the `/audit-legacy-compat` skill's Dimension 6 names by number — *"**Fallout is the stress case** (§7)"*. An auditor following that pointer reads the uncorrected version of the exact claim the correction pass was run to remove, and the correction is invisible unless they also read §2.

## Related

LC-D6-2026-08-24-01 (2026-08-24), #3281 (the partial fix), #1873 (the underlying closed bug). Adjacent open classifier findings: #3335.

## Suggested Fix

Delete §7 item 7 or annotate it in place the way §2 and §4.3 now are, and re-scan §7's other six items for the same residue while the file is open — the correction pass fixed the two passages the prior audit cited by line number, not the claims wherever they appeared.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (§7's other six items, and `material-abstraction.md`'s "Leak B" cross-references)
