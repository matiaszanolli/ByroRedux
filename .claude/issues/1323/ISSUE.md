# #1323 -- TD-D9: 7 files/fns newly exceed LOC threshold

_Snapshot as filed 2026-05-29 from /audit-publish (AUDIT_TECH_DEBT_2026-05-28)._

**Severity**: LOW | **Dim 9** — File / Function Complexity (new crossings)
**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-05-28.md` (TD9-NEW-02, TD9-NEW-03, TD9-NEW-04, TD9-NEW-05, TD9-NEW-06, TD9-NEW-07, TD9-NEW-08)
**Domain**: renderer | **Effort**: medium (per file) — decompose before scheduling

Three binary-crate files newly crossed the 2000-LOC ceiling this cycle, and four functions across the codebase exceed 400 LOC:

| ID | File / Function | LOC | Proposed split axis |
|---|---|---|---|
| TD9-NEW-02 | `byroredux/src/asset_provider.rs` | 2561 | BSA/BA2 resolution vs TextureProvider vs mesh extraction |
| TD9-NEW-03 | `byroredux/src/commands.rs` | 2115 | per-command-group module (stats / entities / systems / tex.* / scene) |
| TD9-NEW-04 | `byroredux/src/main.rs::new()` | 504 LOC fn (file 2448) | boot/config plumbing vs event-loop registration vs system wiring |
| TD9-NEW-05 | `crates/plugin/src/esm/reader.rs::parse_esm_with_load_order` | 879 LOC | 109-arm match → dispatch table or per-record-family module |
| TD9-NEW-06 | `crates/renderer/src/vulkan/context/resize.rs::recreate_swapchain` | 669 LOC | attachment creation vs pipeline rebuild vs descriptor re-bind |
| TD9-NEW-07 | `crates/nif/src/lib.rs::parse_nif_with_options` | 587 LOC | header parse vs block dispatch vs post-link — 3 phases already labelled |
| TD9-NEW-08 | `crates/nif/src/anim/entry.rs::import_embedded_animations` | 424 LOC | nested local fn definitions suggest extractable helpers |

Note: `draw.rs` (3337 LOC) and `context/mod.rs` (3017 LOC) remain BLOCKED on their split preconditions (RenderDoc capture gate for `draw.rs`); see TD9-200 / TD9-201 for their tracking.

**Fix**: File individual split issues per file once a decompose-first design is agreed. This issue is the complexity-inventory entry; link it from each split issue.

## Completeness Checks
- [ ] **SIBLING**: after any split, verify no circular imports introduced
- [ ] **TESTS**: behavior-preserving splits; all existing tests must pass
- [ ] **CANONICAL-BOUNDARY**: TD9-NEW-05 (parse_esm_with_load_order) — no material-translate impact
- [ ] **UNSAFE**: `recreate_swapchain` contains unsafe Vulkan calls — any split must preserve the safety comments
