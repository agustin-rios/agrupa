//! Sugerencia de grupo para alumnos nuevos, sin mover a nadie que ya tenía grupo.
//!
//! 1. Cada nuevo (o bloque `together` de nuevos), en orden, va al grupo con cupo
//!    donde menos sube el costo total (Δ).
//! 2. Se re-ubica a los nuevos entre ellos mientras mejore el resultado conjunto,
//!    para que el orden del archivo no favorezca al primero.
//! 3. La explicación de cada uno se calcula con los demás nuevos ya en su lugar.

use crate::criteria::Context;
use crate::partition::Partition;
use crate::problem::Problem;
use crate::roster::Roster;
use crate::state::{Move, State};

/// Diferencia mínima de costo para considerar que un cambio mejora.
const MIN_IMPROVEMENT: f64 = 1e-9;
/// Rondas de re-ubicación como máximo.
const MAX_ROUNDS: usize = 20;
/// Grupos con cupo que se muestran por alumno.
const SHOWN_OPTIONS: usize = 3;
/// Grupos sin cupo que se muestran como alternativa.
const SHOWN_BIGGER: usize = 3;

/// Alumnos nuevos que se ubican juntos: uno suelto o un bloque `together`.
struct Newcomers {
    students: Vec<usize>,
    /// Grupos donde ya están sus compañeros de bloque: si hay, debe ir ahí.
    anchors: Vec<usize>,
}

impl Newcomers {
    fn allows(&self, group: usize) -> bool {
        self.anchors.is_empty() || self.anchors.contains(&group)
    }

    fn moves_to(&self, group: Option<usize>) -> Vec<Move> {
        self.students
            .iter()
            .map(|&student| Move { student, to: group })
            .collect()
    }
}

/// Un grupo posible para un alumno nuevo.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub group: usize,
    /// Tamaño del grupo antes de sumar al alumno.
    pub size: usize,
    pub delta: f64,
    /// Δ de cada criterio, en el orden del objetivo.
    pub by_term: Vec<f64>,
    /// Si quedaría con alguien de quien debe ir separado.
    pub breaks_constraint: bool,
    /// Tiene cupo y no contradice un bloque `together`.
    pub eligible: bool,
}

#[derive(Debug, Clone)]
pub struct Suggestion {
    pub students: Vec<usize>,
    /// Elegibles primero, de menor a mayor Δ; el primero es el elegido.
    pub candidates: Vec<Candidate>,
    pub reasons: Vec<Reason>,
    pub note: Option<String>,
}

impl Suggestion {
    pub fn chosen(&self) -> &Candidate {
        &self.candidates[0]
    }
}

/// Lo que un criterio dice del grupo elegido.
#[derive(Debug, Clone)]
pub struct Reason {
    pub criterion: String,
    pub delta: f64,
    /// Es una restricción dura (y la incumple).
    pub hard: bool,
    pub text: String,
}

/// Ubica a todos los alumnos sin grupo y deja `state` actualizado.
/// `max_size`: tope por grupo; si no hay, se llenan primero los más chicos.
pub fn suggest_all(
    problem: &Problem,
    state: &mut State,
    labels: &[String],
    max_size: Option<usize>,
) -> Vec<Suggestion> {
    let units = find_newcomers(problem, state.partition());
    for unit in &units {
        let (candidates, _) = rank_groups(state, unit, max_size);
        place(state, unit, Some(candidates[0].group));
    }
    improve_jointly(state, &units, max_size);
    units
        .iter()
        .map(|unit| explain_final(problem, state, unit, labels, max_size))
        .collect()
}

/// Agrupa a los alumnos sin grupo según sus bloques `together`, en orden de aparición.
fn find_newcomers(problem: &Problem, partition: &Partition) -> Vec<Newcomers> {
    let mut grouped = vec![false; partition.student_count()];
    let mut units = Vec::new();
    for student in partition.unassigned() {
        if grouped[student] {
            continue;
        }
        let block = problem.relations.blocks.of(student);
        let students: Vec<usize> = block
            .iter()
            .copied()
            .filter(|&s| partition.group_of(s).is_none())
            .collect();
        for &s in &students {
            grouped[s] = true;
        }
        let mut anchors: Vec<usize> = block
            .iter()
            .filter_map(|&s| partition.group_of(s))
            .collect();
        anchors.sort_unstable();
        anchors.dedup();
        units.push(Newcomers { students, anchors });
    }
    units
}

