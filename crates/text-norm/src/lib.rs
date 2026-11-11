//! Name normalization ("search key") shared by PostgreSQL and Rust.
//!
//! The SQL function `restaurant_brand_name()` (migration 010) is generated from the
//! same [`FOLDS`] table, and a test checks that the migration contains exactly the
//! generated SQL. Keys keep only `[a-z0-9]`: accents are folded (both cases),
//! ligatures expanded, and everything else is dropped.
//!
//! Examples: `"ÁNGEL"` → `"angel"`, `"Burger King – Gran Vía"` → `"burgerkinggranvia"`.

/// Characters folded to ASCII. Both cases are listed so that SQL `lower()` only ever
/// has to handle ASCII, independent of the database collation.
pub const FOLDS: &[(char, &str)] = &[
    ('á', "a"),
    ('à', "a"),
    ('â', "a"),
    ('ä', "a"),
    ('ã', "a"),
    ('å', "a"),
    ('ā', "a"),
    ('ą', "a"),
    ('Á', "a"),
    ('À', "a"),
    ('Â', "a"),
    ('Ä', "a"),
    ('Ã', "a"),
    ('Å', "a"),
    ('Ā', "a"),
    ('Ą', "a"),
    ('é', "e"),
    ('è', "e"),
    ('ê', "e"),
    ('ë', "e"),
    ('ē', "e"),
    ('ę', "e"),
    ('ě', "e"),
    ('É', "e"),
    ('È', "e"),
    ('Ê', "e"),
    ('Ë', "e"),
    ('Ē', "e"),
    ('Ę', "e"),
    ('Ě', "e"),
    ('í', "i"),
    ('ì', "i"),
    ('î', "i"),
    ('ï', "i"),
    ('ī', "i"),
    ('ı', "i"),
    ('Í', "i"),
    ('Ì', "i"),
    ('Î', "i"),
    ('Ï', "i"),
    ('Ī', "i"),
    ('İ', "i"),
    ('ó', "o"),
    ('ò', "o"),
    ('ô', "o"),
    ('ö', "o"),
    ('õ', "o"),
    ('ø', "o"),
    ('ō', "o"),
    ('ő', "o"),
    ('Ó', "o"),
    ('Ò', "o"),
    ('Ô', "o"),
    ('Ö', "o"),
    ('Õ', "o"),
    ('Ø', "o"),
    ('Ō', "o"),
    ('Ő', "o"),
    ('ú', "u"),
    ('ù', "u"),
    ('û', "u"),
    ('ü', "u"),
    ('ū', "u"),
    ('ů', "u"),
    ('ű', "u"),
    ('Ú', "u"),
    ('Ù', "u"),
    ('Û', "u"),
    ('Ü', "u"),
    ('Ū', "u"),
    ('Ů', "u"),
    ('Ű', "u"),
    ('ý', "y"),
    ('ÿ', "y"),
    ('Ý', "y"),
    ('Ÿ', "y"),
    ('ñ', "n"),
    ('ń', "n"),
    ('ň', "n"),
    ('Ñ', "n"),
    ('Ń', "n"),
    ('Ň', "n"),
    ('ç', "c"),
    ('ć', "c"),
    ('č', "c"),
    ('Ç', "c"),
    ('Ć', "c"),
    ('Č', "c"),
    ('ś', "s"),
    ('š', "s"),
    ('ş', "s"),
    ('Ś', "s"),
    ('Š', "s"),
    ('Ş', "s"),
    ('ź', "z"),
    ('ż', "z"),
    ('ž', "z"),
    ('Ź', "z"),
    ('Ż', "z"),
    ('Ž', "z"),
    ('ł', "l"),
    ('Ł', "l"),
    ('ř', "r"),
    ('Ř', "r"),
    ('ť', "t"),
    ('Ť', "t"),
    ('ď', "d"),
    ('Ď', "d"),
    ('đ', "d"),
    ('Đ', "d"),
    ('ğ', "g"),
    ('Ğ', "g"),
    ('ß', "ss"),
    ('ẞ', "ss"),
    ('æ', "ae"),
    ('Æ', "ae"),
    ('œ', "oe"),
    ('Œ', "oe"),
    ('þ', "th"),
    ('Þ', "th"),
];

fn fold(c: char) -> Option<&'static str> {
    FOLDS.iter().find(|(from, _)| *from == c).map(|(_, to)| *to)
}

/// Normalized search key, identical to SQL `restaurant_brand_name(value)`.
pub fn search_key(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if let Some(mapped) = fold(c) {
            out.push_str(mapped);
        }
    }
    out
}

/// Strip a branch suffix such as `"Burger King - Gran Vía"` / `"VIPS · Sol"` / `"Foster's (Sol)"`.
pub fn brand_part(name: &str) -> &str {
    for sep in [" - ", " – ", " — ", " · ", " | ", " ("] {
        if let Some(i) = name.find(sep) {
            return name[..i].trim();
        }
    }
    name.trim()
}

fn sql_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

/// SQL body of `restaurant_brand_name(value)`, generated from [`FOLDS`].
pub fn sql_function_body() -> String {
    let singles: Vec<&(char, &str)> = FOLDS.iter().filter(|(_, to)| to.len() == 1).collect();
    let from: String = singles.iter().map(|(c, _)| *c).collect();
    let to: String = singles.iter().map(|(_, t)| *t).collect();
    let mut expr = format!("translate(value,{},{})", sql_quote(&from), sql_quote(&to));
    for (c, to) in FOLDS.iter().filter(|(_, to)| to.len() > 1) {
        expr = format!(
            "replace({expr},{},{})",
            sql_quote(&c.to_string()),
            sql_quote(to)
        );
    }
    format!("SELECT regexp_replace(lower({expr}),'[^a-z0-9]','','g')")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_accents_in_both_cases() {
        assert_eq!(search_key("ÁNGEL"), "angel");
        assert_eq!(search_key("Ángel"), "angel");
        assert_eq!(search_key("Burger King – Gran Vía"), "burgerkinggranvia");
        assert_eq!(search_key("Straße Ø Æble"), "strasseoaeble");
        assert_eq!(search_key("McDonald's"), "mcdonalds");
        assert_eq!(search_key("Cañas y Tapas"), "canasytapas");
        assert_eq!(search_key("100 Montaditos"), "100montaditos");
        assert_eq!(search_key("¿Qué?!"), "que");
    }

    #[test]
    fn brand_part_strips_branch() {
        assert_eq!(brand_part("Burger King - Gran Vía"), "Burger King");
        assert_eq!(brand_part("VIPS · Sol"), "VIPS");
        assert_eq!(brand_part("Foster's Hollywood (Sol)"), "Foster's Hollywood");
        assert_eq!(brand_part("Telepizza"), "Telepizza");
    }

    #[test]
    fn folds_are_unique_and_lowercase_ascii() {
        for (i, (c, to)) in FOLDS.iter().enumerate() {
            assert!(to.bytes().all(|b| b.is_ascii_lowercase()), "{c}");
            assert!(!FOLDS[i + 1..].iter().any(|(o, _)| o == c), "duplicate {c}");
        }
    }

    #[test]
    fn migration_uses_generated_sql() {
        let migration = include_str!("../../../db/migrations/010_chains_and_search.sql");
        assert!(
            migration.contains(&sql_function_body()),
            "db/migrations/010_chains_and_search.sql is out of date; expected body:\n{}",
            sql_function_body()
        );
    }
}
