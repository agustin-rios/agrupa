//! Relaciones entre alumnos: preferencias (blandas) y restricciones `together`/`apart` (duras).

use crate::config::{Config, PreferencesConfig};
use crate::data::{Table, split_list};
use crate::roster::Roster;
use crate::text::preview;
use anyhow::{Result, bail};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferenceKind {
    /// Quiere trabajar con la otra persona.
    Prefer,
    /// Prefiere no quedar con la otra persona.
    Avoid,
}

/// Una elección de un alumno sobre otro.
#[derive(Debug, Clone, Copy)]
pub struct Preference {
    pub from: usize,
    pub to: usize,
    pub kind: PreferenceKind,
    /// Quien elige a varios reparte su peso: cada alumno pesa lo mismo en total.
    pub weight: f64,
}

/// Alumnos que deben ir juntos. Cada alumno está en exactamente un bloque;
/// los que no tienen restricción forman un bloque de uno.
#[derive(Debug, Clone)]
pub struct Blocks {
    members: Vec<Vec<usize>>,
    block_of: Vec<usize>,
}

impl Blocks {
    pub fn all(&self) -> &[Vec<usize>] {
        &self.members
    }

    /// El bloque del alumno (él mismo, si va suelto).
    pub fn of(&self, student: usize) -> &[usize] {
        &self.members[self.block_of[student]]
    }

    pub fn is_single(&self, student: usize) -> bool {
        self.of(student).len() == 1
    }

    pub fn same_block(&self, a: usize, b: usize) -> bool {
        self.block_of[a] == self.block_of[b]
    }

    /// Arma los bloques a partir de un representante por alumno (los de igual representante van juntos).
    fn from_representatives(representatives: impl Iterator<Item = usize>) -> Blocks {
        let mut index_of: HashMap<usize, usize> = HashMap::new();
        let mut members: Vec<Vec<usize>> = Vec::new();
        let mut block_of = Vec::new();
        for (student, representative) in representatives.enumerate() {
            let block = *index_of.entry(representative).or_insert_with(|| {
                members.push(Vec::new());
                members.len() - 1
            });
            members[block].push(student);
            block_of.push(block);
        }
        Blocks { members, block_of }
    }
}

#[derive(Debug, Clone)]
pub struct Relations {
    pub preferences: Vec<Preference>,
    /// Pares que no pueden compartir grupo.
    pub apart: Vec<(usize, usize)>,
    pub blocks: Blocks,
    apart_partners: Vec<Vec<usize>>,
}

impl Relations {
    pub fn build(
        table: &Table,
        cfg: &Config,
        roster: &Roster,
        warnings: &mut Vec<String>,
    ) -> Result<Relations> {
        let preferences = match &cfg.preferences {
            Some(p) => read_preferences(table, p, roster, warnings)?,
            None => Vec::new(),
        };
        let blocks = together_blocks(&cfg.constraints.together, roster, warnings);
        let apart = apart_pairs(&cfg.constraints.apart, roster, &blocks, warnings)?;
        let mut apart_partners = vec![Vec::new(); roster.len()];
        for &(a, b) in &apart {
            apart_partners[a].push(b);
            apart_partners[b].push(a);
        }
        Ok(Relations {
            preferences,
            apart,
            blocks,
            apart_partners,
        })
    }

    /// Con quiénes no puede compartir grupo el alumno.
    pub fn apart_partners(&self, student: usize) -> &[usize] {
        &self.apart_partners[student]
    }
}