/// Evalúa sumar la unidad (que no debe tener grupo) a cada grupo. Devuelve los
/// candidatos ordenados y un aviso si todos los grupos estaban llenos.
fn rank_groups(
    state: &mut State,
    unit: &Newcomers,
    max_size: Option<usize>,
) -> (Vec<Candidate>, Option<String>) {
    let limit = max_size.unwrap_or(state.partition().smallest_size() + unit.students.len());
    let hard: Vec<bool> = state
        .objective()
        .terms()
        .iter()
        .map(|t| t.is_hard())
        .collect();
    let mut candidates: Vec<Candidate> = (0..state.partition().group_count())
        .map(|group| {
            let by_term = state.cost_change_by_term(&unit.moves_to(Some(group)));
            let size = state.partition().size(group);
            Candidate {
                group,
                size,
                delta: by_term.iter().sum(),
                breaks_constraint: by_term
                    .iter()
                    .zip(&hard)
                    .any(|(&delta, &hard)| hard && delta > 0.0),
                eligible: size + unit.students.len() <= limit && unit.allows(group),
                by_term,
            }
        })
        .collect();

    let mut note = None;
    if !candidates.iter().any(|c| c.eligible) {
        for candidate in &mut candidates {
            candidate.eligible = unit.allows(candidate.group);
        }
        note = Some(match max_size {
            Some(max) => format!(
                "todos los grupos posibles tienen {max} alumnos (max_size); se elige igual el de menor Δ"
            ),
            None => "todos los grupos posibles están llenos; se elige igual el de menor Δ".into(),
        });
    }
    candidates.sort_by(|a, b| {
        b.eligible
            .cmp(&a.eligible)
            .then(a.delta.total_cmp(&b.delta))
            .then(a.size.cmp(&b.size))
            .then(a.group.cmp(&b.group))
    });
    (candidates, note)
}

/// Re-ubica a los nuevos (moviéndolos o intercambiándolos entre ellos) mientras mejore.
fn improve_jointly(state: &mut State, units: &[Newcomers], max_size: Option<usize>) {
    for _ in 0..MAX_ROUNDS {
        let mut improved = false;
        for unit in units {
            improved |= move_to_better_group(state, unit, max_size);
        }
        for (index, a) in units.iter().enumerate() {
            for b in &units[index + 1..] {
                improved |= swap_if_better(state, a, b);
            }
        }
        if !improved {
            break;
        }
    }
}

/// Saca la unidad, la vuelve a ubicar con los demás fijos y avisa si cambió de grupo.
fn move_to_better_group(state: &mut State, unit: &Newcomers, max_size: Option<usize>) -> bool {
    let current = group_of(state, unit);
    place(state, unit, None);
    let (candidates, _) = rank_groups(state, unit, max_size);
    let current_delta = candidates
        .iter()
        .find(|c| c.group == current)
        .map_or(f64::INFINITY, |c| c.delta);
    let best = &candidates[0];
    let target = if best.group != current && best.delta < current_delta - MIN_IMPROVEMENT {
        best.group
    } else {
        current
    };
    place(state, unit, Some(target));
    target != current
}

fn swap_if_better(state: &mut State, a: &Newcomers, b: &Newcomers) -> bool {
    let (group_a, group_b) = (group_of(state, a), group_of(state, b));
    if group_a == group_b
        || a.students.len() != b.students.len()
        || !a.allows(group_b)
        || !b.allows(group_a)
    {
        return false;
    }
    let mut moves = a.moves_to(Some(group_b));
    moves.extend(b.moves_to(Some(group_a)));
    state.try_apply(&moves, |delta| delta < -MIN_IMPROVEMENT)
}

/// La sugerencia final de una unidad: sus opciones y por qué, con los demás nuevos ya ubicados.
fn explain_final(
    problem: &Problem,
    state: &mut State,
    unit: &Newcomers,
    labels: &[String],
    max_size: Option<usize>,
) -> Suggestion {
    let current = group_of(state, unit);
    place(state, unit, None);
    let (mut candidates, mut note) = rank_groups(state, unit, max_size);
    if let Some(position) = candidates.iter().position(|c| c.group == current) {
        let chosen = candidates.remove(position);
        candidates.insert(0, chosen);
    }
    if let [anchor] = unit.anchors[..] {
        note.get_or_insert_with(|| {
            format!(
                "debe ir junto a compañeros que ya están en el grupo {}",
                labels[anchor]
            )
        });
    }
    let reasons = reasons_for(problem, state, unit, &candidates[0], labels);
    place(state, unit, Some(current));
    Suggestion {
        students: unit.students.clone(),
        candidates,
        reasons,
        note,
    }
}

