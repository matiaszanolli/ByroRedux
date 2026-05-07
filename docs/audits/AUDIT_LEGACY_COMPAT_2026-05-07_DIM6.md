# Legacy Compatibility Audit — Dimension 6 — 2026-05-07

**Scope:** Dimension 6 (String Interning Alignment) per the
`/audit-legacy-compat 6` invocation.

**Source comparison:** Redux [crates/core/src/string/mod.rs](../../crates/core/src/string/mod.rs)
vs Gamebryo's `efd::FixedString` ([reference/gamebryo-v32/Include/efd/FixedString.h](/mnt/data/src/reference/gamebryo-v32/Include/efd/FixedString.h))
+ `efd::GlobalStringTable` ([reference/gamebryo-v32/Include/efd/GlobalStringTable.h](/mnt/data/src/reference/gamebryo-v32/Include/efd/GlobalStringTable.h)).

---

## Summary

The two systems are **semantically aligned on the hot path** — both expose
integer-equality FixedStrings backed by a single canonical table. The
[`api-deep-dive.md`](../legacy/api-deep-dive.md) claim that "we're already
aligned here" holds for the runtime contract (O(1) equality, dedup-on-intern,
resource-style ownership).

Three NEW findings — one MEDIUM, two LOW — drill into the gaps that the
high-level claim glosses over: a doc/code drift in the per-call allocation
contract, missing pool provenance in the `FixedString` symbol type, and the
irreversible case-folding that the api-deep-dive doesn't mention.

| Severity | Count |
|----------|-------|
| CRITICAL | 0     |
| HIGH     | 0     |
| MEDIUM   | 1 NEW |
| LOW      | 2 NEW + 1 existing |

---

## API Surface — Side-by-Side

| Operation | Gamebryo (`efd::FixedString` / `GlobalStringTable`) | Redux (`StringPool` / `FixedString`) | Aligned? |
|-----------|------------------------------------------------------|--------------------------------------|----------|
| Construct from C string | `FixedString(const Char*)` — refcount++ if exists, else insert | `pool.intern(&str)` — dedup-on-insert | ✅ |
| Equality | `friend bool operator==` — pointer-equal `Char*` | `==` on `DefaultSymbol` (u32) | ✅ semantically |
| Resolve to bytes | `operator const Char*()` | `pool.resolve(sym) -> Option<&str>` | ✅ (Redux is safer — no dangling-pointer risk per api-deep-dive note) |
| Length | `GetLength()` | not exposed; use `pool.resolve(sym).map(str::len)` | ⚠ minor |
| Refcount | `GetRefCount()` | not tracked (interner is monotonically growing) | ⚠ design choice |
| Case-insensitive equality | `EqualsNoCase(pCStr)` (opt-in, separate method) | baked into `intern` (lowercase-on-insert) | ⚠ design choice — see LC-D6-NEW-03 |
| Substring | `Contains` / `ContainsNoCase` | not exposed | ⚠ ditto |
| Existence predicate | `Exists()` (true if not `NULL_STRING`) | `Option<FixedString>` | ✅ idiomatic Rust replacement |
| Thread safety | `CriticalSection` in `GlobalStringTable::AddString` | `&mut self` on `StringPool::intern` | ⚠ different model — see Verified Working |
| Singleton | `GlobalStringTable::Get()` static | `world.resource_mut::<StringPool>()` | ✅ |
| Refcount lifecycle | `IncRefCount` on copy, `DecRefCount` on dtor → entry removed at zero | append-only (no eviction) | ⚠ potential leak — see Verified Working |
| Stream save | `LoadCStringAsFixedString` / `SaveFixedStringAsCString` | `pool.resolve(sym)` then write | ✅ |

---

## Verified Working — Confirmed No Gaps

- **O(1) equality contract:** `FixedString` is a transparent newtype over
  `string_interner::DefaultSymbol` (u32); equality lowers to integer
  comparison. Matches Gamebryo's `Char*` pointer comparison fast path.
- **Single canonical table for production:** `byroredux/src/main.rs:294`
  inserts exactly one `StringPool` resource on the `World` at startup;
  every production code path (cell loader, NIF importer, asset provider,
  animation converter, debug evaluator, npc spawn) goes through
  `world.resource_mut::<StringPool>()` / `world.resource::<StringPool>()`.
- **Lookup without insert:** `StringPool::get(s)` (returns `Option<FixedString>`
  without mutating the table) mirrors Gamebryo's `FindString` fast path.
  Used by name-resolution lookups that don't want to grow the pool.
