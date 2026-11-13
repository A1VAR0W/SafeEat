//! Splits an ingredient list into a tree of items, keeping each item tied to its
//! position in the original text.
//!
//! "Patatas seleccionadas (64%), aceite de oliva (35%), sal, dextrosa y aromas naturales"
//! becomes five items: the conjunction "y" separates ingredients too (the AI used to
//! receive "dextrosa y aromas naturales" as one ingredient).
//! A parenthesis after an item lists its ingredients (a compound ingredient), unless it
//! only gives a percentage or the cereal it comes from ("dextrosa (trigo)").

use once_cell::sync::Lazy;
use regex::Regex;
use unicode_normalization::UnicodeNormalization;

/// Lowercase ASCII copy of a text with exactly one byte per original character, so a
/// position in the copy maps back to the original.
pub(crate) struct Folded {
    pub text: String,
    /// Byte offset in the original of each byte of `text` (plus the end).
    map: Vec<usize>,
}

impl Folded {
    pub fn new(original: &str) -> Self {
        let mut text = String::with_capacity(original.len());
        let mut map = Vec::with_capacity(original.len() + 1);
        for (offset, c) in original.char_indices() {
            map.push(offset);
            text.push(fold_char(c));
        }
        map.push(original.len());
        Folded { text, map }
    }

    /// Byte range in the original for a byte range of the folded text.
    pub fn original_range(&self, start: usize, end: usize) -> (usize, usize) {
        (self.map[start], self.map[end])
    }
}

fn fold_char(c: char) -> char {
    if c.is_ascii() {
        return match c {
            '-' | '_' | '\t' | '\n' | '\r' => ' ',
            _ => c.to_ascii_lowercase(),
        };
    }
    match c {
        'ß' => 's',
        'æ' | 'Æ' => 'a',
        'œ' | 'Œ' | 'ø' | 'Ø' => 'o',
        'ł' | 'Ł' => 'l',
        '\u{2010}'..='\u{2015}' => ' ',
        _ => c
            .nfkd()
            .next()
            .filter(char::is_ascii_alphanumeric)
            .map_or(' ', |b| b.to_ascii_lowercase()),
    }
}

/// One ingredient of the list.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Item {
    /// As shown to the user (original text, percentages removed).
    pub display: String,
    /// Folded text used by the rules (own words plus a merged origin).
    pub matcher: String,
    /// Byte range of the whole item (children included) in the original text.
    pub span: (usize, usize),
    pub children: Vec<Item>,
    /// The parenthesis names only something added ("goma base (con antioxidante: ...)"),
    /// not the whole composition: the item's own name still counts.
    pub partial: bool,
}

static PERCENT: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"[<>≤≥]?\s*\d+(?:[.,]\d+)?\s*%|\bmin(?:imo)?\.?\s*$").expect("valid regex")
});
/// A parenthesis that only names the cereal an ingredient comes from.
static ORIGIN_ONLY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:(?:de|from|aus|di|du|do|da|von)\s+)?(?:trigo|wheat|ble|froment|frumento|grano|weizen|cebada|barley|orge|orzo|gerste|centeno|rye|seigle|segale|roggen|maiz|corn|maize|mais|milho|arroz|rice|riz|riso|reis|patata|potato|pomme de terre|kartoffel|tapioca|guisante|pea|avena|oats?|avoine|hafer|aveia|cevada|centeio)\s*$")
        .expect("valid regex")
});
/// A parenthesis that lists only additions, not the composition.
static ADDITION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:con|contiene|contem|com|with|contains|avec|contient|mit|enthalt)\b")
        .expect("valid regex")
});
/// Label prefix ("Ingredientes:") and sentences that are not ingredients.
static PREFIX: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^\s*(?:lista de\s+)?(?:ingredientes|ingredients|ingredienti|zutaten|ingredientes|ingredienten)\s*:?")
        .expect("valid regex")
});
/// Precautionary statements, claims and boilerplate: analysed by the core engine,
/// never ingredients. Cut from the marker to the end of the sentence.
static NOT_INGREDIENTS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"\b(?:puede[n]?\s+contener|puede\s+tener|trazas?\s+de|may\s+contain|traces?\s+of|peut\s+contenir|",
        r"traces?\s+(?:eventuelles\s+)?de|kann\s+spuren|spuren\s+von|puo\s+contenere|tracce\s+di|pode\s+conter|",
        r"vestigios\s+de|pot\s+contenir|elaborado\s+en|fabricado\s+en|envasado\s+en|produced\s+in|made\s+in|",
        r"processed\s+in|manufactured\s+in|contaminacion\s+cruzada|cross\s+contamination|",
        r"(?:para\s+)?alergenos|(?:for\s+)?allergens|allergenes|allergeni|allergene|",
        r"conservar|consumir|una\s+vez\s+abierto|store\s+in|keep\s+in|best\s+before|consommer|",
        r"elaborado\s+por|fabricado\s+por|envasado\s+por|distribuido\s+por|peso\s+neto|lote|",
        r"informacion\s+nutricional|valores?\s+nutricionales|nutrition)\b",
    ))
    .expect("valid regex")
});

