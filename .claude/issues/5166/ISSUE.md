# #5166 — TOOL-D5-2026-10-02-01: Every Play rewrites `~/.byroredux/profiles.toml` through `toml::Table`, dropping the user's comments and formatting

Labels: low,tech-debt,bug
URL: https://github.com/matiaszanolli/ByroRedux/issues/5166

From `docs/audits/AUDIT_TOOLING_2026-10-02.md` (HEAD `e737f06bf`).

- **Severity**: LOW
- **Dimension**: Install Detection
- **Exposure**: every launcher user with a hand-curated profiles file
- **Location**: `tools/byro-launcher/src/app.rs:79` (`self.state.remember()` before each Play); `tools/byro-launcher/src/state.rs:156-158`; `crates/game-detect/src/overrides.rs:88-121`
- **Status**: NEW. It is on the skill checklist but was never filed.
- **Description**: `merge_into_file` parses the file into a `toml::Table`, replaces `[roots]`, and serializes it again. Values are preserved. Comments, key order and formatting in the user's `[profiles.*]` and `[defaults]` blocks are lost. The launcher does this on every Play, not only after detection changes. The doc comment says those blocks "survive verbatim — including comments' absence being the only casualty", which contradicts itself.
- **Evidence**: `overrides.rs:98` `let mut document: toml::Table = … toml::from_str(&text)`, and `:121` `document.insert(ROOTS_TABLE…)`, followed by re-serialization.
- **Impact**: Users lose their annotations without any message. Severity is LOW because no value is lost.
- **Related**: TOOL-D4-01 (another write-on-every-action), #4758
- **Suggested Fix**: Edit only `[roots]` with `toml_edit::DocumentMut`, which is already in `Cargo.lock` transitively. Skip the write when `[roots]` is unchanged, and correct the doc comment.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (the other writers / frontends / request variants named above)
- [ ] **TESTS**: A regression test pins this specific fix

