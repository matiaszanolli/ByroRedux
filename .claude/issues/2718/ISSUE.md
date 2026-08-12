# #2718: FO4 host-method catalog is validated against 3 of 311 menus; an uncataloged method is a hard AVM2 error

- **Severity**: MEDIUM
- **Dimension**: 5 (content-driven failure modes)
- **Location**: [`crates/ui/src/catalog.rs`](../../crates/ui/src/catalog.rs):192-331 · [`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs):599-656, 989
- **Status**: NEW
- **Description**: The generated adapter installs **exactly one forwarder per
  catalog entry** onto the movie's `BGSCodeObj` object — 138 for
  `Fallout4Avm2`. Any `BGSCodeObj.Foo(...)` the menu makes that is not in the
  catalog therefore resolves to an absent property on a dynamic object, which
  in AVM2 is a call on `undefined` (`Error #1006`), not a no-op. The catalog is
  a hand-transcribed inventory of a third-party reconstruction of the vanilla
  ActionScript sources, and the guard against it being incomplete is
  `installed_fallout4_host_calls_are_cataloged`
  ([`crates/ui/src/avm2_host.rs`](../../crates/ui/src/avm2_host.rs):989) — which
  inspects **three** movies and is `#[ignore]`d.
- **Evidence**: the install loop emits one `SetProperty` per cataloged method
  and nothing else:
  ```rust
  // crates/ui/src/avm2_host.rs:755
  for (helper, property) in helper_multinames.iter().zip(&method_property_multinames) {
      install_ops.extend([
          Op::GetLocal { index: 2 },
          Op::GetLex { index: *helper },
          Op::SetProperty { index: *property },
      ]);
  }
  ```
  There is no catch-all forwarder, no dynamic-proxy interception, and no fallback that turns an
  unknown method into a recorded `Unknown` dispatch — the `unknown_methods`
  bookkeeping in [`crates/ui/src/host.rs`](../../crates/ui/src/host.rs) only ever sees calls that *reached* the bridge,
  which an absent property never does.
- **Impact**: 308 of 311 shipped menus are unverified against the catalog. Each
  miss aborts the executing ActionScript frame handler at the call site, so the
  symptom is a menu that renders but stops responding — with the true cause
  (one missing string in a Rust table) invisible from the failure.
- **Related**: SAFEUI-03.
- **Suggested Fix**: Give the adapter a fallback path so an uncataloged method
  degrades into a recorded `ScaleformHostDispatch::Unknown` returning `null`,
  rather than throwing — this is also the only way `unknown_methods()` can ever
  become a useful diagnostic. Separately, widen
  `installed_fallout4_host_calls_are_cataloged` from 3 movies to the full
  archive sweep.

---

---
**Source**: `docs/audits/AUDIT_SAFETY_UI_2026-08-12.md` (finding `SAFEUI-04`)

## Completeness Checks
- [ ] **UNSAFE**: If the fix adds `unsafe`, a safety comment states the upheld invariant
- [ ] **SIBLING**: Same pattern checked in related files
- [ ] **DROP**: If Vulkan/wgpu objects change, the Drop impl is still reverse-order correct
- [ ] **TESTS**: A regression test pins this specific fix (prefer a default-suite test, not `#[ignore]`d)

