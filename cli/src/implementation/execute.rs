use std::fmt;
use std::io::Write;

use palsav_decoder::{
    decode_metadata, decode_player, decode_players, decode_world, write_metadata, write_player,
    write_players, write_world,
};

use super::arguments::{parse, DecodeKind};

#[derive(Debug, PartialEq, Eq)]
pub struct CliError {
    pub exit_code: u8,
    pub message: String,
}

impl fmt::Display for CliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

pub fn run(args: Vec<String>, output: &mut impl Write) -> Result<(), CliError> {
    if args.as_slice() == ["--version"] {
        writeln!(output, "palsav {}", env!("CARGO_PKG_VERSION"))
            .map_err(|error| failure(error.to_string()))?;
        return Ok(());
    }

    let arguments = parse(&args).map_err(usage_error)?;
    match arguments.kind {
        DecodeKind::World => {
            let document = decode_world(&arguments.input).map_err(failure)?;
            write_world(&document, arguments.format, output).map_err(failure)
        }
        DecodeKind::Players => {
            let document = decode_players(&arguments.input).map_err(failure)?;
            write_players(&document, arguments.format, output).map_err(failure)
        }
        DecodeKind::Player => {
            let document = decode_player(&arguments.input).map_err(failure)?;
            write_player(&document, arguments.format, output).map_err(failure)
        }
        DecodeKind::Metadata => {
            let document = decode_metadata(&arguments.input).map_err(failure)?;
            write_metadata(&document, arguments.format, output).map_err(failure)
        }
    }
}

fn usage_error(message: String) -> CliError {
    CliError {
        exit_code: 2,
        message,
    }
}

fn failure(message: String) -> CliError {
    CliError {
        exit_code: 1,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_written_through_the_cli_facade() {
        let mut output = Vec::new();

        run(vec!["--version".to_string()], &mut output).unwrap();

        assert_eq!(
            String::from_utf8(output).unwrap(),
            format!("palsav {}\n", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn invalid_arguments_return_the_usage_exit_code() {
        let mut output = Vec::new();

        let error = run(vec!["invalid".to_string()], &mut output).unwrap_err();

        assert_eq!(error.exit_code, 2);
        assert!(output.is_empty());
    }
}
