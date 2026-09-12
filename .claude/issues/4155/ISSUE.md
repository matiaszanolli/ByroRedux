# NIF-D5-2026-09-11-01: HavokPackfile::content_range/absolute_end add two u32 fields before widening — overflow panics in non-release builds

URL: https://github.com/matiaszanolli/ByroRedux/issues/4155
Labels: bug, nif-parser, medium, nif, game:fo4, game:fo76

---

**Severity**: MEDIUM
**Dimension**: 5 — Collision/Shader Parsing
**Game Affected**: Fallout 4, Fallout 76 (the `bhkPhysicsSystem`/`bhkRagdollSystem` → `BhkSystemBinary` path)
**Location**: `crates/nif/src/blocks/collision/havok_packfile.rs:133-148`
**Status**: NEW

**Description**: Unlike every other offset computation in `parse_havok_packfile` (which correctly promotes to `usize` before adding), `PackfileSection::content_range()`/`absolute_end()` add two raw `u32` header fields before casting. A section header whose fields sum past `u32::MAX` triggers Rust's default debug-build overflow panic — live in `cargo test`/`cargo check`/plain `cargo run` (no `overflow-checks` override in `Cargo.toml`).

**Evidence** (`havok_packfile.rs:133-148`):
```rust
pub fn absolute_end(&self) -> u32 {
    self.absolute_data_start + self.end_offset
}
pub fn content_range(&self) -> Range<usize> {
    self.absolute_data_start as usize
        ..(self.absolute_data_start + self.local_fixups_offset) as usize
}
```
Both add the raw `u32` fields before any cast to `usize`.

**Impact**: Currently unreachable — `parse_havok_packfile` has no live caller yet (added by `#3809` as infrastructure for future physics-bridge work). Once wired to a real loader consuming untrusted/moddable `BhkSystemBinary` blobs, a single malformed section header aborts the whole engine process in any non-release build.

**Suggested Fix**: Widen to `usize` before adding, matching the rest of the file's pattern. Add a unit test with a section header summing past `u32::MAX`.

## Completeness Checks
- [ ] **SIBLING**: Check the rest of `parse_havok_packfile` for any other raw-`u32`-before-cast addition
- [ ] **TESTS**: A unit test with header fields summing past `u32::MAX` pins the fix

