# #2724: AVM2 adapter constant pool is hand-indexed by position against Vec literals declared 40-90 lines earlier

**Found independently by 2 audits in the same `ui-deep` suite run** — merged here.

### SAFEUI-08 — SAFETY_UI view

*the generated adapter's constant pool is hand-indexed by position, with eight dead entries still emitted into every patched menu*

- **Severity**: LOW
- **Dimension**: 4 (discipline — the no-`unsafe` analogue in this crate)
- **Location**: [`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs):516-580
- **Status**: NEW
- **Description**: `build_adapter_abc` builds a 27-entry string pool and a
  17-entry multiname pool as literal `vec![...]`, then refers to their members
  through seventeen hand-written `Index::new(N)` constants whose only tie to the
  pool is a trailing comment. Inserting, removing, or reordering a single pool
  entry silently shifts every later index and produces a **valid but wrong**
  ABC — the adapter would install forwarders under the wrong names, or register
  callbacks under the wrong strings, with no parse error anywhere. Eight of the
  entries are already dead: the strings `LoaderInfo`,
  `getLoaderInfoByDefinition`, `addEventListener`, `target`, `content`,
  `complete`, `flash.utils`, `setTimeout` and multinames 2, 6, 7, 8, 9, 15 are
  never referenced by any emitted op — leftovers from an abandoned
  `LoaderInfo`-based install strategy, shipped inside every patched FO4 menu.
- **Evidence**: I cross-checked all seventeen constants against the literal pool
  positions and **all are currently correct** (see §3) — this is a fragility and
  dead-weight finding, not a live mis-index. The structural test
  `generated_adapter_is_valid_abc_with_one_helper_per_method`
  ([`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs):934) counts ops
  and pins exactly one string index (`callback_names == [22]`); it would not
  catch a shift in the other sixteen.
- **Suggested Fix**: Build both pools through the existing `add_string` /
  `add_multiname` helpers so every index is derived rather than transcribed,
  and drop the eight unused entries.

---

---

### TD7-2026-08-12-01 — TECH_DEBT view

*`build_adapter_abc` hand-numbers ~38 constant-pool indices against two Vec literals declared 40-90 lines earlier [UI]*

- **Severity**: LOW
- **Dimension**: 7 (Magic Numbers & Hardcoded Constants)
- **Location**: `crates/ui/src/avm2_host.rs:516-580` (declarations) and `:582-880` (uses)
- **Status**: NEW
- **Description**: The generated ABC's constant pool is built as two literal
  `Vec`s — `strings` (27 entries, `:516`) and `multinames` (17 entries, `:545`)
  — and then referenced by **1-based positional literals**: 21 distinct
  `Index::new(N)` values plus 17 `qname(namespace, name)` literal pairs, plus
  four `Namespace::Package(Index::new(N))` entries at `:857-862`. Correctness
  depends entirely on nobody inserting into the middle of either `Vec`. The
  only defence is a trailing-comment column (`qname(1, 14),  // BGSCodeObj`).
- **Evidence**: Literal `Index::new` values inside `build_adapter_abc`:
  `{0,1,2,3,4,5,10,11,12,13,14,16,17,18,19,20,22,24,25,26,27}`. Cross-checked
  every one against the two `Vec` literals during this audit: **all currently
  correct**, and every trailing comment matches. The file already provides
  `add_string` / `add_multiname` (`:889`, `:894`) which return the correct
  `Index` — but they are used only for the per-catalog-method entries appended
  in the loop, never for the fixed prefix.
- **Impact**: No live defect. The failure mode is silent and delayed: inserting
  one string in the middle shifts every subsequent literal by one, producing an
  ABC that still parses (so `generated_adapter_is_valid_abc_with_one_helper_per_method`
  at `:934` still passes — it asserts *counts*, not *identities*) but forwards
  calls under wrong names. Only the `#[ignore]`d installed-corpus tests would
  catch it. This is also the mechanical reason TD8-2026-08-12-01's dead entries
  cannot simply be deleted.
- **Related**: TD8-2026-08-12-01 (same site, same root cause). One test does
  pin a raw index — `assert_eq!(callback_names, [22])` at `:984` — which is a
  guard, but one whose failure message names an integer rather than a symbol.
- **Suggested Fix**: Route the fixed prefix through the existing
  `add_string`/`add_multiname` helpers into named `let` bindings, exactly as the
  per-method loop already does, and delete every literal `Index::new` except
  `Index::new(0)` (the ABC "any" sentinel). This makes insertion order-independent
  and lets `:984` assert against `loaded_callback_string` instead of `22`.
- **Effort**: small

---
**Sources**: `docs/audits/AUDIT_SAFETY_UI_2026-08-12.md` (SAFEUI-08), `docs/audits/AUDIT_TECH_DEBT_2026-08-12.md` (TD7-2026-08-12-01)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

