//! Lectura y escritura de la asignación: el CSV `id,nombre,grupo`.

use crate::data::Table;
use crate::partition::Partition;
use crate::roster::Roster;
use crate::text::preview;
use anyhow::{Context, Result, anyhow, bail};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Assignment {
    /// Grupo de cada alumno (`None` si no aparece en el archivo).
    pub groups: Vec<Option<usize>>,
    /// Nombre de cada grupo tal como viene en el archivo ("1", "A"...).
    pub labels: Vec<String>,
    pub warnings: Vec<String>,
}

impl Assignment {
    /// Grupos numerados 1..K.
    pub fn numbered(groups: &[usize], group_count: usize) -> Assignment {
        Assignment {
            groups: groups.iter().map(|&g| Some(g)).collect(),
            labels: (1..=group_count).map(|g| g.to_string()).collect(),
            warnings: Vec::new(),
        }
    }

    pub fn read(path: &Path, roster: &Roster, id_column: &str) -> Result<Assignment> {
        let table = Table::read(path, None)?;
        Self::from_table(&table, roster, id_column)
            .with_context(|| format!("Error en la asignación {}", path.display()))
    }

    /// Lee las columnas `id` (o la de la configuración) y `grupo`; el resto se ignora.
    pub fn from_table(table: &Table, roster: &Roster, id_column: &str) -> Result<Assignment> {
        let id_index = find_column(table, &["id", id_column])?;
        let group_index = find_column(table, &["grupo", "group"])?;
        let mut labels: Vec<String> = Vec::new();
        let mut groups = vec![None; roster.len()];
        let mut unknown_ids = Vec::new();
        let mut without_group = 0;
        for row in &table.rows {
            let (id, label) = (row[id_index].trim(), row[group_index].trim());
            if id.is_empty() {
                continue;
            }
            if label.is_empty() {
                without_group += 1;
                continue;
            }
            let Some(student) = roster.find_by_id(id) else {
                unknown_ids.push(id.to_string());
                continue;
            };
            groups[student] = Some(label_index(&mut labels, label));
        }
        if labels.is_empty() {
            bail!("no hay ningún alumno con grupo");
        }

        let mut warnings = Vec::new();
        if !unknown_ids.is_empty() {
            warnings.push(format!(
                "{} ids de la asignación no están en la lista de alumnos y se ignoran: {}",
                unknown_ids.len(),
                preview(&unknown_ids)
            ));
        }
        if without_group > 0 {
            warnings.push(format!(
                "{without_group} filas de la asignación no tienen grupo"
            ));
        }
        let mut assignment = Assignment {
            groups,
            labels,
            warnings,
        };
        assignment.sort_numeric_labels();
        Ok(assignment)
    }

    pub fn group_count(&self) -> usize {
        self.labels.len()
    }

    pub fn partition(&self) -> Partition {
        Partition::new(self.group_count(), self.groups.clone())
    }

    /// Escribe `id,nombre,grupo`, ordenado por grupo y luego según la lista de alumnos.
    pub fn write(&self, path: &Path, roster: &Roster) -> Result<()> {
        let mut writer = csv::Writer::from_path(path)
            .with_context(|| format!("No pude escribir {}", path.display()))?;
        writer.write_record(["id", "nombre", "grupo"])?;
        let mut assigned: Vec<(usize, usize)> = self
            .groups
            .iter()
            .enumerate()
            .filter_map(|(student, group)| Some((student, (*group)?)))
            .collect();
        assigned.sort_by_key(|&(_, group)| group);
        for (student, group) in assigned {
            writer.write_record([
                roster.id(student),
                roster.name(student),
                &self.labels[group],
            ])?;
        }
        writer.flush()?;
        Ok(())
    }

    /// Si todos los grupos son números, los ordena numéricamente ("2" antes que "10").
    fn sort_numeric_labels(&mut self) {
        let Some(numbers) = self
            .labels
            .iter()
            .map(|l| l.parse::<i64>().ok())
            .collect::<Option<Vec<_>>>()
        else {
            return;
        };
        let mut order: Vec<usize> = (0..self.labels.len()).collect();
        order.sort_by_key(|&g| numbers[g]);
        let mut new_index = vec![0; order.len()];
        for (index, &group) in order.iter().enumerate() {
            new_index[group] = index;
        }
        for group in self.groups.iter_mut().flatten() {
            *group = new_index[*group];
        }
        self.labels = order.iter().map(|&g| self.labels[g].clone()).collect();
    }
}

fn find_column(table: &Table, names: &[&str]) -> Result<usize> {
    names
        .iter()
        .find_map(|name| table.find_column(name))
        .ok_or_else(|| {
            anyhow!(
                "falta la columna '{}'. Columnas: {}",
                names[0],
                table.headers.join(", ")
            )
        })
}

/// Índice del grupo con esa etiqueta, agregándolo si es nuevo.
fn label_index(labels: &mut Vec<String>, label: &str) -> usize {
    labels.iter().position(|l| l == label).unwrap_or_else(|| {
        labels.push(label.to_string());
        labels.len() - 1
    })
}
