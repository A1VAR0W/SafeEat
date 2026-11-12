//! Imported OpenStreetMap catalog (port of apps/api/src/modules/restaurants/catalog.ts, with chains and
//! accent-insensitive trigram search from migration 010).

use safeeat_contracts::{
    ChainRef, Evidence, EvidenceKind, GlutenSupport, Restaurant, RestaurantSuggestion,
    SuggestionKind,
};

use crate::error::ApiResult;
use crate::providers::openstreetmap::{gluten_from_tag, is_broad_query};
use crate::state::AppState;
use crate::util::sql_iso;

#[derive(sqlx::FromRow, Clone)]
pub struct CatalogRow {
    pub id: String,
    pub name: String,
    pub address: String,
    pub latitude: f64,
    pub longitude: f64,
    pub website: Option<String>,
    pub phone: Option<String>,
    pub gluten_tag: Option<String>,
    pub chain_key: Option<String>,
    pub chain_name: Option<String>,
    pub imported_at: String,
    pub distance_m: Option<f64>,
}

/// `$2,$3` are longitude, latitude.
pub const POINT: &str = "ST_SetSRID(ST_MakePoint($2,$3),4326)::geography";

pub fn fields(nearby: bool) -> String {
    format!(
        "id,name,address,ST_Y(location::geometry) AS latitude,ST_X(location::geometry) AS longitude,website,phone,gluten_tag,chain_key,chain_name,{} AS imported_at,{} AS distance_m",
        sql_iso("imported_at"),
        if nearby { format!("ST_Distance(location,{POINT})") } else { "NULL::float8".into() }
    )
}

pub fn restaurant(row: CatalogRow) -> Restaurant {
    let mut parts = row.id.splitn(3, '_').skip(1);
    let kind = match parts.next() {
        Some("w") => "way",
        Some("r") => "relation",
        _ => "node",
    };
    let osm_id = parts.next().unwrap_or_default().to_string();
    let support = gluten_from_tag(row.gluten_tag.as_deref());
    let text = match support {
        GlutenSupport::Unknown if row.gluten_tag.as_deref() == Some("limited") => {
            "La ficha indica opciones limitadas. Consulta el menú y la preparación."
        }
        GlutenSupport::Unknown => "Aún no hay información confirmada sobre opciones sin gluten.",
        GlutenSupport::DedicatedGlutenFree => {
            "La ficha indica que el local ofrece prácticamente todo sin gluten. Confirma la preparación antes de acudir."
        }
        GlutenSupport::GlutenFreeOptions => "La ficha indica opciones sin gluten. Confirma ingredientes y contaminación cruzada.",
        GlutenSupport::NoGlutenFreeOptions => "La ficha indica que no ofrece opciones sin gluten.",
    };
    Restaurant {
        chain: row.chain_key.clone().map(|key| ChainRef {
            key,
            name: row
                .chain_name
                .clone()
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| row.name.clone()),
        }),
        id: row.id,
        name: row.name,
        address: row.address,
        latitude: Some(row.latitude),
        longitude: Some(row.longitude),
        website: row.website.filter(|v| !v.is_empty()),
        phone: row.phone.filter(|v| !v.is_empty()),
        maps_url: format!("https://www.openstreetmap.org/{kind}/{osm_id}"),
        gluten_support: support,
        community_created: None,
        review_group: None,
        rating: None,
        gluten_reports: None,
        evidence: vec![Evidence {
            source: "OpenStreetMap".into(),
            kind: EvidenceKind::Uncertainty,
            text: text.into(),
            url: None,
        }],
        source: "OpenStreetMap · ODbL".into(),
        attributions: vec!["© OpenStreetMap contributors".into()],
        fetched_at: row.imported_at,
        distance_m: row.distance_m.map(|d| d.round()),
        has_local_review: None,
    }
}

pub async fn ready(state: &AppState) -> ApiResult<bool> {
    Ok(
        sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM restaurant_catalog_imports)")
            .fetch_one(&state.db)
            .await?,
    )
}

/// Whether the catalog covers this area at all (if it does, Overpass is never needed).
pub async fn covers(state: &AppState, latitude: f64, longitude: f64) -> ApiResult<bool> {
    Ok(sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM restaurant_catalog WHERE ST_DWithin(location,ST_SetSRID(ST_MakePoint($1,$2),4326)::geography,15000))",
    )
    .bind(longitude)
    .bind(latitude)
    .fetch_one(&state.db)
    .await?)
}

