//! Codificación de alumnos y funciones de costo.
//!
//! Cada criterio tiene un costo "crudo" (por ejemplo, cantidad de ausencias) que se
//! normaliza con dos referencias: el valor ideal (o una buena cota) y el valor esperado
//! de un reparto al azar. Así, con `weight = 1`, pasar de un reparto al azar a uno ideal
//! vale 1 en cualquier criterio, sin importar la escala de la columna.

use crate::config::{Config, Goal, Kind};
use crate::data::{Table, parse_number, split_list};
use anyhow::{Result, bail};
use std::collections::{BTreeMap, HashMap};

/// Grupo de un alumno que todavía no tiene grupo (solo en `sugerir`).
pub const NONE: u32 = u32::MAX;

#[derive(Debug, Clone)]
pub struct Model {
    pub ids: Vec<String>,
    pub names: Vec<String>,
    pub attrs: Vec<Attr>,
    pub edges: Vec<Edge>,
    /// Aristas que tocan a cada alumno.
    pub adj: Vec<Vec<u32>>,
    /// Bloques de alumnos que deben ir juntos (los sueltos son bloques de 1).
    pub blocks: Vec<Vec<usize>>,
    pub block_of: Vec<usize>,
    /// Peso de las preferencias, si están configuradas.
    pub pref_weight: Option<f64>,
    pub hard_penalty: f64,
    pub warnings: Vec<String>,
    pub calib: Calib,
}

#[derive(Debug, Clone)]
pub struct Attr {
    pub label: String,
    pub kind: Kind,
    pub goal: Goal,
    pub weight: f64,
    pub data: AttrData,
}

