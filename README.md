# ccsp

Switch [Claude Code](https://claude.com/claude-code) between accounts — say, work and personal —
without logging out and back in.

Each profile is a Claude Code config directory with its own login. `ccsp switch` points the
current shell at it through `CLAUDE_CONFIG_DIR`; `ccsp set-default` picks the profile every new
shell starts with.

## Install

```sh
cargo install --git https://github.com/niltonfrederico/ccsp
```

Then add the shell integration (`switch` changes the current shell, so it needs it):

```sh
# fish: ~/.config/fish/config.fish
ccsp init fish | source

# bash: ~/.bashrc  /  zsh: ~/.zshrc
eval "$(ccsp init bash)"   # or: ccsp init zsh
```

## Usage

```sh
ccsp create work                # shared profile; opens `claude auth login`
ccsp create personal --isolated # own sessions and history
ccsp create lab --isolated --bare # own everything, no global config

ccsp set-default personal       # every new shell starts here
ccsp switch work                # this shell only
ccsp switch                     # this shell back to the default

ccsp list                       # * default, > current shell
ccsp current
ccsp login work --sso           # sign in again; args go to `claude auth login`
ccsp sync                       # pick up new entries in ~/.claude
ccsp delete work                # moves the profile to the trash
ccsp set-default --unset        # new shells use the plain ~/.claude
```

## Profiles

| | shared (default) | `--isolated` | `--isolated --bare` |
|---|---|---|---|
| Login (`.credentials.json`, `oauthAccount`) | own | own | own |
| Settings, `CLAUDE.md`, skills, commands, hooks, agents, plugins | linked to `~/.claude` | linked to `~/.claude` | own, empty |
| Sessions, history, `projects/`, todos, file history | linked to `~/.claude` | own | own |
| `.claude.json` (MCP servers, trusted projects, onboarding) | copy, without the account | copy, without the account | fresh |

"Linked" means a symlink per top-level entry of `~/.claude`, so editing your global
`CLAUDE.md` or adding a skill reaches every profile that replicates the configuration.
Entries bound to the account or to a running service (credentials, `statsig`, `daemon`,
`jobs`, caches, telemetry, locks) are never linked.

Profiles live in `${XDG_DATA_HOME:-~/.local/share}/ccsp/profiles/<name>/home` (override the
root with `CCSP_HOME`). `~/.claude` itself is never written to.

### Caveats

- **`.claude.json` is copied, not linked**, because the login lives in it. MCP servers added
  later with `claude mcp add --scope user` land in the active profile only.
- **A link can turn into a file.** If Claude Code rewrites a linked file by replacing it
  (write a temporary file, then rename), the profile ends up with its own copy. `ccsp switch`,
  `ccsp sync` and new shells warn when that happens; ccsp never deletes the copy.
- **New shells only.** The default is applied by the shell integration, so Claude Code started
  outside an integrated shell (an IDE launched from the desktop, say) uses `~/.claude`.
- Linux only for now; on macOS Claude Code keeps credentials in the Keychain.

## Development

```sh
cargo test
cargo clippy --all-targets -- -D warnings
```

## License

MIT
