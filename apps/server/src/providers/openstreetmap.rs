//! OpenStreetMap via Photon (search), Overpass (nearby) and the OSM API (details).
//! Port of apps/api/src/providers/openstreetmap.ts; used only when the local
//! catalog has no venues for the area.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use once_cell::sync::Lazy;
use regex::Regex;
use safeeat_contracts::{
    ChainRef, Evidence, EvidenceKind, GlutenSupport, Restaurant, RestaurantSuggestion,
    SuggestionKind,
};
use serde_json::Value;

use super::http::{provider_json, ProviderBudget, TtlCache};
use crate::error::ApiError;
use crate::util::{now_iso, safe_http_url};

const FOOD_TYPES: &str = "restaurant|cafe|fast_food|bar|pub|food_court|biergarten";
static FOOD_TYPE: Lazy<Regex> = Lazy::new(|| Regex::new(&format!("^({FOOD_TYPES})$")).unwrap());
static GLUTEN_NAME: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)sin\s+gluten|gluten\s*free|celi[a-záí]+").unwrap());
static BROAD: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?i)^(restaurantes?|cerca de m[ií])$").unwrap());
static WIKIDATA: Lazy<Regex> = Lazy::new(|| Regex::new(r"^Q\d+$").unwrap());
const FOOD_SHOPS: &[&str] = &["bakery", "pastry", "convenience"];

pub fn is_broad_query(query: &str) -> bool {
    BROAD.is_match(query)
}

#[derive(Debug, Clone, Default)]
pub struct RestaurantSearch {
    pub query: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub gluten_free: bool,
}

#[derive(Debug, Clone)]
struct Feature {
    osm_type: &'static str,
    osm_id: i64,
    name: Option<String>,
    street: Option<String>,
    housenumber: Option<String>,
    city: Option<String>,
    state: Option<String>,
    country: Option<String>,
    osm_key: Option<String>,
    osm_value: Option<String>,
    lon: f64,
    lat: f64,
}

impl Feature {
    fn is_food(&self) -> bool {
        (self.osm_key.as_deref() == Some("amenity")
            && FOOD_TYPE.is_match(self.osm_value.as_deref().unwrap_or("")))
            || (self.osm_key.as_deref() == Some("shop")
                && FOOD_SHOPS.contains(&self.osm_value.as_deref().unwrap_or("")))
    }
    fn is_food_amenity(&self) -> bool {
        self.osm_key.as_deref() == Some("amenity")
            && FOOD_TYPE.is_match(self.osm_value.as_deref().unwrap_or(""))
    }
    fn address(&self) -> String {
        [
            &self.street,
            &self.housenumber,
            &self.city,
            &self.state,
            &self.country,
        ]
        .iter()
        .filter_map(|v| v.as_deref().filter(|v| !v.is_empty()))
        .collect::<Vec<_>>()
        .join(", ")
    }
}

fn parse_features(raw: Option<Value>) -> Result<Vec<Feature>, ApiError> {
    let invalid = ApiError::provider_unavailable;
    let features = raw
        .as_ref()
        .and_then(|r| r.get("features"))
        .and_then(Value::as_array)
        .ok_or_else(invalid)?;
    let mut out = Vec::new();
    for f in features {
        let p = f
            .get("properties")
            .and_then(Value::as_object)
            .ok_or_else(invalid)?;
        let osm_type = match p.get("osm_type").and_then(Value::as_str) {
            Some("N") => "node",
            Some("W") => "way",
            Some("R") => "relation",
            _ => return Err(invalid()),
        };
        let coords = f
            .pointer("/geometry/coordinates")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        let s = |k: &str| p.get(k).and_then(Value::as_str).map(str::to_string);
        out.push(Feature {
            osm_type,
            osm_id: p
                .get("osm_id")
                .and_then(Value::as_i64)
                .ok_or_else(invalid)?,
            name: s("name"),
            street: s("street"),
            housenumber: s("housenumber"),
            city: s("city"),
            state: s("state"),
            country: s("country"),
            osm_key: s("osm_key"),
            osm_value: s("osm_value"),
            lon: coords.first().and_then(Value::as_f64).ok_or_else(invalid)?,
            lat: coords.get(1).and_then(Value::as_f64).ok_or_else(invalid)?,
        });
    }
    Ok(out)
}

pub fn distance_km(a: (f64, f64), b: (f64, f64)) -> f64 {
    let r = std::f64::consts::PI / 180.0;
    let lat = (b.0 - a.0) * r;
    let lon = (b.1 - a.1) * r;
    let v =
        (lat / 2.0).sin().powi(2) + (a.0 * r).cos() * (b.0 * r).cos() * (lon / 2.0).sin().powi(2);
    12742.0 * v.sqrt().atan2((1.0 - v).sqrt())
}

