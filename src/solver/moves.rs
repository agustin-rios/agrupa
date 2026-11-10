//! Cambios que propone el recocido. Todos conservan los bloques `together` y los
//! límites de tamaño:
//! - intercambio de dos unidades del mismo tamaño (alumno suelto o bloque);
//! - un bloque por la misma cantidad de alumnos sueltos del otro grupo;
//! - si los tamaños pueden variar, mover una unidad sin contraparte.

use super::layout::Layout;
use crate::partition::Partition;
use crate::relations::Blocks;
use crate::state::Move;
use rand::{Rng, rngs::StdRng, seq::SliceRandom};

/// Si los tamaños pueden variar, probabilidad de intentar mover en vez de intercambiar.
const RELOCATE_PROBABILITY: f64 = 0.3;
/// Intentos por alumno al buscar alumnos sueltos en un grupo.
const TRIES_PER_SINGLE: usize = 4;

pub struct Proposer<'a> {
    blocks: &'a Blocks,
    layout: &'a Layout,
}

impl<'a> Proposer<'a> {
    pub fn new(blocks: &'a Blocks, layout: &'a Layout) -> Self {
        Proposer { blocks, layout }
    }

    /// Llena `moves` con un cambio al azar. Devuelve `false` si esta vez no encontró uno válido.
    pub fn propose(&self, partition: &Partition, rng: &mut StdRng, moves: &mut Vec<Move>) -> bool {
        moves.clear();
        let student = rng.gen_range(0..partition.student_count());
        let from = partition
            .group_of(student)
            .expect("todos los alumnos tienen grupo");
        let to = other_group(from, self.layout.group_count, rng);
        let unit = self.blocks.of(student);

        if self.layout.sizes_can_vary()
            && rng.gen_bool(RELOCATE_PROBABILITY)
            && self.can_relocate(partition, unit.len(), from, to)
        {
            push_all(moves, unit, to);
            return true;
        }

        let Some(&partner) = partition.members(to).choose(rng) else {
            return false;
        };
        let partner_unit = self.blocks.of(partner);
        match (unit.len(), partner_unit.len()) {
            (a, b) if a == b => {
                push_all(moves, unit, to);
                push_all(moves, partner_unit, from);
                true
            }
            (_, 1) => self.swap_block_for_singles(partition, unit, from, to, rng, moves),
            (1, _) => self.swap_block_for_singles(partition, partner_unit, to, from, rng, moves),
            _ => false,
        }
    }

    fn can_relocate(
        &self,
        partition: &Partition,
        unit_size: usize,
        from: usize,
        to: usize,
    ) -> bool {
        partition.size(from) - unit_size >= self.layout.min_size
            && partition.size(to) + unit_size <= self.layout.max_size
    }

    /// Cambia un bloque de `from` por la misma cantidad de alumnos sueltos de `to`.
    fn swap_block_for_singles(
        &self,
        partition: &Partition,
        block: &[usize],
        from: usize,
        to: usize,
        rng: &mut StdRng,
        moves: &mut Vec<Move>,
    ) -> bool {
        let mut singles: Vec<usize> = Vec::with_capacity(block.len());
        for _ in 0..TRIES_PER_SINGLE * block.len() {
            if singles.len() == block.len() {
                break;
            }
            let Some(&candidate) = partition.members(to).choose(rng) else {
                return false;
            };
            if self.blocks.is_single(candidate) && !singles.contains(&candidate) {
                singles.push(candidate);
            }
        }
        if singles.len() < block.len() {
            return false;
        }
        push_all(moves, block, to);
        push_all(moves, &singles, from);
        true
    }
}

/// Un grupo al azar distinto de `group`.
fn other_group(group: usize, group_count: usize, rng: &mut StdRng) -> usize {
    let other = rng.gen_range(0..group_count - 1);
    if other >= group { other + 1 } else { other }
}

fn push_all(moves: &mut Vec<Move>, students: &[usize], group: usize) {
    moves.extend(students.iter().map(|&s| Move::to_group(s, group)));
}
