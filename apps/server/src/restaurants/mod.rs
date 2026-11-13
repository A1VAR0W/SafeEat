//! Restaurants (port of apps/api/src/modules/restaurants) with SafeEat 2.0 chains:
//! chain suggestions first, nearest branches, chain pages and trusted-only chain reviews.

pub mod catalog;
pub mod reports;

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use once_cell::sync::Lazy;
use regex::Regex;
use safeeat_contracts::{
    ChainDetail, ChainSummary, GlutenSupport, Items, Restaurant, RestaurantReportCounts,
    RestaurantSuggestion, SuggestionKind,
};
use safeeat_text_norm::search_key;
use serde_json::{json, Value};
use uuid::Uuid;

use self::catalog::GlutenMode;
use crate::error::{ApiError, ApiResult};
use crate::extract::{is_trusted, AuthUser, JsonBody, MaybeUser, QueryMap};
use crate::permissions::review_note;
use crate::providers::openstreetmap::{is_broad_query, RestaurantSearch};
use crate::ratelimit::{per_day, per_hour, per_minute, Limit};
use crate::reviews::{self, Kind};
use crate::state::AppState;
use crate::util::{self, sql_iso};

const SEARCH_LIMIT: Limit = per_minute("restaurant-search", 15);
const SUGGEST_LIMIT: Limit = per_minute("restaurant-suggest", 60);
const CREATE_LIMIT: Limit = per_day("restaurant-create", 4);
const REPORT_LIMIT: Limit = per_hour("restaurant-report", 8);
const RATING_LIMIT: Limit = per_hour("restaurant-rating", 30);
/// The map asks for a wide area at once and only again when she leaves it.
const AREA_LIMIT: Limit = per_minute("restaurant-area", 40);
/// Pins per area request (favourable places come first, so they are never cut).
const AREA_MAX_PLACES: i64 = 2500;
/// Largest area side in degrees (about 65 km): wider views show clusters already loaded.
const AREA_MAX_SPAN: f64 = 0.6;

static CHAIN_KEY: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(Q[0-9]{1,12}|brand:[a-z0-9]{1,120})$").unwrap());

#[derive(sqlx::FromRow)]
struct CommunityRow {
    id: String,
    name: String,
    address: String,
    latitude: f64,
    longitude: f64,
    created_at: String,
}

fn community(row: CommunityRow) -> Restaurant {
    Restaurant {
        maps_url: format!(
            "https://www.google.com/maps/search/?api=1&query={},{}",
            row.latitude, row.longitude
        ),
        id: row.id,
        name: row.name,
        address: row.address,
        latitude: Some(row.latitude),
        longitude: Some(row.longitude),
        website: None,
        phone: None,
        gluten_support: GlutenSupport::Unknown,
        community_created: Some(true),
        chain: None,
        review_group: None,
        rating: None,
        gluten_reports: None,
        evidence: vec![],
        source: "Comunidad SafeEat".into(),
        attributions: vec![],
        fetched_at: row.created_at,
        distance_m: None,
        has_local_review: None,
    }
}

fn community_fields() -> String {
    format!(
        "id,name,address,latitude,longitude,{} AS created_at",
        sql_iso("created_at")
    )
}

/// Restaurant by id: community (`se_`), catalog (`osm_`) or the live OSM provider.
pub async fn get_restaurant(state: &AppState, id: &str) -> ApiResult<Restaurant> {
    let item = if id.starts_with("se_") {
        sqlx::query_as::<_, CommunityRow>(&format!(
            "SELECT {} FROM community_restaurants WHERE id=$1",
            community_fields()
        ))
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .map(community)
    } else if id.starts_with("osm_") {
        match catalog::get(state, id).await? {
            Some(item) => Some(item),
            None => state.osm.get_by_id(id).await?,
        }
    } else {
        state.osm.get_by_id(id).await?
    };
    let item = item.ok_or_else(ApiError::not_found)?;
    Ok(reports::with_reports(state, vec![item]).await?.remove(0))
}

fn group_key(item: &Restaurant) -> Option<String> {
    item.chain
        .as_ref()
        .map(|c| c.key.clone())
        .or_else(|| item.review_group.as_ref().map(|g| g.key.clone()))
}

