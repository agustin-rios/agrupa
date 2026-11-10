//! Qué se minimiza: la suma de los criterios, cada uno multiplicado por su escala.
//!
//! La escala de un criterio blando es peso / (al azar − ideal): así, con peso 1, pasar
//! de un reparto al azar a uno ideal vale 1 en cualquier columna. Las restricciones
//! duras se multiplican por una penalización que domina a todo lo demás.

use crate::criteria::{Criterion, Reference};

/// Una violación de una restricción dura cuesta esto por cada unidad de peso blando.
const HARD_PENALTY_PER_WEIGHT: f64 = 1000.0;

/// Un criterio dentro del objetivo, con su referencia y su escala.
pub struct Term {
    pub criterion: Box<dyn Criterion>,
    pub reference: Reference,
    pub scale: f64,
}

impl Term {
    pub fn is_hard(&self) -> bool {
        self.criterion.is_hard()
    }
}

pub struct Objective {
    terms: Vec<Term>,
    hard_penalty: f64,
}

impl Objective {
    /// Las escalas quedan en 0 hasta llamar a `calibrate`.
    pub fn new(criteria: Vec<Box<dyn Criterion>>) -> Self {
        let terms = criteria
            .into_iter()
            .map(|criterion| Term {
                criterion,
                reference: Reference::default(),
                scale: 0.0,
            })
            .collect();
        Objective {
            terms,
            hard_penalty: 0.0,
        }
    }

    /// Fija referencias y escalas para grupos con estos tamaños.
    pub fn calibrate(&mut self, sizes: &[usize]) {
        let soft_weight: f64 = self
            .terms
            .iter()
            .filter(|t| !t.is_hard())
            .map(|t| t.criterion.meta().weight)
            .sum();
        self.hard_penalty = HARD_PENALTY_PER_WEIGHT * (1.0 + soft_weight);
        for term in &mut self.terms {
            if term.is_hard() {
                term.scale = self.hard_penalty;
                continue;
            }
            term.reference = term.criterion.reference(sizes);
            term.scale = term
                .reference
                .span()
                .map_or(0.0, |span| term.criterion.meta().weight / span);
        }
    }

    pub fn terms(&self) -> &[Term] {
        &self.terms
    }

    /// Costo de violar una restricción dura una vez.
    pub fn hard_penalty(&self) -> f64 {
        self.hard_penalty
    }
}
