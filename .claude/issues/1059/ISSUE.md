# Issue #1059: Tech-Debt: Audit-skill path rot after Session 36 monolith splits [batch]

**State:** OPEN  
**Labels:** documentation, enhancement, medium, tech-debt

## What was fixed

TD7-025..033 (_audit-common.md paths): ALREADY FIXED by commit 3f75b390 (#1054).
TD7-034..038 (audit-*.md stale .rs refs): ALREADY FIXED by same commit.

This PR fixed:
- TD7-039: audit-renderer.md GpuInstance "3 shaders" → "5 shaders" (+ water.vert, caustic_splat.comp at lines 241/252)
- TD7-039: feedback_shader_struct_sync.md says 4 shaders → 5 (added water.vert)
- TD10-013: audit-renderer.md:282 + audit-safety.md:76 DBG_ catalog range 628-686 → 739-829
- TD7-041: ROADMAP.md 6 stale open-tracker references to #687/#688/#697/#698 (all CLOSED)
- TD7-042: HISTORY.md MAX_MATERIALS = 1024 → 4096
- TD7-044: gpu_types.rs:159 "wait — six trailing vec4s" thinking-aloud removed
