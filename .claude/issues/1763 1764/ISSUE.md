# #1763: TD9-001: NIF heap-allocation regression test never runs in CI (dhat-heap feature dormant)

Severity: LOW · Test Hygiene
Location: `crates/nif/tests/heap_allocation_bounds.rs:30`, `crates/nif/Cargo.toml:28`,
`.github/workflows/ci.yml:31,59`

The NIF heap-budget regression file is gated on the opt-in `dhat-heap`
feature. Its own header claims CI-cadence verification, but ci.yml
only runs `cargo test --workspace` with default features, so
`parse_skyrim_se_single_node_stays_within_heap_budget` and
`parse_skyrim_se_geometry_particle_stays_within_heap_budget` never
execute in CI — pins 4 allocation-hygiene fixes (#832/#833/#831/#408)
that could silently regress.

Suggested fix: add a dedicated CI step running
`cargo test -p byroredux-nif --features dhat-heap --test heap_allocation_bounds`
as its own job (dhat installs a global allocator, must not share a
process with the rest of the suite).

# #1764: TD9-002: ZZZ_probe_ physics test ships dev-probe scaffolding

Severity: LOW · Test Hygiene
Location: `crates/physics/src/water.rs:649` (fn ZZZ_probe_buoyant_body_sleeps_and_sim_quiesces)
+ PROBE: eprintln at :683,687,691,694

A passing test carries a `ZZZ_probe_` name prefix (sort-last + temporary
marker) and 3-4 `eprintln!("PROBE: ...")` diagnostics left over from
investigation commit `1645112ca` (2026-06-20). Cosmetic only.

Suggested fix: rename to a descriptive name
(e.g. buoyant_body_sleeps_so_static_fast_path_re_engages) and delete
the PROBE eprintln lines.
