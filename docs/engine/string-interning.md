# String Interning

All entity names, asset paths, and shader identifiers go through the
string interning system. Equality checks are integer comparisons — O(1),
zero allocation after first intern.

Source: `crates/core/src/string/mod.rs`

## Types

### FixedString

```rust
pub type FixedString = string_interner::DefaultSymbol;
```

An opaque handle to an interned string. Cheap to copy, compare, and hash.
Two `FixedString` values are equal if and only if they refer to the same
string. Comparison is a single integer operation.

### StringPool

```rust
pub struct StringPool(StringInterner<StringBackend>);
impl Resource for StringPool {}
```

The global string table, registered as an ECS resource.

| Method | Signature | Notes |
|--------|-----------|-------|
| `new()` | `→ Self` | Empty pool |
| `intern(s)` | `&mut self, &str → FixedString` | Get or create |
| `resolve(sym)` | `&self, FixedString → Option<&str>` | Symbol → string |
| `get(s)` | `&self, &str → Option<FixedString>` | Lookup without interning |

## Usage

```rust
// At startup
world.insert_resource(StringPool::new());

// Interning a string (requires write access)
let sym = world.resource_mut::<StringPool>().intern("player");

// Resolving (read access only)
let pool = world.resource::<StringPool>();
let name: &str = pool.resolve(sym).unwrap();

// Lookup without interning
let maybe = pool.get("player");  // Some(sym) if previously interned
let nope = pool.get("unknown");  // None
```

## Relation to Gamebryo's NiFixedString

Gamebryo's `NiFixedString` uses a `GlobalStringHandle` from a global string
table — conceptually identical to our `DefaultSymbol` from `string-interner`.
Both are integer handles with O(1) equality.

Key difference: Gamebryo's `NiFixedString` has implicit `operator const char*()`
conversion. Our `FixedString` requires explicit `pool.resolve(sym)`. This is
intentionally safer — no risk of dangling pointers or accidental string
operations in hot paths.

## Where It's Used

- **`Name` component** — entity identification via `Name(FixedString)`
- **`World::find_by_name()`** — resolves `&str` through pool, then scans Name components
- **Future:** asset paths, shader identifiers, animation sequence names
