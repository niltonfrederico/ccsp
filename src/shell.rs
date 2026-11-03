//! Shell integration: a `ccsp` wrapper function that applies `switch` to the current shell.

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Shell {
    Fish,
    Bash,
    Zsh,
}

/// What the current shell's environment should become.
pub enum Env<'a> {
    /// Point Claude Code at a profile. `switched` marks a per-shell `ccsp switch`,
    /// which then wins over the default.
    Set {
        name: &'a str,
        home: PathBuf,
        switched: bool,
    },
    /// Drop the variables ccsp set: back to the plain `~/.claude`.
    Clear,
    /// Nothing ccsp owns to change.
    Keep,
}

pub fn render(shell: Shell, env: &Env) -> Result<String> {
    let out = match (shell, env) {
        (_, Env::Keep) => String::new(),
        (Shell::Fish, Env::Clear) => {
            "set -e CLAUDE_CONFIG_DIR\nset -e CCSP_PROFILE\nset -e CCSP_SWITCHED\ntrue\n".into()
        }
        (_, Env::Clear) => "unset CLAUDE_CONFIG_DIR CCSP_PROFILE CCSP_SWITCHED\n".into(),
        (
            shell,
            Env::Set {
                name,
                home,
                switched,
            },
        ) => {
            let home = home.to_str().context("profile path is not valid UTF-8")?;
            match shell {
                Shell::Fish => {
                    let switched = if *switched {
                        "set -gx CCSP_SWITCHED 1\n"
                    } else {
                        "set -e CCSP_SWITCHED\ntrue\n"
                    };
                    format!(
                        "set -gx CLAUDE_CONFIG_DIR {}\nset -gx CCSP_PROFILE {}\n{switched}",
                        quote_fish(home),
                        quote_fish(name)
                    )
                }
                Shell::Bash | Shell::Zsh => {
                    let switched = if *switched {
                        "export CCSP_SWITCHED=1\n"
                    } else {
                        "unset CCSP_SWITCHED\n"
                    };
                    format!(
                        "export CLAUDE_CONFIG_DIR={}\nexport CCSP_PROFILE={}\n{switched}",
                        quote_posix(home),
                        quote_posix(name)
                    )
                }
            }
        }
    };
    Ok(out)
}

pub fn init_script(shell: Shell) -> String {
    match shell {
        Shell::Fish => FISH.to_owned(),
        Shell::Bash | Shell::Zsh => {
            let name = if shell == Shell::Bash { "bash" } else { "zsh" };
            POSIX.replace("{shell}", name)
        }
    }
}

const FISH: &str = r#"# ccsp shell integration. In ~/.config/fish/config.fish:
#   ccsp init fish | source
function ccsp --description 'Claude Code profile switcher'
    if test "$argv[1]" = switch; and not contains -- --help $argv; and not contains -- -h $argv
        set -l out (command ccsp $argv --shell fish); or return $status
        string join \n -- $out | source
        return 0
    end
    command ccsp $argv; or return $status
    if contains -- "$argv[1]" set-default delete
        command ccsp env --shell fish | source
    end
end
command ccsp env --shell fish | source
"#;

const POSIX: &str = r#"# ccsp shell integration. In your shell rc:
#   eval "$(ccsp init {shell})"
ccsp() {
  if [ "$1" = switch ]; then
    case " $* " in *" -h "*|*" --help "*) command ccsp "$@"; return $? ;; esac
    local __ccsp_out
    __ccsp_out="$(command ccsp "$@" --shell {shell})" || return $?
    eval "$__ccsp_out"
    return 0
  fi
  command ccsp "$@" || return $?
  case "$1" in set-default|delete) eval "$(command ccsp env --shell {shell})" ;; esac
}
eval "$(command ccsp env --shell {shell})"
"#;

fn quote_fish(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}

fn quote_posix(value: &str) -> String {
    format!("'{}'", value.replace('\'', r#"'\''"#))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_should_escape_quotes_when_quoting_for_fish() {
        assert_eq!(quote_fish(r"a'b\c"), r"'a\'b\\c'");
    }

    #[test]
    fn test_should_escape_quotes_when_quoting_for_posix() {
        assert_eq!(quote_posix("a'b"), r#"'a'\''b'"#);
    }

    #[test]
    fn test_should_mark_switched_when_switching() {
        let env = Env::Set {
            name: "work",
            home: PathBuf::from("/p/work/home"),
            switched: true,
        };
        let out = render(Shell::Bash, &env).unwrap();
        assert!(out.contains("export CLAUDE_CONFIG_DIR='/p/work/home'"));
        assert!(out.contains("export CCSP_SWITCHED=1"));
    }
}
