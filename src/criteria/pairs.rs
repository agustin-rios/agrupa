//! Criterios que se miden por pares de alumnos: cada par cuesta según si quedaron
//! juntos o separados. Lo usan las preferencias y la restricción `apart`.

use super::Change;
use crate::partition::Partition;

#[derive(Debug, Clone, Copy)]
pub struct Pair {
    pub a: usize,
    pub b: usize,
    pub cost_if_together: f64,
    pub cost_if_apart: f64,
}

impl Pair {
    pub fn other(&self, student: usize) -> usize {
        if self.a == student { self.b } else { self.a }
    }

    /// Costo según si quedaron juntos; nada si alguno de los dos no tiene grupo.
    fn cost(&self, together: Option<bool>) -> f64 {
        match together {
            Some(true) => self.cost_if_together,
            Some(false) => self.cost_if_apart,
            None => 0.0,
        }
    }
}

/// `Some(true)` si comparten grupo; `None` si alguno no tiene.
fn together(a: Option<usize>, b: Option<usize>) -> Option<bool> {
    Some(a? == b?)
}

/// Pares con un índice por alumno, para encontrar rápido los de quien se mueve.
pub struct PairSet {
    pairs: Vec<Pair>,
    by_student: Vec<Vec<usize>>,
}

impl PairSet {
    pub fn new(pairs: Vec<Pair>, student_count: usize) -> Self {
        let mut by_student = vec![Vec::new(); student_count];
        for (index, pair) in pairs.iter().enumerate() {
            by_student[pair.a].push(index);
            by_student[pair.b].push(index);
        }
        PairSet { pairs, by_student }
    }

    /// Pares en que participa el alumno.
    pub fn of(&self, student: usize) -> impl Iterator<Item = &Pair> {
        self.by_student[student]
            .iter()
            .map(|&index| &self.pairs[index])
    }

    fn total_cost(&self, partition: &Partition) -> f64 {
        self.pairs
            .iter()
            .map(|p| p.cost(together(partition.group_of(p.a), partition.group_of(p.b))))
            .sum()
    }
}

/// El costo de un conjunto de pares, al día tras cada cambio.
pub struct PairCost<'a> {
    set: &'a PairSet,
    cost: f64,
}

impl<'a> PairCost<'a> {
    pub fn new(set: &'a PairSet, partition: &Partition) -> Self {
        PairCost {
            set,
            cost: set.total_cost(partition),
        }
    }

    pub fn cost(&self) -> f64 {
        self.cost
    }

    /// Recalcula solo los pares de los alumnos que se movieron.
    pub fn update(&mut self, change: &Change, partition: &Partition) -> f64 {
        let mut delta = 0.0;
        for step in change.steps {
            for pair in self.set.of(step.student) {
                let other = pair.other(step.student);
                // Un par entre dos alumnos que se movieron se cuenta una sola vez.
                if other < step.student && change.moved(other) {
                    continue;
                }
                let before = together(step.from, change.group_before(other, partition));
                let after = together(step.to, partition.group_of(other));
                delta += pair.cost(after) - pair.cost(before);
            }
        }
        self.cost += delta;
        delta
    }
}
