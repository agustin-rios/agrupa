use agrupa::assignment::Assignment;
use agrupa::config::Config;
use agrupa::data::Table;
use agrupa::model::Model;
use agrupa::report;
use agrupa::solver::{self, Layout, Params};
use agrupa::state::State;
use agrupa::suggest;
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(
    name = "agrupa",
    version,
    about = "Forma grupos balanceados a partir de perfiles de alumnos"
)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Arma grupos desde cero
    Armar {
        /// CSV con los alumnos
        #[arg(short, long)]
        alumnos: PathBuf,
        /// Criterios (TOML)
        #[arg(short, long)]
        config: PathBuf,
        /// CSV de salida
        #[arg(short, long, default_value = "grupos.csv")]
        salida: PathBuf,
        /// Guarda además el detalle en JSON
        #[arg(long)]
        json: Option<PathBuf>,
        /// Pasos de búsqueda por reinicio
        #[arg(long)]
        iteraciones: Option<usize>,
        /// Cantidad de búsquedas independientes (se queda con la mejor)
        #[arg(long)]
        reinicios: Option<usize>,
        /// Semilla: con la misma semilla, el resultado es el mismo
        #[arg(long)]
        semilla: Option<u64>,
    },
    /// Sugiere grupo para alumnos nuevos sin mover a nadie
    Sugerir {
        /// CSV con los alumnos (los que no están en la asignación cuentan como nuevos)
        #[arg(short, long)]
        alumnos: PathBuf,
        /// CSV con alumnos nuevos (opcional)
        #[arg(short, long)]
        nuevos: Option<PathBuf>,
        /// Criterios (TOML)
        #[arg(short, long)]
        config: PathBuf,
        /// Asignación actual (la que generó `armar`)
        #[arg(short, long)]
        grupos: PathBuf,
        /// Guarda la asignación actualizada
        #[arg(short, long)]
        salida: Option<PathBuf>,
    },
    /// Evalúa una asignación (por ejemplo, una hecha a mano)
    Evaluar {
        /// CSV con los alumnos
        #[arg(short, long)]
        alumnos: PathBuf,
        /// CSV con alumnos nuevos (opcional; se suma a la lista de alumnos)
        #[arg(short, long)]
        nuevos: Option<PathBuf>,
        /// Criterios (TOML)
        #[arg(short, long)]
        config: PathBuf,
        /// Asignación a evaluar
        #[arg(short, long)]
        grupos: PathBuf,
        /// Guarda además el detalle en JSON
        #[arg(long)]
        json: Option<PathBuf>,
    },
}

