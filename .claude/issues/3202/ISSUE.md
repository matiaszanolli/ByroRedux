# TD4-2026-08-20-03: _audit-validate.sh never inspects docs/engine/

**Issue**: #3202 — https://github.com/matiaszanolli/ByroRedux/issues/3202
**Severity**: LOW
**Labels**: `low,tech-debt,bug`
**Source report**: `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md`
**Filed**: 2026-08-20 · `/audit-publish` · verified against HEAD `bb0b92f2`

---

Filed from `docs/audits/AUDIT_TECH_DEBT_2026-08-20.md` § TD4-2026-08-20-03 (Dimension 4 — Audit-Finding Rot).

**Severity**: LOW · **Effort**: trivial (glob change) + small (triaging the resulting noise floor)
**Location**: `.claude/commands/_audit-validate.sh:78-81` (`skill_files`)

## Description

The gate globs exactly two shapes:

```bash
skill_files=(
    .claude/commands/audit-*/SKILL.md
    .claude/commands/_audit-*.md
)
```

Yet `_audit-common.md:116-138` lists **eighteen `docs/engine/*.md` files** as *"the authoritative, code-verified reference for their domain"* and instructs every audit to *"prefer them over re-deriving facts from source."*

**Those eighteen files are checked by nothing.** Neither the path half of the gate nor the symbol half reaches them.

## Evidence — the existing logic already works there; only the glob keeps it blind

Simulating the *existing, unmodified* advisory over `docs/engine/*.md` at HEAD:

```
$ grep -rhoE '`[a-z][a-z0-9_]{6,}`' docs/engine/*.md | tr -d '`' | sort -u | wc -l
1184
$ while read s; do grep -qw "$s" /tmp/rs_blob || echo "$s"; done < /tmp/doc_syms | wc -l
92
$ ... | grep gpu_camera_is_336_bytes
gpu_camera_is_336_bytes          ← docs/engine/renderer.md:576
```

The symbol is backticked, lowercase and ≥7 chars — **squarely inside the advisory's current needle**. Only the file glob kept it invisible. Extending the glob would have caught the `GpuCamera` 336 → 352 B doc drift **on day one** instead of four days and one audit sweep later.

The raw noise floor over `docs/engine/` is ~92 entries, dominated by classes the gate already filters (git short hashes) or can filter trivially: `nif.xml` field names (`bhk_rigid_body`, `has_animation_notes`) and game asset names (`glasspitcher`, `citycydoniamainlevel`).

## Impact

Two of this report's three MEDIUMs are doc drift in files the gate cannot see, and one of those is a **GPU layout contract**. The #1114 gate closed the recurring `TD7-*` stale-path family for *skills*; the same family is unpoliced for *reference docs* — and **the reference docs are what audits are told to believe**.

## Suggested Fix

1. Add `docs/engine/*.md` to `skill_files`.
2. Extend the **path** half there too — `docs/engine/` carries many relative markdown links, which the existing `path_exists` suffix matcher already handles.
3. Keep the **symbol** half advisory as it is today.
4. Add filters for a `nif_` / `bhk_` prefix class (git short hashes are already filtered) before enabling, so the first run is not a wall of noise.

Best landed together with the symbol-advisory case/negation fix filed alongside — the two halves of the same gap.

## Related

- The `GpuCamera` 336 → 352 doc-drift finding — what this would have caught, filed from the same report
- The symbol advisory's case/negation blind spots — the other half of this gap
- **#1114** / TD7-050 — the path gate this extends
- `_audit-common.md:116-138` — the eighteen reference docs currently outside both gates

## Completeness Checks
- [ ] **SIBLING**: Both halves of the gate (path + symbol) extended to `docs/engine/*.md`, not just one
- [ ] **NOISE-FLOOR**: First run triaged and the benign classes filtered, so the gate stays usable
- [ ] **TESTS**: Running the extended gate at the pre-fix tree surfaces `gpu_camera_is_336_bytes`; running it post-fix returns clean
