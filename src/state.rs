//! Una partición junto con el costo de cada criterio, al día tras cada cambio.
//!
//! `State` no sabe calcular costos: le avisa a cada criterio qué cambió y suma lo que
//! cada uno responde, multiplicado por su escala.

use crate::criteria::{Change, Step, Tracker};
use crate::objective::Objective;
use crate::partition::Partition;

/// Un alumno que pasa a otro grupo (o queda sin grupo).
#[derive(Debug, Clone, Copy)]
pub struct Move {
    pub student: usize,
    pub to: Option<usize>,
}

impl Move {
    pub fn to_group(student: usize, group: usize) -> Move {
        Move {
            student,
            to: Some(group),
        }
    }

    pub fn unassign(student: usize) -> Move {
        Move { student, to: None }
    }
}

pub struct State<'a> {
    objective: &'a Objective,
    partition: Partition,
    trackers: Vec<Box<dyn Tracker + 'a>>,
    /// Pasos del último cambio aplicado, para poder deshacerlo.
    last_steps: Vec<Step>,
    /// Grupos que tocó el último cambio.
    touched_groups: Vec<usize>,
    /// Búfer reutilizado al deshacer, para no reservar memoria en cada intento.
    undo_moves: Vec<Move>,
}

impl<'a> State<'a> {
    pub fn new(objective: &'a Objective, partition: Partition) -> Self {
        let trackers = objective
            .terms()
            .iter()
            .map(|term| term.criterion.tracker(&partition))
            .collect();
        State {
            objective,
            partition,
            trackers,
            last_steps: Vec::new(),
            touched_groups: Vec::new(),
            undo_moves: Vec::new(),
        }
    }

    pub fn partition(&self) -> &Partition {
        &self.partition
    }

    pub fn into_partition(self) -> Partition {
        self.partition
    }

    pub fn objective(&self) -> &Objective {
        self.objective
    }

    /// Costo total: Σ escala × costo de cada criterio.
    pub fn total(&self) -> f64 {
        self.objective
            .terms()
            .iter()
            .zip(&self.trackers)
            .map(|(term, tracker)| term.scale * tracker.cost())
            .sum()
    }

    /// El seguimiento del criterio `term` (mismo orden que en el objetivo).
    pub fn tracker(&self, term: usize) -> &dyn Tracker {
        self.trackers[term].as_ref()
    }

    /// Aplica los cambios y devuelve cuánto cambió el costo total.
    pub fn apply(&mut self, moves: &[Move]) -> f64 {
        self.record(moves);
        for step in &self.last_steps {
            self.partition.move_student(step.student, step.to);
        }
        let change = Change {
            steps: &self.last_steps,
            groups: &self.touched_groups,
        };
        let mut delta = 0.0;
        for (term, tracker) in self.objective.terms().iter().zip(&mut self.trackers) {
            delta += term.scale * tracker.update(&change, &self.partition);
        }
        delta
    }

    /// Aplica los cambios y los deja solo si `accept` aprueba el cambio de costo.
    pub fn try_apply(&mut self, moves: &[Move], accept: impl FnOnce(f64) -> bool) -> bool {
        let delta = self.apply(moves);
        let accepted = accept(delta);
        if !accepted {
            self.undo_last();
        }
        accepted
    }

    /// Cuánto cambiaría el costo total con estos cambios, sin dejarlos hechos.
    pub fn cost_change(&mut self, moves: &[Move]) -> f64 {
        let delta = self.apply(moves);
        self.undo_last();
        delta
    }

    /// Como `cost_change`, pero criterio por criterio (ya escalado).
    pub fn cost_change_by_term(&mut self, moves: &[Move]) -> Vec<f64> {
        let before: Vec<f64> = self.trackers.iter().map(|t| t.cost()).collect();
        self.apply(moves);
        let deltas = self
            .objective
            .terms()
            .iter()
            .zip(&self.trackers)
            .zip(before)
            .map(|((term, tracker), before)| term.scale * (tracker.cost() - before))
            .collect();
        self.undo_last();
        deltas
    }

