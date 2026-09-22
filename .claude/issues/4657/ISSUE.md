# PAR-D1-2026-09-21-06: The sfmaterial value readers recurse without a depth bound, and an 84-byte CDB aborts the process

Labels: medium,bug,import-pipeline,game:starfield

## Description
`crates/sfmaterial/src/reader.rs`: `read_value` <-> `read_user_class` <-> `read_primitive_ref` (`:846-871`, `:922-993`, `:1025-1056`), and `skip_value` <-> `skip_user_class` (`:727-778`), have no depth counter anywhere in their signatures or bodies.

- A `CLAS` (IS_STRUCT) whose single inline field has its own class as type, plus one `OBJT` of that class, recurses without bound. The same happens with a user-flagged self-reference through chunk fields, one chunk per level.
- `ParseLimits::max_instances` counts top-level object chunks only — it does not protect against depth.
- `parse_with_limits`'s doc tells "callers loading untrusted or memory-constrained content" to choose a finite limit. That limit does not protect against this recursion.

Verified unchanged at HEAD `ee6d3fb39`: none of `read_value`, `read_user_class`, `skip_value`, `skip_user_class` takes a depth parameter.

## Evidence
Probe `cdb`:

```
self-referential CDB: 84 bytes; probe_header -> Ok(4)
parse_with_limits(max_instances: 1_000_000)       -> thread 'main' has overflowed its stack ... aborting (exit 134)
validate_instances_with_limits(max_instances: 1_000_000) -> same abort
```

## Impact
Latent today: production uses only `peek_magic` and `probe_header` (`byroredux/src/asset_provider/material/cdb.rs:141-149`), which accept this file without recursing. `visit_*`/`validate_instances_with_limits` are documented as the Phase-2 (#3398) entry points, which is where this becomes reachable. It is still unbounded today and is an empirically confirmed process abort, not a theoretical one.

## Related
#3398 (OPEN — Starfield CDB Phase 2 presence-only tracker; distinct bug from this recursion DoS, not re-filed against it per the audit-publish policy for a touched-not-refiled open item), #2614, #2623, #3055, #4274

## Suggested Fix
Thread a depth counter through the read and skip recursion (vanilla nesting is shallow; a cap of 64 is ample). Return `Error::NestingTooDeep` and add the 84-byte fixture as a test before #3398 wires the full parse.

Source: docs/audits/AUDIT_PARSERS_2026-09-21.md (PAR-D1-2026-09-21-06)

## Completeness Checks
- [ ] **SIBLING**: Same pattern checked in related files (other archive/material/animation readers)
- [ ] **TESTS**: A regression test pins this specific fix