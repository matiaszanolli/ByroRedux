# Renderer Audit — Dimension 21: Disney BSDF / PBR Gating (2026-05-24)

Focused sweep — only Dimension 21 (Disney BSDF / PBR Gating). This is the dimension landed earlier today in commit `89c224cb` (audit-skill refresh). Audit purpose: verify the prompt I just wrote matches the code it's auditing.

## Executive Summary

- **2 findings**: 0 CRITICAL · 0 HIGH · 0 MEDIUM · **2 LOW** · 5 INFO. Both LOWs are drift in artifacts authored TODAY:
  - **DIM21-NEW-01**: the Dim 21 audit prompt itself (commit `89c224cb`) — gate count off by one, two flag names missing `BGSM_` prefix, anisotropic input range wrong by half-axis
  - **DIM21-NEW-02**: today's `AUDIT_FNV_2026-05-24_DIM4.md` propagates the same gate-count error from an earlier draft
- The **code** is clean — all 5 Disney functions exist and match their prompt-claimed semantics (input-domain clamps live, sheen-not-/PI'd, GGXAniso reduces to GGX, preset table is in `material.rs::presets`).
- The drift is purely audit-side. A user running `/audit-renderer 21` would have one extra grep target than exists in code — false-positive risk on the next audit run.

| Severity | NEW | Carryover | Total |
|----------|-----|-----------|-------|
| HIGH     | 0   | 0         | 0     |
| MEDIUM   | 0   | 0         | 0     |
| LOW      | 2   | 0         | 2     |
| INFO     | 5   | 0         | 5     |

## Verifications (5 contracts clean)

### V1 — Disney lobe functions exist at claimed sites

| Function | Site | Status |
|---|---|---|
| `dielectricF0FromIor(eta)` | `triangle.frag:680` | ✓ |
| `distributionGGXAniso(NdotH, HdotX, HdotY, ax, ay)` | `triangle.frag:616` | ✓ |
| `deriveAxAy(roughness, anisotropic, out ax, out ay)` | `triangle.frag:644` | ✓ |
| `disneyDiffuseSplit(...)` | `triangle.frag:732` | ✓ |
| `DisneyDiffuseSplit` (return struct) | `triangle.frag:729` | ✓ |

### V2 — Input-domain clamps protect against authoring corruption

- **`dielectricF0FromIor`** at `triangle.frag:686`: `float e = max(eta, 1e-3);` — protects against `eta = 0` (division-by-zero in the Fresnel derivation) and `eta < 0` (negative-IOR nonsense). Lower bound = `1e-3` (well below any physical authoring; air ≈ 1.0, water ≈ 1.33, glass ≈ 1.5, diamond ≈ 2.42). ✓
- **`deriveAxAy`** at `triangle.frag:652`: `float aniso = clamp(anisotropic, 0.0, 1.0);` — protects against `sqrt(1 - 0.9·a) < 0` for `a > 1.0` (NaN propagation through `ax`/`ay`). ✓ (NB: the audit prompt I just wrote says the input range is `[-1..1]` — that's wrong; see DIM21-NEW-01.)

### V3 — `distributionGGXAniso` degenerate-to-`distributionGGX` claim

- Doc comment at `triangle.frag:606-607` is load-bearing: *"Reduces exactly to `distributionGGX` when ax == ay (the legacy isotropic case the default ax = ay = roughness path hits)."*
- Math check: for `ax = ay = a`, GGXAniso's denominator `HdotX²/a² + HdotY²/a² + NdotH² = (HdotX² + HdotY²)/a² + NdotH² = (1-NdotH²)/a² + NdotH²` (unit half-vector identity); multiplying by `a²/a²` reduces to GGX's form. Citation: knightcrawler25/GLSL-PathTracer `sampling.glsl:90-95` (`GTR2Aniso`). ✓

### V4 — Sheen is NOT divided by π (Disney 2012 spec)

- `triangle.frag:768`: `o.diffuse = albedo * mix(Fd + Fretro, ss, subsurface) * (1.0 / PI);` — diffuse IS `/PI` (Lambertian). ✓
- `triangle.frag:769`: `o.sheen = FH * sheen * sheenColor;` — sheen is NOT `/PI` (layered atop Lambertian, additive). ✓
- Caller at `triangle.frag:2453`: `diffuseBrdf = (dd.diffuse + dd.sheen) * (1.0 - metalness);` — composes the split lobes without further `/PI`. ✓
- Pre-#1252 code (per doc comment at `triangle.frag:2663`) "over-amplified the sheen component by ~3.14×" — the split-lobe refactor fixes this. The audit-prompt invariant is correct in code.

### V5 — Disney preset table in `material.rs::presets`

- Module exists at `crates/renderer/src/vulkan/material.rs:524` (`pub mod presets`).
- 6 documented presets as `pub fn` constructors:
  - `polished_metal()` — line 532
  - `glass()` — line 553 (IOR 1.45 matches the audit-prompt claim)
  - `car_paint(base)` — line 574
  - `lacquered_plastic(base)` — line 589
  - `painted_matte(base)` — line 605
  - `skin_wax_marble(base)` — line 621
- 2 tests in the same module (`presets_inherit_defaults_for_unset_fields`, line 707) pin the presets against unrelated field-default changes. ✓

## Findings

### LOW

#### DIM21-NEW-01: My own Dim 21 audit prompt has 4 factual drifts vs current code

- **File**: `.claude/commands/audit-renderer.md` — Dim 21 checklist (lines authored in commit `89c224cb` earlier today)
- **Severity**: LOW (cosmetic / will surface as false positives on the next `/audit-renderer 21` run)
- **Effort**: trivial (4 line edits)
- **Status**: NEW

##### Drift list

| Prompt claim | Actual code | Site |
|---|---|---|
| **"Expected count: 4"** PBR gate sites (`2 diffuse-eval + 2 deferred-specular`) | **3** gate sites total | `triangle.frag:1652, 2447, 2669` |
| `MAT_FLAG_HAS_TRANSLUCENCY` (in #1147 Phase 2b checklist) | `MAT_FLAG_BGSM_TRANSLUCENCY (1u << 6)` | `triangle.frag:183` |
| `MAT_FLAG_MODEL_SPACE_NORMALS` (in #1147 Phase 2b checklist) | `MAT_FLAG_BGSM_MODEL_SPACE_NORMALS (1u << 7)` | `triangle.frag:184` |
| "`anisotropic[-1..1]`" in `deriveAxAy` description | `clamp(anisotropic, 0.0, 1.0)` — range is `[0, 1]` | `triangle.frag:652` (see comment at `:646-651`) |

##### Why these slipped through

I wrote this Dim 21 checklist in the previous turn from a combination of:
1. **Commit-log summary** (the #1248-#1252 batch) — accurate on function names and Fresnel-F0-derivation semantics
2. **Memory of today's FNV Dim 4 audit** — that audit had the "4 sites" error baked in (see DIM21-NEW-02)
3. **Disney 2012 spec convention** — the `[-1, 1]` anisotropic range is the *original* paper's convention. ByroRedux follows GLSL-PathTracer's `[0, 1]` simplification (explicit in the `deriveAxAy` doc-comment at lines 646-651). The prompt should match the code's actual convention, not the paper's

##### Suggested fix

4 line edits to `.claude/commands/audit-renderer.md`:

```diff
- - **Gate is `MAT_FLAG_BGSM_PBR` (bit 5) only** — verify the 4 consumer sites in `triangle.frag` ALL test this single flag (no path lights FNV / FO3 / Skyrim legacy materials with the Disney lobe). Gate-site grep target: `if ((mat.materialFlags & MAT_FLAG_BGSM_PBR) != 0u)`. Expected count: 4 (two diffuse-eval sites + two deferred-specular sites).
+ - **Gate is `MAT_FLAG_BGSM_PBR` (bit 5) only** — verify the 3 consumer sites in `triangle.frag` ALL test this single flag (no path lights FNV / FO3 / Skyrim legacy materials with the Disney lobe). Gate-site grep target: `if ((mat.materialFlags & MAT_FLAG_BGSM_PBR) != 0u)`. Expected count: 3 (one BRDF-kernel branch at :1652 + one Disney-diffuse consumer at :2447 + one deferred-specular consumer at :2669).

- - **`deriveAxAy(roughness, anisotropic, out ax, out ay)`** (#1250 + #1253/#1254): remaps perceptual roughness + anisotropic[-1..1] to ax/ay. Input-domain clamps for `roughness < 0` / `anisotropic < -1` / `anisotropic > 1` landed in #1254
+ - **`deriveAxAy(roughness, anisotropic, out ax, out ay)`** (#1250 + #1253/#1254): remaps perceptual roughness + `anisotropic[0..1]` (ByroRedux follows the GLSL-PathTracer half-axis convention, NOT the Disney 2012 paper's full `[-1, 1]`) to ax/ay. Input-domain clamp `clamp(anisotropic, 0.0, 1.0)` at `triangle.frag:652` landed in #1254

- - `MAT_FLAG_HAS_TRANSLUCENCY` → SSS path
- - `MAT_FLAG_MODEL_SPACE_NORMALS` (set by #972 for direct-TXST REFRs) → model-space sampling path
+ - `MAT_FLAG_BGSM_TRANSLUCENCY (1u << 6)` → SSS path (sibling: `MAT_FLAG_BGSM_TRANSLUCENCY_THICK_OBJECT (1u << 8)` and `MAT_FLAG_BGSM_TRANSLUCENCY_MIX_ALBEDO (1u << 9)` modulate the SSS sub-modes)
+ - `MAT_FLAG_BGSM_MODEL_SPACE_NORMALS (1u << 7)` (set by #972 for direct-TXST REFRs) → model-space sampling path
```

##### Completeness Checks

- [ ] **UNSAFE**: N/A — prompt-only edit
- [ ] **SIBLING**: Verify `.claude/commands/audit-fo4.md` and other per-game audits don't quote the same wrong gate count (today's FNV Dim 4 audit DOES — see DIM21-NEW-02)
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: Run `.claude/commands/_audit-validate.sh` after the edit (already OK because path refs unchanged, but confirm)

#### DIM21-NEW-02: Today's FNV Dim 4 audit propagates the same gate-count drift

- **File**: `docs/audits/AUDIT_FNV_2026-05-24_DIM4.md`
- **Severity**: LOW (cosmetic; the audit's conclusion still holds — Disney lobe IS unreachable for FNV)
- **Effort**: trivial (2 line edits in the verification table)
- **Status**: NEW (sibling of DIM21-NEW-01)

##### Drift detail

The audit at `V3 — Disney BSDF gating (highest-risk for FNV)` lists 4 gate sites:
```
- `:1652` — `if ((mat.materialFlags & MAT_FLAG_BGSM_PBR) != 0u) { ... }`
- `:2447-2449` — `disneyDiffuseSplit(...)` consumer
- `:2461` (implied by surrounding gate at 2447) — same flag
- `:2669-2671` — second Disney consumer (deferred specular path)
```

The third line — *":2461 (implied by surrounding gate at 2447) — same flag"* — is a fudge. There is no separate `if (... MAT_FLAG_BGSM_PBR ...)` at line 2461; it's just inside the scope of the gate at 2447. Actual distinct `if`-statements testing `MAT_FLAG_BGSM_PBR`: 3 (lines 1652, 2447, 2669).

The hot-path table further down (`| Disney BSDF gate | 4 sites in triangle.frag | ...`) carries the same wrong count.

##### Why this matters

The audit's headline conclusion is still correct: **FNV materials don't set `MAT_FLAG_BGSM_PBR`, so the Disney lobe is unreachable.** That conclusion is true regardless of whether the count is 3 or 4 — *unreachable on 3 gates is the same as unreachable on 4*. The drift is in the verification's grep target arithmetic, not in the verdict.

But: the audit-publishing workflow files findings against `MAT_FLAG_BGSM_PBR` gate counts. A future audit run that diffs site counts (this gate vs that one) needs the right denominator. Fix while the muscle memory is fresh.

##### Suggested fix

2 line edits to `docs/audits/AUDIT_FNV_2026-05-24_DIM4.md` (the audit report is in `docs/audits/`, so the file is now historical — but a one-line correction is appropriate since the report is referenced by the next audit cycle):

```diff
- - **Sites** (4 gate locations in `triangle.frag`):
+ - **Sites** (3 gate locations in `triangle.frag`):
    - `:1652` — `if ((mat.materialFlags & MAT_FLAG_BGSM_PBR) != 0u) { ... }`
    - `:2447-2449` — `disneyDiffuseSplit(...)` consumer
-   - `:2461` (implied by surrounding gate at 2447) — same flag
    - `:2669-2671` — second Disney consumer (deferred specular path)
```

and:

```diff
- | Disney BSDF gate | 4 sites in `triangle.frag` | PBR-only, FNV bypasses |
+ | Disney BSDF gate | 3 sites in `triangle.frag` | PBR-only, FNV bypasses |
```

##### Completeness Checks

- [ ] **UNSAFE**: N/A
- [ ] **SIBLING**: Audit-side only — no code drift
- [ ] **DROP**: N/A
- [ ] **LOCK_ORDER**: N/A
- [ ] **FFI**: N/A
- [ ] **TESTS**: N/A

## Notes

- This sweep is the first run of the new Dim 21. It caught drift in artifacts I wrote 2 commits ago — useful validation that the audit-dim-then-publish workflow surfaces real issues even when the auditor wrote the code being audited.
- The Dim 21 prompt's **load-bearing claims** (gate flag identity, GGXAniso → GGX degeneracy, sheen-not-`/PI`'d, dielectricF0FromIor clamp existence, Disney preset table location) all verified correct. The drift is in **accessory counts and field names** — easy mistakes when writing from commit-log summary instead of grep.
- A useful follow-up: extend `_audit-validate.sh` to count `if ((mat.materialFlags & X)` occurrences against any audit-prompt "Expected count: N" claims. That would catch DIM21-NEW-01-style drifts mechanically.
- No code findings. No `/audit-publish` follow-up needed unless you want to file the two prompt-drift LOWs as separate trackers — they're trivially fixable in one commit instead.
