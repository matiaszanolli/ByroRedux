**From**: NIFAL audit `docs/audits/AUDIT_NIFAL_2026-06-13.md` (Dimension 7 — Completeness)
**Severity**: MEDIUM · **Tier Violated**: n/a (test-infrastructure — the *signal* is broken, not the layer)
**Game Affected**: FNV (reached), FO76 + Starfield (unreached, latent)
**Location**: `crates/nif/tests/translation_completeness.rs` — FNV `material_kind>=35`, FO76 `texture_path>=75`, Starfield `texture_path>=75`

## Description
The per-game fill-rate floors added by Fix #1320 were authored on the assumption "newer engine ⇒ higher fill-rate for every slot." Three floors contradict the actual measured content of the 200-NIF sample, so the cross-game completeness harness panics RED with **no real regression**:

- **FNV `material_kind >= 35%`** — actual **8.1%**. `material_kind` is set from `shader.shader_type` **only** on the `BSLightingShaderProperty` arm (`crates/nif/src/import/material/walker.rs:337`), plus engine-synthesized 101 (effect) / 102 (nolighting). FNV content uses `BSShaderPPLightingProperty`, which never sets `material_kind` — FNV legitimately classifies only its effect/nolighting meshes (8.1%). 35% was never achievable on this corpus.
- **FO76 `texture_path >= 75%`** — actual **9.6%**. FO76 NIFs fully migrated texture references into BGSM (`material_path = 90.4%`); inline `texture_path` is nearly empty. Same architecture as FO4, except FO4 NIFs still carry inline paths (tex=100%) so the FO4 floor passes while FO76's identical floor cannot.
- **Starfield `texture_path >= 75%`** — actual **0.0%**. `BSGeometry` carries no inline texture path; material lives in `material_path` (100%) / CDB. The floor's own sibling comment already acknowledges Starfield material is CDB-resolved, yet still asserts a 75% inline-texture floor.

## Evidence (live harness run, 2026-06-13)
```
game             imported   tex%   mat_path%  m_kind%   nrm%    tan%   consistent%
FNV          meshes= 629   95.1%     0.0%      8.1%   89.2%   97.3%   100.0%
FO76         meshes= 293    9.6%    90.4%      9.6%    5.5%  100.0%   100.0%
Starfield    meshes= 176    0.0%   100.0%      0.0%    0.0%  100.0%   100.0%
```
Run panics at the FNV `material_kind` floor before reaching the FO76/Starfield floors. Walker arm confirmed at `walker.rs:337` (only `BSLightingShaderProperty` writes `material_kind`). Floor origin: `git log -L` → `4376f7a6 Fix #1320`.

## Impact
The cross-game completeness regression signal is **non-functional** — it panics on the first game and presents as a hard translation-regression failure when it is a stale-threshold artifact. A *real* future regression (e.g. FNV losing tangent synthesis) would be masked behind this pre-existing red. This is exactly the "unvalidated threshold" failure mode TD-D6/#1320 was filed to eliminate, recurring one tier up. Structural consistency is 100% on all 7 games — the layer itself is clean.

## Suggested Fix
Recalibrate the three floors to measured ground truth with a conservative margin: FNV `material_kind >= 5%`; FO76 assert `material_path >= 75%` (the slot that actually carries FO76's material identity) instead of `texture_path`; Starfield drop the `texture_path` floor and assert `material_path >= 75%` + `tangents >= 65%` (both real). Document the *measured* value beside each floor (the #1320 fix added floors but not the measurements behind them).

## Completeness Checks
- [ ] **SIBLING**: Re-check every per-game floor in `translation_completeness.rs` against the measured table, not just the three flagged (FO3 `material_kind>=5`, SkyrimSE small-sample variance).
- [ ] **CANONICAL-BOUNDARY**: Confirm any recalibration measures canonical-`Material` slots, not raw-tier fields; do not push per-game logic into the harness's pass criteria in a way that hides a real extractor leak.
- [ ] **TESTS**: After recalibration, the harness passes green on all 7 installed games and each floor carries an inline measured-value comment.
