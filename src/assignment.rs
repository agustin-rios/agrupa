//! Lectura y escritura de la asignación (CSV `id,nombre,grupo`).

use crate::data::Table;
use crate::model::{Lookup, Model, NONE, preview};
use anyhow::{Context, Result, anyhow, bail};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Assignment {
    /// Grupo de cada alumno del modelo (`NONE` si no aparece en el archivo).
    pub group_of: Vec<u32>,
    /// Nombre de cada grupo tal como viene en el archivo ("1", "A", ...).
    pub labels: Vec<String>,
    pub warnings: Vec<String>,
}

impl Assignment {
    pub fn read(path: &Path, m: &Model, id_column: &str) -> Result<Assignment> {
        let t = Table::read(path, None)?;
        Self::from_table(&t, m, id_column)
            .with_context(|| format!("Error en la asignación {}", path.display()))
    }

    pub fn from_table(t: &Table, m: &Model, id_column: &str) -> Result<Assignment> {
        let id_col = t
            .find_column("id")
            .or_else(|| t.find_column(id_column))
            .ok_or_else(|| anyhow!("falta la columna 'id'. Columnas: {}", t.headers.join(", ")))?;
        let g_col = t
            .find_column("grupo")
            .or_else(|| t.find_column("group"))
            .ok_or_else(|| {
                anyhow!(
                    "falta la columna 'grupo'. Columnas: {}",
                    t.headers.join(", ")
                )
            })?;
        let lookup = Lookup::new(&m.ids, &m.names);
        let mut labels: Vec<String> = Vec::new();
        let mut group_of = vec![NONE; m.n()];
        let mut unknown = Vec::new();
        let mut no_group = 0;
        for row in &t.rows {
            let id = row[id_col].trim();
            let label = row[g_col].trim();
            if id.is_empty() {
                continue;
            }
            if label.is_empty() {
                no_group += 1;
                continue;
            }
            let Some(i) = lookup.find_id(id) else {
                unknown.push(id.to_string());
                continue;
            };
            let g = match labels.iter().position(|l| l == label) {
                Some(g) => g,
                None => {
                    labels.push(label.to_string());
                    labels.len() - 1
                }
            };
            group_of[i] = g as u32;
        }
        if labels.is_empty() {
            bail!("no hay ningún alumno con grupo");
        }
        let mut warnings = Vec::new();
        if !unknown.is_empty() {
            warnings.push(format!(
                "{} ids de la asignación no están en la lista de alumnos y se ignoran: {}",
                unknown.len(),
                preview(&unknown)
            ));
        }
        if no_group > 0 {
            warnings.push(format!("{no_group} filas de la asignación no tienen grupo"));
        }

        // Ordena los grupos: numéricamente si todos son números; si no, por aparición.
        let numeric: Option<Vec<i64>> = labels.iter().map(|l| l.parse().ok()).collect();
        if let Some(nums) = numeric {
            let mut order: Vec<usize> = (0..labels.len()).collect();
            order.sort_by_key(|&g| nums[g]);
            let mut rank = vec![0u32; labels.len()];
            for (r, &g) in order.iter().enumerate() {
                rank[g] = r as u32;
            }
            for g in group_of.iter_mut().filter(|g| **g != NONE) {
                *g = rank[*g as usize];
            }
            labels = order.iter().map(|&g| labels[g].clone()).collect();
        }
        Ok(Assignment {
            group_of,
            labels,
            warnings,
        })
    }

    pub fn numbered(group_of: Vec<u32>, k: usize) -> Assignment {
        Assignment {
            group_of,
            labels: (1..=k).map(|g| g.to_string()).collect(),
            warnings: Vec::new(),
        }
    }

    pub fn k(&self) -> usize {
        self.labels.len()
    }

    /// Escribe `id,nombre,grupo`, ordenado por grupo y luego según la lista de alumnos.
    pub fn write(&self, path: &Path, m: &Model) -> Result<()> {
        let mut w = csv::Writer::from_path(path)
            .with_context(|| format!("No pude escribir {}", path.display()))?;
        w.write_record(["id", "nombre", "grupo"])?;
        let mut order: Vec<usize> = (0..m.n()).filter(|&i| self.group_of[i] != NONE).collect();
        order.sort_by_key(|&i| self.group_of[i]);
        for i in order {
            w.write_record([
                &m.ids[i],
                &m.names[i],
                &self.labels[self.group_of[i] as usize],
            ])?;
        }
        w.flush()?;
        Ok(())
    }
}
