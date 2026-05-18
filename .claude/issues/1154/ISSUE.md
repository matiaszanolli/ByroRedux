# TD8-023: dead re-export alias TreeObjectBounds (zero callers)

## Source Audit
`docs/audits/AUDIT_TECH_DEBT_2026-05-17.md` — Dimension 8 (Backwards-Compat Cruft)

## Severity
**LOW** — pure cruft per CLAUDE.md global rule: "ByroRedux has no external consumers — delete instead of rename."

## Location
`crates/plugin/src/esm/records/mod.rs:66`

## Description
```rust
pub use tree::{parse_tree, ObjectBounds as TreeObjectBounds, TreeRecord};
```

The `TreeObjectBounds` alias is exported but never used anywhere in the workspace. It's a backwards-compat alias for a hypothetical external consumer that does not exist.

## Verification
```
$ grep -rn 'TreeObjectBounds' --include="*.rs"
crates/plugin/src/esm/records/mod.rs:66:pub use tree::{parse_tree, ObjectBounds as TreeObjectBounds, TreeRecord};
# zero other callers
```

## Proposed Fix
Change the re-export from:
```rust
pub use tree::{parse_tree, ObjectBounds as TreeObjectBounds, TreeRecord};
```
to:
```rust
pub use tree::{parse_tree, TreeRecord};
```
If any future external consumer needs `ObjectBounds` from the tree submodule, they can import it directly as `tree::ObjectBounds`.

## Completeness Checks
- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Sweep other re-export aliases in `records/mod.rs` for the same pattern
- [ ] **DROP**: N/A
- [ ] **TESTS**: cargo check should pass; no test breakage expected
