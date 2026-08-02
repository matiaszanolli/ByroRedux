# TD1-NEW-02: draw_frame's 07-25 extraction fix landed and holds, but the function re-grew around new shadow-policy/volumetrics dispatch code

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2255

**Dimension**: 1 (File/Function/Module Complexity)
**Location**: `crates/renderer/src/vulkan/context/draw.rs:872-3001` (`draw_frame`, ~2131 LOC)
**Status**: NEW (this is a continuation of a report-only finding first raised in `docs/audits/AUDIT_TECH_DEBT_2026-07-25.md` as TD1-NEW-02; no GitHub issue was ever filed for it, so this is the first time it's tracked here)

**Description**: The 07-25 tech-debt report's suggested fix — extract the FSR-frame-parameter-assembly block into a standalone `fn build_fsr_frame_parameters(...)` — **did land**: it now exists as a free function at `draw.rs:438-470`, alongside the file's existing `dof_effective_view_proj`/`fsr_gated_dof` pattern, exactly as suggested, with its own dedicated unit tests (`draw.rs:472-568`). That's real progress on the specific complaint. However, `draw_frame` itself did not shrink — it grew from 2048 LOC (07-25 measurement) to ~2131 LOC now (file itself grew 3210→3798, +18%). The new growth is from the shadow-policy refactor (`1fb79038`) and volumetric/local-fog-volume integration landing more inline dispatch/barrier code directly in the function body rather than extracted siblings.

**Evidence**: `git show 9a9a4c5d:...draw.rs` (the commit that closed #1857) had `draw_frame` at 1927 LOC; 07-25 report measured 2048 LOC; current `draw.rs:872` (`pub fn draw_frame`) to `draw.rs:3001` = 2130 lines. `build_fsr_frame_parameters` confirmed present at `draw.rs:438`.

**Impact**: Purely maintainability — not a correctness issue. Confirms the file's extraction discipline is real (functions do get pulled out when a fix lands) but reactive rather than preventing the parent function from re-growing on the next feature arc. This is the second time this specific function has been the subject of an extraction that held only days to weeks.

**Related**: Existing #1857 (CLOSED, file-level split); this report's own TD1-004/#1749 (`VulkanContext::new()`) is the sibling instance of the same "giant constructor/dispatcher re-grows around a fix" shape in a different file.

**Suggested Fix**: Same pattern again — the shadow-policy/global-only-mesh BLAS gating and the volumetrics-UBO-write block newly inlined in `draw_frame` are both pure data-assembly (no borrow-checker reason they must stay inline) — extract a `fn build_volumetrics_write(...)` and/or fold the shadow-policy-related setup into a helper near the acceleration-manager calls it feeds. Opening this as a standing tracking issue (rather than a recurring report-only finding) is itself the fix for the process gap — future growth becomes visible between audit cycles instead of independently rediscovered each time.

## Completeness Checks
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
