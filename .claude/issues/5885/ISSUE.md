# #5885: PEX-D4-2026-10-15-03: a qualified ident's segment after `:` is pulled up from the next line with 0 errors, now including the ScriptName, Extends, Import and event sender (#5021 class)

- **Labels**: low,bug,scripting
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-15.md`

- **Severity**: LOW
- **Dimension**: Papyrus Lexer & Parser
- **Location**: `crates/papyrus/src/parser/mod.rs:421-427`. The `:` test is raw (`check_raw`), but the segment after it goes through the newline-skipping `expect_ident`. e96d687ec routes `parse_script_header` (`crates/papyrus/src/parser/script.rs:107-113`), `parse_import` (`:221-222`) and `parse_event_name` (`:377-378`) through it.
- **Status**: NEW (sibling of open #5445, closed #5021 / #5331)
- **Untrusted-Input**: Yes
- **Description**: e96d687ec guards only the first segment with `expect_same_line`. A segment after a line-ending `:` is a mandatory operand, and the parser takes it from the next line.
- **Evidence** (probe, all **0 errors**):
  - `ScriptName DLC01:` ⏎ `Foo` gives name `DLC01:Foo`.
  - `ScriptName Foo Extends DLC01:` ⏎ `Base` gives parent `DLC01:Base`.
  - `Import DLC01:` ⏎ `Utils` gives an import of `DLC01:Utils`.
  - `Event DLC01:` ⏎ `Q.OnX(…)` gives an event named `DLC01:Q.OnX`.
  - The type and expression positions behave the same way, pre-existing: `DLC01:` ⏎ `Bar x` gives `Var x`, and `parse_expr("a:\nb")` gives `Ident("a:b")`.
  - The new pin `a_namespaced_parent_on_the_next_line_is_still_not_the_parent` covers only a newline *before* the first segment.
- **Impact**: The parser accepts source the reference compiler rejects. The glued reading is what the author meant, so this is LOW, as with #5021.
- **Suggested Fix**: Use `expect_ident_raw` for every segment after `:` in `parse_qualified_ident`, and pin the header forms in `a_mandatory_operand_on_the_next_line_is_an_error`.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in the sibling producers/paths named in the finding
- [ ] **UNTRUSTED-INPUT**: The fix keeps every path bounded (no panic / OOB / unbounded alloc, recursion or CPU) on hostile `.pex`/`.psc`
- [ ] **TESTS**: A regression test pins this specific fix
