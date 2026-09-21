# 4472: PEX-D4-2026-09-19-01: six newline-skipping peek/check sites outside the Pratt loop glue adjacent lines with zero errors

State: OPEN  Labels: ['bug', 'medium', 'scripting']

**Severity**: MEDIUM · **Dimension**: Papyrus Lexer & Pratt Parser · **Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D4-2026-09-19-01) · **Location**: `crates/papyrus/src/parser/mod.rs:290` (colon-qualified ident), `mod.rs:369` (`[` type suffix), `stmt.rs:262` (assign-op), `stmt.rs:206` (VarDecl disambiguation), `script.rs:203-214` (top-level type→item peek), `script.rs:112` (header flag loop)

**Description**
#4321 made the Pratt loop newline-terminating via `peek_raw()`, but the six sites above still decide on newline-skipping `peek()`/`check()`. All six are probe-confirmed to glue the next line into the construct with **zero errors**:
- `foo` ⏎ `:bar()` → one `Call(Ident("foo:bar"))`
- `x` ⏎ `= 5` → one `Assign`
- `Foo` ⏎ `bar = 1` → one `VarDecl`
- `Actor` ⏎ `[] props` → Array-typed Variable
- `Int` ⏎ `Property P = 5` / `Int` ⏎ `Function F()` glue at top level
- the header flag loop consumes a following line's `Native`/`Const`/`Hidden`/`DebugOnly` into `ScriptFlags` — probe: a function's own `Native` silently promoted to a script flag with `Function.flags` left empty

Unlike #4321's valid-source shape, every one of these requires at least one invalid line — hence MEDIUM, not HIGH. This is the residue class SCR-D4-2026-09-14-02 named ("audit the other same-line peek/check decisions"); #2656 was fixed only at `parse_property_flags`.

**Evidence**
All six shapes probe-confirmed zero-error (dim-4 probe table, AUDIT_PAPYRUS_2026-09-19). Most consequential is the header-flag swallow (`script.rs:112-131`): it corrupts a flag bitfield silently rather than merely re-shaping already-broken source.

**Impact**
Malformed mod source is silently accepted as a plausible-but-wrong AST instead of erroring; `extender_preflight` under/over-reports flags and calls on such source. No valid-source shape found in the probe set.

**Related**: #4321 (fixed — the valid-source instance), #2656 (fixed at one site)

**Suggested Fix**
Apply the same `peek_raw()` discipline (or a same-line span check: candidate token's `span.start` must not exceed the previous consumed token's line end) at the six sites, and pin each with a zero-error-glue regression test like `a_line_opening_with_a_paren_is_not_a_call_on_the_previous_line`.

## Completeness Checks
- [ ] **SIBLING**: Re-sweep `parser/{mod,stmt,script,expr}.rs` for any remaining newline-skipping decision on a statement/item boundary
- [ ] **TESTS**: One zero-error-glue regression test per site


---

# 4473: PEX-D3-2026-09-19-02: case-colliding state names both get is_auto: true (malformed .pex only)

State: OPEN  Labels: ['bug', 'medium', 'scripting']

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


---

# 4474: PEX-D1-2026-09-19-01: exhaustive-prefix guard samples never reach the FO4+ debug skip paths, full-property bodies, struct infos, or Value::Float

State: OPEN  Labels: ['bug', 'low', 'scripting', 'test-gap']

**Severity**: LOW (test gap; the paths are corpus-proven correct today) · **Dimension**: PEX Reader · **Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D1-2026-09-19-01) · **Location**: `crates/pex/src/lib.rs:892-918` (sample array); gaps at `crates/pex/src/reader.rs:257-283` (`skip_property_groups`/`skip_struct_orders`), `reader.rs:413-419` (full getter/setter wire bodies), `reader.rs:342-362` (`read_struct_infos`), `reader.rs:173` (`Value::Float`)

