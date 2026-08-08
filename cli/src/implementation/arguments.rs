use std::path::PathBuf;

use palsav_decoder::OutputFormat;

#[derive(Debug, PartialEq)]
pub(super) enum DecodeKind {
    World,
    Players,
    Player,
    Metadata,
}

#[derive(Debug, PartialEq)]
pub(super) struct DecodeArguments {
    pub kind: DecodeKind,
    pub input: PathBuf,
    pub format: OutputFormat,
}

pub(super) fn parse(args: &[String]) -> Result<DecodeArguments, String> {
    if args.len() < 4 || args[0] != "decode" || args[2] != "--input" {
        return Err(usage());
    }
    let kind = match args[1].as_str() {
        "world" => DecodeKind::World,
        "players" => DecodeKind::Players,
        "player" => DecodeKind::Player,
        "meta" => DecodeKind::Metadata,
        _ => return Err(usage()),
    };
    let format = match args.get(4).map(String::as_str) {
        None => OutputFormat::Json,
        Some("--format") => match args.get(5).map(String::as_str) {
            Some("json") => OutputFormat::Json,
            Some("ndjson") => OutputFormat::Ndjson,
            _ => return Err(usage()),
        },
        _ => return Err(usage()),
    };
    if args.len() > 6 || args.len() == 5 {
        return Err(usage());
    }
    Ok(DecodeArguments {
        kind,
        input: PathBuf::from(&args[3]),
        format,
    })
}

pub(super) fn usage() -> String {
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
            parse(&args).unwrap(),
            DecodeArguments {
                kind: DecodeKind::World,
                input: PathBuf::from("save"),
                format: OutputFormat::Json,
            }
        );
    }

    #[test]
    fn rejects_an_unknown_output_format_as_a_usage_error() {
        let args = vec!["decode", "world", "--input", "save", "--format", "yaml"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();

        assert_eq!(parse(&args), Err(usage()));
    }

    #[test]
    fn rejects_an_unknown_decode_kind_as_a_usage_error() {
        let args = vec!["decode", "unknown", "--input", "save"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>();

        assert_eq!(parse(&args), Err(usage()));
    }
}
