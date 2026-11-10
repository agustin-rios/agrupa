//! Configuración en TOML: qué columnas usar, con qué objetivo y con qué límites.

use anyhow::{Context, Result, bail};
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Columna con el identificador único de cada alumno.
    pub id_column: String,
    /// Columna con el nombre visible (opcional; si falta se usa el id).
    #[serde(default)]
    pub name_column: Option<String>,
    /// Separador del CSV: "," ";" o "tab". Si se omite, se detecta solo.
    #[serde(default)]
    pub delimiter: Option<String>,
    pub groups: GroupsConfig,
    #[serde(default)]
    pub attributes: Vec<AttributeConfig>,
    #[serde(default)]
    pub preferences: Option<PreferencesConfig>,
    #[serde(default)]
    pub constraints: ConstraintsConfig,
    #[serde(default)]
    pub solver: SolverConfig,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GroupsConfig {
    /// Cantidad de grupos.
    pub count: Option<usize>,
    /// Alumnos por grupo (objetivo). Se usa uno de los dos: `count` o `size`.
    pub size: Option<usize>,
    pub min_size: Option<usize>,
    pub max_size: Option<usize>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    #[serde(alias = "numerico", alias = "numérico")]
    Numeric,
    #[serde(alias = "categorico", alias = "categórico")]
    Categorical,
    #[serde(alias = "etiquetas")]
    Tags,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Goal {
    /// Numérico: promedios parejos entre grupos (y alta variedad dentro de cada uno).
    #[serde(alias = "balancear")]
    Balance,
    /// Numérico: valores parecidos dentro de cada grupo.
    #[serde(alias = "homogeneo", alias = "homogéneo")]
    Similar,
    /// Categórico/etiquetas: cada valor repartido en proporción entre los grupos.
    #[serde(alias = "repartir")]
    Spread,
    /// Categórico: mismo valor dentro del grupo (p. ej. horario o sección).
    #[serde(alias = "juntar")]
    Together,
    /// Categórico/etiquetas: que cada grupo tenga la mayor cantidad de valores distintos.
    #[serde(alias = "cubrir")]
    Cover,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Numeric => "numeric",
            Kind::Categorical => "categorical",
            Kind::Tags => "tags",
        }
    }
}

impl Goal {
    pub fn as_str(self) -> &'static str {
        match self {
            Goal::Balance => "balance",
            Goal::Similar => "similar",
            Goal::Spread => "spread",
            Goal::Together => "together",
            Goal::Cover => "cover",
        }
    }

    /// Etiqueta corta en español para los reportes.
    pub fn label_es(self) -> &'static str {
        match self {
            Goal::Balance => "balancear",
            Goal::Similar => "homogéneo",
            Goal::Spread => "repartir",
            Goal::Together => "juntar",
            Goal::Cover => "cubrir",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttributeConfig {
    pub column: String,
    /// Nombre para mostrar en los reportes (por defecto, el de la columna).
    #[serde(default)]
    pub label: Option<String>,
    pub kind: Kind,
    pub goal: Goal,
    /// Importancia relativa. 1.0 = un reparto aleatorio de este atributo "cuesta" 1.
    #[serde(default = "one")]
    pub weight: f64,
    /// Caracteres que separan valores múltiples (etiquetas). Google Forms usa ", ".
    #[serde(default = "default_separators")]
    pub separator: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreferencesConfig {
    /// Columna con ids o nombres de compañeros con quienes quiere trabajar.
    pub prefer_column: Option<String>,
    /// Columna con ids o nombres de compañeros que prefiere evitar.
    pub avoid_column: Option<String>,
    #[serde(default = "one")]
    pub weight: f64,
    #[serde(default = "default_separators")]
    pub separator: String,
}

/// Restricciones duras: listas de ids.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConstraintsConfig {
    /// Cada lista debe quedar en el mismo grupo.
    #[serde(default)]
    pub together: Vec<Vec<String>>,
    /// Los integrantes de cada lista deben quedar en grupos distintos.
    #[serde(default)]
    pub apart: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, default)]
pub struct SolverConfig {
    pub iterations: usize,
    pub restarts: usize,
    pub seed: u64,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self { iterations: 200_000, restarts: 8, seed: 42 }
    }
}

fn one() -> f64 {
    1.0
}

fn default_separators() -> String {
    ";,".to_string()
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("No pude leer la configuración {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("Configuración inválida en {}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Config> {
        let cfg: Config = toml::from_str(text)?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<()> {
        match (self.groups.count, self.groups.size) {
            (Some(_), None) | (None, Some(_)) => {}
            _ => bail!("en [groups] define exactamente uno: `count` (cantidad de grupos) o `size` (alumnos por grupo)"),
        }
        if self.attributes.is_empty() && self.preferences.is_none() {
            bail!("define al menos un atributo en [[attributes]]");
        }
        for a in &self.attributes {
            let ok = matches!(
                (a.kind, a.goal),
                (Kind::Numeric, Goal::Balance | Goal::Similar)
                    | (Kind::Categorical, Goal::Spread | Goal::Together | Goal::Cover)
                    | (Kind::Tags, Goal::Spread | Goal::Cover)
            );
            if !ok {
                bail!(
                    "atributo '{}': el objetivo `{}` no aplica a `{}` (numeric: balance|similar · categorical: spread|together|cover · tags: spread|cover)",
                    a.column,
                    a.goal.as_str(),
                    a.kind.as_str()
                );
            }
            if !(a.weight.is_finite() && a.weight >= 0.0) {
                bail!("atributo '{}': `weight` debe ser un número ≥ 0", a.column);
            }
        }
        if let Some(p) = &self.preferences
            && p.prefer_column.is_none()
            && p.avoid_column.is_none()
        {
            bail!("[preferences] necesita `prefer_column`, `avoid_column` o ambos");
        }
        Ok(())
    }

    pub fn delimiter(&self) -> Result<Option<u8>> {
        Ok(match self.delimiter.as_deref() {
            None => None,
            Some(",") => Some(b','),
            Some(";") => Some(b';'),
            Some("tab") | Some("\t") => Some(b'\t'),
            Some(other) => bail!("delimiter '{other}' no soportado; usa \",\", \";\" o \"tab\""),
        })
    }
}
