//! Reparto inicial al azar que respeta los bloques `together` y evita choques `apart`.

use super::layout::Layout;
use crate::partition::Partition;
use crate::problem::Problem;
use anyhow::{Result, anyhow, bail};
use rand::{rngs::StdRng, seq::SliceRandom};
use std::cmp::Reverse;

/// Ubica los bloques, de mayor a menor, cada uno en el grupo donde menos choca con
/// restricciones `apart`; en empate, en el que tiene más espacio.
pub fn random_partition(problem: &Problem, layout: &Layout, rng: &mut StdRng) -> Result<Partition> {
    let blocks = problem.relations.blocks.all();
    let mut order: Vec<usize> = (0..blocks.len()).collect();
    order.shuffle(rng);
    order.sort_by_key(|&b| Reverse(blocks[b].len()));

    let mut groups: Vec<Option<usize>> = vec![None; problem.student_count()];
    let mut sizes = vec![0; layout.group_count];
    let mut candidates: Vec<usize> = (0..layout.group_count).collect();
    for b in order {
        let block = &blocks[b];
        candidates.shuffle(rng);
        let placement = Placement {
            problem,
            block,
            groups: &groups,
            sizes: &sizes,
        };
        let group = placement
            .best_group(&candidates, |g| layout.target_sizes[g])
            .or_else(|| placement.best_group(&candidates, |_| layout.max_size))
            .ok_or_else(|| {
                let ids: Vec<&str> = block.iter().map(|&s| problem.roster.id(s)).collect();
                anyhow!(
                    "no pude acomodar el bloque de {} alumnos que deben ir juntos ({}); prueba con min_size/max_size más amplios",
                    block.len(),
                    ids.join(", ")
                )
            })?;
        for &student in block {
            groups[student] = Some(group);
        }
        sizes[group] += block.len();
    }

    if let Some(group) = (0..layout.group_count).find(|&g| sizes[g] < layout.min_size) {
        bail!(
            "no pude armar grupos de al menos {} alumnos respetando los bloques `together` (el grupo {} quedó con {})",
            layout.min_size,
            group + 1,
            sizes[group]
        );
    }
    Ok(Partition::new(layout.group_count, groups))
}

/// Dónde poner un bloque, dado lo ya ubicado.
struct Placement<'a> {
    problem: &'a Problem,
    block: &'a [usize],
    groups: &'a [Option<usize>],
    sizes: &'a [usize],
}

impl Placement<'_> {
    /// Entre los grupos donde el bloque cabe bajo `capacity`, el de menos choques y más espacio.
    fn best_group(&self, candidates: &[usize], capacity: impl Fn(usize) -> usize) -> Option<usize> {
        candidates
            .iter()
            .copied()
            .filter(|&g| self.sizes[g] + self.block.len() <= capacity(g))
            .min_by_key(|&g| (self.apart_clashes(g), Reverse(capacity(g) - self.sizes[g])))
    }

    /// Cuántos compañeros `apart` del bloque ya están en el grupo.
    fn apart_clashes(&self, group: usize) -> usize {
        self.block
            .iter()
            .flat_map(|&s| self.problem.relations.apart_partners(s))
            .filter(|&&other| self.groups[other] == Some(group))
            .count()
    }
}
