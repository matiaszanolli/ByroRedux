# #4420 — RT-2026-09-16-04: Three improvements are sitting below loose `≤ baseline` gates, and #4124 is resolved but still open

**Labels**: low,tech-debt,bug,game:fo3,game:skyrim,game:oblivion

**Source**: `docs/audits/AUDIT_RUNTIME_2026-09-16.md` (RT-4)

- **Severity**: LOW
- **Status**: NEW (bookkeeping)
- **Dimension**: baseline hygiene
- **Description**:
  - **Loose mesh-cache gates.** `mesh_cache_failed_count` is now 0 on fo3
    (baseline 3) and skyrim_se (baseline 9). Both gates are `≤ baseline`, so a
    regression back to 3 or 9 failed parses would pass silently. The FO3 file
    has no hold reason, so it can be regenerated now. Skyrim's regen waits on
    RT-2.
  - **#4124 left open.** The Oblivion TSV was regenerated on 2026-09-13 with a
    full #4124 rationale (`world.owners` breakdown, cross-checked capture), but
    #4124 is still OPEN. Today's capture matches that regen exactly (745,
    330/20b/2c).
- **Suggested Fix**: `--regen` fo3 (all rows from one capture, per #4195),
  close #4124 with a pointer to the 2026-09-13 regen, and fold Skyrim's
  mesh-cache row into the RT-2 regen.

## Completeness Checks
- [ ] **SIBLING**: All four `bench_draws_*` rows regenerated from one capture (#4195 invariant)