const MAX_DEPTH: usize = 3;
const MAX_ITEMS: usize = 200;
/// Longest list split into items (bytes). The core engine still reads the whole text;
/// what lies beyond is reported as not analysed item by item.
pub(crate) const MAX_INPUT: usize = 16 * 1024;

/// Items left to build, and whether any real ingredient was dropped for lack of them.
pub(crate) struct Budget {
    left: usize,
    lost: bool,
}

/// An ingredient list split into items, with what the split could not cover.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct Parsed {
    pub items: Vec<Item>,
    /// Ingredients beyond `MAX_ITEMS` or `MAX_INPUT` were not classified one by one.
    pub truncated: bool,
    /// Opening and closing brackets do not match: compound ingredients may be misread.
    pub unbalanced: bool,
}

impl Parsed {
    /// Some ingredients were never classified (the reassuring text must not be used).
    pub fn incomplete(&self) -> bool {
        self.truncated
    }
}

/// Items of an ingredient list. Never fails: unreadable parts become items the rules
/// do not recognise, which is the cautious outcome.
#[cfg(test)]
pub(crate) fn split(original: &str) -> Vec<Item> {
    parse(original).items
}

/// Splits a list and reports how complete the split is.
pub(crate) fn parse(original: &str) -> Parsed {
    let mut oversized = false;
    let original = if original.len() > MAX_INPUT {
        oversized = true;
        let mut cut = MAX_INPUT;
        while !original.is_char_boundary(cut) {
            cut -= 1;
        }
        &original[..cut]
    } else {
        original
    };
    let folded = Folded::new(original);
    let text = folded.text.as_str();
    let mut start = PREFIX.find(text).map_or(0, |m| m.end());
    let mut items = Vec::new();
    let mut budget = Budget {
        left: MAX_ITEMS,
        lost: false,
    };
    // Sentences are cut at the non-ingredient markers.
    while start < text.len() {
        let rest = &text[start..];
        let (end, skip_to) = match NOT_INGREDIENTS.find(rest) {
            Some(m) => {
                let sentence_end = sentence_end(text, start + m.end());
                (start + m.start(), sentence_end)
            }
            None => (text.len(), text.len()),
        };
        parse_level(&folded, original, start, end, 0, &mut items, &mut budget);
        start = skip_to;
    }
    Parsed {
        items,
        truncated: oversized || budget.lost,
        unbalanced: !balanced(text),
    }
}

/// Every opening bracket is closed by the same kind, in order.
fn balanced(text: &str) -> bool {
    let mut stack = Vec::new();
    for b in text.bytes() {
        match b {
            b'(' | b'[' | b'{' => stack.push(b),
            b')' | b']' | b'}' => {
                let open = match b {
                    b')' => b'(',
                    b']' => b'[',
                    _ => b'{',
                };
                if stack.pop() != Some(open) {
                    return false;
                }
            }
            _ => {}
        }
    }
    stack.is_empty()
}

