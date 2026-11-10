//! Costo incremental: mantiene agregados por grupo para evaluar un cambio
//! (mover o intercambiar alumnos) sin recalcular todo.

use crate::model::{AttrData, Edge, EdgeKind, Model, NONE, lab_term, num_term};

#[derive(Debug, Clone)]
enum Agg {
    Num {
        sum: Vec<f64>,
        sumsq: Vec<f64>,
    },
    /// Conteos por grupo y valor, en una matriz plana `k × c`.
    Lab {
        counts: Vec<u32>,
        c: usize,
    },
}

#[derive(Debug, Clone)]
pub struct State<'m> {
    pub m: &'m Model,
    pub k: usize,
    pub group_of: Vec<u32>,
    pub sizes: Vec<usize>,
    pub members: Vec<Vec<u32>>,
    pos: Vec<usize>,
    aggs: Vec<Agg>,
    /// Costo de cada atributo en cada grupo: `terms[atributo][grupo]`.
    terms: Vec<Vec<f64>>,
    /// Costo crudo por criterio (atributos y preferencias).
    pub raw: Vec<f64>,
    /// Restricciones `apart` incumplidas.
    pub hard: f64,
    stamp: Vec<u32>,
    epoch: u32,
    scratch_edges: Vec<(u32, f64)>,
    scratch_groups: Vec<usize>,
}

impl<'m> State<'m> {
    pub fn new(m: &'m Model, k: usize, group_of: Vec<u32>) -> Self {
        let n = m.n();
        assert_eq!(group_of.len(), n);
        let mut sizes = vec![0; k];
        let mut members = vec![Vec::new(); k];
        let mut pos = vec![usize::MAX; n];
        for (i, &g) in group_of.iter().enumerate() {
            if g != NONE {
                let g = g as usize;
                sizes[g] += 1;
                pos[i] = members[g].len();
                members[g].push(i as u32);
            }
        }
        let aggs = m
            .attrs
            .iter()
            .map(|a| match &a.data {
                AttrData::Num { centered, .. } => {
                    let mut sum = vec![0.0; k];
                    let mut sumsq = vec![0.0; k];
                    for (i, &g) in group_of.iter().enumerate() {
                        if g != NONE {
                            sum[g as usize] += centered[i];
                            sumsq[g as usize] += centered[i] * centered[i];
                        }
                    }
                    Agg::Num { sum, sumsq }
                }
                AttrData::Lab { values, labels, .. } => {
                    let c = labels.len();
                    let mut counts = vec![0; k * c];
                    for (i, &g) in group_of.iter().enumerate() {
                        if g != NONE {
                            for &v in &values[i] {
                                counts[g as usize * c + v as usize] += 1;
                            }
                        }
                    }
                    Agg::Lab { counts, c }
                }
            })
            .collect();
        let mut st = State {
            m,
            k,
            group_of,
            sizes,
            members,
            pos,
            aggs,
            terms: vec![vec![0.0; k]; m.attrs.len()],
            raw: vec![0.0; m.n_crit()],
            hard: 0.0,
            stamp: vec![0; m.edges.len()],
            epoch: 0,
            scratch_edges: Vec::new(),
            scratch_groups: Vec::new(),
        };
        for a in 0..m.attrs.len() {
            for g in 0..k {
                st.terms[a][g] = st.term(a, g);
            }
            st.raw[a] = st.terms[a].iter().sum();
        }
        for e in &m.edges {
            let c = st.edge_cost(e);
            match e.kind {
                EdgeKind::Apart => st.hard += c,
                _ => st.raw[m.attrs.len()] += c,
            }
        }
        st
    }

    /// Costo total ponderado (lo que minimiza el solver).
    pub fn total(&self) -> f64 {
        let soft: f64 = self
            .raw
            .iter()
            .zip(&self.m.calib.crit)
            .map(|(r, c)| r * c.scale)
            .sum();
        soft + self.hard * self.m.hard_penalty
    }

    /// Costo ponderado de un criterio, medido desde su ideal.
    pub fn weighted(&self, crit: usize) -> f64 {
        let c = &self.m.calib.crit[crit];
        (self.raw[crit] - c.ideal) * c.scale
    }

