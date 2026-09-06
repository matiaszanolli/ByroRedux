//! The route catalog: which `Provider.Function(...)` calls are legal
//! and what SDK route each lowers to.

use super::*;

/// One manifest-published route addressable by Papyrus source or PEX.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "save", derive(serde::Serialize, serde::Deserialize))]
pub struct PapyrusProviderRoute {
    pub(crate) qualified_name: String,
    pub(crate) declaration: ScriptFunctionDeclaration,
}

impl PapyrusProviderRoute {
    /// Principal-qualified engine route used for authenticated dispatch.
    pub fn qualified_name(&self) -> &str {
        &self.qualified_name
    }

    /// Validated SDK declaration backing this route.
    pub fn declaration(&self) -> &ScriptFunctionDeclaration {
        &self.declaration
    }
}

/// Case-insensitive provider/function catalog projected from installed
/// extension manifests.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PapyrusProviderCatalog {
    providers: BTreeSet<String>,
    routes: BTreeMap<(String, String), PapyrusProviderRoute>,
}

impl PapyrusProviderCatalog {
    /// Catalog of exact extender-era aliases implemented by engine services.
    pub fn engine_compatibility() -> Self {
        let mut catalog = Self::default();
        for function in papyrus_game_content_declarations() {
            catalog
                .insert_route(function.route.to_owned(), &function.declaration, false)
                .expect("built-in Papyrus compatibility declaration is valid");
        }
        for function in papyrus_input_declarations() {
            catalog
                .insert_route(function.route.to_owned(), &function.declaration, false)
                .expect("built-in Input compatibility declaration is valid");
        }
        for function in papyrus_ui_declarations() {
            catalog
                .insert_route(function.route.to_owned(), &function.declaration, false)
                .expect("built-in UI compatibility declaration is valid");
        }
        for function in papyrus_storage_util_declarations() {
            catalog
                .insert_route(function.route.to_owned(), &function.declaration, false)
                .expect("built-in StorageUtil compatibility declaration is valid");
        }
        for function in papyrus_legacy_container_declarations() {
            catalog
                .insert_route(function.route, &function.declaration, false)
                .expect("built-in JContainers compatibility declaration is valid");
        }
        for function in papyrus_mod_event_declarations() {
            catalog
                .insert_route(function.route, &function.declaration, false)
                .expect("built-in ModEvent compatibility declaration is valid");
        }
        catalog
    }

    /// Insert one declared function when it publishes a Papyrus alias.
    ///
    /// The operation is atomic: a duplicate alias or invalid declaration does
    /// not modify the catalog.
    pub fn insert(
        &mut self,
        extension: &ExtensionId,
        declaration: &ScriptFunctionDeclaration,
    ) -> Result<(), PapyrusProviderCatalogError> {
        self.insert_route(declaration.qualified_name(extension), declaration, true)
    }

    fn insert_route(
        &mut self,
        qualified_name: String,
        declaration: &ScriptFunctionDeclaration,
        strict_provider: bool,
    ) -> Result<(), PapyrusProviderCatalogError> {
        declaration
            .validate()
            .map_err(PapyrusProviderCatalogError::InvalidDeclaration)?;
        let Some(alias) = declaration.papyrus.as_ref() else {
            return Ok(());
        };
        let key = alias.canonical_key();
        if self.routes.contains_key(&key) {
            return Err(PapyrusProviderCatalogError::DuplicateAlias {
                provider: alias.provider.clone(),
                function: alias.function.clone(),
            });
        }
        let route = PapyrusProviderRoute {
            qualified_name,
            declaration: declaration.clone(),
        };
        if strict_provider {
            self.providers.insert(key.0.clone());
        }
        self.routes.insert(key, route);
        Ok(())
    }

    /// Resolve a Papyrus spelling using the language's case-insensitive rules.
    pub fn resolve(&self, provider: &str, function: &str) -> Option<&PapyrusProviderRoute> {
        self.routes
            .get(&(provider.to_ascii_lowercase(), function.to_ascii_lowercase()))
    }

    pub(crate) fn contains_provider(&self, provider: &str) -> bool {
        self.providers.contains(&provider.to_ascii_lowercase())
    }
}
