# NIF-516: NiCollisionObject (base class) skip-only — Oblivion cascading failure

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: MEDIUM | **Dimension**: Coverage

**Location**: `crates/nif/src/blocks/mod.rs` (skip-only entry)
**Game Affected**: Oblivion

### Description

NiCollisionObject (the base class, not the bhk variant) occasionally appears in Oblivion NIFs. Since it's skip-only and Oblivion has no block_sizes, it causes the same cascading parse failure as NIF-515.

NiCollisionObject is a simple block (just target ref + body ref), so a minimal parser is straightforward.

### Suggested Fix

```rust
// NiCollisionObject: target(BlockRef) + collision_handler(u16) + body(BlockRef)
```

### Completeness Checks

- [ ] **TESTS**: Synthetic Oblivion NIF with NiCollisionObject block
- [ ] **UNSAFE**: N/A

🤖 Generated with [Claude Code](https://claude.com/claude-code)
