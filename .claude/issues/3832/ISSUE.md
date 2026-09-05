# #3832: REN-2026-09-05-D5-01: memory-budget.md's RT-Denoiser section intro is self-contradicted by its own subsections

Filed from `docs/audits/AUDIT_RENDERER_2026-09-05.md` (REN-2026-09-05-D5-01) via `/audit-publish`, 2026-09-05.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3832 --json state`.

---

**Severity**: LOW
**Dimension**: Memory/Lifecycle (doc-rot)
**Source**: `docs/audits/AUDIT_RENDERER_2026-09-05.md` (`REN-2026-09-05-D5-01`, Dim 5 `MEM-5-01`)

## Location

`docs/engine/memory-budget.md` — the intro paragraph of `## RT-Denoiser & Post-Process Screen-Sized Resources`, vs its own `### Volumetrics (M55)` and `### Glass + Water Caustics` subsections in the same section.

## Description

The section opens by asserting that every resource in it *"had **no ledger entry here** until this sweep (#1872 … grep confirmed zero mentions of SVGF, Bloom, SSAO, TAA, Volumetrics, Water, or Caustic anywhere on this page)."*

That was true when #1872 landed. It is no longer true of the page's current content: the same section now contains a detailed `Volumetrics (M55)` subsection and a `Glass + Water Caustics` subsection, and the VRAM roll-up table has its own Volumetrics row.

## Evidence

`grep -n "grep confirmed zero mentions" docs/engine/memory-budget.md` → line 131; `grep -n "^### Volumetrics (M55)" docs/engine/memory-budget.md` → line 264, nested under the very heading whose intro claims zero mentions of it. Re-verified at publish time.

## Impact

Doc-trust only, no runtime effect — but it is the specific sentence that misdirected this audit run's own brief into asserting the volumetrics ledger entry was missing, so it will likely misdirect the next reader too.

## Suggested Fix

Reword the parenthetical to past tense, or simply drop the "grep confirmed zero mentions… anywhere on this page" clause, which is no longer a true statement about the page it sits on.

## Completeness Checks

- [ ] **SIBLING**: Other "as of this sweep" claims in the same doc checked for the same staleness
