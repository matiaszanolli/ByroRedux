# #6152: TD2-2026-10-19-07: Five scripting resources each hand-build a lowercased-EDID -> FormID table with their own `from_index`, lookup and `install_*`
STATE: OPEN
LABELS: bug,low,tech-debt,scripting

- **Severity**: LOW
- **Dimension**: 2 - Logic Duplication
- **Location**: `crates/scripting/src/idle_names.rs:19-35` (`IdleNames`), `sound_requests.rs:30-62` (`SoundEditorIds`), `actor_value_names.rs:12-41` (`ActorValueNames`), `obscript_more/extras.rs:34-58` (`MusicTracks`), `messages.rs:88-98` (globals in `MessageCatalog`)
- **Age**: 2026-10-13..18
- **Effort**: small
- **Description**: each is `HashMap<String, V>` keyed by `editor_id.to_ascii_lowercase()`, filtered on non-empty EDID, queried with a second `to_ascii_lowercase()`, installed by a `pub fn install_*` that returns the count. Case folding on both sides is the part that must stay consistent.
- **Proposed fix**: `pub struct EditorIdTable<V>(HashMap<String, V>)` with `from_records(iter)` and `get(name)` in a new small `crates/scripting/src/name_table.rs`; the five resources wrap it (their extra indexes — `by_form`, the `av` prefix alias — stay local).

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the related files named above
- [ ] **TESTS**: A regression test / source-scan guard pins this specific fix (or the finding is a pure move/delete covered by existing tests)

_Source: `docs/audits/AUDIT_TECH_DEBT_2026-10-19.md` (HEAD 12677d718). Fix with `/fix-issue <number>`._

