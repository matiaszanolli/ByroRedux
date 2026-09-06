# #3875: TD4-2026-09-05-06: 26 CRITICAL/HIGH findings across 12 pre-`/audit-publish` reports have no GitHub trace — the mandated `docs/audits/` dedup step returns false-NEW on already-fixed work

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-06) via `/audit-publish`, 2026-09-05. Labels: `low,doc-rot,documentation`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3875 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD4-2026-09-05-06), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.



- **Severity**: LOW
- **Dimension**: 4 — Audit-Finding Rot
- **Location**: `docs/audits/` — 12 reports dated 2026-04-04 … 2026-06-02
- **Status**: NEW
- **Effort**: small (≤2 h)

**Description**
`_audit-common.md:299-309` makes dedup mandatory and step 3 is *"Scan
`docs/audits/` for prior reports covering the same issue"*, with the routing
rule *"If OPEN: skip. If CLOSED: verify fix… If no match: report as NEW."*
That rule assumes every report finding reached GitHub. For the pre-`/audit-publish`
cohort it did not.

Matching every ID'd CRITICAL/HIGH finding in reports older than 90 days
(< 2026-06-07) against all 3,730 repo issue titles:

```
pre-2026-06-07 reports w/ ID'd CRITICAL/HIGH findings: 50
ID'd CRITICAL/HIGH findings: 131   no GitHub title match: 26
```

The 26 cluster in 12 reports:

| Report | Untraced / total | Findings |
|---|---|---|
| `AUDIT_NIF_2026-04-11.md` | 6/6 | NIF-04-11-C1/C2/C3 (CRITICAL), H1/H2/H3 |
| `AUDIT_NIF_2026-04-04.md` | 3/4 | NIF-009 (CRITICAL), NIF-008, NIF-301 |
| `AUDIT_SAFETY_2026-04-05.md` | 3/3 | SAFE-01, SAFE-02, SAFE-03 |
| `AUDIT_FNV_2026-04-21.md` | 2/6 | FNV-ESM-4, FNV-ESM-6 |
| `AUDIT_FO3_2026-05-01.md` | 2/2 | FO3-4-01, FO3-4-02 |
| `AUDIT_FO4_2026-06-02.md` | 2/2 | FO4-D6-GAP-05, FO4-D8-NEW-01 |
| `AUDIT_POSITIONING_DECALS_2026-04-13.md` | 2/2 | PD-01, PD-02 |
| `AUDIT_RENDERER_2026-04-12c.md` | 2/3 | RL-01 (CRITICAL), RL-02 |
| + 4 more, 1 each | 4/4 | CONC-D2-NEW-01, PERF-04-11-H3, MEM-002, SK-D5-NEW-01 |

**Spot-check: 3 of 3 were fixed, silently.**

- **SAFE-01** *"`write_mapped` silently truncates data exceeding buffer size"* —
  `crates/renderer/src/vulkan/buffer.rs:1273` now logs
  `"write_mapped: data ({} bytes) exceeds buffer capacity ({} bytes) — truncating"`.
  No longer silent.
- **PD-02** *"APP_CULLED flag (0x20) not checked in NIF walker"* — now filtered
  import-side with a dedicated regression file
  `crates/nif/src/import/tests/app_culled_visibility.rs` (#3640).
- **NIF-04-11-H1** *"Property inheritance from parent `NiNode`s is not applied"* —
  `extract_material_info(scene, shape, inherited, &mut pool)` threads an
  `inherited: &[BlockRef]` list, with `alpha_flag_tests.rs` pinning the
  shape-intent-wins cascade (#1201).

So the failure mode is not "26 live CRITICAL bugs". It is that an auditor
executing the mandated dedup finds a CRITICAL in `docs/audits/`, finds no
issue, and concludes **NEW** — re-filing fixed work, or worse, re-deriving a
"regression" against code that never regressed. `RL-01` is the canonical
example: it is recorded in user memory as *"audits claiming RL-01 is unfixed
have a bad premise"*, which is a person having had to absorb this exact loop.

**Impact**
One of the two mandated dedup inputs is unreliable for ~22% of the corpus
(140 of 629 reports predate the workflow). Cost is auditor time and
false-NEW findings, not runtime behaviour.

**Related**
#3218 (CLOSED) and **#3504 (OPEN)** cover the *opposite* direction —
issue → commit citation. Neither covers report-finding → issue, so this is
adjacent, not duplicate. #3440 (CLOSED — a wrong baseline number inside a
`docs/audits/` report, the same "reports rot too" family).

**Suggested Fix**
Cheapest durable fix is a dated caveat in `_audit-common.md`'s Deduplication
section: *"Reports dated before 2026-06-07 predate `/audit-publish`; their
findings have no issue trace. For those, verify against **code**, not GitHub —
absence of an issue is not evidence the finding is open."* Optionally extend
`scripts/check-issue-traceability.sh` with a report→issue direction so the
backlog is measured rather than rediscovered.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
