# TD8-002: Unused global_target fixup accessor in new hkx crate, no test coverage at all

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2267

**Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
**Location**: `crates/hkx/src/packfile.rs:185-193`
**Status**: NEW

**Description**: `crates/hkx` (`byroredux-hkx`) is a brand-new crate, added 2026-08-01 in commit `02c24e4f` ("feat(hkx): add minimal safe Havok packfile reader for Bethesda animation assets"). Its `Packfile` struct exposes both `local_target` (resolves same-section virtual fixups; used 12 times across `animation.rs`) and `global_target` (resolves cross-section fixups), but only `local_target` is ever called. `global_target` has zero call sites anywhere in the crate, and `packfile.rs` has no test module at all (only `animation.rs` does), so unlike the `quest.rs` `AliasFlags` precedent from a prior audit cycle, this isn't test-exercised scaffolding.

**Evidence**:
```rust
#[allow(dead_code)]
pub(crate) fn global_target(&self, source: usize) -> Option<(usize, usize)> {
    self.global_fixups
        .binary_search_by_key(&source, |entry| entry.0)
        .ok()
        .map(|index| {
            let (_, section, target) = self.global_fixups[index];
            (section, target)
        })
}
```
`grep -RIn "global_target" --include="*.rs" crates/hkx` → only the definition itself.

**Impact**: Minor — `pub(crate)`, already scoped correctly, not part of any external API surface. The risk is purely silent rot (the accessor reading `global_fixups` back is never exercised, though the parsing that populates `global_fixups` is exercised implicitly). Given the crate is one day old and the only two object types currently handled (skeleton + spline-compressed animation) apparently never need a cross-section reference, this reads as symmetric API left in "just in case" rather than something with a concrete near-term consumer.

**Suggested Fix**: Either delete `global_target` until a Havok object type that actually needs a cross-section fixup is decoded, or add a one-line comment naming the next object type that will need it plus a placeholder test — matching the `quest.rs` precedent this project already treats as legitimate scaffolding. As shipped today, with no comment and no test, it doesn't meet that bar.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
