# SCR-D4-NEW4-02: Bad setter drops a valid getter from a full-form Property

**Issue**: #2188
**Labels**: medium, bug
**Dimension**: Papyrus Lexer & Parser (Dimension 4)
**Untrusted-Input**: Yes
**Location**: `crates/papyrus/src/parser/script.rs:385-419` (`parse_property`), `:463-505` (`parse_property_accessors`)
**Status**: NEW — sibling gap in the #2125 fix (same bug shape, different container, not covered by commit `cacc9935`); independently found by an orphaned sub-agent from an earlier attempt at this audit

## Description

Full-form `Property`'s getter/setter loop still uses bare `?` on `parse_function(...)` calls (`parse_property_accessors`, lines 477, 495-496) and `parse_property` itself calls `self.parse_property_accessors()?` (line 407) — exactly the pre-fix shape `parse_state`/`parse_struct`/`parse_group` had before #2125. An error in the setter propagates up through `parse_property` → `parse_type_prefixed_item` → `parse_script_item`, discarding the entire `ScriptItem::Property` — including a getter that parsed with zero errors.

Confirmed directly: `parse_property_accessors`'s `Some(Token::KwFunction)` arm (line 477, `let func = self.parse_function(None)?;`) and its `_` arm (lines 495-496, `let ty = self.parse_type()?; let func = self.parse_function(Some(ty))?;`) both use bare `?` with no per-accessor recovery, unlike the per-child recovery loops #2125 added to `parse_state`/`parse_struct`/`parse_group`.

## Evidence

A throwaway test with a valid `Int Function Get() ... EndFunction` followed by a malformed `Function Set(Int value)` body produced 3 recovered errors, but `script.body` was empty — the property (and its valid `Get()`) never appears in the AST at all.

## Impact

Same class as the original #2125 impact, scoped to properties: any full-form property (getter+setter idiom) where the setter has a syntax error silently loses the getter too, with no diagnostic naming the property. This does not hang (bare `?` still exits immediately at EOF, unlike SCR-D4-NEW4-01 / #2185), so it's MEDIUM, not HIGH — the same severity #2125 was originally filed at.

## Related

Sibling of SCR-D4-NEW4-01 (#2185) — same bug shape #2125 fixed in `parse_state`/`parse_struct`/`parse_group` but missed here.

## Suggested Fix

Apply the identical per-child recovery pattern used in `parse_state`/`parse_struct`/`parse_group` to `parse_property_accessors`'s two `Some(Token::KwFunction)` / `_` arms — being careful to add the `at_eof()` guard from SCR-D4-NEW4-01 (#2185) at the same time, rather than reintroducing that regression a second time.

## Completeness Checks
- [ ] **TESTS**: A regression test pins that a malformed setter still leaves the valid getter in the AST (with a recovered error), rather than dropping the whole property
- [ ] **SIBLING**: Fix alongside SCR-D4-NEW4-01 (#2185) — both stem from the same missing shared recovery-loop helper
