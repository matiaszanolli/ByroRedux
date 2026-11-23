//! A game's own UI archive under every mod tier (PRD legacy-mod-compatibility LMC-AST-005, LMC-AST-010; #7336).
//!
//! The UI read its files from the vanilla archives alone: the Scaleform HUD and `ui.open` from the interface archive,
//! the MenuXml HUD and menus from the Misc and texture archives. A mod's menu replacement (catalog U1, U3), font
//! replacement (U5) or HUD texture therefore never reached the screen. A [`LayeredSources`] resolves a path through
//! the tiers the other asset providers layer, highest priority first: each root's loose files (the last mod root
//! first; a loose file beats any archive), then the session's archives under the given flags with the last listed
//! winning (plugin-paired mod archives come after the game's own, #3637), then the base archives.

use super::archive::{open_loose_tiers, Archive};

/// Sources a UI path resolves through, highest priority first.
pub(crate) struct LayeredSources {
    tiers: Vec<Archive>,
}

impl LayeredSources {
    /// `base` (the game's own archives, lowest priority first) under the session's archives named by `flags` and the
    /// loose tiers that hold a file under one of `roots` (lowercase, `\`-terminated: `interface\`). An archive `base`
    /// already holds is not added twice; an unreadable one is left out with a warning, as the other providers do.
    pub(crate) fn over(base: Vec<Archive>, args: &[String], roots: &[&str], flags: &[&str]) -> Self {
        let mut tiers: Vec<Archive> = open_loose_tiers(args)
            .into_iter()
            .rev()
            .filter(|loose| loose.all_paths().iter().any(|path| roots.iter().any(|root| path.starts_with(root))))
            .collect();
        let held: Vec<String> = base.iter().map(|archive| archive.path().to_lowercase()).collect();
        let mut listed: Vec<String> = Vec::new();
        for flag in flags {
            listed.extend(crate::cli_args::parse_all_string_args(args, flag));
        }
        for path in listed.iter().rev() {
            if held.contains(&path.to_lowercase()) {
                continue;
            }
            match Archive::open_shared(path) {
                Ok((archive, _)) => tiers.push(archive),
                Err(error) => log::warn!("UI sources: {path}: {error}"),
            }
        }
        tiers.extend(base.into_iter().rev());
        Self { tiers }
    }

    /// The Scaleform sources over the vanilla interface archive at `vanilla`. Errors only when that archive cannot be
    /// opened.
    pub(crate) fn interface(args: &[String], vanilla: &str) -> Result<Self, String> {
        let (archive, _) = Archive::open_shared(vanilla)?;
        Ok(Self::over(vec![archive], args, &["interface\\"], &["--bsa"]))
    }

    /// How many tiers a lookup consults.
    pub(crate) fn tier_count(&self) -> usize {
        self.tiers.len()
    }

    /// `path` from the highest tier that holds it. A present but unreadable entry is warned about once, as
    /// [`Archive::extract_or_warn`] does, and the lookup does not fall through to a lower tier's copy.
    pub(crate) fn extract(&self, path: &str) -> Option<Vec<u8>> {
        self.tiers.iter().find(|tier| tier.contains(path))?.extract_or_warn(path)
    }
}

impl byroredux_ui::ScaleformResourceProvider for LayeredSources {
    fn load(&self, path: &str) -> std::io::Result<Option<Vec<u8>>> {
        match self.tiers.iter().find(|tier| tier.contains(path)) {
            Some(tier) => tier.extract(path).map(Some),
            None => Ok(None),
        }
    }
}
