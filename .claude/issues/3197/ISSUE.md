# TD4-2026-08-20-01: the validate gate's symbol advisory is blind to SCREAMING_SNAKE_CASE and is cleared by negative assertions

**Issue**: #3197 — https://github.com/matiaszanolli/ByroRedux/issues/3197
**Severity**: MEDIUM
**Labels**: `medium,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md` § TD4-2026-08-20-01 (Dimension 4 — Audit-Finding Rot).

**Severity**: MEDIUM · **Effort**: small
**Location**: `.claude/commands/_audit-validate.sh:169-208` (the symbol-advisory block). Demonstrating case: `.claude/commands/audit-safety/SKILL.md:257` + `crates/renderer/src/shader_constants.rs:1241`.

## Summary

The validate gate's symbol advisory — the guard built specifically to catch renamed/stale symbols in audit skill files — **structurally cannot see the dominant naming convention for the symbols audit skills cite most**, and is separately cleared by *negative* assertions. It prints `0 advisory symbols` at HEAD, and that zero is not evidence of cleanliness.

This matters beyond one gate. `0b9a0c9d` ("resolve advisory symbol-drift flags from session-close gate") recorded the advisories as **resolved**. That claim is true *only for what the gate can see*. #3052 — a backticked symbol that exists nowhere — was OPEN before that commit and is OPEN now, and the gate reports clean over it.

This is the *mechanism*; **#3052 is the instance**. Filed separately per the precedent of TD4-2026-08-16-01 / #2974, which filed a recipe defect apart from the instances it produced.

## Blind spot (a) — the needle is lowercase-anchored

```bash
# _audit-validate.sh:207
done < <(grep -rhoE '`[a-z][a-z0-9_]{6,}`' "${skill_files[@]}" ... )
```

`[a-z]` as the first character excludes **every** `SCREAMING_SNAKE_CASE` constant *before* any existence check runs. That is precisely the convention used for budgets, limits and flag bits — the class audit skills quote most: `MAX_TOTAL_BONES`, `GLASS_RAY_BUDGET`, `MAX_MATERIALS`, `MAX_WATER_DRAWS`, `RESTIR_M_CAP`, `INSTANCE_FLAG_*`, `MAT_FLAG_*`.

Reproduced at HEAD with the *same* logic and an uppercase-inclusive needle:

```
$ git ls-files '*.rs' | xargs cat > /tmp/rs_blob
$ grep -rhoE '`[A-Z][A-Z0-9_]{6,}`' .claude/commands/_audit-*.md \
      .claude/commands/audit-*/SKILL.md | tr -d '`' | sort -u | wc -l
