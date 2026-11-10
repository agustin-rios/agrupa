//! Numérico · `similar`: valores parecidos dentro de cada grupo.
//!
//! Costo = Σ_g Σ_{i∈g} (x_i − media_g)², la variación dentro de los grupos (k-means).

use super::groupwise::{Group, GroupwiseCriterion};
use super::numeric::{GroupSums, NumericColumn};
use super::{Meta, Reference};
use crate::text::format_number;

pub struct Similar {
    meta: Meta,
    column: NumericColumn,
}

impl Similar {
    pub fn new(meta: Meta, column: NumericColumn) -> Self {
        Similar { meta, column }
    }

    fn format_range(&self, students: &[usize]) -> String {
        let (low, high) = self.column.range(students);
        if (high - low).abs() < 1e-9 {
            format_number(low)
        } else {
            format!("{}–{}", format_number(low), format_number(high))
        }
    }
}

impl GroupwiseCriterion for Similar {
    type Stats = GroupSums;

    fn meta(&self) -> &Meta {
        &self.meta
    }

    /// Ideal: ordenar y cortar en bloques contiguos. Al azar: SS · (n − K) / (n − 1).
    fn reference(&self, sizes: &[usize]) -> Reference {
        let n = self.column.student_count();
        let random = if n > 1 {
            self.column.sum_squares * (n - sizes.len()) as f64 / (n as f64 - 1.0)
        } else {
            0.0
        };
        Reference {
            ideal: sorted_blocks_cost(&self.column.centered, sizes),
            random,
        }
    }

    fn empty_stats(&self) -> GroupSums {
        GroupSums::default()
    }

    fn add(&self, sums: &mut GroupSums, student: usize) {
        sums.add(self.column.centered[student]);
    }

    fn remove(&self, sums: &mut GroupSums, student: usize) {
        sums.remove(self.column.centered[student]);
    }

    fn group_cost(&self, sums: &GroupSums, size: usize) -> f64 {
        sums.within(size)
    }

    /// La dispersión se muestra como desviación típica dentro de los grupos.
    fn describe(
        &self,
        groups: &[Group<'_, GroupSums>],
        cost: f64,
        reference: &Reference,
    ) -> String {
        let assigned: usize = groups.iter().map(Group::size).sum();
        let deviation = |sum_squares: f64| (sum_squares.max(0.0) / assigned.max(1) as f64).sqrt();
        format!(
            "dispersión dentro de los grupos {:.2} (mínimo ≈ {:.2}, al azar ≈ {:.2})",
            deviation(cost),
            deviation(reference.ideal),
            deviation(reference.random)
        )
    }

    fn describe_group(&self, group: &Group<'_, GroupSums>) -> String {
        format!("{} {}", self.meta.label, self.format_range(group.members))
    }

    fn explain_join(&self, group: &Group<'_, GroupSums>, students: &[usize]) -> String {
        let own: Vec<String> = students
            .iter()
            .map(|&s| format_number(self.column.value(s)))
            .collect();
        if group.is_empty() {
            return format!("{} (grupo vacío)", own.join(", "));
        }
        let (low, high) = self.column.range(group.members);
        format!(
            "{} en un grupo con {}–{}",
            own.join(", "),
            format_number(low),
            format_number(high)
        )
    }
}

/// Menor variación dentro de grupos con estos tamaños: valores ordenados en bloques
/// contiguos. Se prueban los tamaños de menor a mayor y al revés, y se queda el mejor.
fn sorted_blocks_cost(centered: &[f64], sizes: &[usize]) -> f64 {
    let mut sorted = centered.to_vec();
    sorted.sort_by(f64::total_cmp);
    let cost_with_order = |order: &[usize]| {
        let mut start = 0;
        let mut total = 0.0;
        for &size in order {
            let block = &sorted[start..(start + size).min(sorted.len())];
            let mut sums = GroupSums::default();
            block.iter().for_each(|&x| sums.add(x));
            total += sums.within(block.len());
            start += size;
        }
        total
    };
    let mut ascending = sizes.to_vec();
    ascending.sort_unstable();
    let descending: Vec<usize> = ascending.iter().rev().copied().collect();
    cost_with_order(&ascending).min(cost_with_order(&descending))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_blocks_pair_close_values() {
        assert!((sorted_blocks_cost(&[-3.0, 3.0, -2.0, 2.0], &[2, 2]) - 1.0).abs() < 1e-9);
    }
}
