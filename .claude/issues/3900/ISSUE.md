# #3900: SF-2026-09-05-D8-01: slot_to_role puts Starfield on Skyrim's slot vocabulary while canonical_shader_type puts it on FO76's — one file, two answers

*Filed 2026-09-05 by `/audit-publish` from the `texture-roles-deep` audit suite. Immutable snapshot as filed — GitHub is authoritative for current state (`gh issue view 3900 --json state`).*

---

**Audit**: `docs/audits/AUDIT_STARFIELD_2026-09-05.md` (suite preset `texture-roles-deep`)
**Severity**: MEDIUM · **Dimension**: 8 (shader flags / texture roles)

## Description

`crates/nif/src/import/material/slot_role.rs` gives **two different answers** about which slot vocabulary Starfield uses:

- `canonical_shader_type` groups Starfield with **FO76** — correct on the parser-boundary argument, since both go through `parse_fo76_plus`.
- `slot_to_role` groups Starfield with **Skyrim** on slots 2, 3, 6 and 7.

No arm cites any Starfield evidence for either grouping.

## Evidence

The FO76 readings that the `slot_to_role` arm implicitly rejects are the **measured** ones from the FO76 corpus work: slot 3 is a greyscale LUT (not POM height), and slot 6 is specular — 1,616 of 1,664 occupants are `_s.dds`.

So the file's two halves disagree, and the half that disagrees with the parser boundary is also the half contradicting the measured data.

**Inert today**: a census this audit found zero occupancy on the disputed Starfield slots, so nothing currently mis-binds. The finding is the latent mixed vocabulary, not present breakage.

## Impact

A single canonical translation boundary holding two rival slot vocabularies for the same game is the exact condition the role unification exists to prevent. When Starfield content that occupies slots 2/3/6/7 does appear — or when a mod authors it — the binding will be wrong, silently, and the wrongness will be attributed to the newer CDB path rather than to this table.

## Suggested Fix

Pick one vocabulary for Starfield and cite the evidence in the code. The parser-boundary argument (`parse_fo76_plus`) and the measured FO76 slot occupancy both point the same way: group Starfield with FO76 in `slot_to_role` as well, matching `canonical_shader_type`. If the Skyrim grouping is deliberate, it needs a comment saying what Starfield data supports it — currently nothing does.

Per the project's no-guessing rule, this should be settled from a Starfield corpus census rather than by argument, and the census recipe already exists from the FO76 work.

## Completeness Checks
- [ ] **SIBLING**: `canonical_shader_type` and `slot_to_role` agree for every game after the change, not just Starfield
- [ ] **CANONICAL-BOUNDARY**: The decision stays at the NIFAL parser→`Material` boundary. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins Starfield's slot→role mapping against the chosen vocabulary

## Related
- #3796 (CLOSED — settled the doc contradiction, not the code one)
- #2695 (the precedent: one shared slot table, because two disagreeing tables changed shading semantics)

---
🤖 Filed by `/audit-publish` from the `texture-roles-deep` audit suite.