157                       # uppercase symbols backticked in skill files
$ while read s; do grep -qw "$s" /tmp/rs_blob || echo "$s"; done < /tmp/upper
BGSM_MODEL_SPACE_NORMALS
BGSM_PBR
FO4_ENV_SCALE
RESTIR_M_CAP
TECH_DEBT
VERTEX_INPUT
```

**157 backticked uppercase symbols; the advisory examines none of them.**

## Blind spot (b) — a "must NOT exist" assertion counts as existence

The existence check is `grep -qw "$sym" "$src_blob"` over every tracked `.rs` concatenated. A symbol whose *only* `.rs` occurrence is inside an assertion that it must **not** exist is therefore treated as live:

```
$ grep -rn "REFRACT_PASSTHRU_BUDGET" crates byroredux
crates/renderer/src/shader_constants.rs:1241:  !src.contains("REFRACT_PASSTHRU_BUDGET = 2"),
```

`REFRACT_PASSTHRU_BUDGET` is absent from the six-symbol list above **for this reason** — so widening the regex alone would still not catch #3052. Both blind spots must close for that one instance to surface.

## Triage of the six (so the fix does not land on a noise wall)

| Symbol | Verdict |
|---|---|
| `FO4_ENV_SCALE` — `audit-fo4/SKILL.md:110` | **Genuine.** The sentence itself says the name was replaced by `FO4_DLC_UPPER` under #1242 — so the convention requires it be *italicised*, not backticked |
| `BGSM_PBR`, `BGSM_MODEL_SPACE_NORMALS` — `audit-fo4/SKILL.md:143` | **Genuine, mis-named.** Real symbols are `MAT_FLAG_BGSM_PBR` / `MAT_FLAG_BGSM_MODEL_SPACE_NORMALS` (`crates/renderer/src/vulkan/material.rs:431`, `context/mod.rs:178`) |
| `RESTIR_M_CAP` | False positive of the `.rs`-only corpus — it lives in `crates/renderer/shaders/triangle.frag:2677` |
| `TECH_DEBT`, `VERTEX_INPUT` | Benign prose; same class the existing `comprehensive` filter handles |

~50% true-positive rate — well inside the "advisory, not fatal" framing the block already documents. **These three genuine hits are part of this fix**: widening the regex without correcting them just moves the noise.

## Impact

The advisory's own docstring states why it exists (`_audit-validate.sh:158-159`):

> `gpu_material_size_is_300_bytes` outlived a 300 → 348 B `GpuMaterial` change — a wrong number in a GPU layout contract.

Four days ago `GpuCamera` did the same thing (336 → 352 B on `8e7582ed`) and the advisory again printed nothing — see the sibling finding filed from this report. **A guard that reports clean while the exact defect class it was built for is live is worse than no guard: it converts "nobody checked" into "the check passed."** That is what `0b9a0c9d`'s closeout recorded.

Corroboration: `/audit-safety` hit this independently on day one of the 2026-08-20 sweep — it found the stale `REFRACT_PASSTHRU_BUDGET` reference by reading, while the gate said clean, and had no explanation for the discrepancy. This finding is that explanation.

## Suggested Fix

1. Widen the needle to `` `[A-Za-z][A-Za-z0-9_]{6,}` `` and add `TECH_DEBT` / `VERTEX_INPUT` to the benign list.
2. Build `$src_blob` from lines that are **not** negations — cheapest correct form is to exclude lines matching `!src.contains(` / `!.*contains(`, so a "must not exist" assertion stops counting as evidence of existence.
3. Extend the corpus to `git ls-files '*.rs' '*.glsl' '*.vert' '*.frag' '*.comp'` — removes the `RESTIR_M_CAP` class of false positive and lets several ad-hoc filter entries be dropped.
4. Fix the three genuine hits: italicise `FO4_ENV_SCALE` at `audit-fo4/SKILL.md:110`; correct `BGSM_PBR` / `BGSM_MODEL_SPACE_NORMALS` → `MAT_FLAG_*` at `:143`.

## Related

- **#3052** (OPEN) — the *instance* this mechanism hides
- **#2974** / TD4-2026-08-16-01 — the precedent for filing a recipe defect apart from its instances
- The `GpuCamera` 336 → 352 doc drift filed from this same report — the live recurrence the advisory missed
- The `docs/engine/` file-glob half of the same gap, filed alongside
- `.claude/commands/_audit-common.md:270-279` — the backtick-vs-italics convention this enforces
- `0b9a0c9d` — the commit whose "resolved" claim this scopes

## Completeness Checks
- [ ] **SIBLING**: Both blind spots closed — a case-widened regex alone still misses #3052
- [ ] **NOISE-FLOOR**: The three genuine hits (`FO4_ENV_SCALE`, `BGSM_PBR`, `BGSM_MODEL_SPACE_NORMALS`) corrected in the same change, so the first run is actionable
- [ ] **CORPUS**: Shader sources added to the blob, or `RESTIR_M_CAP` explicitly filtered
- [ ] **TESTS**: `_audit-validate.sh` run at HEAD surfaces #3052's symbol; re-run after the skill fixes returns to a real (not structural) zero