**Description**
The #3942 exhaustive-prefix test enumerates every prefix of 4 handbuilt samples, but none of the samples contains: FO4+/Starfield debug info (so `skip_property_groups`/`skip_struct_orders` never run — the extender sample is BE, which stops before them, and the LE samples are debug-absent), a non-auto property with getter/setter *bodies on the wire*, non-empty `struct_infos` (the Starfield sample has count 0), or a `Value::Float` (tag 4) operand. A one-byte regression in any of those desyncs the stream silently and `cargo test -p byroredux-pex` stays green. Same class: the FO76 dialect (`game_id 3`) appears in zero corpus files and no handbuilt sample.

**Evidence**
Dim-1 sample-builder analysis (`build_sample` debug `u8(0)` + auto-property only; `build_sample_skyrim_be` BE ⇒ skip fns unreachable; `build_sample_starfield_with_guards` `u16(0)` struct infos, Integer-only values). The paths *work*: a corpus probe parsed 12,308 LE+debug vanilla files and 4,740 Starfield files with 0 failures — this is a regression-protection gap, not a live bug.

**Impact**
A future skip-reader refactor (the "one wrong u16" class) ships silently broken for FO4/FO76/Starfield debug-compiled scripts — mis-decoded objects or errors on real game files only.

**Related**: #3942

**Suggested Fix**
Add two samples to the prefix test's array: an FO4/Starfield file with debug present including non-empty property groups + struct orders, and one with a full (non-auto) property carrying getter/setter bodies, a non-empty struct info with members, and a `Value::Float` operand (~30 lines each on the existing `PexWriter`).

## Completeness Checks
- [ ] **SIBLING**: Consider an FO76 (`game_id 3`) sample while touching the builders
- [ ] **TESTS**: The new samples run through `every_prefix_of_every_wire_valid_sample_is_rejected`


---

# 4475: PEX-D1-2026-09-19-02: pin the transmute invariant (MAX_OPCODE == last discriminant + 1) with a const assert

State: OPEN  Labels: ['bug', 'low', 'scripting']

**Severity**: LOW (hardening) · **Dimension**: PEX Reader · **Untrusted-Input**: Yes (the invariant protects `from_u8` on hostile bytes)
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D1-2026-09-19-02) · **Location**: `crates/pex/src/opcode.rs:68,130-137,160-166`

**Description**
The `unsafe transmute` in `from_u8` is sound only while `OpCode` has contiguous discriminants `0..MAX_OPCODE` with `MAX_OPCODE == 51 == last discriminant + 1`. Today that holds (51 variants, `TryLockGuards == 50`, guard `byte >= MAX_OPCODE`), and the SAFETY comment says so. But the only enforcement is the runtime test `discriminants_match_on_disk_order` — nothing fails at *compile time* if a variant is added or `MAX_OPCODE` is edited without re-deriving it. An appended variant with a stale `MAX_OPCODE` is unreachable (safe but wrong); a mid-enum insertion shifts every discriminant and can push `OPCODES[self as usize]` (in `name()`/`arg_count()`) out of bounds for out-of-table discriminants.

**Evidence**
`opcode.rs:134-136` SAFETY comment; `OPCODES` length tied to `MAX_OPCODE as usize` (`:73`); the pin lives in `#[cfg(test)]` only; `grep 'const _: () = assert'` finds nothing.

**Impact**
None today (CI runs the test). The failure mode if the test is skipped or deleted is a wrong-arg-count decode or an index panic on hostile bytes.

**Related**: #2127 (full-table test), #3948 (panic net)

**Suggested Fix**
One line next to `MAX_OPCODE`:
```rust
const _: () = assert!((OpCode::TryLockGuards as u8) + 1 == MAX_OPCODE,
    "MAX_OPCODE must be the last opcode discriminant + 1");
```

## Completeness Checks
- [ ] **UNSAFE**: The SAFETY comment on `from_u8` is updated to reference the const assert as the compile-time half of the invariant
- [ ] **TESTS**: `discriminants_match_on_disk_order` stays (defense in depth)


---

# 4476: PEX-D2-2026-09-19-01: backward_jmpt_builds_a_loop_edge tests a JmpF, not a JmpT (guard misnomer)

State: OPEN  Labels: ['bug', 'low', 'scripting', 'test-gap']