pub async fn get(state: &AppState, id: &str) -> ApiResult<Option<Restaurant>> {
    Ok(sqlx::query_as::<_, CatalogRow>(&format!(
        "SELECT {} FROM restaurant_catalog WHERE id=$1",
        fields(false)
    ))
    .bind(id)
    .fetch_optional(&state.db)
    .await?
    .map(restaurant))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum GlutenMode {
    Any,
    Options,
    Dedicated,
}

fn gluten_filter(mode: GlutenMode) -> &'static str {
    match mode {
        GlutenMode::Any => "",
        GlutenMode::Dedicated => "AND (gluten_tag='only' OR EXISTS(SELECT 1 FROM restaurant_gluten_reports r WHERE r.revoked_at IS NULL AND (r.restaurant_id=restaurant_catalog.id OR r.chain_key=restaurant_catalog.chain_key OR r.chain_key='name:'||restaurant_catalog.search_name) AND r.support='DEDICATED_GLUTEN_FREE'))",
        GlutenMode::Options => "AND (gluten_tag IN ('only','yes') OR EXISTS(SELECT 1 FROM restaurant_gluten_reports r WHERE r.revoked_at IS NULL AND (r.restaurant_id=restaurant_catalog.id OR r.chain_key=restaurant_catalog.chain_key OR r.chain_key='name:'||restaurant_catalog.search_name) AND r.support IN ('DEDICATED_GLUTEN_FREE','GLUTEN_FREE_OPTIONS')))",
    }
}

/// Branches of one chain, nearest first (within 50 km when a location is known; else anywhere).
pub async fn chain_branches(
    state: &AppState,
    chain_key: &str,
    location: Option<(f64, f64)>,
    mode: GlutenMode,
    limit: i64,
) -> ApiResult<Vec<Restaurant>> {
    let filter = gluten_filter(mode);
    let rows = match location {
        Some((lat, lng)) => {
            let near = sqlx::query_as::<_, CatalogRow>(&format!(
                "SELECT {} FROM restaurant_catalog WHERE chain_key=$1 AND ST_DWithin(location,{POINT},50000) {filter} ORDER BY location <-> {POINT} LIMIT $4",
                fields(true)
            ))
            .bind(chain_key)
            .bind(lng)
            .bind(lat)
            .bind(limit)
            .fetch_all(&state.db)
            .await?;
            if near.is_empty() {
                sqlx::query_as::<_, CatalogRow>(&format!(
                    "SELECT {} FROM restaurant_catalog WHERE chain_key=$1 {filter} ORDER BY location <-> {POINT} LIMIT $4",
                    fields(true)
                ))
                .bind(chain_key)
                .bind(lng)
                .bind(lat)
                .bind(limit)
                .fetch_all(&state.db)
                .await?
            } else {
                near
            }
        }
        None => {
            sqlx::query_as::<_, CatalogRow>(&format!(
                "SELECT {} FROM restaurant_catalog WHERE chain_key=$1 {filter} ORDER BY name,id LIMIT $2",
                fields(false)
            ))
            .bind(chain_key)
            .bind(limit)
            .fetch_all(&state.db)
            .await?
        }
    };
    Ok(rows.into_iter().map(restaurant).collect())
}

