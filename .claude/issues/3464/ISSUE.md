# #3464 — NIF-2026-08-27-D1-01: `BSFaceGenNiNode` under-reads 2 bytes on 100% of Starfield facegen head nodes

Source: `docs/audits/AUDIT_NIF_2026-08-27.md`
Filed: 2026-08-27 via `/audit-publish`
Labels: medium, nif-parser, nif, bug, game:starfield

---

Audit: `docs/audits/AUDIT_NIF_2026-08-27.md` — Dimension 1 (Stream Position Integrity). Severity **MEDIUM** (per `_audit-severity.md`: "NIF parse mismatch (stream position off)"). Game: **Starfield**, `bsver = 175` (the `SF_WEAK_REF_GAP` band).

## Location
`crates/nif/src/blocks/mod.rs:314-325` — `BSFaceGenNiNode` is aliased into the plain-`NiNode` arm (re-verified at publish time: the alias sits at `mod.rs:325`, its comment at `:314`).

## Description
Every `BSFaceGenNiNode` block in shipped Starfield content consumes exactly 2 bytes fewer than its declared `block_size`. The block is dispatched to `NiNode::parse` as a "coverage-first stub", and the dispatch comment cites its own corpus count as evidence the alias works:

```rust
// BSFaceGenNiNode (Starfield, 1,282 / 1,282 in `FaceMeshes.ba2`,
// #727) is aliased here as a coverage-first stub: the wire
// layout is unconfirmed and nif.xml has no SF schema for it.
```

Those same 1,282 blocks are 1,282 two-byte under-reads. The comment reads as a coverage claim; the measurement says the alias is 2 bytes short on every single one.

## Evidence
`NifScene::drift_histogram` over `Starfield - FaceMeshes.ba2` + `ShatteredSpace - Main01/02.ba2`:

```
-- BSFaceGenNiNode blocks present, by bsver --
  bsver=175	blocks=1417
-- drift, by bsver --
  bsver=175	drift=+2	count=1417
```

100% (1,417 / 1,417). A representative file (`meshes\actors\character\facegendata\facegeom\starfield.esm\000124aa.nif`, `version=20.2.0.7 bsver=175`) declares `block 0 BSFaceGenNiNode size=122` against 120 consumed.

**The 2 bytes are `BSFaceGenNiNode`-specific, not the `NiNode` base and not the #2105 `SF_WEAK_REF_GAP` field.** Discriminating measurement over every `bsver == 175` file in `MeshesPatch.ba2` + `Meshes01.ba2` + `ShatteredSpace - Main01.ba2`:

```
-- node blocks present in bsver-175 files --
  NiNode              58128
  BSWeakReferenceNode  9440
  BSFaceGenNiNode        135
-- drift in bsver-175 files --
  BSFaceGenNiNode  drift=+2  135
```

58,128 plain `NiNode` blocks in the same band drift by zero; 9,440 `BSWeakReferenceNode` blocks drift by zero (their own gap is already handled at `crates/nif/src/blocks/node.rs:936`). Only `BSFaceGenNiNode` drifts.

## Impact
`block_size` reconciliation realigns the stream, so nothing cascades and Starfield facegen heads still load. What is lost is whatever those 2 bytes carry on every Starfield NPC head node, and — more operationally — the dispatch comment's coverage claim is misleading to the next reader.

## Needs research
nif.xml has **no Starfield `BSFaceGenNiNode` schema** (the type is not present in `/mnt/data/src/reference/nifxml/nif.xml` at all), and the Gamebryo 2.3 source predates it. The *semantics* of the 2 bytes cannot be settled from either authority and must be reverse-engineered from the bytes.

## Related
Closed #727 (the alias itself — the residual drift it leaves has never been filed); #2105 / #2201 (`SF_WEAK_REF_GAP = 175`, `crates/nif/src/version.rs:476` — same width, same bsver band, different block); #1606 (the `read_starfield_tail` opaque-capture precedent).

## Suggested Fix
Capture the 2 bytes opaquely with the #1606 / `BsWeakReferenceNode::parse_with_size` idiom (a dedicated `BsFaceGenNiNode { base: NiNode, starfield_tail: Vec<u8> }` consumed to `block_size`), which removes the drift without fabricating field semantics. Update the dispatch comment so "1,282 / 1,282" no longer reads as a correctness claim.

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files (`BsWeakReferenceNode::parse_with_size`, the other opaque-tail capture sites, `dispatch_tests/nodes.rs:531-541`)
- [ ] **TESTS**: A regression test pins this specific fix
