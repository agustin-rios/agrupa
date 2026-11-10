//! Sugerencia de grupo para alumnos nuevos, sin mover a nadie.
//!
//! Cada alumno nuevo (o bloque `together` de nuevos) se prueba en cada grupo; se
//! recomienda el que menos sube el costo total (Δ), entre los grupos que no están llenos.

use crate::config::{Goal, Kind};
use crate::model::{AttrData, EdgeKind, NONE, fair_range};
use crate::state::State;

#[derive(Debug, Clone)]
pub struct Candidate {
    pub group: usize,
    pub size: usize,
    pub delta: f64,
    /// Δ ponderado por criterio (atributos y preferencias).
    pub by_crit: Vec<f64>,
    pub hard: bool,
    pub eligible: bool,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub unit: Vec<usize>,
    /// Candidatos ordenados: elegibles primero, luego por Δ.
    pub candidates: Vec<Candidate>,
    pub reasons: Vec<Reason>,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Reason {
    pub criterion: String,
    pub delta: f64,
    pub text: String,
}

impl Suggestion {
    pub fn chosen(&self) -> &Candidate {
        &self.candidates[0]
    }
}

/// Alumnos nuevos que se ubican juntos (un alumno suelto o un bloque `together`).
struct Unit {
    members: Vec<usize>,
    /// Grupos de compañeros de bloque que ya tenían grupo (si hay, el nuevo debe ir ahí).
    anchors: Vec<usize>,
}

impl Unit {
    fn allows(&self, g: usize) -> bool {
        self.anchors.is_empty() || self.anchors.contains(&g)
    }
}

/// Ubica a todos los alumnos sin grupo y deja `st` actualizado. Nadie que ya tenía
/// grupo se mueve. `max_size`: tope por grupo; si no hay, se llenan primero los más chicos.
///
/// 1. Cada nuevo va al grupo con menor Δ, en orden.
/// 2. Se re-ubica a los nuevos (moviéndolos o intercambiándolos entre ellos) mientras mejore.
/// 3. La explicación de cada uno se calcula con los demás nuevos ya en su lugar.
pub fn suggest_all(st: &mut State, labels: &[String], max_size: Option<usize>) -> Vec<Suggestion> {
    let m = st.m;
    let mut done = vec![false; m.n()];
    let mut units = Vec::new();
    for i in 0..m.n() {
        if st.group_of[i] != NONE || done[i] {
            continue;
        }
        let block = &m.blocks[m.block_of[i]];
        let members: Vec<usize> = block
            .iter()
            .copied()
            .filter(|&s| st.group_of[s] == NONE)
            .collect();
        for &s in &members {
            done[s] = true;
        }
        let mut anchors: Vec<usize> = block
            .iter()
            .filter(|&&s| st.group_of[s] != NONE)
            .map(|&s| st.group_of[s] as usize)
            .collect();
        anchors.sort_unstable();
        anchors.dedup();
        units.push(Unit { members, anchors });
    }

    for u in &units {
        let (cands, _) = candidates(st, u, max_size);
        place(st, u, cands[0].group as u32);
    }

    for _ in 0..20 {
        let mut improved = false;
        for u in &units {
            let cur = st.group_of[u.members[0]] as usize;
            place(st, u, NONE);
            let (cands, _) = candidates(st, u, max_size);
            let here = cands
                .iter()
                .find(|c| c.group == cur)
                .map_or(f64::INFINITY, |c| c.delta);
            let best = &cands[0];
            if best.group != cur && best.delta < here - 1e-9 {
                improved = true;
                place(st, u, best.group as u32);
            } else {
                place(st, u, cur as u32);
            }
        }
        for a in 0..units.len() {
            for b in a + 1..units.len() {
                let (ua, ub) = (&units[a], &units[b]);
                let ga = st.group_of[ua.members[0]];
                let gb = st.group_of[ub.members[0]];
                if ga == gb
                    || ua.members.len() != ub.members.len()
                    || !ua.allows(gb as usize)
                    || !ub.allows(ga as usize)
                {
                    continue;
                }
                let mut moves: Vec<(usize, u32)> = ua.members.iter().map(|&s| (s, gb)).collect();
                moves.extend(ub.members.iter().map(|&s| (s, ga)));
                if st.apply(&moves) < -1e-9 {
                    improved = true;
                } else {
                    let mut undo: Vec<(usize, u32)> = ua.members.iter().map(|&s| (s, ga)).collect();
                    undo.extend(ub.members.iter().map(|&s| (s, gb)));
                    st.apply(&undo);
                }
            }
        }
        if !improved {
            break;
        }
    }

    units
        .iter()
        .map(|u| {
            let cur = st.group_of[u.members[0]] as usize;
            place(st, u, NONE);
            let (mut cands, mut note) = candidates(st, u, max_size);
            if let Some(p) = cands.iter().position(|c| c.group == cur) {
                let c = cands.remove(p);
                cands.insert(0, c);
            }
            if u.anchors.len() == 1 {
                note.get_or_insert_with(|| {
                    format!(
                        "debe ir junto a compañeros que ya están en el grupo {}",
                        labels[u.anchors[0]]
                    )
                });
            }
            let reasons = explain(st, &u.members, &cands[0], labels);
            place(st, u, cur as u32);
            Suggestion {
                unit: u.members.clone(),
                candidates: cands,
                reasons,
                note,
            }
        })
        .collect()
}

/// Mueve la unidad al grupo `g` (o la saca, con `NONE`).
fn place(st: &mut State, u: &Unit, g: u32) {
    let moves: Vec<(usize, u32)> = u.members.iter().map(|&s| (s, g)).collect();
    st.apply(&moves);
}

/// Evalúa sumar la unidad (que no debe tener grupo) a cada grupo. Devuelve los
/// candidatos ordenados (con cupo primero, luego por Δ) y un aviso si todos están llenos.
fn candidates(
    st: &mut State,
    u: &Unit,
    max_size: Option<usize>,
) -> (Vec<Candidate>, Option<String>) {
    let m = st.m;
    let s = u.members.len();
    let smallest = st.sizes.iter().copied().min().unwrap_or(0);
    let limit = max_size.unwrap_or(smallest + s);
    let mut cands = Vec::with_capacity(st.k);
    for h in 0..st.k {
        let before = st.raw.clone();
        let hard_before = st.hard;
        place(st, u, h as u32);
        let delta_raw: Vec<f64> = st.raw.iter().zip(&before).map(|(a, b)| a - b).collect();
        let hard = st.hard > hard_before;
        place(st, u, NONE);
        let by_crit: Vec<f64> = delta_raw
            .iter()
            .zip(&m.calib.crit)
            .map(|(d, c)| d * c.scale)
            .collect();
        let delta = by_crit.iter().sum::<f64>() + if hard { m.hard_penalty } else { 0.0 };
        let fits = st.sizes[h] + s <= limit;
        cands.push(Candidate {
            group: h,
            size: st.sizes[h],
            delta,
            by_crit,
            hard,
            eligible: fits && u.allows(h),
        });
    }
    let mut note = None;
    if !cands.iter().any(|c| c.eligible) {
        for c in &mut cands {
            c.eligible = u.allows(c.group);
        }
        note = Some(match max_size {
            Some(max) => format!(
                "todos los grupos posibles tienen {max} alumnos (max_size); se elige igual el de menor Δ"
            ),
            None => "todos los grupos posibles están llenos; se elige igual el de menor Δ".into(),
        });
    }
    cands.sort_by(|a, b| {
        b.eligible
            .cmp(&a.eligible)
            .then(a.delta.total_cmp(&b.delta))
            .then(a.size.cmp(&b.size))
            .then(a.group.cmp(&b.group))
    });
    (cands, note)
}

/// Explica, criterio por criterio, qué cambia al sumar `unit` al grupo elegido.
/// Se calcula antes de aplicar el cambio.
fn explain(st: &State, unit: &[usize], c: &Candidate, labels: &[String]) -> Vec<Reason> {
    let m = st.m;
    let g = c.group;
    let size = st.sizes[g];
    let mut reasons = Vec::new();
    for (a, attr) in m.attrs.iter().enumerate() {
        let d = c.by_crit[a];
        let text = match (&attr.data, attr.goal) {
            (AttrData::Num { mean, centered, .. }, Goal::Balance) => {
                let sum = st.num_sum(a, g);
                let add: f64 = unit.iter().map(|&s| centered[s]).sum();
                let before = if size > 0 {
                    format!("{:.2}", mean + sum / size as f64)
                } else {
                    "—".into()
                };
                let after = mean + (sum + add) / (size + unit.len()) as f64;
                format!("promedio del grupo {before} → {after:.2} (global {mean:.2})")
            }
            (AttrData::Num { mean, centered, .. }, _) => {
                let vals: Vec<f64> = st.members[g]
                    .iter()
                    .map(|&i| mean + centered[i as usize])
                    .collect();
                let own: Vec<String> = unit.iter().map(|&s| fmt_num(mean + centered[s])).collect();
                if vals.is_empty() {
                    format!("{} (grupo vacío)", own.join(", "))
                } else {
                    let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
                    let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    format!(
                        "{} en un grupo con {}–{}",
                        own.join(", "),
                        fmt_num(lo),
                        fmt_num(hi)
                    )
                }
            }
            (
                AttrData::Lab {
                    values,
                    labels: names,
                    totals,
                },
                goal,
            ) => {
                let counts = st.counts(a, g);
                let mut own: Vec<u16> = unit
                    .iter()
                    .flat_map(|&s| values[s].iter().copied())
                    .collect();
                own.sort_unstable();
                own.dedup();
                if own.is_empty() {
                    "sin dato".into()
                } else {
                    match goal {
                        Goal::Spread => own
                            .iter()
                            .map(|&v| {
                                let add = unit.iter().filter(|&&s| values[s].contains(&v)).count();
                                let x = counts[v as usize] as usize;
                                let (lo, hi) =
                                    fair_range(size + unit.len(), totals[v as usize], m.n());
                                let range = if lo == hi {
                                    format!("{lo}")
                                } else {
                                    format!("{lo}–{hi}")
                                };
                                format!(
                                    "{}: {x} → {} (proporcional {range})",
                                    names[v as usize],
                                    x + add
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("; "),
                        Goal::Cover => {
                            let new: Vec<String> = own
                                .iter()
                                .filter(|&&v| counts[v as usize] == 0)
                                .map(|&v| names[v as usize].clone())
                                .collect();
                            let old: Vec<String> = own
                                .iter()
                                .filter(|&&v| counts[v as usize] > 0)
                                .map(|&v| names[v as usize].clone())
                                .collect();
                            match (new.len(), attr.kind) {
                                (0, _) => format!(
                                    "{} ya está{} en el grupo",
                                    y_list(&old),
                                    if old.len() > 1 { "n" } else { "" }
                                ),
                                (1, Kind::Tags) => format!("aporta {}, que faltaba", new[0]),
                                (_, Kind::Tags) => format!("aporta {}, que faltaban", y_list(&new)),
                                (1, _) => format!("aporta {}, que no estaba", new[0]),
                                _ => format!("aporta {}, que no estaban", y_list(&new)),
                            }
                        }
                        _ => {
                            let own_names: Vec<String> =
                                own.iter().map(|&v| names[v as usize].clone()).collect();
                            let mut present: Vec<(u32, usize)> = counts
                                .iter()
                                .enumerate()
                                .filter(|(_, x)| **x > 0)
                                .map(|(v, &x)| (x, v))
                                .collect();
                            present.sort_by_key(|&(x, v)| (std::cmp::Reverse(x), v));
                            let group: Vec<String> = present
                                .iter()
                                .map(|&(x, v)| {
                                    if x > 1 {
                                        format!("{} ×{x}", names[v])
                                    } else {
                                        names[v].clone()
                                    }
                                })
                                .collect();
                            if group.is_empty() {
                                format!("{}; el grupo no tiene datos", own_names.join(", "))
                            } else {
                                format!("{}; el grupo: {}", own_names.join(", "), group.join(", "))
                            }
                        }
                    }
                }
            }
        };
        if d.abs() > 1e-9 || attr.goal == Goal::Cover {
            reasons.push(Reason {
                criterion: attr.label.clone(),
                delta: d,
                text,
            });
        }
    }

    if let Some(p) = m.pref_crit() {
        let mut parts = Vec::new();
        for &u in unit {
            for &e in &m.adj[u] {
                let e = &m.edges[e as usize];
                let (other, mine) = if e.a == u { (e.b, true) } else { (e.a, false) };
                let og = st.group_of[other];
                if og == NONE && !unit.contains(&other) {
                    continue;
                }
                let here = og == g as u32 || unit.contains(&other);
                let who = &m.names[other];
                let text = match (e.kind, mine, here) {
                    (EdgeKind::Prefer, true, true) => {
                        format!("eligió a {who}, que está en el grupo")
                    }
                    (EdgeKind::Prefer, true, false) => {
                        format!(
                            "eligió a {who}, que está en el grupo {}",
                            labels[og as usize]
                        )
                    }
                    (EdgeKind::Prefer, false, true) => format!("{who} eligió a {}", m.names[u]),
                    (EdgeKind::Avoid, true, true) => {
                        format!("prefiere evitar a {who}, que está en el grupo")
                    }
                    (EdgeKind::Avoid, false, true) => {
                        format!("{who} prefiere evitar a {}", m.names[u])
                    }
                    _ => continue,
                };
                parts.push(text);
            }
        }
        if !parts.is_empty() {
            reasons.push(Reason {
                criterion: "preferencias".into(),
                delta: c.by_crit[p],
                text: parts.join("; "),
            });
        }
    }

    for &u in unit {
        for &e in &m.adj[u] {
            let e = &m.edges[e as usize];
            let other = if e.a == u { e.b } else { e.a };
            if e.kind == EdgeKind::Apart && st.group_of[other] == g as u32 {
                reasons.push(Reason {
                    criterion: "restricción".into(),
                    delta: f64::INFINITY,
                    text: format!("debe ir separado de {}", m.names[other]),
                });
            }
        }
    }

    reasons.sort_by(|a, b| b.delta.abs().total_cmp(&a.delta.abs()));
    reasons
}

fn y_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} y {last}", init.join(", ")),
    }
}

fn fmt_num(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{}", x.round())
    } else {
        format!("{x:.2}")
    }
}

