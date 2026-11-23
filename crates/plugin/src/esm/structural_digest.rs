//! Order-independent structural digest of a value's `Debug` text.
//!
//! `EsmIndex` has no `PartialEq` (hundreds of record types, and float fields where `NaN != NaN`), and its `Debug` text
//! cannot be compared verbatim: every `HashMap` / `HashSet` prints in its own randomly seeded order.
//! [`structural_digest`] hashes that text with the entries of every `{…}` block made order-independent: map, set and
//! struct entries alike (a struct entry carries its field name, so sorting loses nothing), while `[…]` and `(…)` keep
//! their order. Every field reachable through `Debug` is covered, including ones added later, and only one index is
//! held at a time. The load-order tests compare two walks with it (#3813); the decode probe
//! ([`super::decode_probe`], LMC-REC-010) asks whether a sub-record's removal changes the index.

use std::collections::hash_map::DefaultHasher;
use std::fmt::Write as _;
use std::hash::Hasher;

/// Order-independent structural digest of `value`'s `Debug` text.
pub fn structural_digest(value: &impl std::fmt::Debug) -> u64 {
    let mut digest = DebugDigest {
        blocks: vec![Block::new(None)],
        quote: None,
        escaped: false,
    };
    write!(digest, "{value:?}").expect("DebugDigest never rejects a write");
    assert!(
        digest.quote.is_none() && digest.blocks.len() == 1,
        "unbalanced Debug text"
    );
    digest.blocks.pop().expect("the root block").finish()
}

/// One bracketed block of `Debug` text, or the whole text (`open == None`).
struct Block {
    open: Option<u8>,
    entries: Vec<u64>,
    entry: DefaultHasher,
    entry_empty: bool,
}

impl Block {
    fn new(open: Option<u8>) -> Self {
        Self {
            open,
            entries: Vec::new(),
            entry: DefaultHasher::new(),
            entry_empty: true,
        }
    }

    fn feed(&mut self, bytes: &[u8]) {
        if !bytes.is_empty() {
            self.entry.write(bytes);
            self.entry_empty = false;
        }
    }

    fn end_entry(&mut self) {
        if !std::mem::replace(&mut self.entry_empty, true) {
            let entry = std::mem::replace(&mut self.entry, DefaultHasher::new());
            self.entries.push(entry.finish());
        }
    }

    fn finish(mut self) -> u64 {
        self.end_entry();
        if self.open == Some(b'{') {
            self.entries.sort_unstable();
        }
        let mut block = DefaultHasher::new();
        block.write_u8(self.open.unwrap_or(0));
        for entry in &self.entries {
            block.write_u64(*entry);
        }
        block.finish()
    }
}

struct DebugDigest {
    blocks: Vec<Block>,
    /// The closing quote of the string or char literal being read.
    quote: Option<u8>,
    escaped: bool,
}

impl std::fmt::Write for DebugDigest {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        let bytes = s.as_bytes();
        // Start of the pending run of entry text; literals stay in the run.
        let mut run = 0;
        for (i, &b) in bytes.iter().enumerate() {
            if let Some(quote) = self.quote {
                if self.escaped {
                    self.escaped = false;
                } else if b == b'\\' {
                    self.escaped = true;
                } else if b == quote {
                    self.quote = None;
                }
                continue;
            }
            match b {
                b'"' | b'\'' => self.quote = Some(b),
                b'{' | b'[' | b'(' | b'}' | b']' | b')' | b',' | b' ' | b'\n' | b'\r' | b'\t' => {
                    self.blocks.last_mut().expect("root").feed(&bytes[run..i]);
                    run = i + 1;
                    match b {
                        b'{' | b'[' | b'(' => self.blocks.push(Block::new(Some(b))),
                        b'}' | b']' | b')' => {
                            let block = self.blocks.pop().expect("root");
                            let expected_open = match b {
                                b'}' => b'{',
                                b']' => b'[',
                                _ => b'(',
                            };
                            assert_eq!(block.open, Some(expected_open), "unbalanced Debug text");
                            let nested = block.finish();
                            let parent = self.blocks.last_mut().expect("root");
                            // 0xFF never occurs in UTF-8, so a nested digest
                            // cannot alias entry text.
                            parent.entry.write_u8(0xFF);
                            parent.entry.write_u64(nested);
                            parent.entry_empty = false;
                        }
                        b',' => self.blocks.last_mut().expect("root").end_entry(),
                        // Whitespace outside a literal is layout only.
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        self.blocks.last_mut().expect("root").feed(&bytes[run..]);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn digest_ignores_hash_order_but_not_sequence_order_or_values() {
        let entry = |k: u32| (k, vec![format!("v{k}"), "w".to_string()]);
        let a: HashMap<u32, Vec<String>> = (0..64).map(entry).collect();
        let b: HashMap<u32, Vec<String>> = (0..64).rev().map(entry).collect();
        assert_ne!(format!("{a:?}"), format!("{b:?}"), "the fixture must print in two orders");
        assert_eq!(structural_digest(&a), structural_digest(&b));

        let mut reordered = b.clone();
        reordered.get_mut(&7).unwrap().reverse();
        assert_ne!(structural_digest(&a), structural_digest(&reordered), "`[…]` order is data");
        let mut rebound = b;
        let (seven, eight) = (rebound[&7].clone(), rebound[&8].clone());
        rebound.insert(7, eight);
        rebound.insert(8, seven);
        assert_ne!(structural_digest(&a), structural_digest(&rebound), "a key stays bound to its value");

        // Brackets, commas and quotes inside literals are text, never structure;
        // misreading one unbalances the blocks and panics.
        structural_digest(&(vec!["{", "(", "[", ","], "\"{", '\'', '"', '{'));
    }
}
