//! Generate shell completions and the manual page from the CLI definition.

use std::io::Write;

use anyhow::Result;
use clap::CommandFactory;
use clap_complete::generate;

use crate::cli::{Cli, CompletionsArgs};

/// Write a completion script for `args.shell` to stdout.
pub fn completions(args: CompletionsArgs) -> Result<()> {
    let mut command = Cli::command();
    let name = command.get_name().to_string();
    let mut stdout = std::io::stdout();
    generate(args.shell, &mut command, name, &mut stdout);
    stdout.flush()?;
    Ok(())
}

/// Write the manual page to stdout.
pub fn man() -> Result<()> {
    let command = Cli::command();
    let man = clap_mangen::Man::new(command);
    let mut stdout = std::io::stdout();
    man.render(&mut stdout)?;
    stdout.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap_complete::Shell;

    #[test]
    fn cli_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn every_shell_renders() {
        for shell in [
            Shell::Bash,
            Shell::Zsh,
            Shell::Fish,
            Shell::PowerShell,
            Shell::Elvish,
        ] {
            let mut command = Cli::command();
            let mut buffer = Vec::new();
            generate(shell, &mut command, "d2mz", &mut buffer);
            assert!(!buffer.is_empty(), "{shell:?} produced nothing");
        }
    }

    #[test]
    fn man_page_renders() {
        let command = Cli::command();
        let man = clap_mangen::Man::new(command);
        let mut buffer = Vec::new();
        man.render(&mut buffer).unwrap();
        let text = String::from_utf8(buffer).unwrap();
        assert!(text.contains("d2mz"), "{text}");
    }
}
