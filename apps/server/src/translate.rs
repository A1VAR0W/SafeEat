//! Spanish view of ingredient lists, built from the Open Food Facts taxonomy.
//!
//! No model is involved: every ingredient OFF recognised (`en:wheat-flour`) has a
//! curated Spanish name ("harina de trigo"). Unrecognised parts keep their original
//! text. The gluten analysis keeps running on the original; the translation can only
//! make it more cautious (see `products::from_external`).

use std::collections::HashMap;

use crate::providers::{IngredientLanguage, ParsedIngredient};
use crate::state::AppState;

/// Below this share of recognised ingredients the mix of languages would mislead.
const MIN_TRANSLATED_SHARE: f64 = 0.6;
const CACHE_DAYS: i32 = 90;
const MAX_IDS_PER_REQUEST: usize = 60;

#[derive(Debug, Clone, PartialEq)]
pub struct Translation {
    pub text: String,
    /// Language of the original list ("fr", "en"...).
    pub language: String,
}

fn collect_ids(nodes: &[ParsedIngredient], out: &mut Vec<String>) {
    for node in nodes {
        if !out.contains(&node.id) {
            out.push(node.id.clone());
        }
        collect_ids(&node.children, out);
    }
}

/// Spanish names for taxonomy ids: database cache first, then one OFF request per 60 ids.
pub async fn spanish_names(
    state: &AppState,
    tagtype: &str,
    ids: &[String],
) -> HashMap<String, String> {
    let mut names = HashMap::new();
    if ids.is_empty() {
        return names;
    }
    let cached = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT id,name FROM taxonomy_names_es
         WHERE tagtype=$1 AND id=ANY($2) AND fetched_at > now() - make_interval(days => $3)",
    )
    .bind(tagtype)
    .bind(ids)
    .bind(CACHE_DAYS)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();
    let mut known: Vec<String> = Vec::with_capacity(cached.len());
    for (id, name) in cached {
        known.push(id.clone());
        if let Some(name) = name {
            names.insert(id, name);
        }
    }
    let missing: Vec<String> = ids
        .iter()
        .filter(|id| !known.contains(id))
        .cloned()
        .collect();
    for chunk in missing.chunks(MAX_IDS_PER_REQUEST) {
        let Ok(found) = state.off.spanish_names(tagtype, chunk).await else {
            break; // provider busy or down: show the original text this time
        };
        let (found_ids, found_names): (Vec<String>, Vec<Option<String>>) =
            found.iter().cloned().unzip();
        let _ = sqlx::query(
            "INSERT INTO taxonomy_names_es(tagtype,id,name)
             SELECT $1,id,name FROM UNNEST($2::text[],$3::text[]) AS t(id,name)
             ON CONFLICT (tagtype,id) DO UPDATE SET name=EXCLUDED.name, fetched_at=now()",
        )
        .bind(tagtype)
        .bind(&found_ids)
        .bind(&found_names)
        .execute(&state.db)
        .await;
        for (id, name) in found {
            if let Some(name) = name {
                names.insert(id, name);
            }
        }
    }
    names
}

fn percent(value: f64) -> String {
    let rounded = (value * 10.0).round() / 10.0;
    if rounded.fract() == 0.0 {
        format!("{rounded:.0} %")
    } else {
        format!("{} %", format!("{rounded:.1}").replace('.', ","))
    }
}

/// Renders the list in Spanish. Returns (text, translated nodes, total nodes).
fn render(nodes: &[ParsedIngredient], names: &HashMap<String, String>) -> (String, usize, usize) {
    let (mut translated, mut total) = (0, 0);
    let parts: Vec<String> = nodes
        .iter()
        .map(|node| {
            total += 1;
            let mut part = match names.get(&node.id) {
                Some(name) => {
                    translated += 1;
                    lower_first(name)
                }
                None if !node.text.is_empty() => node.text.clone(),
                None => node
                    .id
                    .split_once(':')
                    .map_or(node.id.clone(), |(_, rest)| rest.replace('-', " ")),
            };
            if let Some(p) = node.percent {
                part.push(' ');
                part.push_str(&percent(p));
            }
            if !node.children.is_empty() {
                let (inner, t, n) = render(&node.children, names);
                translated += t;
                total += n;
                part.push_str(&format!(" ({inner})"));
            }
            part
        })
        .collect();
    (parts.join(", "), translated, total)
}

