//! Columnas numéricas: valores centrados en la media y sumas por grupo.

use crate::data::parse_number;
use crate::roster::Roster;
use crate::text::preview;

pub struct NumericColumn {
    /// Valor de cada alumno menos la media. Las celdas sin número quedan en 0, es decir,
    /// cuentan como la media. Centrar evita perder precisión en Q − S²/n.
    pub centered: Vec<f64>,
    pub mean: f64,
    /// Σ (x − media)² sobre todos los alumnos.
    pub sum_squares: f64,
}

impl NumericColumn {
    /// Lee la columna y avisa de celdas vacías o que no son números.
    pub fn read(
        label: &str,
        cells: &[&str],
        roster: &Roster,
        warnings: &mut Vec<String>,
    ) -> NumericColumn {
        let mut empty = 0;
        let mut invalid = Vec::new();
        let values: Vec<Option<f64>> = cells
            .iter()
            .enumerate()
            .map(|(student, cell)| {
                if cell.trim().is_empty() {
                    empty += 1;
                    return None;
                }
                let value = parse_number(cell);
                if value.is_none() {
                    invalid.push(format!("'{}' ({})", cell.trim(), roster.id(student)));
                }
                value
            })
            .collect();

        let known: Vec<f64> = values.iter().flatten().copied().collect();
        let mean = if known.is_empty() {
            0.0
        } else {
            known.iter().sum::<f64>() / known.len() as f64
        };
        warn_missing(label, empty, &invalid, mean, warnings);

        let centered: Vec<f64> = values.iter().map(|v| v.map_or(0.0, |x| x - mean)).collect();
        let sum_squares = centered.iter().map(|x| x * x).sum();
        NumericColumn {
            centered,
            mean,
            sum_squares,
        }
    }

    pub fn student_count(&self) -> usize {
        self.centered.len()
    }

    /// Valor original del alumno (la media si no tenía dato).
    pub fn value(&self, student: usize) -> f64 {
        self.mean + self.centered[student]
    }

    /// Menor y mayor valor original entre los alumnos dados.
    pub fn range(&self, students: &[usize]) -> (f64, f64) {
        let values = students.iter().map(|&s| self.value(s));
        let low = values.clone().fold(f64::INFINITY, f64::min);
        let high = values.fold(f64::NEG_INFINITY, f64::max);
        (low, high)
    }
}

fn warn_missing(
    label: &str,
    empty: usize,
    invalid: &[String],
    mean: f64,
    warnings: &mut Vec<String>,
) {
    if !invalid.is_empty() {
        warnings.push(format!(
            "{label}: valores no numéricos tratados como vacíos: {}",
            preview(invalid)
        ));
    }
    let missing = empty + invalid.len();
    if missing > 0 {
        let one = missing == 1;
        warnings.push(format!(
            "{label}: {missing} {} sin número se {} como el promedio ({mean:.2})",
            if one { "celda" } else { "celdas" },
            if one { "cuenta" } else { "cuentan" },
        ));
    }
}

/// Sumas de los valores centrados de un grupo.
#[derive(Debug, Clone, Copy, Default)]
pub struct GroupSums {
    pub sum: f64,
    pub sum_squares: f64,
}

impl GroupSums {
    pub fn add(&mut self, x: f64) {
        self.sum += x;
        self.sum_squares += x * x;
    }

    pub fn remove(&mut self, x: f64) {
        self.sum -= x;
        self.sum_squares -= x * x;
    }

    /// n · (media del grupo − media global)²: lo que el grupo se aleja del promedio general.
    pub fn between(&self, size: usize) -> f64 {
        if size == 0 {
            0.0
        } else {
            self.sum * self.sum / size as f64
        }
    }

    /// Σ (x − media del grupo)²: cuán dispersos están sus integrantes.
    pub fn within(&self, size: usize) -> f64 {
        if size == 0 {
            0.0
        } else {
            (self.sum_squares - self.sum * self.sum / size as f64).max(0.0)
        }
    }
}
