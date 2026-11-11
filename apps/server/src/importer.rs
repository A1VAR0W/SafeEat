//! OSM restaurant catalog import (port of scripts/import-restaurant-catalog.mjs).
//! Reads an osmium GeoJSON sequence; the replacement of one region is atomic.

use std::path::Path;

use anyhow::{bail, Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use serde_json::Value;
use sqlx::PgPool;
use tokio::io::AsyncBufReadExt;

static REGION: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-z][a-z0-9_-]{1,40}$").unwrap());
static WIKIDATA: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Q\d+$").unwrap());
const AMENITIES: &[&str] = &[
    "restaurant",
    "cafe",
    "fast_food",
    "bar",
    "pub",
    "food_court",
    "biergarten",
];
const SHOPS: &[&str] = &["bakery", "pastry", "convenience"];

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub id: String,
    pub name: String,
    pub address: String,
    pub category: String,
    pub longitude: f64,
    pub latitude: f64,
    pub website: Option<String>,
    pub phone: Option<String>,
    pub gluten_tag: Option<String>,
    pub brand_wikidata: Option<String>,
    pub brand: Option<String>,
}

fn collect(value: &Value, pairs: &mut Vec<(f64, f64)>) {
    if let Some(items) = value.as_array() {
        match (
            items.first().and_then(Value::as_f64),
            items.get(1).and_then(Value::as_f64),
        ) {
            (Some(x), Some(y)) => pairs.push((x, y)),
            _ => items.iter().for_each(|child| collect(child, pairs)),
        }
    }
}

/// Point or bounding-box centre, like the JS importer.
fn point(geometry: &Value) -> Option<(f64, f64)> {
    let coordinates = geometry.get("coordinates")?;
    if geometry.get("type").and_then(Value::as_str) == Some("Point") {
        return Some((coordinates.get(0)?.as_f64()?, coordinates.get(1)?.as_f64()?));
    }
    let mut pairs = Vec::new();
    collect(coordinates, &mut pairs);
    if pairs.is_empty() {
        return None;
    }
    let (min_x, max_x) = pairs
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.0), b.max(p.0)));
    let (min_y, max_y) = pairs
        .iter()
        .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
    Some(((min_x + max_x) / 2.0, (min_y + max_y) / 2.0))
}

fn truncate(value: &str, max: usize) -> String {
    // JS `slice` counts UTF-16 units; char counting is close enough and never splits a character.
    value.chars().take(max).collect()
}

