//! Load-order activation of a legacy plugin profile (PRD legacy-mod-compatibility, Phase 1).
//!
//! The profile is an ordered list of plugins, each with the masters its `TES4` header declares. Activation turns it into
//! the order the fold reads, without aborting on what an original engine would crash on:
//!
//! - **Masters first (LMC-ID-003).** A plugin loads after every master it declares. A master the profile lists *after*
//!   one of its dependents is hoisted to just ahead of the first dependent that needs it, and the move is reported. No
//!   other reordering is performed: among plugins with no dependency relation the profile order stands, and it is
//!   what decides overrides (the fold's last-writer rule).
//! - **Missing masters (LMC-ID-011).** A plugin whose declared master is not in the profile is disabled, with every
//!   plugin that transitively depends on it; each is reported with the reason, and activation carries on with the
//!   rest. The original engines crash before the main menu instead (UESP, *Oblivion Mod:PCS Library/CTD on load*).
//! - **Identity (LMC-ID-001, -009).** Names are matched by [`plugin_key`]: case-folded basenames, `\` and `/`
//!   equivalent. The second entry with an identity already in the profile is dropped and reported (LMC-ID-010): as a
//!   repeat when both entries hold the same bytes, as an identity collision when they differ.
//!
//! The module is pure over [`ProfilePlugin`]s; [`read_profile_plugin`] fills one from a file by reading its `TES4`
//! record and nothing else, and [`Activation::compare_duplicates`] with [`same_file_content`] tells the two kinds of
//! duplicate apart.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use anyhow::{Context, Result};
use byroredux_core::form_id::plugin_key;

use crate::esm::reader::{EsmReader, FileHeader, GameKind};

/// One plugin of a profile, in profile order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfilePlugin {
    /// The name or path the profile lists it under, kept as given for the report and for opening the file.
    pub name: String,
    /// The masters its header declares, in `MAST` order, as written. `None` when the header could not be read: the
    /// plugin is then disabled ([`DisableCause::Unreadable`]) and so is everything that depends on it.
    pub masters: Option<Vec<String>>,
    /// Why the header could not be read, when it could not.
    pub unreadable: Option<String>,
    /// The game its header was written for, when the header was read from a file; it picks the archive-pairing rule
    /// (LMC-AST-002, [`crate::archive_pairing`]).
    pub game: Option<GameKind>,
}

impl ProfilePlugin {
    /// A plugin whose header has been read.
    pub fn new(name: impl Into<String>, masters: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            name: name.into(),
            masters: Some(masters.into_iter().map(Into::into).collect()),
            unreadable: None,
            game: None,
        }
    }

    /// A plugin whose header could not be read.
    pub fn unreadable(name: impl Into<String>, error: impl Into<String>) -> Self {
        Self { name: name.into(), masters: None, unreadable: Some(error.into()), game: None }
    }
}

/// Why a plugin of the profile is not activated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisableCause {
    /// It declares `master`, which is not in the profile (LMC-ID-011).
    MissingMaster { master: String },
    /// It declares the profile plugin at `master`, which is itself disabled (LMC-ID-011's transitive dependents).
    DisabledMaster { master: usize },
    /// Its masters lead back to it. `cycle` lists the profile positions around the loop, starting and ending here. No
    /// shipped content does this; it is reported rather than looped on.
    MasterCycle { cycle: Vec<usize> },
    /// It has the identity of the plugin at `first`, which the profile lists earlier (LMC-ID-010). The earlier entry
    /// loads either way; `content` says whether this one was a repeat or a different file under the same name.
    Duplicate { first: usize, content: DuplicateContent },
    /// Its header could not be read.
    Unreadable { error: String },
}

/// How a duplicate's content compares with the plugin it duplicates (LMC-ID-010, the sandbox design's PKG-013).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DuplicateContent {
    /// Not compared: [`activate`] works on names alone, and [`Activation::compare_duplicates`] fills this in; it stays
    /// unknown when either file cannot be read.
    Unknown,
    /// The same bytes, or the same file listed twice: a repeat.
    Same,
    /// Different bytes under one identity: an identity collision.
    Different,
}

/// A plugin the activation left out, by profile position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disabled {
    pub plugin: usize,
    pub cause: DisableCause,
}

/// A master moved ahead of a dependent that the profile listed before it (LMC-ID-003), by profile position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hoist {
    pub master: usize,
    /// The plugin it now loads ahead of: the first in profile order that needed it, directly or through another
    /// hoisted master.
    pub ahead_of: usize,
}