fn distance_m(item: &Restaurant, location: Option<(f64, f64)>) -> Option<f64> {
    let (lat, lng) = location?;
    let (a, b) = (item.latitude?, item.longitude?);
    Some((crate::providers::openstreetmap::distance_km((lat, lng), (a, b)) * 1000.0).round())
}

struct SearchParams {
    query: String,
    location: Option<(f64, f64)>,
    mode: GlutenMode,
    chain: Option<String>,
}

fn matches_mode(item: &Restaurant, mode: GlutenMode) -> bool {
    match mode {
        GlutenMode::Any => true,
        GlutenMode::Dedicated => item.gluten_support == GlutenSupport::DedicatedGlutenFree,
        GlutenMode::Options => item.gluten_support.is_positive(),
    }
}

/// Exact chain alias for a free-text query ("Burger King", "burgerking", "VIPS").
async fn chain_for_query(state: &AppState, key: &str) -> ApiResult<Option<String>> {
    if key.len() < 3 {
        return Ok(None);
    }
    let exact = sqlx::query_scalar::<_, String>(
        "SELECT chain_key FROM restaurant_chain_aliases WHERE alias=$1
         UNION ALL SELECT key FROM restaurant_chains WHERE alias=$1 LIMIT 1",
    )
    .bind(key)
    .fetch_optional(&state.db)
    .await?;
    if exact.is_some() || key.len() < 5 {
        return Ok(exact);
    }
    // Typos: "burguer king" → burgerking (trigram similarity 0.64), "mcdonals" → 0.58.
    // A typo keeps about the same length: a bare word ("burger", "pizza", both 0.5) is a
    // name search, not one chain.
    Ok(sqlx::query_scalar::<_, String>(
        "SELECT key FROM restaurant_chains WHERE similarity(alias,$1) >= $2
           AND abs(length(alias)-length($1)) <= 2
         ORDER BY similarity(alias,$1) DESC,branch_count DESC LIMIT 1",
    )
    .bind(key)
    .bind(CHAIN_TYPO_SIMILARITY)
    .fetch_optional(&state.db)
    .await?)
}

/// Minimum trigram similarity for a misspelled chain name to still count as that chain.
const CHAIN_TYPO_SIMILARITY: f32 = 0.5;

