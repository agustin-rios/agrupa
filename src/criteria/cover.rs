//! Categórico o etiquetas · `cover`: la mayor variedad posible en cada grupo.
//!
//! Costo = ausencias: pares (grupo, valor) en que el valor no aparece. Con etiquetas,
//! busca que cada grupo tenga todas las habilidades; con categorías y tamaños fijos,
//! equivale a maximizar los valores distintos por grupo.

use super::groupwise::{Group, GroupwiseCriterion};
use super::labels::LabelColumn;
use super::stats::Hypergeometric;
use super::{Meta, Reference};
use crate::text::{format_counts, join_with_and, plural};

pub struct Cover {
    meta: Meta,
    column: LabelColumn,
}

impl Cover {
    pub fn new(meta: Meta, column: LabelColumn) -> Self {
        Cover { meta, column }
    }

    fn missing_values(&self, counts: &[u32]) -> Vec<String> {
        counts
            .iter()
            .zip(&self.column.labels)
            .filter(|(count, _)| **count == 0)
            .map(|(_, l)| l.clone())
            .collect()
    }

    /// Cota inferior de ausencias. Un valor con T alumnos cubre a lo más T grupos; con
    /// categorías, además, un grupo de s alumnos tiene a lo más s valores distintos.
    fn fewest_absences(&self, sizes: &[usize]) -> usize {
        let values = self.column.value_count();
        let by_value: usize = self
            .column
            .totals
            .iter()
            .map(|&t| sizes.len().saturating_sub(t))
            .sum();
        if !self.column.categorical {
            return by_value;
        }
        let by_size: usize = sizes.iter().map(|&s| values.saturating_sub(s)).sum();
        by_value.max(by_size)
    }
}

impl GroupwiseCriterion for Cover {
    type Stats = Vec<u32>;

    fn meta(&self) -> &Meta {
        &self.meta
    }

    /// Al azar, un valor falta en un grupo con la probabilidad hipergeométrica de cero.
    fn reference(&self, sizes: &[usize]) -> Reference {
        let hypergeometric = Hypergeometric::new(self.column.student_count());
        let random = sizes
            .iter()
            .flat_map(|&s| self.column.totals.iter().map(move |&t| (s, t)))
            .map(|(s, t)| hypergeometric.none(t, s))
            .sum();
        Reference {
            ideal: self.fewest_absences(sizes) as f64,
            random,
        }
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
        counts.iter().filter(|&&count| count == 0).count() as f64
    }

    fn describe(&self, groups: &[Group<'_, Vec<u32>>], cost: f64, reference: &Reference) -> String {
        if self.column.categorical {
            // En categorías se lee mejor como "valores distintos por grupo".
            let used = groups.iter().filter(|g| !g.is_empty()).count().max(1) as f64;
            let distinct = |absences: f64| self.column.value_count() as f64 - absences / used;
            return format!(
                "{:.1} valores distintos por grupo (ideal {:.1}, al azar ≈ {:.1})",
                distinct(cost),
                distinct(reference.ideal),
                distinct(reference.random)
            );
        }
        let absences = cost.round() as usize;
        format!(
            "{absences} ausencia{} (mínimo posible {}, al azar ≈ {:.1})",
            plural(absences),
            reference.ideal.round(),
            reference.random
        )
    }

    fn describe_group(&self, group: &Group<'_, Vec<u32>>) -> String {
        if self.column.categorical {
            let counts = format_counts(&self.column.labels, group.stats);
            return if counts.is_empty() {
                format!("{} —", self.meta.label)
            } else {
                format!("{} {counts}", self.meta.label)
            };
        }
        let missing = self.missing_values(group.stats);
        if missing.is_empty() {
            format!("{}: todas", self.meta.label)
        } else {
            format!("{}: falta {}", self.meta.label, missing.join(", "))
        }
    }

    fn explain_join(&self, group: &Group<'_, Vec<u32>>, students: &[usize]) -> String {
        let own = self.column.values_of(students);
        if own.is_empty() {
            return "sin dato".into();
        }
        let (new, present): (Vec<u16>, Vec<u16>) =
            own.iter().partition(|&&v| group.stats[v as usize] == 0);
        let (new, present) = (self.column.names(&new), self.column.names(&present));
        match (new.len(), self.column.categorical) {
            (0, _) => format!(
                "{} ya está{} en el grupo",
                join_with_and(&present),
                if present.len() > 1 { "n" } else { "" }
            ),
            (1, false) => format!("aporta {}, que faltaba", new[0]),
            (_, false) => format!("aporta {}, que faltaban", join_with_and(&new)),
            (1, true) => format!("aporta {}, que no estaba", new[0]),
            (_, true) => format!("aporta {}, que no estaban", join_with_and(&new)),
        }
    }
}
