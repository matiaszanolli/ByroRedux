# #5147: TOOL-D1-2026-09-29-04: `create_new` with non-unique default names — a second default `tex.dump` always fails, and two auto-named screenshots in the same second collide

**Labels**: low, bug, tech-debt

**Source report**: `docs/audits/AUDIT_TOOLING_2026-09-29.md`
**Severity**: LOW
**Dimension**: Debug Trust Boundary (exposure: developers)

## Location
- `byroredux/src/commands/assets.rs` (`tex.dump` default `"tex_dump.png"`, `create_new`)
- `crates/debug-server/src/system.rs` (`screenshot_<unix secs>.png` default; `write_screenshot` `create_new`)

## Description
`63c0aee3b` correctly switched both writers to `create_new`, so neither can overwrite a file. Their default names were not made unique:
- `tex.dump <bsa> <tex>` without an `out.png` fails from the second call on, with "write '…/texture-dumps/tex_dump.png': File exists".
- Two bare `screenshot` commands in the same wall-clock second get the same name, and the second one fails.

## Evidence
`unwrap_or_else(|| "tex_dump.png".to_string())` + `.create_new(true)` in assets.rs; `format!("screenshot_{}.png", <secs>)` + `.create_new(true)` in system.rs.

## Impact
Minor usability regression: the user has to clean up `texture-dumps/` or pass explicit names.

## Related
#4752, `63c0aee3b`. Label gap: debug server has no own label → `tech-debt`.

## Suggested Fix
Suffix the default with a counter or nanoseconds, or pick the next free `_N` name when the default already exists.

Validated at HEAD 9fcfdc3fc: both default names and both `create_new(true)` opens are present.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **TESTS**: A regression test pins this specific fix
