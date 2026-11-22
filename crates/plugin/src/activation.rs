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
//!   equivalent. The second entry with an identity already in the profile is dropped and reported (LMC-ID-010).
//!
//! The module is pure over [`ProfilePlugin`]s; [`read_profile_plugin`] fills one from a file by reading its `TES4`
//! record and nothing else.

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
}

impl ProfilePlugin {
    /// A plugin whose header has been read.
    pub fn new(name: impl Into<String>, masters: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self { name: name.into(), masters: Some(masters.into_iter().map(Into::into).collect()), unreadable: None }
    }

    /// A plugin whose header could not be read.
    pub fn unreadable(name: impl Into<String>, error: impl Into<String>) -> Self {
        Self { name: name.into(), masters: None, unreadable: Some(error.into()) }
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
    /// It has the identity of the plugin at `first`, which the profile lists earlier (LMC-ID-010).
    Duplicate { first: usize },
    /// Its header could not be read.
    Unreadable { error: String },
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
                DisableCause::Duplicate { first } => format!("the profile already lists it as {}", name(*first)),
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
            Some(&first) => cause[position] = Some(DisableCause::Duplicate { first }),
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

/// Read `path`'s `TES4` header, and only that record, into a [`ProfilePlugin`] named `path`. A file that cannot be
/// opened or whose header does not parse becomes an [`ProfilePlugin::unreadable`] entry, so activation can report it
/// and carry on.
pub fn read_profile_plugin(path: &str) -> ProfilePlugin {
    match read_plugin_header(Path::new(path)) {
        Ok((header, _)) => ProfilePlugin::new(path, header.master_files),
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
