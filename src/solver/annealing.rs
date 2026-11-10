//! Recocido simulado: acepta siempre los cambios que mejoran y, a veces, los que
//! empeoran, con una probabilidad que baja a medida que la temperatura se enfría.

use super::moves::Proposer;
use crate::state::{Move, State};
use rand::{Rng, rngs::StdRng};

/// Propuestas al azar que se miden para elegir la temperatura inicial.
const TEMPERATURE_SAMPLES: usize = 200;
/// La temperatura final es esta fracción de la inicial.
const FINAL_TEMPERATURE_RATIO: f64 = 1e-3;
/// Temperatura inicial si ninguna propuesta empeoró el costo.
const FALLBACK_TEMPERATURE: f64 = 1e-3;
/// Mejora mínima para registrar una nueva mejor asignación.
const MIN_IMPROVEMENT: f64 = 1e-12;

/// Corre `iterations` pasos y deja `state` en la mejor asignación encontrada.
pub fn anneal(state: &mut State, proposer: &Proposer, iterations: usize, rng: &mut StdRng) {
    let mut moves = Vec::new();
    let start = initial_temperature(state, proposer, rng, &mut moves);
    let end = start * FINAL_TEMPERATURE_RATIO;
    let cooling = (end / start).powf(1.0 / iterations as f64);

    let mut temperature = start;
    let mut current = state.total();
    let mut best = current;
    let mut best_assignment = state.partition().assignment().to_vec();
    for _ in 0..iterations {
        if proposer.propose(state.partition(), rng, &mut moves) {
            let mut delta = 0.0;
            let accepted = state.try_apply(&moves, |d| {
                delta = d;
                metropolis_accepts(d, temperature, rng)
            });
            if accepted {
                current += delta;
                if current < best - MIN_IMPROVEMENT {
                    best = current;
                    best_assignment.clone_from_slice(state.partition().assignment());
                }
            }
        }
        temperature *= cooling;
    }
    state.reset_to(&best_assignment);
}

/// Regla de Metropolis: mejoras siempre; empeoramientos con probabilidad e^(−Δ/T).
fn metropolis_accepts(delta: f64, temperature: f64, rng: &mut StdRng) -> bool {
    delta <= 0.0 || rng.r#gen::<f64>() < (-delta / temperature).exp()
}

/// El empeoramiento promedio de un cambio al azar, sin contar los que violan una
/// restricción dura. Así la temperatura se ajusta a la escala de cada problema.
fn initial_temperature(
    state: &mut State,
    proposer: &Proposer,
    rng: &mut StdRng,
    moves: &mut Vec<Move>,
) -> f64 {
    let hard_limit = state.objective().hard_penalty() / 2.0;
    let mut worsenings = Vec::new();
    for _ in 0..TEMPERATURE_SAMPLES {
        if proposer.propose(state.partition(), rng, moves) {
            let delta = state.cost_change(moves);
            if delta > MIN_IMPROVEMENT && delta < hard_limit {
                worsenings.push(delta);
            }
        }
    }
    if worsenings.is_empty() {
        FALLBACK_TEMPERATURE
    } else {
        worsenings.iter().sum::<f64>() / worsenings.len() as f64
    }
}
