//! Numérico · `balance`: promedios parejos entre grupos.
//!
//! Costo = Σ_g n_g · (media_g − media)², la variación entre grupos. Como la variación
//! total es fija, minimizarla maximiza la variedad dentro de cada grupo (anticlustering).

use super::groupwise::{Group, GroupwiseCriterion};
use super::numeric::{GroupSums, NumericColumn};
use super::{Meta, Reference};

pub struct Balance {
    meta: Meta,
    column: NumericColumn,
}

impl Balance {
    pub fn new(meta: Meta, column: NumericColumn) -> Self {
        Balance { meta, column }
    }

    fn group_mean(&self, sums: &GroupSums, size: usize) -> f64 {
        self.column.mean + sums.sum / size as f64
    }
}

impl GroupwiseCriterion for Balance {
    type Stats = GroupSums;

    fn meta(&self) -> &Meta {
        &self.meta
    }

    /// Ideal 0. Al azar, la media de un grupo varía como en un muestreo sin reposición,
    /// lo que suma SS · (K − 1) / (n − 1).
    fn reference(&self, sizes: &[usize]) -> Reference {
        let n = self.column.student_count();
        let groups = sizes.len() as f64;
        let random = if n > 1 {
            self.column.sum_squares * (groups - 1.0) / (n as f64 - 1.0)
        } else {
            0.0
        };
        Reference { ideal: 0.0, random }
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
        sums.between(size)
    }

    fn describe(
        &self,
        groups: &[Group<'_, GroupSums>],
        _cost: f64,
        _reference: &Reference,
    ) -> String {
        let means: Vec<f64> = groups
            .iter()
            .filter(|g| !g.is_empty())
            .map(|g| self.group_mean(g.stats, g.size()))
            .collect();
        let low = means.iter().copied().fold(f64::INFINITY, f64::min);
        let high = means.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        format!(
            "promedios de grupo entre {low:.2} y {high:.2} (global {:.2})",
            self.column.mean
        )
    }

    fn describe_group(&self, group: &Group<'_, GroupSums>) -> String {
        format!(
            "{} {:.2}",
            self.meta.label,
            self.group_mean(group.stats, group.size())
        )
    }

    fn explain_join(&self, group: &Group<'_, GroupSums>, students: &[usize]) -> String {
        let before = if group.is_empty() {
            "—".to_string()
        } else {
            format!("{:.2}", self.group_mean(group.stats, group.size()))
        };
        let mut after = *group.stats;
        for &student in students {
            after.add(self.column.centered[student]);
        }
        let after = self.group_mean(&after, group.size() + students.len());
        format!(
            "promedio del grupo {before} → {after:.2} (global {:.2})",
            self.column.mean
        )
    }
}
