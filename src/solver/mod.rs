//! Búsqueda de la mejor partición: varios reinicios independientes en paralelo.
//!
//! Cada reinicio: reparto inicial al azar → recocido simulado → pulido local.
//! Gana el de menor costo; en empate, el de menor número, así el resultado no
//! depende de cuántos núcleos tenga el equipo.

mod annealing;
mod initial;
mod layout;
mod moves;
mod polish;

pub use layout::{Layout, even_sizes};

use crate::problem::Problem;
use anyhow::{Result, bail};
use rand::{SeedableRng, rngs::StdRng};

/// Esfuerzo de búsqueda.
#[derive(Debug, Clone, Copy)]
pub struct SearchParams {
    /// Pasos de recocido por reinicio.
    pub iterations: usize,
    pub restarts: usize,
    pub seed: u64,
}

/// Separa las semillas de los reinicios (constante de Fibonacci de 64 bits).
const SEED_STEP: u64 = 0x9E37_79B9_7F4A_7C15;
/// Diferencia mínima de costo para preferir otro reinicio.
const MIN_IMPROVEMENT: f64 = 1e-12;

/// Resultado de un reinicio.
struct Outcome {
    restart: usize,
    cost: f64,
    groups: Vec<usize>,
}

/// Busca la mejor asignación. Devuelve el grupo de cada alumno (0..K), numerados
/// según el primer alumno de la lista que aparece en cada grupo.
pub fn solve(problem: &Problem, layout: &Layout, params: &SearchParams) -> Result<Vec<usize>> {
    check_blocks_fit(problem, layout)?;
    let mut best: Option<Outcome> = None;
    for outcome in run_restarts(problem, layout, params) {
        let outcome = outcome?;
        if best
            .as_ref()
            .is_none_or(|b| outcome.cost < b.cost - MIN_IMPROVEMENT)
        {
            best = Some(outcome);
        }
    }
    let best = best.expect("al menos un reinicio");
    Ok(number_by_first_student(&best.groups, layout.group_count))
}

/// Corre los reinicios repartidos entre los núcleos y los devuelve en orden.
fn run_restarts(problem: &Problem, layout: &Layout, params: &SearchParams) -> Vec<Result<Outcome>> {
    let restarts = params.restarts.max(1);
    let workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(restarts);
    let mut outcomes: Vec<Result<Outcome>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                scope.spawn(move || {
                    (worker..restarts)
                        .step_by(workers)
                        .map(|r| run_restart(problem, layout, params, r))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("un hilo del solver falló"))
            .collect()
    });
    outcomes.sort_by_key(|o| o.as_ref().map_or(0, |o| o.restart));
    outcomes
}

fn run_restart(
    problem: &Problem,
    layout: &Layout,
    params: &SearchParams,
    restart: usize,
) -> Result<Outcome> {
    let mut rng = StdRng::seed_from_u64(
        params
            .seed
            .wrapping_add((restart as u64).wrapping_mul(SEED_STEP)),
    );
    let start = initial::random_partition(problem, layout, &mut rng)?;
    let mut state = problem.state(start);
    if layout.group_count > 1 && params.iterations > 0 {
        let proposer = moves::Proposer::new(&problem.relations.blocks, layout);
        annealing::anneal(&mut state, &proposer, params.iterations, &mut rng);
    }
    polish::polish(&mut state, &problem.relations.blocks, layout);

    // Se recalcula desde cero para comparar reinicios sin error de redondeo acumulado.
    let partition = state.into_partition();
    let cost = problem.state(partition.clone()).total();
    let groups = partition
        .assignment()
        .iter()
        .map(|g| g.expect("todos los alumnos tienen grupo"))
        .collect();
    Ok(Outcome {
        restart,
        cost,
        groups,
    })
}

fn check_blocks_fit(problem: &Problem, layout: &Layout) -> Result<()> {
    if let Some(block) = problem
        .relations
        .blocks
        .all()
        .iter()
        .find(|b| b.len() > layout.max_size)
    {
        let ids: Vec<&str> = block.iter().map(|&s| problem.roster.id(s)).collect();
        bail!(
            "[constraints] together: {} alumnos deben ir juntos ({}) pero el máximo por grupo es {}",
            block.len(),
            ids.join(", "),
            layout.max_size
        );
    }
    Ok(())
}

/// Renumera los grupos según el primer alumno que aparece en cada uno, para que la
/// salida sea estable.
fn number_by_first_student(groups: &[usize], group_count: usize) -> Vec<usize> {
    let mut new_number: Vec<Option<usize>> = vec![None; group_count];
    let mut next = 0;
    for group in groups.iter().copied().chain(0..group_count) {
        if new_number[group].is_none() {
            new_number[group] = Some(next);
            next += 1;
        }
    }
    groups
        .iter()
        .map(|&g| new_number[g].expect("todos numerados"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_groups_by_first_student() {
        assert_eq!(number_by_first_student(&[2, 0, 2, 1], 3), vec![0, 1, 0, 2]);
        // Un grupo vacío queda al final.
        assert_eq!(number_by_first_student(&[1, 1], 2), vec![0, 0]);
    }
}
