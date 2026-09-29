# #5067: FNV-2026-09-29-D6-01: Prospector's content changed between bench records (−32 entities, −24 draws, −35 TLAS instances), and the ROADMAP record says the content is unchanged

**Labels**: documentation, medium, performance, legacy-compat, game:fnv, doc-rot

**Source report**: `docs/audits/AUDIT_FNV_2026-09-29.md`
**Severity**: MEDIUM
**Dimension**: Real-Data Validation / Bench-of-Record

## Location
- `ROADMAP.md` bench-of-record refresh paragraph (`a37fcba3c`): "Entity, light and TLAS counts match the old record in every scene, so the content is unchanged."
- `docs/audits/BENCH_stepped-camera_4c9a5b36.tsv`, `BENCH_stepped-camera_cb44d99f6.tsv`, `BENCH_stepped-camera_a37fcba3c.tsv` (Prospector rows).

## Description
The committed TSVs for the same harness mode (renderer-stepped, orbit) show Prospector's content changed:

| Record | Entities | Draws | Lights | TLAS |
|---|---:|---|---:|---:|
| `4c9a5b36` (2026-09-09) | 3146 | 928/54b/6c | 25 | 928 |
| `cb44d99f6` (2026-09-22, uncontrolled) | 3149 | 928/54b/6c | 25 | 928 |
| `a37fcba3c` (2026-09-28, record) | **3114** | **904**/257b/68c | 25 | **893** |

- The drop landed in `cb44d99f6..a37fcba3c`. Fewer entities and TLAS instances is a spawn-side change, not batching.
- No commit in that window says it changes Prospector content. Candidates in the window: `5570c221c` (dismemberment caps identified from body-part data; AUDIT_RUNTIME_2026-09-29 measured FNV AtomicWrangler −92 entities/draws/skin slots from it and names it the most likely cause, inferred, not measured on Prospector), `e52d4a8a1` (#4812/#4696 TPLT-terminal outfit/level + LVLO counts), `b46fd9d33` (#4697/#4698/#4706 disabled refs and loose items). `b9e961eeb` (bench camera move) explains the draw-split change but not the entity/TLAS drop.
- The same "match" claim is also false for Whiterun (+12 entities, 1260 → 1301 draws), MedTek (+23 entities) and Dugout (TLAS 1818 → 1807).

## Evidence
```
awk -F'\t' '$1=="prospector" && $2=="taa" && $3=="1"{print $19,$20,$21,$22}' docs/audits/BENCH_stepped-camera_{4c9a5b36,cb44d99f6,a37fcba3c}.tsv
3146 928/54b/6c 25 928
3149 928/54b/6c 25 928
3114 904/257b/68c 25 893
```

Validated at HEAD 9fcfdc3fc: re-ran the awk above (identical output); the ROADMAP sentence is still present verbatim; `5570c221c`, `e52d4a8a1`, `b46fd9d33` and `b9e961eeb` are all ancestors of `a37fcba3c` and descendants of `cb44d99f6`.

## Impact
- FNV's reference scene lost 35 rendered instances without explanation: either a content regression (missing NPC gear or props) or an intended change (e.g. dismemberment caps) that nobody recorded.
- The +5% Prospector TAA "win" in the refresh is measured on different content.
- The false "content is unchanged" line stops the next reader from looking.

## Related
- R6a-regress-22 (FO4 frame-time; separate), #4812, #4696, #4698, AUDIT_RUNTIME_2026-09-29 (RT-4 attribution of `5570c221c`).

## Suggested Fix
- Run `--game fnv --cell GSProspectorSaloonInterior --bench-hold` at `cb44d99f6` and at `a37fcba3c` (or at `5570c221c^` / `5570c221c`) using the same-machine control procedure, and diff `entities` / per-NPC equipment lists to attribute the 35 instances.
- Then correct the ROADMAP sentence (all four scenes), or file the regression if it is content loss.

## Completeness Checks
- [ ] **SIBLING**: Same check done for the Whiterun, MedTek and Dugout rows the same sentence covers
- [ ] **TESTS**: The attribution is recorded (TSV `# regenerated:` header or ROADMAP note) so the next refresh can diff against it

