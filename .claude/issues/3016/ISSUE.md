# SCR-D7-03: base-record script attach gated in two branches and ungated in three

**Issue**: #3016
**Severity**: MEDIUM
**Dimension**: 7 — Engine Attach & Trigger Wiring
**Labels**: `medium,scripting,bug`
**Source report**: `docs/audits/AUDIT_SCRIPTING_2026-08-16.md`
**Filed**: 2026-08-17 via `/audit-publish`

---

Filed from `docs/audits/AUDIT_SCRIPTING_2026-08-16.md` (Dimension 7 — Engine Attach & Trigger Wiring).

**Location**:
- **ungated** — `byroredux/src/cell_loader/references/synth_child.rs`:599 (main static mesh), :155 (trigger volume), `byroredux/src/cell_loader/references/mod.rs`:610 (actor)
- **gated** — `synth_child.rs`:238-248 (LIGH light-only), :333-343 (fxlight)

## Description

`refr_script_instance_for_synth_child` correctly restricts the **outer REFR's own** VMAD to `synth_idx == 0` (#2026). Orthogonally to that, each synthetic child has its own `child_form_id` and therefore its own base record's `SCRI`/`VMAD`.

**Three spawn branches attach that base-record script for every child; two attach it only for child 0** — even though in those two the entity itself *is* spawned for every child. Nothing in the code or comments records which policy is intended.

## Impact

Either three branches over-attach (running a base-record script once per synthetic child) or two under-attach (dropping scripts on children that should have them). Both are live today and the code gives no way to tell which is the bug.

The uncertainty is the finding: a reader cannot resolve it from the source, so the next person to touch this will pick arbitrarily.

## Suggested Fix

Decide the policy, apply it to all five branches, and **record the rationale in a comment** — the base-record script belongs to the child's own base record, which argues for ungating, but that is a design call the audit deliberately does not make for you.

## Related

- #3015 (SCR-D7-2026-08-16-02 — one of the ungated branches; fix as one policy)
- #2026 (the outer-REFR VMAD restriction, which is a separate and correct rule)

## Completeness Checks
- [ ] **ONE-POLICY**: All five branches agree after the fix
- [ ] **RATIONALE**: The chosen policy is documented so it cannot drift apart again
- [ ] **ORTHOGONAL**: The #2026 outer-REFR rule is preserved and not conflated with the base-record rule
- [ ] **TESTS**: A regression test covers a multi-child REFR in both a gated and an ungated branch

---

*Immutable snapshot of the issue as filed. GitHub is authoritative for current state — query `gh issue view 3016 --json state` when live state is needed.*
