//! Pruebas de punta a punta con los datos de `ejemplos/`.

use agrupa::assignment::Assignment;
use agrupa::config::Config;
use agrupa::data::Table;
use agrupa::partition::Partition;
use agrupa::problem::Problem;
use agrupa::solver::{self, Layout, SearchParams, even_sizes};
use agrupa::suggest;
use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
use std::path::Path;

fn load(with_newcomers: bool) -> (Config, Table) {
    let cfg = Config::load(Path::new("ejemplos/config.toml")).unwrap();
    let mut table = Table::read(Path::new("ejemplos/alumnos.csv"), None).unwrap();
    if with_newcomers {
        table.append(&Table::read(Path::new("ejemplos/nuevos.csv"), None).unwrap());
    }
    (cfg, table)
}

/// El problema de ejemplo, calibrado para su distribución de grupos.
fn example() -> (Problem, Layout) {
    let (cfg, table) = load(false);
    let mut problem = Problem::from_table(&table, &cfg).unwrap();
    let layout = Layout::new(problem.student_count(), &cfg.groups).unwrap();
    problem.calibrate(&layout.target_sizes);
    (problem, layout)
}

fn random_groups(sizes: &[usize], rng: &mut StdRng) -> Vec<usize> {
    let mut groups: Vec<usize> = sizes
        .iter()
        .enumerate()
        .flat_map(|(g, &s)| std::iter::repeat_n(g, s))
        .collect();
    groups.shuffle(rng);
    groups
}

/// El valor "al azar" de cada criterio debe coincidir con el promedio de repartos al azar.
#[test]
fn reference_matches_random_average() {
    let (problem, layout) = example();
    let mut rng = StdRng::seed_from_u64(1);
    let trials = 20_000;
    let terms = problem.objective.terms();
    let mut sums = vec![0.0; terms.len()];
    for _ in 0..trials {
        let groups = random_groups(&layout.target_sizes, &mut rng);
        let state = problem.state(Partition::complete(layout.group_count, &groups));
        for (term, sum) in sums.iter_mut().enumerate() {
            *sum += state.tracker(term).cost();
        }
    }
    for (term, sum) in terms.iter().zip(&sums).filter(|(t, _)| !t.is_hard()) {
        let average = sum / trials as f64;
        let reference = term.reference;
        assert!(
            (average - reference.random).abs() <= 0.02 * reference.random.max(0.1),
            "{}: promedio {average} vs. al azar {}",
            term.criterion.meta().label,
            reference.random
        );
        assert!(reference.ideal <= reference.random);
    }
}

#[test]
fn armar_respects_constraints_and_sizes() {
    let (problem, layout) = example();
    let params = SearchParams {
        iterations: 50_000,
        restarts: 2,
        seed: 3,
    };
    let groups = solver::solve(&problem, &layout, &params).unwrap();
    let state = problem.state(Partition::complete(layout.group_count, &groups));

    let mut sizes = state.partition().sizes();
    sizes.sort_unstable();
    assert_eq!(sizes, vec![3, 3, 4, 4, 4, 4, 4, 4]);
    for block in problem.relations.blocks.all() {
        assert!(block.iter().all(|&s| groups[s] == groups[block[0]]));
    }
    for &(a, b) in &problem.relations.apart {
        assert_ne!(groups[a], groups[b]);
    }
    // Mucho mejor que al azar, que en promedio cuesta la suma de los pesos.
    let soft_weight: f64 = problem
        .objective
        .terms()
        .iter()
        .filter(|t| !t.is_hard())
        .map(|t| t.criterion.meta().weight)
        .sum();
    assert!(
        state.total() < 0.5 * soft_weight,
        "costo {} de {soft_weight}",
        state.total()
    );
}

#[test]
fn same_seed_same_result() {
    let (problem, layout) = example();
    let params = SearchParams {
        iterations: 20_000,
        restarts: 4,
        seed: 9,
    };
    assert_eq!(
        solver::solve(&problem, &layout, &params).unwrap(),
        solver::solve(&problem, &layout, &params).unwrap()
    );
}

#[test]
fn newcomers_fill_the_small_groups_without_moving_anyone() {
    let (original, layout) = example();
    let before = solver::solve(
        &original,
        &layout,
        &SearchParams {
            iterations: 50_000,
            restarts: 2,
            seed: 1,
        },
    )
    .unwrap();

    let (cfg, table) = load(true);
    let mut problem = Problem::from_table(&table, &cfg).unwrap();
    let mut groups: Vec<usize> = before.clone();
    groups.resize(problem.student_count(), usize::MAX);
    let assignment = Assignment {
        groups: groups
            .iter()
            .map(|&g| (g != usize::MAX).then_some(g))
            .collect(),
        labels: (1..=layout.group_count).map(|g| g.to_string()).collect(),
        warnings: Vec::new(),
    };
    problem.calibrate(&even_sizes(
        problem.student_count(),
        assignment.group_count(),
    ));
    let mut state = problem.state(assignment.partition());
    let sizes_before = state.partition().sizes();

    let suggestions = suggest::suggest_all(&problem, &mut state, &assignment.labels, None);
    assert_eq!(suggestions.len(), 2);
    let after = state.partition().assignment();
    assert!(after.iter().all(Option::is_some));
    assert!(
        before.iter().enumerate().all(|(s, &g)| after[s] == Some(g)),
        "nadie se movió"
    );
    for suggestion in &suggestions {
        assert_eq!(sizes_before[suggestion.chosen().group], 3);
    }
}