/// Escribe en stdout sin entrar en pánico si se cerró la salida (p. ej. `| head`).
fn emit(text: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    if let Err(e) = out.write_all(text.as_bytes()).and_then(|_| out.flush()) {
        if e.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

macro_rules! say {
    () => {
        emit("\n")
    };
    ($($arg:tt)*) => {
        emit(&format!("{}\n", format_args!($($arg)*)))
    };
}

fn main() {
    if let Err(e) = run(Cli::parse()) {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<()> {
    match cli.cmd {
        Cmd::Armar {
            alumnos,
            config,
            salida,
            json,
            iteraciones,
            reinicios,
            semilla,
        } => {
            let cfg = Config::load(&config)?;
            let table = read_students(&alumnos, None, &cfg)?;
            let mut m = Model::build(&table, &cfg)?;
            warn_all(&m.warnings);
            let layout = Layout::new(m.n(), &cfg.groups)?;
            m.calibrate(&layout.sizes);
            let params = Params {
                iterations: iteraciones.unwrap_or(cfg.solver.iterations),
                restarts: reinicios.unwrap_or(cfg.solver.restarts),
                seed: semilla.unwrap_or(cfg.solver.seed),
            };
            let group_of = solver::solve(&m, &layout, &params)?;
            let asg = Assignment::numbered(group_of, layout.k);
            asg.write(&salida, &m)?;
            let st = State::new(&m, asg.k(), asg.group_of.clone());
            let mut r = report::evaluate(&st, &asg.labels, Some((layout.min, layout.max)));
            r.warnings = m.warnings.clone();
            say!("{}", report::render_text(&r).trim_end());
            say!();
            say!("Asignación guardada en {}", salida.display());
            if let Some(path) = json {
                write_json(&path, &r)?;
            }
        }
        Cmd::Sugerir {
            alumnos,
            nuevos,
            config,
            grupos,
            salida,
        } => {
            let cfg = Config::load(&config)?;
            let table = read_students(&alumnos, nuevos.as_deref(), &cfg)?;
            let mut m = Model::build(&table, &cfg)?;
            warn_all(&m.warnings);
            let mut asg = Assignment::read(&grupos, &m, &cfg.id_column)?;
            warn_all(&asg.warnings);
            m.calibrate(&Layout::even_sizes(m.n(), asg.k()));
            let mut st = State::new(&m, asg.k(), asg.group_of.clone());
            if st.group_of.iter().all(|&g| g != agrupa::model::NONE) {
                say!("No hay alumnos nuevos: todos ya tienen grupo.");
                return Ok(());
            }
            let sugs = suggest::suggest_all(&mut st, &asg.labels, cfg.groups.max_size);
            say!(
                "{}",
                suggest::render_text(&st, &sugs, &asg.labels).trim_end()
            );
            say!();
            asg.group_of = st.group_of.clone();
            match salida {
                Some(path) => {
                    asg.write(&path, &m)?;
                    say!("Asignación actualizada guardada en {}", path.display());
                }
                None => {
                    say!("Para guardar la asignación actualizada, agrega -s <archivo.csv>.")
                }
            }
        }
        Cmd::Evaluar {
            alumnos,
            nuevos,
            config,
            grupos,
            json,
        } => {
            let cfg = Config::load(&config)?;
            let table = read_students(&alumnos, nuevos.as_deref(), &cfg)?;
            let full = Model::build(&table, &cfg)?;
            let asg = Assignment::read(&grupos, &full, &cfg.id_column)?;
            warn_all(&asg.warnings);
            // Se evalúan solo los alumnos con grupo.
            let missing: Vec<String> = (0..full.n())
                .filter(|&i| asg.group_of[i] == agrupa::model::NONE)
                .map(|i| full.ids[i].clone())
                .collect();
            let (m, group_of) = if missing.is_empty() {
                (full, asg.group_of.clone())
            } else {
                eprintln!(
                    "aviso: {} alumnos sin grupo no se consideran: {}",
                    missing.len(),
                    agrupa::model::preview(&missing)
                );
                let mut sub = table.clone();
                sub.rows = table
                    .rows
                    .iter()
                    .zip(&asg.group_of)
                    .filter(|(_, g)| **g != agrupa::model::NONE)
                    .map(|(r, _)| r.clone())
                    .collect();
                let g: Vec<u32> = asg
                    .group_of
                    .iter()
                    .copied()
                    .filter(|&g| g != agrupa::model::NONE)
                    .collect();
                (Model::build(&sub, &cfg)?, g)
            };
            warn_all(&m.warnings);
            let mut m = m;
            let mut sizes = vec![0; asg.k()];
            for &g in &group_of {
                sizes[g as usize] += 1;
            }
            m.calibrate(&sizes);
            let st = State::new(&m, asg.k(), group_of);
            let bounds = Layout::new(m.n(), &cfg.groups).ok().map(|l| (l.min, l.max));
            let mut r = report::evaluate(&st, &asg.labels, bounds);
            r.warnings = m.warnings.clone();
            say!("{}", report::render_text(&r).trim_end());
            if let Some(path) = json {
                write_json(&path, &r)?;
            }
        }
    }
    Ok(())
}

/// Lee el CSV de alumnos (y opcionalmente el de nuevos) y deja una fila por id.
fn read_students(path: &Path, extra: Option<&Path>, cfg: &Config) -> Result<Table> {
    let delim = cfg.delimiter()?;
    let mut t = Table::read(path, delim)?;
    if let Some(extra) = extra {
        let other = Table::read(extra, delim)?;
        other
            .column(&cfg.id_column)
            .with_context(|| format!("en {}", extra.display()))?;
        t.append(&other);
    }
    let id_col = t
        .column(&cfg.id_column)
        .with_context(|| format!("en {}", path.display()))?;
    warn_all(&t.dedupe_by(id_col));
    Ok(t)
}

fn warn_all(warnings: &[String]) {
    for w in warnings {
        eprintln!("aviso: {w}");
    }
}

fn write_json(path: &Path, r: &report::Report) -> Result<()> {
    let text = serde_json::to_string_pretty(r)?;
    std::fs::write(path, text).with_context(|| format!("No pude escribir {}", path.display()))?;
    say!("Detalle guardado en {}", path.display());
    Ok(())
}
