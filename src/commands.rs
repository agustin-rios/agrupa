//! Los tres comandos de la CLI: leer archivos, llamar al modelo y mostrar resultados.

use crate::assignment::Assignment;
use crate::config::Config;
use crate::data::Table;
use crate::problem::Problem;
use crate::report::{self, Report};
use crate::solver::{self, Layout, SearchParams, even_sizes};
use crate::suggest;
use crate::text::preview;
use anyhow::{Context, Result};
use clap::Args;
use std::path::{Path, PathBuf};

#[derive(Args)]
pub struct ArmarArgs {
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
}

#[derive(Args)]
pub struct SugerirArgs {
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
}

#[derive(Args)]
pub struct EvaluarArgs {
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
}

/// Arma grupos desde cero.
pub fn armar(args: ArmarArgs) -> Result<()> {
    let cfg = Config::load(&args.config)?;
    let table = read_students(&args.alumnos, None, &cfg)?;
    let mut problem = Problem::from_table(&table, &cfg)?;
    warn_all(&problem.warnings);

    let layout = Layout::new(problem.student_count(), &cfg.groups)?;
    problem.calibrate(&layout.target_sizes);
    let params = SearchParams {
        iterations: args.iteraciones.unwrap_or(cfg.solver.iterations),
        restarts: args.reinicios.unwrap_or(cfg.solver.restarts),
        seed: args.semilla.unwrap_or(cfg.solver.seed),
    };
    let groups = solver::solve(&problem, &layout, &params)?;
    let assignment = Assignment::numbered(&groups, layout.group_count);
    assignment.write(&args.salida, &problem.roster)?;

    let state = problem.state(assignment.partition());
    let report = report::build(
        &problem,
        &state,
        &assignment.labels,
        Some((layout.min_size, layout.max_size)),
    );
    say(&report::render_text(&report));
    say("");
    say(&format!("Asignación guardada en {}", args.salida.display()));
    if let Some(path) = args.json {
        write_json(&path, &report)?;
    }
    Ok(())
}

/// Sugiere grupo para los alumnos que no tienen, sin mover a nadie.
pub fn sugerir(args: SugerirArgs) -> Result<()> {
    let cfg = Config::load(&args.config)?;
    let table = read_students(&args.alumnos, args.nuevos.as_deref(), &cfg)?;
    let mut problem = Problem::from_table(&table, &cfg)?;
    warn_all(&problem.warnings);
    let mut assignment = Assignment::read(&args.grupos, &problem.roster, &cfg.id_column)?;
    warn_all(&assignment.warnings);

    if assignment.groups.iter().all(Option::is_some) {
        say("No hay alumnos nuevos: todos ya tienen grupo.");
        return Ok(());
    }
    problem.calibrate(&even_sizes(
        problem.student_count(),
        assignment.group_count(),
    ));
    let mut state = problem.state(assignment.partition());
    let suggestions = suggest::suggest_all(
        &problem,
        &mut state,
        &assignment.labels,
        cfg.groups.max_size,
    );
    say(&suggest::render_text(
        &problem.roster,
        &suggestions,
        &assignment.labels,
    ));
    say("");

    assignment.groups = state.partition().assignment().to_vec();
    match args.salida {
        Some(path) => {
            assignment.write(&path, &problem.roster)?;
            say(&format!(
                "Asignación actualizada guardada en {}",
                path.display()
            ));
        }
        None => say("Para guardar la asignación actualizada, agrega -s <archivo.csv>."),
    }
    Ok(())
}

/// Evalúa una asignación existente. Los alumnos sin grupo no se consideran.
pub fn evaluar(args: EvaluarArgs) -> Result<()> {
    let cfg = Config::load(&args.config)?;
    let mut table = read_students(&args.alumnos, args.nuevos.as_deref(), &cfg)?;
    let everyone = Problem::from_table(&table, &cfg)?;
    let mut assignment = Assignment::read(&args.grupos, &everyone.roster, &cfg.id_column)?;
    warn_all(&assignment.warnings);

    let without_group: Vec<String> = (0..everyone.student_count())
        .filter(|&s| assignment.groups[s].is_none())
        .map(|s| everyone.roster.id(s).to_string())
        .collect();
    let mut problem = if without_group.is_empty() {
        everyone
    } else {
        eprintln!(
            "aviso: {} alumnos sin grupo no se consideran: {}",
            without_group.len(),
            preview(&without_group)
        );
        let keep: Vec<bool> = assignment.groups.iter().map(Option::is_some).collect();
        table.rows = table
            .rows
            .into_iter()
            .zip(keep)
            .filter_map(|(row, keep)| keep.then_some(row))
            .collect();
        assignment.groups.retain(Option::is_some);
        Problem::from_table(&table, &cfg)?
    };
    warn_all(&problem.warnings);

    let partition = assignment.partition();
    problem.calibrate(&partition.sizes());
    let state = problem.state(partition);
    let bounds = Layout::new(problem.student_count(), &cfg.groups)
        .ok()
        .map(|l| (l.min_size, l.max_size));
    let report = report::build(&problem, &state, &assignment.labels, bounds);
    say(&report::render_text(&report));
    if let Some(path) = args.json {
        write_json(&path, &report)?;
    }
    Ok(())
}

/// Lee el CSV de alumnos (y el de nuevos, si hay) y deja una fila por id.
fn read_students(path: &Path, extra: Option<&Path>, cfg: &Config) -> Result<Table> {
    let delimiter = cfg.delimiter()?;
    let mut table = Table::read(path, delimiter)?;
    if let Some(extra) = extra {
        let newcomers = Table::read(extra, delimiter)?;
        newcomers
            .column(&cfg.id_column)
            .with_context(|| format!("en {}", extra.display()))?;
        table.append(&newcomers);
    }
    let id_column = table
        .column(&cfg.id_column)
        .with_context(|| format!("en {}", path.display()))?;
    warn_all(&table.dedupe_by(id_column));
    Ok(table)
}

fn warn_all(warnings: &[String]) {
    for warning in warnings {
        eprintln!("aviso: {warning}");
    }
}

fn write_json(path: &Path, report: &Report) -> Result<()> {
    let text = serde_json::to_string_pretty(report)?;
    std::fs::write(path, text).with_context(|| format!("No pude escribir {}", path.display()))?;
    say(&format!("Detalle guardado en {}", path.display()));
    Ok(())
}

/// Escribe una línea en la salida estándar. Si quien lee cerró la salida
/// (por ejemplo, `| head`), termina sin error en vez de entrar en pánico.
fn say(text: &str) {
    use std::io::Write;
    let mut out = std::io::stdout().lock();
    if let Err(error) = writeln!(out, "{text}").and_then(|_| out.flush()) {
        if error.kind() == std::io::ErrorKind::BrokenPipe {
            std::process::exit(0);
        }
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}