#[derive(Debug, Clone)]
pub enum AttrData {
    /// Valores centrados en la media (los vacíos quedan en 0, es decir, en la media).
    Num {
        centered: Vec<f64>,
        mean: f64,
        ss: f64,
    },
    /// Índices de valores por alumno (0 o 1 para categóricos, varios para etiquetas).
    Lab {
        values: Vec<Vec<u16>>,
        labels: Vec<String>,
        totals: Vec<usize>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeKind {
    /// `a` quiere trabajar con `b`: cuesta si quedan separados.
    Prefer,
    /// `a` prefiere evitar a `b`: cuesta si quedan juntos.
    Avoid,
    /// Restricción dura: no pueden quedar juntos.
    Apart,
}

#[derive(Debug, Clone, Copy)]
pub struct Edge {
    pub a: usize,
    pub b: usize,
    pub kind: EdgeKind,
    pub w: f64,
}

impl Edge {
    /// Costo de la arista según si los dos quedaron en el mismo grupo.
    pub fn cost(&self, same: bool) -> f64 {
        match (self.kind, same) {
            (EdgeKind::Prefer, false) | (EdgeKind::Avoid, true) | (EdgeKind::Apart, true) => self.w,
            _ => 0.0,
        }
    }
}

/// Referencias de normalización por criterio (atributos y, al final, preferencias).
#[derive(Debug, Clone, Default)]
pub struct Calib {
    pub crit: Vec<CritCalib>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CritCalib {
    pub ideal: f64,
    pub random: f64,
    /// Multiplicador del costo crudo: peso / (al azar − ideal).
    pub scale: f64,
}

impl CritCalib {
    fn new(ideal: f64, random: f64, weight: f64) -> Self {
        let span = random - ideal;
        let scale = if span > 1e-9 { weight / span } else { 0.0 };
        Self {
            ideal,
            random,
            scale,
        }
    }

    /// 1 = ideal, 0 = como al azar (puede ser negativo si es peor que al azar).
    pub fn quality(&self, raw: f64) -> f64 {
        let span = self.random - self.ideal;
        if span <= 1e-9 {
            1.0
        } else {
            (1.0 - (raw - self.ideal) / span).min(1.0)
        }
    }
}

impl Model {
    pub fn n(&self) -> usize {
        self.ids.len()
    }

    /// Cantidad de criterios con costo blando: atributos + preferencias.
    pub fn n_crit(&self) -> usize {
        self.attrs.len() + usize::from(self.pref_weight.is_some())
    }

    pub fn pref_crit(&self) -> Option<usize> {
        self.pref_weight.map(|_| self.attrs.len())
    }

    /// Codifica la tabla según la configuración.
    pub fn build(table: &Table, cfg: &Config) -> Result<Model> {
        let n = table.rows.len();
        if n == 0 {
            bail!("no hay alumnos en el archivo");
        }
        let id_col = table.column(&cfg.id_column)?;
        let name_col = match &cfg.name_column {
            Some(c) => Some(table.column(c)?),
            None => None,
        };
        let ids: Vec<String> = table
            .rows
            .iter()
            .map(|r| r[id_col].trim().to_string())
            .collect();
        let names: Vec<String> = table
            .rows
            .iter()
            .zip(&ids)
            .map(|(r, id)| {
                let name = name_col.map(|c| r[c].trim()).unwrap_or("");
                if name.is_empty() {
                    id.clone()
                } else {
                    name.to_string()
                }
            })
            .collect();
        let mut warnings = Vec::new();

        let mut attrs = Vec::new();
        for a in &cfg.attributes {
            let col = table.column(&a.column)?;
            let label = a.label.clone().unwrap_or_else(|| a.column.clone());
            let cells: Vec<&str> = table.rows.iter().map(|r| r[col].as_str()).collect();
            let data = match a.kind {
                Kind::Numeric => encode_numeric(&label, &cells, &ids, &mut warnings),
                Kind::Categorical => encode_labels(&label, &cells, None, &mut warnings),
                Kind::Tags => encode_labels(&label, &cells, Some(&a.separator), &mut warnings),
            };
            attrs.push(Attr {
                label,
                kind: a.kind,
                goal: a.goal,
                weight: a.weight,
                data,
            });
        }

        let lookup = Lookup::new(&ids, &names);
        let mut edges = Vec::new();
        let mut pref_weight = None;
        if let Some(p) = &cfg.preferences {
            pref_weight = Some(p.weight);
            for (column, kind) in [
                (&p.prefer_column, EdgeKind::Prefer),
                (&p.avoid_column, EdgeKind::Avoid),
            ] {
                let Some(column) = column else { continue };
                let col = table.column(column)?;
                let mut unknown = Vec::new();
                for (i, row) in table.rows.iter().enumerate() {
                    let mut targets = Vec::new();
                    for token in split_list(&row[col], &p.separator) {
                        match lookup.find(&token) {
                            Some(j) if j != i && !targets.contains(&j) => targets.push(j),
                            Some(_) => {}
                            None => unknown.push(format!("'{token}' ({})", ids[i])),
                        }
                    }
                    // Quien elige a varios reparte su peso: cada alumno pesa lo mismo.
                    let w = match kind {
                        EdgeKind::Prefer => 1.0 / targets.len().max(1) as f64,
                        _ => 1.0,
                    };
                    edges.extend(targets.into_iter().map(|j| Edge {
                        a: i,
                        b: j,
                        kind,
                        w,
                    }));
                }
                if !unknown.is_empty() {
                    warnings.push(format!(
                        "{column}: {} {} con ningún id ni nombre y se {}: {}",
                        unknown.len(),
                        if unknown.len() == 1 {
                            "valor no coincide"
                        } else {
                            "valores no coinciden"
                        },
                        if unknown.len() == 1 {
                            "ignora"
                        } else {
                            "ignoran"
                        },
                        preview(&unknown)
                    ));
                }
            }
        }

        // Restricciones duras.
        let mut uf = UnionFind::new(n);
        for list in &cfg.constraints.together {
            let members = resolve_ids(list, &lookup, "together", &mut warnings);
            for w in members.windows(2) {
                uf.union(w[0], w[1]);
            }
        }
        let mut apart_pairs = Vec::new();
        for list in &cfg.constraints.apart {
            let members = resolve_ids(list, &lookup, "apart", &mut warnings);
            for (x, &a) in members.iter().enumerate() {
                for &b in &members[x + 1..] {
                    if uf.find(a) == uf.find(b) {
                        bail!(
                            "restricciones contradictorias: '{}' y '{}' deben ir juntos y separados a la vez",
                            ids[a],
                            ids[b]
                        );
                    }
                    apart_pairs.push((a, b));
                }
            }
        }
        let soft_total: f64 =
            cfg.attributes.iter().map(|a| a.weight).sum::<f64>() + pref_weight.unwrap_or(0.0);
        let hard_penalty = 1000.0 * (1.0 + soft_total);
        edges.extend(apart_pairs.into_iter().map(|(a, b)| Edge {
            a,
            b,
            kind: EdgeKind::Apart,
            w: 1.0,
        }));

        let mut adj = vec![Vec::new(); n];
        for (e, edge) in edges.iter().enumerate() {
            adj[edge.a].push(e as u32);
            adj[edge.b].push(e as u32);
        }

        let mut root_block: BTreeMap<usize, usize> = BTreeMap::new();
        let mut blocks: Vec<Vec<usize>> = Vec::new();
        let mut block_of = vec![0; n];
        for (i, slot) in block_of.iter_mut().enumerate() {
            let r = uf.find(i);
            let b = *root_block.entry(r).or_insert_with(|| {
                blocks.push(Vec::new());
                blocks.len() - 1
            });
            blocks[b].push(i);
            *slot = b;
        }

        Ok(Model {
            ids,
            names,
            attrs,
            edges,
            adj,
            blocks,
            block_of,
            pref_weight,
            hard_penalty,
            warnings,
            calib: Calib::default(),
        })
    }

    /// Calcula las referencias (ideal y al azar) para grupos con estos tamaños.
    pub fn calibrate(&mut self, sizes: &[usize]) {
        let n = self.n();
        let k = sizes.len();
        let lnf = ln_factorials(n);
        let mut crit = Vec::new();
        for a in &self.attrs {
            let (ideal, random) = match (&a.data, a.goal) {
                (AttrData::Num { ss, .. }, Goal::Balance) => (
                    0.0,
                    if n > 1 {
                        ss * (k as f64 - 1.0) / (n as f64 - 1.0)
                    } else {
                        0.0
                    },
                ),
                (AttrData::Num { centered, ss, .. }, _) => {
                    let random = if n > 1 {
                        ss * (n - k) as f64 / (n as f64 - 1.0)
                    } else {
                        0.0
                    };
                    (similar_ideal(centered, sizes), random)
                }
                (AttrData::Lab { totals, .. }, Goal::Spread) => {
                    let mut random = 0.0;
                    for &ng in sizes {
                        for &t in totals {
                            let pmf = hypergeom(&lnf, n, t, ng);
                            random += pmf
                                .iter()
                                .enumerate()
                                .map(|(x, p)| p * band(x, ng, t, n))
                                .sum::<f64>();
                        }
                    }
                    (0.0, random)
                }
                (AttrData::Lab { totals, .. }, Goal::Cover) => {
                    let mut random = 0.0;
                    for &ng in sizes {
                        for &t in totals {
                            random += hypergeom(&lnf, n, t, ng)[0];
                        }
                    }
                    let c = totals.len();
                    let lb_values: usize = totals.iter().map(|&t| k.saturating_sub(t)).sum();
                    let lb = if a.kind == Kind::Categorical {
                        let lb_size: usize = sizes.iter().map(|&s| c.saturating_sub(s)).sum();
                        lb_values.max(lb_size)
                    } else {
                        lb_values
                    };
                    (lb as f64, random)
                }
                (AttrData::Lab { totals, values, .. }, _) => {
                    // Together: pares del mismo grupo con valores distintos.
                    let known: usize = totals.iter().sum();
                    let same: usize = totals.iter().map(|t| t * t).sum();
                    let p_diff = if n > 1 {
                        (known * known - same) as f64 / (n * (n - 1)) as f64
                    } else {
                        0.0
                    };
                    let random: f64 = sizes
                        .iter()
                        .map(|&s| (s * s.saturating_sub(1) / 2) as f64 * p_diff)
                        .sum();
                    let unknown = values.iter().filter(|v| v.is_empty()).count();
                    (together_ideal(totals, unknown, sizes), random)
                }
            };
            crit.push(CritCalib::new(ideal, random, a.weight));
        }
        if let Some(w) = self.pref_weight {
            let same: f64 = sizes
                .iter()
                .map(|&s| (s * s.saturating_sub(1)) as f64)
                .sum::<f64>()
                / (n * n.saturating_sub(1)).max(1) as f64;
            let random: f64 = self
                .edges
                .iter()
                .map(|e| match e.kind {
                    EdgeKind::Prefer => e.w * (1.0 - same),
                    EdgeKind::Avoid => e.w * same,
                    EdgeKind::Apart => 0.0,
                })
                .sum();
            crit.push(CritCalib::new(0.0, random, w));
        }
        self.calib = Calib { crit };
    }
}

/// Costo por grupo de un atributo, a partir de los agregados del grupo.
pub fn num_term(goal: Goal, size: usize, sum: f64, sumsq: f64) -> f64 {
    if size == 0 {
        return 0.0;
    }
    match goal {
        // Σ n_g (media_g − media)², con valores centrados.
        Goal::Balance => sum * sum / size as f64,
        // Suma de cuadrados dentro del grupo.
        _ => (sumsq - sum * sum / size as f64).max(0.0),
    }
}

/// Costo por grupo de un atributo categórico o de etiquetas.
/// `counts[c]` = integrantes con el valor `c`; `n` = total de alumnos.
pub fn lab_term(goal: Goal, size: usize, counts: &[u32], totals: &[usize], n: usize) -> f64 {
    match goal {
        Goal::Spread => counts
            .iter()
            .zip(totals)
            .map(|(&x, &t)| band(x as usize, size, t, n))
            .sum(),
        Goal::Cover => counts.iter().filter(|&&x| x == 0).count() as f64,
        _ => {
            let known: u64 = counts.iter().map(|&x| x as u64).sum();
            let same: u64 = counts.iter().map(|&x| (x as u64) * (x as u64)).sum();
            (known * known - same) as f64 / 2.0
        }
    }
}

/// Rango proporcional [piso, techo] de `total * size / n`.
pub fn fair_range(size: usize, total: usize, n: usize) -> (usize, usize) {
    let num = size * total;
    (num / n.max(1), num.div_ceil(n.max(1)))
}

/// Distancia al cuadrado de `x` al rango proporcional: 0 si está dentro.
pub fn band(x: usize, size: usize, total: usize, n: usize) -> f64 {
    let (lo, hi) = fair_range(size, total, n);
    let d = if x < lo { lo - x } else { x.saturating_sub(hi) };
    (d * d) as f64
}

fn encode_numeric(
    label: &str,
    cells: &[&str],
    ids: &[String],
    warnings: &mut Vec<String>,
) -> AttrData {
    let mut empty = 0;
    let mut bad = Vec::new();
    let raw: Vec<Option<f64>> = cells
        .iter()
        .zip(ids)
        .map(|(c, id)| {
            if c.trim().is_empty() {
                empty += 1;
                return None;
            }
            let v = parse_number(c);
            if v.is_none() {
                bad.push(format!("'{}' ({id})", c.trim()));
            }
            v
        })
        .collect();
    let known: Vec<f64> = raw.iter().flatten().copied().collect();
    let mean = if known.is_empty() {
        0.0
    } else {
        known.iter().sum::<f64>() / known.len() as f64
    };
    if !bad.is_empty() {
        warnings.push(format!(
            "{label}: valores no numéricos tratados como vacíos: {}",
            preview(&bad)
        ));
    }
    let missing = empty + bad.len();
    if missing > 0 {
        warnings.push(format!(
            "{label}: {missing} {} sin número se {} como el promedio ({mean:.2})",
            if missing == 1 { "celda" } else { "celdas" },
            if missing == 1 { "cuenta" } else { "cuentan" },
        ));
    }
    let centered: Vec<f64> = raw.iter().map(|v| v.map_or(0.0, |x| x - mean)).collect();
    let ss = centered.iter().map(|x| x * x).sum();
    AttrData::Num { centered, mean, ss }
}

fn encode_labels(
    label: &str,
    cells: &[&str],
    separator: Option<&str>,
    warnings: &mut Vec<String>,
) -> AttrData {
    let mut index: HashMap<String, u16> = HashMap::new();
    let mut labels: Vec<String> = Vec::new();
    let mut totals: Vec<usize> = Vec::new();
    let mut empty = 0;
    let values = cells
        .iter()
        .map(|cell| {
            let tokens = match separator {
                Some(sep) => split_list(cell, sep),
                None if cell.trim().is_empty() => Vec::new(),
                None => vec![cell.trim().to_string()],
            };
            if tokens.is_empty() {
                empty += 1;
            }
            let mut v: Vec<u16> = Vec::new();
            for t in tokens {
                let key = t.to_lowercase();
                let idx = *index.entry(key).or_insert_with(|| {
                    labels.push(t.clone());
                    totals.push(0);
                    (labels.len() - 1) as u16
                });
                if !v.contains(&idx) {
                    v.push(idx);
                    totals[idx as usize] += 1;
                }
            }
            v
        })
        .collect();
    if empty > 0 {
        warnings.push(format!(
            "{label}: {empty} {} vacía{} no {}",
            if empty == 1 { "celda" } else { "celdas" },
            if empty == 1 { "" } else { "s" },
            if empty == 1 { "cuenta" } else { "cuentan" },
        ));
    }
    AttrData::Lab {
        values,
        labels,
        totals,
    }
}

/// Busca alumnos por id o por nombre exacto (sin distinguir mayúsculas).
pub struct Lookup {
    by_id: HashMap<String, usize>,
    by_name: HashMap<String, Option<usize>>,
}

impl Lookup {
    pub fn new(ids: &[String], names: &[String]) -> Self {
        let by_id = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.to_lowercase(), i))
            .collect();
        let mut by_name: HashMap<String, Option<usize>> = HashMap::new();
        for (i, name) in names.iter().enumerate() {
            by_name
                .entry(name.trim().to_lowercase())
                .and_modify(|e| *e = None) // nombre repetido: ambiguo
                .or_insert(Some(i));
        }
        Self { by_id, by_name }
    }

    pub fn find(&self, token: &str) -> Option<usize> {
        let key = token.trim().to_lowercase();
        self.by_id
            .get(&key)
            .copied()
            .or_else(|| self.by_name.get(&key).copied().flatten())
    }

    pub fn find_id(&self, id: &str) -> Option<usize> {
        self.by_id.get(&id.trim().to_lowercase()).copied()
    }
}

fn resolve_ids(
    list: &[String],
    lookup: &Lookup,
    what: &str,
    warnings: &mut Vec<String>,
) -> Vec<usize> {
    let mut out = Vec::new();
    for id in list {
        match lookup.find(id) {
            Some(i) if !out.contains(&i) => out.push(i),
            Some(_) => {}
            None => warnings.push(format!(
                "[constraints] {what}: '{id}' no está en la lista de alumnos y se ignora"
            )),
        }
    }
    out
}

pub fn preview(items: &[String]) -> String {
    const MAX: usize = 5;
    let mut s = items
        .iter()
        .take(MAX)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > MAX {
        s.push_str(&format!(" y {} más", items.len() - MAX));
    }
    s
}

struct UnionFind(Vec<usize>);

impl UnionFind {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }
    fn find(&mut self, x: usize) -> usize {
        let mut r = x;
        while self.0[r] != r {
            r = self.0[r];
        }
        let mut x = x;
        while self.0[x] != r {
            let next = self.0[x];
            self.0[x] = r;
            x = next;
        }
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[ra.max(rb)] = ra.min(rb);
        }
    }
}

