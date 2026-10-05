# #5328 — PEX-D4-2026-10-05-03: FO4+ Papyrus grammar forms are rejected or truncated — namespaced ScriptName/Extends/Import, remote-event declarations, and `new Struct`

- **Labels**: medium,scripting,game:fo4,bug
- **Filed from**: `docs/audits/AUDIT_PAPYRUS_2026-10-05.md` (2026-10-05 comprehensive suite)
- **URL**: https://github.com/matiaszanolli/ByroRedux/issues/5328

- **Severity**: MEDIUM
- **Dimension**: Papyrus Lexer & Parser
- **Location**:
  - `crates/papyrus/src/parser/script.rs:100` (script name), `:104` (`Extends` target) and `:194`
    (`Import` target). All three use `expect_ident_raw`, not `parse_qualified_ident`.
  - `script.rs:320` + `:338` (`Event` name, then `(` is expected immediately).
  - `crates/papyrus/src/parser/expr.rs:235-248` (`parse_new_expr` always requires `[size]`).
- **Status**: NEW
- **Untrusted-Input**: Yes
- **Description**: `docs/engine/papyrus-parser.md:115-116` says namespaces are "parsed by
  `parse_qualified_ident`". Its grammar summary gives `import ::= "Import" qualified_ident`. In the
  code, only *type* positions and expression primaries go through `parse_qualified_ident`. The
  header, `Extends` and `Import` read one bare identifier and leave `:rest` as a stray token. Two other
  FO4 forms from the falloutck wiki are also unparseable:
  - remote-event handlers, `Event <Type>.<EventName>(<Type> akSender, …)`
  - struct creation without a size, `Point myPoint = new Point`. The `.pex` side lowers
    `StructCreate` to `New` with size 0, so the AST can already represent it.
- **Evidence**:

  | Input | Result | errors |
  |---|---|---|
  | `ScriptName DLC01:Foo Extends ObjectReference` | `name = "DLC01"`, `parent = None` | 1 |
  | `ScriptName Foo Extends DLC01:Base` | **`parent = Some("DLC01")`** | 1 |
  | `Import DLC01:Utils` | `Import("DLC01")` | 1 |
  | `Event Actor.OnDeath(Actor akSender, Var[] akArgs)` … `EndEvent` | event dropped ("expected (, found '.'") | 2 |
  | `S v = new S` | function dropped ("expected '[' after new type") | 1 |

  On disk, `Weapon.psc`, `Actor.psc` and `UI.psc` (F4SE) fail on `new InstanceData:Owner` and
  `new MenuData`. Together with finding 02, that makes **5 of the 29** on-disk FO4 `.psc` files that
  fail to parse cleanly.
- **Impact**: Any FO4, FO76 or Starfield `.psc` that uses namespaces, remote events or struct
  creation parses with errors, and with a wrong `name`/`parent`. All three forms are idiomatic in
  FO4+ content. Errors are reported, so this is not silent, and it is MEDIUM. The parent truncation
  (`"DLC01"`) is a wrong value, though, not a missing one. The decompiler emits remote events as
  `Event OnDeath` (it strips `::remote_`), so the two frontends cannot round-trip FO4 remote handlers.
  Reachability is the same as finding 01.
- **Related**: findings 01 and 02. This is the parser-side counterpart of the FO4 `.pex` coverage in
  `/audit-scripting` Dim 1.
- **Suggested Fix**: Use `parse_qualified_ident` for the script name, the `Extends` target and the
  `Import` target, keeping the #5021 same-line check before each. Accept an optional `Ident '.'`
  prefix on event names (store the sender type, or mangle it the same way the `.pex` `::remote_`
  form is). Make `[size]` optional in `parse_new_expr` when the type is an object (struct) type. Add
  an FO4 round-trip fixture.


_Source: `AUDIT_PAPYRUS_2026-10-05.md` (PEX-D4-2026-10-05-03), audit HEAD `a2c24b16e`._

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files / call sites
- [ ] **TESTS**: A regression test pins this specific fix
