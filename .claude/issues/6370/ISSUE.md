# TD2-2026-10-25-11: six `mod tests {` cuts in renderer/bin source scans bypass `source_scan` and the weak-needle guard cannot see them

Labels: low, renderer, tech-debt, bug
Issue: https://github.com/matiaszanolli/ByroRedux/issues/6370
(Immutable snapshot of the issue as filed; GitHub is authoritative for state.)

- **Dimension**: 2
- **Location**: `crates/renderer/src/vulkan/context/resize.rs:1802` (`split("\nmod tests {")`), `vulkan/skin_compute.rs:1672,2199`, `vulkan/context/resources.rs:1178,1266`, `byroredux/src/loading_screen.rs:1077` (all `find("mod tests {")`; oldest 2026-05-23, newest 2026-10-03).
- **Effort**: trivial
- **Fix**: use `crate::source_scan::production_text` (renderer re-exports it, `crates/renderer/src/source_scan.rs:9`) or `byroredux_core::source_scan::production_text_or_whole` (bin). Add `"find(\"mod tests {\")"`, `"split(\"\\nmod tests {\")"` to the `weak` list at `byroredux/src/workspace_hygiene_tests.rs:~717`. (All else in the guard's allowlist is intact; no new weak `#[cfg(test)]` cut was added this window.)

---
Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-25.md` (HEAD c7463c04d), finding `TD2-2026-10-25-11`. Fix with: `/fix-issue <this issue>`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / crates
- [ ] **DROP**: If Vulkan objects change, the Drop impl is still reverse-order correct (no barrier/order change without RenderDoc or `BYRO_VALIDATION=1` — feedback_speculative_vulkan_fixes)
- [ ] **TESTS**: A regression test or source-scan guard pins this specific fix (or the finding is doc-only and `_audit-validate.sh` / `cargo test` stays green)
