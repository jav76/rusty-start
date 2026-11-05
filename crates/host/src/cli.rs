//! Command-line parsing for the `rusty-start` binary. Pure, so every spelling is unit tested.

const USAGE: &str = "\
usage: rusty-start [command]

  (none)       start the host, or toggle the menu of the host already running
  toggle       toggle the running host's menu
  quit         stop the running host
  setup ...    desktop integration; `rusty-start setup --help` lists its options
  --version    print the version
  --help       print this text";

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    Run,
    /// `rusty-start setup`, with the arguments that follow it.
    Setup(Vec<String>),
    Toggle,
    Quit,
    Version,
    Help,
}

/// Parses the arguments after the program name. The error is the message to print, usage included.
pub fn parse(arguments: &[String]) -> Result<Command, String> {
    let Some((first, rest)) = arguments.split_first() else {
        return Ok(Command::Run);
    };
    let command: Command = match first.as_str() {
        "setup" => return Ok(Command::Setup(rest.to_vec())),
        "toggle" => Command::Toggle,
        "quit" => Command::Quit,
        "--version" | "-V" | "version" => Command::Version,
        "--help" | "-h" | "help" => Command::Help,
        other => return Err(format!("unknown command {other:?}\n{USAGE}")),
    };
    if let Some(extra) = rest.first() {
        return Err(format!("{first} takes no arguments, got {extra:?}\n{USAGE}"));
    }
    Ok(command)
}

pub fn help_text() -> &'static str {
    USAGE
}

pub fn version_text() -> String {
    format!("rusty-start {}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_words(words: &[&str]) -> Result<Command, String> {
        let arguments: Vec<String> = words.iter().map(|word| (*word).to_owned()).collect();
        parse(&arguments)
    }

    #[test]
    fn no_arguments_run_the_host() {
        assert_eq!(parse_words(&[]), Ok(Command::Run));
    }

    #[test]
    fn setup_passes_its_own_arguments_through() {
        assert_eq!(parse_words(&["setup"]), Ok(Command::Setup(Vec::new())));
        assert_eq!(
            parse_words(&["setup", "--dev", "--check"]),
            Ok(Command::Setup(vec!["--dev".to_owned(), "--check".to_owned()]))
        );
    }

    #[test]
    fn every_spelling_of_version_and_help_is_accepted() {
        for word in ["--version", "-V", "version"] {
            assert_eq!(parse_words(&[word]), Ok(Command::Version), "{word}");
        }
        for word in ["--help", "-h", "help"] {
            assert_eq!(parse_words(&[word]), Ok(Command::Help), "{word}");
        }
    }

    #[test]
    fn toggle_and_quit_are_commands_without_arguments() {
        assert_eq!(parse_words(&["toggle"]), Ok(Command::Toggle));
        assert_eq!(parse_words(&["quit"]), Ok(Command::Quit));

        let error: String = parse_words(&["quit", "now"]).unwrap_err();
        assert!(error.starts_with("quit takes no arguments, got \"now\""));
        assert!(error.contains("usage: rusty-start"));
    }

    #[test]
    fn an_unknown_command_is_an_error_with_the_usage() {
        let error: String = parse_words(&["--toggle"]).unwrap_err();
        assert!(error.starts_with("unknown command \"--toggle\""));
        assert!(error.contains("usage: rusty-start"));
    }

    #[test]
    fn the_version_text_names_the_binary_and_the_crate_version() {
        assert_eq!(version_text(), format!("rusty-start {}", env!("CARGO_PKG_VERSION")));
    }
}