async fn search_restaurants(state: &AppState, params: SearchParams) -> ApiResult<Vec<Restaurant>> {
    let key = search_key(&params.query);
    let broad = is_broad_query(&params.query);
    let chain = match params.chain.clone() {
        Some(chain) => Some(chain),
        None if !broad => chain_for_query(state, &key).await?,
        None => None,
    };
    // Community restaurants nearby (or by name).
    let local_rows = match params.location {
        Some((lat, lng)) => sqlx::query_as::<_, CommunityRow>(&format!(
            "SELECT {} FROM community_restaurants WHERE latitude BETWEEN $1 AND $2 AND longitude BETWEEN $3 AND $4
             AND ($5::boolean OR restaurant_brand_name(name) LIKE '%'||$6||'%') ORDER BY created_at DESC LIMIT 20",
            community_fields()
        ))
        .bind(lat - 0.025)
        .bind(lat + 0.025)
        .bind(lng - 0.035)
        .bind(lng + 0.035)
        .bind(broad)
        .bind(&key)
        .fetch_all(&state.db)
        .await?,
        None => sqlx::query_as::<_, CommunityRow>(&format!(
            "SELECT {} FROM community_restaurants WHERE $1<>'' AND restaurant_brand_name(name) LIKE '%'||$1||'%' ORDER BY created_at DESC LIMIT 20",
            community_fields()
        ))
        .bind(&key)
        .fetch_all(&state.db)
        .await?,
    };
    let local: Vec<Restaurant> = local_rows.into_iter().map(community).collect();

    let external = async {
        let ready = catalog::ready(state).await?;
        let found = match (&chain, ready) {
            (Some(chain), true) => {
                catalog::chain_branches(state, chain, params.location, params.mode, 50).await?
            }
            (None, true) => {
                catalog::search(state, &params.query, &key, params.location, params.mode).await?
            }
            _ => vec![],
        };
        if !found.is_empty() {
            return Ok(found);
        }
        // Overpass/Photon only where the catalog has nothing (not imported, or no venues in this area).
        let covered = match (ready, params.location) {
            (true, Some((lat, lng))) => catalog::covers(state, lat, lng).await?,
            _ => false,
        };
        if covered {
            return Ok(vec![]);
        }
        state
            .osm
            .search(&RestaurantSearch {
                query: params.query.clone(),
                latitude: params.location.map(|l| l.0),
                longitude: params.location.map(|l| l.1),
                gluten_free: params.mode != GlutenMode::Any,
            })
            .await
    };
    let external: Vec<Restaurant> = match external.await {
        Ok(items) => items,
        Err(error) if local.is_empty() => return Err(error),
        Err(_) => vec![],
    };

    let mut seen = HashSet::new();
    let merged: Vec<Restaurant> = local
        .into_iter()
        .chain(external)
        .filter(|i| seen.insert(i.id.clone()))
        .collect();
    let merged = reports::with_reports(state, merged).await?;
    let mut items: Vec<Restaurant> = merged
        .into_iter()
        .filter(|item| {
            if broad {
                true
            } else if let Some(chain) = &chain {
                item.chain.as_ref().is_some_and(|c| &c.key == chain)
                    || item.community_created == Some(true) && search_key(&item.name).contains(&key)
            } else {
                !key.is_empty()
                    && (search_key(&item.name).contains(&key)
                        || item
                            .chain
                            .as_ref()
                            .is_some_and(|c| search_key(&c.name).contains(&key)))
            }
        })
        .filter(|item| matches_mode(item, params.mode))
        .map(|item| Restaurant {
            distance_m: item
                .distance_m
                .or_else(|| distance_m(&item, params.location)),
            ..item
        })
        .collect();
    if chain.is_some() && params.location.is_some() {
        items.sort_by(|a, b| {
            a.distance_m
                .unwrap_or(f64::MAX)
                .total_cmp(&b.distance_m.unwrap_or(f64::MAX))
        });
    }
    Ok(items)
}

fn location(query: &HashMap<String, String>) -> ApiResult<Option<(f64, f64)>> {
    let lat = util::query_number(query.get("latitude"), -90.0, 90.0)?;
    let lng = util::query_number(query.get("longitude"), -180.0, 180.0)?;
    match (lat, lng) {
        (Some(a), Some(b)) => Ok(Some((a, b))),
        (None, None) => Ok(None),
        _ => Err(ApiError::invalid_input()),
    }
}

fn chain_param(value: Option<&String>) -> ApiResult<Option<String>> {
    match value {
        None => Ok(None),
        Some(v) if CHAIN_KEY.is_match(v) => Ok(Some(v.clone())),
        Some(_) => Err(ApiError::invalid_input()),
    }
}

async fn search(
    State(state): State<AppState>,
    QueryMap(query): QueryMap,
) -> ApiResult<Json<Items<Restaurant>>> {
    let q = util::trimmed(
        query
            .get("q")
            .map(String::as_str)
            .ok_or_else(ApiError::invalid_input)?,
        2,
        120,
    )?;
    let location = location(&query)?;
    let gluten_free = util::query_bool(query.get("glutenFree"))?.unwrap_or(false);
    let mode = match query.get("glutenMode").map(String::as_str) {
        None => {
            if gluten_free {
                GlutenMode::Options
            } else {
                GlutenMode::Any
            }
        }
        Some("dedicated") => GlutenMode::Dedicated,
        Some("options") => GlutenMode::Options,
        Some(_) => return Err(ApiError::invalid_input()),
    };
    let chain = chain_param(query.get("chain"))?;
    let items = search_restaurants(
        &state,
        SearchParams {
            query: q,
            location,
            mode,
            chain,
        },
    )
    .await?;
    Ok(Json(Items { items }))
}

// ---------- map area ----------

