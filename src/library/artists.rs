//! Parsing and matching of artist credit strings.
//!
//! Tags often store several artists in one field (`A / B`, `A, B`, `A - B`,
//! `A feat. B`). The library lists each artist separately and matches names
//! case-insensitively, while deliberately leaving names such as `AC/DC`,
//! `Jay-Z` and `Earth, Wind & Fire` intact.

use std::collections::HashSet;

/// Internal marker used while normalising separators. It cannot appear in a
/// real tag value, so splitting on it is unambiguous.
const SEPARATOR: char = '\u{1}';

/// Credits that look like several names but belong to one act.
const SINGLE_ACT_EXCEPTIONS: &[&str] = &[
    "ac/dc",
    "crosby, stills & nash",
    "crosby, stills, nash & young",
    "earth, wind & fire",
    "tyler, the creator",
];

/// Case-insensitive identity of an artist name.
pub fn key(name: &str) -> String {
    name.trim().to_lowercase()
}

/// First artist named by a credit, preserving its original spelling.
pub fn primary(credit: &str) -> String {
    split_credit(credit)
        .into_iter()
        .next()
        .unwrap_or_else(|| credit.trim().to_string())
}

/// Split an artist credit into the individual names it contains.
///
/// - `/`, `,`, `;`, `|` and spaced dashes separate artists.
/// - Hyphens inside a name (`Jay-Z`) and short slash names (`AC/DC`) are kept.
/// - `feat.` / `ft.` / `featuring` are treated as separators.
/// - Commas are not split when the credit contains `&`, which keeps group
///   names such as `Earth, Wind & Fire` together.
pub fn split_credit(credit: &str) -> Vec<String> {
    let credit = credit.trim();
    if credit.is_empty() {
        return Vec::new();
    }
    if SINGLE_ACT_EXCEPTIONS.contains(&credit.to_lowercase().as_str()) {
        return vec![credit.to_string()];
    }

    let mut text = credit.to_string();
    for dash in [" - ", " – ", " — "] {
        text = text.replace(dash, &SEPARATOR.to_string());
    }
    // A slash touching whitespace is always a separator, even when either
    // side is short ("A / B").
    text = text.replace(" /", &SEPARATOR.to_string());
    text = text.replace("/ ", &SEPARATOR.to_string());

    // Normalise featuring credits into the same marker. The search is ASCII
    // case-insensitive but keeps byte offsets into the original string.
    loop {
        let mut found: Option<(usize, usize)> = None;
        for marker in [" feat. ", " feat ", " ft. ", " ft ", " featuring "] {
            if let Some(position) = find_ascii_case_insensitive(&text, marker) {
                if found.map_or(true, |(best, _)| position < best) {
                    found = Some((position, marker.len()));
                }
            }
        }
        let Some((position, length)) = found else {
            break;
        };
        text.replace_range(position..position + length, &SEPARATOR.to_string());
    }

    // Keep comma-separated group names such as "Earth, Wind & Fire" intact.
    if !text.contains('&') {
        text = text.replace(',', &SEPARATOR.to_string());
    }

    let mut names = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for chunk in text.split(SEPARATOR) {
        for part in chunk.split([';', '|']) {
            for name in split_slashes(part) {
                let name = name.trim().to_string();
                if name.is_empty() || !seen.insert(key(&name)) {
                    continue;
                }
                names.push(name);
            }
        }
    }
    names
}

/// Split a slash-separated segment unless one side is too short to be a real
/// artist name, which protects names such as `AC/DC`.
fn split_slashes(segment: &str) -> Vec<String> {
    let Some(position) = segment.find('/') else {
        return vec![segment.to_string()];
    };
    let left = segment[..position].trim();
    let right = segment[position + 1..].trim();
    if left.chars().count() > 2 && right.chars().count() > 2 {
        let mut names = split_slashes(left);
        names.extend(split_slashes(right));
        names
    } else {
        vec![segment.to_string()]
    }
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    haystack.char_indices().find_map(|(index, _)| {
        haystack[index..]
            .get(..needle.len())
            .filter(|candidate| candidate.eq_ignore_ascii_case(needle))
            .map(|_| index)
    })
}

#[cfg(test)]
mod tests {
    use super::{key, primary, split_credit};

    #[test]
    fn splits_common_separators() {
        assert_eq!(split_credit("A / B, C; D | E"), ["A", "B", "C", "D", "E"]);
        assert_eq!(split_credit("A/B"), ["A/B"]);
        assert_eq!(split_credit("Artist One/Artist Two"), ["Artist One", "Artist Two"]);
        assert_eq!(split_credit("A - B – C — D"), ["A", "B", "C", "D"]);
        assert_eq!(split_credit("A feat. B ft. C featuring D"), ["A", "B", "C", "D"]);
    }

    #[test]
    fn keeps_single_act_names_intact() {
        assert_eq!(split_credit("AC/DC"), ["AC/DC"]);
        assert_eq!(split_credit("Jay-Z"), ["Jay-Z"]);
        assert_eq!(split_credit("Earth, Wind & Fire"), ["Earth, Wind & Fire"]);
        assert_eq!(split_credit("Tyler, The Creator"), ["Tyler, The Creator"]);
    }

    #[test]
    fn merges_duplicate_credits_case_insensitively() {
        assert_eq!(split_credit("Daft Punk / daft punk"), ["Daft Punk"]);
        assert_eq!(key(" Daft PUNK "), "daft punk");
        assert_eq!(primary("Daft Punk / Pharrell"), "Daft Punk");
    }

    #[test]
    fn ignores_empty_input() {
        assert!(split_credit("   ").is_empty());
        assert!(split_credit(" , ; | ").is_empty());
    }
}
