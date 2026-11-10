//! Preferencias (blandas): con quién quiere trabajar cada alumno y a quién prefiere evitar.
//!
//! Una elección cumplida no cuesta; una incumplida cuesta su peso. Quien elige a
//! varios reparte su peso, así cada alumno pesa lo mismo. Evitar a alguien y quedar
//! juntos cuesta 1.

use super::pairs::{Pair, PairCost, PairSet};
use super::{Change, Context, Criterion, Meta, Reference, Tracker};
use crate::partition::Partition;
use crate::relations::{Preference, PreferenceKind};

pub struct Preferences {
    meta: Meta,
    choices: Vec<Preference>,
    pairs: PairSet,
    student_count: usize,
}

impl Preferences {
    pub fn new(weight: f64, choices: &[Preference], student_count: usize) -> Self {
        let pairs = choices
            .iter()
            .map(|choice| match choice.kind {
                PreferenceKind::Prefer => Pair {
                    a: choice.from,
                    b: choice.to,
                    cost_if_together: 0.0,
                    cost_if_apart: choice.weight,
                },
                PreferenceKind::Avoid => Pair {
                    a: choice.from,
                    b: choice.to,
                    cost_if_together: choice.weight,
                    cost_if_apart: 0.0,
                },
            })
            .collect();
        Preferences {
            meta: Meta {
                label: "preferencias".into(),
                goal: "cumplir",
                weight,
            },
            choices: choices.to_vec(),
            pairs: PairSet::new(pairs, student_count),
            student_count,
        }
    }

    /// Cuántas elecciones se cumplen, quiénes no consiguieron a nadie y cuántos pares a evitar quedaron juntos.
    fn summary(&self, partition: &Partition) -> String {
        let mut chosen = 0;
        let mut met = 0;
        let mut asked_and_got = vec![(0, 0); self.student_count];
        let mut to_avoid = 0;
        let mut avoided_together = 0;
        for choice in &self.choices {
            let (Some(a), Some(b)) = (
                partition.group_of(choice.from),
                partition.group_of(choice.to),
            ) else {
                continue;
            };
            match choice.kind {
                PreferenceKind::Prefer => {
                    chosen += 1;
                    asked_and_got[choice.from].0 += 1;
                    if a == b {
                        met += 1;
                        asked_and_got[choice.from].1 += 1;
                    }
                }
                PreferenceKind::Avoid => {
                    to_avoid += 1;
                    avoided_together += usize::from(a == b);
                }
            }
        }
        let without_any = asked_and_got
            .iter()
            .filter(|(asked, got)| *asked > 0 && *got == 0)
            .count();
        let mut parts = Vec::new();
        if chosen > 0 {
            parts.push(format!("{met} de {chosen} elecciones cumplidas"));
            parts.push(format!("{without_any} sin ninguno de sus elegidos"));
        }
        if to_avoid > 0 {
            parts.push(format!(
                "{avoided_together} de {to_avoid} pares a evitar quedaron juntos"
            ));
        }
        if parts.is_empty() {
            "sin preferencias declaradas".into()
        } else {
            parts.join(" · ")
        }
    }

    /// Frases sobre las elecciones que involucran a `students` si se suman a `group`.
    fn join_notes(&self, ctx: &Context, students: &[usize], group: usize) -> Vec<String> {
        let mut notes = Vec::new();
        for &student in students {
            for choice in self
                .choices
                .iter()
                .filter(|c| c.from == student || c.to == student)
            {
                let chooser = choice.from == student;
                let other = if chooser { choice.to } else { choice.from };
                let other_group = ctx.partition.group_of(other);
                let joins_with_us = students.contains(&other);
                if other_group.is_none() && !joins_with_us {
                    continue;
                }
                let same_group = joins_with_us || other_group == Some(group);
                let who = ctx.roster.name(other);
                let me = ctx.roster.name(student);
                let note = match (choice.kind, chooser, same_group) {
                    (PreferenceKind::Prefer, true, true) => {
                        format!("eligió a {who}, que está en el grupo")
                    }
                    (PreferenceKind::Prefer, true, false) => {
                        let label = other_group.map_or("?", |g| ctx.group_labels[g].as_str());
                        format!("eligió a {who}, que está en el grupo {label}")
                    }
                    (PreferenceKind::Prefer, false, true) => format!("{who} eligió a {me}"),
                    (PreferenceKind::Avoid, true, true) => {
                        format!("prefiere evitar a {who}, que está en el grupo")
                    }
                    (PreferenceKind::Avoid, false, true) => format!("{who} prefiere evitar a {me}"),
                    _ => continue,
                };
                notes.push(note);
            }
        }
        notes
    }
}

impl Criterion for Preferences {
    fn meta(&self) -> &Meta {
        &self.meta
    }

    /// Ideal 0. Al azar, dos alumnos dados comparten grupo con probabilidad
    /// p = Σ s(s − 1) / (n(n − 1)).
    fn reference(&self, sizes: &[usize]) -> Reference {
        let n = self.student_count;
        let same_group = sizes
            .iter()
            .map(|&s| (s * s.saturating_sub(1)) as f64)
            .sum::<f64>()
            / (n * n.saturating_sub(1)).max(1) as f64;
        let random = self
            .choices
            .iter()
            .map(|choice| match choice.kind {
                PreferenceKind::Prefer => choice.weight * (1.0 - same_group),
                PreferenceKind::Avoid => choice.weight * same_group,
            })
            .sum();
        Reference { ideal: 0.0, random }
    }

    fn tracker<'a>(&'a self, partition: &Partition) -> Box<dyn Tracker + 'a> {
        Box::new(PreferencesTracker {
            criterion: self,
            cost: PairCost::new(&self.pairs, partition),
        })
    }
}

struct PreferencesTracker<'a> {
    criterion: &'a Preferences,
    cost: PairCost<'a>,
}

impl Tracker for PreferencesTracker<'_> {
    fn cost(&self) -> f64 {
        self.cost.cost()
    }

    fn update(&mut self, change: &Change, partition: &Partition) -> f64 {
        self.cost.update(change, partition)
    }

    fn describe(&self, ctx: &Context, _reference: &Reference) -> String {
        self.criterion.summary(ctx.partition)
    }

    fn describe_group(&self, _ctx: &Context, _group: usize) -> Option<String> {
        None
    }

    fn explain_join(&self, ctx: &Context, students: &[usize], group: usize) -> Option<String> {
        let notes = self.criterion.join_notes(ctx, students, group);
        (!notes.is_empty()).then(|| notes.join("; "))
    }
}
