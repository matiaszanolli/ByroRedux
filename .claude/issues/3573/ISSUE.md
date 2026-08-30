# #3573 — REN-2026-08-30-D16-01: `docs/engine/renderer.md` still documents M55 as Phase 1 with the output gated OFF

**Labels**: `medium,renderer,doc-rot,documentation`
**Filed**: 2026-08-30 via `/audit-publish`
**Report**: `docs/audits/AUDIT_RENDERER_2026-08-30.md`

> Immutable snapshot of the issue as filed (TD10-001 / #1156). GitHub is
> authoritative for current state — `gh issue view 3573 --json state`.

---

- **Severity**: Medium
- **Dimension**: Volumetrics
- **Location**: `docs/engine/renderer.md` (§"Volumetric lighting (M55)", ~line 665; plus the pipeline bullet at ~line 67)
- **Status**: OPEN — new
- **Description**: The engine's primary renderer reference describes the
  volumetrics pass as scaffolding: *"**Phase 1 (current)** allocates the
  per-FIF 3D images … the inject/integrate dispatch plumbing is wired but the
  output is gated off (`VOLUMETRIC_OUTPUT_CONSUMED = false`) until Phase 2
  adds density+lighting injection (TLAS shadow raymarch + Henyey-Greenstein
  phase) and ray-march integration."* Every clause is false at HEAD. The same
  claim is repeated in the pipeline bullet list near the top of the file
  (*"allocation + layout + dispatch plumbing live, scattering output not yet
  consumed (`VOLUMETRIC_OUTPUT_CONSUMED = false`)"*). The section also states
  `VOLUME_FAR = 200`.
- **Evidence**:
  - `crates/renderer/src/vulkan/volumetrics.rs:546` — `pub const VOLUMETRIC_OUTPUT_CONSUMED: bool = true;`
  - `crates/renderer/shaders/composite.frag:720` — `combined = combined * vol.a + vol.rgb;` (the output *is* consumed)
  - `crates/renderer/shaders/volumetrics_inject.comp:1633`–`1636` — the TLAS shadow ray query the doc says is "Phase 2" work; `:1212` — the HG phase clamp
  - `crates/renderer/shaders/include/shader_constants.glsl:133` — `#define VOLUME_FAR 8960.0`, not 200 (`shader_constants_data.rs:345`–`354` records that the 200 was a units bug: 200 world units = 2.86 m)
  - Session 62 + 69 shipped injection, temporal reprojection, clustered local volumes and the transported combustion solver (`ROADMAP.md:809`)
- **Impact**: A reader (or an auditor) consulting the authoritative renderer
  doc concludes the volumetrics dispatches are dead GPU work and is one step
  from "optimising away" ~0.1–0.25 ms/frame of load-bearing work — precisely
  the mistake `post_passes.rs:440`–`446` warns against in code. It also
  understates the pass's VRAM and ray-query cost by describing an empty
  scaffold.
- **Suggested Fix**: Rewrite both sites against current code: consumed = true,
  the six-volume per-FIF set, the `froxel_extent` derivation with the live
  divisor, the config-driven far plane (`VolumetricsParams::volume_params.x`,
  default 128 m = 8 960 units), and a pointer to
  `docs/engine/procedural-volumetric-fog.md` as the deep spec. Consider
  extending the existing `froxel_grid_cost_matches_the_memory_budget_doc`
  pattern with a one-line `include_str!` assertion that `renderer.md` does not
  contain the string `VOLUMETRIC_OUTPUT_CONSUMED = false`.

---

**Source**: `docs/audits/AUDIT_RENDERER_2026-08-30.md` — REN-2026-08-30-D16-01

## Completeness Checks
- [ ] **SIBLING**: Same stale claim checked in related files (other docs, other in-code comments, audit SKILL files)
- [ ] **TESTS**: Where the codebase already pins a doc/code agreement with an `include_str!` scan, extend that pin rather than relying on review