    fn term(&self, a: usize, g: usize) -> f64 {
        let attr = &self.m.attrs[a];
        match (&self.aggs[a], &attr.data) {
            (Agg::Num { sum, sumsq }, _) => num_term(attr.goal, self.sizes[g], sum[g], sumsq[g]),
            (Agg::Lab { counts, c }, AttrData::Lab { totals, .. }) => lab_term(
                attr.goal,
                self.sizes[g],
                &counts[g * c..(g + 1) * c],
                totals,
                self.m.n(),
            ),
            _ => unreachable!("agregado y datos del atributo no coinciden"),
        }
    }

    /// Costo de una arista; no cuenta si alguno de los dos no tiene grupo.
    fn edge_cost(&self, e: &Edge) -> f64 {
        let (ga, gb) = (self.group_of[e.a], self.group_of[e.b]);
        if ga == NONE || gb == NONE {
            0.0
        } else {
            e.cost(ga == gb)
        }
    }

    /// Aplica los cambios `(alumno, grupo nuevo)` (o `NONE` para sacarlo) y devuelve
    /// cuánto cambió el costo total. Para deshacer, aplica los grupos anteriores.
    pub fn apply(&mut self, moves: &[(usize, u32)]) -> f64 {
        let m = self.m;
        let mut delta = -self.hard * m.hard_penalty;

        // Aristas afectadas, contadas una sola vez.
        self.epoch = self.epoch.wrapping_add(1);
        if self.epoch == 0 {
            self.stamp.fill(0);
            self.epoch = 1;
        }
        let mut edges = std::mem::take(&mut self.scratch_edges);
        edges.clear();
        for &(s, _) in moves {
            for &e in &m.adj[s] {
                if self.stamp[e as usize] != self.epoch {
                    self.stamp[e as usize] = self.epoch;
                    edges.push((e, self.edge_cost(&m.edges[e as usize])));
                }
            }
        }

        // Grupos afectados.
        let mut groups = std::mem::take(&mut self.scratch_groups);
        groups.clear();
        for &(s, to) in moves {
            for g in [self.group_of[s], to] {
                if g != NONE && !groups.contains(&(g as usize)) {
                    groups.push(g as usize);
                }
            }
        }

        // Mover y actualizar agregados.
        for &(s, to) in moves {
            let from = self.group_of[s];
            if from == to {
                continue;
            }
            if from != NONE {
                self.detach(s, from as usize);
            }
            if to != NONE {
                self.attach(s, to as usize);
            }
            self.group_of[s] = to;
        }

        for a in 0..m.attrs.len() {
            let scale = m.calib.crit[a].scale;
            for &g in &groups {
                let new = self.term(a, g);
                let diff = new - self.terms[a][g];
                self.terms[a][g] = new;
                self.raw[a] += diff;
                delta += diff * scale;
            }
        }

        let pref = m.attrs.len();
        for &(e, old) in &edges {
            let edge = &m.edges[e as usize];
            let diff = self.edge_cost(edge) - old;
            match edge.kind {
                EdgeKind::Apart => self.hard += diff,
                _ => {
                    self.raw[pref] += diff;
                    delta += diff * m.calib.crit[pref].scale;
                }
            }
        }
        delta += self.hard * m.hard_penalty;

        self.scratch_edges = edges;
        self.scratch_groups = groups;
        delta
    }

    fn detach(&mut self, s: usize, g: usize) {
        let p = self.pos[s];
        self.members[g].swap_remove(p);
        if let Some(&moved) = self.members[g].get(p) {
            self.pos[moved as usize] = p;
        }
        self.pos[s] = usize::MAX;
        self.sizes[g] -= 1;
        self.update_aggs(s, g, -1);
    }

    fn attach(&mut self, s: usize, g: usize) {
        self.pos[s] = self.members[g].len();
        self.members[g].push(s as u32);
        self.sizes[g] += 1;
        self.update_aggs(s, g, 1);
    }

    fn update_aggs(&mut self, s: usize, g: usize, sign: i32) {
        for (agg, attr) in self.aggs.iter_mut().zip(&self.m.attrs) {
            match (agg, &attr.data) {
                (Agg::Num { sum, sumsq }, AttrData::Num { centered, .. }) => {
                    let x = centered[s];
                    sum[g] += sign as f64 * x;
                    sumsq[g] += sign as f64 * x * x;
                }
                (Agg::Lab { counts, c }, AttrData::Lab { values, .. }) => {
                    for &v in &values[s] {
                        let cell = &mut counts[g * *c + v as usize];
                        *cell = cell.wrapping_add_signed(sign);
                    }
                }
                _ => unreachable!("agregado y datos del atributo no coinciden"),
            }
        }
    }

