//! Pruebas de punta a punta con los datos de `ejemplos/`.

use agrupa::assignment::Assignment;
use agrupa::config::Config;
use agrupa::data::Table;
use agrupa::model::{EdgeKind, Model, NONE};
use agrupa::solver::{self, Layout, Params};
use agrupa::state::State;
use agrupa::suggest;
use rand::{SeedableRng, rngs::StdRng, seq::SliceRandom};
use std::path::Path;

fn load(extra: bool) -> (Config, Table) {
    let cfg = Config::load(Path::new("ejemplos/config.toml")).unwrap();
    let mut t = Table::read(Path::new("ejemplos/alumnos.csv"), None).unwrap();
    if extra {
        t.append(&Table::read(Path::new("ejemplos/nuevos.csv"), None).unwrap());
    }
    (cfg, t)
}

fn random_assignment(sizes: &[usize], rng: &mut StdRng) -> Vec<u32> {
    let mut v: Vec<u32> = sizes
        .iter()
        .enumerate()
        .flat_map(|(g, &s)| std::iter::repeat_n(g as u32, s))
        .collect();
    v.shuffle(rng);
    v
}

/// El valor "al azar" de cada criterio debe coincidir con el promedio de repartos al azar.
#[test]
fn calibration_matches_random_average() {
    let (cfg, t) = load(false);
    let mut m = Model::build(&t, &cfg).unwrap();
    let layout = Layout::new(m.n(), &cfg.groups).unwrap();
    m.calibrate(&layout.sizes);
    let mut rng = StdRng::seed_from_u64(1);
    let trials = 20_000;
    let mut sum = vec![0.0; m.n_crit()];
    for _ in 0..trials {
        let st = State::new(&m, layout.k, random_assignment(&layout.sizes, &mut rng));
        for (s, r) in sum.iter_mut().zip(&st.raw) {
            *s += r;
        }
    }
    for (c, s) in m.calib.crit.iter().zip(&sum) {
        let avg = s / trials as f64;
        assert!(
            (avg - c.random).abs() <= 0.02 * c.random.max(0.1),
            "promedio {avg} vs. al azar {}",
            c.random
        );
        assert!(c.ideal <= c.random);
    }
}

#[test]
fn armar_respects_constraints_and_sizes() {
    let (cfg, t) = load(false);
    let mut m = Model::build(&t, &cfg).unwrap();
    let layout = Layout::new(m.n(), &cfg.groups).unwrap();
    m.calibrate(&layout.sizes);
    let params = Params {
        iterations: 50_000,
        restarts: 2,
        seed: 3,
    };
    let g = solver::solve(&m, &layout, &params).unwrap();
    let st = State::new(&m, layout.k, g.clone());
    assert_eq!(st.hard, 0.0);
    let mut sizes = st.sizes.clone();
    sizes.sort_unstable();
    assert_eq!(sizes, vec![3, 3, 4, 4, 4, 4, 4, 4]);
    for b in &m.blocks {
        assert!(b.iter().all(|&i| g[i] == g[b[0]]));
    }
    for e in m.edges.iter().filter(|e| e.kind == EdgeKind::Apart) {
        assert_ne!(g[e.a], g[e.b]);
    }
    // Mucho mejor que al azar en el total.
    let total_weight: f64 = cfg.attributes.iter().map(|a| a.weight).sum::<f64>() + 1.0;
    let scaled: f64 = (0..m.n_crit()).map(|c| st.weighted(c)).sum();
    assert!(
        scaled < 0.5 * total_weight,
        "costo {scaled} de {total_weight}"
    );
}

#[test]
fn same_seed_same_result() {
    let (cfg, t) = load(false);
    let mut m = Model::build(&t, &cfg).unwrap();
    let layout = Layout::new(m.n(), &cfg.groups).unwrap();
    m.calibrate(&layout.sizes);
    let params = Params {
        iterations: 20_000,
        restarts: 4,
        seed: 9,
    };
    assert_eq!(
        solver::solve(&m, &layout, &params).unwrap(),
        solver::solve(&m, &layout, &params).unwrap()
    );
}

#[test]
fn suggests_group_missing_the_tag() {
    let (cfg, t) = load(true);
    let mut m = Model::build(&t, &cfg).unwrap();
    // Asignación previa: los 30 originales en 8 grupos; los nuevos sin grupo.
    let (cfg0, t0) = load(false);
    let mut m0 = Model::build(&t0, &cfg0).unwrap();
    let layout = Layout::new(m0.n(), &cfg0.groups).unwrap();
    m0.calibrate(&layout.sizes);
    let g0 = solver::solve(
        &m0,
        &layout,
        &Params {
            iterations: 50_000,
            restarts: 2,
            seed: 1,
        },
    )
    .unwrap();
    let mut group_of = vec![NONE; m.n()];
    group_of[..g0.len()].copy_from_slice(&g0);
    let asg = Assignment::numbered(group_of, layout.k);

    m.calibrate(&Layout::even_sizes(m.n(), asg.k()));
    let mut st = State::new(&m, asg.k(), asg.group_of.clone());
    let sizes_before = st.sizes.clone();
    let sugs = suggest::suggest_all(&mut st, &asg.labels, None);
    assert_eq!(sugs.len(), 2);
    assert!(st.group_of.iter().all(|&g| g != NONE));
    // Nadie se movió.
    assert_eq!(&st.group_of[..g0.len()], &g0[..]);
    // Se llenan primero los grupos de 3.
    for s in &sugs {
        assert_eq!(sizes_before[s.chosen().group], 3);
    }
}
