//! Los alumnos: id y nombre de cada uno, y búsqueda por cualquiera de los dos.

use crate::config::Config;
use crate::data::Table;
use anyhow::{Result, bail};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Roster {
    ids: Vec<String>,
    names: Vec<String>,
    by_id: HashMap<String, usize>,
    /// `None` cuando dos alumnos comparten nombre: buscar por ese nombre es ambiguo.
    by_name: HashMap<String, Option<usize>>,
}

impl Roster {
    /// Lee ids y nombres de la tabla. Si un alumno no tiene nombre, se usa su id.
    pub fn from_table(table: &Table, cfg: &Config) -> Result<Roster> {
        if table.rows.is_empty() {
            bail!("no hay alumnos en el archivo");
        }
        let id_column = table.column(&cfg.id_column)?;
        let name_column = cfg
            .name_column
            .as_deref()
            .map(|c| table.column(c))
            .transpose()?;
        let ids: Vec<String> = table
            .rows
            .iter()
            .map(|row| row[id_column].trim().to_string())
            .collect();
        let names = table
            .rows
            .iter()
            .zip(&ids)
            .map(|(row, id)| {
                let name = name_column.map_or("", |c| row[c].trim());
                if name.is_empty() {
                    id.clone()
                } else {
                    name.to_string()
                }
            })
            .collect();
        Ok(Roster::new(ids, names))
    }

    pub fn new(ids: Vec<String>, names: Vec<String>) -> Roster {
        let by_id = ids
            .iter()
            .enumerate()
            .map(|(student, id)| (search_key(id), student))
            .collect();
        let mut by_name = HashMap::new();
        for (student, name) in names.iter().enumerate() {
            by_name
                .entry(search_key(name))
                .and_modify(|slot| *slot = None)
                .or_insert(Some(student));
        }
        Roster {
            ids,
            names,
            by_id,
            by_name,
        }
    }

    pub fn len(&self) -> usize {
        self.ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn id(&self, student: usize) -> &str {
        &self.ids[student]
    }

    pub fn name(&self, student: usize) -> &str {
        &self.names[student]
    }

    /// Busca por id, sin distinguir mayúsculas.
    pub fn find_by_id(&self, id: &str) -> Option<usize> {
        self.by_id.get(&search_key(id)).copied()
    }

    /// Busca por id o, si no hay, por nombre exacto que no esté repetido.
    pub fn find(&self, id_or_name: &str) -> Option<usize> {
        let key = search_key(id_or_name);
        self.by_id
            .get(&key)
            .copied()
            .or_else(|| self.by_name.get(&key).copied().flatten())
    }
}

fn search_key(text: &str) -> String {
    text.trim().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_by_id_or_unique_name() {
        let roster = Roster::new(
            vec!["a1".into(), "a2".into(), "a3".into()],
            vec!["Ana".into(), "Beto".into(), "Beto".into()],
        );
        assert_eq!(roster.find("A1"), Some(0));
        assert_eq!(roster.find("ana"), Some(0));
        assert_eq!(roster.find("Beto"), None, "nombre repetido: ambiguo");
        assert_eq!(roster.find_by_id("Ana"), None);
    }
}
