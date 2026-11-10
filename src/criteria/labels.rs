//! Columnas categóricas y de etiquetas: qué valores tiene cada alumno y cuántos
//! hay de cada uno en un grupo.

use crate::data::split_list;
use std::collections::HashMap;

pub struct LabelColumn {
    /// Índices de los valores de cada alumno: a lo más uno si la columna es categórica.
    pub values: Vec<Vec<u16>>,
    /// Nombre de cada valor, con la primera forma en que apareció escrito.
    pub labels: Vec<String>,
    /// Cuántos alumnos tienen cada valor.
    pub totals: Vec<usize>,
    pub categorical: bool,
}

impl LabelColumn {
    /// Lee la columna. Con `separator`, cada celda puede tener varias etiquetas;
    /// sin él, la celda completa es un solo valor. Mayúsculas no importan.
    pub fn read(
        label: &str,
        cells: &[&str],
        separator: Option<&str>,
        warnings: &mut Vec<String>,
    ) -> LabelColumn {
        let mut index_of: HashMap<String, u16> = HashMap::new();
        let mut labels: Vec<String> = Vec::new();
        let mut totals: Vec<usize> = Vec::new();
        let mut empty = 0;
        let values = cells
            .iter()
            .map(|cell| {
                let tokens = match separator {
                    Some(separator) => split_list(cell, separator),
                    None if cell.trim().is_empty() => Vec::new(),
                    None => vec![cell.trim().to_string()],
                };
                if tokens.is_empty() {
                    empty += 1;
                }
                let mut own: Vec<u16> = Vec::new();
                for token in tokens {
                    let value = *index_of.entry(token.to_lowercase()).or_insert_with(|| {
                        labels.push(token.clone());
                        totals.push(0);
                        (labels.len() - 1) as u16
                    });
                    if !own.contains(&value) {
                        own.push(value);
                        totals[value as usize] += 1;
                    }
                }
                own
            })
            .collect();
        if empty > 0 {
            let one = empty == 1;
            warnings.push(format!(
                "{label}: {empty} {} vacía{} no {}",
                if one { "celda" } else { "celdas" },
                if one { "" } else { "s" },
                if one { "cuenta" } else { "cuentan" },
            ));
        }
        LabelColumn {
            values,
            labels,
            totals,
            categorical: separator.is_none(),
        }
    }

    pub fn student_count(&self) -> usize {
        self.values.len()
    }

    pub fn value_count(&self) -> usize {
        self.labels.len()
    }

    /// Alumnos sin ningún valor.
    pub fn unknown_count(&self) -> usize {
        self.values.iter().filter(|v| v.is_empty()).count()
    }

    pub fn empty_counts(&self) -> Vec<u32> {
        vec![0; self.value_count()]
    }

    pub fn add(&self, counts: &mut [u32], student: usize) {
        for &value in &self.values[student] {
            counts[value as usize] += 1;
        }
    }

    pub fn remove(&self, counts: &mut [u32], student: usize) {
        for &value in &self.values[student] {
            counts[value as usize] -= 1;
        }
    }

    /// Valores distintos que tienen estos alumnos, ordenados.
    pub fn values_of(&self, students: &[usize]) -> Vec<u16> {
        let mut values: Vec<u16> = students
            .iter()
            .flat_map(|&s| self.values[s].iter().copied())
            .collect();
        values.sort_unstable();
        values.dedup();
        values
    }

    /// Cuántos de estos alumnos tienen el valor.
    pub fn count_with(&self, students: &[usize], value: u16) -> usize {
        students
            .iter()
            .filter(|&&s| self.values[s].contains(&value))
            .count()
    }

    pub fn names(&self, values: &[u16]) -> Vec<String> {
        values
            .iter()
            .map(|&v| self.labels[v as usize].clone())
            .collect()
    }
}
