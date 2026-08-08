use std::process::ExitCode;

fn main() -> ExitCode {
    let mut output = std::io::stdout().lock();
    match palsav_decoder_cli::run(std::env::args().skip(1).collect(), &mut output) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(error.exit_code)
        }
    }
}
