//! agrupa: forma grupos balanceados a partir de perfiles de alumnos.
//!
//! Recorrido de los datos:
//! - `data`, `config`: leen el CSV y el TOML.
//! - `roster`, `relations`: quiénes son los alumnos y qué relaciones tienen.
//! - `criteria`: cada criterio de calidad, con su costo, referencias y explicaciones.
//! - `objective`, `partition`, `state`: qué se minimiza, quién está dónde y el costo al día.
//! - `problem`: junta todo lo anterior.
//! - `solver`: busca la mejor partición; `suggest`: ubica a alumnos nuevos.
//! - `report`, `assignment`, `commands`: salidas y la CLI.

pub mod assignment;
pub mod commands;
pub mod config;
pub mod criteria;
pub mod data;
pub mod objective;
pub mod partition;
pub mod problem;
pub mod relations;
pub mod report;
pub mod roster;
pub mod solver;
pub mod state;
pub mod suggest;
pub mod text;