    /// Suma de valores centrados del atributo numérico `a` en el grupo `g`.
    pub fn num_sum(&self, a: usize, g: usize) -> f64 {
        match &self.aggs[a] {
            Agg::Num { sum, .. } => sum[g],
            Agg::Lab { .. } => 0.0,
        }
    }

    /// Conteos por valor del atributo categórico/etiquetas `a` en el grupo `g`.
    pub fn counts(&self, a: usize, g: usize) -> &[u32] {
        match &self.aggs[a] {
            Agg::Lab { counts, c } => &counts[g * c..(g + 1) * c],
            Agg::Num { .. } => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::data::Table;
    use rand::{Rng, SeedableRng, rngs::StdRng};

    const CSV: &str = "id,nombre,g,nota,hab,horario,quiere,evita
a1,Ana,F,5.0,\"Front, Back\",M,a2,
a2,Beto,M,6.1,Back,T,,a3
a3,Caro,F,4.2,Datos,M,a1,
a4,Dani,M,5.5,\"Front, UX\",T,Caro,
a5,Eli,F,,Back,M,,
a6,Fede,M,6.8,Datos,,a5,a1
a7,Gabi,F,3.9,UX,T,,
a8,Hugo,M,5.1,Front,M,a7,
";

    const CFG: &str = r#"
id_column = "id"
name_column = "nombre"
[groups]
count = 3
[[attributes]]
column = "g"
kind = "categorical"
goal = "spread"
[[attributes]]
column = "nota"
kind = "numeric"
goal = "balance"
[[attributes]]
column = "nota"
kind = "numeric"
goal = "similar"
[[attributes]]
column = "hab"
kind = "tags"
goal = "cover"
[[attributes]]
column = "horario"
kind = "categorical"
goal = "together"
[preferences]
prefer_column = "quiere"
avoid_column = "evita"
[constraints]
apart = [["a1", "a6"]]
"#;

    fn model() -> Model {
        let t = Table::parse(CSV, None).unwrap();
        let cfg = Config::parse(CFG).unwrap();
        let mut m = Model::build(&t, &cfg).unwrap();
        m.calibrate(&[3, 3, 2]);
        m
    }

    #[test]
    fn incremental_matches_full_recompute() {
        let m = model();
        let mut rng = StdRng::seed_from_u64(7);
        let start: Vec<u32> = vec![0, 0, 0, 1, 1, 1, 2, 2];
        let mut st = State::new(&m, 3, start);
        let mut total = st.total();
        for _ in 0..500 {
            let i = rng.gen_range(0..8);
            let to = if rng.gen_bool(0.1) {
                NONE
            } else {
                rng.gen_range(0..3)
            };
            total += st.apply(&[(i, to)]);
            let fresh = State::new(&m, 3, st.group_of.clone());
            assert!(
                (fresh.total() - total).abs() < 1e-9,
                "{} vs {}",
                fresh.total(),
                total
            );
            for (a, b) in fresh.raw.iter().zip(&st.raw) {
                assert!((a - b).abs() < 1e-9);
            }
            assert_eq!(fresh.sizes, st.sizes);
        }
    }

    #[test]
    fn swap_then_revert_restores_cost() {
        let m = model();
        let mut st = State::new(&m, 3, vec![0, 0, 0, 1, 1, 1, 2, 2]);
        let before = st.total();
        let d = st.apply(&[(0, 1), (3, 0)]);
        let back = st.apply(&[(0, 0), (3, 1)]);
        assert!((d + back).abs() < 1e-12);
        assert!((st.total() - before).abs() < 1e-12);
    }

    #[test]
    fn apart_violation_is_hard() {
        let m = model();
        let st = State::new(&m, 3, vec![0, 1, 1, 1, 2, 0, 2, 2]);
        assert_eq!(st.hard, 1.0);
        assert!(st.total() >= m.hard_penalty);
    }
}
