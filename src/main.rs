use std::path::PathBuf;
use std::process::ExitCode;

use palsav_decoder::contract::{
    write_metadata, write_player, write_players, write_world, OutputFormat,
};
use palsav_decoder::decoder::{decode_metadata, decode_player, decode_players, decode_world};

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err((code, message)) => {
            eprintln!("{message}");
            ExitCode::from(code)
        }
    }
}

fn run(args: Vec<String>) -> Result<(), (u8, String)> {
    if args.as_slice() == ["--version"] {
        println!("palsav {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let (kind, input, format) = parse_args(&args).map_err(|message| (2, message))?;
    let mut output = std::io::stdout().lock();
    match kind.as_str() {
        "world" => {
            let document = decode_world(&input).map_err(|message| (1, message))?;
            write_world(&document, format, &mut output).map_err(|message| (1, message))
        }
        "meta" => {
            let document = decode_metadata(&input).map_err(|message| (1, message))?;
            write_metadata(&document, format, &mut output).map_err(|message| (1, message))
        }
        "player" => {
            let document = decode_player(&input).map_err(|message| (1, message))?;
            write_player(&document, format, &mut output).map_err(|message| (1, message))
        }
        "players" => {
            let document = decode_players(&input).map_err(|message| (1, message))?;
            write_players(&document, format, &mut output).map_err(|message| (1, message))
        }
        _ => Err((2, usage())),
    }
}

fn parse_args(args: &[String]) -> Result<(String, PathBuf, OutputFormat), String> {
    if args.len() < 4 || args[0] != "decode" || args[2] != "--input" {
        return Err(usage());
    }
    let format = match args.get(4).map(String::as_str) {
        None => OutputFormat::Json,
        Some("--format") => match args.get(5).map(String::as_str) {
            Some("json") => OutputFormat::Json,
            Some("ndjson") => OutputFormat::Ndjson,
            _ => return Err(usage()),
        },
        _ => return Err(usage()),
    };
    if args.len() > 6 || (args.len() == 5) {
        return Err(usage());
    }
    Ok((args[1].clone(), PathBuf::from(&args[3]), format))
}

fn usage() -> String {
    "Usage: palsav decode <world|players|player|meta> --input <path> [--format <json|ndjson>]"
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_as_the_default_output_format() {
        let args = vec!["decode", "world", "--input", "save"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert_eq!(
            parse_args(&args).unwrap(),
            (
                "world".to_string(),
                PathBuf::from("save"),
                OutputFormat::Json
            )
        );
    }

    #[test]
    fn rejects_an_unknown_output_format_as_a_usage_error() {
        let args = vec!["decode", "world", "--input", "save", "--format", "yaml"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();
        assert!(parse_args(&args).is_err());
    }
}