**Severity**: LOW (test-naming rot; no coverage hole) · **Dimension**: Decompiler CFG & Lift · **Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D2-2026-09-19-01) · **Location**: `crates/pex/src/decompile/cfg.rs:371-401` (misnomer); JmpT polarity actually pinned at `cfg.rs:474-475` in the differently-named #2122 sibling

**Description**
The guard a reader would credit for JmpT loop polarity, `backward_jmpt_builds_a_loop_edge`, builds its conditional from `OpCode::JmpF` (`cfg.rs:379`) with the backedge carried by the unconditional `Jmp` at `:381` — its edge assertions re-pin JmpF polarity a second time. JmpT polarity *is* pinned, but in the differently-named #2122 sibling `backward_jmpt_target_inside_own_block_conditions_the_right_block` (`on_true == 1` = jump target, `on_false == 3` = fall-through). No coverage hole; a naming/auditability defect that would misdirect a future regression hunt.

**Evidence**
`cfg.rs:379` uses `(OpCode::JmpF, vec![id("t"), Value::Integer(3)])` inside the `jmpt`-named test; contrast `cfg.rs:459` which uses `OpCode::JmpT` in the sibling.

**Impact**
An auditor or future edit greps the guard by name, concludes JmpT loop polarity is pinned here, and deletes/mutates the wrong test while breaking the real pin.

**Related**: #2122

**Suggested Fix**
Rename the test (e.g. `forward_jmpf_with_unconditional_backedge_builds_a_loop`) or add the missing mirror: a `JmpT`-conditional loop head asserting `on_true` = backedge target, `on_false` = exit.

## Completeness Checks
- [ ] **TESTS**: If renamed, the audit trail references the new name; if mirrored, both edges asserted


---

# 4477: PEX-D2-2026-09-19-02: is opcode decompiles to an object-typed cast — third Champollion departure needs bookkeeping

State: OPEN  Labels: ['bug', 'low', 'scripting']

**Severity**: LOW · **Dimension**: Decompiler CFG & Lift (observed at `lift.rs:231`; rewrite in `lower.rs:110-118`) · **Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D2-2026-09-19-02) · **Location**: `crates/pex/src/decompile/lower.rs:110-118` (rewrite), `crates/pex/src/decompile/lift.rs:231` (producer)

**Description**
`a is T` (opcode 36, Bool result) lowers to `Expr::Cast { expr: a, target_type: T }` — an object-typed `as` expression — because the shared Papyrus AST has no `is` operator. In an `if` condition the rewritten AST is ill-typed Papyrus (`as` does not yield Bool). Documented in-code as deliberate and "recognizer-irrelevant"; recognizers key on names/calls/condition shape and ≈0 vanilla scripts contain opcode 36 (no `.psc` construct emits it), so no current consumer diverges. This is a third in-code Champollion departure alongside the two documented in the scripting baseline — it should be findable there too.

**Evidence**
`lower.rs:110-118` comment "lower to the structurally closest `a as T`. Rare and recognizer-irrelevant."; the 09-11→09-14 corpus runs show no `is`-related errors.

**Impact**
A future consumer that renders the lower-only AST to source text or evaluates it strictly would emit/reject ill-typed conditions for FO4+/Starfield scripts containing opcode 36. Silent wrong AST with no current divergence.

**Related**: departure bookkeeping adjacent to #4115

**Suggested Fix**
Either add an `Is`-capable expression (or a condition-only `TypeTest` variant) to the shared AST, or record the departure next to the two documented Champollion departures in the scripting audit baseline so it is findable.

## Completeness Checks
- [ ] **SIBLING**: If the AST gains an `Is` operator, check the `.psc` parser never produces it spuriously
- [ ] **TESTS**: Whatever route is chosen, a pin documents the opcode-36 lowering


---

# 4478: PEX-D4-2026-09-19-02: papyrus-parser.md depth-cap/newline sections predate #4320/#4321; stale paren-depth comment

State: OPEN  Labels: ['documentation', 'low', 'scripting', 'doc-rot']

**Severity**: LOW (doc rot) · **Dimension**: Papyrus Lexer & Pratt Parser · **Untrusted-Input**: No
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D4-2026-09-19-02) · **Location**: `docs/engine/papyrus-parser.md:223-229` (depth caps), `:108-110` + `:234-236` (newlines), `:267-279` (test counts); `crates/papyrus/src/parser/expr.rs:944-949` (inline comment)

