# #1608 — NIF-D5-03/D2-01: Constraint version gate uses file-version not bsver

_Filed from `docs/audits/AUDIT_NIF_2026-06-14.md` via /audit-publish. Immutable snapshot as-filed; GitHub is authoritative for current state._

**Severity**: LOW (no wrong decode on any shipping game or Morrowind; pre-retail FO3 dev band only; architectural fidelity + cross-parser inconsistency) · **Dimension**: Version Gating / Collision Parsing · **Status**: NEW (the PHYSAL constraint decode landed 2026-06-14, commit `0a0bc3ce`)
**Source**: AUDIT_NIF_2026-06-14 (NIF-D5-03 / NIF-D2-01)
**Game Affected**: None shipping. Diverges only for non-shipping FO3 pre-retail dev builds at bsver 14/16, which would decode the FO3+ constraint layout instead of the Oblivion one.

**Location**: [collision/constraints.rs:266](crates/nif/src/blocks/collision/constraints.rs#L266) (and `:538`) — `let is_oblivion = stream.version() <= NifVersion::V20_0_0_5;`.

## Description
nif.xml defines the `#NI_BS_LTE_16#` verset as `(#BSVER# #LTE# 16)` — a **bsver** test. The new decode expresses it as a **NIF-version** comparison (`version() <= V20_0_0_5`). The two are equivalent on all seven shipping games + Morrowind, but diverge for FO3 pre-retail dev builds (bsver 14/16, NIF version > 20.0.0.5): those would wrongly take the FO3+ path. A named [`bsver::NI_BS_LTE_16 = 16`](crates/nif/src/version.rs#L307) constant already exists and is unused; the sibling [`rigid_body.rs`](crates/nif/src/blocks/collision/rigid_body.rs) already gates on bsver. This is a deliberate, documented PHYSAL design choice (spec maps the verset to "NIF ≤ 20.0.0.5"), so it is a fidelity gap, not a bug.

## Evidence
`constraints.rs:266` vs nif.xml `<version name="NI_BS_LTE_16">(#BSVER# #LTE# 16)</version>`; `bsver::NI_BS_LTE_16` defined and unused; `rigid_body.rs` gates on `bsver`.

## Related
PHYSAL spec (`docs/engine/physal.md`); `rigid_body.rs`.

## Suggested Fix
One-token swap to `stream.bsver() <= bsver::NI_BS_LTE_16`, matching nif.xml + `rigid_body.rs`; or, if the NIF-version framing is intentional, add a one-line comment citing the PHYSAL spec so it doesn't read as an oversight.

## Completeness Checks
- [ ] **SIBLING**: Apply the same gate at both `constraints.rs:266` and `:538` (and audit any other `version() <= V20_0_0_5` Oblivion-era gates in the constraint decoders)
- [ ] **TESTS**: A regression test pins the Oblivion-vs-FO3+ branch selection by `bsver`