fn ln_factorials(n: usize) -> Vec<f64> {
    let mut v = vec![0.0; n + 1];
    for i in 1..=n {
        v[i] = v[i - 1] + (i as f64).ln();
    }
    v
}

/// Probabilidades de tener x = 0..=draws éxitos al sacar `draws` de `n` con `succ` éxitos.
fn hypergeom(lnf: &[f64], n: usize, succ: usize, draws: usize) -> Vec<f64> {
    let ln_choose = |a: usize, b: usize| lnf[a] - lnf[b] - lnf[a - b];
    (0..=draws)
        .map(|x| {
            if x > succ || draws - x > n - succ {
                0.0
            } else {
                (ln_choose(succ, x) + ln_choose(n - succ, draws - x) - ln_choose(n, draws)).exp()
            }
        })
        .collect()
}

/// Mínimo de la suma de cuadrados dentro de grupos: valores ordenados en bloques contiguos.
fn similar_ideal(centered: &[f64], sizes: &[usize]) -> f64 {
    let mut sorted = centered.to_vec();
    sorted.sort_by(f64::total_cmp);
    let within = |order: &[usize]| {
        let mut start = 0;
        let mut total = 0.0;
        for &s in order {
            let chunk = &sorted[start..(start + s).min(sorted.len())];
            let sum: f64 = chunk.iter().sum();
            let sumsq: f64 = chunk.iter().map(|x| x * x).sum();
            total += num_term(Goal::Similar, chunk.len(), sum, sumsq);
            start += s;
        }
        total
    };
    let mut asc = sizes.to_vec();
    asc.sort_unstable();
    let desc: Vec<usize> = asc.iter().rev().copied().collect();
    within(&asc).min(within(&desc))
}

