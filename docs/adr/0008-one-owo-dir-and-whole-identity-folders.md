# One owo dir, and an identity folder that holds all of its own data

owo keeps two kinds of files. Some belong to owo on one machine: the config and the template overrides. The others belong to one identity: its entries, the raw material its entries cite, and the session data of its sync.

There is one owo dir, found like the config folder of any CLI program: the `--config-dir` flag, then `OWO_DIR`, then `$XDG_CONFIG_HOME/owo`. It holds `config.toml` and the templates dir, and by default the identities dir. owo finds the config first, so the config can set the identities dir and the templates dir.

Each identity folder holds its own `sources/` and `sessions/` beside `persona/`, `life/` and `work/`. An identity is then one whole folder, and a copy of it keeps every file that its entries cite.

There is no local config. owo has few settings, and none of them belongs to one repo. The working project comes from `--project` or `OWO_PROJECT`.
