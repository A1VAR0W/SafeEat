//! Public viewer (replaces the Next.js app in apps/web): shared products and restaurants,
//! privacy, sources and the `.well-known` files for app links. Runs with `--mode web`.

use std::time::Duration;

use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use once_cell::sync::Lazy;
use regex::Regex;
use safeeat_contracts::{
    GlutenStatus, GlutenSupport, Product, PublicShare, Restaurant, PRODUCT_DISCLAIMER,
    RESTAURANT_DISCLAIMER,
};
use serde_json::{json, Value};

use crate::util::html_escape as esc;

static TOKEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-f0-9]{48}$").unwrap());
static FINGERPRINT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^([A-Fa-f0-9]{2}:){31}[A-Fa-f0-9]{2}$").unwrap());

const CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' https: data:; connect-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

#[derive(Clone)]
pub struct WebConfig {
    pub port: u16,
    pub api_url: String,
    pub public_url: String,
    pub base_path: String,
    pub apple_team_id: Option<String>,
    pub fingerprints: Vec<String>,
    http: reqwest::Client,
}

impl WebConfig {
    pub fn from_env() -> Result<Self> {
        let var = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let base_path = var("WEB_BASE_PATH")
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_string();
        if !base_path.is_empty() && !Regex::new(r"^/[a-z0-9-]+$")?.is_match(&base_path) {
            anyhow::bail!("WEB_BASE_PATH must look like /preproduction");
        }
        Ok(Self {
            port: var("PORT").map(|p| p.parse()).transpose()?.unwrap_or(3000),
            api_url: var("API_INTERNAL_URL")
                .or_else(|| var("API_PUBLIC_URL"))
                .unwrap_or_else(|| "http://localhost:4000".into())
                .trim_end_matches('/')
                .into(),
            public_url: var("PUBLIC_APP_URL")
                .unwrap_or_else(|| "http://localhost:3000".into())
                .trim_end_matches('/')
                .into(),
            base_path,
            apple_team_id: var("APPLE_TEAM_ID")
                .filter(|t| t.chars().all(|c| c.is_ascii_alphanumeric())),
            fingerprints: var("ANDROID_SHA256_FINGERPRINTS")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|v| FINGERPRINT.is_match(v))
                .map(str::to_string)
                .collect(),
            http: reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .build()?,
        })
    }
}

struct Page {
    title: String,
    description: Option<String>,
    head: String,
    body: String,
}

