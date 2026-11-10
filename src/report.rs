//! Reporte de calidad de una asignación, en texto y JSON.
//!
//! Cada criterio escribe su propia línea y su resumen por grupo; aquí solo se
//! arman las piezas y se revisan las restricciones duras.

use crate::criteria::Context;
use crate::problem::Problem;
use crate::state::State;
use crate::text::wrap;
use serde::Serialize;

/// Ancho máximo de las líneas de resumen de cada grupo.
const SUMMARY_WIDTH: usize = 96;

#[derive(Debug, Serialize)]
pub struct Report {
    pub sizes: String,
    pub criteria: Vec<CriterionLine>,
    pub constraints: ConstraintsReport,
    pub groups: Vec<GroupReport>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CriterionLine {
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
    /// Cantidad de bloques `together` y de pares `apart`.
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

/// Arma el reporte. `size_bounds` = tamaños permitidos, si se conocen.
pub fn build(
    problem: &Problem,
    state: &State,
    group_labels: &[String],
    size_bounds: Option<(usize, usize)>,
) -> Report {
    let ctx = Context {
        partition: state.partition(),
        roster: &problem.roster,
        group_labels,
    };
    Report {
        sizes: sizes_summary(&ctx.partition.sizes()),
        criteria: criterion_lines(state, &ctx),
        constraints: check_constraints(problem, &ctx, size_bounds),
        groups: group_reports(state, &ctx),
        warnings: problem.warnings.clone(),
    }
}

fn criterion_lines(state: &State, ctx: &Context) -> Vec<CriterionLine> {
    state
        .objective()
        .terms()
        .iter()
        .enumerate()
        .filter(|(_, term)| !term.is_hard())
        .map(|(index, term)| {
            let tracker = state.tracker(index);
            let meta = term.criterion.meta();
            let cost = tracker.cost();
            CriterionLine {
                criterion: meta.label.clone(),
                goal: meta.goal.to_string(),
                weight: meta.weight,
                quality: term.reference.quality(cost),
                raw: cost,
                ideal: term.reference.ideal,
                random: term.reference.random,
                detail: tracker.describe(ctx, &term.reference),
            }
        })
        .collect()
}

fn group_reports(state: &State, ctx: &Context) -> Vec<GroupReport> {
    let soft_terms: Vec<usize> = state
        .objective()
        .terms()
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.is_hard())
        .map(|(i, _)| i)
        .collect();
    (0..ctx.partition.group_count())
        .map(|group| {
            let mut members = ctx.partition.members(group).to_vec();
            members.sort_unstable();
            GroupReport {
                group: ctx.group_labels[group].clone(),
                members: members
                    .iter()
                    .map(|&s| Member {
                        id: ctx.roster.id(s).to_string(),
                        name: ctx.roster.name(s).to_string(),
                    })
                    .collect(),
                summary: soft_terms
                    .iter()
                    .filter_map(|&t| state.tracker(t).describe_group(ctx, group))
                    .collect(),
            }
        })
        .collect()
}

/// Revisa `together`, `apart` y tamaños; útil sobre todo con asignaciones hechas a mano.
fn check_constraints(
    problem: &Problem,
    ctx: &Context,
    size_bounds: Option<(usize, usize)>,
) -> ConstraintsReport {
    let partition = ctx.partition;
    let name = |s: usize| ctx.roster.name(s);
    let mut report = ConstraintsReport::default();

    for block in problem
        .relations
        .blocks
        .all()
        .iter()
        .filter(|b| b.len() > 1)
    {
        report.together += 1;
        let mut groups: Vec<usize> = block
            .iter()
            .filter_map(|&s| partition.group_of(s))
            .collect();
        groups.sort_unstable();
        groups.dedup();
        if groups.len() > 1 {
            let names: Vec<&str> = block.iter().map(|&s| name(s)).collect();
            report.violations.push(format!(
                "deben ir juntos y quedaron separados: {}",
                names.join(", ")
            ));
        }
    }

    for &(a, b) in &problem.relations.apart {
        report.apart += 1;
        if let Some(group) = partition
            .group_of(a)
            .filter(|&g| partition.group_of(b) == Some(g))
        {
            report.violations.push(format!(
                "deben ir separados y quedaron en el grupo {}: {} y {}",
                ctx.group_labels[group],
                name(a),
                name(b)
            ));
        }
    }

    if let Some((min, max)) = size_bounds {
        for (group, size) in partition.sizes().into_iter().enumerate() {
            if size < min || size > max {
                report.violations.push(format!(
                    "el grupo {} tiene {size} alumnos (permitido: {min}–{max})",
                    ctx.group_labels[group]
                ));
            }
        }
    }
    report
}

/// "8 grupos: 6 de 4 y 2 de 3 alumnos" o "8 grupos de 4 alumnos".
fn sizes_summary(sizes: &[usize]) -> String {
    let mut distinct = sizes.to_vec();
    distinct.sort_unstable_by(|a, b| b.cmp(a));
    distinct.dedup();
    let groups = if sizes.len() == 1 { "grupo" } else { "grupos" };
    if let [only] = distinct[..] {
        return format!("{} {groups} de {only} alumnos", sizes.len());
    }
    let parts: Vec<String> = distinct
        .iter()
        .map(|&size| format!("{} de {size}", sizes.iter().filter(|&&s| s == size).count()))
        .collect();
    format!(
        "{} {groups}: {} alumnos",
        sizes.len(),
        crate::text::join_with_and(&parts)
    )
}

pub fn render_text(report: &Report) -> String {
    let mut lines = vec![report.sizes.clone(), String::new()];
    lines.push("Calidad (100 % = ideal · 0 % = como un reparto al azar)".into());
    let label_width = report
        .criteria
        .iter()
        .map(|c| c.criterion.chars().count())
        .max()
        .unwrap_or(0);
    let goal_width = report
        .criteria
        .iter()
        .map(|c| c.goal.chars().count())
        .max()
        .unwrap_or(0);
    for line in &report.criteria {
        lines.push(format!(
            "  {:<label_width$}  {:<goal_width$}  {:>4} %   {}",
            line.criterion,
            line.goal,
            (line.quality * 100.0).round() as i64,
            line.detail
        ));
    }
    lines.extend(constraint_lines(&report.constraints));
    lines.push(String::new());
    for group in &report.groups {
        let names: Vec<&str> = group.members.iter().map(|m| m.name.as_str()).collect();
        lines.push(format!(
            "Grupo {} ({}): {}",
            group.group,
            group.members.len(),
            names.join(", ")
        ));
        lines.extend(
            wrap(&group.summary, " · ", SUMMARY_WIDTH)
                .into_iter()
                .map(|l| format!("   {l}")),
        );
    }
    lines.join("\n")
}

fn constraint_lines(constraints: &ConstraintsReport) -> Vec<String> {
    if constraints.together + constraints.apart == 0 && constraints.violations.is_empty() {
        return Vec::new();
    }
    let mut kinds = Vec::new();
    if constraints.together > 0 {
        kinds.push(format!("{} juntos", constraints.together));
    }
    if constraints.apart > 0 {
        kinds.push(format!("{} separados", constraints.apart));
    }
    let kinds = if kinds.is_empty() {
        String::new()
    } else {
        format!(" ({})", kinds.join(" · "))
    };
    if constraints.violations.is_empty() {
        return vec![format!("Restricciones: todas cumplidas{kinds}")];
    }
    let mut lines = vec![format!(
        "Restricciones: {} sin cumplir{kinds}",
        constraints.violations.len()
    )];
    lines.extend(constraints.violations.iter().map(|v| format!("  ✗ {v}")));
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summarizes_sizes() {
        assert_eq!(
            sizes_summary(&[4, 4, 3, 4]),
            "4 grupos: 3 de 4 y 1 de 3 alumnos"
        );
        assert_eq!(sizes_summary(&[4, 4]), "2 grupos de 4 alumnos");
    }
}
