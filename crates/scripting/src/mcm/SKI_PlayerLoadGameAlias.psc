ScriptName SKI_PlayerLoadGameAlias extends ReferenceAlias
{The engine's SKI_PlayerLoadGameAlias (LMC-UI-002): SkyUI's player alias forwards each game load to its
config quest. The engine raises the config scripts' OnGameReload itself after every load, so this
alias does nothing; it exists so the alias a config quest carries resolves.}
