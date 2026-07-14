**Source:** FNV compatibility audit — Dimension 8 (Real-Data Validation), `docs/audits/AUDIT_FNV_2026-07-13.md`
**Severity:** LOW (documentation) · **Status when filed:** NEW, CONFIRMED

## Description
`CLAUDE.md` (Usage section, line 276) gives the FNV interior-cell example as:
```
cargo run -- --esm FalloutNV.esm --cell <id> --bsa Meshes.bsa --textures-bsa Textures.bsa   # interior cell
```
`--bsa <name>` opens the literal path relative to CWD (`build_texture_provider` → `open_with_numeric_siblings` → `Archive::open`, no prefix fallback). Vanilla FNV ships the base archives as `Fallout - Meshes.bsa` and `Fallout - Textures.bsa` — there is **no** bare `Meshes.bsa` / `Textures.bsa`. Running the example verbatim, even with the correct CWD = `Fallout New Vegas/Data/`, opens **zero** archives and loads a near-empty scene (~36 ent / spurious FPS).

## Evidence
- `ls "…/Fallout New Vegas/Data/Meshes.bsa"` → No such file (only `Fallout - Meshes.bsa` exists).
- `assets/debug_profiles.toml [profiles.fnv]` correctly uses `default_bsas = ["Fallout - Meshes.bsa"]` / `default_textures_bsas = ["Fallout - Textures.bsa"]`.
- `README.md:121-122` correctly uses the quoted `"Fallout - …"` names.
- Drift is isolated to the CLAUDE.md Quick-Reference example. Now surfaced loudly at runtime by the #1776 zero-archive guard (`log::error!` "0 mesh archives opened") rather than silently — so this is a papercut, not a trap.

## Impact
A reader following the project's own primary instructions file runs the reference-title interior command and gets an empty scene. Documentation only.

## Suggested Fix
Edit the CLAUDE.md example to `--bsa "Fallout - Meshes.bsa" --textures-bsa "Fallout - Textures.bsa"` (matching README), or switch it to the `--game fnv` profile form which resolves the data dir + archive names automatically.

## Completeness Checks
- [ ] **SIBLING**: confirm README + `assets/debug_profiles.toml` already carry the correct names (they do — only CLAUDE.md drifted)
