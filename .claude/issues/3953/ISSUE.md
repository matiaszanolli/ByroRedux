# #3953 — SCR-ORCH-2026-09-06-01: `docs/feature-matrix.md` and `ROADMAP.md`'s M47.2 row are silent on the ~23k-LOC SKSE/JContainers/StorageUtil/ObScript compatibility layer

- **Finding ID**: SCR-ORCH-2026-09-06-01
- **Labels**: low,scripting,documentation,doc-rot
- **Filed**: 2026-09-06 by /audit-publish from `docs/audits/AUDIT_SCRIPTING_2026-09-06.md`
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/3953

**Source**: `docs/audits/AUDIT_SCRIPTING_2026-09-06.md` — `/audit-scripting` pass 2026-09-06 (seventeenth). Verified against `main` at HEAD on 2026-09-06.

- **Severity**: LOW
- **Dimension**: Scope / doc-rot (orchestrator) · **Untrusted-Input**: No · **Location**: `docs/feature-matrix.md:165-175` (Scripting section), `:308-322` ("What Doesn't Work Yet — live gaps as of 2026-08-19"); `ROADMAP.md:749` (M47.2 row) · **Status**: NEW (#3847, OPEN, covers `_audit-common.md`'s `crates/sdk` row — a different document)
- **Description**: `grep -i "skse\|jcontainers\|storageutil\|extender\|provider"` over both files returns nothing relevant, while `docs/engine/sdk-v0.1-development-plan.md` (1563 lines) and `sdk-v0.1-next-action-plan.md` describe a shipped vertical slice ("ten SKSE `Game` content calls plus the vanilla Papyrus … executable without an extension package"). The scripting-section line numbers the skill pins (174 / 308 / 322) are otherwise **still exact** — no other doc-rot in that file this cycle.
- **Suggested Fix**: one row in the Scripting section + one sentence in the M47.2 row pointing at the SDK plan, marked as "vertical slice, unaudited" until the dedicated pass lands.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (the other decompiler passes / the other fragment producers / the sibling recognizer)
- [ ] **LOCK_ORDER**: If a RwLock/guard scope changes, the canonical order in `docs/engine/ecs.md` is preserved and `BYRO_LOCK_ORDER_CHECK=1` stays green
- [ ] **TESTS**: A regression test pins this specific fix