/// Name (or broad "restaurantes cerca") search over the catalog.
pub async fn search(
    state: &AppState,
    query: &str,
    key: &str,
    location: Option<(f64, f64)>,
    mode: GlutenMode,
) -> ApiResult<Vec<Restaurant>> {
    let broad = is_broad_query(query);
    if (broad && location.is_none()) || (!broad && key.is_empty()) {
        return Ok(vec![]);
    }
    let filter = gluten_filter(mode);
    // $1 = normalized key; $2,$3 = point; $4 = radius.
    let name = if broad {
        "AND $1::text IS NOT NULL".to_string()
    } else {
        "AND (search_name LIKE '%'||$1||'%' OR restaurant_brand_name(COALESCE(chain_name,'')) LIKE '%'||$1||'%')".to_string()
    };
    let rows = match location {
        Some((lat, lng)) => {
            let order = if broad { String::new() } else { "similarity(search_name,$1) DESC,".into() };
            sqlx::query_as::<_, CatalogRow>(&format!(
                "SELECT {} FROM restaurant_catalog WHERE ST_DWithin(location,{POINT},$4) {name} {filter} ORDER BY {order} location <-> {POINT} LIMIT 100",
                fields(true)
            ))
            .bind(key)
            .bind(lng)
            .bind(lat)
            .bind(if broad { 12000.0 } else { 25000.0 })
            .fetch_all(&state.db)
            .await?
        }
        None => {
            sqlx::query_as::<_, CatalogRow>(&format!(
                "SELECT {} FROM restaurant_catalog WHERE true {name} {filter} ORDER BY similarity(search_name,$1) DESC,name LIMIT 100",
                fields(false)
            ))
            .bind(key)
            .fetch_all(&state.db)
            .await?
        }
    };
    if rows.is_empty() && !broad && key.len() >= 5 {
        let rows = fuzzy(state, key, location, filter, 100).await?;
        return Ok(rows.into_iter().map(restaurant).collect());
    }
    Ok(rows.into_iter().map(restaurant).collect())
}

/// Second try for names with a typo ("burguer", "telepiza"): trigram word similarity,
/// nearest first. Only runs when the exact substring search found nothing.
async fn fuzzy(
    state: &AppState,
    key: &str,
    location: Option<(f64, f64)>,
    filter: &str,
    limit: i64,
) -> ApiResult<Vec<CatalogRow>> {
    Ok(match location {
        Some((lat, lng)) => {
            sqlx::query_as::<_, CatalogRow>(&format!(
                "SELECT {} FROM restaurant_catalog WHERE ST_DWithin(location,{POINT},25000)
                 AND word_similarity($1,search_name) >= 0.5 {filter}
                 ORDER BY round(word_similarity($1,search_name)::numeric,1) DESC, location <-> {POINT} LIMIT $4",
                fields(true)
            ))
            .bind(key)
            .bind(lng)
            .bind(lat)
            .bind(limit)
            .fetch_all(&state.db)
            .await?
        }
        None => {
            sqlx::query_as::<_, CatalogRow>(&format!(
                "SELECT {} FROM restaurant_catalog WHERE $1 <% search_name {filter}
                 ORDER BY word_similarity($1,search_name) DESC,name LIMIT $2",
                fields(false)
            ))
            .bind(key)
            .bind(limit)
            .fetch_all(&state.db)
            .await?
        }
    })
}

pub async fn suggest(
    state: &AppState,
    key: &str,
    location: Option<(f64, f64)>,
) -> ApiResult<Vec<RestaurantSuggestion>> {
    if key.is_empty() {
        return Ok(vec![]);
    }
    let mut rows = match location {
        Some((lat, lng)) => {
            sqlx::query_as::<_, CatalogRow>(&format!(
                "SELECT {} FROM restaurant_catalog WHERE search_name LIKE '%'||$1||'%' ORDER BY location <-> {POINT},similarity(search_name,$1) DESC LIMIT 5",
                fields(true)
            ))
            .bind(key)
            .bind(lng)
            .bind(lat)
            .fetch_all(&state.db)
            .await?
        }
        None => {
            sqlx::query_as::<_, CatalogRow>(&format!(
                "SELECT {} FROM restaurant_catalog WHERE search_name LIKE '%'||$1||'%' ORDER BY similarity(search_name,$1) DESC LIMIT 5",
                fields(false)
            ))
            .bind(key)
            .fetch_all(&state.db)
            .await?
        }
    };
    if rows.is_empty() && key.len() >= 5 {
        rows = fuzzy(state, key, location, "", 5).await?;
    }
    Ok(rows
        .into_iter()
        .map(|row| RestaurantSuggestion {
            id: Some(row.id.clone()),
            label: [row.name.as_str(), row.address.as_str()]
                .iter()
                .filter(|v| !v.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(" · "),
            kind: SuggestionKind::Restaurant,
            query: row
                .chain_name
                .clone()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| row.name.clone()),
            latitude: Some(row.latitude),
            longitude: Some(row.longitude),
            chain_key: None,
            branch_count: None,
        })
        .collect())
}