/// The activated profile.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Activation {
    /// The profile positions of the activated plugins, in the order the fold reads them.
    pub order: Vec<usize>,
    pub hoists: Vec<Hoist>,
    /// The disabled plugins, in profile order.
    pub disabled: Vec<Disabled>,
}

impl Activation {
    /// Whether the profile loads as listed: nothing moved and nothing left out.
    pub fn is_unchanged(&self) -> bool {
        self.hoists.is_empty() && self.disabled.is_empty()
    }

    /// The disabled entry for the plugin at `plugin`, if it is disabled.
    pub fn disabled_entry(&self, plugin: usize) -> Option<&Disabled> {
        self.disabled.iter().find(|entry| entry.plugin == plugin)
    }

    /// Compare each duplicate's content with the plugin it duplicates, by their profile names, through `same_content`
    /// (`None` when it cannot tell; [`same_file_content`] for files). Only the duplicate pairs are compared.
    pub fn compare_duplicates(
        &mut self,
        profile: &[ProfilePlugin],
        mut same_content: impl FnMut(&str, &str) -> Option<bool>,
    ) {
        for entry in &mut self.disabled {
            let DisableCause::Duplicate { first, content } = &mut entry.cause else { continue };
            let (Some(earlier), Some(later)) = (profile.get(*first), profile.get(entry.plugin)) else { continue };
            *content = match same_content(&earlier.name, &later.name) {
                Some(true) => DuplicateContent::Same,
                Some(false) => DuplicateContent::Different,
                None => DuplicateContent::Unknown,
            };
        }
    }

    /// The report lines: one per hoist and one per disabled plugin, each disabled dependent naming the chain back to
    /// the plugin whose own problem started it (LMC-ID-011, "report the chain"). Empty when [`Self::is_unchanged`].
    pub fn report(&self, profile: &[ProfilePlugin]) -> Vec<String> {
        // The file name as the profile spells it; a path's directories are noise in a report about plugins.
        let name = |position: usize| {
            profile.get(position).map_or("?", |plugin| plugin.name.rsplit(['/', '\\']).next().unwrap_or(&plugin.name))
        };
        let mut lines = Vec::new();
        for hoist in &self.hoists {
            lines.push(format!(
                "hoisted: {} now loads ahead of {}, which declares it as a master (the profile listed it later)",
                name(hoist.master),
                name(hoist.ahead_of)
            ));
        }
        for entry in &self.disabled {
            let reason = match &entry.cause {
                DisableCause::MissingMaster { master } => format!("its master {master} is not in the profile"),
                DisableCause::DisabledMaster { master } => {
                    let mut chain = vec![name(*master).to_owned()];
                    let mut at = *master;
                    while let Some(Disabled { cause: DisableCause::DisabledMaster { master: next }, .. }) = self.disabled_entry(at) {
                        if chain.len() > profile.len() {
                            break;
                        }
                        chain.push(name(*next).to_owned());
                        at = *next;
                    }
                    let root = match self.disabled_entry(at).map(|entry| &entry.cause) {
                        Some(DisableCause::MissingMaster { master }) => format!(" (missing master {master})"),
                        Some(DisableCause::MasterCycle { .. }) => " (master cycle)".to_owned(),
                        Some(DisableCause::Unreadable { .. }) => " (unreadable)".to_owned(),
                        _ => String::new(),
                    };
                    format!("its master is disabled: {}{root}", chain.join(" <- "))
                }
                DisableCause::MasterCycle { cycle } => {
                    format!("its masters form a cycle: {}", cycle.iter().map(|&p| name(p)).collect::<Vec<_>>().join(" -> "))
                }
                DisableCause::Duplicate { first, content: DuplicateContent::Unknown } => {
                    format!("the profile already lists it as {}", name(*first))
                }
                DisableCause::Duplicate { first, content: DuplicateContent::Same } => {
                    format!("the profile already lists it as {}, with the same content", name(*first))
                }
                DisableCause::Duplicate { first, content: DuplicateContent::Different } => format!(
                    "identity collision (LMC-ID-010): {} has the identity of {}, which the profile lists earlier, but \
                     different content; the earlier one loads",
                    profile.get(entry.plugin).map_or("?", |plugin| plugin.name.as_str()),
                    profile.get(*first).map_or("?", |plugin| plugin.name.as_str())
                ),
                DisableCause::Unreadable { error } => format!("its header could not be read: {error}"),
            };
            lines.push(format!("disabled: {} — {reason}", name(entry.plugin)));
        }
        lines
    }
}

