//! Utilidades de texto compartidas por reportes y sugerencias.

/// "s" cuando `n` no es 1, para pluralizar.
pub fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Une una lista en español: "a", "a y b", "a, b y c".
pub fn join_with_and(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [init @ .., last] => format!("{} y {last}", init.join(", ")),
    }
}

/// Un número sin decimales si es entero; si no, con dos.
pub fn format_number(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{}", x.round())
    } else {
        format!("{x:.2}")
    }
}

/// Valores presentes con su conteo, del más al menos frecuente: "F ×2, M".
pub fn format_counts(labels: &[String], counts: &[u32]) -> String {
    let mut present: Vec<(u32, usize)> = counts
        .iter()
        .enumerate()
        .filter(|(_, count)| **count > 0)
        .map(|(value, &count)| (count, value))
        .collect();
    present.sort_by_key(|&(count, value)| (std::cmp::Reverse(count), value));
    present
        .iter()
        .map(|&(count, value)| {
            if count > 1 {
                format!("{} ×{count}", labels[value])
            } else {
                labels[value].clone()
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// Los primeros elementos de una lista larga y cuántos quedan: "a, b, c, d, e y 4 más".
pub fn preview(items: &[String]) -> String {
    const SHOWN: usize = 5;
    let mut text = items
        .iter()
        .take(SHOWN)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ");
    if items.len() > SHOWN {
        text.push_str(&format!(" y {} más", items.len() - SHOWN));
    }
    text
}

/// Reparte ítems en líneas de a lo más `width` caracteres, separados por `separator`.
pub fn wrap(items: &[String], separator: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for item in items {
        let needed = separator.chars().count() + item.chars().count();
        if !line.is_empty() && line.chars().count() + needed > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push_str(separator);
        }
        line.push_str(item);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn joins_in_spanish() {
        assert_eq!(join_with_and(&strings(&["a"])), "a");
        assert_eq!(join_with_and(&strings(&["a", "b", "c"])), "a, b y c");
    }

    #[test]
    fn counts_most_frequent_first() {
        assert_eq!(format_counts(&strings(&["F", "M"]), &[1, 2]), "M ×2, F");
    }

    #[test]
    fn wraps_long_lines() {
        assert_eq!(
            wrap(&strings(&["aaaa", "bbbb", "cccc"]), " · ", 11),
            vec!["aaaa · bbbb", "cccc"]
        );
    }
}
