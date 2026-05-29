# #1313 -- OBL-D4-NEW-03: Dead legacy-particle match arms + misleading comments

_Snapshot as filed 2026-05-28 from /audit-publish (AUDIT_OBLIVION_2026-05-28)._

**Severity**: LOW | **Dim 4** — Rendering Path for Oblivion Shaders
**Source**: `docs/audits/AUDIT_OBLIVION_2026-05-28.md` (OBL-D4-NEW-03)

**Location**: `crates/nif/src/import/walk/mod.rs:520-543` and `:1182-1204` (match on `NiPSysBlock.original_type`); `crates/nif/src/blocks/mod.rs:308-321` (comment "Oblivion magic FX"); `crates/nif/src/blocks/legacy_particle.rs:1-12` (module doc)

**Issue**: Dead `NiPSysBlock` legacy-type match arms in the import walk + module comments claim these arms serve Oblivion magic-FX content. Real Oblivion content uses modern `NiParticleSystem`, not the legacy stack — the arms are unreachable on Oblivion and the comments are misleading.

**Suggested fix**: either delete the unreachable `NiPSysBlock` legacy match arms (walk/mod.rs:524-543, 1186-1203) or, if legacy particle support is intended for pre-Gamebryo content (Morrowind/NetImmerse), redirect to downcast the actual typed structs. Update the `legacy_particle.rs` module doc and the `blocks/mod.rs:308-321` comment to accurately describe what content they serve.

## Completeness Checks
- [ ] **SIBLING**: audit FO3/FNV import walk for the same dead arms
- [ ] **TESTS**: no behavior change; add a comment-accuracy note in any existing legacy-particle test
- [ ] **CANONICAL-BOUNDARY**: import-walk only; no material/translate impact
- [ ] **UNSAFE**: no unsafe involved
