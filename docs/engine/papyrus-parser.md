# Papyrus Language Parser

The `byroredux-papyrus` crate parses Papyrus `.psc` source files into a typed AST.
It does not execute anything — the AST feeds a future transpiler (M47.2) that
generates ECS component definitions + system functions for legacy mod compatibility.

**Crate:** `crates/papyrus/` (package `byroredux-papyrus`) | **Milestones:** M30 Phase 1
(lexer + Pratt expressions) + M30.2 (full `.psc` → AST) | **Tests:** 73 (69 inline +
4 integration round-trips)

> **Status (2026-05-28).** M30 Phase 1 and M30.2 are both **closed**. The crate now
> parses a complete `.psc` file — header, top-level items, statements, expressions —
> with per-item error recovery. The expression layer is already in production use as
> the query language for the debug protocol (`crates/debug-server/src/evaluator.rs`
> calls [`parse_expr`](#public-api)). Remaining work (semantic validation,
> doc-comment threading on non-doc-aware items, the actual AST→ECS transpiler) is
> tracked under M47.2 in [ROADMAP.md](../../ROADMAP.md).

## Why a Parser?

ByroRedux replaces the Papyrus VM with ECS-native scripting (see
[Scripting Architecture](scripting.md)), but legacy mods distribute `.psc` source
files that define script behavior. To support these mods, we need to:

1. **Parse** `.psc` files into a structured AST — done (this crate)
2. **Transpile** the AST into ECS component definitions + system functions — M47.2
3. Optionally **interpret** `.pex` bytecode for mods that ship only compiled scripts
   (stretch goal — not started)

This crate handles step 1. The grammar is compatibility-critical — mod scripts
depend on exact interpretation, so we use a proper parser (not ad-hoc string splitting).

The R5 quest-prototype evaluation (closed 2026-05-16) confirmed the transpiler target:
a Papyrus event handler maps to ECS marker components + dt-driven systems, with latent
waits splitting a handler into a before-wait and an after-wait system. M30.2 round-trips
all four R5 reference scripts end-to-end (see [Integration tests](#integration-tests)).

## Architecture

```
.psc source text
     │
     ├── preprocess()     Remove \ line continuations, build OffsetMap
     │
     ├── lex()            logos tokenizer → Vec<LexedToken>
     │                    Case-insensitive keywords, skip line/block comments,
     │                    preserve { doc comments }, keep Newline tokens significant
     │
     └── Parser           Hand-written recursive descent
          ├── parse_expr()     Pratt parser (precedence climbing, depth-capped)
          ├── parse_stmt()     Return / If / While / VarDecl / Assign / ExprStmt
          ├── parse_script()   Full .psc file (header + top-level items)
          └── error recovery   skip_to_next_line() between malformed items
```

Two public entry points in `lib.rs`:

- `parse_expr(&str) -> Result<Spanned<Expr>, Vec<ParseError>>` — a single expression
  (used for testing and by the debug-protocol evaluator).
- `parse_script(&str) -> Result<(Script, Vec<ParseError>), Vec<ParseError>>` — a whole
  `.psc` file. Returns `Ok((Script, recovered_errors))` even when some items had
  recoverable parse errors, so a tolerant caller gets the partial `Script` while a
  strict caller can check `result.1.is_empty()`. Only a missing `ScriptName` header or
  a fatal lex error returns `Err`. Spans in the returned AST are remapped back to
  original-source coordinates via the `OffsetMap`.

### Why logos + hand-written recursive descent?

- **logos** handles case-insensitive keyword matching natively via `ignore(ascii_case)`,
  produces token spans for free, compiles to a jump table (no runtime regex overhead).
- **Hand-written recursive descent** gives full control over error messages and recovery.
  Papyrus has a simple, unambiguous grammar — no need for parser generators.
- Error quality matters: modders will see parse errors. We control every diagnostic.

## Papyrus Grammar Summary

```
ScriptName <id> [Extends <id>] [Native] [Const] [DebugOnly] [Hidden]
Body: (Import | Variable | Property | Function | Event | State | Struct | CustomEvent | Group)*
```

**Types:** `Bool`, `Int`, `Float`, `String`, `Var` (FO4+), object types (`Actor`, `Quest`...),
arrays (`Int[]`, `Actor[]`), structs (FO4+). An array type is *only* `Base[]` with empty
brackets — `expr[index]` after a base type is rewound and re-parsed as an index expression.

**Operator precedence** (lowest → highest binding, from `BinaryOp::precedence()` +
the Pratt postfix levels in `parser/expr.rs`):

```
||  →  &&  →  comparison (== != < <= > >=)  →  + - (level 4)  →  * / % (level 5)
   →  unary(-, !)  →  cast(as)  →  postfix(. [] ())  →  atoms
```

(The grammar reserves a string-concatenation `StrCat` binary op at the same level as
`+`/`-`, but the lexer has no dedicated `+`-vs-concat distinction yet, so it is declared
in the AST and unused by the parser today.)

**Keywords are case-insensitive** (`ignore(ascii_case)` on every `#[token]`). Identifiers
are case-preserving; comparison goes through `Identifier::eq_ignore_case`.

**Comments:** `;` single-line, `;/ ... /;` block, `{ ... }` doc comments. Line and block
comments are skipped at lex time; doc comments survive as `Token::DocComment(String)`.

**Line continuation:** `\` at end of line joins the next line. Handled in `preprocess()`
before lexing; an `OffsetMap` records the removed bytes so diagnostics still point at the
original source.

**Newlines are significant.** `Token::Newline` is preserved and acts as the statement
terminator. `Parser::peek()` transparently skips newlines; `peek_raw()` does not — the
distinction is load-bearing for empty-`Return` detection (see [Pitfalls](#pitfalls)).

**Namespaces (FO4+):** colon-delimited — `MyNamespace:MyScript:MyStruct`, parsed by
`parse_qualified_ident` and folded into a single `Identifier` with embedded `:`.

## AST Types

Every node carries a `Span` (byte offset range, in `span.rs`) for diagnostics, wrapped in
`Spanned<T>`. Bitflag sets use the `bitflags` crate. Key types (`ast.rs`):

| AST Node | Represents |
|----------|-----------|
| `Script` | `name`, `parent` (Extends), `flags: ScriptFlags`, `body: Vec<Spanned<ScriptItem>>` |
| `ScriptItem` | `Import` / `Variable` / `Property` / `Function` / `Event` / `State` / `Struct` / `CustomEvent` / `Group` |
| `Type` | `Bool`, `Int`, `Float`, `String`, `Var`, `Object(Identifier)`, `Array(Box<Type>)` |
| `Variable` | typed field: `ty`, `name`, `initial_value`, `is_conditional`, `is_const` |
| `Property` | typed field, `flags: PropertyFlags`, `initial_value`, optional `getter`/`setter`, `doc_comment` |
| `Function` | `return_type`, `name`, `params: Vec<Param>`, `flags: FunctionFlags`, `body: Vec<Spanned<Stmt>>`, `doc_comment` |
| `Event` | like `Function` but no return type |
| `Param` | `ty`, `name`, optional `default` expression |
| `State` | `name`, `is_auto`, `body: Vec<Spanned<StateItem>>` |
| `StateItem` | `Function` or `Event` |
| `Struct` | `name`, `members: Vec<Variable>` (FO4+) |
| `Group` | `name`, `flags: GroupFlags`, `properties: Vec<Spanned<Property>>` |
| `Stmt` | `Assign{target, op, value}`, `Return(Option<Expr>)`, `If{condition, body, elseif_clauses, else_body}`, `While{condition, body}`, `ExprStmt`, `VarDecl(Variable)` |
| `AssignOp` | `Eq`, `PlusEq`, `MinusEq`, `MulEq`, `DivEq`, `ModEq` |
| `Expr` | `IntLit`, `FloatLit`, `BoolLit`, `StringLit`, `NoneLit`, `Ident`, `MemberAccess`, `Index`, `Call{callee, args}`, `UnaryOp`, `BinaryOp`, `Cast`, `New{ty, size}`, `ArrayLit`, `ParentAccess` |
| `CallArg` | optional `name` (named argument `name = value`) + `value` |
| `UnaryOp` | `Neg`, `Not` |
| `BinaryOp` | `Or`, `And`, `Eq`, `Ne`, `Lt`, `Le`, `Gt`, `Ge`, `Add`, `Sub`, `Mul`, `Div`, `Mod`, `StrCat` |

### Flag bitsets

| Bitset | Flags |
|--------|-------|
| `ScriptFlags` | `NATIVE`, `CONST`, `DEBUG_ONLY`, `HIDDEN` |
| `PropertyFlags` | `AUTO`, `AUTO_READ_ONLY`, `CONST`, `MANDATORY`, `HIDDEN`, `CONDITIONAL` |
| `FunctionFlags` | `GLOBAL`, `NATIVE`, `DEBUG_ONLY`, `BETA_ONLY` |
| `GroupFlags` | `COLLAPSED_ON_REF`, `COLLAPSED_ON_BASE` |

The FO4 extensions (`Const`, `Hidden`, `Mandatory`, `BetaOnly`, `DebugOnly`) land as flag
tokens decorating existing item forms — there is no separate FO4 grammar. `Self` lexes to
`Token::KwSelf` and parses to `Expr::Ident("self")`; `Parent` to `Expr::ParentAccess`;
`None` to `Expr::NoneLit`. (`Expr::ArrayLit` is declared but the grammar has no array-literal
syntax in scope yet — only `new Type[size]` via `Expr::New` is parsed.)

## Public API

```rust
// crates/papyrus/src/lib.rs
pub fn parse_expr(source: &str) -> Result<Spanned<Expr>, Vec<ParseError>>;
pub fn parse_script(source: &str) -> Result<(Script, Vec<ParseError>), Vec<ParseError>>;
```

`Parser` itself (`parser/mod.rs`) is public and supports speculative parsing via
`pos()` / `error_count()` snapshots + `restore(pos, count)` (used by the
VarDecl-vs-expression disambiguator in `parser/stmt.rs`).

## File Layout

```
crates/papyrus/
  src/
    lib.rs              Public API: parse_expr(), parse_script()
    ast.rs              All AST types + flag bitsets + BinaryOp/UnaryOp precedence
    span.rs             Span, Spanned<T>
    token.rs            Token enum (logos derive), Display, can_start_expr()
    lexer.rs            preprocess() + OffsetMap + lex() + LexedToken
    error.rs            ParseError, ErrorKind, render()/offset_to_line_col()
    parser/
      mod.rs            Parser struct, token access, expect helpers, type parsing
      expr.rs           Pratt expression parser (depth-capped at MAX_EXPR_DEPTH = 256)
      stmt.rs           Statement parser (Return/If/While/VarDecl/Assign/ExprStmt, blocks)
      script.rs         Top-level item parser + parse_script() driver
  tests/
    r5_round_trip.rs    4 end-to-end round-trips on real R5 .psc fixtures
```

## Parser Details

### Statements (`parser/stmt.rs`)

`parse_stmt` dispatches on the first significant token: `Return`, `If`, `While`, a
primitive type keyword (always a `VarDecl`), an identifier (speculatively a `VarDecl`,
else an expression/assignment), or anything else (expression/assignment). Block
statements consume their matching terminator: `If`/`ElseIf`/`Else`/`EndIf`,
`While`/`EndWhile`. `parse_block(&[terminators])` walks statements until any terminator is
peeked. Compound assignments map to `AssignOp` (`+=` → `PlusEq`, `*=` → `MulEq`, …).

The identifier-prefix disambiguator (`parse_var_decl_or_expr`) snapshots position +
error count, speculatively parses a type; if a type is followed by another identifier it
commits to a `VarDecl` (`Form myProp = SomeRef`), otherwise it `restore`s and parses an
expression/assignment (`x = 5`, `someActor.SetActorValue(...)`).

### Top-level items (`parser/script.rs`)

`parse_script` parses the `ScriptName [Extends …] [flags]` header, then loops over
top-level items with per-item error recovery — a malformed item is reported and
`skip_to_next_line()` resumes so one bad line does not sink the file. Item dispatch:
`Import`, `Event`, `State`/`Auto State`, `Struct`, `CustomEvent`, `Group`, bare
`Function`, and a type-prefix path that disambiguates typed `Function`, `Property`, or
top-level `Variable` by the token after the type.

Properties support both the short form (`Float Property duration = 0.25 Auto`) and the
full form (`Property … EndProperty` with getter/setter accessor functions). `Native`
functions are body-less — the header is the whole item, no `EndFunction`.

### Error recovery & diagnostics (`error.rs`)

`ParseError { kind: ErrorKind, span }`. `ErrorKind` variants: `UnexpectedToken`,
`UnexpectedEof`, `InvalidLiteral`, `LexError`, and `ExpressionTooDeep { max_depth }`.
`render(source, filename)` produces `file:line:col: error: …` via `offset_to_line_col`.

### Expression depth cap (`parser/expr.rs`, #1270)

`parse_expr_bp` increments/decrements `Parser::expr_depth` around each recursion and bails
with `ErrorKind::ExpressionTooDeep` once depth reaches `MAX_EXPR_DEPTH = 256`. This stops a
pathologically nested `.psc` (e.g. `((((…))))` to arbitrary depth) from stack-overflowing
the parser. Vanilla Skyrim/FO4 scripts nest at most a few levels, so 256 is generous; a
512-paren input errors gracefully, a 200-paren input still parses. (#1270 /
SAFE-DIM3-NEW-02.)

## Pitfalls (load-bearing findings)

- **`peek()` skips newlines, `peek_raw()` does not.** Empty-`Return` detection must use
  `peek_raw()` — otherwise a `Return` on its own line is treated as having a value (the
  next statement / `EndEvent` on a later line). Fixed across `parser/stmt.rs`.
- **Array type vs index expression.** `parse_type` only treats `Base[]` (empty brackets)
  as an array; `Base[expr]` rewinds and the brackets are re-parsed as a postfix index.
- **Some keywords are valid identifiers** in name positions (`Auto`, `Hidden`,
  `Mandatory`, `Conditional`, `Native`, `Const`, `Global`) — handled by
  `keyword_as_ident` in `parser/mod.rs`.

## Status & Phases

| Phase | Scope | Status |
|-------|-------|--------|
| 1 | Lexer + expression parser | Done (M30 Phase 1, commit `f03a7711`) |
| 2 | Statements + function bodies (`parser/stmt.rs`) | Done (M30.2, `ab0eee96`) |
| 3 | Script-level declarations: properties, states, structs, groups, imports (`parser/script.rs`) | Done (M30.2, `ab0eee96`) |
| 4 | FO4 extensions + error recovery + real-corpus integration tests | Done (M30.2) — FO4 flags as tokens; per-item recovery; 4 R5 round-trips |
| 5 | Console / runtime integration | Partial — `parse_expr` backs the debug-protocol query evaluator (`crates/debug-server/src/evaluator.rs`); a dedicated `psc` file-load command is not wired |
| — | Depth-cap hardening | Done (#1270, commit `4ff28e8b`) |

### Tests (73 total)

- `parser/expr.rs` — 35 inline (literals, precedence, casts, calls, named args, member
  access, indexing, `new`, depth cap)
- `lexer.rs` — 13 inline (preprocess, case-insensitive keywords, operators, literals,
  comments, newlines)
- `parser/stmt.rs` — 12 inline (Return, VarDecl, Assign/compound-assign, If/ElseIf/Else,
  While, nested blocks, type disambiguation)
- `parser/script.rs` — 9 inline (header, Extends + flags, Event/Function bodies, Native
  function, Auto property, State, Import, full R5 rumble script)
- `tests/r5_round_trip.rs` — 4 integration round-trips

### Integration tests

`tests/r5_round_trip.rs` parses the four R5 reference scripts end-to-end with zero
recovered errors, asserting structural shape (item counts, names, key flags):
`defaultRumbleOnActivate`, `DA10MainDoorScript`, `MG07LabyrinthianDoorScript`,
`DLC2TTR4aPlayerScript`. These are the scripts that motivated M30.2 — see
[`docs/r5-evaluation.md`](../r5-evaluation.md) and the fixtures in `docs/r5/source/`.

## ECS Transpilation Target (M47.2)

The AST maps cleanly to ECS concepts:

| Papyrus | ECS Equivalent |
|---------|---------------|
| Property (Auto) | Component field |
| Event (OnActivate, OnHit...) | Marker component + system |
| Event with latent wait | Two systems — code-before-wait on the event, code-after-wait on the dt-zero frame |
| State (GoToState) | Enum field on component |
| Function (Global) | Standalone system function |
| Function (instance) | Method taking entity ID |
| Inheritance (extends) | Component composition |

The hand-translated R5 prototype (`crates/scripting/src/papyrus_demo/`) validated this
shape and wired into engine init under M47.0 (event-hook runtime, closed 2026-05-23).
M47.2 turns this hand-translation into a per-script transpiler emitting the same
components + systems. See [Scripting Architecture](scripting.md) for the full mapping and
[Papyrus API Reference](../legacy/papyrus-api-reference.md) for the complete API surface
(101 script types, 136 events). Roadmap entries: M30.2, M47.0, M47.2 in
[ROADMAP.md](../../ROADMAP.md).

## Open research — Starfield Papyrus grammar extensions

Source: `starfieldwiki.net/wiki/Starfield_Mod:Papyrus_-_New_Features`, 2026-07-04
(pasted by the user — this wiki is Cloudflare-blocked for direct fetch, see
`charal-starfield-ruleset.md`). Page is explicitly marked **WIP** and gives only
3 named features with no grammar detail — nothing here is LOCKED, no guessing
([[feedback_no_guessing]]):

- **Guards — "critical path single-thread protection".** No equivalent exists
  in the current AST/parser at all — sounds like a new statement/block form
  (mutex-style critical section), plausibly relevant because Starfield's engine
  is more multithreaded than Skyrim/FO4's Papyrus VM, but the actual syntax is
  unknown. Needs the dedicated feature page (not yet fetched).
- **Structs — "almost first-class user-defined data structures".** Structs
  already exist in this parser (FO4+, `Struct` AST node, §Papyrus Grammar
  Summary above) — this description implies Starfield extends them further
  (candidates: structs as function parameter/return types, nested structs),
  but which extension isn't specified on this stub page. Not a from-scratch
  feature; a possible widening of an existing one.
- **Imports — "import namespaces as well as attributes from other scripts".**
  The current grammar only supports whole-script import
  (`import ::= "Import" qualified_ident NEWLINE`, §Papyrus Grammar Summary).
  "Attributes from other scripts" implies a partial/selective import form not
  currently representable — a real grammar gap if confirmed, but no concrete
  syntax given yet.

Not actioning any of these without the actual per-feature grammar pages (the
stub page links out to dedicated sub-articles per feature that weren't
fetched this session).

## References

- [Papyrus Introduction](https://falloutck.uesp.net/wiki/Papyrus_Introduction)
- [Script File Structure](https://falloutck.uesp.net/wiki/Script_File_Structure) (`.psc` grammar)
- [Expression Reference](https://falloutck.uesp.net/wiki/Expression_Reference)
- [Events Reference](https://falloutck.uesp.net/wiki/Events_Reference)
- Internal: [Scripting Architecture](scripting.md), [Papyrus API Reference](../legacy/papyrus-api-reference.md),
  [R5 evaluation](../r5-evaluation.md), [M47.0 design](m47-0-design.md)