/// Every restaurant inside the map area, compact, for the pins: the map loads a much
/// wider area than it shows and keeps what it has, so panning never empties it.
/// Community reports apply as everywhere else (the pin colour matches the list).
async fn area(State(state): State<AppState>, QueryMap(query): QueryMap) -> ApiResult<Json<Value>> {
    let side = |key: &str, min: f64, max: f64| {
        util::query_number(query.get(key), min, max)?.ok_or_else(ApiError::invalid_input)
    };
    let south = side("south", -90.0, 90.0)?;
    let north = side("north", -90.0, 90.0)?;
    let west = side("west", -180.0, 180.0)?;
    let east = side("east", -180.0, 180.0)?;
    if north <= south
        || east <= west
        || north - south > AREA_MAX_SPAN
        || east - west > AREA_MAX_SPAN
    {
        return Err(ApiError::invalid_input());
    }
    let (lat, lng) = ((south + north) / 2.0, (west + east) / 2.0);
    let covered = catalog::ready(&state).await? && catalog::covers(&state, lat, lng).await?;
    let catalog_items = if covered {
        catalog::area(&state, (west, south, east, north), AREA_MAX_PLACES).await?
    } else {
        vec![]
    };
    let truncated = catalog_items.len() as i64 >= AREA_MAX_PLACES;
    let community_items: Vec<Restaurant> = sqlx::query_as::<_, CommunityRow>(&format!(
        "SELECT {} FROM community_restaurants WHERE latitude BETWEEN $1 AND $2 AND longitude BETWEEN $3 AND $4 LIMIT 200",
        community_fields()
    ))
    .bind(south)
    .bind(north)
    .bind(west)
    .bind(east)
    .fetch_all(&state.db)
    .await?
    .into_iter()
    .map(community)
    .collect();

    // Only places someone reported on (directly, through their chain or their name
    // group) need the full report merge; the rest keep their OpenStreetMap tag.
    let reported = sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT DISTINCT restaurant_id,chain_key FROM restaurant_gluten_reports WHERE revoked_at IS NULL",
    )
    .fetch_all(&state.db)
    .await?;
    let ids: HashSet<String> = reported.iter().map(|(id, _)| id.clone()).collect();
    let keys: HashSet<String> = reported.into_iter().filter_map(|(_, key)| key).collect();
    let needs_reports = |item: &Restaurant| {
        item.community_created == Some(true)
            || ids.contains(&item.id)
            || item.chain.as_ref().is_some_and(|c| keys.contains(&c.key))
            || keys.contains(&format!("name:{}", search_key(&item.name)))
    };
    let (merge, plain): (Vec<Restaurant>, Vec<Restaurant>) = catalog_items
        .into_iter()
        .chain(community_items)
        .partition(|item| needs_reports(item));
    let merged = reports::with_reports(&state, merge).await?;

    let items: Vec<Value> = merged
        .into_iter()
        .chain(plain)
        .filter_map(|r| {
            Some(json!({
                "id": r.id,
                "name": r.name,
                "lat": (r.latitude? * 1e5).round() / 1e5,
                "lng": (r.longitude? * 1e5).round() / 1e5,
                "support": r.gluten_support.as_str(),
            }))
        })
        .collect();
    Ok(Json(
        json!({ "items": items, "covered": covered, "truncated": truncated }),
    ))
}

// ---------- suggestions ----------

/// Chains whose name starts with (or contains) the query: "burger k" → Burger King.
async fn chain_suggestions(
    state: &AppState,
    key: &str,
    limit: i64,
) -> ApiResult<Vec<(String, String, i64)>> {
    if key.len() < 2 {
        return Ok(vec![]);
    }
    Ok(sqlx::query_as::<_, (String, String, i64)>(
        "SELECT key,name,branch_count::bigint FROM restaurant_chains
         WHERE alias LIKE $1||'%' OR (length($1)>=4 AND alias LIKE '%'||$1||'%')
            OR (length($1)>=5 AND GREATEST(similarity(alias,$1),word_similarity($1,alias)) >= $3)
         ORDER BY (alias LIKE $1||'%') DESC,(alias LIKE '%'||$1||'%') DESC,
                  GREATEST(similarity(alias,$1),word_similarity($1,alias)) DESC,branch_count DESC,name LIMIT $2",
    )
    .bind(key)
    .bind(limit)
    // While typing ("burguer k") the prefix only half-matches: a bit more tolerant.
    .bind(CHAIN_TYPO_SIMILARITY - 0.05)
    .fetch_all(&state.db)
    .await?)
}

