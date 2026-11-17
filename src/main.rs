mod camera;
mod images;
mod logging;
mod palette;
#[cfg(test)]
mod tests;
mod tiles;
mod window;
mod world;

use std::process::ExitCode;

use logging::{error, info};

fn main() -> ExitCode {
    let level = match logging::level_from_args(std::env::args().skip(1)) {
        Ok(level) => level,
        Err(err) => {
            eprintln!("error: {err}");
            eprintln!("{}", logging::USAGE);
            return ExitCode::from(2);
        }
    };
    logging::init(level);

    info!("ug0 {} starting", env!("CARGO_PKG_VERSION"));
    match window::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            error!("{err}");
            ExitCode::FAILURE
        }
    }
}
