# Issue #248 — PERF-04-11-L3

**Title**: NiUnknown.type_name cloned as String on every unknown block
**Severity**: LOW
**Dimension**: NIF Parse
**Audit**: `docs/audits/AUDIT_PERFORMANCE_2026-04-11.md`
**Location**: `crates/nif/src/lib.rs:169,224,248`; `blocks/mod.rs:578,600`

## Summary
`NiUnknown { type_name: type_name.to_string(), data }` clones the type name. The name already lives in the header's block-type table. Switch to `Cow<'static, str>` or `Arc<str>`.

## Sibling
#245 (same `NiUnknown` struct — bundle them)

## Fix with
`/fix-issue 248`
