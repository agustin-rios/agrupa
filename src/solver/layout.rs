//! Cuántos grupos armar y de qué tamaños.

use crate::config::GroupsConfig;
use anyhow::{Result, bail};

#[derive(Debug, Clone)]
pub struct Layout {
    pub group_count: usize,
    /// Tamaños parejos de partida, de mayor a menor.
    pub target_sizes: Vec<usize>,
    pub min_size: usize,
    pub max_size: usize,
}

impl Layout {
    pub fn new(students: usize, cfg: &GroupsConfig) -> Result<Layout> {
        if students == 0 {
            bail!("no hay alumnos");
        }
        let group_count = group_count(students, cfg)?;
        if group_count > students {
            bail!("hay {students} alumnos: no alcanzan para {group_count} grupos");
        }
        let target_sizes = even_sizes(students, group_count);
        let (smallest, largest) = (*target_sizes.last().unwrap(), target_sizes[0]);
        let min_size = cfg.min_size.unwrap_or(smallest);
        let max_size = cfg.max_size.unwrap_or(largest);
        if min_size > max_size {
            bail!("[groups]: min_size ({min_size}) es mayor que max_size ({max_size})");
        }
        if smallest < min_size || largest > max_size {
            bail!(
                "con {students} alumnos en {group_count} grupos los tamaños quedan entre {smallest} y {largest}, fuera de min_size/max_size ({min_size}–{max_size})"
            );
        }
        Ok(Layout {
            group_count,
            target_sizes,
            min_size,
            max_size,
        })
    }

    /// Si los tamaños pueden variar, el solver también prueba mover alumnos sin contraparte.
    pub fn sizes_can_vary(&self) -> bool {
        self.min_size < self.max_size
    }
}

/// `count` si está; si no, lo más cerca posible de `size` (en empate, grupos más chicos),
/// dentro de `min_size` y `max_size`.
fn group_count(students: usize, cfg: &GroupsConfig) -> Result<usize> {
    match (cfg.count, cfg.size) {
        (Some(0), _) | (_, Some(0)) => bail!("[groups]: `count` y `size` deben ser mayores que 0"),
        (Some(count), _) => Ok(count),
        (None, Some(size)) => {
            // round(students / size), con 0,5 hacia arriba.
            let mut count = ((2 * students + size) / (2 * size)).max(1);
            if let Some(max) = cfg.max_size.filter(|&m| m > 0) {
                count = count.max(students.div_ceil(max));
            }
            if let Some(min) = cfg.min_size.filter(|&m| m > 0) {
                count = count.min((students / min).max(1));
            }
            Ok(count)
        }
        (None, None) => bail!("[groups]: define `count` o `size`"),
    }
}

/// Tamaños lo más parejos posible: los primeros grupos llevan uno más.
pub fn even_sizes(students: usize, group_count: usize) -> Vec<usize> {
    let group_count = group_count.max(1);
    let (base, extra) = (students / group_count, students % group_count);
    (0..group_count)
        .map(|g| base + usize::from(g < extra))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn groups(
        count: Option<usize>,
        size: Option<usize>,
        min: Option<usize>,
        max: Option<usize>,
    ) -> GroupsConfig {
        GroupsConfig {
            count,
            size,
            min_size: min,
            max_size: max,
        }
    }

    #[test]
    fn size_picks_the_closest_count() {
        // 30 de a 4: 8 grupos (6 de 4 y 2 de 3) en vez de 7 con grupos de 5.
        let layout = Layout::new(30, &groups(None, Some(4), None, None)).unwrap();
        assert_eq!(layout.group_count, 8);
        assert_eq!(layout.target_sizes.iter().filter(|&&s| s == 4).count(), 6);
        assert_eq!((layout.min_size, layout.max_size), (3, 4));
        // 21 de a 4: 5 grupos (uno de 5) es más cercano que 6 (tres de 3).
        assert_eq!(
            Layout::new(21, &groups(None, Some(4), None, None))
                .unwrap()
                .group_count,
            5
        );
        // Con max_size = 4 no puede haber grupos de 5.
        assert_eq!(
            Layout::new(21, &groups(None, Some(4), None, Some(4)))
                .unwrap()
                .group_count,
            6
        );
    }

    #[test]
    fn rejects_impossible_bounds() {
        assert!(Layout::new(10, &groups(Some(3), None, Some(4), None)).is_err());
        assert!(Layout::new(3, &groups(Some(5), None, None, None)).is_err());
    }
}
