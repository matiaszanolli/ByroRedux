# #3525 — SF-2026-08-27b-X-01: the CI clippy gate is red on main — two warnings, both from 2026-08-27 audit-fix commits

Source: `docs/audits/AUDIT_STARFIELD_2026-08-27b.md`
Filed: 2026-08-28 (`/audit-publish`)
Labels: high, bug, tech-debt, esm-plugin, terrain-exterior, game:starfield, legacy-compat

---

From `docs/audits/AUDIT_STARFIELD_2026-08-27b.md` (branch `main` @ `969d81c8`).

- **Severity**: HIGH (raised from the report's MEDIUM at publish time — this breaks CI for every contributor and blocks every PR)
- **Dimension**: cross-cutting (build gate). **Not Starfield-specific** — reported in the Starfield pass because it gates every Starfield check.
- **Location**: `.github/workflows/ci.yml:94` (the gate); `crates/plugin/src/esm/records/outfit.rs:71-92`; `byroredux/src/cell_loader/object_lod.rs:250-259`

## Description

CI runs `cargo clippy --workspace -- -D warnings`. On `969d81c8` the workspace emits **two** warnings, so that command exits non-zero and the gate fails. Both were introduced the same day, by commits whose whole purpose was closing audit findings.

Verified independently at publish time: both sites are present on `main`.

## Evidence

`cargo clippy --workspace` on `969d81c8`:
```
warning: you seem to be trying to use `match` for an equality check. Consider using `if`
  --> crates/plugin/src/esm/records/outfit.rs:71:9      [clippy::single_match]
warning: this function has too many arguments (8/7)
  --> byroredux/src/cell_loader/object_lod.rs:250:1     [clippy::too_many_arguments]
warning: `byroredux-plugin` (lib) generated 1 warning
warning: `byroredux` (bin "byroredux") generated 1 warning
```

Attribution by `git log -- <file>`:
- `outfit.rs` last touched by `fa71f1a2` ("Fix #3356: INAM is one array of FormIDs…", 2026-08-27) — the fix collapsed the `INAM` handling to a single `match` arm plus `_ => {}`, which is exactly the `single_match` shape.
- `object_lod.rs` last touched by `c7a70d45` ("Fix #3385: memoise the distant-LOD archive-presence probe", 2026-08-27), which pushed `spawn_object_lod_quad` from 7 to 8 parameters.

No other crate in the workspace emits a warning — these two are the whole gate failure.

**Attempts to disprove**: the gate is not `--all-targets`, so the (larger) crop of warnings in tests and `_tmp_*` examples is genuinely out of scope and does not muddy this; both warnings are on-by-default lints, not pedantic ones; the repo has no `#![allow]` covering either site, and no `clippy.toml` raising `too-many-arguments-threshold`. There is **no** `cargo fmt --check` job, so the unrelated rustfmt drift in these files is correctly not a gate failure.

## Impact

Every PR and every push to `main` fails CI until fixed. The second-order cost is worse than the first: a permanently-red `-D warnings` gate is the standard way a workspace learns to ignore clippy, and this one is the only static-analysis gate the repo has.

## Related

#3356 (`fa71f1a2`), #3385 (`c7a70d45`). A concurrent `/audit-esm` or `/audit-tech-debt` may raise the same item — reconcile before fixing twice.

## Suggested Fix

- `if sub.sub_type == *b"INAM" { … }` at `outfit.rs:71` (the clippy suggestion preserves the comment block).
- For `spawn_object_lod_quad`, bundle `(qx, qy)` or the `world`/`ctx`/`tex_provider` triple into a struct, matching the `Dx10TexInfo` precedent in `crates/bsa/src/ba2.rs:857-863` that was introduced for this exact lint.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other record parsers collapsed to a single `match` arm, other recently-widened spawn helpers)
- [ ] **TESTS**: A regression test pins this specific fix — here, `cargo clippy --workspace -- -D warnings` passing is the pin