fn layout(config: &WebConfig, page: Page) -> String {
    let base = &config.base_path;
    format!(
        r#"<!doctype html><html lang="es"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">
<title>{title}</title>{description}{head}<link rel="stylesheet" href="{base}/assets/style.css"><link rel="icon" href="{base}/brand/safeeat-mark.png"></head>
<body><header><a class="brand" href="{base}/"><img src="{base}/brand/safeeat-mark.png" alt="" width="34" height="34"><img src="{base}/brand/safeeat-wordmark.png" alt="SafeEat" width="118" height="26"></a><span>Conoce lo que comes.</span></header>
<main>{body}</main>
<footer>SafeEat · Comprueba siempre la etiqueta actual.<br><a href="{base}/privacy">Privacidad</a> · <a href="{base}/sources">Fuentes y metodología</a></footer></body></html>"#,
        title = esc(&page.title),
        description = page
            .description
            .map(|d| format!(r#"<meta name="description" content="{}">"#, esc(&d)))
            .unwrap_or_default(),
        head = page.head,
        body = page.body,
    )
}

fn html(config: &WebConfig, status: StatusCode, page: Page) -> Response {
    (status, Html(layout(config, page))).into_response()
}

fn not_found(config: &WebConfig) -> Response {
    html(
        config,
        StatusCode::NOT_FOUND,
        Page {
            title: "No encontramos este enlace · SafeEat".into(),
            description: None,
            head: String::new(),
            body: format!(
                r#"<h1>No encontramos este enlace.</h1><p>Comprueba que esté completo. Es posible que el propietario haya eliminado la etiqueta o su cuenta.</p><a href="{}/" class="button">Volver a SafeEat</a>"#,
                config.base_path
            ),
        },
    )
}

fn unavailable(config: &WebConfig) -> Response {
    html(
        config,
        StatusCode::SERVICE_UNAVAILABLE,
        Page {
            title: "SafeEat".into(),
            description: None,
            head: String::new(),
            body: "<h1>No se puede consultar la fuente en este momento.</h1><p>Inténtalo de nuevo en unos minutos.</p>".into(),
        },
    )
}

enum Fetched {
    Share(PublicShare),
    NotFound,
    Failed,
}

async fn fetch_share(config: &WebConfig, token: &str) -> Fetched {
    if !TOKEN.is_match(token) {
        return Fetched::NotFound;
    }
    let response = config
        .http
        .get(format!("{}/api/v1/public/shares/{token}", config.api_url))
        .timeout(Duration::from_secs(10))
        .send()
        .await;
    match response {
        Ok(r)
            if r.status() == reqwest::StatusCode::NOT_FOUND
                || r.status() == reqwest::StatusCode::BAD_REQUEST =>
        {
            Fetched::NotFound
        }
        Ok(r) if r.status().is_success() => r
            .json::<PublicShare>()
            .await
            .map(Fetched::Share)
            .unwrap_or(Fetched::Failed),
        _ => Fetched::Failed,
    }
}

fn og(
    config: &WebConfig,
    kind: &str,
    token: &str,
    title: &str,
    description: &str,
    image: Option<&str>,
) -> String {
    let url = format!("{}/{kind}/{token}", config.public_url);
    format!(
        r#"<meta name="robots" content="noindex,nofollow"><link rel="canonical" href="{url}"><meta property="og:title" content="{title}"><meta property="og:description" content="{description}"><meta property="og:url" content="{url}"><meta property="og:type" content="website">{image}<meta name="twitter:card" content="summary_large_image">"#,
        url = esc(&url),
        title = esc(title),
        description = esc(description),
        image = image
            .map(|i| format!(r#"<meta property="og:image" content="{}">"#, esc(i)))
            .unwrap_or_default(),
    )
}

fn plural(n: i64, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

fn displayed_status(p: &Product) -> (String, String) {
    let free = p.gluten_reports.map(|r| r.gluten_free).unwrap_or(0);
    let contains = p.gluten_reports.map(|r| r.contains_gluten).unwrap_or(0);
    let status = p.assessment.status;
    if (free > 0 && contains > 0)
        || (contains > 0 && status == GlutenStatus::GlutenFree)
        || (free > 0 && status == GlutenStatus::ContainsGluten)
    {
        return ("⚠".into(), "INFORMACIÓN CONTRADICTORIA".into());
    }
    if status == GlutenStatus::Unknown && contains > 0 {
        return (
            "⚠".into(),
            format!(
                "{} gluten",
                plural(contains, "usuario indica", "usuarios indican")
            ),
        );
    }
    if status == GlutenStatus::Unknown && free > 0 {
        return (
            "⚠".into(),
            format!(
                "{} sin gluten (sin confirmar)",
                plural(free, "usuario indica", "usuarios indican")
            ),
        );
    }
    let (icon, label) = status.label();
    (icon.into(), label.into())
}

fn spanish_date(iso: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(iso)
        .map(|d| d.format("%-d/%-m/%Y").to_string())
        .unwrap_or_default()
}

fn product_page(config: &WebConfig, token: &str, p: &Product) -> Response {
    let (icon, label) = displayed_status(p);
    let title = format!("{} · {label}", p.name);
    let reasons: Vec<&str> = p
        .assessment
        .reasons
        .iter()
        .map(|r| r.message.as_str())
        .collect();
    let old = chrono::DateTime::parse_from_rfc3339(&p.fetched_at)
        .map(|d| chrono::Utc::now().signed_duration_since(d) > chrono::Duration::hours(24))
        .unwrap_or(true);
    let mut body = String::new();
    if let Some(image) = &p.image_url {
        body += &format!(
            r#"<img class="product-image" src="{}" alt="{}" width="220" height="220">"#,
            esc(image),
            esc(&p.name)
        );
    }
    body += &format!(
        r#"<p class="eyebrow">Producto compartido</p><h1>{}</h1><p class="muted">{}</p>"#,
        esc(&p.name),
        esc(p.brand.as_deref().unwrap_or(""))
    );
    if p.information_source == safeeat_contracts::InformationSource::Community {
        body += r#"<p class="muted">👤 Ingredientes aportados por un usuario</p>"#;
    }
    if p.stale || old {
        body += r#"<p class="notice">Información guardada que puede estar desactualizada. Comprueba la etiqueta actual.</p>"#;
    }
    body += &format!(
        r#"<section class="card assessment {}"><h2 class="status">{} {}</h2><p class="muted">Comprueba siempre la etiqueta antes de consumirlo.</p>"#,
        p.assessment.status.as_str(),
        esc(&icon),
        esc(&label)
    );
    if let Some(cert) = p.assessment.certification {
        body += &format!(r#"<p><strong>{}</strong></p>"#, esc(cert.label()));
    }
    for reason in &reasons {
        body += &format!("<p>{}</p>", esc(reason));
    }
    body += "<h2>La evidencia</h2>";
    if p.assessment.evidence.is_empty() {
        body += "<p>No hay evidencia suficiente.</p>";
    }
    for e in &p.assessment.evidence {
        body += &format!(r#"<div class="evidence"><p>“{}”</p></div>"#, esc(&e.text));
    }
    body += &format!(
        r#"<p class="muted">{}</p></section>"#,
        esc(PRODUCT_DISCLAIMER)
    );
    body += &format!(
        r#"<section class="card"><h2>Calidad nutricional</h2><div class="score">{}</div><p class="muted">{}</p>"#,
        p.quality
            .score
            .map(|s| format!("{s}/100"))
            .unwrap_or_else(|| "Sin datos suficientes".into()),
        esc(&p.quality.explanation)
    );
    if let Some(n) = &p.nutri_score {
        body += &format!(
            "<p>Nutri-Score del proveedor: <strong>{}</strong></p>",
            esc(n)
        );
    }
    if !p.quality.breakdown.is_empty() {
        body += "<ul>";
        for b in &p.quality.breakdown {
            body += &format!(
                "<li>{}: {}{}</li>",
                esc(&b.label),
                if b.points > 0 { "+" } else { "" },
                b.points
            );
        }
        body += "</ul>";
    }
    body += "</section>";
    if let Some(ingredients) = &p.ingredients {
        let clean = ingredients
            .replace("\\_", " ")
            .replace('_', " ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        body += &format!(
            r#"<section class="card"><h2>Ingredientes</h2><p>{}</p>"#,
            esc(&clean)
        );
        if let Some(photo) = &p.ingredients_image_url {
            body += &format!(
                r#"<img src="{}" alt="Etiqueta de ingredientes aportada por un usuario" style="max-width:100%;border-radius:12px">"#,
                esc(photo)
            );
        }
        body += "</section>";
    }
    body += &format!(
        r#"<a class="button" href="safeeat://p/{token}">Abrir en SafeEat</a><p class="muted">Consultado el {}</p>"#,
        spanish_date(&p.fetched_at)
    );
    html(
        config,
        StatusCode::OK,
        Page {
            title: title.clone(),
            description: Some(reasons.join(" ")),
            head: og(
                config,
                "p",
                token,
                &title,
                reasons.first().copied().unwrap_or(""),
                p.image_url.as_deref(),
            ),
            body,
        },
    )
}

fn restaurant_page(config: &WebConfig, token: &str, r: &Restaurant) -> Response {
    let heading = match r.gluten_support {
        GlutenSupport::Unknown => "⚠ No verificado",
        GlutenSupport::DedicatedGlutenFree => "Dedicado a comida sin gluten",
        GlutenSupport::NoGlutenFreeOptions => "Sin opciones sin gluten conocidas",
        GlutenSupport::GlutenFreeOptions => "Opciones sin gluten",
    };
    let mut body = format!(
        r#"<p class="eyebrow">Restaurante compartido</p><h1>{}</h1><p class="lede">{}</p>"#,
        esc(&r.name),
        esc(&r.address)
    );
    if let Some(chain) = &r.chain {
        body += &format!(r#"<p class="muted">Cadena: {}</p>"#, esc(&chain.name));
    }
    body += &format!(r#"<section class="card assessment UNKNOWN"><h2>{heading}</h2>"#);
    for e in &r.evidence {
        body += &format!("<p>{}</p>", esc(&e.text));
    }
    body += &format!("<p>{}</p></section>", esc(RESTAURANT_DISCLAIMER));
    body += &format!(
        r#"<a class="button" href="{}" rel="noreferrer">Cómo llegar</a><a class="button" href="safeeat://r/{token}">Abrir en SafeEat</a>"#,
        esc(&r.maps_url)
    );
    if let Some(site) = &r.website {
        body += &format!(
            r#"<p><a href="{}" rel="noreferrer">Web del restaurante</a></p>"#,
            esc(site)
        );
    }
    html(
        config,
        StatusCode::OK,
        Page {
            title: format!("{} · SafeEat", r.name),
            description: Some(format!("{}. {}", r.address, RESTAURANT_DISCLAIMER)),
            head: og(config, "r", token, &r.name, RESTAURANT_DISCLAIMER, None),
            body,
        },
    )
}

async fn product(State(config): State<WebConfig>, Path(token): Path<String>) -> Response {
    match fetch_share(&config, &token).await {
        Fetched::Share(PublicShare::Product(p)) => product_page(&config, &token, &p),
        Fetched::Failed => unavailable(&config),
        _ => not_found(&config),
    }
}

async fn restaurant(State(config): State<WebConfig>, Path(token): Path<String>) -> Response {
    match fetch_share(&config, &token).await {
        Fetched::Share(PublicShare::Restaurant(r)) => restaurant_page(&config, &token, &r),
        Fetched::Failed => unavailable(&config),
        _ => not_found(&config),
    }
}

async fn home(State(config): State<WebConfig>) -> Response {
    html(
        &config,
        StatusCode::OK,
        Page {
            title: "SafeEat · La información que necesitas".into(),
            description: Some("Consulta ingredientes, evidencias sobre gluten y calidad nutricional de forma independiente.".into()),
            head: String::new(),
            body: r#"<p class="eyebrow">Tu compañero en cada etiqueta</p><h1>Más información.<br>Menos dudas.</h1>
<p class="lede">Escanea un producto, revisa sus ingredientes y descubre qué evidencia hay sobre el gluten. Porque una respuesta útil también explica el porqué.</p>
<a class="button" href="safeeat://home">Abrir SafeEat</a>
<div class="grid"><section class="card"><h2>Primero, la etiqueta.</h2><p>Códigos de barras y lectura de ingredientes, con resultados que distinguen gluten, trazas e información insuficiente.</p></section>
<section class="card"><h2>Dos cosas distintas.</h2><p>La información sobre gluten y la calidad nutricional se muestran por separado. Sin gluten no significa más saludable.</p></section>
<section class="card"><h2>Comparte con contexto.</h2><p>Abre un enlace de SafeEat para consultar el producto o restaurante, sus fuentes y las limitaciones de la información.</p></section></div>"#.into(),
        },
    )
}

async fn privacy(State(config): State<WebConfig>) -> Response {
    html(&config, StatusCode::OK, Page {
        title: "Privacidad · SafeEat".into(),
        description: None,
        head: String::new(),
        body: r#"<h1>Privacidad</h1>
<p>SafeEat almacena el correo electrónico, una contraseña cifrada mediante hash y las sesiones de acceso necesarias para tu cuenta. Puedes eliminarla desde Tu cuenta → Opciones avanzadas; se eliminarán sus favoritos, marcadores, veredictos personales, etiquetas, fotos, revisiones y restaurantes aportados, además de los enlaces personales.</p>
<p>La lectura de una etiqueta personal se realiza en el dispositivo y su foto se elimina después. Si decides aportar la foto de ingredientes de un producto con código de barras, enviamos al servidor la imagen y el texto que has revisado. Otros usuarios podrán consultarlos hasta que elimines tu cuenta. La imagen se reduce y se vuelve a codificar sin metadatos antes de guardarse. Si compartes una etiqueta personal, cualquier persona con el enlace podrá leerla.</p>
<p><strong>Foto de ingredientes.</strong> El texto de la foto se lee en tu propio móvil (reconocimiento de texto sin conexión); la foto no se envía a ningún servicio externo. Solo si guardas el análisis se envía al servidor de SafeEat el texto que has revisado. SafeEat no utiliza servicios de inteligencia artificial de pago ni de terceros: la revisión orientativa de ingredientes desconocidos la hace un modelo que se ejecuta en el propio servidor de SafeEat y nunca decide si un producto es apto.</p>
<p>Tus veredictos personales («Seguro para mí» / «No es seguro para mí») solo los ves tú.</p>
<p>Si revisas un producto o restaurante, guardamos tu valoración y el texto que aportas junto a tu cuenta. Publicamos el resultado agregado sin mostrar tu identidad. Los restaurantes que añadas, su nombre, dirección y posición en el mapa serán visibles para otros usuarios.</p>
<p>No incorporamos publicidad ni herramientas de analítica. Las consultas de productos se envían a Open Food Facts y, cuando esté configurado, a USDA FoodData Central. Las consultas de restaurantes se envían a OpenStreetMap/Photon. El mapa se descarga de OpenFreeMap. Tu ubicación se envía al buscar cerca de ti o en una zona del mapa. Al añadir un restaurante, se guarda la posición que selecciones para situarlo en el mapa.</p>
<p>La caché local puede borrarse desde Ajustes. Los datos eliminados pueden permanecer hasta 14 días en copias de seguridad restringidas. Las búsquedas no se registran en los logs de la aplicación.</p>"#.into(),
    })
}

async fn sources(State(config): State<WebConfig>) -> Response {
    html(&config, StatusCode::OK, Page {
        title: "Fuentes y metodología · SafeEat".into(),
        description: None,
        head: String::new(),
        body: r#"<h1>Fuentes y metodología</h1>
<h2>Gluten</h2><p>SafeEat busca declaraciones explícitas, cereales, alérgenos y advertencias de trazas. Las evidencias de riesgo prevalecen sobre una declaración contradictoria de ausencia de gluten. La falta de ingredientes peligrosos no demuestra que un alimento sea sin gluten.</p>
<p>El sello de la Espiga Barrada y la mención «sin gluten» del envase se muestran aparte, tal como los registra Open Food Facts. La lectura con IA nunca declara un producto sin gluten por sí sola: se combina con el análisis automático y prevalece el resultado más prudente.</p>
<h2>Datos abiertos</h2><p>Los productos proceden de <a href="https://world.openfoodfacts.org">Open Food Facts</a>, una base comunitaria bajo ODbL. Sus contenidos están bajo Database Contents License y sus imágenes bajo CC BY-SA. La información puede ser incompleta o incorrecta.</p>
<p>Si está configurado, también se consulta <a href="https://fdc.nal.usda.gov/">USDA FoodData Central</a> por código de barras exacto. Las fotos de ingredientes aportadas a SafeEat se distinguen de las fichas de esas fuentes. Una aportación de usuario no certifica que el producto sea apto para celíacos.</p>
<h2>Calidad nutricional</h2><p>El indicador propio SafeEat utiliza valores por 100 g/ml: parte de 70 puntos, descuenta energía, azúcar, grasas saturadas y sal, y añade fibra y proteína. Se limita a 0–100 y se redondea a múltiplos de 5. Sin seis valores válidos no se calcula. No es un modelo clínicamente validado, no es Nutri-Score y no mide seguridad frente al gluten.</p>
<h2>Restaurantes</h2><p>OpenStreetMap aporta datos del lugar, pero no verifica la seguridad frente al gluten. Las cadenas se reconocen por su marca en OpenStreetMap. Solo las cuentas de confianza pueden marcar una cadena entera, y la revisión de un local concreto siempre prevalece sobre la de su cadena. Las indicaciones de las fichas y las revisiones de usuarios se muestran como aportaciones, no como certificaciones. Si hay revisiones contradictorias, SafeEat no presenta una conclusión favorable. Confirma siempre el menú, los ingredientes y la contaminación cruzada con el restaurante.</p>"#.into(),
    })
}

fn cached_json(value: Value) -> Response {
    let mut response = Json(value).into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public,max-age=3600"),
    );
    response
}

async fn apple(State(config): State<WebConfig>) -> Response {
    let details = match &config.apple_team_id {
        Some(team) => {
            json!([{"appID": format!("{team}.com.a1var0w.safeeat"), "paths": ["/p/*", "/r/*"]}])
        }
        None => json!([]),
    };
    cached_json(json!({"applinks": {"apps": [], "details": details}}))
}

async fn assetlinks(State(config): State<WebConfig>) -> Response {
    if config.fingerprints.is_empty() {
        return cached_json(json!([]));
    }
    cached_json(json!([{
        "relation": ["delegate_permission/common.handle_all_urls"],
        "target": {"namespace": "android_app", "package_name": "com.a1var0w.safeeat", "sha256_cert_fingerprints": config.fingerprints}
    }]))
}

fn asset(bytes: &'static [u8], mime: &'static str) -> Response {
    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "public,max-age=86400"),
        ],
        bytes,
    )
        .into_response()
}

async fn headers(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    let mut response = next.run(request).await;
    let h = response.headers_mut();
    h.insert(
        "x-content-type-options",
        HeaderValue::from_static("nosniff"),
    );
    h.insert("x-frame-options", HeaderValue::from_static("DENY"));
    h.insert(
        "referrer-policy",
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    h.insert("content-security-policy", HeaderValue::from_static(CSP));
    response
}

pub fn router(config: WebConfig) -> Result<Router> {
    let base = config.base_path.clone();
    let site = Router::new()
        .route("/", get(home))
        .route("/p/{token}", get(product))
        .route("/r/{token}", get(restaurant))
        .route("/privacy", get(privacy))
        .route("/sources", get(sources))
        .route("/health", get(|| async { Json(json!({"status": "ok"})) }))
        .route("/.well-known/apple-app-site-association", get(apple))
        .route("/.well-known/assetlinks.json", get(assetlinks))
        .route(
            "/assets/style.css",
            get(|| async {
                asset(
                    include_bytes!("../assets/style.css"),
                    "text/css; charset=utf-8",
                )
            }),
        )
        .route(
            "/brand/safeeat-mark.png",
            get(|| async { asset(include_bytes!("../assets/safeeat-mark.png"), "image/png") }),
        )
        .route(
            "/brand/safeeat-wordmark.png",
            get(|| async {
                asset(
                    include_bytes!("../assets/safeeat-wordmark.png"),
                    "image/png",
                )
            }),
        )
        .fallback(|State(config): State<WebConfig>| async move { not_found(&config) })
        .with_state(config.clone());
    // nginx redirects `/preproduction` to `/preproduction/`, and the brand link points there too.
    let app = if base.is_empty() {
        site
    } else {
        Router::new()
            .route(&format!("{base}/"), get(home))
            .with_state(config)
            .nest(&base, site)
    };
    Ok(app.layer(axum::middleware::from_fn(headers)))
}
