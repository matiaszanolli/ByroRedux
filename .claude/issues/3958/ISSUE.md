# LC-2026-09-06-D2-01: nifal.md's Skinning leak inventory has no entry for the #3549 external-skeleton slice, so the layer spec no longer describes its own largest skinning change

Issue: #3958 · Filed from `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-06.md` (base `a8233f2f`)
Labels: low, legacy-compat, nifal, documentation, doc-rot, game:starfield

Reported by `/audit-legacy-compat` — `docs/audits/AUDIT_LEGACY_COMPAT_2026-09-06.md` (base `a8233f2f`).

- **Severity**: LOW (documentation drifted from code)
- **Dimension**: 2 — NIFAL mapping shape
- **Location**: `docs/engine/nifal.md:140-183` (§2, "Skinning — **half-stale (2026-08-07): loose-NIF path only**"), vs `crates/nif/src/import/mesh/skeleton.rs` and `crates/nif/src/import/mesh/skin.rs:405-425`
- **Status**: NEW

## Description

`nifal.md` §2 is the leak inventory that `/audit-legacy-compat` Dimension 2 instructs every auditor to cross-check against "so audits stop re-filing" closed leaks.

Its Skinning entry describes exactly one thing: the #2440 cell-loader gap (cell-placed skinned REFRs get no palette binding). That entry is still **accurate** — re-verified at `a8233f2f`: `SkinnedMesh::new_with_global` has exactly one production caller (`byroredux/src/scene/nif_loader.rs:1264`) and the cell loader has no `node_by_name` map.

But the entry is no longer **complete**. #3549 added a whole sub-slice to the skinning category — external-skeleton bone-name recovery for a game where 73% of authored bone refs are NULL — and §2 does not mention it, name `skeleton.rs`, or record its decline contract.

An auditor reading §2 to decide whether Starfield skinning is converged, gapped, or parked gets no answer. The module's own header is currently the only place the census and the zero-wrong validation live.

## Evidence

- `docs/engine/nifal.md:140` heading is dated `2026-08-07`; `crates/nif/src/import/mesh/skeleton.rs` was created after it, and `git log --since=2026-08-30 -- docs/engine/nifal.md` shows the file was not touched in the 497-commit window.
- Grep for `skeleton.rs`, `3549`, `resolve_external_bone_names` or `BSSkin` in `docs/engine/nifal.md` → **0 hits**.
- The slice being omitted is substantial (18 KB, `crates/nif/src/import/mesh/skeleton.rs`, `resolve_external_bone_names` at `:280`), and carries measurements that exist nowhere else in the docs:
  - Census: 5,896 `BSSkin::Instance` blocks over 68,459 vanilla NIFs, 107,717 bone refs, **78,587 (73%) NULL**; 3,738 skins (63%) fully NULL.
  - Validation against 346 in-file skins with known ground truth: unique offset solved on 239 (69%); of 9,057 recovered names, **8,708 exact, 349 position-coincident, ZERO wrong**.
  - The contract — it resolves correctly or declines, and declining restores the prior behaviour.

## Impact

Documentation only. The risk is the one §2 exists to prevent: a future sweep re-deriving Starfield skinning status from scratch, or filing "Starfield NPCs render in bind pose" against code that fixed it.

Related open issue #3930 proposes a better input for the same solve (`SkinAttach` carries the authored names for 100% of the skins #3549 solves geometrically) — a reader of §2 alone would not connect the two.

## Related

- #3549 — the slice itself
- #3930 — `SkinAttach` as a better input for the same solve (OPEN)
- #2440 — the cell-loader gap §2 *does* record, re-verified still true
- #2441 — the `Option` residual note directly below it in §2

## Suggested Fix

Add a Starfield sub-entry under §2 Skinning naming `crates/nif/src/import/mesh/skeleton.rs`, its decline-or-be-right contract, the measured accuracy, and a pointer to #3930 as the open follow-up. Re-date the section heading from `2026-08-07`.

## Completeness Checks
- [ ] **SIBLING**: Check whether §2's other category entries have also fallen behind their code (Materials / Geometry / Lights / Animation / Shader-flags were spot-verified as still accurate in this sweep; Collision and Particles were not re-walked field by field)
- [ ] **TESTS**: n/a — documentation
