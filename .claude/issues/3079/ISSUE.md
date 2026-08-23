# Issue #3079: SPT-D3-2026-08-16-02: the is_spt dispatch moved out of references/mod.rs — skill and prior report both point at the wrong file

**State**: OPEN
**Labels**: documentation, low, tech-debt, terrain-exterior, speedtree, doc-rot

## Body

Filed from `docs/audits/AUDIT_SPEEDTREE_2026-08-16.md` (Dimension 3 — dispatch location).

**Location**: `.claude/commands/audit-speedtree/SKILL.md` (Scope bullet 1 and Entry points)

## Description

The `is_spt` dispatch **moved out of `references/mod.rs`** — the skill and the prior report both point at the wrong file.

## Evidence

Re-verified 2026-08-17:
```
$ grep -c "is_spt" byroredux/src/cell_loader/references/mod.rs
0

$ grep -rn "is_spt" --include="*.rs" . | grep -v _tests
byroredux/src/scene/nif_loader.rs:200:    let is_spt = label
byroredux/src/cell_loader/references/synth_child.rs:418:  let is_spt = model_path
```

`.claude/commands/audit-speedtree/SKILL.md`:40 and :192 both name `byroredux/src/cell_loader/references/mod.rs` as "the production route" / "Entry points".

## Impact

An auditor following the skill inspects a file with zero `.spt` dispatch in it and finds nothing — then either reports the subsystem clean or has to rediscover the real sites. Both live sites (`scene/nif_loader.rs`, `synth_child.rs`) go unexamined.

This is the fifth audit-skill drift found in this sweep (#2974, #3035, #3046, #3052, this).

## Suggested Fix

Re-point both SKILL.md references to `byroredux/src/scene/nif_loader.rs` and `byroredux/src/cell_loader/references/synth_child.rs`. Run `.claude/commands/_audit-validate.sh` afterwards.

## Related

- #2974, #3035, #3046, #3052 — the other audit-infrastructure drifts this sweep

## Completeness Checks
- [ ] **BOTH-SITES**: Both live `is_spt` locations named, not just one
- [ ] **SCOPE-AND-ENTRY**: SKILL.md:40 and :192 both corrected
- [ ] **PATH-GATE**: `_audit-validate.sh` still passes