/// Interleave local and external suggestions, without duplicates (port of suggestions.ts).
pub fn combine_suggestions(
    local: Vec<RestaurantSuggestion>,
    external: Vec<RestaurantSuggestion>,
    max: usize,
) -> Vec<RestaurantSuggestion> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    let len = local.len().max(external.len());
    for i in 0..len {
        for item in [local.get(i), external.get(i)].into_iter().flatten() {
            let key = item.id.clone().unwrap_or_else(|| {
                format!(
                    "{:?}:{}:{:?}:{:?}",
                    item.kind, item.label, item.latitude, item.longitude
                )
            });
            if seen.insert(key) {
                result.push(item.clone());
                if result.len() == max {
                    return result;
                }
            }
        }
    }
    result
}

async fn suggest(
    State(state): State<AppState>,
    QueryMap(query): QueryMap,
) -> ApiResult<Json<Items<RestaurantSuggestion>>> {
    let q = util::trimmed(
        query
            .get("q")
            .map(String::as_str)
            .ok_or_else(ApiError::invalid_input)?,
        2,
        120,
    )?;
    let location = location(&query)?;
    let local_only = util::query_bool(query.get("localOnly"))?.unwrap_or(false);
    // Opt-in: 1.0.12 validates `kind` against restaurant|place, so one chain item would hide every suggestion.
    let include_chains = util::query_bool(query.get("includeChains"))?.unwrap_or(false);
    let key = search_key(&q);
    let pattern = format!("%{}%", util::like_escape(&q));
    let community = sqlx::query_as::<_, (String, String, f64, f64)>(
        "SELECT id,name,latitude,longitude FROM community_restaurants
         WHERE name ILIKE $1 OR ($2<>'' AND restaurant_brand_name(name) LIKE '%'||$2||'%') ORDER BY created_at DESC LIMIT 4",
    )
    .bind(&pattern)
    .bind(&key)
    .fetch_all(&state.db);
    let chains = async {
        if include_chains {
            chain_suggestions(&state, &key, 3).await
        } else {
            Ok(vec![])
        }
    };
    let indexed = catalog::suggest(&state, &key, location);
    // Photon only fills gaps: where the catalog covers the area it would just add unlinked duplicates.
    let external = async {
        if local_only {
            return vec![];
        }
        if let Some((lat, lng)) = location {
            if catalog::covers(&state, lat, lng).await.unwrap_or(false) {
                return vec![];
            }
        }
        state
            .osm
            .suggest(&q, location, Duration::from_millis(6500))
            .await
            .unwrap_or_default()
    };
    let (community, chains, indexed, external) = tokio::join!(community, chains, indexed, external);
    let (community, chains, indexed) = (community?, chains?, indexed?);
    let mut local: Vec<RestaurantSuggestion> = community
        .into_iter()
        .map(|(id, name, latitude, longitude)| RestaurantSuggestion {
            id: Some(id),
            label: name.clone(),
            kind: SuggestionKind::Restaurant,
            query: name,
            latitude: Some(latitude),
            longitude: Some(longitude),
            chain_key: None,
            branch_count: None,
        })
        .collect();
    local.extend(indexed);
    let mut items: Vec<RestaurantSuggestion> = chains
        .into_iter()
        .map(|(key, name, count)| RestaurantSuggestion {
            id: Some(format!("chain:{key}")),
            label: format!("Cadena {name} · {count} locales"),
            kind: SuggestionKind::Chain,
            query: name,
            latitude: None,
            longitude: None,
            chain_key: Some(key),
            branch_count: Some(count),
        })
        .collect();
    let room = 10usize.saturating_sub(items.len());
    items.extend(combine_suggestions(local, external, room));
    Ok(Json(Items { items }))
}

// ---------- chains ----------

