mod claude_json;
mod layout;
mod paths;
mod profile;
mod shell;
mod state;

use std::io::{BufRead, IsTerminal, Write};
use std::process::{Command, ExitCode};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};

use crate::paths::Paths;
use crate::profile::{Meta, Mode, Profile};
use crate::shell::{Env, Shell};
use crate::state::State;

/// Switch Claude Code between accounts.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create a profile and sign in to its account.
    Create {
        name: String,
        /// Keep the profile's own sessions and history instead of sharing ~/.claude.
        #[arg(long)]
        isolated: bool,
        /// Start without replicating the global configuration.
        #[arg(long, requires = "isolated")]
        bare: bool,
        /// Skip `claude auth login`; run `ccsp login <name>` later.
        #[arg(long)]
        no_login: bool,
    },
    /// Delete a profile (moves it to the trash).
    Delete {
        name: String,
        /// Do not ask for confirmation.
        #[arg(short, long)]
        yes: bool,
    },
    /// Set the profile every new shell starts with.
    SetDefault {
        /// Profile name; omit with --unset.
        #[arg(required_unless_present = "unset")]
        name: Option<String>,
        /// Clear the default: new shells use the plain ~/.claude.
        #[arg(long, conflicts_with = "name")]
        unset: bool,
    },
    /// Switch the current shell to a profile (no name: back to the default).
    Switch {
        name: Option<String>,
        /// Print the environment for this shell (used by the shell integration).
        #[arg(long, value_enum)]
        shell: Option<Shell>,
    },
    /// List profiles (* default, > current shell).
    List,
    /// Print the current shell's profile.
    Current,
    /// Sign in to a profile's account (extra args go to `claude auth login`).
    Login {
        name: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Refresh a profile's links to ~/.claude (all profiles when omitted).
    Sync { name: Option<String> },
    /// Print the environment a new shell should start with.
    Env {
        #[arg(long, value_enum)]
        shell: Shell,
    },
    /// Print the shell integration script.
    Init {
        #[arg(value_enum)]
        shell: Shell,
    },
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("ccsp: {err:#}");
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    let paths = Paths::from_env()?;
    match cli.command {
        Cmd::Create {
            name,
            isolated,
            bare,
            no_login,
        } => {
            let mode = if isolated {
                Mode::Isolated
            } else {
                Mode::Shared
            };
            let profile = profile::create(&paths, &name, Meta { mode, bare })?;
            eprintln!(
                "created profile '{}' ({})",
                profile.name,
                profile.meta.describe()
            );
            if no_login {
                eprintln!("sign in later with: ccsp login {}", profile.name);
                return Ok(ExitCode::SUCCESS);
            }
            login(&profile, &[])
        }
        Cmd::Delete { name, yes } => {
            let profile = profile::load(&paths, &name)?;
            if !yes && !confirm(&profile)? {
                eprintln!("aborted");
                return Ok(ExitCode::FAILURE);
            }
            trash::delete(&profile.dir)
                .with_context(|| format!("moving {} to the trash", profile.dir.display()))?;
            let mut state = State::load(&paths)?;
            if state.default.as_deref() == Some(name.as_str()) {
                state.default = None;
                state.save(&paths)?;
                eprintln!("'{name}' was the default; no default is set now");
            }
            eprintln!("deleted profile '{name}' (it is in the trash)");
            Ok(ExitCode::SUCCESS)
        }
        Cmd::SetDefault { name, unset: _ } => {
            if let Some(name) = &name {
                profile::load(&paths, name)?;
            }
            let mut state = State::load(&paths)?;
            state.default = name;
            state.save(&paths)?;
            match &state.default {
                Some(name) => eprintln!("default profile: {name}"),
                None => eprintln!("no default profile: new shells use ~/.claude"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Switch { name, shell } => {
            let env = match &name {
                Some(name) => {
                    let profile = profile::load(&paths, name)?;
                    report_sync(&profile, &paths)?;
                    Target::Profile(profile, true)
                }
                None => default_target(&paths)?,
            };
            let Some(shell) = shell else {
                eprintln!(
                    "ccsp: `switch` changes the current shell, so it needs the shell integration:\n  \
                     fish: add `ccsp init fish | source` to ~/.config/fish/config.fish\n  \
                     bash/zsh: add `eval \"$(ccsp init bash)\"` to your rc file"
                );
                return Ok(ExitCode::FAILURE);
            };
            print!("{}", shell::render(shell, &env.as_env())?);
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Env { shell } => {
            let target = current_or_default(&paths)?;
            if let Target::Profile(profile, _) = &target {
                report_sync(profile, &paths)?;
            }
            print!("{}", shell::render(shell, &target.as_env())?);
            Ok(ExitCode::SUCCESS)
        }
        Cmd::List => {
            let default = State::load(&paths)?.default;
            let current = std::env::var("CCSP_PROFILE").ok();
            let profiles = profile::list(&paths)?;
            if profiles.is_empty() {
                eprintln!("no profiles yet: ccsp create <name>");
            }
            for profile in profiles {
                let is_default = default.as_deref() == Some(profile.name.as_str());
                let is_current = current.as_deref() == Some(profile.name.as_str());
                println!(
                    "{}{} {:<20} {}",
                    if is_default { '*' } else { ' ' },
                    if is_current { '>' } else { ' ' },
                    profile.name,
                    profile.meta.describe()
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Current => match std::env::var("CCSP_PROFILE") {
            Ok(name) if !name.is_empty() => {
                println!("{name}");
                Ok(ExitCode::SUCCESS)
            }
            _ => {
                eprintln!("no profile in this shell: Claude Code uses ~/.claude");
                Ok(ExitCode::FAILURE)
            }
        },
        Cmd::Login { name, args } => login(&profile::load(&paths, &name)?, &args),
        Cmd::Sync { name } => {
            let profiles = match name {
                Some(name) => vec![profile::load(&paths, &name)?],
                None => profile::list(&paths)?,
            };
            for profile in &profiles {
                let report = profile.sync(&paths)?;
                for entry in &report.linked {
                    eprintln!("{}: linked {entry}", profile.name);
                }
                for entry in &report.pruned {
                    eprintln!("{}: removed stale link {entry}", profile.name);
                }
                warn_diverged(profile, &report.diverged);
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Init { shell } => {
            print!("{}", shell::init_script(shell));
            Ok(ExitCode::SUCCESS)
        }
    }
}

enum Target {
    Profile(Profile, bool),
    Clear,
    Keep,
}

impl Target {
    fn as_env(&self) -> Env<'_> {
        match self {
            Target::Profile(profile, switched) => Env::Set {
                name: &profile.name,
                home: profile.home(),
                switched: *switched,
            },
            Target::Clear => Env::Clear,
            Target::Keep => Env::Keep,
        }
    }
}

/// The shell keeps a profile it switched to; otherwise it follows the default.
fn current_or_default(paths: &Paths) -> Result<Target> {
    let switched = std::env::var("CCSP_SWITCHED").is_ok_and(|v| v == "1");
    if switched
        && let Ok(name) = std::env::var("CCSP_PROFILE")
        && let Ok(profile) = profile::load(paths, &name)
    {
        return Ok(Target::Profile(profile, true));
    }
    default_target(paths)
}

fn default_target(paths: &Paths) -> Result<Target> {
    let ours = std::env::var_os("CCSP_PROFILE").is_some();
    match State::load(paths)?.default {
        Some(name) => match profile::load(paths, &name) {
            Ok(profile) => Ok(Target::Profile(profile, false)),
            Err(err) => {
                eprintln!("ccsp: default profile is unusable: {err:#}");
                Ok(if ours { Target::Clear } else { Target::Keep })
            }
        },
        // Only undo what ccsp set: a CLAUDE_CONFIG_DIR from elsewhere stays.
        None => Ok(if ours { Target::Clear } else { Target::Keep }),
    }
}

fn report_sync(profile: &Profile, paths: &Paths) -> Result<()> {
    let report = profile.sync(paths)?;
    warn_diverged(profile, &report.diverged);
    Ok(())
}

fn warn_diverged(profile: &Profile, diverged: &[String]) {
    for entry in diverged {
        eprintln!(
            "ccsp: warning: {}/{entry} is no longer a link to ~/.claude/{entry}; \
             changes to it stay in profile '{}'",
            profile.home().display(),
            profile.name
        );
    }
}

fn login(profile: &Profile, args: &[String]) -> Result<ExitCode> {
    eprintln!(
        "signing in to profile '{}' with `claude auth login`",
        profile.name
    );
    let status = Command::new("claude")
        .args(["auth", "login"])
        .args(args)
        .env("CLAUDE_CONFIG_DIR", profile.home())
        .status();
    match status {
        Ok(status) if status.success() => Ok(ExitCode::SUCCESS),
        Ok(_) => bail!("login failed; retry with: ccsp login {}", profile.name),
        Err(err) => bail!(
            "could not run `claude` ({err}); install Claude Code, then: ccsp login {}",
            profile.name
        ),
    }
}

fn confirm(profile: &Profile) -> Result<bool> {
    let stdin = std::io::stdin();
    if !stdin.is_terminal() {
        bail!(
            "refusing to delete '{}' without a terminal; pass --yes",
            profile.name
        );
    }
    let what = match profile.meta.mode {
        Mode::Shared => "its login",
        Mode::Isolated => "its login, sessions and history",
    };
    eprint!(
        "Delete profile '{}' ({what} go to the trash)? [y/N] ",
        profile.name
    );
    std::io::stderr().flush()?;
    let mut answer = String::new();
    stdin.lock().read_line(&mut answer)?;
    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}
