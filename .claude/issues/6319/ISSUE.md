# #6319: FNV-2026-10-21-03: `scripts/run-real-data-pins.sh` reports three passing FNV pins as "green no-op" failures because it counts `test … ok` lines that `--nocapture` splits

**Labels**: low,bug,game:fnv,legacy-compat,tech-debt

**Source**: docs/audits/AUDIT_FNV_2026-10-21.md · **Severity**: LOW · **Dimension**: 6. **Owner**: `/audit-tooling` (audit / CI infrastructure). · **Location**: `scripts/run-real-data-pins.sh` (the `ran_ok=$(grep -cE '^test .* \.\.\. ok$' …)` check after the `cargo test … --nocapture` call; introduced by `9ee74b6e7`, #6183).

- **Severity**: LOW
- **Dimension**: 6. **Owner**: `/audit-tooling` (audit / CI infrastructure).
- **Location**: `scripts/run-real-data-pins.sh` (the `ran_ok=$(grep -cE '^test .* \.\.\. ok$' …)` check after the `cargo test … --nocapture` call; introduced by `9ee74b6e7`, #6183).
- **Status**: NEW. Related: #6182 (open; no runner registered, so this local script is the lane that actually runs).
- **Description**: A pin whose test prints anything writes its output between `test NAME ... ` and `ok`, so the line never matches `^test .* \.\.\. ok$`. The runner then declares the pin a green no-op and exits 1. `.github/workflows/real-data-gates.yml:225-228` already avoids this for its own jobs ("under --nocapture a test's own output can split its `test ... ok` line") by summing `test result: ok. N passed`. The pin runner duplicates the check without that fix.
- **Evidence**: `scripts/run-real-data-pins.sh fnv` → `ERROR: closed-HIGH pin '…' matched nothing (green no-op)` for `parse_real_fnv_dial_infos_populated`, `fnv_armo_female_meshes_are_parsed_and_selected` and `fnv_leveled_item_multi_pick_semantics_are_pinned_on_the_shipped_master`, each with `test result: ok. 1 passed`. Example from `target/real-data-gates/pins-fnv_armo_female_meshes_are_parsed_and_selected.log` lines 5–6: `test fnv_armo_female_meshes_are_parsed_and_selected ... [#3416] FNV ARMO: 389 total, …` / `ok`. The other 13 FNV pins pass.
- **Impact**: The documented "run every closed-HIGH pin" command can never print "all pins green" on a clean tree. A real failure is therefore hidden among the false ones. Pins for other games that print are presumably hit the same way (not measured).
- **Suggested Fix**: Count with the workflow's expression (`grep -oE 'test result: ok\. [0-9]+ passed'`, summed), ideally from one shared helper that both the workflow and the script call.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other shader types, other block parsers)

_Source: `docs/audits/AUDIT_FNV_2026-10-21.md` (HEAD 4a3e89d56). Fix with `/fix-issue <number>`._
