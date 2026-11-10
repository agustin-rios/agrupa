//! Criterios que se suman por grupo.
//!
//! El costo total es la suma de un costo por grupo, y el costo de cada grupo sale de
//! un agregado de sus integrantes (sumas, conteos...). Basta con implementar
//! `GroupwiseCriterion`: el seguimiento incremental es común a todos.

use super::{Change, Context, Criterion, Meta, Reference, Tracker};
use crate::partition::Partition;

pub trait GroupwiseCriterion: Send + Sync {
    /// Agregado por grupo del que sale su costo.
    type Stats: Clone;

    fn meta(&self) -> &Meta;
    fn reference(&self, sizes: &[usize]) -> Reference;

    fn empty_stats(&self) -> Self::Stats;
    fn add(&self, stats: &mut Self::Stats, student: usize);
    fn remove(&self, stats: &mut Self::Stats, student: usize);
    fn group_cost(&self, stats: &Self::Stats, size: usize) -> f64;

    /// Resumen de todos los grupos para el reporte.
    fn describe(
        &self,
        groups: &[Group<'_, Self::Stats>],
        cost: f64,
        reference: &Reference,
    ) -> String;
    /// Resumen corto de un grupo con integrantes.
    fn describe_group(&self, group: &Group<'_, Self::Stats>) -> String;
    /// Qué cambia en el grupo si se le suman `students`.
    fn explain_join(&self, group: &Group<'_, Self::Stats>, students: &[usize]) -> String;
}

/// Un grupo visto por un criterio: su agregado y sus integrantes.
pub struct Group<'a, S> {
    pub stats: &'a S,
    pub members: &'a [usize],
}

impl<S> Group<'_, S> {
    pub fn size(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }
}

impl<C: GroupwiseCriterion> Criterion for C {
    fn meta(&self) -> &Meta {
        GroupwiseCriterion::meta(self)
    }

    fn reference(&self, sizes: &[usize]) -> Reference {
        GroupwiseCriterion::reference(self, sizes)
    }

    fn tracker<'a>(&'a self, partition: &Partition) -> Box<dyn Tracker + 'a> {
        Box::new(GroupwiseTracker::new(self, partition))
    }
}

/// Guarda el agregado y el costo de cada grupo; un cambio recalcula solo los grupos que toca.
struct GroupwiseTracker<'a, C: GroupwiseCriterion> {
    criterion: &'a C,
    stats: Vec<C::Stats>,
    group_costs: Vec<f64>,
    cost: f64,
}

impl<'a, C: GroupwiseCriterion> GroupwiseTracker<'a, C> {
    fn new(criterion: &'a C, partition: &Partition) -> Self {
        let stats: Vec<C::Stats> = (0..partition.group_count())
            .map(|group| {
                let mut stats = criterion.empty_stats();
                for &student in partition.members(group) {
                    criterion.add(&mut stats, student);
                }
                stats
            })
            .collect();
        let group_costs: Vec<f64> = stats
            .iter()
            .enumerate()
            .map(|(group, stats)| criterion.group_cost(stats, partition.size(group)))
            .collect();
        let cost = group_costs.iter().sum();
        GroupwiseTracker {
            criterion,
            stats,
            group_costs,
            cost,
        }
    }

    fn group<'p>(&'p self, partition: &'p Partition, group: usize) -> Group<'p, C::Stats> {
        Group {
            stats: &self.stats[group],
            members: partition.members(group),
        }
    }
}

impl<C: GroupwiseCriterion> Tracker for GroupwiseTracker<'_, C> {
    fn cost(&self) -> f64 {
        self.cost
    }

    fn update(&mut self, change: &Change, partition: &Partition) -> f64 {
        for step in change.steps {
            if let Some(from) = step.from {
                self.criterion.remove(&mut self.stats[from], step.student);
            }
            if let Some(to) = step.to {
                self.criterion.add(&mut self.stats[to], step.student);
            }
        }
        let mut delta = 0.0;
        for &group in change.groups {
            let new_cost = self
                .criterion
                .group_cost(&self.stats[group], partition.size(group));
            delta += new_cost - self.group_costs[group];
            self.group_costs[group] = new_cost;
        }
        self.cost += delta;
        delta
    }

    fn describe(&self, ctx: &Context, reference: &Reference) -> String {
        let groups: Vec<_> = (0..ctx.partition.group_count())
            .map(|g| self.group(ctx.partition, g))
            .collect();
        self.criterion.describe(&groups, self.cost, reference)
    }

    fn describe_group(&self, ctx: &Context, group: usize) -> Option<String> {
        let group = self.group(ctx.partition, group);
        if group.is_empty() {
            return Some(format!(
                "{} —",
                GroupwiseCriterion::meta(self.criterion).label
            ));
        }
        Some(self.criterion.describe_group(&group))
    }

    fn explain_join(&self, ctx: &Context, students: &[usize], group: usize) -> Option<String> {
        Some(
            self.criterion
                .explain_join(&self.group(ctx.partition, group), students),
        )
    }
}
