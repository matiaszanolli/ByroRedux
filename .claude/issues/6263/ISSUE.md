# #6263: PEX-D4-2026-10-21-01: #5888 accepts `Parent` in every declaration-name position, so a local or parameter named `Parent` is declared fine but every use of it parses as `Expr::ParentAccess`, with 0 errors

**Labels**: low,scripting,bug

**Source**: docs/audits/AUDIT_PAPYRUS_2026-10-21.md · **Severity**: LOW · **Dimension**: Papyrus Lexer & Parser · **Location**: `crates/papyrus/src/parser/mod.rs:421-427`. `keyword_as_ident` maps `Token::KwParent` to `"Parent"` for every `expect_ident` caller. Compare `crates/papyrus/src/parser/expr.rs:187`, where `parse_prefix` turns `KwParent` into `ParentAccess`.

**Audit status**: NEW (introduced by `3081d23f2`, the #5888 fix; #5888 itself is closed and the struct-member case works)

**Untrusted-Input**: Yes (`.psc`)

## Description

#5888 needed `Parent` as a struct member name (F4SE `ObjectReference.psc`: `struct ConnectPoint { string parent }`) and as a `.member` name. The fix put it in the shared `keyword_as_ident` table. That table serves every name position that goes through `expect_ident`: locals, parameters, properties, functions, and qualified script, `Extends` and type names.

The other keywords in that table (`Auto`, `Hidden`, `Native`, …) have no meaning as an expression, so allowing them as names is harmless. `Parent` does: in primary-expression position it is `ParentAccess`. So a name can now be declared and then never referred to. Every reference silently becomes the parent-script accessor.

## Evidence

The `cases` probe parsed `ScriptName T / Function F() / Int Parent = 1 / Parent = 2 / EndFunction`. The result is `[VarDecl(Parent), Assign { target: ParentAccess, … }]` with **0 errors**. A parameter would behave the same: `Function F(Actor Parent)` followed by `Parent.Kill()` calls the parent script's `Kill`.

## Impact

No vanilla or F4SE source declares `Parent` anywhere except that struct member. A grep of the on-disk FO4 `Scripts/Source` finds `parent` only in comments, apart from `ObjectReference.psc`. For any other `.psc` (mods, the extender preflight, the debug console), the result is a silent wrong AST instead of an error. It is the same kind of gap as #5021 and #5885: wrong tree, 0 diagnostics.

## Related

#5888 (closed), #5328

## Suggested Fix

Accept `KwParent` only where #5888 needs it, in struct-member declaration names and the name after `.`, and remove it from the shared `keyword_as_ident`. Alternatively, record a recovered error when a local or parameter is declared as `Parent`. Pin it with the probe's `Int Parent` case.

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other keywords in keyword_as_ident that also have an expression meaning (Self, None, …))
- [ ] **TESTS**: A regression test pins this specific fix
