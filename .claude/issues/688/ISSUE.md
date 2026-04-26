# Issue #688: O5-3: 154 files truncate at root NiNode 'failed to fill whole buffer' — root-block under-consumption

**Severity**: HIGH
**File**: `crates/nif/src/blocks/node.rs` + ancestors (NiAVObject / NiObjectNET — likely a shared parent field-width issue)
**Dimension**: Real-Data Validation

154 of the 384 truncated files in the 2026-04-25 Oblivion sweep fail with the same shape:

```
Block 0 'NiNode' (offset 0, consumed 24775): failed to fill whole buffer
Block 0 'NiNode' (offset 0, consumed 68834): failed to fill whole buffer
Block 0 'NiNode' (offset 0, consumed 32838): failed to fill whole buffer
```

Root-block under-consumption — the parser advances less than `block_size` and the next read fails. **Largest single bucket in the sweep.**

This is structurally identical to the pre-04-17 "138 files truncate at root NiNode = empty render" finding. The number nominally dropped from 138 to ~150 (different counting), but did NOT clear despite the H-1 parser additions in #394 / #545 / #614. **Suggests there is at least one shared parent of NiNode (NiAVObject? NiObjectNET?) that has a field-width discrepancy on a subset of v20.0.0.5 content.**

Affected files include:
- Menu-only assets (e.g. `meshes\\menus\\hud_brackets\\a_b_c_d_seq.nif`)
- **Critical-path content**: `meshes\\creatures\\horse\\horseheadgrey.nif`, `meshes\\creatures\\imp\\imp.nif`-class siblings, architecture pieces

**Fix**:
1. Add a debug-build assertion at end-of-block-parse that the consumed byte count matches `block_size` for blocks that have one.
2. Capture an offending file's NiNode hex dump via `crates/nif/examples/trace_block.rs`.
3. Bisect against the Gamebryo 2.3 `NiNode::LoadBinary` source path to find the field-width discrepancy.

Pairs with O5-2 (alloc-cap victims). Both findings are subsymptoms of upstream parser drift; closing the drift source(s) closes both.

## Completeness Checks
- [ ] **SIBLING**: same pattern checked in related files (other shader types, other block parsers, other game variants)
- [ ] **TESTS**: regression test added for this specific fix
- [ ] **CROSS-GAME**: if Oblivion-only fix, verify FO3/FNV/Skyrim variants are unaffected
- [ ] **DOC**: ROADMAP.md / CLAUDE.md / audit-oblivion.md updated if they cite the affected behaviour

---
*From [AUDIT_OBLIVION_2026-04-25.md](docs/audits/AUDIT_OBLIVION_2026-04-25.md) (commit 1ebdd0d)*
