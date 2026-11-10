//! Un problema de agrupación: los alumnos, sus relaciones y los criterios a optimizar.

use crate::config::Config;
use crate::criteria::{self, Apart, Preferences};
use crate::data::Table;
use crate::objective::Objective;
use crate::partition::Partition;
use crate::relations::Relations;
use crate::roster::Roster;
use crate::state::State;
use anyhow::Result;

pub struct Problem {
    pub roster: Roster,
    pub relations: Relations,
    pub objective: Objective,
    /// Avisos sobre los datos (celdas vacías, nombres desconocidos...).
    pub warnings: Vec<String>,
}

impl Problem {
    pub fn from_table(table: &Table, cfg: &Config) -> Result<Problem> {
        let roster = Roster::from_table(table, cfg)?;
        let mut warnings = Vec::new();
        let mut criteria = criteria::from_attributes(table, cfg, &roster, &mut warnings)?;
        let relations = Relations::build(table, cfg, &roster, &mut warnings)?;
        if let Some(preferences) = &cfg.preferences {
            criteria.push(Box::new(Preferences::new(
                preferences.weight,
                &relations.preferences,
                roster.len(),
            )));
        }
        if !relations.apart.is_empty() {
            criteria.push(Box::new(Apart::new(&relations.apart, roster.len())));
        }
        Ok(Problem {
            roster,
            relations,
            objective: Objective::new(criteria),
            warnings,
        })
    }

    pub fn student_count(&self) -> usize {
        self.roster.len()
    }

    /// Fija las referencias de los criterios para grupos con estos tamaños.
    pub fn calibrate(&mut self, sizes: &[usize]) {
        self.objective.calibrate(sizes);
    }

    /// Empieza a seguir el costo de una partición.
    pub fn state(&self, partition: Partition) -> State<'_> {
        State::new(&self.objective, partition)
    }
}
