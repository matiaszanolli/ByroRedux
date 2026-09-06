# #3877: TD5-2026-09-05-02: the two Dim 5 grep patterns disagree with each other, and neither sees the `TBD` convention the codebase actually uses

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD5-2026-09-05-02) via `/audit-publish`, 2026-09-05. Labels: `low,esm-plugin,tech-debt,bug`.

Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3877 --json state`.

---

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-09-05.md` (TD5-2026-09-05-02), `/audit-tech-debt` full 9-dimension sweep at `fa5c4191`. Premise verified against HEAD at publish time.

> `Location:` line numbers are as-audited and drift; anchor on the named symbols.


- **Severity**: LOW
- **Dimension**: 5 (Stale Markers)
- **Location**: `.claude/commands/audit-tech-debt/SKILL.md` — the Dimension 5
  **Discovery** block, both commands; live instance at
  `crates/plugin/src/esm/records/items.rs` (the `b"DNAM"` FNV `WEAP` arm, offset-20 read)
- **Status**: NEW
- **Age**: `items.rs` `TBD` — `67e1baafe`, 2026-06-09 (2.9 months). The pattern
  asymmetry predates it.
- **Effort**: trivial (≤30 min)
- **Description**: Two independent pattern defects, same root cause — the marker
  vocabulary is hand-written twice and never reconciled.

  **(a) The shader command is narrower than the source command.** Line 1 is
  `(TODO|FIXME|HACK|XXX)\b`; line 2 is `(TODO|HACK)` — it drops `FIXME` and
  `XXX` entirely, and drops the `\b` anchor. A `// FIXME` in `triangle.frag` or
  any of the 22 shaders / 15 GLSL includes would not be reported by the
  dimension that exists to report it. (I re-ran the shader scan
  case-insensitively across all four tokens: still 0, so nothing is hidden
  *today*.)

  **(b) The source command misses `TBD`, a convention in live use.** Exactly one
  site in the tree uses it, and the recipe has never seen it:

  > `// Offset 20 — next f32 present in the blob; semantic`
  > `// TBD (may or may not duplicate the NAM6 spread). Not`
  > `// stored; NAM6 remains the authoritative spread source.`

  On merit this site is **not** debt and I am not filing it as one — it is an
  honest documented-unknown that records its own resolution in place ("Not
  stored; NAM6 remains the authoritative spread source"), the same class as the
  documented staged-rollout exclusions. But it is precisely the shape this
  dimension hunts (an unresolved format semantic parked in a comment), and it
  sits four lines below a comment block whose neighbour was already the subject
  of a real finding — **#3324** (CLOSED) closed a false-premise comment in this
  very `DNAM` arm that *"sent two audits searching this blob."* A marker
  vocabulary that cannot see the one convention used in the most audit-prone
  comment block in the ESM parser is a measurable blind spot.
- **Evidence**:
  ```
  SKILL.md Dim 5 Discovery, command 1: grep -RInE '(TODO|FIXME|HACK|XXX)\b' crates byroredux
  SKILL.md Dim 5 Discovery, command 2: grep -RInE '(TODO|HACK)' crates/renderer/shaders/
                                                    ^^^^^^^^^^ no FIXME, no XXX, no \b

  $ grep -RInE '\b(WIP|TBD|KLUDGE|XXX_)\b' crates byroredux --include='*.rs'
  crates/plugin/src/esm/records/items.rs:348:  // TBD (may or may not duplicate the NAM6 spread). Not
  # ^ one hit, never surfaced by four consecutive Dim 5 runs
  ```
- **Impact**: Two silent under-counts. (a) is the more dangerous half — the
  shader tree is where `feedback_shader_struct_sync.md` says lockstep drift
  hurts most, and a `FIXME` there is exactly the breadcrumb an auditor would
  want. Neither is hiding live debt today; both are measured, not assumed.
- **Related**: #3456 (CLOSED — Dim 9's regex under-count, 19 %, the direct
  precedent for this class of finding); #3324 (CLOSED — the false-premise
  comment four lines above the `TBD` site).
- **Suggested Fix**: Make both commands share one vocabulary —
  `(TODO|FIXME|HACK|XXX|TBD|WIP|KLUDGE)\b` — and point the second at
  `crates/renderer/shaders/` with the same pattern as the first. Add `TBD` to
  the exclusion guidance with the `items.rs` site as the worked example of a
  *legitimate* documented-unknown, so the next auditor doesn't over-file it.

## Completeness Checks

- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test (or gate) pins this specific fix
- [ ] **DROP**: If Vulkan objects change, the Drop impl stays reverse-order correct
- [ ] **LOCK_ORDER**: If a RwLock scope changes, TypeId-sorted acquisition is preserved