**Description**
The parser contract doc predates both #4320 and #4321:
- The depth-cap section still says the cap works by "`parse_expr_bp` increments/decrements `Parser::expr_depth` around each recursion" — it omits #4320's `enter_chain_link` charging of iteratively built postfix/binary chains, the load-bearing half of the cap since the fix.
- The "Newlines are significant" section and Pitfalls name only empty-`Return` as the `peek_raw` load-bearer; the rule "a newline terminates an expression; an operator ending a line still continues" (#4321) is undocumented.
- Test counts say "73 total (expr.rs — 35)"; the crate is now 100 unit + 4 round-trip with 7 new expr.rs guards.
- Separately, the inline comment in `depth_cap_accepts_legitimate_nesting` (`expr.rs:944-949`) claims "Each paren-pair contributes 2 to expr_depth" — measured 1/pair (200 pairs parse; the cap admits ≈255 pairs), so the test's margin rationale is factually wrong even though the test passes.

**Evidence**
Doc text at the cited lines vs `expr.rs:41-70` (entry-charge + per-chain-link charge + value-restore); dim-4 probe paren rows.

**Impact**
A future editor loosening the cap or touching the loop reads a wrong model of the depth ledger and of the newline contract — exactly the drift the contract doc exists to prevent.

**Related**: #4320, #4321

**Suggested Fix**
Update the depth-cap section to describe entry-charge + per-chain-link charge + restore-on-every-exit; add a "newline ends an expression" bullet to Pitfalls; refresh test counts; correct the paren-pair comment to 1 unit/pair.

## Completeness Checks
- [ ] **TESTS**: N/A (doc-only), but confirm the corrected paren comment matches a measured boundary


---

# 4479: PEX-D4-2026-09-19-03: lexer silently mislexes 0x, 1e5, and CR-only line endings with zero diagnostics

State: OPEN  Labels: ['bug', 'low', 'scripting']

**Severity**: LOW · **Dimension**: Papyrus Lexer & Pratt Parser · **Untrusted-Input**: Yes
**Source**: `docs/audits/AUDIT_PAPYRUS_2026-09-19.md` (PEX-D4-2026-09-19-03) · **Location**: `crates/papyrus/src/token.rs:241-247` (literal regexes), `:105` (`skip r"[ \t\r]+"`), `:108` (`Newline` = `\n` only)

**Description**
Three lexer-level shapes produce no diagnostic while yielding token streams a reader would not expect:
(a) `0x` with no hex digits is not matched by the hex regex, so it lexes as `IntLit(0)` + `Ident("x")` — invalid source silently becomes plausible valid tokens;
(b) `1e5` lexes as `IntLit(1)` + `Ident("e5")` (Papyrus has no exponent notation; 0 errors);
(c) `\r` is in the skip-whitespace regex and `Newline` matches only `\n`, so a CR-only (classic-Mac) file produces **no `Newline` tokens at all** — the entire file is one line to the parser, the newline-terminator contract is void, and the #4321 protection is vacuous on such files (probe: `SetStage(10)`␍`(akRef).Disable()` → 1 glued statement, 0 errors). CRLF files are handled correctly (`\r` skipped, `\n` kept). Literal *overflow* is fine (`0xFFFFFFFFFFFFFFFF` errors cleanly, no panic).

**Evidence**
Dim-4 probe rows `0x-alone`, `1e5`, `pure-CR endings` (AUDIT_PAPYRUS_2026-09-19).

**Impact**
Exotic/typo'd source gets silently mis-tokenized instead of a precise lex error; the CR-only case neutralizes the entire statement-terminator design on such files. No known corpus hits — hence LOW.

**Related**: #1908 (same "lex must not lie" theme)

**Suggested Fix**
(a) make a `0[xX]` prefix with no digits a lex error (or fold it into one IntLit + error); (b) reject `e`-adjacent mislexes with an "exponents not supported" lex error; (c) either treat a lone `\r` as `Newline` (matching many legacy tools) or lex-error on a `\r` not followed by `\n`.

## Completeness Checks
- [ ] **SIBLING**: Check the other literal regexes (float, char) for the same prefix-only shape
- [ ] **TESTS**: One regression test per shape asserting a lex error (or the chosen `\r` behavior)


---

# 4488: EXT-D6-2026-09-19-02: FO76 ships 1,007 .bto object-LOD files the scheme table calls "none"

State: OPEN  Labels: ['bug', 'medium', 'game:fo76', 'terrain-exterior']

**Severity**: MEDIUM · **Dimension**: Distant LOD · **Tier Violated**: no-fabrication (inverse: assets exist, scheme claims none) · **Game Affected**: FO76
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D6-2026-09-19-02)

