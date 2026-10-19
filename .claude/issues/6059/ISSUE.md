# FNV-2026-10-18-10: `AddSpell … dropped — the actor has no SpellList` is logged at WARN 50–300 times per second by an authored per-frame script on a far persistent-cell actor

- **Issue**: #6059 (https://github.com/matiaszanolli/ByroRedux/issues/6059)
- **Labels**: low, scripting, bug, game:fnv, legacy-compat
- **Source**: docs/audits/AUDIT_FNV_2026-10-18.md (snapshot as filed; GitHub is authoritative for state)

Found by the FNV compatibility audit of 2026-10-18 (`docs/audits/AUDIT_FNV_2026-10-18.md`, HEAD `af5063597`).

- **Origin**: D1-05. **Status**: NEW (closed #4819 / #5071 are about SpellList parking and lock inventory).
- **Location**: `crates/scripting/src/magic.rs:192` (the #4822 warn), driven by `obscript_effects.rs:638` from `obscript_objects::attach_object_script`, attached in `byroredux/src/cell_loader/references/attach.rs`.
- **Evidence**: 754 lines in the first 2 s and 1,695 in 2 min (radius 12); 1,261 in 25 s (radius 3). Entity 6278 (REFR `0x000B9A2D`, no mesh, no `SpellList`, ~85 k units from the player) runs `VNovacChupacabraNightkinSCRIPT`, whose `GameMode` block authors `addspell StealthBoyInvisibilitySpell` every frame. `obscript.objects` shows 676 persistent-cell references running object scripts every frame regardless of distance.
- **Impact**: log and disk flood on any WastelandNV session. It also raises a fidelity question for `/audit-scripting`: object-script `GameMode` runs for persistent refs far outside the loaded grid, while the real game only runs it for 3D-loaded refs.
- **Suggested Fix**: log the dropped `AddSpell` once per (actor, spell); decide the residency gate for persistent-cell object scripts.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the other FO3 / Oblivion / Skyrim paths that share this code
- [ ] **TESTS**: A regression test (or a repaired gate) pins this specific fix

