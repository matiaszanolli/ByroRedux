# REN-D5-2026-09-20-05: Drop's own SAFETY comment still says 'four' load-bearing orderings — #4188 made it three

- **ID**: REN-D5-2026-09-20-05
- **Labels**: low,renderer,memory,documentation,doc-rot
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: LOW · **Dimension**: Memory/Lifecycle
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D5-2026-09-20-05)

**Location**: `crates/renderer/src/vulkan/context/teardown.rs:244`

**Description**
#4188 demoted the placeholder orderings in the helper doc but missed the same-file SAFETY comment on Drop.

**Evidence**
Audit D5, 2026-09-20.

**Impact**
The two doc sites disagree; a future teardown edit trusts the wrong one.

**Suggested Fix**
One-word fix, plus the skill rule 'check Drop's own SAFETY comment agrees'.

## Completeness Checks
- [ ] **TESTS**: A regression test pins this specific fix
