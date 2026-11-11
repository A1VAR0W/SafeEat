//! Community data on top of restaurants: chains, ratings, same-name groups and reports.
//!
//! SafeEat 2.0 rules:
//! - A branch's own (local) reviews always override its chain's reviews.
//! - Same-name groups (`name:` keys, restaurants without a real chain) only share
//!   reviews within 30 km, so two unrelated "Casa Pepe" never mix.

use std::collections::HashMap;

use safeeat_contracts::{
    ChainRef, Evidence, EvidenceKind, GlutenSupport, RatingSummary, Restaurant,
    RestaurantReportCounts, ReviewGroup,
};
use safeeat_text_norm::{brand_part, search_key};

use crate::error::ApiResult;
use crate::state::AppState;

pub const NAME_GROUP_RADIUS_M: f64 = 30_000.0;

/// What users say, as in the TS API: contradictions are UNKNOWN.
pub fn support_from_users(counts: RestaurantReportCounts) -> GlutenSupport {
    let positive = counts.dedicated + counts.options;
    if counts.no_options > 0 && positive > 0 {
        GlutenSupport::Unknown
    } else if counts.no_options > 0 {
        GlutenSupport::NoGlutenFreeOptions
    } else if counts.dedicated > 0 && counts.options == 0 {
        GlutenSupport::DedicatedGlutenFree
    } else if positive > 0 {
        GlutenSupport::GlutenFreeOptions
    } else {
        GlutenSupport::Unknown
    }
}