**Location**: `byroredux/src/cell_loader/object_lod.rs:653-662` (`object_lod_scheme` → `_ => None`); corpus: `SeventySix - GeneratedMeshes01.ba2`

**Description**
The table's own comment says "FO76/Starfield: not yet exercised — add an arm with archive evidence rather than by lineage". The archive evidence now exists: **1007** `meshes\terrain\appalachia\objects\appalachia.<L>.<x>.<y>.bto` (level4 ×795 / level16 ×164 / level32 ×48; **no level 8**) — the exact `BakedBto` naming family Skyrim/FO4 use. FO76 distant objects therefore never render from baked LOD: the #3321 false-premise shape, one game over. The skipped level-8 band would exercise the #3502 coarsen path on a mixed 4/16/32 ladder if wired; `LodBandLadder::for_game(FO76) = None` and the `!combined_lod_supported(FO76)` pin (`lod_support.rs:398`) need a joint decision.

**Impact**
FO76 horizons lack all distant objects from baked LOD. Feature gap, not a crash.

**Related**: #3321 (FNV precedent, closed); #3502 (coarsen escape — implemented, would be exercised)

**Suggested Fix**
Decide the ladder+scheme shape (FO4-style `BakedBto` objects + explicit ladder decision), then add the arm with this census as the archive evidence. Starfield "none" re-verified correct (0 `.bto`/`.btr` in `LODMeshes.ba2`).

## Completeness Checks
- [ ] **SIBLING**: Revisit `combined_lod_supported(FO76)` + the lod_support pin in the same change
- [ ] **TESTS**: Scheme⇒ladder coherence test extended to the new arm; corpus counts pinned


---

# 4507: EXT-D7-2026-09-19-07: exterior matrix covers 5 of 7 games; W1 traversal route exists only for FNV

State: OPEN  Labels: ['bug', 'low', 'game:fo76', 'game:starfield', 'terrain-exterior', 'test-gap']

**Severity**: LOW · **Dimension**: Acceptance harness · **Game Affected**: FO76, Starfield (matrix); all but FNV (W1 traversal)
**Source**: `docs/audits/AUDIT_EXTERIOR_2026-09-19.md` (EXT-D7-2026-09-19-07)

**Location**: `docs/smoke-tests/m-exteriors.sh:896-913` (game case list); `docs/smoke-tests/fixtures/` (only `fnv.env` declares `W1_WATER_SOURCE`, line 133)

**Description**
The exterior readiness matrix — the harness the audit's Dim 1-6 verdicts lean on — has no FO76/Starfield profile, and the WATAL traversal gate (`w1-water-traversal.sh`) has exactly one measured route (FNV deep profile); `skyrim_se`, the default game, is a SKIP. Partially by design: FO76/Starfield exterior support is documented open scope (watal.md; terrain-LOD scheme "none" for both — though see EXT-D6-2026-09-19-02 for FO76's shipped `.bto` family). The scripts do not overclaim in their own usage text.

**Impact**
Any FO76/Starfield exterior work lands with zero harness backstop; "W1 passed" in a session summary usually means FNV only.

**Suggested Fix**
When FO76/Starfield exterior support lands, add `fixtures/<game>.env` + water fixture rows (and revisit with EXT-D6-2026-09-19-02). Until then the README's honest-skip note is adequate.

## Completeness Checks
- [ ] **TESTS**: New fixture rows pin per-game gates when support lands


---
