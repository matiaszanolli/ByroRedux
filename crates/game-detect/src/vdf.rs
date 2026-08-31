//! A minimal reader for Valve's KeyValues text format (`.vdf` / `.acf`).
//!
//! Steam stores both the library list (`steamapps/libraryfolders.vdf`) and each
//! app's install record (`steamapps/appmanifest_<appid>.acf`) in this format:
//!
//! ```text
//! "libraryfolders"
//! {
//!     "0"
//!     {
//!         "path"      "/home/u/.local/share/Steam"
//!         "apps"
//!         {
//!             "22380"     "9907238641"
//!         }
//!     }
//! }
//! ```
//!
//! A key is a quoted string followed by either a quoted value or a nested
//! block. That is the entire grammar we need, so this is a ~100-line hand
//! parser rather than a dependency — and it is only ever pointed at files
//! Steam wrote.
//!
//! Duplicate keys are legal in KeyValues, so entries are kept in an ordered
//! `Vec` rather than a map; [`Value::get`] returns the first match, which is
//! the same precedence Steam's own reader applies.

use std::collections::BTreeMap;

/// One parsed KeyValues node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    String(String),
    Object(Vec<(String, Value)>),
}

impl Value {
    /// First child with this key, if this node is an object.
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(entries) => entries
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(key))
                .map(|(_, value)| value),
            Value::String(_) => None,
        }
    }

    /// First child with this key, as a string.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Value::String(value) => Some(value),
            Value::Object(_) => None,
        }
    }

    /// Children of this node, or an empty slice for a leaf.
    pub fn entries(&self) -> &[(String, Value)] {
        match self {
            Value::Object(entries) => entries,
            Value::String(_) => &[],
        }
    }

    /// Flatten a `"key" "value"` block into a map, skipping nested objects.
    pub fn string_map(&self) -> BTreeMap<String, String> {
        self.entries()
            .iter()
            .filter_map(|(key, value)| match value {
                Value::String(value) => Some((key.clone(), value.clone())),
                Value::Object(_) => None,
            })
            .collect()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum VdfError {
    #[error("unterminated quoted string at byte {0}")]
    UnterminatedString(usize),
    #[error("unexpected '{found}' at byte {at}")]
    Unexpected { found: char, at: usize },
    #[error("unclosed block opened at byte {0}")]
    UnclosedBlock(usize),
}

/// Parse a whole KeyValues document into its implicit root object.
///
/// The outermost `"name" { … }` pair becomes a single entry of the returned
/// root, so `parse(text)?.get("libraryfolders")` reaches the real content.
pub fn parse(text: &str) -> Result<Value, VdfError> {
    let bytes: Vec<char> = text.chars().collect();
    let mut cursor = Cursor { bytes, at: 0 };
    let entries = cursor.parse_entries(None)?;
    Ok(Value::Object(entries))
}

struct Cursor {
    bytes: Vec<char>,
    at: usize,
}

impl Cursor {
    /// Parse entries until end-of-input, or until the `}` closing the block
    /// that opened at `opened_at`.
    fn parse_entries(
        &mut self,
        opened_at: Option<usize>,
    ) -> Result<Vec<(String, Value)>, VdfError> {
        let mut entries = Vec::new();
        loop {
            self.skip_trivia();
            let Some(&ch) = self.bytes.get(self.at) else {
                return match opened_at {
                    Some(at) => Err(VdfError::UnclosedBlock(at)),
                    None => Ok(entries),
                };
            };
            match ch {
                '}' => {
                    if opened_at.is_none() {
                        return Err(VdfError::Unexpected {
                            found: '}',
                            at: self.at,
                        });
                    }
                    self.at += 1;
                    return Ok(entries);
                }
                '"' => {
                    let key = self.parse_quoted()?;
                    self.skip_trivia();
                    match self.bytes.get(self.at) {
                        Some('{') => {
                            let opened = self.at;
                            self.at += 1;
                            let nested = self.parse_entries(Some(opened))?;
                            entries.push((key, Value::Object(nested)));
                        }
                        Some('"') => {
                            let value = self.parse_quoted()?;
                            entries.push((key, Value::String(value)));
                        }
                        // A key with neither a value nor a block is malformed;
                        // skip it rather than failing the whole file, so one
                        // odd line cannot hide an otherwise-readable library.
                        _ => continue,
                    }
                }
                other => {
                    return Err(VdfError::Unexpected {
                        found: other,
                        at: self.at,
                    })
                }
            }
        }
    }

    fn parse_quoted(&mut self) -> Result<String, VdfError> {
        let opened = self.at;
        self.at += 1; // opening quote
        let mut out = String::new();
        while let Some(&ch) = self.bytes.get(self.at) {
            match ch {
                '"' => {
                    self.at += 1;
                    return Ok(out);
                }
                '\\' => {
                    self.at += 1;
                    match self.bytes.get(self.at) {
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        // Windows paths arrive as `C:\\Games`, so an escaped
                        // backslash must collapse to one, not two.
                        Some(&other) => out.push(other),
                        None => return Err(VdfError::UnterminatedString(opened)),
                    }
                    self.at += 1;
                }
                other => {
                    out.push(other);
                    self.at += 1;
                }
            }
        }
        Err(VdfError::UnterminatedString(opened))
    }

    fn skip_trivia(&mut self) {
        loop {
            while self.bytes.get(self.at).is_some_and(|c| c.is_whitespace()) {
                self.at += 1;
            }
            if self.bytes.get(self.at) == Some(&'/') && self.bytes.get(self.at + 1) == Some(&'/') {
                while self
                    .bytes
                    .get(self.at)
                    .is_some_and(|&c| c != '\n' && c != '\r')
                {
                    self.at += 1;
                }
                continue;
            }
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim shape of a real `libraryfolders.vdf`, tabs and all.
    const LIBRARY_FOLDERS: &str = "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"/home/u/.local/share/Steam\"\n\t\t\"label\"\t\t\"\"\n\t\t\"apps\"\n\t\t{\n\t\t\t\"228980\"\t\t\"215150090\"\n\t\t}\n\t}\n\t\"1\"\n\t{\n\t\t\"path\"\t\t\"/mnt/data/SteamLibrary\"\n\t\t\"apps\"\n\t\t{\n\t\t\t\"22380\"\t\t\"9907238641\"\n\t\t\t\"489830\"\t\t\"1\"\n\t\t}\n\t}\n}\n";

    #[test]
    fn a_real_library_folders_document_parses_to_its_paths_and_apps() {
        let root = parse(LIBRARY_FOLDERS).unwrap();
        let folders = root.get("libraryfolders").unwrap();
        let paths: Vec<&str> = folders
            .entries()
            .iter()
            .filter_map(|(_, entry)| entry.get_str("path"))
            .collect();
        assert_eq!(
            paths,
            ["/home/u/.local/share/Steam", "/mnt/data/SteamLibrary"]
        );

        let second = &folders.entries()[1].1;
        let apps = second.get("apps").unwrap().string_map();
        assert!(apps.contains_key("22380"));
        assert!(apps.contains_key("489830"));
    }

    #[test]
    fn an_app_manifest_parses_to_its_install_dir() {
        let text = "\"AppState\"\n{\n\t\"appid\"\t\t\"22380\"\n\t\"name\"\t\t\"Fallout: New Vegas\"\n\t\"installdir\"\t\t\"Fallout New Vegas\"\n}\n";
        let state = parse(text).unwrap();
        let state = state.get("AppState").unwrap();
        assert_eq!(state.get_str("installdir"), Some("Fallout New Vegas"));
        assert_eq!(state.get_str("name"), Some("Fallout: New Vegas"));
    }

    /// Keys are matched case-insensitively, because Steam is inconsistent
    /// about them across formats (`AppState` vs `libraryfolders`).
    #[test]
    fn keys_match_case_insensitively() {
        let root =
            parse("\"AppState\"\n{\n\t\"InstallDir\"\t\"Skyrim Special Edition\"\n}\n").unwrap();
        assert_eq!(
            root.get("appstate").unwrap().get_str("installdir"),
            Some("Skyrim Special Edition")
        );
    }

    /// Windows library paths are escaped in the file; collapsing them wrong
    /// would produce a path that never matches anything on disk.
    #[test]
    fn escaped_windows_paths_collapse_to_single_separators() {
        let root = parse(
            "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\t\"D:\\\\SteamLibrary\"\n\t}\n}\n",
        )
        .unwrap();
        assert_eq!(
            root.get("libraryfolders").unwrap().entries()[0]
                .1
                .get_str("path"),
            Some("D:\\SteamLibrary")
        );
    }

    #[test]
    fn comments_and_blank_lines_are_skipped() {
        let root =
            parse("// leading comment\n\"AppState\"\n{\n\n\t// inner\n\t\"appid\" \"1\"\n}\n")
                .unwrap();
        assert_eq!(root.get("AppState").unwrap().get_str("appid"), Some("1"));
    }

    /// Duplicate keys are legal; the first wins, matching Steam's own reader.
    #[test]
    fn duplicate_keys_resolve_to_the_first() {
        let root = parse("\"a\" \"one\"\n\"a\" \"two\"\n").unwrap();
        assert_eq!(root.get_str("a"), Some("one"));
        assert_eq!(root.entries().len(), 2);
    }

    #[test]
    fn malformed_documents_are_rejected_rather_than_half_read() {
        assert!(matches!(
            parse("\"AppState\"\n{\n\t\"appid\" \"1\"\n"),
            Err(VdfError::UnclosedBlock(_))
        ));
        assert!(matches!(
            parse("\"unterminated"),
            Err(VdfError::UnterminatedString(_))
        ));
        assert!(matches!(
            parse("}"),
            Err(VdfError::Unexpected { found: '}', .. })
        ));
    }
}
