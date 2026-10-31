//! "Did you mean": the nearest known name to a mistyped one.

/// The candidate closest to `name` by edit distance, if it is close enough to
/// be a plausible typo (at most two edits, and fewer than half the letters).
pub fn nearest<'a, I>(name: &str, candidates: I) -> Option<String>
where
    I: IntoIterator<Item = &'a str>,
{
    let limit = 2.min(name.chars().count().div_ceil(2));
    candidates
        .into_iter()
        .filter(|c| *c != name)
        .map(|c| (distance(name, c), c))
        .filter(|(d, _)| *d <= limit)
        .min_by_key(|(d, c)| (*d, c.len()))
        .map(|(_, c)| c.to_string())
}

/// Levenshtein distance, counting a swap of two neighbours as one edit.
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typos_find_their_name() {
        assert_eq!(nearest("sqtr", ["sqrt", "sin"]), Some("sqrt".into()));
        assert_eq!(nearest("slop", ["slope", "x"]), Some("slope".into()));
        assert_eq!(nearest("colour", ["color"]), Some("color".into()));
    }

    #[test]
    fn unrelated_names_find_nothing() {
        assert_eq!(nearest("x", ["slope"]), None);
        assert_eq!(nearest("ab", ["xy"]), None);
    }
}
