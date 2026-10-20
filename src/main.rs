use clap::Parser;
use mdpkg::cli::Cli;
use mdpkg::commands::execute_command;
use mdpkg::sources::RealSystemBackend;
use std::process;

fn main() {
    let cli = Cli::parse();
    let mut backend = RealSystemBackend::new();
    if let Err(e) = execute_command(cli, &mut backend) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
