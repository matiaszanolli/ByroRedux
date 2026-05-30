# #1322 Investigation — premise check

## Finding (as filed)
TD8-D8-NEW-01: `pub use legacy::{LegacyFormId, LegacyLoadOrder}` is "pure rot —
kept as a compatibility shim after the per-game legacy stubs were removed under
#390." Proposed fix: delete the re-export and the types, or demote to `pub(crate)`.

## What the code actually shows

**Usage sweep (whole workspace):** the ONLY reference to either type outside
their own definition file (`crates/plugin/src/legacy/mod.rs`) is the single
re-export at `crates/plugin/src/lib.rs:35`. Everything else is the definition +
12 in-module unit tests. Zero consumers in `esm/`, `byroredux/`, other crates,
`tools/`.

**But the premise is wrong.** These are NOT external-compat rot:

1. `crates/plugin/src/legacy/mod.rs:25-28` documents them as forward-looking:
   > The working ESM path lives in `crate::esm` ... The plumbing for converting
   > parsed records into the stable `Record` form will land alongside its first
   > real consumer.

2. Project memory (`modern_plugin_system.md`) names this as the core direction:
   > Design the Form ID resolver as an abstraction that can handle both legacy
   > load-order-based and modern content-addressed resolution.
   `LegacyLoadOrder::resolve()` IS that abstraction (handles ESM/ESP/ESL/ESH +
   save-generated 0xFF slots → stable `FormIdPair`).

3. `pub mod legacy;` (lib.rs:29) is already public, so the `pub use` at :35 only
   *flattens* the path (`plugin::LegacyFormId` vs `plugin::legacy::LegacyFormId`).
   Removing the re-export is nearly cosmetic — the types stay in the public API.

4. The crate does not `deny(warnings)`, and `pub` items in a `pub` module never
   warn `dead_code`. There is no compiler pressure.

## Conclusion
The finding correctly identified "unused public surface" but mis-framed it as
deletable rot. The types are tested, documented design scaffolding for a
cornerstone feature (stable content-addressed FormIds). Deleting them is the
wrong move. The defensible options are: (a) leave as-is and close the finding as
based on a stale premise, (b) demote `pub mod legacy` → `pub(crate)` so the
types leave the *external* API surface while staying available to the future
in-crate consumer (Record/DataStore live in this same crate). Surfaced to user.
