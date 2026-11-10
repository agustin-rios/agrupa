//! Lectura de CSV (exportado de Google Forms, Excel, etc.).

use anyhow::{Context, Result, anyhow, bail};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct Table {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl Table {
    pub fn read(path: &Path, delimiter: Option<u8>) -> Result<Table> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("No pude leer {}", path.display()))?;
        Self::parse(&text, delimiter).with_context(|| format!("Error al leer {}", path.display()))
    }

    pub fn parse(text: &str, delimiter: Option<u8>) -> Result<Table> {
        let text = text.trim_start_matches('\u{feff}');
        let delim = delimiter.unwrap_or_else(|| sniff_delimiter(text));
        let mut rdr = csv::ReaderBuilder::new()
            .delimiter(delim)
            .flexible(true)
            .trim(csv::Trim::All)
            .from_reader(text.as_bytes());
        let headers: Vec<String> = rdr.headers()?.iter().map(str::to_string).collect();
        if headers.iter().all(|h| h.is_empty()) {
            bail!("el archivo no tiene encabezados");
        }
        let mut rows = Vec::new();
        for rec in rdr.records() {
            let rec = rec?;
            if rec.iter().all(|c| c.trim().is_empty()) {
                continue;
            }
            let mut row: Vec<String> = rec.iter().map(str::to_string).collect();
            row.resize(headers.len(), String::new());
            rows.push(row);
        }
        Ok(Table { headers, rows })
    }

    pub fn find_column(&self, name: &str) -> Option<usize> {
        let key = name.trim().to_lowercase();
        self.headers.iter().position(|h| h.trim().to_lowercase() == key)
    }

    pub fn column(&self, name: &str) -> Result<usize> {
        self.find_column(name).ok_or_else(|| {
            anyhow!(
                "la columna '{name}' no existe. Columnas disponibles: {}",
                self.headers.join(", ")
            )
        })
    }

    /// Agrega las filas de otra tabla, alineando por nombre de columna.
    pub fn append(&mut self, other: &Table) {
        let map: Vec<Option<usize>> = self.headers.iter().map(|h| other.find_column(h)).collect();
        for row in &other.rows {
            self.rows.push(
                map.iter()
                    .map(|m| m.map(|i| row[i].clone()).unwrap_or_default())
                    .collect(),
            );
        }
    }

    /// Si un id aparece varias veces (p. ej. alguien respondió el formulario dos veces),
    /// se queda la última respuesta. Las filas sin id se descartan.
    pub fn dedupe_by(&mut self, col: usize) -> Vec<String> {
        let mut warnings = Vec::new();
        let mut order: Vec<String> = Vec::new();
        let mut latest: HashMap<String, (usize, Vec<String>)> = HashMap::new();
        let mut empty = 0;
        for row in self.rows.drain(..) {
            let key = row[col].trim().to_lowercase();
            if key.is_empty() {
                empty += 1;
                continue;
            }
            match latest.get_mut(&key) {
                Some(entry) => {
                    entry.0 += 1;
                    entry.1 = row;
                }
                None => {
                    order.push(key.clone());
                    latest.insert(key, (1, row));
                }
            }
        }
        for key in order {
            let (times, row) = latest.remove(&key).expect("clave registrada");
            if times > 1 {
                warnings.push(format!(
                    "el id '{}' aparece {times} veces; se usa la última respuesta",
                    row[col].trim()
                ));
            }
            self.rows.push(row);
        }
        if empty > 0 {
            warnings.push(format!("{empty} filas sin id fueron descartadas"));
        }
        warnings
    }
}

fn sniff_delimiter(text: &str) -> u8 {
    let first = text.lines().next().unwrap_or("");
    let mut best = b',';
    let mut best_count = first.matches(',').count();
    for &c in b";\t" {
        let n = first.matches(c as char).count();
        if n > best_count {
            best = c;
            best_count = n;
        }
    }
    best
}

/// Acepta "5.8" y también "5,8" (coma decimal).
pub fn parse_number(s: &str) -> Option<f64> {
    let s = s.trim();
    s.parse::<f64>()
        .ok()
        .or_else(|| s.replace(',', ".").parse::<f64>().ok())
        .filter(|x| x.is_finite())
}

/// Separa una celda con varios valores ("Backend, Datos" o "a01;a02").
pub fn split_list(s: &str, separators: &str) -> Vec<String> {
    s.split(|c| separators.contains(c))
        .map(str::trim)
        .filter(|x| !x.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_semicolon_and_decimal_comma() {
        let t = Table::parse("id;nota\na1;5,8\na2;6.1\n", None).unwrap();
        assert_eq!(t.headers, vec!["id", "nota"]);
        assert_eq!(parse_number(&t.rows[0][1]), Some(5.8));
        assert_eq!(parse_number(&t.rows[1][1]), Some(6.1));
    }

    #[test]
    fn keeps_last_duplicate() {
        let mut t = Table::parse("id,x\na,1\nb,2\nA,3\n", None).unwrap();
        let w = t.dedupe_by(0);
        assert_eq!(t.rows.len(), 2);
        assert_eq!(t.rows[0][1], "3");
        assert_eq!(w.len(), 1);
    }

    #[test]
    fn splits_google_forms_checkboxes() {
        assert_eq!(split_list("Backend, Datos ;", ";,"), vec!["Backend", "Datos"]);
    }
}
