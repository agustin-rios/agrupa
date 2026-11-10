//! Distribución hipergeométrica: cuántos alumnos con cierto valor caen en un grupo
//! armado al azar (extracciones sin reposición).

pub struct Hypergeometric {
    population: usize,
    /// ln(k!) para k = 0..=población.
    ln_factorial: Vec<f64>,
}

impl Hypergeometric {
    pub fn new(population: usize) -> Self {
        let mut ln_factorial = vec![0.0; population + 1];
        for k in 1..=population {
            ln_factorial[k] = ln_factorial[k - 1] + (k as f64).ln();
        }
        Hypergeometric {
            population,
            ln_factorial,
        }
    }

    /// P(X = x) para x = 0..=draws, al sacar `draws` alumnos cuando `successes` tienen el valor.
    pub fn pmf(&self, successes: usize, draws: usize) -> Vec<f64> {
        let n = self.population;
        (0..=draws)
            .map(|x| {
                if x > successes || draws - x > n - successes {
                    return 0.0;
                }
                let ln_p = self.ln_choose(successes, x) + self.ln_choose(n - successes, draws - x)
                    - self.ln_choose(n, draws);
                ln_p.exp()
            })
            .collect()
    }

    /// Probabilidad de que nadie con el valor caiga en un grupo de `draws`.
    pub fn none(&self, successes: usize, draws: usize) -> f64 {
        self.pmf(successes, draws)[0]
    }

    fn ln_choose(&self, n: usize, k: usize) -> f64 {
        self.ln_factorial[n] - self.ln_factorial[k] - self.ln_factorial[n - k]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pmf_sums_to_one_and_matches_closed_form() {
        let h = Hypergeometric::new(30);
        let p = h.pmf(7, 4);
        assert!((p.iter().sum::<f64>() - 1.0).abs() < 1e-9);
        // P(0) = C(23,4) / C(30,4)
        assert!((h.none(7, 4) - 8855.0 / 27405.0).abs() < 1e-9);
    }
}
