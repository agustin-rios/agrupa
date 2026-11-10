//! Recocido simulado: parte de un reparto al azar que respeta las restricciones duras
//! y prueba intercambios (y movimientos, si los tamaños lo permiten) entre grupos.

use crate::config::GroupsConfig;
use crate::model::{EdgeKind, Model};
use crate::state::State;
use anyhow::{Result, bail};
use rand::{Rng, SeedableRng, rngs::StdRng, seq::SliceRandom};
use std::cmp::Reverse;

/// Cantidad de grupos y tamaños permitidos.
#[derive(Debug, Clone)]
pub struct Layout {
    pub k: usize,
    /// Tamaños objetivo (parejos), de mayor a menor.
    pub sizes: Vec<usize>,
    pub min: usize,
    pub max: usize,
}

impl Layout {
    pub fn new(n: usize, g: &GroupsConfig) -> Result<Layout> {
        if n == 0 {
            bail!("no hay alumnos");
        }
        let k = match (g.count, g.size) {
            (Some(0), _) | (_, Some(0)) => {
                bail!("[groups]: `count` y `size` deben ser mayores que 0")
            }
            (Some(c), _) => c,
            (None, Some(s)) => {
                // Lo más cerca posible de `size`; en empate, grupos más chicos.
                let mut k = ((2 * n + s) / (2 * s)).max(1);
                if let Some(max) = g.max_size.filter(|&m| m > 0) {
                    k = k.max(n.div_ceil(max));
                }
                if let Some(min) = g.min_size.filter(|&m| m > 0) {
                    k = k.min((n / min).max(1));
                }
                k
            }
            (None, None) => bail!("[groups]: define `count` o `size`"),
        };
        if k > n {
            bail!("hay {n} alumnos: no alcanzan para {k} grupos");
        }
        let (q, r) = (n / k, n % k);
        let sizes: Vec<usize> = (0..k).map(|i| q + usize::from(i < r)).collect();
        let hi = q + usize::from(r > 0);
        let min = g.min_size.unwrap_or(q);
        let max = g.max_size.unwrap_or(hi);
        if min > max {
            bail!("[groups]: min_size ({min}) es mayor que max_size ({max})");
        }
        if q < min || hi > max {
            bail!(
                "con {n} alumnos en {k} grupos los tamaños quedan entre {q} y {hi}, fuera de min_size/max_size ({min}–{max})"
            );
        }
        Ok(Layout { k, sizes, min, max })
    }

    /// Tamaños parejos para `n` alumnos en `k` grupos (para calibrar).
    pub fn even_sizes(n: usize, k: usize) -> Vec<usize> {
        let (q, r) = (n / k.max(1), n % k.max(1));
        (0..k).map(|i| q + usize::from(i < r)).collect()
    }

