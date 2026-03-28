# Testing

All tests are in-module (`#[cfg(test)] mod tests`) within the source files
they cover. Run with `cargo test -p gamebyro-core`.

## Test Coverage: 57 Tests

### Storage Backends (13 tests)

**SparseSetStorage** — `crates/core/src/ecs/sparse_set.rs` (7 tests)
- `insert_and_get` — basic insert + lookup
- `overwrite` — re-insert same entity overwrites, doesn't duplicate
- `swap_remove` — remove from middle, verify swap-remove fixes pointers
- `remove_last` — remove last element (no swap needed)
- `remove_nonexistent` — returns None
- `iter_all` — iteration covers all entities
- `iter_mut_modify` — mutation through iterator

**PackedStorage** — `crates/core/src/ecs/packed.rs` (6 tests)
- `insert_maintains_sort_order` — out-of-order inserts stay sorted
- `overwrite` — re-insert overwrites, doesn't duplicate
- `remove_middle` — remove from middle maintains sort
- `remove_nonexistent` — returns None
- `iteration_is_sorted` — iteration order matches entity ID order
- `iter_mut_modify` — mutation through iterator

### World (34 tests)

**Basic operations** — `crates/core/src/ecs/world.rs` (6 tests)
- `spawn_and_insert` — spawn entity, insert two component types
- `different_storage_backends` — sparse + packed coexist
- `remove_component` — remove returns the value
- `mutate_component` — get_mut modifies in place
- `get_nonexistent` — missing component/entity returns None
- `lazy_storage_init` — count/has work before any insert

**Single-component queries** (5 tests)
- `query_read_single` — read query with get/len
- `query_write_single` — write query with mutation
- `query_write_insert_remove` — insert/remove through QueryWrite
- `query_returns_none_for_unregistered` — None for unknown types
- `query_after_register` — register without insert, query succeeds (empty)

**Multi-component queries** (5 tests)
- `multiple_read_queries_coexist` — two QueryReads at once
- `query_2_mut_read_and_write` — read A + write B simultaneously
- `query_2_mut_mut_both_writable` — write A + write B simultaneously
- `query_2_mut_same_type_panics` — same type → panic (deadlock prevention)
- `intersection_iteration` — iterate velocity, look up position

**Iteration** (2 tests)
- `query_iter` — sum values across iterator
- `query_iter_mut` — mutate all values through iterator

**Resources** (10 tests)
- `resource_insert_and_read` — insert then read
- `resource_insert_and_mutate` — insert, mutate, verify
- `two_resource_types_coexist` — multiple resource types readable at once
- `missing_resource_panics_with_type_name` — panic includes type name
- `missing_resource_mut_panics` — panics with "not found"
- `remove_resource_returns_value` — remove returns the value, gone afterward
- `remove_nonexistent_resource_returns_none` — None for missing
- `resource_overwrite` — second insert replaces first
- `resource_visible_to_system_via_scheduler` — system reads resource inside scheduler.run()
- `try_resource_returns_none_when_missing` — non-panicking variant

**Name + StringPool** (6 tests)
- `name_component_attach_and_query` — attach Name, resolve through pool
- `find_by_name_hit` — find_by_name returns correct entity
- `find_by_name_miss` — wrong name returns None
- `find_by_name_no_pool` — no StringPool resource → None (no panic)
- `find_by_name_no_name_components` — string interned but no entities → None
- `string_pool_as_world_resource` — pool accessible as resource

### Scheduler (6 tests)

`crates/core/src/ecs/scheduler.rs`
- `closure_system` — closure modifies world through query
- `struct_system` — struct implementing System trait
- `systems_run_in_order` — atomic counter verifies order
- `mutation_visible_to_next_system` — system 1 writes, system 2 reads
- `empty_scheduler_runs_cleanly` — no panic
- `system_names_in_order` — correct names in registration order

### String Interning (4 tests)

`crates/core/src/string/mod.rs`
- `intern_same_string_returns_same_symbol` — dedup
- `different_strings_different_symbols` — distinct
- `resolve_round_trips` — intern then resolve
- `get_without_interning` — lookup without side effects

## Running Tests

```bash
# All core tests
cargo test -p gamebyro-core

# Specific module
cargo test -p gamebyro-core -- ecs::world
cargo test -p gamebyro-core -- ecs::sparse_set
cargo test -p gamebyro-core -- string

# Full workspace (includes renderer/platform compilation check)
cargo test
```
