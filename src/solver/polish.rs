//! Pulido local: prueba todos los intercambios (y movimientos) de alumnos sueltos y
//! se queda con cada mejora, hasta una pasada en que ninguno mejore.

use super::layout::Layout;
use crate::relations::Blocks;
use crate::state::{Move, State};

/// Pasadas completas como máximo.
const MAX_PASSES: usize = 20;
/// Cada pasada cuesta O(n²): sobre esta cantidad de alumnos se omite.
const MAX_STUDENTS: usize = 1500;
/// Mejora mínima para aceptar un cambio.
const MIN_IMPROVEMENT: f64 = 1e-12;

pub fn polish(state: &mut State, blocks: &Blocks, layout: &Layout) {
    let students = state.partition().student_count();
    if layout.group_count < 2 || students > MAX_STUDENTS {
        return;
    }
    let singles: Vec<usize> = (0..students).filter(|&s| blocks.is_single(s)).collect();
    for _ in 0..MAX_PASSES {
        if !improve_once(state, &singles, layout) {
            break;
        }
    }
}

/// Una pasada completa. Devuelve si hubo alguna mejora.
fn improve_once(state: &mut State, singles: &[usize], layout: &Layout) -> bool {
    let mut improved = false;
    for (index, &a) in singles.iter().enumerate() {
        for &b in &singles[index + 1..] {
            improved |= try_swap(state, a, b);
        }
        if layout.sizes_can_vary() {
            for group in 0..layout.group_count {
                improved |= try_relocate(state, a, group, layout);
            }
        }
    }
    improved
}

fn try_swap(state: &mut State, a: usize, b: usize) -> bool {
    let (group_a, group_b) = (group(state, a), group(state, b));
    if group_a == group_b {
        return false;
    }
    state.try_apply(
        &[Move::to_group(a, group_b), Move::to_group(b, group_a)],
        improves,
    )
}

fn try_relocate(state: &mut State, student: usize, to: usize, layout: &Layout) -> bool {
    let from = group(state, student);
    let partition = state.partition();
    if to == from
        || partition.size(from) <= layout.min_size
        || partition.size(to) >= layout.max_size
    {
        return false;
    }
    state.try_apply(&[Move::to_group(student, to)], improves)
}

fn improves(delta: f64) -> bool {
    delta < -MIN_IMPROVEMENT
}

fn group(state: &State, student: usize) -> usize {
    state
        .partition()
        .group_of(student)
        .expect("todos los alumnos tienen grupo")
}
