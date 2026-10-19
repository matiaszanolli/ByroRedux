# NIFAL-SD-01: Restoring a stripped NPC body part re-imports it without the spawn-time hooks, so an Oblivion body loses its race skin

Labels: medium,nifal,gameplay,game:oblivion,game:fo3,game:fnv,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/6128

**Severity**: MEDIUM · **Dimension**: Nodes / NPC part assembly (spawn-time translation hooks) · **Tier violated**: `single-boundary` · **Games**: Oblivion (race skin texture); FO3/FNV (hand seam blend)

## Location
`byroredux/src/npc_spawn/loot_appearance.rs` (restore import via `load_nif_bytes_with_skeleton(.., part.tint.as_deref(), None)`; `RestorePart { path, tint }`), `byroredux/src/npc_spawn/resumable/runtime.rs` (parts registered for restore as `RestorePart::body(path)`; the `pre_spawn` hooks the restore skips)

## Description
The NPC part appearance is not a function of the part path. It depends on per-call-site `pre_spawn` closures: the Oblivion race body-section `ICON` base-texture override (#5487, `body_textures`), and the FO3/FNV hand seam blend. The initial spawn applies them. A body piece covered by worn armour is skipped at spawn and registered as `RestorePart::body(path)`. When the outfit is removed (corpse take-all, `equipment_appearance_system`), the part is re-imported by `load_nif_bytes_with_skeleton` in `loot_appearance.rs` with `pre_spawn = None`. `RestorePart` carries only `path` and `tint`.

## Evidence
`humanoid_body_path_icon_index` returns `Some` only for Oblivion (`npc_spawn.rs`), and its doc says the bodies "enter with the race's body-section ICON as their spawn-time base texture" because the NIF default is Imperial. `RestorePart` has no texture field and `loot_appearance.rs` has no `body_textures`/`race_body_texture` reference (grep at publish time), so the restored body gets the NIF's own Imperial skin while the head keeps the race head texture. Static reading only; the end-to-end take-all restore was not run.

## Impact
A non-Imperial Oblivion NPC (Dunmer, Orc, ...) whose body armour is taken shows an Imperial-toned torso, legs or hands under a race-toned head - the exact mismatch #5487 fixed at spawn. On FO3/FNV the restored hands lose the wrist-cut normal/tone blend (minor). No crash and no data loss. Reachable on any corpse with covered body parts.

## Related
#5487 (closed; fixed the spawn path only), #5794 (shared finalize tail; its dedup did not cover the restore re-import), seam_blend. Note: FO3-SD-04 (#6090) is a separate defect (RACE body-section ICON skin never applied on FO3 at spawn).

## Suggested Fix
Make the part's appearance a value, not a closure: store the resolved `body_texture` on `RestorePart` and apply it through the same imported-tier override helper the spawn uses (extract that closure body into one function both sites call). Add a test that an Oblivion restore carries the race texture.

Source: docs/audits/AUDIT_NIFAL_2026-10-19.md (area-scoped, streaming-deep)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **CANONICAL-BOUNDARY**: Per-game logic stays at the NIFAL parser->`Material` boundary - never pushed into shaders/renderer, never re-derived at render time. See `/audit-nifal`.
- [ ] **TESTS**: A regression test pins this specific fix
