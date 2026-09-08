# Issue #4065 — ECS-2026-09-08-D0-01: three stale claims in audit-ecs/SKILL.md steer the audit at code that has moved or does not exist

Filed: 2026-09-08 from `docs/audits/AUDIT_ECS_2026-09-08.md` via `/audit-publish`
Repo state at filing: `bb8ced68`
Labels: low, tech-debt, doc-rot, documentation

---

**Severity**: LOW · **Dimension**: audit infrastructure (`doc-rot`)
**Location**: `.claude/commands/audit-ecs/SKILL.md` — Dimensions 7 and 8
**Source**: `docs/audits/AUDIT_ECS_2026-09-08.md`

## Description

Three assertions the skill uses to steer the audit no longer match the code.

**1. Dim 8 — wrong stage for `footstep_system`.** The skill says:

> `footstep_system` scratch (#932) … (Registered `add_exclusive(Stage::PostUpdate, footstep_system)`.)

It is registered inside `register_late_systems` in `byroredux/src/boot.rs`, i.e. `Stage::Late`, moved by `1382efb0` ("Fix #3652 (SIBLING follow-up): move footstep_system to Stage::Late too"). The stage matters here: the skill's own PostUpdate ordering contract and the `GlobalTransform`-drain invariant are stage-relative, so an auditor following this checks the wrong stage's contract.

**2. Dim 7 — a predicate that no longer exists.** The skill says:

> a regression that lets two of these seven land on the same entity is a correctness bug in the `npc_spawn.rs` spawn-tail's `if runs_*` chain

There is no `runs_` predicate anywhere in the workspace. Behavior selection is now a single `match` over the `AmbientBehavior` enum in `byroredux/src/npc_spawn/ai_package.rs` (`insert_at_spawn` / `insert_at_runtime`), which makes "two behaviors on one actor" structurally impossible rather than a thing to audit for. The invariant the skill wants is real; the mechanism it names is gone, so the check either produces a false gap or is silently dropped.

**3. Dim 7 — stale count.** The skill says `register_component::<T>()` is called "for 15 inspectable types". It is 27.

## Evidence

```
$ grep -n "footstep_system" byroredux/src/boot.rs
1642:        footstep_system,          # inside register_late_systems (starts line 1556)

$ grep -rn "runs_" --include=*.rs byroredux/src crates
(no matches)

$ grep -c "register_component::<" crates/debug-server/src/registration.rs
27
```

## Impact

Audit-steering only — no runtime effect. But (1) and (2) each send an auditor to check something that does not exist, which costs a pass and can manufacture a false finding.

## Related

- #4031 is the same class of defect in the renderer skill (a SKILL instruction under-counting what it points at), so this is an established filing pattern.
- ECS-2026-09-08-D7-01 is the count in (3) as an actual code gap.

## Suggested Fix

Update the three claims. For (3), replace the count with the re-derivation recipe this skill already uses elsewhere for exactly this reason —

```
grep -c 'register_component::<' crates/debug-server/src/registration.rs
```

— rather than a number that moves with every component added.

## Completeness Checks
- [ ] **SIBLING**: check the other audit skills for the same two stale claims (`footstep_system`'s stage appears in `/audit-audio` and `/audit-performance` territory; the seven-procedure roster appears in `/audit-scripting`)
- [ ] **TESTS**: `.claude/commands/_audit-validate.sh` passes after the edit (it gates backticked path references in skill files)