fn read_preferences(
    table: &Table,
    cfg: &PreferencesConfig,
    roster: &Roster,
    warnings: &mut Vec<String>,
) -> Result<Vec<Preference>> {
    let mut preferences = Vec::new();
    let columns = [
        (&cfg.prefer_column, PreferenceKind::Prefer),
        (&cfg.avoid_column, PreferenceKind::Avoid),
    ];
    for (column, kind) in columns {
        let Some(column) = column else { continue };
        let index = table.column(column)?;
        let mut unknown = Vec::new();
        for (student, row) in table.rows.iter().enumerate() {
            let chosen =
                resolve_choices(&row[index], &cfg.separator, student, roster, &mut unknown);
            let weight = match kind {
                PreferenceKind::Prefer => 1.0 / chosen.len().max(1) as f64,
                PreferenceKind::Avoid => 1.0,
            };
            preferences.extend(chosen.into_iter().map(|to| Preference {
                from: student,
                to,
                kind,
                weight,
            }));
        }
        if !unknown.is_empty() {
            let one = unknown.len() == 1;
            warnings.push(format!(
                "{column}: {} {} con ningún id ni nombre y se {}: {}",
                unknown.len(),
                if one {
                    "valor no coincide"
                } else {
                    "valores no coinciden"
                },
                if one { "ignora" } else { "ignoran" },
                preview(&unknown)
            ));
        }
    }
    Ok(preferences)
}

/// Compañeros nombrados en una celda, sin repetir ni incluirse a sí mismo.
fn resolve_choices(
    cell: &str,
    separator: &str,
    student: usize,
    roster: &Roster,
    unknown: &mut Vec<String>,
) -> Vec<usize> {
    let mut chosen = Vec::new();
    for token in split_list(cell, separator) {
        match roster.find(&token) {
            Some(other) if other != student && !chosen.contains(&other) => chosen.push(other),
            Some(_) => {}
            None => unknown.push(format!("'{token}' ({})", roster.id(student))),
        }
    }
    chosen
}

fn together_blocks(lists: &[Vec<String>], roster: &Roster, warnings: &mut Vec<String>) -> Blocks {
    let mut union_find = UnionFind::new(roster.len());
    for list in lists {
        let members = resolve_ids(list, roster, "together", warnings);
        for pair in members.windows(2) {
            union_find.union(pair[0], pair[1]);
        }
    }
    Blocks::from_representatives((0..roster.len()).map(|s| union_find.find(s)))
}

fn apart_pairs(
    lists: &[Vec<String>],
    roster: &Roster,
    blocks: &Blocks,
    warnings: &mut Vec<String>,
) -> Result<Vec<(usize, usize)>> {
    let mut pairs = Vec::new();
    for list in lists {
        let members = resolve_ids(list, roster, "apart", warnings);
        for (index, &a) in members.iter().enumerate() {
            for &b in &members[index + 1..] {
                if blocks.same_block(a, b) {
                    bail!(
                        "restricciones contradictorias: '{}' y '{}' deben ir juntos y separados a la vez",
                        roster.id(a),
                        roster.id(b)
                    );
                }
                pairs.push((a, b));
            }
        }
    }
    Ok(pairs)
}

fn resolve_ids(
    list: &[String],
    roster: &Roster,
    constraint: &str,
    warnings: &mut Vec<String>,
) -> Vec<usize> {
    let mut found = Vec::new();
    for id in list {
        match roster.find(id) {
            Some(student) if !found.contains(&student) => found.push(student),
            Some(_) => {}
            None => warnings.push(format!(
                "[constraints] {constraint}: '{id}' no está en la lista de alumnos y se ignora"
            )),
        }
    }
    found
}

/// Conjuntos disjuntos: une alumnos que deben ir juntos, aunque vengan de listas distintas.
struct UnionFind {
    parent: Vec<usize>,
}

impl UnionFind {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
        }
    }

    fn find(&mut self, x: usize) -> usize {
        let mut root = x;
        while self.parent[root] != root {
            root = self.parent[root];
        }
        let mut node = x;
        while self.parent[node] != root {
            let next = self.parent[node];
            self.parent[node] = root;
            node = next;
        }
        root
    }

    fn union(&mut self, a: usize, b: usize) {
        let (root_a, root_b) = (self.find(a), self.find(b));
        if root_a != root_b {
            self.parent[root_a.max(root_b)] = root_a.min(root_b);
        }
    }
}