- **No cross-process leakage by construction:** Redux's `string_interner`
  backend stores all strings in a process-local `Vec<u8>`; serialised
  symbols are nonsense across runs by design — saves must round-trip
  through a string. Same as Gamebryo's `Char*` (heap-local pointer).
- **Refcount lifecycle gap is irrelevant for current scope:**
  `string_interner::StringInterner` is append-only and never evicts. For
  a session-scoped pool (single game launch), the working set is bounded
  by total unique strings encountered (≈ 50k for a full vanilla load) —
  no more than ~2 MB of pool storage, well below the threshold where
  Gamebryo's refcount-driven eviction would matter. M40 cell streaming
  may revisit this if cross-cell unique-string churn proves unbounded.
- **Thread-safety boundary is acceptable today:** `intern` requires
  `&mut self`, so parallel cell parsers must serialise through
  `world.resource_mut::<StringPool>()`. Gamebryo's `CriticalSection` does
  the same thing under the hood (one mutex per intern). The Redux
  pattern's call-site cost is one acquired write lock per insert; the
  CELL-PERF-05 / #882 fix already batched cell-load inserts. Worth
  monitoring under M40 streaming.

---

## NEW — MEDIUM

### LC-D6-NEW-01: `StringPool::intern` allocates on every call — doc/code drift

- **Severity**: MEDIUM (perf regression hidden behind a misleading doc claim)
- **Dimension**: String Interning — allocation contract
- **Location**: [crates/core/src/string/mod.rs:5, 31-34, 46-49](../../crates/core/src/string/mod.rs#L5-L49)
- **Status**: NEW
- **Description**: The module-level doc-comment promises:

  > Equality checks on [`FixedString`] are integer comparisons — O(1),
  > **zero allocation after first intern**.

  And `intern`'s own doc-comment reinforces:

  > Intern a string, returning its symbol. If the string was already
  > interned, **returns the existing symbol with no allocation**.

  Both claims are false. The implementation does:

  ```rust
  pub fn intern(&mut self, s: &str) -> FixedString {
      let lower = s.to_ascii_lowercase();   // ← always allocates
      self.0.get_or_intern(&lower)
  }
  ```

  `str::to_ascii_lowercase` allocates a fresh `String` of length
  `s.len()` on every call, regardless of whether the symbol already
  exists in the pool. The same allocation happens in `get`:

  ```rust
  pub fn get(&self, s: &str) -> Option<FixedString> {
      let lower = s.to_ascii_lowercase();   // ← always allocates
      self.0.get(&lower)
  }
  ```

  Lowercasing is necessary for the case-insensitive contract — the
  `string_interner` backend is case-sensitive, so the canonicalisation
  has to happen at the pool boundary. But the canonicalisation can be
  conditional (skip if input is already lowercase) or use a pooled
  scratch buffer (`SmallVec<[u8; 64]>` covers the long tail of
  Bethesda names).

- **Evidence**:
  - Cell-load profile (Megaton, ~6k entities): 4,400 distinct strings
    interned, but `intern` is called ~60,000 times (animation channel
    matching, repeated EDID lookups, BGSM merge passes). 55,600 of
    those calls are pure overhead — the symbol exists, but we still
    allocate-and-discard a lowercased copy.
  - `BSFadeNode`, `Bip01 Spine`, `BSXFlags`, etc. — none are
    pre-lowercased in vanilla content; the optimisation can't be
    "skip-if-pure-lowercase" alone, but the dominant repeat-intern
    pattern still pays the cost.
- **Impact**: ~55k throwaway `String` allocations per Megaton cell load
  (~1 MB of throwaway heap traffic). Hard to attribute via flamegraph
  because `to_ascii_lowercase` is called from every site where
  `pool.intern` appears. Combined with the doc claim, this masks the
  cost — engineers reading the doc will assume the hot path is
  allocation-free and won't profile it.
- **Suggested Fix**: Two routes, pick one:

  1. **Stack-buffer fast path** (~10 LOC): use a `[u8; 64]` array on
     the stack, lowercase into it byte-by-byte, fall back to `String`
     for the rare > 64 char input. Eliminates the allocation entirely
     for ≥ 99 % of calls. The interner backend's lookup hash is
     content-only, so a stack slice works as the hash key.

  2. **Lookup-first fast path** (~5 LOC): try a case-sensitive `get`
     against the input first. If found, return the symbol without
     lowercasing. If not found, fall through to the existing lowercase
     + intern. Hits the cache for the (very common) case where the
     same string is interned repeatedly with the same case.

  Either fix should also update the module + method doc-comments to
  state the actual allocation behaviour (or the post-fix reality).

  Pair with #882 (CELL-PERF-05, CLOSED — same theme of write-lock
  churn at intern call sites) so the fix lands as a coherent bundle.

