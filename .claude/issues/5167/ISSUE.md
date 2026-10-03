# #5167 — TOOL-D5-2026-10-02-02: `byro-detect` ignores unknown flags, so a mistyped `--profiles` sends `--write` to the default file

Labels: low,tech-debt,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/5167

From `docs/audits/AUDIT_TOOLING_2026-10-02.md` (HEAD `e737f06bf`).

- **Severity**: LOW
- **Dimension**: Install Detection
- **Exposure**: CLI users of `byro-detect`
- **Location**: `tools/byro-detect/src/main.rs:26-34`, `:131-136` (`value_of`), `:147-158` (`print_usage`)
- **Status**: NEW. It is on the skill checklist but was never filed.
- **Description**: Flags are parsed with `args.iter().any(..)` and a positional `value_of`.
  - `byro-detect --write --profile /tmp/p.toml` (singular) ignores `--profile` and merges into the real `~/.byroredux/profiles.toml`. That hits TOOL-D5-01's comment loss too.
  - `--profiles --write` takes `"--write"` as the path, and the write is off.

  `--help` documents neither behaviour.
- **Evidence**: `value_of` returns `args.get(index + 1)` without checking that the value is not itself a flag. No branch rejects an unrecognised argument.
- **Impact**: A typo silently changes which file gets rewritten.
- **Related**: TOOL-D5-01
- **Suggested Fix**: Reject unknown arguments, and values that start with `--`, with a usage error and a non-zero exit.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / frontends / request variants named above)
- [ ] **TESTS**: A regression test pins this specific fix