/// Lo que dice cada criterio del grupo elegido, de mayor a menor efecto.
fn reasons_for(
    problem: &Problem,
    state: &State,
    unit: &Newcomers,
    chosen: &Candidate,
    labels: &[String],
) -> Vec<Reason> {
    let ctx = Context {
        partition: state.partition(),
        roster: &problem.roster,
        group_labels: labels,
    };
    let mut reasons: Vec<Reason> = state
        .objective()
        .terms()
        .iter()
        .enumerate()
        .filter_map(|(index, term)| {
            let text = state
                .tracker(index)
                .explain_join(&ctx, &unit.students, chosen.group)?;
            Some(Reason {
                criterion: term.criterion.meta().label.clone(),
                delta: chosen.by_term[index],
                hard: term.is_hard(),
                text,
            })
        })
        .collect();
    reasons.sort_by(|a, b| b.delta.abs().total_cmp(&a.delta.abs()));
    reasons
}

fn place(state: &mut State, unit: &Newcomers, group: Option<usize>) {
    state.apply(&unit.moves_to(group));
}

fn group_of(state: &State, unit: &Newcomers) -> usize {
    state
        .partition()
        .group_of(unit.students[0])
        .expect("la unidad ya tiene grupo")
}

pub fn render_text(roster: &Roster, suggestions: &[Suggestion], labels: &[String]) -> String {
    let mut lines =
        vec!["Sugerencias (Δ = cambio en el costo total; más bajo = mejor)".to_string()];
    for suggestion in suggestions {
        lines.push(String::new());
        lines.extend(render_suggestion(roster, suggestion, labels));
    }
    lines.join("\n")
}

fn render_suggestion(roster: &Roster, suggestion: &Suggestion, labels: &[String]) -> Vec<String> {
    let who: Vec<String> = suggestion
        .students
        .iter()
        .map(|&s| format!("{} ({})", roster.name(s), roster.id(s)))
        .collect();
    let chosen = suggestion.chosen();
    let joining = suggestion.students.len();
    let mut lines = vec![format!(
        "{} → grupo {}",
        who.join(" + "),
        labels[chosen.group]
    )];
    if let Some(note) = &suggestion.note {
        lines.push(format!("   aviso: {note}"));
    }

    let width = labels
        .iter()
        .map(|l| l.chars().count())
        .max()
        .unwrap_or(1)
        .max(5);
    lines.push(format!("     {:<width$}  tamaño       Δ", "grupo"));
    let eligible: Vec<&Candidate> = suggestion
        .candidates
        .iter()
        .filter(|c| c.eligible)
        .collect();
    for (rank, candidate) in eligible.iter().take(SHOWN_OPTIONS).enumerate() {
        let delta = if candidate.breaks_constraint {
            "restricción".to_string()
        } else {
            format!("{:+.3}", candidate.delta)
        };
        lines.push(format!(
            "   {} {:<width$}  {:>2} → {:<2} {:>8}",
            if rank == 0 { "★" } else { " " },
            labels[candidate.group],
            candidate.size,
            candidate.size + joining,
            delta
        ));
    }
    let hidden = eligible.len().saturating_sub(SHOWN_OPTIONS);
    if hidden > 0 {
        lines.push(format!(
            "     … y {hidden} grupo{} más",
            crate::text::plural(hidden)
        ));
    }
    let bigger: Vec<String> = suggestion
        .candidates
        .iter()
        .filter(|c| !c.eligible && !c.breaks_constraint)
        .take(SHOWN_BIGGER)
        .map(|c| {
            format!(
                "{} ({} → {}, {:+.3})",
                labels[c.group],
                c.size,
                c.size + joining,
                c.delta
            )
        })
        .collect();
    if !bigger.is_empty() {
        lines.push(format!("     agrandando un grupo: {}", bigger.join(" · ")));
    }

    if !suggestion.reasons.is_empty() {
        lines.push(format!("   Por qué el grupo {}:", labels[chosen.group]));
        let width = suggestion
            .reasons
            .iter()
            .map(|r| r.criterion.chars().count())
            .max()
            .unwrap_or(0);
        for reason in &suggestion.reasons {
            let (verdict, delta) = match reason.delta {
                _ if reason.hard => ("incumple", String::new()),
                d if d < -MIN_IMPROVEMENT => ("mejora ", format!("{d:+.3}")),
                d if d > MIN_IMPROVEMENT => ("empeora", format!("{d:+.3}")),
                _ => ("igual  ", format!("{:+.3}", 0.0)),
            };
            lines.push(format!(
                "     {verdict}  {:<width$}  {delta:>7}  {}",
                reason.criterion, reason.text
            ));
        }
    }
    lines
}
