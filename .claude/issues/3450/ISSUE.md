# #3450 — TD4-2026-08-27-04: two audit SKILL files pin GpuCamera at 352 B and name a test that no longer exists — the struct grew to 368 B on 2026-08-26

Labels: `low,renderer,tech-debt,doc-rot,documentation`
Filed: 2026-08-28 · Source report: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md`

---

**Severity**: LOW · **Dimension**: 4 — Audit-Finding Rot · **Source**: `docs/audits/AUDIT_TECH_DEBT_2026-08-27.md` (TD4-2026-08-27-04)

**Location**: `.claude/commands/audit-renderer/SKILL.md:115`, `.claude/commands/audit-regression/SKILL.md:149`

**Age**: `4dcbd187` (2026-08-26, #3323 — `exterior_sky_tint` vec4 appended)

## Description
Both skills carry a "sizes pinned by tests — confirm they hold" instruction naming `gpu_camera_is_352_bytes`. That test no longer exists; `GpuCamera` grew 352 → 368 B when #3323 appended `exterior_sky_tint`, and the live pin is `gpu_camera_is_368_bytes`.

`audit-renderer/SKILL.md:115` is otherwise scrupulously current — its `GpuInstance` (160 B) and `GpuMaterial` (432 B, with the full 260→…→432 history) entries were updated through 2026-08-25 — so the `GpuCamera` clause is a single missed field in an otherwise maintained line, not general neglect.

This is a **different file set** from the `docs/engine/shader-pipeline.md` / `memory-budget.md` GPU-struct drift filed by a concurrent audit in the same suite run: different files, different struct, different growth event.

## Evidence
Verified at publish time (2026-08-28):

```
$ grep -rn "gpu_camera_is_352_bytes" .claude/commands/
.claude/commands/audit-regression/SKILL.md:149:  `GpuCamera` = 352 B (`gpu_camera_is_352_bytes`). Run them:
.claude/commands/audit-renderer/SKILL.md:115:... `GpuCamera` = **352 B** (`gpu_camera_is_352_bytes` — ...)

$ grep -rn "fn gpu_camera_is" crates/renderer/src/
crates/renderer/src/vulkan/scene_buffer/gpu_instance_layout_tests.rs:66:fn gpu_camera_is_368_bytes() {
      assert_eq!(size_of::<GpuCamera>(), 368,
          "GpuCamera must be 368 B (352 B + 16 B exterior_sky_tint vec4, #3323) ...");

$ .claude/commands/_audit-validate.sh | sed -n '4,12p'
ADVISORY (audit skills) — backticked symbols not found in any tracked source file:
  gpu_camera_is_352_bytes                        audit-regression audit-renderer
```

The gate's own advisory already names it — this is the fourth consecutive audit in which the GPU-struct size drift recurs, and the first in which the gate flagged it and the flag was not acted on.

## Impact
An auditor following `audit-regression/SKILL.md:149`'s "Run them:" instruction runs a test name that does not exist and gets a silent zero-test pass, which reads as green. Blast radius is the audit tier only.

## Related
#3201 (the 336→352 instance), #3240, and the `shader-pipeline.md` / `memory-budget.md` sites filed concurrently in the same suite run.

## Suggested Fix
`352 B` → `368 B` and `gpu_camera_is_352_bytes` → `gpu_camera_is_368_bytes` at both sites, with the growth history appended in the style `audit-renderer/SKILL.md` already uses for the other two structs.

**Mechanism note** (from the report, offered rather than filed separately): every recurrence has been fixed by hand, and each hand fix has held for exactly as long as the struct did not grow again. The durable fix is generation, not vigilance — have `crates/renderer/build.rs`, which already emits `shaders/include/shader_constants.glsl` from `shader_constants_data.rs`, additionally emit a small generated table of `struct → size → pin-test-name`, and have `_audit-validate.sh` diff the backticked sizes in `.claude/commands/**` and `docs/engine/**` against it. That converts a recurring MEDIUM into a build failure at the moment of growth.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (every other backticked GPU-struct size/pin-test name in `.claude/commands/**` and `docs/engine/**`)
- [ ] **TESTS**: `.claude/commands/_audit-validate.sh`'s symbol advisory no longer lists `gpu_camera_is_352_bytes`