/// End of the sentence that contains `from` (after its full stop).
fn sentence_end(text: &str, from: usize) -> usize {
    let bytes = text.as_bytes();
    let mut i = from;
    let mut depth = 0i32;
    while i < bytes.len() {
        match bytes[i] {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b'.' | b';' if depth <= 0 => {
                let decimal = i > 0
                    && bytes[i - 1].is_ascii_digit()
                    && bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
                if !decimal {
                    return i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    bytes.len()
}

/// Parses `text[start..end]` as a comma-separated list at `depth`, pushing items.
fn parse_level(
    folded: &Folded,
    original: &str,
    start: usize,
    end: usize,
    depth: usize,
    out: &mut Vec<Item>,
    budget: &mut Budget,
) {
    let bytes = folded.text.as_bytes();
    let mut i = start;
    let mut segment_start = start;
    // Ranges of direct parenthesised groups inside the current segment.
    let mut groups: Vec<(usize, usize)> = Vec::new();
    while i <= end {
        let at_end = i == end;
        let c = if at_end { b',' } else { bytes[i] };
        match c {
            b'(' | b'[' | b'{' => {
                let close = matching(bytes, i, end);
                groups.push((i, close));
                i = close + usize::from(close < end);
                continue;
            }
            b',' | b';' | b':' | b'.' | b')' | b']' | b'}' => {
                let decimal = c == b'.'
                    && i > 0
                    && bytes[i - 1].is_ascii_digit()
                    && bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
                let decimal_comma = c == b','
                    && !at_end
                    && i > 0
                    && bytes[i - 1].is_ascii_digit()
                    && bytes.get(i + 1).is_some_and(u8::is_ascii_digit);
                if !decimal && !decimal_comma {
                    emit_segment(
                        folded,
                        original,
                        segment_start,
                        i,
                        &groups,
                        depth,
                        out,
                        budget,
                    );
                    groups.clear();
                    segment_start = i + 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
}

fn matching(bytes: &[u8], open: usize, end: usize) -> usize {
    let mut depth = 0i32;
    for (j, b) in bytes.iter().enumerate().take(end).skip(open) {
        match b {
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => {
                depth -= 1;
                if depth == 0 {
                    return j;
                }
            }
            _ => {}
        }
    }
    end
}

/// Conjunctions that join two ingredients ("dextrosa y aromas"), in the folded text.
fn conjunction_splits(
    text: &str,
    start: usize,
    end: usize,
    groups: &[(usize, usize)],
) -> Vec<usize> {
    const WORDS: &[&str] = &["y", "e", "and", "et", "und", "ed", "o", "or", "ou", "oder"];
    let bytes = text.as_bytes();
    let mut cuts = Vec::new();
    let mut i = start;
    while i < end {
        if groups.iter().any(|(a, b)| i >= *a && i <= *b) || bytes[i] != b' ' {
            i += 1;
            continue;
        }
        let word_start = i + 1;
        let mut j = word_start;
        while j < end && bytes[j].is_ascii_alphabetic() {
            j += 1;
        }
        let word = &text[word_start..j];
        let followed = j < end && bytes[j] == b' ';
        if followed && WORDS.contains(&word) {
            let next = text[j..end].trim_start();
            let previous = text[start..i].trim_end();
            let vitamin =
                word == "e" && (previous.ends_with("vitamina") || previous.ends_with("vitamin"));
            let additive = next.starts_with(|c: char| c.is_ascii_digit());
            // "mono y diglicéridos": the first half is not an ingredient on its own.
            let prefix_only = previous.ends_with("mono");
            if !next.is_empty() && !previous.is_empty() && !vitamin && !additive && !prefix_only {
                cuts.push(i);
                cuts.push(j);
            }
        }
        i = j.max(i + 1);
    }
    cuts
}

#[allow(clippy::too_many_arguments)]
fn emit_segment(
    folded: &Folded,
    original: &str,
    start: usize,
    end: usize,
    groups: &[(usize, usize)],
    depth: usize,
    out: &mut Vec<Item>,
    budget: &mut Budget,
) {
    let text = folded.text.as_str();
    let cuts = conjunction_splits(text, start, end, groups);
    let mut bounds = vec![start];
    bounds.extend(cuts);
    bounds.push(end);
    for pair in bounds.chunks(2) {
        let (a, b) = (pair[0], pair[1]);
        let own: Vec<(usize, usize)> = groups
            .iter()
            .copied()
            .filter(|(g, _)| *g >= a && *g < b)
            .collect();
        if let Some(item) = build_item(folded, original, a, b, &own, depth, budget) {
            out.push(item);
        }
    }
}

fn build_item(
    folded: &Folded,
    original: &str,
    start: usize,
    end: usize,
    groups: &[(usize, usize)],
    depth: usize,
    budget: &mut Budget,
) -> Option<Item> {
    let text = folded.text.as_str();
    if budget.left == 0 {
        // A real ingredient (not punctuation) that will not be classified.
        budget.lost |= text[start..end].bytes().any(|b| b.is_ascii_alphabetic());
        return None;
    }
    // Own words: the segment without its parentheses.
    let mut own_ranges = Vec::new();
    let mut cursor = start;
    for (open, close) in groups {
        own_ranges.push((cursor, *open));
        cursor = (*close + 1).min(end);
    }
    own_ranges.push((cursor, end));
    let mut matcher = String::new();
    let mut display = String::new();
    for (a, b) in &own_ranges {
        if a >= b {
            continue;
        }
        matcher.push_str(&text[*a..*b]);
        matcher.push(' ');
        let (oa, ob) = folded.original_range(*a, *b);
        display.push_str(&original[oa..ob]);
        display.push(' ');
    }
    let mut children = Vec::new();
    let mut origins = Vec::new();
    let mut partial = false;
    for (open, close) in groups {
        let inner_start = open + 1;
        let inner_end = (*close).min(end);
        if inner_start >= inner_end {
            continue;
        }
        let inner = PERCENT.replace_all(&text[inner_start..inner_end], " ");
        if inner.trim().is_empty() {
            continue;
        }
        partial |= ADDITION.is_match(&inner);
        if ORIGIN_ONLY.is_match(&inner) {
            let (oa, ob) = folded.original_range(inner_start, inner_end);
            origins.push((
                inner.trim().to_string(),
                original[oa..ob].trim().to_string(),
            ));
            continue;
        }
        if depth + 1 < MAX_DEPTH {
            parse_level(
                folded,
                original,
                inner_start,
                inner_end,
                depth + 1,
                &mut children,
                budget,
            );
        } else {
            // Too deep: keep the words so the rules still see them.
            matcher.push_str(&inner);
            matcher.push(' ');
        }
    }
    let mut matcher = squash(&PERCENT.replace_all(&matcher, " "));
    let mut display = squash(&PERCENT.replace_all(&display, " "));
    display = display
        .trim_matches(|c: char| c.is_whitespace() || ".,;:*".contains(c))
        .to_string();
    for (origin, shown) in origins {
        let has_preposition = origin
            .split_whitespace()
            .next()
            .is_some_and(|w| ["de", "from", "aus", "di", "du", "do", "da", "von"].contains(&w));
        matcher.push_str(if has_preposition { " " } else { " de " });
        matcher.push_str(&origin);
        display = format!("{display} ({shown})");
    }
    let matcher = squash(&matcher);
    if matcher.chars().all(|c| !c.is_ascii_alphabetic()) && children.is_empty() {
        return None;
    }
    budget.left -= 1;
    let (span_start, span_end) = folded.original_range(start, end);
    Some(Item {
        display,
        matcher,
        span: (span_start, span_end),
        children,
        partial,
    })
}

fn squash(text: &str) -> String {
    text.split(|c: char| c.is_whitespace() || "*_".contains(c))
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(items: &[Item]) -> Vec<String> {
        items.iter().map(|i| i.display.clone()).collect()
    }

    #[test]
    fn kimbo_list_splits_on_the_conjunction() {
        let items = split(
            "Patatas seleccionadas (64%), aceite de oliva (35%), sal, dextrosa y aromas naturales.",
        );
        assert_eq!(
            names(&items),
            vec![
                "Patatas seleccionadas",
                "aceite de oliva",
                "sal",
                "dextrosa",
                "aromas naturales"
            ]
        );
        assert_eq!(items[0].matcher, "patatas seleccionadas");
        assert!(items.iter().all(|i| i.children.is_empty()));
    }

    #[test]
    fn compound_ingredients_have_children() {
        let items = split("Chocolate negro 70% (pasta de cacao, azúcar, manteca de cacao), sal.");
        assert_eq!(names(&items), vec!["Chocolate negro", "sal"]);
        assert_eq!(
            names(&items[0].children),
            vec!["pasta de cacao", "azúcar", "manteca de cacao"]
        );
    }

    #[test]
    fn origin_in_parentheses_is_merged() {
        let items = split("Jarabe de glucosa (de trigo), dextrosa (trigo), Weizenmehl");
        assert_eq!(items[0].matcher, "jarabe de glucosa de trigo");
        assert_eq!(items[0].display, "Jarabe de glucosa (de trigo)");
        assert_eq!(items[1].matcher, "dextrosa de trigo");
        assert_eq!(items[2].matcher, "weizenmehl");
    }

    #[test]
    fn traces_claims_and_boilerplate_are_not_items() {
        let items = split(
            "Ingredientes: arroz, sal. Puede contener trazas de trigo, huevo y soja. Conservar en lugar fresco.",
        );
        assert_eq!(names(&items), vec!["arroz", "sal"]);
    }

    #[test]
    fn additives_and_vitamins_are_not_split() {
        let items = split("colorante: e 150a, vitamina e, mono y diglicéridos de ácidos grasos");
        assert_eq!(
            names(&items),
            vec![
                "colorante",
                "e 150a",
                "vitamina e",
                "mono y diglicéridos de ácidos grasos"
            ]
        );
    }

    #[test]
    fn spans_point_into_the_original() {
        let text = "Harina de maíz, jarabe de glucosa (trigo), sal";
        let items = split(text);
        let (a, b) = items[1].span;
        assert_eq!(&text[a..b], " jarabe de glucosa (trigo)");
    }

    #[test]
    fn decimals_are_not_separators() {
        let items = split("leche 2,5 %, cacao 1.5%, sal");
        assert_eq!(names(&items), vec!["leche", "cacao", "sal"]);
    }
}
