# TD1-2026-08-20-01: resolve_water_material is 522 LOC - one 495-line if-let arm

**Issue**: #3208 — https://github.com/matiaszanolli/ByroRedux/issues/3208
**Severity**: LOW
**Labels**: `low,import-pipeline,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md` § TD1-2026-08-20-01 (Dimension 1 — File / Function / Module Complexity).

**Severity**: LOW · **Effort**: medium
**Location**: `byroredux/src/env_translate.rs:535-1056` (`resolve_water_material`)
**Status**: NEW — the *file* is on the secondary >2000-LOC bucket and covered by **#2977**; this is the **function-level** signal, which the SKILL notes is independent: *"file-level crossings and function-level splits are independent signals; don't assume one moves the other."*

## Description

The EXAL water-translate boundary is one function whose body is, after five `let mut` accumulators, a single `if let Some(rec) = waters.get(&form) { … }` arm spanning **495 lines** at brace depth up to 5.

It is well past the SKILL's >200-LOC extraction trigger and is the largest function in the second-hottest file of the delta (`env_translate.rs`, 48 commits since 2026-08-16).

Two of this sweep's water findings land **inside** it (the `foam_strength` literals at `:932`/`:947`, per `NIFAL-D1-2026-08-20-02`), which is the practical cost: every water fix now edits the same 500-line block, and reviewing one means paging past the other 480 lines.

## Evidence (verified at HEAD `bb0b92f2`)

```
$ python3 fnlen.py byroredux/src/env_translate.rs | sort -rn | head -1
522     byroredux/src/env_translate.rs:535     resolve_water_material
# body top-level statements:
let mut mat / kind / flow / normal_path / noise_paths;
if let Some(form) = xcwt_form { … 495 lines … }
let _ = SubmersionState::default();
# 167 lines at indent >=16, 75 at >=20, 24 at >=24; max brace depth 5
```

The function is **long, not tangled** — which is what makes it splittable. The field writes group cleanly by responsibility:

| Group | Fields written |
|---|---|
| Colour + fog | `shallow_color`, `deep_color`, `fog_near/far`, `day_*`, `night_*`, `underwater_*` (×4) |
| Layer motion | `scroll_a/b/c`, `uv_scale_a/b/c`, `flowmap_scale` |
| Noise + rain | `noise_falloff`, `rain_velocity/falloff/dampener/response/start_size` |
| Specular + reflection | `reflectivity`, `reflection_tint`, `reflection_hdr_multiplier`, `specular_radius/magnitude`, `sun_specular_power`, `fresnel_f0` |
| Kind + flow | `kind`, `flow`, `foam_strength`, `wave_amplitude/frequency` |
| Texture paths | `normal_path`, `noise_paths[3]` |

## Impact

**Maintenance cost only — no correctness claim.** But this is the single translate boundary for *every game's* exterior water, it took 48 commits in four days, and the next water change will land in it too.

## Suggested Fix

Extract five private helpers along the table above, each taking `&mut WaterMaterial` and `&WatrRecord`:

- `resolve_water_colors`
- `resolve_water_layer_motion`
- `resolve_water_noise_and_rain`
- `resolve_water_specular`
- `classify_water_kind_and_flow`

`classify_water_kind_and_flow` is also the natural home for the `foam_strength` mapping `NIFAL-D1-2026-08-20-02` wants hoisted, so **the two fixes compose**.

Follow the *feedback_safe_large_function_split* method from project memory: `sed`-extract exact line ranges rather than retyping, and diff-check before committing since `cargo fmt` reformats the whole crate.

## Related

- **#2977** — the file-level >2000-LOC bucket (`env_translate.rs` is 3216 total / 1405 production)
- `NIFAL-D1-2026-08-20-02` — a finding *inside* this function; the two fixes compose
- The Session-34/35 split precedent (project memory: *session34_layout*, *session35_layout*)

## Completeness Checks
- [ ] **CANONICAL-BOUNDARY**: The split preserves the single EXAL translate boundary — per-game logic stays at the parser→`WaterMaterial` seam, never pushed downstream
- [ ] **SAFE-SPLIT**: Line ranges extracted mechanically, not retyped; `cargo fmt` diff reviewed before commit
- [ ] **TESTS**: The ~19 existing `resolve_water_material_*` tests in the same file still pass unchanged — the split must be behaviour-preserving
