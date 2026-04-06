# NIF-402: Cell loader flat import path discards all collision data

## Finding

**Audit**: NIF 2026-04-05b | **Severity**: MEDIUM | **Dimension**: Import Pipeline

**Location**: `crates/nif/src/import/walk.rs:93-147`
**Game Affected**: All games (cell loading path)

### Description

`walk_node_flat()` processes NiNodes for transform accumulation and mesh extraction but never calls `extract_collision()`. The hierarchical path (`walk_node_hierarchical()`, line 41) correctly extracts collision and stores it in `ImportedNode.collision`.

The cell loader at `byroredux/src/cell_loader.rs` uses `import_nif()` (which calls `walk_node_flat`), so all collision data from cell-loaded NIFs is silently dropped.

### Suggested Fix

Either (a) switch the cell loader to use `import_nif_scene()` (hierarchical) and handle the node hierarchy, or (b) add collision output to the flat import path (requires extending the return type).

Option (a) is preferred since the cell loader would also benefit from proper parent/children hierarchy.

### Completeness Checks

- [ ] **SIBLING**: Check if any other callers use import_nif() that need collision
- [ ] **TESTS**: Verify cell-loaded NIF entities have collision components after fix
- [ ] **UNSAFE**: N/A

🤖 Generated with [Claude Code](https://claude.com/claude-code)
