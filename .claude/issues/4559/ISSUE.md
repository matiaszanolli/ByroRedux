# NIFAL-D4-2026-09-21-02: spawn_nif_nodes SceneFlags comment claims unconditional emission; the code has gated it on flags != 0 since the comment's own commit #222

**Labels**: low, nifal, documentation, doc-rot

**Severity**: LOW · **Dimension**: Nodes · **Tier Violated**: — (comment/code mismatch at a node spawn site) · **Game Affected**: all (any all-zero-flags node, i.e. most nodes)
**Location**: `byroredux/src/scene/nif_loader.rs:1560-1570` — comment "unconditionally (not gated on `flags != 0`)" immediately above `if node.flags != 0 { … }`
**Source**: `docs/audits/AUDIT_NIFAL_2026-09-21.md` (CONFIRMED at `4dca737e2`)

### Description
A node with zero flags gets no `SceneFlags` row at all, so the "future toggle-visible system can just flip the bit on the existing component" contract the comment offers does not hold for those entities. Contradiction present since the comment's introducing commit `f0a0ec1f6` (2026-04-20), which added the comment and the gated `if` together. Production readers today: only the debug console listing (re-verified this sweep).

### Evidence
`git show f0a0ec1f6` shows both added together; `SceneFlags::from_nif(0)` is `Default` (visible).

### Impact
None today; latent for a future visibility-toggle system written to the comment's contract (no row on zero-flag nodes → silent no-op).

### Related
#222, #1235

### Suggested Fix
Prefer making the code match the comment (drop the gate — `from_nif(0)` is Default/visible, sparse-stored, one line), else fix the comment to state the gate.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other readers, other gates)
- [ ] **TESTS**: A regression test pins this specific fix