pub async fn with_reports(state: &AppState, items: Vec<Restaurant>) -> ApiResult<Vec<Restaurant>> {
    if items.is_empty() {
        return Ok(items);
    }
    let unchained: Vec<&Restaurant> = items.iter().filter(|i| i.chain.is_none()).collect();
    let alias_ids: Vec<String> = unchained.iter().map(|i| i.id.clone()).collect();
    let alias_keys: Vec<String> = unchained
        .iter()
        .map(|i| search_key(brand_part(&i.name)))
        .collect();
    let ids: Vec<String> = items.iter().map(|i| i.id.clone()).collect();
    let name_keys: Vec<String> = items.iter().map(|i| search_key(&i.name)).collect();

    let aliases = sqlx::query_as::<_, (String, String, String)>(
        "SELECT target.id,a.chain_key,a.chain_name FROM unnest($1::text[],$2::text[]) AS target(id,alias)
         JOIN restaurant_chain_aliases a ON a.alias=target.alias",
    )
    .bind(&alias_ids)
    .bind(&alias_keys)
    .fetch_all(&state.db);
    let ratings = sqlx::query_as::<_, (String, f64, i64)>(
        "SELECT restaurant_id,avg(stars)::float8,count(*)::bigint FROM restaurant_ratings WHERE restaurant_id=ANY($1::text[]) GROUP BY restaurant_id",
    )
    .bind(&ids)
    .fetch_all(&state.db);
    let groups = sqlx::query_as::<_, (String, String, i64)>(
        "SELECT name_key,min(name),count(DISTINCT id)::bigint FROM (
           SELECT id,name,search_name AS name_key FROM restaurant_catalog WHERE search_name=ANY($1::text[]) AND search_name<>''
           UNION ALL SELECT id,name,restaurant_brand_name(name) FROM community_restaurants WHERE restaurant_brand_name(name)=ANY($1::text[]) AND restaurant_brand_name(name)<>''
         ) matches GROUP BY name_key HAVING count(DISTINCT id)>1",
    )
    .bind(&name_keys)
    .fetch_all(&state.db);
    let (aliases, ratings, groups) = futures::try_join!(aliases, ratings, groups)?;

    let chains: HashMap<String, ChainRef> = aliases
        .into_iter()
        .map(|(id, key, name)| (id, ChainRef { key, name }))
        .collect();
    let scores: HashMap<String, RatingSummary> = ratings
        .into_iter()
        .map(|(id, average, count)| (id, RatingSummary { average, count }))
        .collect();
    let same: HashMap<String, ReviewGroup> = groups
        .into_iter()
        .map(|(key, name, count)| {
            (
                key.clone(),
                ReviewGroup {
                    key: format!("name:{key}"),
                    name,
                    count,
                },
            )
        })
        .collect();
    let items: Vec<Restaurant> = items
        .into_iter()
        .map(|item| Restaurant {
            chain: item.chain.clone().or_else(|| chains.get(&item.id).cloned()),
            rating: scores.get(&item.id).cloned(),
            review_group: same.get(&search_key(&item.name)).cloned(),
            ..item
        })
        .collect();

    let group_keys: Vec<Option<String>> = items
        .iter()
        .map(|i| {
            i.chain
                .as_ref()
                .map(|c| c.key.clone())
                .or_else(|| i.review_group.as_ref().map(|g| g.key.clone()))
        })
        .collect();
    let lats: Vec<Option<f64>> = items.iter().map(|i| i.latitude).collect();
    let lngs: Vec<Option<f64>> = items.iter().map(|i| i.longitude).collect();
    let rows = sqlx::query_as::<_, (String, i64, i64, i64, bool)>(
        "WITH target AS (
           SELECT * FROM unnest($1::text[],$2::text[],$3::float8[],$4::float8[]) AS t(id,chain_key,lat,lng)
         ), relevant AS (
           SELECT DISTINCT ON (target.id,r.user_id) target.id AS restaurant_id,r.support,(r.restaurant_id=target.id) AS local
           FROM target JOIN restaurant_gluten_reports r ON r.restaurant_id=target.id OR (
             r.chain_key=target.chain_key AND r.chain_key IS NOT NULL AND (
               r.chain_key NOT LIKE 'name:%' OR target.lat IS NULL OR EXISTS(
                 SELECT 1 FROM restaurant_catalog c WHERE c.id=r.restaurant_id
                   AND ST_DWithin(c.location,ST_SetSRID(ST_MakePoint(target.lng,target.lat),4326)::geography,$5)
                 UNION ALL
                 SELECT 1 FROM community_restaurants cr WHERE cr.id=r.restaurant_id
                   AND ST_DWithin(ST_SetSRID(ST_MakePoint(cr.longitude,cr.latitude),4326)::geography,
                                  ST_SetSRID(ST_MakePoint(target.lng,target.lat),4326)::geography,$5))))
           WHERE r.revoked_at IS NULL
           ORDER BY target.id,r.user_id,(r.restaurant_id=target.id) DESC,r.updated_at DESC,r.id
         ), scoped AS (
           SELECT x.*,bool_or(x.local) OVER (PARTITION BY x.restaurant_id) AS has_local FROM relevant x
         )
         SELECT restaurant_id,
           count(*) FILTER (WHERE support='DEDICATED_GLUTEN_FREE'),
           count(*) FILTER (WHERE support='GLUTEN_FREE_OPTIONS'),
           count(*) FILTER (WHERE support='NO_GLUTEN_FREE_OPTIONS'),
           bool_or(has_local)
         FROM scoped WHERE local OR NOT has_local GROUP BY restaurant_id",
    )
    .bind(&ids)
    .bind(&group_keys)
    .bind(&lats)
    .bind(&lngs)
    .bind(NAME_GROUP_RADIUS_M)
    .fetch_all(&state.db)
    .await?;
    let by_id: HashMap<String, (RestaurantReportCounts, bool)> = rows
        .into_iter()
        .map(|(id, dedicated, options, no_options, local)| {
            (
                id,
                (
                    RestaurantReportCounts {
                        dedicated,
                        options,
                        no_options,
                    },
                    local,
                ),
            )
        })
        .collect();

    Ok(items
        .into_iter()
        .map(|item| {
            let Some((report, local)) = by_id.get(&item.id).copied() else { return item };
            let positive = report.dedicated + report.options;
            let from_users = support_from_users(report);
            let conflict = (report.no_options > 0 && positive > 0)
                || (item.gluten_support == GlutenSupport::NoGlutenFreeOptions && positive > 0)
                || (item.gluten_support != GlutenSupport::Unknown
                    && item.gluten_support != GlutenSupport::NoGlutenFreeOptions
                    && report.no_options > 0);
            let mut evidence = if item.gluten_support == GlutenSupport::Unknown { vec![] } else { item.evidence.clone() };
            evidence.push(Evidence {
                source: "Usuarios de SafeEat".into(),
                kind: EvidenceKind::Uncertainty,
                text: if conflict {
                    "Hay indicaciones contradictorias sobre las opciones sin gluten.".into()
                } else {
                    format!(
                        "{} usuario(s) han compartido su experiencia. Confirma el menú y la preparación.",
                        report.dedicated + report.options + report.no_options
                    )
                },
                url: None,
            });
            Restaurant {
                gluten_support: if conflict {
                    GlutenSupport::Unknown
                } else if from_users == GlutenSupport::Unknown {
                    item.gluten_support
                } else {
                    from_users
                },
                evidence,
                gluten_reports: Some(report),
                has_local_review: Some(local),
                ..item
            }
        })
        .collect())
}
