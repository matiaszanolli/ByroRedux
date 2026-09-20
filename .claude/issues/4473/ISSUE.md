# PEX-D3-2026-09-19-02: case-colliding state names both get is_auto: true (malformed .pex only)

- **ID**: D3-02
- **Labels**: medium,scripting,bug
- **Filed from**: docs/audits/AUDIT_PAPYRUS_2026-09-19.md
- **GitHub**: https://github.com/matiaszanolli/ByroRedux/issues/4473

**Severity**: MEDIUM (wrong-AST property with no current consumer; input not compiler-producible) · **Dimension**: Decompiler Boolean/Control-Flow/Lower (script assembly) · **Untrusted-Input**: Yes (requires a hand-assembled/hostile `.pex`)
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D3-2026-09-19-02) · **Location**: `crates/pex/src/decompile/lower.rs:408-410,450-463`

**Description**
`is_auto_state` matches case-insensitively (#3786 — correct and retained), so a `.pex` carrying two states whose names differ only in case (e.g. `waiting` + `WAITING`, `auto_state_name = "Waiting"`) marks **both** `State` items `is_auto: true` — an AST no Papyrus source can express (the compiler rejects duplicate case-insensitive state names, so only a hand-assembled/hostile `.pex` reaches it). Champollion's case-sensitive comparison marks at most one.

**Evidence**
Adversarial probe case C (AUDIT_PAPYRUS_2026-09-19, dim-3): `body=[Ev OnInit; State(AUTO) waiting [Ev OnActivate]; State(AUTO) WAITING [Ev OnUpdate]]`.

**Impact**
Cosmetic today — no recognizer or runtime consumer reads `State::is_auto` (repo-wide grep: producers + tests only); vanilla corpora cannot contain the input (26,641 scripts, 0 shape mismatches). Becomes a HIGH-class wrong-AST-accepted-by-consumer only if a state-aware consumer boots on `is_auto` without its own case handling.

**Related**: #4319 (assembly rule), #3786/#3943

**Suggested Fix**
In the named-state arm, mark `is_auto` on the *first* case-insensitive match and, when `auto_state_name` is non-empty, skip duplicate case-collisions — or document the malformed-input behavior at `is_auto_state`.

## Completeness Checks
- [ ] **SIBLING**: Check the corpus smoke's `expected_top_level_item_count` agrees with the dedup rule
- [ ] **TESTS**: A regression test pins one-auto-only under a case collision
