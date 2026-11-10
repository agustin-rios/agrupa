//! Restricción dura `apart`: pares que no pueden compartir grupo.
//!
//! Se trata como un criterio más, pero con una penalización tan grande que ninguna
//! mejora en los demás la compensa (ver `Objective`).

use super::pairs::{Pair, PairCost, PairSet};
use super::{Change, Context, Criterion, Meta, Reference, Tracker};
use crate::partition::Partition;

pub struct Apart {
    meta: Meta,
    pairs: PairSet,
}

impl Apart {
    pub fn new(pairs: &[(usize, usize)], student_count: usize) -> Self {
        let pairs = pairs
            .iter()
            .map(|&(a, b)| Pair {
                a,
                b,
                cost_if_together: 1.0,
                cost_if_apart: 0.0,
            })
            .collect();
        Apart {
            meta: Meta {
                label: "restricción".into(),
                goal: "separar",
                weight: 0.0,
            },
            pairs: PairSet::new(pairs, student_count),
        }
    }
}

impl Criterion for Apart {
    fn meta(&self) -> &Meta {
        &self.meta
    }

    fn is_hard(&self) -> bool {
        true
    }

    fn reference(&self, _sizes: &[usize]) -> Reference {
        Reference::default()
    }

    fn tracker<'a>(&'a self, partition: &Partition) -> Box<dyn Tracker + 'a> {
        Box::new(ApartTracker {
            criterion: self,
            cost: PairCost::new(&self.pairs, partition),
        })
    }
}

struct ApartTracker<'a> {
    criterion: &'a Apart,
    cost: PairCost<'a>,
}

impl Tracker for ApartTracker<'_> {
    fn cost(&self) -> f64 {
        self.cost.cost()
    }

    fn update(&mut self, change: &Change, partition: &Partition) -> f64 {
        self.cost.update(change, partition)
    }

    fn describe(&self, _ctx: &Context, _reference: &Reference) -> String {
        format!(
            "{} pares que deben ir separados quedaron juntos",
            self.cost()
        )
    }

    fn describe_group(&self, _ctx: &Context, _group: usize) -> Option<String> {
        None
    }

    /// Avisa si alguno de `students` quedaría con alguien de quien debe ir separado.
    fn explain_join(&self, ctx: &Context, students: &[usize], group: usize) -> Option<String> {
        let clashes: Vec<String> = students
            .iter()
            .flat_map(|&s| self.criterion.pairs.of(s).map(move |pair| pair.other(s)))
            .filter(|&other| ctx.partition.group_of(other) == Some(group))
            .map(|other| format!("debe ir separado de {}", ctx.roster.name(other)))
            .collect();
        (!clashes.is_empty()).then(|| clashes.join("; "))
    }
}
