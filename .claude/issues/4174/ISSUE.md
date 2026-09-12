# ECS-2026-09-11-01: Stale `byroredux/src/boot.rs` path references in audit-ecs SKILL.md (two locations)

Labels: low,tech-debt,doc-rot,documentation

**Description**: Dimension 5b of `.claude/commands/audit-ecs/SKILL.md` instructs the auditor to enumerate live `add_to_with_access` declarations and verify no plain `add_to` calls survive via `rg -n '^\s*scheduler\.add_to_with_access\(' byroredux/src/boot.rs` and the matching `add_to` check (lines 154-155). `byroredux/src/boot.rs` does not exist — the scheduler build moved to `byroredux/src/boot/schedule/mod.rs` plus per-stage siblings (`early.rs`, `update.rs`, `post_update.rs`, `physics.rs`, `late.rs`). A second, independent stale reference to the same removed file sits at line 305 of the same skill file, in the Dim 7 AI-package-lifecycle discussion.

**Evidence**:
`ls byroredux/src/boot.rs` -> "No such file or directory"; `find . -name boot.rs -not -path './target/*'` -> no results. The correct check is `rg -n '^\s*scheduler\.add_to\(' byroredux/src/boot/schedule/*.rs` (confirmed empty). The three release assertions this dimension protects live in `byroredux/src/boot/registries.rs` (already correctly pointed at elsewhere in the same skill file).

**Impact**: None on runtime behavior. A future auditor who runs the given `rg` command literally against `boot.rs` gets an empty/error result and could misread that as "confirmed zero plain `add_to` calls" for the wrong reason. Purely an audit-tooling-accuracy issue. `_audit-validate.sh` already lists this exact basename as an advisory (non-fatal) miss, alongside the same stale basename in `audit-fnv`, `audit-save`, `audit-scripting`, and `audit-tech-debt` SKILL.md files, and several `docs/engine/*.md` references.

**Related**: Same basename is stale in `audit-fnv/SKILL.md:188`, `audit-save/SKILL.md:86`, `audit-scripting/SKILL.md:1517`, `audit-tech-debt/SKILL.md:221,224`.

**Suggested Fix**: Update the two `rg` targets at lines 154-155 from `byroredux/src/boot.rs` to `byroredux/src/boot/schedule/*.rs`, and update the line-305 reference to point at the owning `byroredux/src/boot/schedule/*.rs` file for the AI-package scheduler registration it describes.



## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers, other systems)
- [ ] **TESTS**: A regression test pins this specific fix



*Filed via /audit-publish from `docs/audits/AUDIT_ECS_2026-09-11.md`.*