pub fn record(feature: &Value) -> Option<Record> {
    let tags = feature.get("properties")?.as_object()?;
    let tag = |k: &str| {
        tags.get(k)
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
    };
    let kind = match tag("@type")? {
        "node" => "n",
        "way" => "w",
        "relation" => "r",
        _ => return None,
    };
    let id = match tags.get("@id")? {
        Value::Number(n) => n.as_i64()?,
        Value::String(s) => s.parse::<i64>().ok()?,
        _ => return None,
    };
    let category = tag("amenity")
        .filter(|a| AMENITIES.contains(a))
        .or_else(|| tag("shop").filter(|s| SHOPS.contains(s)))?;
    let name = match tags.get("name")? {
        Value::String(s) if !s.is_empty() => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let (longitude, latitude) = point(feature.get("geometry")?)?;
    if id <= 0
        || !longitude.is_finite()
        || !latitude.is_finite()
        || latitude.abs() > 90.0
        || longitude.abs() > 180.0
    {
        return None;
    }
    let website = tag("website").or_else(|| tag("contact:website"));
    let address = [
        tag("addr:street"),
        tag("addr:housenumber"),
        tag("addr:city"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(", ");
    Some(Record {
        id: format!("osm_{kind}_{id}"),
        name: truncate(&name, 200),
        address: truncate(&address, 300),
        category: category.into(),
        longitude,
        latitude,
        website: website
            .filter(|w| {
                w.get(..7)
                    .is_some_and(|p| p.eq_ignore_ascii_case("http://"))
                    || w.get(..8)
                        .is_some_and(|p| p.eq_ignore_ascii_case("https://"))
            })
            .map(str::to_string),
        phone: tag("phone")
            .or_else(|| tag("contact:phone"))
            .map(str::to_string),
        gluten_tag: tag("diet:gluten_free").map(str::to_string),
        brand_wikidata: tag("brand:wikidata")
            .filter(|v| WIKIDATA.is_match(v))
            .map(str::to_string),
        brand: tag("brand").map(str::to_string),
    })
}

async fn flush(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    region: &str,
    batch: &mut Vec<Record>,
) -> Result<()> {
    if batch.is_empty() {
        return Ok(());
    }
    let col = |f: fn(&Record) -> Option<String>| batch.iter().map(f).collect::<Vec<_>>();
    sqlx::query(
        "INSERT INTO incoming_restaurants(id,region,name,address,category,location,website,phone,gluten_tag,chain_key,chain_name,brand,brand_wikidata)
         SELECT id,$1,name,address,category,ST_SetSRID(ST_MakePoint(lng,lat),4326)::geography,website,phone,gluten_tag,wikidata,brand,brand,wikidata
         FROM UNNEST($2::text[],$3::text[],$4::text[],$5::text[],$6::float8[],$7::float8[],$8::text[],$9::text[],$10::text[],$11::text[],$12::text[])
           AS t(id,name,address,category,lng,lat,website,phone,gluten_tag,wikidata,brand)
         ON CONFLICT(id) DO NOTHING",
    )
    .bind(region)
    .bind(batch.iter().map(|r| r.id.clone()).collect::<Vec<_>>())
    .bind(batch.iter().map(|r| r.name.clone()).collect::<Vec<_>>())
    .bind(batch.iter().map(|r| r.address.clone()).collect::<Vec<_>>())
    .bind(batch.iter().map(|r| r.category.clone()).collect::<Vec<_>>())
    .bind(batch.iter().map(|r| r.longitude).collect::<Vec<_>>())
    .bind(batch.iter().map(|r| r.latitude).collect::<Vec<_>>())
    .bind(col(|r| r.website.clone()))
    .bind(col(|r| r.phone.clone()))
    .bind(col(|r| r.gluten_tag.clone()))
    .bind(col(|r| r.brand_wikidata.clone()))
    .bind(col(|r| r.brand.clone()))
    .execute(&mut **tx)
    .await?;
    batch.clear();
    Ok(())
}

pub async fn import(db: &PgPool, region: &str, file: &Path) -> Result<()> {
    if !REGION.is_match(region) {
        bail!("Usage: safeeat-server import-restaurants <region> <geojsonseq-file>");
    }
    let reader = tokio::io::BufReader::new(
        tokio::fs::File::open(file)
            .await
            .with_context(|| format!("opening {}", file.display()))?,
    );
    let mut lines = reader.lines();
    let mut tx = db.begin().await?;
    sqlx::query("SET LOCAL statement_timeout=0")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SELECT pg_advisory_xact_lock(87349103)")
        .execute(&mut *tx)
        .await?;
    sqlx::query("CREATE TEMP TABLE incoming_restaurants (LIKE restaurant_catalog INCLUDING DEFAULTS) ON COMMIT DROP")
        .execute(&mut *tx)
        .await?;
    sqlx::query("ALTER TABLE incoming_restaurants ADD PRIMARY KEY(id)")
        .execute(&mut *tx)
        .await?;
    let mut count = 0usize;
    let mut batch = Vec::with_capacity(1000);
    while let Some(line) = lines.next_line().await? {
        let line = line.trim_start_matches('\u{1e}');
        if line.trim().is_empty() {
            continue;
        }
        let feature: Value = serde_json::from_str(line).context("invalid GeoJSON line")?;
        let Some(item) = record(&feature) else {
            continue;
        };
        batch.push(item);
        count += 1;
        if batch.len() == 1000 {
            flush(&mut tx, region, &mut batch).await?;
        }
    }
    flush(&mut tx, region, &mut batch).await?;
    if count == 0 {
        bail!("No food establishments found; keeping the existing catalog");
    }
    sqlx::query("DELETE FROM restaurant_catalog WHERE region=$1")
        .bind(region)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO restaurant_catalog(id,region,name,address,category,location,website,phone,gluten_tag,chain_key,chain_name,brand,brand_wikidata)
         SELECT id,region,name,address,category,location,website,phone,gluten_tag,chain_key,chain_name,brand,brand_wikidata FROM incoming_restaurants
         ON CONFLICT(id) DO UPDATE SET region=EXCLUDED.region,name=EXCLUDED.name,address=EXCLUDED.address,category=EXCLUDED.category,
           location=EXCLUDED.location,website=EXCLUDED.website,phone=EXCLUDED.phone,gluten_tag=EXCLUDED.gluten_tag,chain_key=EXCLUDED.chain_key,
           chain_name=EXCLUDED.chain_name,brand=EXCLUDED.brand,brand_wikidata=EXCLUDED.brand_wikidata,imported_at=now()",
    )
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO restaurant_catalog_imports(region,item_count) VALUES($1,$2) ON CONFLICT(region) DO UPDATE SET imported_at=now(),item_count=$2",
    )
    .bind(region)
    .bind(count as i32)
    .execute(&mut *tx)
    .await?;
    // Recompute chains (aliases, assignments, branch counts) and re-key chain reviews.
    sqlx::query("SELECT refresh_restaurant_chains()")
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    println!("Imported {count} {region} restaurants");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_ways_and_nodes() {
        let r = record(&json!({
            "properties": {"@type": "way", "@id": 42, "amenity": "fast_food", "name": "Burger King Gran Vía",
                "brand": "Burger King", "brand:wikidata": "Q177054", "website": "HTTPS://bk.es", "addr:city": "Madrid"},
            "geometry": {"type": "Polygon", "coordinates": [[[-3.70, 40.41], [-3.68, 40.43]]]}
        }))
        .unwrap();
        assert_eq!(r.id, "osm_w_42");
        assert_eq!(r.brand_wikidata.as_deref(), Some("Q177054"));
        assert!((r.longitude + 3.69).abs() < 1e-9);
        assert_eq!(r.website.as_deref(), Some("HTTPS://bk.es"));
        assert_eq!(r.address, "Madrid");
        assert!(record(
            &json!({"properties": {"@type": "node", "@id": 1, "amenity": "bank", "name": "X"},
            "geometry": {"type": "Point", "coordinates": [0, 0]}})
        )
        .is_none());
    }
}
