# PAR-D6-2026-09-29-01: #4673's `installdir` containment is bypassable on Windows (regression of #4673)

**Labels**: low,bug,import-pipeline,safety

**Source report**: `docs/audits/AUDIT_PARSERS_2026-09-29.md`

**Regression of #4673** — the fix for closed #4673 is incomplete; this is filed as a new issue rather than reopening.

- **Severity**: LOW
- **Dimension**: I/O & Paths
- **Location**: `crates/game-detect/src/steam.rs:163-182` (`installs_in_library`), test `absolute_or_escaping_installdir_is_rejected`
- **Status**: Regression of #4673 (incomplete fix, `08b72eede`; policy owner `/audit-tooling` Dim 5)
- **Trigger Input**: an `appmanifest_*.acf` whose `installdir` is `\Somewhere` (rooted, no prefix) or `D:Somewhere` (prefix, no root), on Windows.
- **Description**:
  - The check rejects only `authored.is_absolute()` or a `ParentDir` component, then runs `steamapps.join("common").join(install_dir)`.
  - The `std::path` docs say that on Windows `\temp` and `c:temp` are **not** absolute.
  - `join` with a rooted prefixless path "replaces everything except for the prefix", and with a prefixed rootless path it "replaces self". Both forms pass the check and escape `steamapps\common`.
  - The test's `..\..\evil` case passes on Linux for the wrong reason. Backslash is not a separator there, so the value is one `Normal` component and the join stays contained. The test goes green only because that directory does not exist.
- **Evidence**: see the location; `Path::is_absolute` and `Path::join` docs (Windows semantics).
- **Impact**: on Windows, a tampered Steam manifest can make the launcher or `byro-detect` report an install anywhere on the drive. This needs a modified Steam install.
- **Related**: #4673, PAR-D6-2026-09-21-04, #4759
- **Suggested Fix**:
  - Accept only a single `Component::Normal` after splitting on both `/` and `\` on every platform.
  - Add Windows-shaped cases (`\x`, `C:x`) that assert on the check itself, not on the directory being absent.

**Validated at HEAD 9fcfdc3fc**: `installs_in_library` in `crates/game-detect/src/steam.rs` still rejects only `authored.is_absolute()` or a `ParentDir` component before `steamapps.join("common").join(install_dir)`; Windows rooted-prefixless / prefixed-rootless forms are not rejected.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)
- [ ] **TESTS**: A regression test pins this specific fix
