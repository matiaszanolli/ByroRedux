# D4-09: NiMaterialProperty ambient/diffuse colors discarded in material path

**Severity:** LOW | nif-parser, enhancement
**Source:** `docs/audits/AUDIT_LEGACY_COMPAT_2026-04-10.md`

## Problem
`NiMaterialProperty` carries ambient + diffuse + specular + emissive. Importer captures specular/emissive/shininess/alpha but discards ambient + diffuse entirely.

## Suggested fix
Add `diffuse_color` + `ambient_color` to Material. Apply diffuse as multiplicative tint; use ambient for ambient-light modulation.