pub fn render_text(st: &State, sugs: &[Suggestion], labels: &[String]) -> String {
    let mut out = String::new();
    let m = st.m;
    outln!(
        out,
        "Sugerencias (Δ = cambio en el costo total; más bajo = mejor)"
    );
    for s in sugs {
        let who: Vec<String> = s
            .unit
            .iter()
            .map(|&i| format!("{} ({})", m.names[i], m.ids[i]))
            .collect();
        let best = s.chosen();
        outln!(out);
        outln!(out, "{} → grupo {}", who.join(" + "), labels[best.group]);
        if let Some(note) = &s.note {
            outln!(out, "   aviso: {note}");
        }
        let add = s.unit.len();
        let wg = labels
            .iter()
            .map(|l| l.chars().count())
            .max()
            .unwrap_or(1)
            .max(5);
        outln!(out, "     {:<wg$}  tamaño       Δ", "grupo");
        let shown: Vec<&Candidate> = s.candidates.iter().filter(|c| c.eligible).take(3).collect();
        for (r, c) in shown.iter().enumerate() {
            let delta = if c.hard {
                "restricción".to_string()
            } else {
                format!("{:+.3}", c.delta)
            };
            outln!(
                out,
                "   {} {:<wg$}  {:>2} → {:<2} {:>8}",
                if r == 0 { "★" } else { " " },
                labels[c.group],
                c.size,
                c.size + add,
                delta
            );
        }
        let rest = s
            .candidates
            .iter()
            .filter(|c| c.eligible)
            .count()
            .saturating_sub(shown.len());
        if rest > 0 {
            outln!(
                out,
                "     … y {rest} grupo{} más",
                if rest == 1 { "" } else { "s" }
            );
        }
        let bigger: Vec<String> = s
            .candidates
            .iter()
            .filter(|c| !c.eligible && !c.hard)
            .take(3)
            .map(|c| {
                format!(
                    "{} ({} → {}, {:+.3})",
                    labels[c.group],
                    c.size,
                    c.size + add,
                    c.delta
                )
            })
            .collect();
        if !bigger.is_empty() {
            outln!(out, "     agrandando un grupo: {}", bigger.join(" · "));
        }
        if !s.reasons.is_empty() {
            outln!(out, "   Por qué el grupo {}:", labels[best.group]);
            let wc = s
                .reasons
                .iter()
                .map(|r| r.criterion.chars().count())
                .max()
                .unwrap_or(0);
            for r in &s.reasons {
                let (tag, delta) = if r.delta.is_infinite() {
                    ("incumple", "".to_string())
                } else if r.delta < -1e-9 {
                    ("mejora ", format!("{:+.3}", r.delta))
                } else if r.delta > 1e-9 {
                    ("empeora", format!("{:+.3}", r.delta))
                } else {
                    ("igual  ", format!("{:+.3}", 0.0))
                };
                outln!(
                    out,
                    "     {tag}  {:<wc$}  {delta:>7}  {}",
                    r.criterion,
                    r.text
                );
            }
        }
    }
    out
}
