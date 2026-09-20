# REN-D5-2026-09-20-03: HUD 3-buffer overlay rotation missing from the #3643/#870 frames-in-flight bump tripwire

- **ID**: REN-D5-2026-09-20-03
- **Labels**: medium,renderer,memory,test-gap,bug
- **Filed from**: docs/audits/AUDIT_RENDERER_2026-09-20.md

**Severity**: MEDIUM · **Dimension**: Memory/Lifecycle
**Source**: docs/audits/AUDIT_RENDERER_2026-09-20.md (REN-D5-2026-09-20-03)

**Location**: `frames_in_flight_contract_names_every_dependent_resource` enumeration vs `byroredux/src/hud.rs` 3-slot rotation (`upload_frame`, `(current+1)%3`)

**Description**
The MenuXml HUD's new 3-buffer rotation (dc306a6a0) is safe only under the both-slots fence wait (one slot of margin at MAX_FRAMES_IN_FLIGHT == 2). The tripwire test that exists precisely to catch FIF-dependent resources when MAX_FRAMES_IN_FLIGHT changes does not enumerate the rotation — a future FIF ≥ 3 silently becomes a WAR use-after-free.

**Evidence**
The rotation's safety argument is documented in hud.rs and audited sound at FIF=2; the tripwire's enumeration predates the rotation.

**Impact**
Latent correctness cliff gated on a constant bump that the test was built to make loud.

**Suggested Fix**
Add the rotation (and its slot count) to the tripwire's enumeration.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
