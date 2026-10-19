# FNV-2026-10-18-07: Two more bin-crate hygiene guards are red (debug-cli.md command count, example-target LOC ceiling)

- **Issue**: #6056 (https://github.com/matiaszanolli/ByroRedux/issues/6056)
- **Labels**: low, documentation, doc-rot, tech-debt, game:fnv, legacy-compat
- **Source**: docs/audits/AUDIT_FNV_2026-10-18.md (snapshot as filed; GitHub is authoritative for state)

Found by the FNV compatibility audit of 2026-10-18 (`docs/audits/AUDIT_FNV_2026-10-18.md`, HEAD `af5063597`).

- **Origin**: D1-02 + D5-04 (merged). **Status**: NEW. **Owner**: `/audit-tooling` / `/audit-tech-debt`.
- **Evidence**: `commands::debug_cli_doc_tests::every_registered_command_is_named_in_debug_cli_docs`: the registry has 130 commands, `docs/engine/debug-cli.md:337` says 127 (new `obscript.objects`, `obscript.cmd`, `script.requests` from the M47.3 commits). `workspace_hygiene_tests::example_targets_hold_their_per_crate_ceilings`: `scripting: 13 files / 4852 LOC exceeds ceiling 13 files / 4777 LOC` (`byroredux/src/workspace_hygiene_tests.rs:983`; #5856's own guidance applies).
- **Suggested Fix**: update the doc count and list the new commands; fold the new example code into `inspect.rs` subcommands or raise the ceiling in the same commit.

**Related**: closed #4756 / #5150 fixed earlier `debug-cli.md` count drift; this is new drift after the 2026-10-18 M47.3 commits.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / the other FO3 / Oblivion / Skyrim paths that share this code
- [ ] **TESTS**: A regression test (or a repaired gate) pins this specific fix

