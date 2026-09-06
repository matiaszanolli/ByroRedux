# #4040 — REN-2026-09-06-D5-06: memory-budget.md's `### Not yet ledgered` says "One is known" and then "Both are listed"

**Labels**: low, memory, renderer, documentation, doc-rot

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-09-06.md` (REN-2026-09-06-D5-06), full 23-dimension `/audit-renderer` sweep at `229306ce`.
Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.

- **Severity**: LOW (doc-rot in the authoritative ledger)
- **Dimension**: Memory/Lifecycle
- **Location**: `docs/engine/memory-budget.md` — the `### Not yet ledgered`
  subsection.
- **Status**: **NEW.** Not in the 151 open issues; not in the 2026-08-30 or
  2026-09-05 reports.
- **Description**: The subsection opens "A grep of this page for the owning
  subsystem name is the cheapest way to find a gap in it. **One** is known and
  unquantified:", lists a single bullet (`StagingPool` retained capacity), and
  closes "**Both** are listed rather than estimated on purpose: a fabricated
  number on this page is worse than an acknowledged hole".

  `git show 6cdb598c -- docs/engine/memory-budget.md` shows the section
  landed with two bullets — per-entity morph slots and the staging pool. The
  morph bullet was correctly removed when #3661 gave morph slots their own
  `## Morph-target GPU resources` section and the count was updated to "One",
  but the closing sentence was not.
- **Evidence**: The three quoted strings are adjacent in the current file.
  `git log -S "Not yet ledgered" -- docs/engine/memory-budget.md` →
  `6cdb598c`, whose diff carries both bullets.
- **Impact**: None at runtime. It matters only because this is the one
  subsection whose entire purpose is to be an accurate inventory of the page's
  own gaps, and a reader counting bullets against the prose will conclude one
  is missing from the render rather than from the sentence.
- **Related**: `REN-2026-09-05-D5-01` (the sibling stale-preamble fix in the
  same file, fixed by `b10a7b7e`), `REN-2026-09-06-D5-02` (a gap that belongs
  in this subsection, or better, in a real row).
- **Suggested Fix**: Change "Both are listed" to "It is listed", or restore a
  second bullet if `REN-2026-09-06-D5-02` is resolved by acknowledgement
  rather than by a row. Prefer the row.

---

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files (other shader mirrors, other passes)
- [ ] **TESTS**: A regression test pins this specific fix
