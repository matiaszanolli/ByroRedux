# #7112 — ESM-D6-02: #6797 lowercase `strings/` probe does nothing for a bare plugin filename (`read_dir("")`)

Labels: low,esm-plugin,bug
Filed from: docs/audits/AUDIT_ESM_2026-11-20.md (finding ESM-D6-02)

Source: `docs/audits/AUDIT_ESM_2026-11-20.md` (`/audit-esm`, HEAD 47ae285c6 / f3713bf5e) · ID **ESM-D6-02**

- **Severity**: LOW
- **Dimension**: 6
- **Record / Sub-record**: n/a (loose-override table discovery)
- **Location**: `crates/plugin/src/esm/strings_table.rs:370-386` (`plugin_dir = plugin_path.parent().unwrap_or(".")`, then `std::fs::read_dir(plugin_dir).ok()...map_or(exact, ...)`)
- **Status**: NEW (incomplete fix of #6797, the directory half of #4176; the same empty-parent trap #5303 fixed for archive discovery in `strings_archive_directory`)
- **Description**: `Path::new("Skyrim.esm").parent()` is `Some("")`, so `unwrap_or` never fires; `Path::new("").join("Strings")` resolves relative to the cwd (the exact-case probe works), but the fallback `std::fs::read_dir("")` fails with `NotFound`, `.ok()` swallows it, and `map_or(exact, ...)` keeps the non-existent `Strings` path. A `strings/` or `STRINGS/` directory next to a bare-named plugin is therefore never found and the archive wins, the inverted override order #6797 set out to close. The bare name is the documented smoke/CLI convention (`--esm Skyrim.esm` with cwd = Data, #5303), and `load_order.rs:968` passes the path through unmodified. The regression test uses an absolute `temp_dir()` path, so it cannot see this.
- **Evidence** (scratch crate `dirprobe` linking `byroredux-plugin`, fixture = loose `testmod_english.strings` id 1 = "Loose Override", archive callback returns "Archive" for `.strings`):
  - cwd with `strings/`: `TestMod.esm` -> `Some("Archive")`; `./TestMod.esm` -> `Some("Loose Override")`; absolute -> `Some("Loose Override")`.
  - cwd with `STRINGS/`: identical (`Archive` for the bare name).
  - cwd with `Strings/`: all three spellings -> `Loose Override`.
- **Impact**: on a case-sensitive filesystem a mod's lowercase-directory string override is silently ignored when the engine is launched with a bare plugin name from the Data directory; #4073's zero-table warning does not fire because the archive supplies tables.
- **Suggested fix**: normalise the parent exactly as `strings_archive_directory` does (`Some(p) if !p.as_os_str().is_empty() => p, _ => Path::new(".")`) before both the `Strings` join and the `read_dir`; extend `loose_companion_file_is_found_in_a_lowercase_strings_directory` with a bare-name variant (run it from a fixture cwd or factor the directory resolution into a function taking an explicit base).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related decoders / other games' arms (per-game layout splits, other callers of the same helper)
- [ ] **TESTS**: A regression test pins this specific fix (synthetic fixture in the default lane; real-data pins stay in the ignored lane)