/// Chain-wide verdict from trusted chain reviews (latest per reviewer).
async fn chain_verdicts(
    state: &AppState,
    keys: &[String],
) -> ApiResult<HashMap<String, RestaurantReportCounts>> {
    let rows = sqlx::query_as::<_, (String, i64, i64, i64)>(
        "WITH latest AS (SELECT DISTINCT ON (chain_key,user_id) chain_key,support FROM restaurant_gluten_reports
           WHERE chain_key=ANY($1) AND scope='chain' AND revoked_at IS NULL ORDER BY chain_key,user_id,updated_at DESC)
         SELECT chain_key,count(*) FILTER (WHERE support='DEDICATED_GLUTEN_FREE'),
           count(*) FILTER (WHERE support='GLUTEN_FREE_OPTIONS'),count(*) FILTER (WHERE support='NO_GLUTEN_FREE_OPTIONS')
         FROM latest GROUP BY chain_key",
    )
    .bind(keys)
    .fetch_all(&state.db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(key, dedicated, options, no_options)| {
            (
                key,
                RestaurantReportCounts {
                    dedicated,
                    options,
                    no_options,
                },
            )
        })
        .collect())
}

fn summary(
    key: String,
    name: String,
    branch_count: i64,
    reports: RestaurantReportCounts,
) -> ChainSummary {
    ChainSummary {
        key,
        name,
        branch_count,
        gluten_support: reports::support_from_users(reports),
        reports,
    }
}

async fn list_chains(
    State(state): State<AppState>,
    QueryMap(query): QueryMap,
) -> ApiResult<Json<Items<ChainSummary>>> {
    let q = util::trimmed(
        query
            .get("q")
            .map(String::as_str)
            .ok_or_else(ApiError::invalid_input)?,
        2,
        120,
    )?;
    let rows = chain_suggestions(&state, &search_key(&q), 20).await?;
    let keys: Vec<String> = rows.iter().map(|r| r.0.clone()).collect();
    let verdicts = chain_verdicts(&state, &keys).await?;
    Ok(Json(Items {
        items: rows
            .into_iter()
            .map(|(key, name, count)| {
                let reports = verdicts.get(&key).copied().unwrap_or_default();
                summary(key, name, count, reports)
            })
            .collect(),
    }))
}

async fn chain_detail(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(key): Path<String>,
    QueryMap(query): QueryMap,
) -> ApiResult<Json<ChainDetail>> {
    let key = chain_param(Some(&key))?.ok_or_else(ApiError::invalid_input)?;
    let location = location(&query)?;
    let (name, branch_count) = sqlx::query_as::<_, (String, i64)>(
        "SELECT name,branch_count::bigint FROM restaurant_chains WHERE key=$1",
    )
    .bind(&key)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(ApiError::not_found)?;
    let branches = catalog::chain_branches(&state, &key, location, GlutenMode::Any, 50).await?;
    let branches = reports::with_reports(&state, branches).await?;
    let reports = chain_verdicts(&state, std::slice::from_ref(&key))
        .await?
        .remove(&key)
        .unwrap_or_default();
    let can_mark_chain = match user {
        Some(user) => is_trusted(&state, user.id).await?,
        None => false,
    };
    Ok(Json(ChainDetail {
        chain: summary(key, name, branch_count, reports),
        branches,
        total: branch_count,
        can_mark_chain,
    }))
}

// ---------- create / report / show ----------

