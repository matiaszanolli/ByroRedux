# TD2-2026-10-27-02: Five workspace-hygiene scans re-spell the "bin src + every crates/*" root walk; none covers `tools/`

Labels: low, tech-debt, test-gap, bug
Source: docs/audits/AUDIT_TECH_DEBT_2026-10-27.md

# TD2-2026-10-27-02: Five workspace-hygiene scans re-spell the "bin src + every crates/*" root walk; none covers `tools/`

**Source**: `docs/audits/AUDIT_TECH_DEBT_2026-10-27.md` finding `TD2-2026-10-27-02` (severity LOW, status NEW, CONFIRMED against HEAD `fa6be4a6b`).
**Location (re-mapped to current paths)**: `byroredux/src/workspace_hygiene_tests.rs` - walks at ~:988-993 (#6351), :1057-1061 (#6366), :1114-1117 (#6373), :1172-1176 (#6372), :1500-1506 (zup); the existing `hygiene_scan_roots()` (~:313) and the one scan that walks `../tools` (~:524-530).

## Description
Each test builds its own roots with a private `read_dir(manifest.join("../crates"))` loop, and the loops disagree:
- **Roots**: some push `<crate>/src`, some the whole crate dir; some check `is_dir()`, some do not.
- **Test-file skip**: most re-spell the `_tests.rs` / `tests.rs` / `tests/` skip.
- **Coverage**: only the scan at ~:524-530 walks `../tools`. The spliced-doc, `init_resource`, pool-to-local and `weapon_stub` guards therefore never see byro-dbg, byro-launcher, byro-detect or texture-upscale, although #6373's own message says it covers "the bin and every crate".

## Evidence
`grep '../crates\|../tools' workspace_hygiene_tests.rs` shows five separate `read_dir(manifest.join("../crates"))` loops (lines 1058, 1115, 1173, 1501 plus :988) and `../tools` only at :529.

## Impact
Hygiene guards claim workspace-wide coverage but skip four tool crates; the divergent loops will drift further as guards are added.

## Related
#6351, #6366, #6372, #6373 (the guards that duplicated the walk).

## Suggested Fix
Add one `production_scan_roots()` (bin `src` + `crates/*/src` + `tools/*/src`) and a test-file-skipping visitor beside `hygiene_scan_roots()` / `visit_workspace_rs_files`, and route the five scans through it. Run the suite afterwards: the newly covered `tools/` crates may surface real hits to fix or allowlist.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / sibling call sites
- [ ] **TESTS**: A regression test pins this specific fix