    fn flexible(&self) -> bool {
        self.min < self.max
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Params {
    pub iterations: usize,
    pub restarts: usize,
    pub seed: u64,
}

/// Busca la mejor asignación. Devuelve el grupo de cada alumno (0..k), numerados
/// según el primer alumno de la lista que aparece en cada uno.
pub fn solve(m: &Model, layout: &Layout, p: &Params) -> Result<Vec<u32>> {
    check_feasible(m, layout)?;
    let restarts = p.restarts.max(1);
    let workers = std::thread::available_parallelism()
        .map_or(1, |n| n.get())
        .min(restarts);
    let mut results: Vec<(usize, Result<Outcome>)> = Vec::new();
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|w| {
                scope.spawn(move || {
                    (w..restarts)
                        .step_by(workers)
                        .map(|r| (r, run(m, layout, p.iterations, restart_seed(p.seed, r))))
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        for h in handles {
            results.extend(h.join().expect("un hilo del solver falló"));
        }
    });
    results.sort_by_key(|(r, _)| *r);
    let mut best: Option<Outcome> = None;
    for (_, res) in results {
        let (cost, assign) = res?;
        if best.as_ref().is_none_or(|(b, _)| cost < *b - 1e-12) {
            best = Some((cost, assign));
        }
    }
    let (_, assign) = best.expect("al menos un reinicio");
    Ok(canonical(&assign, layout.k))
}

/// Costo final y asignación de un reinicio.
type Outcome = (f64, Vec<u32>);

fn restart_seed(seed: u64, r: usize) -> u64 {
    seed.wrapping_add((r as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
}

fn check_feasible(m: &Model, layout: &Layout) -> Result<()> {
    for b in &m.blocks {
        if b.len() > layout.max {
            let ids: Vec<&str> = b.iter().map(|&i| m.ids[i].as_str()).collect();
            bail!(
                "[constraints] together: {} alumnos deben ir juntos ({}) pero el máximo por grupo es {}",
                b.len(),
                ids.join(", "),
                layout.max
            );
        }
    }
    Ok(())
}

/// Renumera los grupos según el primer alumno que aparece en cada uno.
fn canonical(assign: &[u32], k: usize) -> Vec<u32> {
    let mut map = vec![u32::MAX; k];
    let mut next = 0;
    for &g in assign {
        if map[g as usize] == u32::MAX {
            map[g as usize] = next;
            next += 1;
        }
    }
    for slot in map.iter_mut().filter(|s| **s == u32::MAX) {
        *slot = next;
        next += 1;
    }
    assign.iter().map(|&g| map[g as usize]).collect()
}

/// Un reinicio completo: reparto inicial, recocido y pulido final.
fn run(m: &Model, layout: &Layout, iterations: usize, seed: u64) -> Result<Outcome> {
    let mut rng = StdRng::seed_from_u64(seed);
    let init = initial(m, layout, &mut rng)?;
    let mut st = State::new(m, layout.k, init);
    if layout.k > 1 && iterations > 0 {
        anneal(&mut st, layout, iterations, &mut rng);
    }
    polish(&mut st, layout);
    // Recalcula desde cero para comparar reinicios sin error acumulado.
    let st = State::new(m, layout.k, st.group_of);
    Ok((st.total(), st.group_of))
}

/// Reparto al azar: bloques grandes primero, cada uno al grupo con más espacio
/// donde no choque con una restricción `apart`.
fn initial(m: &Model, layout: &Layout, rng: &mut StdRng) -> Result<Vec<u32>> {
    let k = layout.k;
    let mut order: Vec<usize> = (0..m.blocks.len()).collect();
    order.shuffle(rng);
    order.sort_by_key(|&b| Reverse(m.blocks[b].len()));
    let mut assign = vec![u32::MAX; m.n()];
    let mut size = vec![0usize; k];
    let mut groups: Vec<usize> = (0..k).collect();
    for b in order {
        let block = &m.blocks[b];
        let s = block.len();
        groups.shuffle(rng);
        let conflicts = |g: usize, assign: &[u32]| {
            block
                .iter()
                .flat_map(|&i| m.adj[i].iter().map(move |&e| (i, &m.edges[e as usize])))
                .filter(|(i, e)| {
                    let other = if e.a == *i { e.b } else { e.a };
                    e.kind == EdgeKind::Apart && assign[other] == g as u32
                })
                .count()
        };
        let pick = |limit: &dyn Fn(usize) -> usize, assign: &[u32]| {
            groups
                .iter()
                .copied()
                .filter(|&g| size[g] + s <= limit(g))
                .min_by_key(|&g| (conflicts(g, assign), Reverse(limit(g) - size[g])))
        };
        let g = pick(&|g| layout.sizes[g], &assign)
            .or_else(|| pick(&|_| layout.max, &assign))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "no pude acomodar el bloque de {s} alumnos que deben ir juntos ({}); prueba con min_size/max_size más amplios",
                    block.iter().map(|&i| m.ids[i].as_str()).collect::<Vec<_>>().join(", ")
                )
            })?;
        for &i in block {
            assign[i] = g as u32;
        }
        size[g] += s;
    }
    if let Some(g) = (0..k).find(|&g| size[g] < layout.min) {
        bail!(
            "no pude armar grupos de al menos {} alumnos respetando los bloques `together` (el grupo {} quedó con {})",
            layout.min,
            g + 1,
            size[g]
        );
    }
    Ok(assign)
}

/// Propone un cambio en `moves`. Devuelve `false` si no encontró uno válido.
fn propose(st: &State, layout: &Layout, rng: &mut StdRng, moves: &mut Vec<(usize, u32)>) -> bool {
    let m = st.m;
    moves.clear();
    let i = rng.gen_range(0..m.n());
    let g = st.group_of[i] as usize;
    let bi = &m.blocks[m.block_of[i]];
    let mut h = rng.gen_range(0..layout.k - 1);
    if h >= g {
        h += 1;
    }
    if layout.flexible()
        && rng.gen_bool(0.3)
        && st.sizes[g] - bi.len() >= layout.min
        && st.sizes[h] + bi.len() <= layout.max
    {
        moves.extend(bi.iter().map(|&s| (s, h as u32)));
        return true;
    }
    let Some(&j) = st.members[h].choose(rng) else {
        return false;
    };
    let j = j as usize;
    let bj = &m.blocks[m.block_of[j]];
    match (bi.len(), bj.len()) {
        (a, b) if a == b => {
            moves.extend(bi.iter().map(|&s| (s, h as u32)));
            moves.extend(bj.iter().map(|&s| (s, g as u32)));
            true
        }
        (_, 1) => swap_block_for_singles(st, bi, g, h, rng, moves),
        (1, _) => swap_block_for_singles(st, bj, h, g, rng, moves),
        _ => false,
    }
}

/// Intercambia un bloque de `from` por la misma cantidad de alumnos sueltos de `to`.
fn swap_block_for_singles(
    st: &State,
    block: &[usize],
    from: usize,
    to: usize,
    rng: &mut StdRng,
    moves: &mut Vec<(usize, u32)>,
) -> bool {
    let m = st.m;
    let mut singles: Vec<usize> = Vec::with_capacity(block.len());
    for _ in 0..4 * block.len() {
        if singles.len() == block.len() {
            break;
        }
        let Some(&c) = st.members[to].choose(rng) else {
            return false;
        };
        let c = c as usize;
        if m.blocks[m.block_of[c]].len() == 1 && !singles.contains(&c) {
            singles.push(c);
        }
    }
    if singles.len() < block.len() {
        return false;
    }
    moves.extend(block.iter().map(|&s| (s, to as u32)));
    moves.extend(singles.iter().map(|&s| (s, from as u32)));
    true
}

fn anneal(st: &mut State, layout: &Layout, iterations: usize, rng: &mut StdRng) {
    let mut moves = Vec::new();
    let mut undo = Vec::new();

    // Temperatura inicial: el empeoramiento típico de un cambio al azar.
    let mut ups = Vec::new();
    for _ in 0..200 {
        if propose(st, layout, rng, &mut moves) {
            undo.clear();
            undo.extend(moves.iter().map(|&(s, _)| (s, st.group_of[s])));
            let d = st.apply(&moves);
            st.apply(&undo);
            if d > 1e-12 && d < st.m.hard_penalty / 2.0 {
                ups.push(d);
            }
        }
    }
    let t0 = if ups.is_empty() {
        1e-3
    } else {
        ups.iter().sum::<f64>() / ups.len() as f64
    };
    let t_end = t0 * 1e-3;
    let alpha = (t_end / t0).powf(1.0 / iterations as f64);

    let mut t = t0;
    let mut cur = st.total();
    let mut best = cur;
    let mut best_assign = st.group_of.clone();
    for _ in 0..iterations {
        if propose(st, layout, rng, &mut moves) {
            undo.clear();
            undo.extend(moves.iter().map(|&(s, _)| (s, st.group_of[s])));
            let d = st.apply(&moves);
            if d <= 0.0 || rng.r#gen::<f64>() < (-d / t).exp() {
                cur += d;
                if cur < best - 1e-12 {
                    best = cur;
                    best_assign.clone_from(&st.group_of);
                }
            } else {
                st.apply(&undo);
            }
        }
        t *= alpha;
    }
    // Vuelve a la mejor asignación encontrada.
    let back: Vec<(usize, u32)> = best_assign
        .iter()
        .enumerate()
        .filter(|&(i, &g)| st.group_of[i] != g)
        .map(|(i, &g)| (i, g))
        .collect();
    st.apply(&back);
}

/// Búsqueda local: prueba todos los intercambios (y movimientos) de alumnos sueltos
/// hasta que ninguno mejore.
fn polish(st: &mut State, layout: &Layout) {
    let m = st.m;
    let n = m.n();
    if layout.k < 2 || n > 1500 {
        return;
    }
    let single: Vec<bool> = (0..n).map(|i| m.blocks[m.block_of[i]].len() == 1).collect();
    for _ in 0..20 {
        let mut improved = false;
        for i in (0..n).filter(|&i| single[i]) {
            for j in (i + 1..n).filter(|&j| single[j]) {
                let (gi, gj) = (st.group_of[i], st.group_of[j]);
                if gi == gj {
                    continue;
                }
                if st.apply(&[(i, gj), (j, gi)]) < -1e-12 {
                    improved = true;
                } else {
                    st.apply(&[(i, gi), (j, gj)]);
                }
            }
            if layout.flexible() {
                for h in 0..layout.k as u32 {
                    let g = st.group_of[i];
                    if h == g
                        || st.sizes[g as usize] <= layout.min
                        || st.sizes[h as usize] >= layout.max
                    {
                        continue;
                    }
                    if st.apply(&[(i, h)]) < -1e-12 {
                        improved = true;
                    } else {
                        st.apply(&[(i, g)]);
                    }
                }
            }
        }
        if !improved {
            break;
        }
    }
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
    fn layout_from_size() {
        // 30 de a 4: 8 grupos (6 de 4 y 2 de 3), no 7 con grupos de 5.
        let l = Layout::new(30, &groups(None, Some(4), None, None)).unwrap();
        assert_eq!(l.k, 8);
        assert_eq!(l.sizes.iter().filter(|&&s| s == 4).count(), 6);
        assert_eq!((l.min, l.max), (3, 4));
        // 21 de a 4: 5 grupos (uno de 5) es más cercano que 6 (tres de 3).
        assert_eq!(
            Layout::new(21, &groups(None, Some(4), None, None))
                .unwrap()
                .k,
            5
        );
        // Con max_size = 4 no puede haber grupos de 5.
        assert_eq!(
            Layout::new(21, &groups(None, Some(4), None, Some(4)))
                .unwrap()
                .k,
            6
        );
    }

    #[test]
    fn layout_rejects_impossible_bounds() {
        assert!(Layout::new(10, &groups(Some(3), None, Some(4), None)).is_err());
        assert!(Layout::new(3, &groups(Some(5), None, None, None)).is_err());
    }

    #[test]
    fn canonical_numbers_by_first_member() {
        assert_eq!(canonical(&[2, 0, 2, 1], 3), vec![0, 1, 0, 2]);
    }
}
