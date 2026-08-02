# TD8-003: Dead NIF particle-modifier back-compat shims whose own 'few internal call sites' premise is no longer true

Severity: low
Source audit: docs/audits/AUDIT_TECH_DEBT_2026-08-02.md
GitHub: https://github.com/matiaszanolli/ByroRedux/issues/2268

**Dimension**: 8 (Dead Code & Backwards-Compat Cruft)
**Location**: `crates/nif/src/blocks/particle.rs:322-329` (`parse_color_modifier`), `:607-613` (`parse_simple_color_modifier`)
**Status**: NEW (the code itself predates the current audit window by several weeks; it carries no `#[allow(dead_code)]` because both are `pub fn`, which suppresses rustc's dead-code lint even though nothing calls them — invisible to the standard `allow(dead_code)`-grep discovery method, found here by cross-checking call sites directly)

**Description**: Both functions are explicitly documented as "Back-compat shim — earlier dispatch returned a `NiPSysBlock` for every modifier subtype. Kept so the few internal call sites that only need byte-correct stream advancement still compile." That claim is false today: the block dispatcher (`crates/nif/src/blocks/mod.rs`) calls `NiPSysColorModifier::parse`/`BSPSysSimpleColorModifier::parse` directly, exactly as the shims' own doc comments recommend "new code" do.

**Evidence**:
```rust
/// Back-compat shim — earlier dispatch returned a `NiPSysBlock` for
/// every modifier subtype. Kept so the few internal call sites that
/// only need byte-correct stream advancement still compile, but new
/// code should call [`NiPSysColorModifier::parse`] directly.
pub fn parse_color_modifier(stream: &mut NifStream) -> io::Result<NiPSysBlock> {
    let _modifier = NiPSysColorModifier::parse(stream)?;
    Ok(NiPSysBlock { original_type: "NiPSysColorModifier".to_string() })
}
```
`grep -RIn "parse_color_modifier(\|parse_simple_color_modifier(" crates/nif/src` finds no call sites at all outside the two function definitions.

**Impact**: Cosmetic/maintenance only — dead `pub fn` surface in a parser crate with zero external consumers. Because they're `pub` (not `pub(crate)`), `cargo check`/clippy don't flag them, so this class of rot is invisible to the compiler and will persist indefinitely unless someone greps for call sites directly (as done here).

**Suggested Fix**: Delete both functions. Nothing depends on the `NiPSysBlock`-returning shape for these two block types anymore.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix, if applicable
