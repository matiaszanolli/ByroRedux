# #3740: SPT-2026-08-30-D4-01: BNAM ships on 142/142 vanilla Oblivion TREE records, so the MODB tier added by #1001 to size Cyrodiil trees is reached by 0 records — four comments state the opposite

**Labels**: bug, medium, legacy-compat, game:oblivion, terrain-exterior, speedtree, doc-rot
**Filed**: 2026-08-30 (audit-publish)

---

**Report**: `docs/audits/AUDIT_SPEEDTREE_2026-08-30.md` · **Severity**: MEDIUM · **Dimension**: 4 (Per-Game Variants)
**Game affected**: Oblivion

## Location
- `crates/spt/src/import/mod.rs` — `compute_billboard_size` and the `bound_radius` / `billboard_size` field docs
- `byroredux/src/cell_loader/references/import.rs`
- `crates/plugin/src/esm/records/tree.rs` — the module doc and the `billboard_size` field doc

## Description
`compute_billboard_size`'s precedence is **OBND → BNAM → MODB → default**, and four separate comments justify the MODB tier as *the Oblivion path* on the grounds that Oblivion ships MODB and no OBND.

The OBND half is true; **the conclusion does not follow**, because BNAM is also present on every vanilla Oblivion TREE record and sits above MODB. The MODB tier is therefore dead on the only game it exists for.

Two of the four comments state the premise as fact and are contradicted by the corpus:
- `crates/spt/src/import/mod.rs` calls BNAM "(FO3/FNV only)"
- `crates/plugin/src/esm/records/tree.rs` says "`None` on Oblivion (BNAM absent there)"

## Evidence
Field-presence census over the three plugins, `.spt`-bearing TREE records only:

| Game | records | OBND | MODB | BNAM | ICON | SNAM | CNAM | tier `compute_billboard_size` actually takes |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| FNV | 3 | 100 % | 0 % | **100 %** | 100 % | 100 % | 100 % | OBND ×3 |
| FO3 | 9 | 100 % | 0 % | **100 %** | 100 % | 100 % | 100 % | OBND ×9 |
| Oblivion | 142 | 0 % | 100 % | **100 %** | 96 % | 99 % | 100 % | **BNAM ×142** |

All 142 Oblivion BNAM payloads are the expected 8 bytes / 2 × f32, so they decode and win cleanly. **The MODB tier is reached by 0 records in any game.**

Measured consequence over all 142 Oblivion records — BNAM-chosen height against the height the MODB tier would have produced:

- height ratio: min **0.36**, median **0.41**, max **0.41**
- 136 of 142 BNAMs are square (h/w median **1.000**) against the intended 1:2 silhouette; only 6 are non-square
- BNAM width / MODB radius: median **0.828** — width is broadly sane, the divergence is almost entirely vertical
- e.g. `Mbush16` BNAM `270×270` vs MODB-derived `326×652`; `Dbush15` `300×300` vs `362×724`

## Impact
Every Cyrodiil tree and shrub placeholder renders at roughly **41 % of the height #1001 intended**, and square rather than tall. Latent behind the D3-01 MODL-resolution defect today (nothing renders at all), so this becomes visible **the moment D3-01 is fixed** — and would then read as a *new* regression introduced by that fix.

Also a live doc-rot hazard: four comments across two crates describe a precedence the code does not execute, and a future editor reading them will reason from the wrong corpus.

## Related
#1001, #1002, #3080 (the docstring repair that restated the same premise); the D3-01 MODL-resolution issue filed alongside this one; the CNAM 5-vs-8 finding (same record, same class of corpus-premise error).

## Suggested Fix — two separable pieces

**(1) Documentation, unconditional.** Correct the four comments to state the measured presence table above. The claim "BNAM absent on Oblivion" is simply false.

**(2) Behaviour — needs research first.** Whether Oblivion should size from BNAM or MODB is a format question **this audit deliberately does not answer**. BNAM is plausibly the authored imposter-card dimension, which is precisely what the placeholder *is*, while `(R, 2R)` from a bounding-sphere radius is an admitted heuristic. **Settle it against an attested TREE/BNAM definition or a screenshot comparison before reordering the tiers; do not reorder on this report alone.**

## Completeness Checks
- [ ] **SIBLING**: all four comments across the two crates must be corrected together, not just the one nearest the fix
- [ ] **TESTS**: pin the measured presence table (BNAM on 142/142 Oblivion) so the premise cannot silently re-rot
