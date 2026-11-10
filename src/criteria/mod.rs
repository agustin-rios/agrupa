//! Criterios de calidad.
//!
//! Cada criterio sabe tres cosas:
//! - cuánto cuesta una partición y cómo cambia ese costo al mover alumnos (su `Tracker`);
//! - sus valores de referencia: el ideal y el esperado en un reparto al azar;
//! - explicar en palabras cómo quedó cada grupo y qué aporta sumar a alguien.
//!
//! Los criterios de columnas (balance, similar, spread, together, cover) se suman por
//! grupo y comparten la maquinaria de `groupwise`. Las preferencias y la restricción
//! `apart` se miden por pares de alumnos (`pairs`).
//!
//! Para agregar un criterio: un archivo nuevo que implemente `GroupwiseCriterion`
//! (o `Criterion`, si no se suma por grupo) y una línea en `from_attribute`.

mod apart;
mod balance;
mod cover;
mod groupwise;
mod labels;
mod numeric;
mod pairs;
mod preferences;
mod similar;
mod spread;
mod stats;
mod together;

pub use apart::Apart;
pub use preferences::Preferences;

use crate::config::{AttributeConfig, Config, Goal, Kind};
use crate::data::Table;
use crate::partition::Partition;
use crate::roster::Roster;
use anyhow::{Result, bail};
use balance::Balance;
use cover::Cover;
use labels::LabelColumn;
use numeric::NumericColumn;
use similar::Similar;
use spread::Spread;
use together::Together;

/// Nombre, objetivo y peso de un criterio, tal como se muestran en los reportes.
#[derive(Debug, Clone)]
pub struct Meta {
    pub label: String,
    /// Objetivo en palabras: "balancear", "repartir"...
    pub goal: &'static str,
    pub weight: f64,
}

/// Valores de referencia de un criterio: el mejor posible y el esperado al azar.
#[derive(Debug, Clone, Copy, Default)]
pub struct Reference {
    pub ideal: f64,
    pub random: f64,
}

impl Reference {
    /// Bajo esta diferencia, el criterio no tiene variación posible.
    const MIN_SPAN: f64 = 1e-9;

    /// Distancia entre "al azar" e "ideal"; `None` si es despreciable.
    pub fn span(&self) -> Option<f64> {
        let span = self.random - self.ideal;
        (span > Self::MIN_SPAN).then_some(span)
    }

    /// 1 = ideal, 0 = como al azar; negativo si es peor que al azar.
    pub fn quality(&self, cost: f64) -> f64 {
        match self.span() {
            Some(span) => (1.0 - (cost - self.ideal) / span).min(1.0),
            None => 1.0,
        }
    }
}

/// Un alumno que cambió de grupo.
#[derive(Debug, Clone, Copy)]
pub struct Step {
    pub student: usize,
    pub from: Option<usize>,
    pub to: Option<usize>,
}

/// Un cambio ya aplicado a la partición: los pasos y los grupos que tocaron.
pub struct Change<'a> {
    pub steps: &'a [Step],
    pub groups: &'a [usize],
}

impl Change<'_> {
    /// Grupo que tenía el alumno antes del cambio.
    pub fn group_before(&self, student: usize, partition: &Partition) -> Option<usize> {
        match self.steps.iter().find(|step| step.student == student) {
            Some(step) => step.from,
            None => partition.group_of(student),
        }
    }

    pub fn moved(&self, student: usize) -> bool {
        self.steps.iter().any(|step| step.student == student)
    }
}

/// Lo necesario para escribir explicaciones con nombres de alumnos y de grupos.
pub struct Context<'a> {
    pub partition: &'a Partition,
    pub roster: &'a Roster,
    pub group_labels: &'a [String],
}

/// Un criterio de calidad: datos fijos, referencias y cómo seguir su costo.
pub trait Criterion: Send + Sync {
    fn meta(&self) -> &Meta;

    /// Las restricciones duras no se escalan por su referencia ni se reportan como calidad.
    fn is_hard(&self) -> bool {
        false
    }

    /// Ideal y valor esperado al azar para grupos con estos tamaños.
    fn reference(&self, sizes: &[usize]) -> Reference;

    /// Empieza a seguir el costo de este criterio sobre una partición.
    fn tracker<'a>(&'a self, partition: &Partition) -> Box<dyn Tracker + 'a>;
}

/// El costo de un criterio sobre una partición concreta, al día tras cada cambio.
pub trait Tracker {
    /// Costo actual, sin escalar.
    fn cost(&self) -> f64;

    /// Ajusta el costo a un cambio que la partición ya refleja. Devuelve cuánto cambió.
    fn update(&mut self, change: &Change, partition: &Partition) -> f64;

    /// Resumen para el reporte: "promedios de grupo entre 5.30 y 5.50 (global 5.41)".
    fn describe(&self, ctx: &Context, reference: &Reference) -> String;

    /// Resumen de un grupo: "género F ×2, M ×2". `None` si el criterio no tiene uno.
    fn describe_group(&self, ctx: &Context, group: usize) -> Option<String>;

    /// Qué pasa con este criterio si `students` (sin grupo) se suman a `group`.
    fn explain_join(&self, ctx: &Context, students: &[usize], group: usize) -> Option<String>;
}

/// Un criterio por cada `[[attributes]]` de la configuración, en el mismo orden.
pub fn from_attributes(
    table: &Table,
    cfg: &Config,
    roster: &Roster,
    warnings: &mut Vec<String>,
) -> Result<Vec<Box<dyn Criterion>>> {
    cfg.attributes
        .iter()
        .map(|attr| from_attribute(table, attr, roster, warnings))
        .collect()
}

fn from_attribute(
    table: &Table,
    attr: &AttributeConfig,
    roster: &Roster,
    warnings: &mut Vec<String>,
) -> Result<Box<dyn Criterion>> {
    let column = table.column(&attr.column)?;
    let cells: Vec<&str> = table.rows.iter().map(|row| row[column].as_str()).collect();
    let label = attr.label.clone().unwrap_or_else(|| attr.column.clone());
    let meta = Meta {
        label: label.clone(),
        goal: attr.goal.label_es(),
        weight: attr.weight,
    };
    let numeric =
        |warnings: &mut Vec<String>| NumericColumn::read(&label, &cells, roster, warnings);
    let labels = |warnings: &mut Vec<String>| {
        let separator = (attr.kind == Kind::Tags).then_some(attr.separator.as_str());
        LabelColumn::read(&label, &cells, separator, warnings)
    };
    Ok(match (attr.kind, attr.goal) {
        (Kind::Numeric, Goal::Balance) => Box::new(Balance::new(meta, numeric(warnings))),
        (Kind::Numeric, Goal::Similar) => Box::new(Similar::new(meta, numeric(warnings))),
        (Kind::Categorical | Kind::Tags, Goal::Spread) => {
            Box::new(Spread::new(meta, labels(warnings)))
        }
        (Kind::Categorical, Goal::Together) => Box::new(Together::new(meta, labels(warnings))),
        (Kind::Categorical | Kind::Tags, Goal::Cover) => {
            Box::new(Cover::new(meta, labels(warnings)))
        }
        (kind, goal) => bail!(
            "atributo '{}': el objetivo `{}` no aplica a `{}`",
            attr.column,
            goal.as_str(),
            kind.as_str()
        ),
    })
}
