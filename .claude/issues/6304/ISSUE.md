# #6304: NIF-D5-2026-10-21-03: Doc rot from the #5015 / #6066 / #6067 ragdoll wave

**Labels**: low,documentation,doc-rot,nif-parser,physics,game:fo4

**Source**: docs/audits/AUDIT_NIF_2026-10-21.md · **Severity**: LOW · **Dimension**: Collision/Shader Parsing (docs) · **Location**: `docs/engine/physal.md` (roadmap item 6); `crates/nif/src/blocks/collision/np_ragdoll.rs` (`NpRagdollBody::shape` doc); `docs/engine/nif-parser.md` § Open items

- **Game Affected**: FO4 (plus all titles for the `nif-parser.md` constraint entry)
- **Location**: `docs/engine/physal.md:458-463`, `crates/nif/src/blocks/collision/np_ragdoll.rs:188-189`, `docs/engine/nif-parser.md:929-936` ("Open items")
- **Status**: NEW
- **Description**: Three statements are now false:
  1. `physal.md` roadmap item 6 still lists "the undecoded material/motion rows" as remaining work. #6066 (`019b087e3`) decodes them, and the same file documents that decode at `:419-430`.
  2. `NpRagdollBody::shape`'s doc says it is "`None` for a shape class this decoder does not read (compounds)". #6067 (`fc4a15104`) decodes `hknpDynamicCompoundShape`, and the corpus gate reports 0 shape-less bodies.
  3. `nif-parser.md` § "Open items" says Havok constraint data "has no consumer until physics wiring lands" (#331, closed) and that `BhkNPCollisionObject` blobs "need a separate decoder". The classic constraints have fed `extract_ragdoll` and the canonical joints for many releases, and FO4 skeleton blobs are decoded.
- **Evidence**: Each line quoted above, set against `np_ragdoll.rs:418-436` (`read_properties`), `np_ragdoll.rs:472-495` (`read_compound`) and `np_ragdoll_corpus` ("6 compounds, 0 without a shape"; "986 with readable rows").
- **Impact**: A reader takes rows that are already decoded, and constraints that are already consumed, as open work.
- **Related**: #6066, #6067, #5015, #331. NIF-D5-2026-10-21-01 covers the separate FO76/Starfield container claim.
- **Suggested Fix**: Drop "the undecoded material/motion rows" from item 6. Change the field doc to "`None` for a shape class this decoder does not read; every vanilla body decodes one (#6067)". Rewrite the two "Open items" bullets to point at `extract_ragdoll` / `decode_np_ragdoll` and keep only `hknpCompressedMeshShapeData` and phantoms open.

**Validation note**: `docs/engine/nif-parser.md` also carries the same "has no consumer until physics wiring lands" claim at a second site (around line 503, outside § Open items); fix both. Siblings filed separately in this suite: TD3-2026-10-21-01 (`docs/feature-matrix.md` / `ROADMAP.md`) and PHYS-D3-2026-10-21-04 (`combat.rs` comment).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
