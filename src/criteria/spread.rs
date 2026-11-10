//! Categórico o etiquetas · `spread`: cada valor repartido en proporción.
//!
//! Un grupo de tamaño s "debería" tener s · T_v / n alumnos con el valor v. Como los
//! conteos son enteros, cualquier valor entre el piso y el techo de esa cifra está bien;
//! fuera de ese rango, el costo es la distancia al cuadrado.

use super::groupwise::{Group, GroupwiseCriterion};
use super::labels::LabelColumn;
use super::stats::Hypergeometric;
use super::{Meta, Reference};
use crate::text::format_counts;

/// Hasta esta cantidad de valores, el reporte muestra el rango de cada uno.
const MAX_VALUES_LISTED: usize = 4;

pub struct Spread {
    meta: Meta,
    column: LabelColumn,
}

impl Spread {
    pub fn new(meta: Meta, column: LabelColumn) -> Self {
        Spread { meta, column }
    }

    fn penalty(&self, count: usize, group_size: usize, value: usize) -> f64 {
        band_penalty(
            count,
            group_size,
            self.column.totals[value],
            self.column.student_count(),
        )
    }

    /// Cantidad de pares (grupo, valor) fuera de su rango proporcional.
    fn out_of_range(&self, groups: &[Group<'_, Vec<u32>>]) -> usize {
        groups
            .iter()
            .filter(|g| !g.is_empty())
            .map(|g| {
                (0..self.column.value_count())
                    .filter(|&v| self.penalty(g.stats[v] as usize, g.size(), v) > 0.0)
                    .count()
            })
            .sum()
    }
}

/// Rango aceptable [piso, techo] de total · size / n, calculado con enteros.
pub fn fair_range(size: usize, total: usize, n: usize) -> (usize, usize) {
    let scaled = size * total;
    (scaled / n.max(1), scaled.div_ceil(n.max(1)))
}

/// Distancia al cuadrado del conteo al rango proporcional (0 si está dentro).
pub fn band_penalty(count: usize, size: usize, total: usize, n: usize) -> f64 {
    let (low, high) = fair_range(size, total, n);
    let distance = if count < low {
        low - count
    } else {
        count.saturating_sub(high)
    };
    (distance * distance) as f64
}

impl GroupwiseCriterion for Spread {
    type Stats = Vec<u32>;

    fn meta(&self) -> &Meta {
        &self.meta
    }

    /// Ideal 0 (alcanzable para un atributo aislado). Al azar, cada conteo sigue una
    /// hipergeométrica y se promedia su penalización.
    fn reference(&self, sizes: &[usize]) -> Reference {
        let n = self.column.student_count();
        let hypergeometric = Hypergeometric::new(n);
        let mut random = 0.0;
        for &size in sizes {
            for &total in &self.column.totals {
                let pmf = hypergeometric.pmf(total, size);
                random += pmf
                    .iter()
                    .enumerate()
                    .map(|(x, p)| p * band_penalty(x, size, total, n))
                    .sum::<f64>();
            }
        }
        Reference { ideal: 0.0, random }
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

    fn group_cost(&self, counts: &Vec<u32>, size: usize) -> f64 {
        counts
            .iter()
            .enumerate()
            .map(|(value, &count)| self.penalty(count as usize, size, value))
            .sum()
    }

    fn describe(
        &self,
        groups: &[Group<'_, Vec<u32>>],
        _cost: f64,
        _reference: &Reference,
    ) -> String {
        let out = self.out_of_range(groups);
        if !self.column.categorical || self.column.value_count() > MAX_VALUES_LISTED {
            let cells = groups.iter().filter(|g| !g.is_empty()).count() * self.column.value_count();
            return format!("{out} de {cells} conteos fuera de proporción");
        }
        let ranges: Vec<String> = self
            .column
            .labels
            .iter()
            .enumerate()
            .map(|(value, label)| {
                let counts = groups
                    .iter()
                    .filter(|g| !g.is_empty())
                    .map(|g| g.stats[value]);
                let (low, high) = (counts.clone().min().unwrap_or(0), counts.max().unwrap_or(0));
                if low == high {
                    format!("{label} {low}")
                } else {
                    format!("{label} {low}–{high}")
                }
            })
            .collect();
        let mut text = format!("{} por grupo", ranges.join(" · "));
        if out > 0 {
            text.push_str(&format!(" ({out} fuera de proporción)"));
        }
        text
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
        let new_size = group.size() + students.len();
        own.iter()
            .map(|&value| {
                let before = group.stats[value as usize] as usize;
                let after = before + self.column.count_with(students, value);
                let (low, high) = fair_range(
                    new_size,
                    self.column.totals[value as usize],
                    self.column.student_count(),
                );
                let range = if low == high {
                    format!("{low}")
                } else {
                    format!("{low}–{high}")
                };
                format!(
                    "{}: {before} → {after} (proporcional {range})",
                    self.column.labels[value as usize]
                )
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_penalty_inside_fair_range() {
        // 4 de 32 en un grupo de 4: lo proporcional es 0,5, así que 0 o 1 están bien.
        assert_eq!(band_penalty(0, 4, 4, 32), 0.0);
        assert_eq!(band_penalty(1, 4, 4, 32), 0.0);
        assert_eq!(band_penalty(2, 4, 4, 32), 1.0);
        // Lo proporcional es exactamente 2: solo 2 está bien.
        assert_eq!(fair_range(4, 16, 32), (2, 2));
        assert_eq!(band_penalty(3, 4, 16, 32), 1.0);
    }
}