pub fn gluten_from_tag(tag: Option<&str>) -> GlutenSupport {
    match tag {
        Some("only") => GlutenSupport::DedicatedGlutenFree,
        Some("yes") => GlutenSupport::GlutenFreeOptions,
        Some("no") => GlutenSupport::NoGlutenFreeOptions,
        _ => GlutenSupport::Unknown,
    }
}

struct Input<'a> {
    kind: &'a str,
    id: i64,
    name: String,
    address: String,
    lat: Option<f64>,
    lon: Option<f64>,
    tags: Option<&'a HashMap<String, String>>,
    gluten_name_match: bool,
}

fn restaurant(input: Input) -> Restaurant {
    let tag = |k: &str| {
        input
            .tags
            .and_then(|t| t.get(k))
            .map(String::as_str)
            .filter(|v| !v.is_empty())
    };
    let support = gluten_from_tag(tag("diet:gluten_free"));
    let text = if input.gluten_name_match {
        "El nombre de la ficha menciona opciones sin gluten. Confirma ingredientes y contaminación cruzada con el establecimiento."
    } else {
        match support {
            GlutenSupport::DedicatedGlutenFree => "La ficha indica que el local ofrece prácticamente todo sin gluten. Confirma la preparación antes de acudir.",
            GlutenSupport::GlutenFreeOptions => "La ficha indica opciones sin gluten. Confirma ingredientes y contaminación cruzada.",
            GlutenSupport::NoGlutenFreeOptions => "La ficha indica que no ofrece opciones sin gluten.",
            GlutenSupport::Unknown => "Aún no hay información sobre opciones sin gluten.",
        }
    };
    let wikidata = tag("brand:wikidata").filter(|v| WIKIDATA.is_match(v));
    Restaurant {
        id: format!("osm_{}_{}", &input.kind[..1], input.id),
        chain: wikidata.map(|key| ChainRef {
            key: key.into(),
            name: tag("brand")
                .map(str::to_string)
                .unwrap_or_else(|| input.name.clone()),
        }),
        name: input.name,
        address: input.address,
        latitude: input.lat,
        longitude: input.lon,
        website: safe_http_url(tag("website").or_else(|| tag("contact:website"))),
        phone: tag("phone")
            .or_else(|| tag("contact:phone"))
            .map(str::to_string),
        maps_url: format!("https://www.openstreetmap.org/{}/{}", input.kind, input.id),
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
        fetched_at: now_iso(),
        distance_m: None,
        has_local_review: None,
    }
}

fn from_feature(f: &Feature, gluten_name_match: bool) -> Option<Restaurant> {
    let name = f.name.clone()?;
    if !f.is_food() || (gluten_name_match && !GLUTEN_NAME.is_match(&name)) {
        return None;
    }
    Some(restaurant(Input {
        kind: f.osm_type,
        id: f.osm_id,
        name,
        address: f.address(),
        lat: Some(f.lat),
        lon: Some(f.lon),
        tags: None,
        gluten_name_match,
    }))
}

#[derive(Debug)]
struct Element {
    kind: String,
    id: i64,
    lat: Option<f64>,
    lon: Option<f64>,
    tags: HashMap<String, String>,
}

fn parse_elements(raw: Option<Value>) -> Result<Vec<Element>, ApiError> {
    let elements = raw
        .as_ref()
        .and_then(|r| r.get("elements"))
        .and_then(Value::as_array)
        .ok_or_else(ApiError::provider_unavailable)?;
    Ok(elements
        .iter()
        .filter_map(|e| {
            let kind = e
                .get("type")
                .and_then(Value::as_str)
                .filter(|t| matches!(*t, "node" | "way" | "relation"))?;
            Some(Element {
                kind: kind.into(),
                id: e.get("id").and_then(Value::as_i64)?,
                lat: e
                    .get("lat")
                    .and_then(Value::as_f64)
                    .or_else(|| e.pointer("/center/lat").and_then(Value::as_f64)),
                lon: e
                    .get("lon")
                    .and_then(Value::as_f64)
                    .or_else(|| e.pointer("/center/lon").and_then(Value::as_f64)),
                tags: e
                    .get("tags")
                    .and_then(Value::as_object)
                    .map(|t| {
                        t.iter()
                            .filter_map(|(k, v)| Some((k.clone(), v.as_str()?.to_string())))
                            .collect()
                    })
                    .unwrap_or_default(),
            })
        })
        .collect())
}