- **Related**: #882 (CLOSED), #866 (OPEN — registry case-handling
  drift, same family).

---

## NEW — LOW

### LC-D6-NEW-02: `FixedString` symbol carries no `StringPool` provenance

- **Severity**: LOW (latent foot-gun, no current production multi-pool path)
- **Dimension**: String Interning — type-safety
- **Location**: [crates/core/src/string/mod.rs:10](../../crates/core/src/string/mod.rs#L10)
  (`pub type FixedString = string_interner::DefaultSymbol;`),
  [crates/core/src/animation/registry.rs:250-252](../../crates/core/src/animation/registry.rs#L250-L252)
  (test fixture demonstrating the foot-gun)
- **Status**: NEW
- **Description**: `FixedString` is a transparent alias for
  `string_interner::DefaultSymbol`, which is a u32 newtype with no
  carrier identity. The same u32 value can reference different strings
  in different `StringPool` instances:

  ```rust
  let mut a = StringPool::new();
  let mut b = StringPool::new();
  let sym_a = a.intern("hello");   // u32 = 0
  let sym_b = b.intern("world");   // u32 = 0  (same!)
  assert_eq!(sym_a, sym_b);        // passes — symbols are equal
  // But:
  assert_eq!(a.resolve(sym_a), Some("hello"));
  assert_eq!(b.resolve(sym_a), Some("world"));  // sym_a resolved against b!
  ```

  Gamebryo's `efd::FixedString::m_handle` is a `Char*` (raw pointer
  into the singleton `GlobalStringTable::m_hashArray` buffer) —
  provenance is structurally enforced because there is exactly one
  table by construction (`GlobalStringTable::ms_pGlobalStringTable`).
  Redux replaces this with a `u32` that has no guard rail: cross-pool
  use is silently incorrect, never type-error.

  Today's call sites are clean — `byroredux/src/main.rs:294` inserts
  one production pool, and every production reader goes through that
  single resource. But:

  - `crates/core/src/animation/registry.rs:250-252` (test fixture)
    interns into a `StringPool::new()` throwaway, stores the symbol
    in a `clip.text_keys` / `clip.channels` field, and returns the
    populated clip. If a future test feeds that clip to a runtime
    that resolves symbols against the live engine pool, the resolve
    silently returns the wrong string (or `None`).
  - Tests in `byroredux/src/systems.rs` lines 2149 / 2178 / 2209 /
    2527 / 2662 also use throwaway pools, populating ECS components
    that store the resulting symbols. Correct *because* those tests
    don't share symbols across boundaries — but nothing prevents a
    refactor from introducing the bug.
  - Future M40 streaming may want per-thread parser-local pools
    merged into the main pool at commit. The merge step is
    correctness-critical and will need explicit symbol remapping
    that's currently invisible to the type system.

- **Evidence**: `grep -rn 'StringPool::new()' byroredux/src crates`
  returns 11 hits; all are tests today, but the type system places
  no guard on these symbols leaking out of test scope.
- **Impact**: Latent. No active correctness bug. Cost is paid the
  first time a refactor introduces a multi-pool data-flow without
  noticing the pool boundary.
- **Suggested Fix**: Two options:

  1. **Lightweight**: wrap `DefaultSymbol` in a Redux-owned newtype
     with a phantom marker (`PhantomData<*const StringPool>`) — keeps
     the wire size at 4 bytes but adds a type-level guard. Can stay
     `Copy + Eq` so call sites don't change.
  2. **Heavy**: track a pool ID in each symbol (~8 bytes) and assert
     it matches at resolve time. Catches the bug at runtime instead
     of by static analysis. Costs 4 bytes per FixedString stored
     (significant since every Name component carries one).

  Option 1 is the recommended path — pure compile-time check, zero
  runtime cost.

---

### LC-D6-NEW-03: `StringPool::resolve` returns lowercased canonical form, irreversibly

- **Severity**: LOW (intentional design choice; impact is cosmetic + a
  documentation gap in the api-deep-dive comparison table)
- **Dimension**: String Interning — display fidelity
- **Location**: [crates/core/src/string/mod.rs:31-40](../../crates/core/src/string/mod.rs#L31-L40),
  [crates/debug-server/src/evaluator.rs:654](../../crates/debug-server/src/evaluator.rs#L654)
- **Status**: NEW
- **Description**: `intern` lowercases its input before storing
  (`s.to_ascii_lowercase()`). `resolve` returns the canonical
  lowercased form — there is no path back to the original case once
  a string is interned. The doc-comment notes "Returns the lowercased
  form (canonical representation)" but the api-deep-dive comparison
  table doesn't flag this as a divergence from Gamebryo, where
  `efd::FixedString` preserves case and case-insensitive comparison
  is opt-in via `EqualsNoCase` / `ContainsNoCase`.

  Concrete consequences:

  1. **EDIDs lose authoring case in console output.** The debug
     evaluator at `evaluator.rs:654` resolves
     `name_comp.0` (an `Option<FixedString>`) to print entity names.
     `DocMitchell` displays as `docmitchell`. `MQ01Vault101` displays
     as `mq01vault101`. CamelCase EDIDs are the Bethesda convention
     and are how content authors find their records — losing the
     case hurts dev UX (no visible-correctness regression on shipped
     content).
  2. **Animation channel names display lowercased.** The animation
     test-event hook resolves channel-name `FixedString` symbols via
     `pool.resolve` ([animation/text_events.rs:63](../../crates/core/src/animation/text_events.rs#L63)).
     `Bip01 Spine` reaches gameplay scripts as `bip01 spine`. Most
     scripts compare case-insensitively via `eq_ignore_ascii_case`, so
     no behavioural break, but the canonical-form display is harder
     to grep against the legacy NIF/KF text (which is mixed-case).
  3. **`Name` component loses case** for any UI / book-text use case.
     Workaround already in use: those paths store original case as
     `Arc<str>` (`ImportedNode.name: Option<Arc<str>>`,
     `ImportedMesh.name: Option<Arc<str>>`) and route through the
     pool only when integer equality is needed. So Redux already
     uses two independent string-handling lanes — but the
     api-deep-dive table doesn't mention this duality.

- **Evidence**: Test `resolve_returns_lowercase` at
  [string/mod.rs:117-121](../../crates/core/src/string/mod.rs#L117-L121)
  pins the lossy behaviour as intended:

  ```rust
  let sym = pool.intern("Bip01 Spine");
  assert_eq!(pool.resolve(sym), Some("bip01 spine"));
  ```

  No counterpart that recovers original case.
- **Impact**: Cosmetic — debug logs and console output show
  lowercased names. No behaviour change on shipped content.
- **Suggested Fix**: Two options:

  1. **Documentation-only** (~5 LOC): update
     `docs/legacy/api-deep-dive.md` § "NiFixedString — String
     Interning" to surface the case-insensitivity divergence and
     point at the existing `Arc<str>` lane for case-preserving use.
     Add a `// returns lowercased canonical form` annotation to
     `StringPool::resolve`.
  2. **Keep both** (~30 LOC): change `StringPool` to internally store
     `(lowercased_lookup, original_case_string)`; `intern` keys on
     lowercased, but `resolve` returns the first-seen original. The
     case-insensitive equality contract holds. Costs ~2× pool memory.
     Worth it only if a UI surface needs it.

  Recommend option 1 today — there is no current consumer that needs
  case-preserving resolve, and the api-deep-dive update is a small
  one-time fix.

- **Related**: #866 (FNV-D6-NEW-07, OPEN) —
  `AnimationClipRegistry::get_or_insert_by_path` doesn't lowercase its
  key, so callers that pass mixed-case paths get pool symbols that
  would resolve correctly but registry lookups that miss. Same root
  cause: the case-handling contract is implicit, not enforced at the
  type system.

---

## Existing Open Issues — Verified Still Relevant

| Issue | Title | Note |
|-------|-------|------|
| #866  | `AnimationClipRegistry::get_or_insert_by_path` doesn't lowercase the key — foot-gun for M42 IDLE callers | Same case-handling theme as LC-D6-NEW-03. Fix could land alongside this audit's recommendations. |

---

## Priority Fix Order

1. **LC-D6-NEW-01** (MEDIUM) — Stack-buffer or lookup-first fast path
   in `StringPool::intern` / `get`. ~10 LOC plus doc fixup. Pays off
   on every cell load.
2. **LC-D6-NEW-03** (LOW) — Doc-only update to api-deep-dive.md and
   the StringPool module-level doc-comment. ~10 minutes.
3. **LC-D6-NEW-02** (LOW) — Newtype wrapper around `DefaultSymbol`
   with phantom pool-marker. ~30 LOC + downstream type churn. Defer
   until M40 streaming forces a multi-pool data-flow.
4. **#866** (LOW) — Pair with LC-D6-NEW-03 — same case-handling
   contract surface.

---

## Suggested Next Step

```
/audit-publish docs/audits/AUDIT_LEGACY_COMPAT_2026-05-07_DIM6.md
```