    /// Lleva la partición a la asignación dada (por ejemplo, la mejor encontrada).
    pub fn reset_to(&mut self, assignment: &[Option<usize>]) {
        let moves: Vec<Move> = assignment
            .iter()
            .enumerate()
            .filter(|&(student, &group)| self.partition.group_of(student) != group)
            .map(|(student, &to)| Move { student, to })
            .collect();
        self.apply(&moves);
    }

    fn undo_last(&mut self) {
        let mut undo = std::mem::take(&mut self.undo_moves);
        undo.clear();
        undo.extend(self.last_steps.iter().map(|step| Move {
            student: step.student,
            to: step.from,
        }));
        self.apply(&undo);
        self.undo_moves = undo;
    }

    /// Anota los pasos reales (ignora los que no cambian de grupo) y los grupos que tocan.
    fn record(&mut self, moves: &[Move]) {
        self.last_steps.clear();
        self.touched_groups.clear();
        for mv in moves {
            let from = self.partition.group_of(mv.student);
            if from == mv.to {
                continue;
            }
            self.last_steps.push(Step {
                student: mv.student,
                from,
                to: mv.to,
            });
            for group in [from, mv.to].into_iter().flatten() {
                if !self.touched_groups.contains(&group) {
                    self.touched_groups.push(group);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;
    use crate::data::Table;
    use crate::problem::Problem;
    use rand::{Rng, SeedableRng, rngs::StdRng};

    const STUDENTS: &str = "id,nombre,g,nota,hab,horario,quiere,evita
a1,Ana,F,5.0,\"Front, Back\",M,a2,
a2,Beto,M,6.1,Back,T,,a3
a3,Caro,F,4.2,Datos,M,a1,
a4,Dani,M,5.5,\"Front, UX\",T,Caro,
a5,Eli,F,,Back,M,,
a6,Fede,M,6.8,Datos,,a5,a1
a7,Gabi,F,3.9,UX,T,,
a8,Hugo,M,5.1,Front,M,a7,
";

    const CONFIG: &str = r#"
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

    fn problem() -> Problem {
        let table = Table::parse(STUDENTS, None).unwrap();
        let mut problem = Problem::from_table(&table, &Config::parse(CONFIG).unwrap()).unwrap();
        problem.calibrate(&[3, 3, 2]);
        problem
    }

    fn groups(assignment: &[usize]) -> Partition {
        Partition::complete(3, assignment)
    }

    #[test]
    fn incremental_cost_matches_full_recompute() {
        let problem = problem();
        let mut rng = StdRng::seed_from_u64(7);
        let mut state = problem.state(groups(&[0, 0, 0, 1, 1, 1, 2, 2]));
        let mut total = state.total();
        for _ in 0..500 {
            let student = rng.gen_range(0..8);
            let to = if rng.gen_bool(0.1) {
                None
            } else {
                Some(rng.gen_range(0..3))
            };
            total += state.apply(&[Move { student, to }]);
            let fresh = problem.state(state.partition().clone());
            assert!(
                (fresh.total() - total).abs() < 1e-9,
                "{} vs {}",
                fresh.total(),
                total
            );
            for term in 0..problem.objective.terms().len() {
                assert!((fresh.tracker(term).cost() - state.tracker(term).cost()).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn rejected_change_leaves_cost_untouched() {
        let problem = problem();
        let mut state = problem.state(groups(&[0, 0, 0, 1, 1, 1, 2, 2]));
        let before = state.total();
        let accepted = state.try_apply(&[Move::to_group(0, 1), Move::to_group(3, 0)], |_| false);
        assert!(!accepted);
        assert!((state.total() - before).abs() < 1e-12);
        assert_eq!(state.partition().group_of(0), Some(0));
    }

    #[test]
    fn apart_violation_costs_the_hard_penalty() {
        let problem = problem();
        let state = problem.state(groups(&[0, 1, 1, 1, 2, 0, 2, 2]));
        assert!(state.total() >= problem.objective.hard_penalty());
    }
}