/// Activate `profile` (see the module docs). Deterministic: the same profile always activates the same way.
pub fn activate(profile: &[ProfilePlugin]) -> Activation {
    let count = profile.len();
    let mut cause: Vec<Option<DisableCause>> = vec![None; count];

    // Identity: the first plugin with a key owns it; a later one with the same key is a duplicate.
    let mut by_key: HashMap<String, usize> = HashMap::with_capacity(count);
    for (position, plugin) in profile.iter().enumerate() {
        match by_key.get(&plugin_key(&plugin.name)) {
            Some(&first) => cause[position] = Some(DisableCause::Duplicate { first, content: DuplicateContent::Unknown }),
            None => {
                by_key.insert(plugin_key(&plugin.name), position);
            }
        }
    }

    // Each plugin's masters as profile positions, sorted by profile position so a hoist keeps the profile's relative
    // order among the masters it moves. A master that is not in the profile disables the plugin.
    let mut masters: Vec<Vec<usize>> = vec![Vec::new(); count];
    for (position, plugin) in profile.iter().enumerate() {
        if cause[position].is_some() {
            continue;
        }
        let Some(declared) = &plugin.masters else {
            cause[position] = Some(DisableCause::Unreadable {
                error: plugin.unreadable.clone().unwrap_or_else(|| "no header".to_owned()),
            });
            continue;
        };
        for master in declared {
            match by_key.get(&plugin_key(master)) {
                Some(&found) if found != position => masters[position].push(found),
                // A plugin naming itself as a master is a one-plugin cycle.
                Some(_) => {
                    cause[position] = Some(DisableCause::MasterCycle { cycle: vec![position, position] });
                    break;
                }
                None => {
                    cause[position] = Some(DisableCause::MissingMaster { master: master.clone() });
                    break;
                }
            }
        }
        masters[position].sort_unstable();
        masters[position].dedup();
    }

    // Cycles among the rest: an iterative three-colour walk, so a hostile profile cannot grow the stack.
    {
        #[derive(Clone, Copy, PartialEq)]
        enum Mark {
            New,
            Open,
            Done,
        }
        let mut mark = vec![Mark::New; count];
        for start in 0..count {
            if mark[start] != Mark::New || cause[start].is_some() {
                continue;
            }
            let mut stack: Vec<(usize, usize)> = vec![(start, 0)];
            mark[start] = Mark::Open;
            while let Some(&mut (node, ref mut next)) = stack.last_mut() {
                if let Some(&master) = masters[node].get(*next) {
                    *next += 1;
                    match mark[master] {
                        Mark::New if cause[master].is_none() => {
                            mark[master] = Mark::Open;
                            stack.push((master, 0));
                        }
                        Mark::Open => {
                            let from = stack.iter().position(|&(n, _)| n == master).expect("an open node is on the stack");
                            let mut cycle: Vec<usize> = stack[from..].iter().map(|&(n, _)| n).collect();
                            cycle.push(master);
                            for &member in &stack[from..] {
                                let member = member.0;
                                if cause[member].is_none() {
                                    // Each member reports the loop starting from itself.
                                    let at = cycle.iter().position(|&n| n == member).expect("member is in the cycle");
                                    let mut own: Vec<usize> = cycle[at..cycle.len() - 1].to_vec();
                                    own.extend_from_slice(&cycle[..at]);
                                    own.push(member);
                                    cause[member] = Some(DisableCause::MasterCycle { cycle: own });
                                }
                            }
                        }
                        _ => {}
                    }
                } else {
                    mark[node] = Mark::Done;
                    stack.pop();
                }
            }
        }
    }

    // Transitive dependents of a disabled plugin, to a fixed point. Each names the first disabled master it declares.
    loop {
        let mut changed = false;
        for position in 0..count {
            if cause[position].is_some() {
                continue;
            }
            if let Some(&master) = masters[position].iter().find(|&&master| cause[master].is_some()) {
                cause[position] = Some(DisableCause::DisabledMaster { master });
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }

    // Order: profile order, each plugin preceded by any of its masters not yet placed (a post-order walk from each
    // plugin in turn, masters in profile order). A master placed during the walk from a plugin it follows in the profile
    // is a hoist ahead of that plugin.
    let mut placed = vec![false; count];
    let mut order = Vec::with_capacity(count);
    let mut hoists = Vec::new();
    for root in 0..count {
        if placed[root] || cause[root].is_some() {
            continue;
        }
        let mut stack: Vec<(usize, usize)> = vec![(root, 0)];
        while let Some(&mut (node, ref mut next)) = stack.last_mut() {
            if let Some(&master) = masters[node].get(*next) {
                *next += 1;
                if !placed[master] && !stack.iter().any(|&(n, _)| n == master) {
                    stack.push((master, 0));
                }
            } else {
                stack.pop();
                placed[node] = true;
                order.push(node);
                if node > root {
                    hoists.push(Hoist { master: node, ahead_of: root });
                }
            }
        }
    }

    let disabled = cause
        .into_iter()
        .enumerate()
        .filter_map(|(plugin, cause)| cause.map(|cause| Disabled { plugin, cause }))
        .collect();
    Activation { order, hoists, disabled }
}

/// Whether the files at `a` and `b` hold the same bytes: true at once for one file under two spellings, false at once
/// for two lengths, otherwise compared in chunks up to the first difference.
pub fn same_file_content(a: &Path, b: &Path) -> std::io::Result<bool> {
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(a), std::fs::canonicalize(b))
        && a == b
    {
        return Ok(true);
    }
    if std::fs::metadata(a)?.len() != std::fs::metadata(b)?.len() {
        return Ok(false);
    }
    // Fill `buffer` from `reader`, short only at the end of the file.
    fn fill(reader: &mut impl Read, buffer: &mut [u8]) -> std::io::Result<usize> {
        let mut filled = 0;
        while filled < buffer.len() {
            match reader.read(&mut buffer[filled..])? {
                0 => break,
                read => filled += read,
            }
        }
        Ok(filled)
    }
    let (mut a, mut b) = (std::fs::File::open(a)?, std::fs::File::open(b)?);
    let (mut chunk_a, mut chunk_b) = (vec![0u8; 1 << 16], vec![0u8; 1 << 16]);
    loop {
        let (read_a, read_b) = (fill(&mut a, &mut chunk_a)?, fill(&mut b, &mut chunk_b)?);
        if chunk_a[..read_a] != chunk_b[..read_b] {
            return Ok(false);
        }
        if read_a == 0 {
            return Ok(true);
        }
    }
}

/// Read `path`'s `TES4` header, and only that record, into a [`ProfilePlugin`] named `path`. A file that cannot be
/// opened or whose header does not parse becomes an [`ProfilePlugin::unreadable`] entry, so activation can report it
/// and carry on.
pub fn read_profile_plugin(path: &str) -> ProfilePlugin {
    match read_plugin_header(Path::new(path)) {
        Ok((header, game)) => ProfilePlugin { game: Some(game), ..ProfilePlugin::new(path, header.master_files) },
        Err(error) => ProfilePlugin::unreadable(path, format!("{error:#}")),
    }
}

/// The `TES4` header of the plugin at `path` and the game it was written for, reading the header record alone (a few
/// hundred bytes) rather than the file.
pub fn read_plugin_header(path: &Path) -> Result<(FileHeader, GameKind)> {
    let mut file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    // The 24-byte Tes5+ record header covers the 20-byte Oblivion one; the data size sits at offset 4 in both.
    let mut prefix = Vec::with_capacity(24);
    (&mut file).take(24).read_to_end(&mut prefix).with_context(|| format!("read {}", path.display()))?;
    anyhow::ensure!(prefix.len() >= 8 && &prefix[..4] == b"TES4", "{} does not start with a TES4 record", path.display());
    let data_size = u32::from_le_bytes(prefix[4..8].try_into().expect("four bytes")) as u64;
    // A header larger than this is not a plugin header; refuse rather than read a whole file into memory.
    anyhow::ensure!(data_size <= 1 << 20, "{}: TES4 record claims {data_size} bytes", path.display());
    file.take(data_size).read_to_end(&mut prefix).with_context(|| format!("read {}", path.display()))?;
    let mut reader = EsmReader::new(&prefix);
    let header = reader.read_file_header().with_context(|| format!("parse the TES4 header of {}", path.display()))?;
    let game = GameKind::from_header(reader.variant(), header.hedr_version, header.record_version);
    Ok((header, game))
}

#[cfg(test)]
#[path = "activation_tests.rs"]
mod tests;
