# One owo dir, and an identity folder that holds all of its own data

owo keeps two kinds of files. Some belong to owo on one machine: the config and the template overrides. The others belong to one identity: its entries, the raw material its entries cite, and the session data of its sync. Before this decision the two kinds were mixed in one folder, the root. The root held the identities, a `sources/<slug>/` folder for each identity, the session data of all identities in `.owo/sessions/`, the template overrides and the config. A second config, `.owo.toml`, was read from the working folder and its parents.

That layout had three problems. The config was in the folder it had to find, so it could not set that folder. An identity was not whole: a copy of its folder lost the raw material that its entries cite, and its sync cursors. And the settings of a machine, such as the ingest command, travelled with the identities.

So there is one owo dir, found like the config folder of any CLI program: the `--config-dir` flag, then `OWO_DIR`, then `$XDG_CONFIG_HOME/owo`. It holds `config.toml` and the templates dir, and by default the identities dir. The config can set the identities dir and the templates dir, because owo finds the config before either. Each identity folder holds its own `sources/` and `sessions/` beside `persona/`, `life/` and `work/`. An identity is then one whole folder, and two identities dirs on one machine cannot mix their session data.

There is no local config. owo has few settings, and none of them belongs to one repo. The one value that differs by repo is the working project, and it comes from `--project` or `OWO_PROJECT`.

This has costs. No file in `sources/` or `sessions/` is an entry, so the load of an identity and the Git calls of `changes` and `lint --changed` leave both folders out by a rule in the code, and that rule must hold in every new walk of an identity folder. The session data are working files of one machine inside a folder that people commit, so `owo new` writes a `.gitignore` with `sessions/`, and lint warns when that line is missing. A citation lost its slug prefix, so the identities and the config of the older layout need a move by hand.

The other choice kept the root and moved only the config to the XDG folder. It would have fixed the first problem and left an identity in three places.
