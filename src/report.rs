//! Reportes de calidad en texto y JSON.

use crate::config::{Goal, Kind};
use crate::model::{AttrData, EdgeKind, NONE};
use crate::state::State;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Report {
    pub sizes: String,
    pub criteria: Vec<CritReport>,
    pub constraints: ConstraintsReport,
    pub groups: Vec<GroupReport>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CritReport {
    pub criterion: String,
    pub goal: String,
    pub weight: f64,
    /// 1 = ideal, 0 = como un reparto al azar.
    pub quality: f64,
    pub raw: f64,
    pub ideal: f64,
    pub random: f64,
    pub detail: String,
}

#[derive(Debug, Default, Serialize)]
pub struct ConstraintsReport {
    pub together: usize,
    pub apart: usize,
    pub violations: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct GroupReport {
    pub group: String,
    pub members: Vec<Member>,
    pub summary: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct Member {
    pub id: String,
    pub name: String,
}

/// Arma el reporte de una asignación. `bounds` = tamaños permitidos (si se conocen).
pub fn evaluate(st: &State, labels: &[String], bounds: Option<(usize, usize)>) -> Report {
    let m = st.m;
    let k = st.k;
    let assigned = st.group_of.iter().filter(|&&g| g != NONE).count();
    let mut criteria = Vec::new();
    for (a, attr) in m.attrs.iter().enumerate() {
        let c = &m.calib.crit[a];
        let raw = st.raw[a];
        criteria.push(CritReport {
            criterion: attr.label.clone(),
            goal: attr.goal.label_es().to_string(),
            weight: attr.weight,
            quality: c.quality(raw),
            raw,
            ideal: c.ideal,
            random: c.random,
            detail: crit_detail(st, a, assigned),
        });
    }
    if let (Some(p), Some(w)) = (m.pref_crit(), m.pref_weight) {
        let c = &m.calib.crit[p];
        criteria.push(CritReport {
            criterion: "preferencias".into(),
            goal: "cumplir".into(),
            weight: w,
            quality: c.quality(st.raw[p]),
            raw: st.raw[p],
            ideal: c.ideal,
            random: c.random,
            detail: pref_detail(st),
        });
    }

    let groups = (0..k)
        .map(|g| {
            let mut ids: Vec<usize> = st.members[g].iter().map(|&i| i as usize).collect();
            ids.sort_unstable();
            GroupReport {
                group: labels[g].clone(),
                members: ids
                    .iter()
                    .map(|&i| Member {
                        id: m.ids[i].clone(),
                        name: m.names[i].clone(),
                    })
                    .collect(),
                summary: (0..m.attrs.len())
                    .map(|a| group_summary(st, a, g))
                    .collect(),
            }
        })
        .collect();

    Report {
        sizes: sizes_summary(&st.sizes),
        criteria,
        constraints: constraints(st, labels, bounds),
        groups,
        warnings: Vec::new(),
    }
}

fn sizes_summary(sizes: &[usize]) -> String {
    let mut distinct: Vec<usize> = sizes.to_vec();
    distinct.sort_unstable_by(|a, b| b.cmp(a));
    distinct.dedup();
    let k = sizes.len();
    let grupos = if k == 1 { "grupo" } else { "grupos" };
    if distinct.len() == 1 {
        return format!("{k} {grupos} de {} alumnos", distinct[0]);
    }
    let parts: Vec<String> = distinct
        .iter()
        .map(|&s| format!("{} de {s}", sizes.iter().filter(|&&x| x == s).count()))
        .collect();
    format!("{k} {grupos}: {} alumnos", join_y(&parts))
}

fn join_y(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} y {last}", init.join(", ")),
    }
}

fn crit_detail(st: &State, a: usize, assigned: usize) -> String {
    let m = st.m;
    let attr = &m.attrs[a];
    let c = &m.calib.crit[a];
    let k = st.k;
    let used: Vec<usize> = (0..k).filter(|&g| st.sizes[g] > 0).collect();
    match (&attr.data, attr.goal) {
        (AttrData::Num { mean, .. }, Goal::Balance) => {
            let means: Vec<f64> = used
                .iter()
                .map(|&g| mean + st.num_sum(a, g) / st.sizes[g] as f64)
                .collect();
            let lo = means.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = means.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            format!("promedios de grupo entre {lo:.2} y {hi:.2} (global {mean:.2})")
        }
        (AttrData::Num { .. }, _) => {
            let sd = |ss: f64| (ss.max(0.0) / assigned.max(1) as f64).sqrt();
            format!(
                "dispersión dentro de los grupos {:.2} (mínimo ≈ {:.2}, al azar ≈ {:.2})",
                sd(st.raw[a]),
                sd(c.ideal),
                sd(c.random)
            )
        }
        (AttrData::Lab { labels, totals, .. }, Goal::Spread) => {
            let n = m.n();
            let out: usize = used
                .iter()
                .map(|&g| {
                    st.counts(a, g)
                        .iter()
                        .zip(totals)
                        .filter(|&(&x, &t)| crate::model::band(x as usize, st.sizes[g], t, n) > 0.0)
                        .count()
                })
                .sum();
            if attr.kind == Kind::Categorical && labels.len() <= 4 {
                let ranges: Vec<String> = labels
                    .iter()
                    .enumerate()
                    .map(|(v, l)| {
                        let xs = used.iter().map(|&g| st.counts(a, g)[v]);
                        let (lo, hi) = (xs.clone().min().unwrap_or(0), xs.max().unwrap_or(0));
                        if lo == hi {
                            format!("{l} {lo}")
                        } else {
                            format!("{l} {lo}–{hi}")
                        }
                    })
                    .collect();
                let mut s = format!("{} por grupo", ranges.join(" · "));
                if out > 0 {
                    s.push_str(&format!(" ({out} fuera de proporción)"));
                }
                s
            } else {
                format!(
                    "{out} de {} conteos fuera de proporción",
                    used.len() * labels.len()
                )
            }
        }
        (AttrData::Lab { labels, .. }, Goal::Cover) => {
            if attr.kind == Kind::Categorical {
                let c_vals = labels.len() as f64;
                let per = |abs: f64| c_vals - abs / used.len().max(1) as f64;
                format!(
                    "{:.1} valores distintos por grupo (ideal {:.1}, al azar ≈ {:.1})",
                    per(st.raw[a]),
                    per(c.ideal),
                    per(c.random)
                )
            } else {
                let abs = st.raw[a].round() as usize;
                format!(
                    "{abs} ausencia{} (mínimo posible {}, al azar ≈ {:.1})",
                    plural(abs),
                    c.ideal.round(),
                    c.random
                )
            }
        }
        (AttrData::Lab { .. }, _) => {
            let mut pure = 0;
            let mut off = 0;
            for &g in &used {
                let counts = st.counts(a, g);
                let known: u32 = counts.iter().sum();
                let top = counts.iter().copied().max().unwrap_or(0);
                if known == top {
                    pure += 1;
                }
                off += known - top;
            }
            format!(
                "{pure} de {} grupos con un solo valor · {off} alumno{} distinto{} a la mayoría de su grupo",
                used.len(),
                plural(off as usize),
                plural(off as usize)
            )
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

fn pref_detail(st: &State) -> String {
    let m = st.m;
    let same = |a: usize, b: usize| {
        let (ga, gb) = (st.group_of[a], st.group_of[b]);
        (ga != NONE && gb != NONE).then_some(ga == gb)
    };
    let mut total = 0;
    let mut met = 0;
    let mut chooser = vec![(0, 0); m.n()];
    let mut avoid_total = 0;
    let mut avoid_bad = 0;
    for e in &m.edges {
        let Some(together) = same(e.a, e.b) else {
            continue;
        };
        match e.kind {
            EdgeKind::Prefer => {
                total += 1;
                chooser[e.a].0 += 1;
                if together {
                    met += 1;
                    chooser[e.a].1 += 1;
                }
            }
            EdgeKind::Avoid => {
                avoid_total += 1;
                avoid_bad += usize::from(together);
            }
            EdgeKind::Apart => {}
        }
    }
    let lonely = chooser
        .iter()
        .filter(|(asked, got)| *asked > 0 && *got == 0)
        .count();
    let mut parts = Vec::new();
    if total > 0 {
        parts.push(format!("{met} de {total} elecciones cumplidas"));
        parts.push(format!("{lonely} sin ninguno de sus elegidos"));
    }
    if avoid_total > 0 {
        parts.push(format!(
            "{avoid_bad} de {avoid_total} pares a evitar quedaron juntos"
        ));
    }
    if parts.is_empty() {
        "sin preferencias declaradas".into()
    } else {
        parts.join(" · ")
    }
}

fn constraints(st: &State, labels: &[String], bounds: Option<(usize, usize)>) -> ConstraintsReport {
    let m = st.m;
    let mut r = ConstraintsReport::default();
    for b in m.blocks.iter().filter(|b| b.len() > 1) {
        r.together += 1;
        let mut gs: Vec<u32> = b
            .iter()
            .map(|&i| st.group_of[i])
            .filter(|&g| g != NONE)
            .collect();
        gs.sort_unstable();
        gs.dedup();
        if gs.len() > 1 {
            let names: Vec<&str> = b.iter().map(|&i| m.names[i].as_str()).collect();
            r.violations.push(format!(
                "deben ir juntos y quedaron separados: {}",
                names.join(", ")
            ));
        }
    }
    for e in m.edges.iter().filter(|e| e.kind == EdgeKind::Apart) {
        r.apart += 1;
        let g = st.group_of[e.a];
        if g != NONE && g == st.group_of[e.b] {
            r.violations.push(format!(
                "deben ir separados y quedaron en el grupo {}: {} y {}",
                labels[g as usize], m.names[e.a], m.names[e.b]
            ));
        }
    }
    if let Some((min, max)) = bounds {
        for (g, &s) in st.sizes.iter().enumerate() {
            if s < min || s > max {
                r.violations.push(format!(
                    "el grupo {} tiene {s} alumnos (permitido: {min}–{max})",
                    labels[g]
                ));
            }
        }
    }
    r
}

fn fmt_num(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{}", x.round())
    } else {
        format!("{x:.2}")
    }
}

/// Resumen corto de un atributo en un grupo, para explicar cómo quedó.
fn group_summary(st: &State, a: usize, g: usize) -> String {
    let attr = &st.m.attrs[a];
    let label = &attr.label;
    if st.sizes[g] == 0 {
        return format!("{label} —");
    }
    match &attr.data {
        AttrData::Num { mean, centered, .. } => {
            if attr.goal == Goal::Balance {
                format!(
                    "{label} {:.2}",
                    mean + st.num_sum(a, g) / st.sizes[g] as f64
                )
            } else {
                let vals: Vec<f64> = st.members[g]
                    .iter()
                    .map(|&i| mean + centered[i as usize])
                    .collect();
                let lo = vals.iter().copied().fold(f64::INFINITY, f64::min);
                let hi = vals.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                if (hi - lo).abs() < 1e-9 {
                    format!("{label} {}", fmt_num(lo))
                } else {
                    format!("{label} {}–{}", fmt_num(lo), fmt_num(hi))
                }
            }
        }
        AttrData::Lab { labels, .. } => {
            let counts = st.counts(a, g);
            if attr.goal == Goal::Cover && attr.kind == Kind::Tags {
                let missing: Vec<&str> = counts
                    .iter()
                    .zip(labels)
                    .filter(|(x, _)| **x == 0)
                    .map(|(_, l)| l.as_str())
                    .collect();
                return if missing.is_empty() {
                    format!("{label}: todas")
                } else {
                    format!("{label}: falta {}", missing.join(", "))
                };
            }
            let mut present: Vec<(u32, usize)> = counts
                .iter()
                .enumerate()
                .filter(|(_, x)| **x > 0)
                .map(|(v, &x)| (x, v))
                .collect();
            present.sort_by_key(|&(x, v)| (std::cmp::Reverse(x), v));
            if present.is_empty() {
                return format!("{label} —");
            }
            let items: Vec<String> = present
                .iter()
                .map(|&(x, v)| {
                    if x > 1 {
                        format!("{} ×{x}", labels[v])
                    } else {
                        labels[v].clone()
                    }
                })
                .collect();
            format!("{label} {}", items.join(", "))
        }
    }
}

pub fn render_text(r: &Report) -> String {
    let mut out = String::new();
    outln!(out, "{}", r.sizes);
    outln!(out);
    outln!(
        out,
        "Calidad (100 % = ideal · 0 % = como un reparto al azar)"
    );
    let w1 = r
        .criteria
        .iter()
        .map(|c| c.criterion.chars().count())
        .max()
        .unwrap_or(0);
    let w2 = r
        .criteria
        .iter()
        .map(|c| c.goal.chars().count())
        .max()
        .unwrap_or(0);
    for c in &r.criteria {
        outln!(
            out,
            "  {:<w1$}  {:<w2$}  {:>4} %   {}",
            c.criterion,
            c.goal,
            (c.quality * 100.0).round() as i64,
            c.detail
        );
    }
    let cons = &r.constraints;
    if cons.together + cons.apart > 0 || !cons.violations.is_empty() {
        let mut kinds = Vec::new();
        if cons.together > 0 {
            kinds.push(format!("{} juntos", cons.together));
        }
        if cons.apart > 0 {
            kinds.push(format!("{} separados", cons.apart));
        }
        let kinds = if kinds.is_empty() {
            String::new()
        } else {
            format!(" ({})", kinds.join(" · "))
        };
        if cons.violations.is_empty() {
            outln!(out, "Restricciones: todas cumplidas{kinds}");
        } else {
            outln!(
                out,
                "Restricciones: {} sin cumplir{kinds}",
                cons.violations.len()
            );
            for v in &cons.violations {
                outln!(out, "  ✗ {v}");
            }
        }
    }
    outln!(out);
    for g in &r.groups {
        let names: Vec<&str> = g.members.iter().map(|m| m.name.as_str()).collect();
        outln!(
            out,
            "Grupo {} ({}): {}",
            g.group,
            g.members.len(),
            names.join(", ")
        );
        for line in wrap(&g.summary, " · ", 96) {
            outln!(out, "   {line}");
        }
    }
    out
}

fn wrap(items: &[String], sep: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for it in items {
        if !cur.is_empty() && cur.chars().count() + sep.chars().count() + it.chars().count() > width
        {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push_str(sep);
        }
        cur.push_str(it);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_text() {
        assert_eq!(
            sizes_summary(&[4, 4, 3, 4]),
            "4 grupos: 3 de 4 y 1 de 3 alumnos"
        );
        assert_eq!(sizes_summary(&[4, 4]), "2 grupos de 4 alumnos");
    }

    #[test]
    fn wraps_long_summaries() {
        let items: Vec<String> = ["aaaa", "bbbb", "cccc"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(wrap(&items, " · ", 11), vec!["aaaa · bbbb", "cccc"]);
    }
}
