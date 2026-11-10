use agrupa::commands::{self, ArmarArgs, EvaluarArgs, SugerirArgs};
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "agrupa",
    version,
    about = "Forma grupos balanceados a partir de perfiles de alumnos"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Arma grupos desde cero
    Armar(ArmarArgs),
    /// Sugiere grupo para alumnos nuevos sin mover a nadie
    Sugerir(SugerirArgs),
    /// Evalúa una asignación (por ejemplo, una hecha a mano)
    Evaluar(EvaluarArgs),
}

fn main() {
    let result = match Cli::parse().command {
        Command::Armar(args) => commands::armar(args),
        Command::Sugerir(args) => commands::sugerir(args),
        Command::Evaluar(args) => commands::evaluar(args),
    };
    if let Err(error) = result {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
