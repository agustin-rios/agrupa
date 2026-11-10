//! Categórico · `together`: mismo valor dentro de cada grupo (horario, sección).
//!
//! Costo = pares del mismo grupo con valores conocidos distintos. Se prefiere a
//! "alumnos fuera de la mayoría" porque cambia con cada movimiento: el recocido
//! avanza mejor sobre un costo suave.

use super::groupwise::{Group, GroupwiseCriterion};
use super::labels::LabelColumn;
use super::{Meta, Reference};
use crate::text::{format_counts, plural};

pub struct Together {
    meta: Meta,
    column: LabelColumn,
}

impl Together {
    pub fn new(meta: Meta, column: LabelColumn) -> Self {
        Together { meta, column }
    }
}

/// Pares con valores distintos: (conocidos² − Σ conteo²) / 2.
fn mixed_pairs(counts: &[u32]) -> f64 {
    let known: u64 = counts.iter().map(|&c| c as u64).sum();
    let same: u64 = counts.iter().map(|&c| (c as u64) * (c as u64)).sum();
    (known * known - same) as f64 / 2.0
}

impl GroupwiseCriterion for Together {
    type Stats = Vec<u32>;

    fn meta(&self) -> &Meta {
        &self.meta
    }

    /// Ideal: una forma alcanzable de llenar los grupos. Al azar: cada par de un grupo
    /// tiene valores conocidos distintos con probabilidad (M² − Σ T_v²) / (n(n − 1)).
    fn reference(&self, sizes: &[usize]) -> Reference {
        let n = self.column.student_count();
        let known: usize = self.column.totals.iter().sum();
        let same: usize = self.column.totals.iter().map(|t| t * t).sum();
        let p_different = if n > 1 {
            (known * known - same) as f64 / (n * (n - 1)) as f64
        } else {
            0.0
        };
        let random = sizes
            .iter()
            .map(|&s| (s * s.saturating_sub(1) / 2) as f64 * p_different)
            .sum();
        let ideal = packed_mixed_pairs(&self.column.totals, self.column.unknown_count(), sizes);
        Reference { ideal, random }
    }

    fn empty_stats(&self) -> Vec<u32> {
        self.column.empty_counts()
    }

    fn add(&self, counts: &mut Vec<u32>, student: usize) {
        self.column.add(counts, student);
    }

    fn remove(&self, counts: &mut Vec<u32>, student: usize) {
        self.column.remove(counts, student);
    }

    fn group_cost(&self, counts: &Vec<u32>, _size: usize) -> f64 {
        mixed_pairs(counts)
    }

    fn describe(
        &self,
        groups: &[Group<'_, Vec<u32>>],
        _cost: f64,
        _reference: &Reference,
    ) -> String {
        let used: Vec<_> = groups.iter().filter(|g| !g.is_empty()).collect();
        let mut single_valued = 0;
        let mut off_majority = 0;
        for group in &used {
            let known: u32 = group.stats.iter().sum();
            let majority = group.stats.iter().copied().max().unwrap_or(0);
            if known == majority {
                single_valued += 1;
            }
            off_majority += (known - majority) as usize;
        }
        format!(
            "{single_valued} de {} grupos con un solo valor · {off_majority} alumno{} distinto{} a la mayoría de su grupo",
            used.len(),
            plural(off_majority),
            plural(off_majority)
        )
    }

    fn describe_group(&self, group: &Group<'_, Vec<u32>>) -> String {
        let counts = format_counts(&self.column.labels, group.stats);
        if counts.is_empty() {
            format!("{} —", self.meta.label)
        } else {
            format!("{} {counts}", self.meta.label)
        }
    }

    fn explain_join(&self, group: &Group<'_, Vec<u32>>, students: &[usize]) -> String {
        let own = self.column.values_of(students);
        if own.is_empty() {
            return "sin dato".into();
        }
        let own = self.column.names(&own).join(", ");
        let in_group = format_counts(&self.column.labels, group.stats);
        if in_group.is_empty() {
            format!("{own}; el grupo no tiene datos")
        } else {
            format!("{own}; el grupo: {in_group}")
        }
    }
}

/// Pares mezclados de una forma alcanzable de llenar los grupos, del más grande al
/// más chico: cada grupo toma primero el valor que mejor lo llena y, si sobra espacio,
/// prefiere alumnos sin dato antes que mezclar valores.
fn packed_mixed_pairs(totals: &[usize], unknown: usize, sizes: &[usize]) -> f64 {
    let mut remaining = totals.to_vec();
    let mut without_value = unknown;
    let mut capacities = sizes.to_vec();
    capacities.sort_unstable_by(|a, b| b.cmp(a));
    capacities
        .into_iter()
        .map(|capacity| mixed_pairs(&fill_group(capacity, &mut remaining, &mut without_value)))
        .sum()
}

fn fill_group(capacity: usize, remaining: &mut [usize], without_value: &mut usize) -> Vec<u32> {
    let mut counts = vec![0u32; remaining.len()];
    let mut room = capacity;
    while room > 0 {
        let group_is_empty = counts.iter().all(|&c| c == 0);
        match best_value_for(room, remaining) {
            Some(value) if group_is_empty || remaining[value] >= room || *without_value == 0 => {
                let taken = remaining[value].min(room);
                remaining[value] -= taken;
                counts[value] += taken as u32;
                room -= taken;
            }
            _ if *without_value > 0 => {
                let taken = (*without_value).min(room);
                *without_value -= taken;
                room -= taken;
            }
            _ => break,
        }
    }
    counts
}

/// El valor que mejor llena `room` lugares: primero uno que calce exacto, luego el más abundante.
fn best_value_for(room: usize, remaining: &[usize]) -> Option<usize> {
    remaining
        .iter()
        .enumerate()
        .filter(|&(_, &left)| left > 0)
        .max_by_key(|&(_, &left)| (left == room, left.min(room), left))
        .map(|(value, _)| value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packing_fills_whole_groups_first() {
        // A=6, B=5, C=5 en 4 grupos de 4: AAAA BBBB CCCC + {A,A,B,C} = 5 pares mezclados.
        assert_eq!(packed_mixed_pairs(&[6, 5, 5], 0, &[4, 4, 4, 4]), 5.0);
        // Con 2 alumnos sin dato no hace falta mezclar.
        assert_eq!(packed_mixed_pairs(&[6, 4, 4], 2, &[4, 4, 4, 4]), 0.0);
    }
}
