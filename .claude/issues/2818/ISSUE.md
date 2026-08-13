# REN-D19-06: extract_tangents_from_extra_data has no test coverage despite load-bearing tangent/bitangent swap

Labels: low, nif-parser, bug

## Description

The site of the load-bearing #786 `CalcTangentSpace` swap — Bethesda's `tangents` field holds `∂P/∂V` and `bitangents` holds `∂P/∂U`, so the decoder reads the **second** 12-byte half into `Vertex.tangent.xyz` — has **no test coverage**, while every other tangent producer is unit-tested. Untested consequences: the half-swap itself, the `blob.len() != num_verts * 24` size gate (whose failure is a silent warn + fall-through to synthesis), the exact extra-data name match, and the `zup_to_yup_pos` application to both halves. Code reads correct today; the symptom of a regression is "chrome-looking walls", which this project has a standing rule to *mis*attribute to missing textures.

## Location

`crates/nif/src/import/mesh/tangent.rs` (`extract_tangents_from_extra_data`)

## Source

Filed from docs/audits/AUDIT_RENDERER_2026-08-12b.md (finding REN-D19-06).

https://github.com/matiaszanolli/ByroRedux/issues/2818
