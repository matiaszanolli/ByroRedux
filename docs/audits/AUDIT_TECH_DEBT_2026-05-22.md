# Tech-Debt Audit — 2026-05-22

## Executive Summary

**0 NEW findings + 3 carryovers** across all 10 dimensions. Yesterday's single NEW finding (`TD7-NEW-02 / TD10-NEW-02` — 4 stale `tri_shape.rs` refs in audit skill files) has been **fixed in code** but the tracker (#1229) is still OPEN — adds to the broader #1156 stale-ISSUE-file pattern. No regressions from today's 15+ commits. The two BLOCKED Vulkan-recording file-split carryovers (`draw.rs`, `context/mod.rs`) held steady at exactly 2899 + 2661 LOC — no creep over 24h.

| Severity | NEW | Carryover | Total | Dimensions affected |
|----------|-----|-----------|-------|---------------------|
| HIGH     | 0   | 0         | 0     | — |
| MEDIUM   | 0   | 1         | 1     | D9 (BLOCKED carry — TD9-200/201) |
| LOW      | 0   | 2         | 2     | D4 (TD4-201, TD4-202) |

The dim-10 finding (#1229 fixed-in-code-but-tracker-OPEN) is a documentation hygiene loop, not a new defect — folding into the #1156 immutable-snapshot policy discussion.

## Baseline Snapshot

| Metric | Today (2026-05-22) | Yesterday (2026-05-21) | Δ |
|---|---:|---:|---:|
| TODO / FIXME / HACK / XXX markers | 4 | 4 | 0 |
| ↳ of which *active* (not closure-mention prose) | **0** | **0** | 0 |
| `#[allow(dead_code)]` | 26 | 26 | 0 |
| `unimplemented!()` / `todo!()` | 0 | 0 | 0 |
| `panic!("not yet"|"not impl")` | 0 | 0 | 0 |
| `#[ignore]` tests | 126 | 126 | 0 |
| `#[allow(unused...)]` | 20 | 20 | 0 |
| Files > 2000 LOC | 2 (draw.rs 2899, context/mod.rs 2661) | 2 (same; 2899 + 2661) | 0 |

Baseline persisted to [`/tmp/audit/tech-debt/baseline.txt`](baseline.txt) before cleanup.

## Top 10 Quick Wins

None this cycle. Yesterday's trivial-effort finding (TD7-NEW-02 / TD10-NEW-02) was already fixed in code over the last 24h — confirmed by `_audit-validate.sh` reporting `OK: all path references valid (293 refs across 22 skill files)`. Only the GitHub tracker (#1229) is lagging — closing it is the only quick win, and it's a one-click action not a code change.

## Top 5 Medium Investments

1. **TD9-200 / TD9-201** *(BLOCKED carryover)* — `crates/renderer/src/vulkan/context/draw.rs` (2899) + `crates/renderer/src/vulkan/context/mod.rs` (2661) over the 2000 LOC ceiling. **Blocked** on RenderDoc-driven smoke test infrastructure per `feedback_speculative_vulkan_fixes.md` ("Don't ship Vulkan render-pass/pipeline/barrier changes when failure modes are invisible to cargo test"). 24h LOC change: 0 (held steady). Unblocking precondition: design a captured-frame baseline harness.
2. **TD10-001** *(carryover, #1156)* — Stale local `.claude/issues/<N>/ISSUE.md` files marked Open while GitHub says Closed. 164 files at last count (will need a re-count). Recommended fix is policy (Option C in #1156): document the immutable-snapshot semantics and stop relitigating. Today's #1229 fixed-in-code-but-tracker-OPEN is a sibling instance — same root cause.
3. **TD4-201** *(carryover)* — 32 bare-hex NIF version compares should adopt `NifVersion::*` constants. Mechanical sed-style work.
4. **TD4-202** *(carryover)* — 112 ESM subrecord size literals (`if data.len() == N`) should map to named `RecordType::*_SIZE` constants. Larger sweep but mechanical.
5. *(slot empty — no new medium-effort items this cycle)*

## Findings

### HIGH
None.

### MEDIUM

#### TD9-200 / TD9-201 *(carryover, BLOCKED)* — Vulkan-recording files exceed 2000-LOC ceiling

- **Files**: `crates/renderer/src/vulkan/context/draw.rs` (2899 LOC), `crates/renderer/src/vulkan/context/mod.rs` (2661 LOC)
- **Severity**: MEDIUM (file complexity)
- **Effort**: large (decompose first)
- **Age**: 24h since last check — no LOC delta. Last meaningful growth was the M58 bloom + M55 volumetric integrations through 2026-05-09 / 2026-05-15.
- **Observation**: Both files are Vulkan command-recording adjacent — `draw.rs` is `draw_frame` itself; `context/mod.rs` is the orchestration container. Splitting either requires moving `vkCmd*` recording sequences across module boundaries, which is exactly the failure surface called out in `feedback_speculative_vulkan_fixes.md`.
- **Why this matters**: Each new RT pass, denoiser stage, or material-table refactor lands additional inline recording code. The trajectory is +0 LOC this cycle, but the longer-term arc (2899 today vs ~2200 pre-M58) is upward; without a RenderDoc smoke harness gating splits, the file will keep growing until something forces an unsafe split.
- **Fix**: design a RenderDoc-driven captured-frame baseline so a split can be verified for byte-equality of the next-frame swapchain image. This is a precondition for the split, not the split itself. File the split work behind that.

### LOW

#### TD4-201 *(carryover)* — 32 bare-hex NIF version compares should adopt `NifVersion::*` constants

- **Files**: scattered across `crates/nif/src/blocks/*.rs`
- **Severity**: LOW (magic-number readability)
- **Effort**: small (mechanical)
- **Age**: pre-Session-34, original audit 2026-05-13.
- **Observation**: Bare hex compares like `if version >= 0x14020007` instead of `NifVersion::SKYRIM_LE` (or equivalent). The `NifVersion` constants exist and are used in some files; the inconsistency is the readability cost.
- **Fix**: sed across `crates/nif/src/blocks/`. Add a regression test asserting bare-hex compares are absent (grep gate).

#### TD4-202 *(carryover)* — 112 ESM subrecord size literals should map to named `RecordType::*_SIZE` constants

- **Files**: scattered across `crates/plugin/src/esm/records/*.rs`
- **Severity**: LOW (magic-number readability)
- **Effort**: medium (112 sites, but each is mechanical)
- **Age**: pre-Session-34.
- **Observation**: `if data.len() == 24` style size checks where the constant `24` is the documented size of a specific subrecord. Should reference a struct-derived `mem::size_of::<...>` or a named const per `crates/plugin/src/esm/records/<record>.rs`.
- **Fix**: per-record sweep; for each record file, define `pub const TNAM_SIZE: usize = …` next to the struct and replace the literal compares.

## Verified-Clean Dimensions (no findings this cycle)

### Dim 1 — Stale Markers: PASS
4 lines hit `(TODO|FIXME|HACK|XXX)\b` but inspection shows all 4 are closure-mention prose, not active markers:

| File | Content |
|---|---|
| `crates/bgsm/src/bgem.rs:122` | "Order matches the reference's `// FIXME` note" — describes external comment |
| `byroredux/src/main.rs:1390` | "Closes the #242 consumer-side TODO" — closure note |
| `crates/nif/src/blocks/bs_geometry.rs:552` | "Per the `// FIXME` at" — references external comment |
| `byroredux/src/scene.rs:630` | "Closes the #242 consumer-side TODO (#1055)" — closure note |

Zero active TODO/FIXME/HACK/XXX markers in the codebase. Steady at 0 since 2026-05-21.

### Dim 2 — Dead Code: PASS
26 `#[allow(dead_code)]` annotations, unchanged from yesterday. The recent M28.5 additions (`character.rs`, `world.rs::cast_ray_down`, `world.rs::update_query_pipeline`) introduced zero new dead-code annotations — all added functions are wired and called.

### Dim 3 — Logic Duplication: PASS
Today's deltas (M28.5 spawn fix, BGSM cycle resolver #1148, PKIN/SCOL recursion #1180+#1182, MOVS coverage #1179) all added single-site implementations or test coverage. No new duplication patterns. The `EXTERIOR_CELL_UNITS = 4096.0` consolidation (TD3-202 / #1112) continues to hold; all remaining `4096.0` literals are test assertions, feature defaults that happen to coincide, or boundary checks — none are duplication regressions.

### Dim 4 — Magic Numbers: 2 carryover only
TD4-201 + TD4-202 above. Today's M28.5 changes introduced no new magic-number debt: `CharacterController::HUMAN` is a named const with per-field documentation, `controller.offset = CharacterLength::Absolute(4.0)` carries a 10-line explanatory comment (`crates/physics/src/world.rs::move_character`), and `MAX_DT: f32 = 1.0 / 30.0` is a named scope-local const with rationale in [systems/character.rs](byroredux/src/systems/character.rs#L96).

### Dim 5 — Stub Implementations: PASS
Zero `unimplemented!()` / `todo!()` / `panic!("not yet"|"not impl")` in the codebase. ESM per-game coverage continues to ride the unified records tree (the legacy `crates/plugin/src/legacy/{tes3,tes4,tes5,fo4}.rs` per-game stubs were removed under #390 and have not been re-introduced).

### Dim 6 — Test Hygiene: PASS
126 `#[ignore]` tests, unchanged from yesterday. The #1050 TD6-* findings stand as known coverage gaps (golden-frame coverage gap on TAA / GPU skin / composite — REN-D14-NEW-02 from today's Dim 14 audit, filed as #1231, is a narrower instance of TD6-006). Today's new tests (`pkin_expansion_tests.rs`, `scol_expansion_tests.rs`, `cell/tests/movs.rs`, `bs_geometry_skin_tests.rs`, `vertex_color_precedence_tests.rs`) all carry concrete assertions, not smoke-only patterns.

### Dim 7 — Stale Documentation: PASS
`_audit-validate.sh` (the post-#1114 structural gate) reports: **"Checked 293 refs across 22 skill files. OK: all path references valid."** This means yesterday's TD7-NEW-02 finding (4 stale `tri_shape.rs` refs) has been fixed in the audit skill files — all 4 sites now correctly cite `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs` with the post-#1118 split context. The fix appears to have shipped between yesterday's audit and today; the gate's continued green is the regression guard.

### Dim 8 — Backwards-Compat Cruft: PASS
Zero `// removed:` / `// deleted:` / `// deprecated:` breadcrumb comments in the codebase. One `_world` / `_dt` prefixed-unused-param pair survives in `crates/core/src/ecs/scheduler.rs:827` (a test stub `read_position_system`), which is the legitimate test-fixture pattern, not a refactor leak. The CLAUDE.md "delete completely, no breadcrumbs" policy is intact.

### Dim 9 — File / Function Complexity: 1 carryover (above)
TD9-200 / TD9-201. No new offenders. Next-tier files (1500-2000 LOC band) are: `main.rs` 1920, `asset_provider.rs` 1815, `nif/import/tests.rs` 1788, `blocks/shader_tests.rs` 1732, `records/actor.rs` 1662 — none over the ceiling, but the band has 10 files within 480 LOC of the threshold. Worth a "next Session-N split" budget if any of them grow.

### Dim 10 — Audit-Finding Rot: 1 NEW *(tracker-only)* + 1 carryover

#### TD10-NEW-03 *(tracker-only, no code finding)* — #1229 fixed in code but tracker still OPEN

- **GitHub**: [#1229](https://github.com/matiaszanolli/ByroRedux/issues/1229) (OPEN, LOW, tech-debt, audit-finding-rot)
- **Code state**: All 4 cited `crates/nif/src/blocks/tri_shape.rs` references in audit skill files have been updated to `crates/nif/src/blocks/tri_shape/bs_tri_shape.rs`. `_audit-validate.sh` confirms 293/293 refs valid.
- **Why this matters**: Per `feedback_audit_findings.md` — stale audit-tracker state is itself audit-finding-rot. Closing #1229 takes seconds; leaving it open inflates the open-tech-debt count and is the same pattern flagged at scale by #1156.
- **Fix**: close #1229 with a note pointing to the validate-script run that confirms the fix; no code change needed. Treat as part of the same TD10-001 / #1156 conversation.

#### TD10-001 *(carryover, #1156)* — Stale local ISSUE.md files marking issues OPEN while GitHub says CLOSED

See yesterday's audit. No new instances generated today; the closure recommendation (Option C in #1156: immutable-snapshot semantics) still pending.

## Deferred

- **TD9-200 / TD9-201** (Vulkan file splits) — gated on RenderDoc baseline infrastructure. Track under the M28.5 / M40 follow-up bucket rather than as an actionable Dim 9 fix.

## Methodology Notes

- Sub-agent delegation attempted for Dim 15 of today's parallel renderer audit but the sub-agent returned interim status without writing files; this audit was done directly. The pattern is logged but not a Dim 10 finding (sub-agent reliability isn't a tech-debt category).
- Path-drift validation via `_audit-validate.sh` was decisive here — it caught yesterday's TD7-NEW-02 fix landing without a tracker close, and it's the single highest-leverage tool for Dim 7 / Dim 10 in this codebase.
- 24h commit set (15 commits) reviewed for new tech debt: all are fixes (no stubs, no new magic numbers beyond named constants, no new TODO markers). M28.5 work in particular added well-documented constants and per-field explanatory comments.
- Severity calibration: nothing fired the LOW → MEDIUM promotion triggers from the skill spec. The two MEDIUM carryovers retain their severity from yesterday.