/// Buena cota (alcanzable) de los pares mezclados al juntar por valor:
/// llena cada grupo con el valor más abundante y completa con vacíos o el que mejor calce.
fn together_ideal(totals: &[usize], unknown: usize, sizes: &[usize]) -> f64 {
    let mut left: Vec<usize> = totals.to_vec();
    let mut wild = unknown;
    let mut order = sizes.to_vec();
    order.sort_unstable_by(|a, b| b.cmp(a));
    let mut cost = 0.0;
    for cap in order {
        let mut room = cap;
        let mut counts: Vec<u32> = vec![0; left.len()];
        while room > 0 {
            // Calce exacto si existe; si no, el más abundante.
            let pick = left
                .iter()
                .enumerate()
                .filter(|&(_, &r)| r > 0)
                .max_by_key(|&(_, &r)| (r == room, r.min(room), r))
                .map(|(c, _)| c);
            let take_wild = wild.min(room);
            match pick {
                Some(c) if counts.iter().all(|&x| x == 0) || left[c] >= room || take_wild == 0 => {
                    let take = left[c].min(room);
                    left[c] -= take;
                    counts[c] += take as u32;
                    room -= take;
                }
                _ if take_wild > 0 => {
                    wild -= take_wild;
                    room -= take_wild;
                }
                _ => break,
            }
        }
        cost += lab_term(Goal::Together, cap, &counts, totals, 1);
    }
    cost
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_is_zero_inside_fair_range() {
        // 4 de 32 en un grupo de 4: esperado 0.5 → 0 o 1 está bien.
        assert_eq!(band(0, 4, 4, 32), 0.0);
        assert_eq!(band(1, 4, 4, 32), 0.0);
        assert_eq!(band(2, 4, 4, 32), 1.0);
        // Esperado exacto 2: solo 2 está bien.
        assert_eq!(fair_range(4, 16, 32), (2, 2));
        assert_eq!(band(3, 4, 16, 32), 1.0);
    }

    #[test]
    fn hypergeom_sums_to_one() {
        let lnf = ln_factorials(30);
        let p = hypergeom(&lnf, 30, 7, 4);
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        // P(0) = C(23,4)/C(30,4)
        assert!((p[0] - 8855.0 / 27405.0).abs() < 1e-9);
    }

    #[test]
    fn together_ideal_packs_full_groups_first() {
        // A=6, B=5, C=5 en 4 grupos de 4: AAAA BBBB CCCC + {A,A,B,C} = 5 pares mezclados.
        assert_eq!(together_ideal(&[6, 5, 5], 0, &[4, 4, 4, 4]), 5.0);
        // Con 2 vacíos sobra espacio para no mezclar: AAAA BBBB CCCC + {A,A,_,_}... queda B y C sueltos.
        assert!(together_ideal(&[6, 4, 4], 2, &[4, 4, 4, 4]) <= 1.0);
    }

    #[test]
    fn similar_ideal_matches_sorted_chunks() {
        let v = [-3.0, -2.0, 2.0, 3.0];
        assert!((similar_ideal(&v, &[2, 2]) - 1.0).abs() < 1e-9);
    }
}
