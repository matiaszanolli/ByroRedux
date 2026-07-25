# Scaleform bridge fixtures

The two `.swf.b64` files are synthetic ExternalInterface fixtures from
Ruffle's own test suite at pinned revision
`0dde9813b47fa6b3a202dc497704009334677de1`:

- `tests/tests/swfs/avm1/external_interface/test.swf`
- `tests/tests/swfs/avm2/external_interface/test.swf`

They contain no Bethesda game assets. They are stored as base64 so the binary
fixtures remain reviewable through text-only patches.
