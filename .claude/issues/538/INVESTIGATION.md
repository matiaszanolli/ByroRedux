# #538 + #543 Investigation — classification byte offset

## Domain
**esm** — `crates/plugin/src/esm/records/weather.rs` DATA arm.

## Strategy

Issue body (#538) mandates: "byte-sample 3–5 WTHRs with known
classifications before choosing the offset." Hypothesis from single-
record audit evidence was byte 11; confirm across multiple records
whose EDID encodes the expected classification.

Reference classification flag bits:
- `0x01` = PLEASANT
- `0x02` = CLOUDY
- `0x04` = RAINY
- `0x08` = SNOW

Plan:
1. Throwaway scratch test dumps the full 15-byte DATA payload (hex) and
   EDID for every WTHR in FNV/FO3/Oblivion. Keyword-filter to pick
   records whose EDID contains `Clear`/`Storm`/`Rain`/`Snow`/
   `Cloud`/`Overcast`/`Dust`.
2. For each candidate, record the byte at each plausible offset (10–14).
3. Pick the offset where each keyword consistently matches its expected
   flag bit.
4. Patch the parser; delete the scratch; update the comment per #543.

## Files touched (expected)

1. `crates/plugin/src/esm/records/weather.rs` — offset + comment
2. `crates/plugin/src/esm/records/weather.rs` — unit test update
3. `crates/plugin/tests/parse_real_esm.rs` — classification-matches-EDID
   assertion for at least one keyword-bearing weather

2 code files. Scratch test gets deleted before commit.
