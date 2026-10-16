//! The binary's command line. The grammar is three flags and nothing else, so
//! it is parsed by hand rather than pulling an argument-parsing crate into a
//! GUI binary. Help and version are answered before logging, hardening or
//! the webview start, so they cost nothing and touch nothing.
//!
//! Release builds on Windows use the GUI subsystem, so `--help` and
//! `--version` print nothing there; the exit code still tells.

use std::ffi::OsString;
use std::fmt;

pub const USAGE: &str = "Usage: mr-usage [--forget-keys | --help | --version]";

/// What the process was asked to do.
#[derive(Debug, PartialEq, Eq)]
pub enum Command {
    /// Open the window. The default.
    Gui,
    /// Delete every key this app stored, print nothing on success, and exit.
    ForgetKeys,
    Help,
    Version,
}

#[derive(Debug, PartialEq, Eq)]
pub enum CliError {
    UnknownOption(String),
    UnexpectedArgument(String),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownOption(o) => write!(f, "unknown option: {o}"),
            Self::UnexpectedArgument(a) => write!(f, "unexpected argument: {a}"),
        }
    }
}

/// Parse the arguments after the program name, left to right. `--help` and
/// `--version` answer as soon as they are seen, so flags after them are not
/// checked; an unknown option before them is still an error. `--` ends the
/// options; since the app takes no positional arguments, anything after it
/// is an error too.
pub fn parse<I>(args: I) -> Result<Command, CliError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut command = Command::Gui;
    let mut options_ended = false;
    for arg in args {
        let text = arg.to_string_lossy();
        if options_ended {
            return Err(CliError::UnexpectedArgument(text.into_owned()));
        }
        match text.as_ref() {
            "--" => options_ended = true,
            "--help" | "-h" => return Ok(Command::Help),
            "--version" | "-V" => return Ok(Command::Version),
            "--forget-keys" => command = Command::ForgetKeys,
            // Older macOS launchers append a process serial number.
            t if t.starts_with("-psn_") => {}
            t if t.starts_with('-') => return Err(CliError::UnknownOption(t.to_owned())),
            t => return Err(CliError::UnexpectedArgument(t.to_owned())),
        }
    }
    Ok(command)
}

pub fn help_text() -> String {
    format!(
        "{name} {version}\n\
         Remaining API credits at a glance.\n\
         \n\
         {USAGE}\n\
         \n\
         With no options, opens the window.\n\
         \n\
         Options:\n\
         \x20     --forget-keys  Delete every API key this app stored in the OS\n\
         \x20                    credential store, then exit without opening a window.\n\
         \x20 -h, --help         Print this help and exit.\n\
         \x20 -V, --version      Print the version and exit.\n\
         \n\
         Exit status:\n\
         \x20 0  Success. For --forget-keys: every key is gone or was never there.\n\
         \x20 1  The app could not start, or the credential store could not be\n\
         \x20    reached or refused a deletion (one line per provider on stderr).\n\
         \x20 2  The command line was not understood.\n\
         \n\
         Environment:\n\
         \x20 MR_USAGE_LOG  Log level on stderr: error, warn (default), info, debug, trace.\n\
         \x20               Every line is scrubbed of key-shaped text before it is written.\n",
        name = env!("CARGO_PKG_NAME"),
        version = env!("CARGO_PKG_VERSION"),
    )
}

pub fn version_text() -> String {
    format!("{} {}\n", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_strs(args: &[&str]) -> Result<Command, CliError> {
        parse(args.iter().map(OsString::from))
    }

    #[test]
    fn no_arguments_opens_the_window() {
        assert_eq!(parse_strs(&[]), Ok(Command::Gui));
    }

    #[test]
    fn each_flag_parses() {
        assert_eq!(parse_strs(&["--forget-keys"]), Ok(Command::ForgetKeys));
        assert_eq!(parse_strs(&["--help"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["-h"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["--version"]), Ok(Command::Version));
        assert_eq!(parse_strs(&["-V"]), Ok(Command::Version));
    }

    #[test]
    fn help_and_version_win_over_other_flags() {
        assert_eq!(parse_strs(&["--forget-keys", "--help"]), Ok(Command::Help));
        assert_eq!(parse_strs(&["--version", "--bogus"]), Ok(Command::Version));
    }

    #[test]
    fn unknown_options_and_positionals_are_errors() {
        assert_eq!(parse_strs(&["--bogus"]), Err(CliError::UnknownOption("--bogus".into())));
        assert_eq!(parse_strs(&["-x"]), Err(CliError::UnknownOption("-x".into())));
        assert_eq!(parse_strs(&["file.txt"]), Err(CliError::UnexpectedArgument("file.txt".into())));
        assert_eq!(parse_strs(&["--", "--help"]), Err(CliError::UnexpectedArgument("--help".into())));
        // An unknown option before --help is still an error: the line is read left to right.
        assert_eq!(parse_strs(&["--bogus", "--help"]), Err(CliError::UnknownOption("--bogus".into())));
    }

    #[test]
    fn macos_process_serial_number_is_tolerated() {
        assert_eq!(parse_strs(&["-psn_0_12345"]), Ok(Command::Gui));
    }

    #[test]
    fn help_names_every_option_and_exit_code() {
        let help = help_text();
        for needle in [
            "--forget-keys",
            "--help",
            "--version",
            "Exit status",
            " 0 ",
            " 1 ",
            " 2 ",
            "MR_USAGE_LOG",
        ] {
            assert!(help.contains(needle), "help text lacks {needle:?}:\n{help}");
        }
        assert_eq!(version_text(), format!("mr-usage {}\n", env!("CARGO_PKG_VERSION")));
    }
}
