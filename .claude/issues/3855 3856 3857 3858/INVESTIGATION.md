# INVESTIGATION — the `boot.rs` split is not mechanical (#3855)

The issue calls this "the lowest-risk split in the bucket — the boundaries
already exist as function boundaries." The function boundaries were indeed
clean. What was not visible from the issue is that **`boot.rs` is read as
text by 30 assertions**, and the split silently invalidates all of them.

## The hidden coupling

Nine `include_str!` consumers, in two places:

- `byroredux/src/scheduler_access_tests.rs:20` — `const BOOT_RS: &str =
  include_str!("boot.rs")`, feeding 21 assertion sites.
- Four `#[cfg(test)]` modules inside `boot.rs` itself, each re-declaring its
  own `const BOOT_SRC: &str = include_str!("boot.rs")`.

They exist because the invariants are only observable as source: *which types
a system declares* and *what order systems are registered in* are not
reachable from `cargo test` without a Vulkan device and on-disk game data.

Deleting `boot.rs` breaks the build (a missing file is a hard error), which is
the benign half. The dangerous half is what a naive repair does: retargeting
each test to `include_str!` of whichever new file it now lives in **narrows
the search space silently**. `every_parallel_system_declares_everything_it_
acquires` scans for registrations across all five stages; pointed at
`schedule/mod.rs` alone it would find none, and the panic message would blame
a missing declaration rather than a missing file.

## The fix and its own trap

`boot::SOURCES` — `concat!(include_str!(…), …)` over every production file,
which every consumer now reads. `concat!` does expand nested `include_str!`
(verified before relying on it). All 30 assertion sites keep their original
text unchanged.

**The concatenation order is load-bearing, and this is the non-obvious part.**
Each source-shape module truncates the string at its *own* `mod` declaration:

```rust
let setup = BOOT_SRC.split("mod fragment_activation_order_tests").next()...
```

so that the module's own mentions of a system name cannot be what a scan
finds. That convention only holds while every production registration
precedes the earliest test module's text. The natural order — `mod`
declaration order, which puts `schedule/mod.rs` before `schedule/early.rs` —
puts four test modules ahead of all five stage files and truncates the entire
schedule away. **Six tests went red exactly that way.**

Then the pin written to catch it reintroduced it from the other side: a module
in `mod.rs` (first in the concat) containing the sentinel *as a literal* puts
`"mod fragment_activation_order_tests"` at offset ~0 of `SOURCES`. Same six
tests, same failure, new cause. The pin now assembles sentinels at run time
(`format!("mod {module}")`) and strips `mod.rs`'s own contribution off the
front before scanning, so it cannot satisfy or truncate its own assertions.

## What is pinned now

- `every_production_file_precedes_the_first_test_module` — per sentinel × per
  file, with one distinctive witness registration each, so a file is proven
  present rather than the concatenation merely being non-empty.
- `the_concat_list_covers_every_file_in_the_boot_directory` — reads `boot/`
  from disk and asserts each `.rs` is in the `concat!`. A future
  `boot/foo.rs` carrying registrations that nobody adds to `SOURCES` would
  otherwise be invisible to every scheduler test, which would keep passing
  while covering less than they claim.

Both were negative-tested: break the order → witness assertion fires; add an
unlisted file → coverage assertion fires.

## Generalisation

Any file-level split in this repo must first ask **who reads this file as
text**. `grep -rn 'include_str!("<file>")'` before moving anything. The same
question applies to #3856 and #3857.