/// "Lactosa" → "lactosa" inside a list, but "E322" and "UHT" stay as they are.
fn lower_first(name: &str) -> String {
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(first), Some(second)) if first.is_uppercase() && second.is_lowercase() => {
            first.to_lowercase().collect::<String>() + &name[first.len_utf8()..]
        }
        _ => name.to_string(),
    }
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// Pure part of the translation (tested without a database).
pub fn compose(
    language: &IngredientLanguage,
    ingredient_names: &HashMap<String, String>,
    allergen_names: &HashMap<String, String>,
) -> Option<Translation> {
    let code = language.code.as_deref()?;
    if code == "es" || language.parsed.is_empty() {
        return None;
    }
    let (list, translated, total) = render(&language.parsed, ingredient_names);
    if total == 0 || (translated as f64) < (total as f64) * MIN_TRANSLATED_SHARE {
        return None;
    }
    let mut text = capitalize(&list);
    text.push('.');
    let traces: Vec<String> = language
        .trace_tags
        .iter()
        .map(|tag| {
            allergen_names.get(tag).cloned().unwrap_or_else(|| {
                tag.split_once(':')
                    .map_or(tag.clone(), |(_, rest)| rest.replace('-', " "))
            })
        })
        .collect();
    if !traces.is_empty() {
        text.push_str(&format!(
            " Puede contener trazas de: {}.",
            traces.join(", ")
        ));
    }
    Some(Translation {
        text,
        language: code.to_string(),
    })
}

/// Spanish version of a non-Spanish ingredient list, or `None` when it is already
/// Spanish, OFF did not parse it, or too little of it could be translated.
pub async fn spanish_ingredients(
    state: &AppState,
    language: &IngredientLanguage,
) -> Option<Translation> {
    if language.code.as_deref().is_none_or(|c| c == "es") || language.parsed.is_empty() {
        return None;
    }
    let mut ids = Vec::new();
    collect_ids(&language.parsed, &mut ids);
    let ingredient_names = spanish_names(state, "ingredients", &ids).await;
    let allergen_names = spanish_names(state, "allergens", &language.trace_tags).await;
    compose(language, &ingredient_names, &allergen_names)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, text: &str) -> ParsedIngredient {
        ParsedIngredient {
            id: id.into(),
            text: text.into(),
            ..Default::default()
        }
    }

    fn names(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn translates_french_list_with_children_percent_and_traces() {
        let language = IngredientLanguage {
            code: Some("fr".into()),
            parsed: vec![
                node("en:corn-flour", "Farine de maïs"),
                ParsedIngredient {
                    percent: Some(29.0),
                    children: vec![
                        node("en:sugar", "sucre"),
                        node("en:cocoa-butter", "beurre de cacao"),
                    ],
                    ..node("en:milk-chocolate", "chocolat au LAIT")
                },
                node("fr:diphosphates sel", "diphosphates sel"),
            ],
            unknown: Some(1),
            trace_tags: vec!["en:nuts".into()],
        };
        let t = compose(
            &language,
            &names(&[
                ("en:corn-flour", "harina de maíz"),
                ("en:milk-chocolate", "chocolate con leche"),
                ("en:sugar", "azúcar"),
                ("en:cocoa-butter", "manteca de cacao"),
            ]),
            &names(&[("en:nuts", "frutos de cáscara")]),
        )
        .unwrap();
        assert_eq!(
            t.text,
            "Harina de maíz, chocolate con leche 29 % (azúcar, manteca de cacao), diphosphates sel. Puede contener trazas de: frutos de cáscara."
        );
        assert_eq!(t.language, "fr");
    }

    #[test]
    fn skips_spanish_and_mostly_unknown_lists() {
        let spanish = IngredientLanguage {
            code: Some("es".into()),
            parsed: vec![node("en:sugar", "azúcar")],
            ..Default::default()
        };
        assert!(compose(&spanish, &names(&[("en:sugar", "azúcar")]), &HashMap::new()).is_none());
        let unknown = IngredientLanguage {
            code: Some("de".into()),
            parsed: vec![
                node("de:a", "a"),
                node("de:b", "b"),
                node("en:sugar", "Zucker"),
            ],
            ..Default::default()
        };
        assert!(compose(&unknown, &names(&[("en:sugar", "azúcar")]), &HashMap::new()).is_none());
    }
}