fn support_param(value: &str) -> ApiResult<GlutenSupport> {
    match GlutenSupport::parse(value) {
        Some(GlutenSupport::Unknown) | None => Err(ApiError::invalid_input()),
        Some(s) => Ok(s),
    }
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    JsonBody(body): JsonBody,
) -> ApiResult<Response> {
    let object = util::strict_object(
        &body,
        &[
            "name",
            "address",
            "latitude",
            "longitude",
            "support",
            "note",
        ],
    )?;
    let name = util::trimmed(util::req_str(object, "name")?, 2, 160)?;
    let address = util::trimmed(util::req_str(object, "address")?, 5, 300)?;
    let latitude = util::req_f64(object, "latitude", -90.0, 90.0)?;
    let longitude = util::req_f64(object, "longitude", -180.0, 180.0)?;
    let support = support_param(util::req_str(object, "support")?)?;
    let note = util::trimmed(util::opt_str(object, "note")?.unwrap_or(""), 0, 500)?;
    let id = format!("se_{}", Uuid::new_v4().simple());
    let note = review_note(&state, auth.id, &note).await?;
    let duplicate = sqlx::query_scalar::<_, i32>(
        "SELECT 1 FROM community_restaurants WHERE lower(name)=lower($1) AND abs(latitude-$2)<0.0005 AND abs(longitude-$3)<0.0005 LIMIT 1",
    )
    .bind(&name)
    .bind(latitude)
    .bind(longitude)
    .fetch_optional(&state.db)
    .await?;
    if duplicate.is_some() {
        return Err(ApiError::new(
            409,
            "RESTAURANT_EXISTS",
            "Ese restaurante ya aparece cerca de aquí.",
        ));
    }
    let mut tx = state.db.begin().await?;
    sqlx::query("INSERT INTO community_restaurants(id,user_id,name,address,latitude,longitude) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(&id)
        .bind(auth.id)
        .bind(&name)
        .bind(&address)
        .bind(latitude)
        .bind(longitude)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO restaurant_gluten_reports(restaurant_id,user_id,support,note) VALUES($1,$2,$3,$4)")
        .bind(&id)
        .bind(auth.id)
        .bind(support.as_str())
        .bind(&note)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok((
        StatusCode::CREATED,
        Json(get_restaurant(&state, &id).await?),
    )
        .into_response())
}

async fn gluten_report(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Restaurant>> {
    let id = util::restaurant_id(&id)?;
    let object = util::strict_object(&body, &["support", "note", "scope"])?;
    let support = support_param(util::req_str(object, "support")?)?;
    let note = util::trimmed(util::opt_str(object, "note")?.unwrap_or(""), 0, 500)?;
    let requested = match util::opt_str(object, "scope")? {
        None => None,
        Some("local") => Some("local"),
        Some("chain") => Some("chain"),
        Some(_) => return Err(ApiError::invalid_input()),
    };
    let item = get_restaurant(&state, &id).await?;
    let note = review_note(&state, auth.id, &note).await?;
    let trusted = is_trusted(&state, auth.id).await?;
    let scope = requested.unwrap_or(
        if trusted && support != GlutenSupport::NoGlutenFreeOptions && item.chain.is_some() {
            "chain"
        } else {
            "local"
        },
    );
    if scope == "chain" && !trusted {
        return Err(ApiError::new(
            403,
            "CHAIN_REVIEW_FORBIDDEN",
            "Solo las cuentas de confianza pueden marcar una cadena entera. Elige «Solo este restaurante» para enviar tu revisión.",
        ));
    }
    let chain_key = group_key(&item);
    if scope == "chain" && chain_key.is_none() {
        return Err(ApiError::new(
            400,
            "CHAIN_NOT_AVAILABLE",
            "Todavía no encontramos otros locales de este restaurante.",
        ));
    }
    let written = sqlx::query(
        "INSERT INTO restaurant_gluten_reports(restaurant_id,user_id,support,note,chain_key,scope) VALUES($1,$2,$3,$4,$5,$6)
         ON CONFLICT(restaurant_id,user_id) DO UPDATE SET support=$3,note=$4,chain_key=$5,scope=$6,updated_at=now()
         WHERE restaurant_gluten_reports.revoked_at IS NULL RETURNING id",
    )
    .bind(&id)
    .bind(auth.id)
    .bind(support.as_str())
    .bind(&note)
    .bind(if scope == "chain" { chain_key } else { None })
    .bind(scope)
    .fetch_optional(&state.db)
    .await?;
    if written.is_none() {
        return Err(ApiError::new(
            409,
            "REVIEW_REVOKED",
            "Esta revisión ha sido retirada. Contacta con la administración para revisarla.",
        ));
    }
    Ok(Json(get_restaurant(&state, &id).await?))
}

async fn show(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Restaurant>> {
    let id = util::restaurant_id(&id)?;
    Ok(Json(get_restaurant(&state, &id).await?))
}

// ---------- ratings ----------

async fn my_rating(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    let id = util::restaurant_id(&id)?;
    get_restaurant(&state, &id).await?;
    let stars = sqlx::query_scalar::<_, i16>(
        "SELECT stars FROM restaurant_ratings WHERE restaurant_id=$1 AND user_id=$2",
    )
    .bind(&id)
    .bind(user.map(|u| u.id))
    .fetch_optional(&state.db)
    .await?;
    Ok(Json(json!({"stars": stars})))
}

async fn put_rating(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Restaurant>> {
    let id = util::restaurant_id(&id)?;
    let object = util::strict_object(&body, &["stars"])?;
    let stars = object
        .get("stars")
        .and_then(Value::as_i64)
        .filter(|s| (1..=5).contains(s))
        .ok_or_else(ApiError::invalid_input)?;
    if object
        .get("stars")
        .and_then(Value::as_f64)
        .is_some_and(|f| f.fract() != 0.0)
    {
        return Err(ApiError::invalid_input());
    }
    get_restaurant(&state, &id).await?;
    sqlx::query(
        "INSERT INTO restaurant_ratings(restaurant_id,user_id,stars) VALUES($1,$2,$3) ON CONFLICT(restaurant_id,user_id) DO UPDATE SET stars=$3,updated_at=now()",
    )
    .bind(&id)
    .bind(auth.id)
    .bind(stars as i16)
    .execute(&state.db)
    .await?;
    Ok(Json(get_restaurant(&state, &id).await?))
}

async fn delete_rating(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<String>,
) -> ApiResult<Json<Restaurant>> {
    let id = util::restaurant_id(&id)?;
    get_restaurant(&state, &id).await?;
    sqlx::query("DELETE FROM restaurant_ratings WHERE restaurant_id=$1 AND user_id=$2")
        .bind(&id)
        .bind(auth.id)
        .execute(&state.db)
        .await?;
    Ok(Json(get_restaurant(&state, &id).await?))
}

// ---------- reviews ----------

async fn list_reviews(
    State(state): State<AppState>,
    MaybeUser(user): MaybeUser,
    Path(id): Path<String>,
) -> ApiResult<Response> {
    util::bounded(&id, 0, 200)?;
    let item = get_restaurant(&state, &id).await?;
    Ok(reviews::list(
        &state,
        Kind::Restaurants,
        &id,
        group_key(&item).as_deref(),
        user.map(|u| u.id),
    )
    .await?
    .into_response())
}

async fn confirm_review(
    State(state): State<AppState>,
    auth: AuthUser,
    Path((id, review)): Path<(String, String)>,
    JsonBody(body): JsonBody,
) -> ApiResult<Json<Value>> {
    util::bounded(&id, 0, 200)?;
    util::uuid_param(&review)?;
    let item = get_restaurant(&state, &id).await?;
    reviews::confirm(
        &state,
        Kind::Restaurants,
        &id,
        group_key(&item).as_deref(),
        &review,
        auth.id,
        &body,
    )
    .await
}

pub fn routes(state: &AppState) -> Router<AppState> {
    use crate::app::limit;
    Router::new()
        .route(
            "/api/v1/restaurants/suggest",
            get(suggest).route_layer(limit(state, SUGGEST_LIMIT)),
        )
        .route(
            "/api/v1/restaurants/search",
            get(search).route_layer(limit(state, SEARCH_LIMIT)),
        )
        .route(
            "/api/v1/restaurants/area",
            // About 250 kB of JSON for a busy city, a fifth of that compressed.
            get(area)
                .route_layer(tower_http::compression::CompressionLayer::new())
                .route_layer(limit(state, AREA_LIMIT)),
        )
        .route(
            "/api/v1/restaurants/chains",
            get(list_chains).route_layer(limit(state, SUGGEST_LIMIT)),
        )
        .route(
            "/api/v1/restaurants/chains/{key}",
            get(chain_detail).route_layer(limit(state, SEARCH_LIMIT)),
        )
        .route(
            "/api/v1/restaurants",
            post(create).route_layer(limit(state, CREATE_LIMIT)),
        )
        .route("/api/v1/restaurants/{id}", get(show))
        .route(
            "/api/v1/restaurants/{id}/gluten-report",
            post(gluten_report).route_layer(limit(state, REPORT_LIMIT)),
        )
        .route(
            "/api/v1/restaurants/{id}/rating",
            get(my_rating)
                .merge(axum::routing::put(put_rating).route_layer(limit(state, RATING_LIMIT)))
                .merge(axum::routing::delete(delete_rating)),
        )
        .route("/api/v1/restaurants/{id}/reviews", get(list_reviews))
        .route(
            "/api/v1/restaurants/{id}/reviews/{review}/confirm",
            post(confirm_review).route_layer(limit(state, reviews::CONFIRM_RESTAURANT_LIMIT)),
        )
}
