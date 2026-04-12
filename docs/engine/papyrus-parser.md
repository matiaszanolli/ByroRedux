# Papyrus Language Parser

The `byroredux-papyrus` crate parses Papyrus `.psc` source files into a typed AST.
It does not execute anything — the AST feeds a future transpiler that generates
ECS component definitions + system functions for legacy mod compatibility.

**Crate:** `crates/papyrus/` | **Milestone:** M30 | **Tests:** 45

## Why a Parser?

ByroRedux replaces the Papyrus VM with ECS-native scripting (see
[Scripting Architecture](scripting.md)), but legacy mods distribute `.psc` source
files that define script behavior. To support these mods, we need to:

1. **Parse** `.psc` files into a structured AST
2. **Transpile** the AST into ECS component definitions + system functions (future)
3. Optionally **interpret** `.pex` bytecode for mods that ship only compiled scripts (stretch goal)

This crate handles step 1. The grammar is compatibility-critical — mod scripts
depend on exact interpretation, so we use a proper parser (not ad-hoc string splitting).

## Architecture

```
.psc source text
     │
     ├── preprocess()     Remove \ line continuations, build offset map
     │
     ├── lex()            logos tokenizer → Vec<LexedToken>
     │                    Case-insensitive keywords, skip comments, preserve doc comments
     │
     └── Parser           Hand-written recursive descent
          ├── parse_expr()     Pratt parser (precedence climbing)
          ├── parse_stmt()     If/While/Return/Assign/VarDecl  (Phase 2)
          ├── parse_script()   Full .psc file                  (Phase 3)
          └── error recovery   Skip to End* keywords           (Phase 4)
```

### Why logos + hand-written recursive descent?

- **logos** handles case-insensitive keyword matching natively via `ignore(ascii_case)`,
  produces token spans for free, compiles to a jump table (no runtime regex overhead).
- **Hand-written recursive descent** gives full control over error messages and recovery.
  Papyrus has a simple, unambiguous grammar — no need for parser generators.
- Error quality matters: modders will see parse errors. We control every diagnostic.

## Papyrus Grammar Summary

```
ScriptName <id> [extends <id>] [Native] [Const] [DebugOnly] [Hidden]
Body: (Import | Variable | Property | Function | Event | State | Struct | CustomEvent | Group)*
```

**Types:** `Bool`, `Int`, `Float`, `String`, `Var` (FO4+), object types (`Actor`, `Quest`...),
arrays (`Int[]`, `Actor[]`), structs (FO4+).

**Operator precedence:** `||` → `&&` → comparison → `+/-` → `*/%` → unary(`-`,`!`) → cast(`as`) → dot(`.`) → array(`[]`) → atoms.

**Keywords are case-insensitive.** Identifiers are case-preserving.

**Comments:** `;` single-line, `;/ ... /;` block, `{ ... }` doc comments.

**Line continuation:** `\` at end of line joins the next line.

**Namespaces (FO4+):** colon-delimited — `MyNamespace:MyScript:MyStruct`.

## AST Types

Every node carries a `Span` (byte offset range) for diagnostics. Key types:

| AST Node | Represents |
|----------|-----------|
| `Script` | Top-level: name, parent (extends), flags, body items |
| `ScriptItem` | Import, Variable, Property, Function, Event, State, Struct, CustomEvent, Group |
| `Type` | Bool, Int, Float, String, Var, Object(id), Array(Type) |
| `Property` | Typed field with Auto/Const/Mandatory flags, optional get/set |
| `Function` / `Event` | Return type, params (with defaults), flags, body statements |
| `State` | Named mode with per-state function/event handlers |
| `Stmt` | Assign, Return, If/ElseIf/Else, While, ExprStmt, VarDecl |
| `Expr` | Literals, idents, member access, indexing, calls, unary/binary ops, cast, new |

## File Layout

```
crates/papyrus/
  src/
    lib.rs              Public API: parse_expr()
    ast.rs              All AST types
    span.rs             Span, Spanned<T>
    token.rs            Token enum (logos derive)
    lexer.rs            Preprocessor + lexer wrapper
    error.rs            ParseError with diagnostics
    parser/
      mod.rs            Parser struct, type parsing
      expr.rs           Pratt expression parser
```

## Current Status (Phase 1)

- Token enum with all Papyrus keywords (case-insensitive), operators, literals
- Lexer wrapper: line continuation removal, single-line/block/doc comments
- Full AST type definitions for the entire Papyrus language
- Pratt expression parser with correct precedence
- 45 tests covering lexer and expression parsing

## Planned Phases

| Phase | Scope | Status |
|-------|-------|--------|
| 1 | Lexer + expression parser | Done (45 tests) |
| 2 | Statements + function bodies | Pending |
| 3 | Script-level declarations (properties, states, imports) | Pending |
| 4 | FO4 extensions + error recovery + real-corpus integration tests | Pending |
| 5 | Console integration (`psc` command) | Pending |

## ECS Transpilation Target (Future)

The AST maps cleanly to ECS concepts:

| Papyrus | ECS Equivalent |
|---------|---------------|
| Property (Auto) | Component field |
| Event (OnActivate, OnHit...) | Marker component + system |
| State (GoToState) | Enum field on component |
| Function (Global) | Standalone system function |
| Function (instance) | Method taking entity ID |
| Inheritance (extends) | Component composition |

See [Scripting Architecture](scripting.md) for the full mapping and
[Papyrus API Reference](../legacy/papyrus-api-reference.md) for the complete
API surface (101 script types, 136 events).

## References

- [Papyrus Introduction](https://falloutck.uesp.net/wiki/Papyrus_Introduction)
- [Script File Structure](https://falloutck.uesp.net/wiki/Script_File_Structure) (`.psc` grammar)
- [Expression Reference](https://falloutck.uesp.net/wiki/Expression_Reference)
- [Events Reference](https://falloutck.uesp.net/wiki/Events_Reference)
- Internal: [Scripting Architecture](scripting.md), [Papyrus API Reference](../legacy/papyrus-api-reference.md)
