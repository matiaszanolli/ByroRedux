# #1840 (NIF-D2-03) + #1841 (NIF-D3-01) — both LOW, both NIF cleanup

## #1840 — delete call-site-less NifVariant trap helpers
Seven `NifVariant` feature predicates had zero production call sites (every
parser reads `stream.bsver()` directly per the #160/#1331 raw-bsver doctrine).
Four were dead outright; three (`has_shader_alpha_refs`/`has_material_crc`/
`has_culling_mode`) were orphaned by MY earlier #1838/#1839 fix this session
(commit 450691e0) — which is exactly the coordination the issue's SIBLING check
asked for. Leaving them alive is a contributor foot-gun: adopting one in a new
parser re-introduces the transitional-export / Unknown-hybrid-header mis-parse
those call sites were fixed to avoid.

Deleted all seven + their five `version.rs` per-feature tests
(`avobject_flags_u32`/`uses_bs_tri_shape` had no tests). Kept
`has_shader_property_fo3_fields` (live consumer in `shader_flags.rs`) — verified
by a full sweep of NifVariant's pub methods (detect/bsver/that one are the only
survivors with call sites). Updated the stale `── Feature flags ──` block
comment that cited the deleted helpers as "blessed", and converted the three
`assert!(!variant.has_*())` sanity lines in my #1838/#1839 regression tests to
comments (the byte-consumption assertions remain the real guards; the
`assert_eq!(variant, Unknown)` corner documentation stays).

Files: version.rs + 4 test files (tri_shape_skin_vertex_tests,
shape_compound_tests, tri_shape_nigeometry_data_version_tests,
dispatch_tests/nodes). nif suite green (865 lib, −5 deleted test fns); no
dead-code warnings.

Out of scope (per the issue's "adopt or ticket"): the `ShaderFlags` typed view
(shader_flags.rs, #1277 Task 6) — a deliberately-added typed API, not a
call-site-less trap; left as-is.

## #1841 — regenerate 5 stale per-block baseline TSVs
The FO3/FNV/FO4/FO76/SkyrimSE baselines (last regen 2026-05-15/16) predated the
NiPSys* typed promotions (#1345 BSPSysSimpleColorModifier + the NiPSysEmitter /
NiPSysEmitterCtlr / NiPSysGrowFadeModifier typing), which moved those keys out
of the opaque NiPSysBlock aggregate. The opt-in per-type gate sat silently RED
(false-positive) on 5 of 7 games.

CRITICAL (no-launder): regenerated then verified each diff is a purely
CONSERVATIVE key-move — NiPSysBlock shrink == sum of the new typed rows, per
game, every row unknown=0, no other type's parsed count touched:
- FNV: −4468 = 1112+1262+1204+890
- FO3: −1484 = 419+422+361+282  (matches the AUDIT_NIF_2026-06-14 note)
- SkyrimSE: −3485 = 1143+1173+1169
- FO4: −3847 = 1344+1345+1158
- FO76: −21812 = 7306+7478+7028
All five archives parse 100% clean, 0 unknown blocks. Oblivion + Starfield TSVs
(already post-#1345) left untouched — only the 5 affected regenerated. All 7
baseline asserts now GREEN; the gate is re-armed.

The issue framed the cause as "#1345-shaped"; the live diff is slightly broader
(the emitter/growfade typings landed too) but the SHAPE is identical and safe:
parsed-count conservation + zero unknown growth.
