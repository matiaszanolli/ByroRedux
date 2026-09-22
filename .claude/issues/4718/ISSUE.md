# UI-D7-2026-09-21-03: --hud splits FO3 from FNV by the --esm filename; FNV DLC and mod launches get the FO3 profile and a fully transparent HUD

**Issue**: #4718
**Severity**: MEDIUM
**Labels**: medium,ui,bug,game:fnv,legacy-compat

## Description
`hud_archive_args` (`byroredux/src/hud.rs`) decides FO3 vs FNV by checking whether the `--esm` path string (lowercased) contains `"falloutnv"` — it never reads `--master`. The documented DLC/mod-plugin launch shape (`--master FalloutNV.esm --esm HonestHearts.esm`, or any third-party plugin) therefore resolves to the FO3 profile.

That profile's `default_textures_bsa` is `"Fallout - Textures.bsa"` (FNV's HUD-relevant art and every font live in `"Fallout - Textures2.bsa"` instead), and the texture archive is opened with a plain `Archive::open(&textures_path)` — not `open_with_numeric_siblings` (defined in `byroredux/src/asset_provider/archive.rs` and used by the texture provider elsewhere), so the FO3 profile's mis-chosen archive never auto-pulls in the numbered sibling that would have the right content.

## Evidence
Verified at HEAD `ee6d3fb39`:
- `byroredux/src/hud.rs`, `hud_archive_args`: `if esm.to_lowercase().contains("falloutnv") { HudGameProfile::fallout_nv() } else { HudGameProfile::fallout3() }` — filename-only split, `--master` unread.
- `byroredux/src/hud.rs`, `HudGameProfile::fallout()`: `default_textures_bsa: if label == "Fallout 3" { "Fallout - Textures.bsa" } else { "Fallout - Textures2.bsa" }`.
- `byroredux/src/hud.rs`, `launch_hud`: `Archive::open(&textures_path)` (imported from `crate::asset_provider::Archive`) — no numeric-sibling auto-load, unlike `asset_provider::archive::open_with_numeric_siblings` which exists in the same module for exactly this pattern.
- Report evidence: `Fallout - Textures.bsa` holds 0 `interface\hud\`/`textures\fonts\` entries of 11,470 files; `Fallout - Textures2.bsa` holds 81. A replay of the FO3/FNV HUD assembly with the FNV profile + Textures2 produced 15,562 opaque px; with the FO3 profile + Textures, 0 opaque px — while `launch_hud` still logs `hud: loaded …` (no error surfaced).

## Impact
A fully transparent (invisible) HUD on every FNV DLC or mod-plugin `--esm` launch with `--hud`, with no diagnostic — `hud: loaded` still logs success. The only workaround today is an explicit `--hud-textures …Textures2.bsa`. No smoke or corpus test covers FNV DLC discovery (tracked separately, see Related), so this regresses silently.

## Related
- PAR-D4-2026-09-21-03 (issue #4665) — no FNV MenuXml corpus test exists to catch this class of gap.
- UI-D7-2026-09-21-04 (this report) — the HUD drivers have zero unit tests, which is exactly why a discovery bug like this ships unnoticed.

## Suggested Fix
Decide FO3 vs FNV structurally: walk the `--master` chain (or check for `FalloutNV.esm` in the data directory) rather than pattern-matching the `--esm` filename. Carry the correct texture archive as profile data instead of a `label == "Fallout 3"` string comparison, and open it with `open_with_numeric_siblings`, matching the texture provider's own archive-resolution path.

## Completeness Checks
- [ ] **TESTS**: A regression test covers `--master FalloutNV.esm --esm <DLC>.esm` / a mod-plugin `--esm` resolving to the FNV profile
- [ ] **SIBLING**: An FNV twin of `m48-5-fo3-hud.sh` with a DLC `--esm`, per UI-D7-2026-09-21-04's suggested fix

Source: docs/audits/AUDIT_UI_2026-09-21.md (UI-D7-2026-09-21-03)