fn element_restaurant(e: &Element) -> Option<Restaurant> {
    let name = e.tags.get("name")?.clone();
    let address = ["addr:street", "addr:housenumber", "addr:city"]
        .iter()
        .filter_map(|k| e.tags.get(*k).filter(|v| !v.is_empty()).map(String::as_str))
        .collect::<Vec<_>>()
        .join(", ");
    Some(restaurant(Input {
        kind: &e.kind,
        id: e.id,
        name,
        address,
        lat: e.lat,
        lon: e.lon,
        tags: Some(&e.tags),
        gluten_name_match: false,
    }))
}

pub struct OpenStreetMap {
    http: reqwest::Client,
    budget: ProviderBudget,
    restaurants: TtlCache<Vec<Restaurant>>,
    suggestions: TtlCache<Vec<RestaurantSuggestion>>,
}

impl OpenStreetMap {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            http,
            budget: ProviderBudget::new(45),
            restaurants: TtlCache::new(Duration::from_secs(3600), 150),
            suggestions: TtlCache::new(Duration::from_secs(3600), 150),
        }
    }

    async fn photon(
        &self,
        q: &str,
        limit: u32,
        location: Option<(f64, f64)>,
        timeout: Duration,
    ) -> Result<Vec<Feature>, ApiError> {
        self.budget.take()?;
        let mut params = vec![
            ("q".to_string(), q.to_string()),
            ("limit".into(), limit.to_string()),
        ];
        if let Some((lat, lon)) = location {
            params.push(("lat".into(), lat.to_string()));
            params.push(("lon".into(), lon.to_string()));
        }
        parse_features(
            provider_json(
                self.http
                    .get("https://photon.komoot.io/api/")
                    .query(&params),
                timeout,
            )
            .await?,
        )
    }

    pub async fn suggest(
        &self,
        query: &str,
        location: Option<(f64, f64)>,
        timeout: Duration,
    ) -> Result<Vec<RestaurantSuggestion>, ApiError> {
        let rounded = location
            .map(|(lat, lon)| ((lat * 100.0).round() / 100.0, (lon * 100.0).round() / 100.0));
        let key = format!(
            "suggest:{}:{}:{}",
            query.to_lowercase(),
            rounded.map(|r| r.0.to_string()).unwrap_or_default(),
            rounded.map(|r| r.1.to_string()).unwrap_or_default()
        );
        if let Some(old) = self.suggestions.get(&key) {
            return Ok(old);
        }
        let found = self.photon(query, 12, rounded, timeout).await?;
        let mut seen = HashSet::new();
        let items = found
            .iter()
            .filter(|f| f.name.is_some())
            .filter(|f| {
                seen.insert(format!(
                    "{}:{}:{}",
                    f.name.as_deref().unwrap_or(""),
                    f.city.as_deref().unwrap_or(""),
                    f.osm_key.as_deref().unwrap_or("")
                ))
            })
            .map(|f| {
                let food = f.is_food_amenity();
                let place = f
                    .city
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .or(f.country.as_deref())
                    .filter(|v| !v.is_empty());
                RestaurantSuggestion {
                    id: None,
                    label: [f.name.as_deref(), place]
                        .iter()
                        .flatten()
                        .copied()
                        .collect::<Vec<_>>()
                        .join(" · "),
                    kind: if food {
                        SuggestionKind::Restaurant
                    } else {
                        SuggestionKind::Place
                    },
                    query: if food {
                        f.name.clone().unwrap_or_default()
                    } else {
                        "restaurantes".into()
                    },
                    latitude: Some(f.lat),
                    longitude: Some(f.lon),
                    chain_key: None,
                    branch_count: None,
                }
            })
            .collect();
        Ok(self.suggestions.put(key, items))
    }

    async fn nearby(
        &self,
        lat: f64,
        lon: f64,
        gluten_free: bool,
    ) -> Result<Vec<Restaurant>, ApiError> {
        self.budget.take()?;
        let tag = if gluten_free {
            r#"["diet:gluten_free"~"^(yes|only|limited)$"]"#
        } else {
            ""
        };
        let radius = if gluten_free { 4000 } else { 1800 };
        let q = format!(
            r#"[out:json][timeout:7];nwr[amenity~"^({FOOD_TYPES})$"]{tag}(around:{radius},{lat},{lon});out center {};"#,
            if gluten_free { 60 } else { 50 }
        );
        let request = self
            .http
            .post("https://overpass-api.de/api/interpreter")
            .form(&[("data", q)]);
        let elements = parse_elements(provider_json(request, Duration::from_millis(7500)).await?)?;
        Ok(elements.iter().filter_map(element_restaurant).collect())
    }

    async fn nearby_with_fallback(
        &self,
        lat: f64,
        lon: f64,
        gluten_free: bool,
    ) -> Result<Vec<Restaurant>, ApiError> {
        match self.nearby(lat, lon, gluten_free).await {
            Ok(items) => Ok(items),
            Err(_) if gluten_free => {
                let mut seen = HashSet::new();
                let mut out = Vec::new();
                for q in ["sin gluten", "gluten free", "celiaco"] {
                    for f in self
                        .photon(q, 30, Some((lat, lon)), Duration::from_secs(8))
                        .await?
                    {
                        if let Some(item) = from_feature(&f, true) {
                            if distance_km((lat, lon), (f.lat, f.lon)) <= 25.0
                                && seen.insert(item.id.clone())
                            {
                                out.push(item);
                            }
                        }
                    }
                }
                out.truncate(30);
                Ok(out)
            }
            Err(_) => {
                let found = self
                    .photon("restaurantes", 50, Some((lat, lon)), Duration::from_secs(8))
                    .await?;
                Ok(found
                    .iter()
                    .filter_map(|f| {
                        from_feature(f, false)
                            .filter(|_| distance_km((lat, lon), (f.lat, f.lon)) <= 25.0)
                    })
                    .take(50)
                    .collect())
            }
        }
    }

    pub async fn search(&self, params: &RestaurantSearch) -> Result<Vec<Restaurant>, ApiError> {
        let key = format!(
            "search:{}:{:?}:{:?}:{}",
            params.query.to_lowercase(),
            params.latitude,
            params.longitude,
            params.gluten_free
        );
        if let Some(old) = self.restaurants.get(&key) {
            return Ok(old);
        }
        let location = params.latitude.zip(params.longitude);
        if let Some((lat, lon)) = location {
            if is_broad_query(&params.query) {
                let items = self
                    .nearby_with_fallback(lat, lon, params.gluten_free)
                    .await?;
                return Ok(self.restaurants.put(key, items));
            }
        }
        let matches_place = |f: &Feature| {
            f.name
                .as_deref()
                .is_some_and(|n| n.to_lowercase() == params.query.to_lowercase())
        };
        if location.is_none() {
            let places = self
                .photon(&params.query, 5, None, Duration::from_secs(8))
                .await?;
            if let Some(place) = places
                .iter()
                .find(|f| matches_place(f) && !f.is_food_amenity())
            {
                let items = self
                    .nearby_with_fallback(place.lat, place.lon, params.gluten_free)
                    .await?;
                return Ok(self.restaurants.put(key, items));
            }
        }
        let places: Vec<Restaurant> = self
            .photon(&params.query, 20, location, Duration::from_secs(8))
            .await?
            .iter()
            .filter_map(|f| from_feature(f, false))
            .collect();
        if !places.is_empty() {
            return Ok(self.restaurants.put(key, places));
        }
        let locations = self
            .photon(&params.query, 5, None, Duration::from_secs(8))
            .await?;
        let items = match locations.iter().find(|f| matches_place(f)) {
            Some(place) => {
                self.nearby_with_fallback(place.lat, place.lon, params.gluten_free)
                    .await?
            }
            None => vec![],
        };
        Ok(self.restaurants.put(key, items))
    }

    pub async fn get_by_id(&self, id: &str) -> Result<Option<Restaurant>, ApiError> {
        static ID: Lazy<Regex> = Lazy::new(|| Regex::new(r"^osm_([nwr])_(\d+)$").unwrap());
        let Some(caps) = ID.captures(id) else {
            return Ok(None);
        };
        self.budget.take()?;
        let kind = match &caps[1] {
            "n" => "node",
            "w" => "way",
            _ => "relation",
        };
        let url = format!(
            "https://api.openstreetmap.org/api/0.6/{kind}/{}.json",
            &caps[2]
        );
        let Some(raw) = provider_json(self.http.get(url), Duration::from_secs(8)).await? else {
            return Ok(None);
        };
        let elements = parse_elements(Some(raw))?;
        let Some(item) = elements.first() else {
            return Ok(None);
        };
        let food = item
            .tags
            .get("amenity")
            .is_some_and(|a| FOOD_TYPE.is_match(a))
            || item
                .tags
                .get("shop")
                .is_some_and(|s| FOOD_SHOPS.contains(&s.as_str()));
        if !food {
            return Ok(None);
        }
        Ok(element_restaurant(item))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broad_queries() {
        assert!(is_broad_query("Restaurantes"));
        assert!(is_broad_query("cerca de mí"));
        assert!(!is_broad_query("burger king"));
    }

    #[test]
    fn distance() {
        let d = distance_km((40.4168, -3.7038), (41.3874, 2.1686));
        assert!((d - 505.0).abs() < 5.0, "{d}");
    }
}
