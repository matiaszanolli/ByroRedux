# String Interning

Entity names, asset paths (texture / material / mesh), animation channel
and text-key names, attach-point bone names, and shader identifiers go
through the string interning system. Equality checks are integer
comparisons — O(1), zero allocation after the first intern of a given
string.

Source: `crates/core/src/string/mod.rs`

## Types

### FixedString

```rust
pub type FixedString = string_interner::DefaultSymbol;
```

An opaque handle to an interned string. Cheap to copy, compare, and hash.
Two `FixedString` values are equal if and only if they refer to the same
**canonical (lowercased)** string. Comparison is a single integer
operation.

### StringPool

```rust
pub struct StringPool(
    string_interner::StringInterner<string_interner::backend::StringBackend>,
);
impl Resource for StringPool {}
impl Default for StringPool { /* delegates to new() */ }
```

The global string table, registered as an ECS resource. Backed by the
`string-interner` crate (`= "0.17"`, pinned in the workspace `Cargo.toml`)
using its `StringBackend`. The backend allocates exactly once per unique
canonical string the first time it is seen.

| Method | Signature | Notes |
|--------|-----------|-------|
| `new()` | `→ Self` | Empty pool |
| `intern(s)` | `&mut self, &str → FixedString` | Get or create (case-insensitive) |
| `resolve(sym)` | `&self, FixedString → Option<&str>` | Symbol → **lowercased** string |
| `get(s)` | `&self, &str → Option<FixedString>` | Lookup without interning (case-insensitive) |

## Case-insensitive interning

Since #216 (`fix(ecs): case-insensitive StringPool …`), `intern` and `get`
**ASCII-lowercase the input before hashing**, to match Gamebryo's
`GlobalStringTable` behavior. `"Bip01 Head"`, `"bip01 head"`, and
`"BIP01 HEAD"` all produce the same symbol. This is correct for the
engine's consumers of interned strings — asset paths, EDIDs, animation
channel / node names — which are all case-insensitive in the source data.

Two consequences worth knowing:

- **`resolve` returns the canonical *lowercased* form**, not the case the
  caller originally passed to `intern`. The original casing is not
  preserved (#895 / LC-D6-NEW-03 documents this divergence from
  Gamebryo's case-preserving `NiFixedString`). This API is therefore
  *wrong* for any case-preserving use such as book/UI text or paths
  surfaced to mod authors. Those sites carry an `Arc<str>` alongside the
  `FixedString` instead — see `ImportedNode` / `ImportedMesh` (with their
  `Option<Arc<str>>` `name` fields) in `crates/nif/src/import/types.rs`.
- Only ASCII bytes (`0x41..=0x5A`) are folded; multi-byte UTF-8 sequences
  are left untouched, so a string like `"Naïve Café"` round-trips as
  `"naïve café"` and stays valid UTF-8.

## Allocation-free case-fold fast path

The lowercasing itself is allocation-free for short inputs (#893 /
LC-D6-NEW-01). A `const LOWERCASE_STACK_BUF: usize = 256` stack buffer
holds the case-folded copy for inputs ≤ 256 bytes — sized to cover every
string observed in vanilla Bethesda content (longest BSA asset path
~120 bytes; longest NIF node name ~64 bytes). The private
`ascii_lowercase_into_buf` helper copies the bytes into the stack buffer,
calls `<[u8]>::make_ascii_lowercase`, and returns a `&str` via an
`unsafe { from_utf8_unchecked }` (safe because the source was already a
`&str` and only single-byte ASCII codepoints are touched).

Inputs longer than 256 bytes fall back to an allocated
`s.to_ascii_lowercase()`. So the per-call cost for ≥ 99 % of engine call
sites is one stack copy plus one hash lookup — no `String` allocation. The
interner backend still allocates once per *new* unique canonical string.

The boundary is exact and exercised by regression tests: 256 bytes fits
the fast path, 257 falls back; both paths share the pool and case-fold
identically.

## Usage

```rust
// At startup
world.insert_resource(StringPool::new());

// Interning a string (requires write access)
let sym = world.resource_mut::<StringPool>().intern("Player");

// Resolving (read access only) — note: returns lowercased canonical form
let pool = world.resource::<StringPool>();
let name: &str = pool.resolve(sym).unwrap();  // "player"

// Lookup without interning (case-insensitive)
let maybe = pool.get("PLAYER");  // Some(sym) if "player" was previously interned
let nope  = pool.get("unknown"); // None
```

## Relation to Gamebryo's NiFixedString

Gamebryo's `NiFixedString` uses a `GlobalStringHandle` from a global string
table — conceptually identical to our `DefaultSymbol` from `string-interner`.
Both are integer handles with O(1) equality, and both fold case in the
global table.

Two differences:

- Gamebryo's `NiFixedString` has an implicit `operator const char*()`
  conversion. Our `FixedString` requires explicit `pool.resolve(sym)`.
  This is intentionally safer — no risk of dangling pointers or accidental
  string operations in hot paths.
- Gamebryo's `NiFixedString` is *case-preserving* (it stores the original
  casing for display), whereas `StringPool::resolve` returns the
  lowercased canonical form. See #895 — case-preserving sites in Redux use
  a parallel `Arc<str>` rather than relying on `resolve`.

## Where It's Used

- **`Name` component** — `Name(pub FixedString)` in
  `crates/core/src/ecs/components/name.rs`; sparse-stored, only entities
  that need identification (actors, triggers, markers, quest objects)
  carry one.
- **`World::find_by_name(&str)`** — in `crates/core/src/ecs/world.rs`;
  resolves `&str` through the pool with `get` (case-insensitive, never
  interns), then scans `Name` components for a matching symbol. Returns
  `None` if the string was never interned or no entity has that name.
- **Asset / texture / material paths** — realized by #609 (D6-NEW-01,
  bundled with #231). `MaterialInfo` and `ImportedMesh` carry
  `Option<FixedString>` for their ten-plus texture-slot fields
  (`texture_path`, `normal_map`, `glow_map`, `detail_map`, `gloss_map`,
  `dark_map`, `parallax_map`, `env_map`, `env_mask`, `material_path`, …) in
  `crates/nif/src/import/material/mod.rs` and
  `crates/nif/src/import/types.rs`. A shared
  `intern_texture_path(pool, &str)` helper centralises the
  empty-string-collapses-to-`None` invariant. The NIF import entry points
  (`import_nif_scene`, `import_nif`, `import_nif_with_collision`) and the
  walkers thread `&mut StringPool` through to every slot. The cell
  loader's per-REFR `RefrTextureOverlay` (XATO/XTNM/XTXR shadow in
  `byroredux/src/cell_loader/refr.rs`) also stores `Option<FixedString>`,
  so REFR overlays share the dedup table with `ImportedMesh`; the
  BGSM/BGEM merge path in `byroredux/src/asset_provider.rs` interns into
  the same engine pool.
- **Animation channels and text keys** — `AnimationClip` keys its
  transform/float/color/bool/texture-flip channels and text-key events by
  `FixedString`, pre-interned at clip load time
  (`crates/core/src/animation/types.rs`).
- **Attach points** — `AttachPoint` stores `name` /
  `parent_bone: Option<FixedString>`, and the sibling
  `ChildAttachConnections` stores `connect_names: Vec<FixedString>`,
  so the equip hot path does integer compares, not string compares
  (`crates/core/src/ecs/components/attach_points.rs`).
