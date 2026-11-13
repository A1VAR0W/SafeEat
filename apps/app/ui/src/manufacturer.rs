//! «Buscar en la web del fabricante»: a link that opens an ordinary web search
//! limited to the brand's official site, in her browser. Free, no API and no key:
//! SafeEat reads nothing from it and claims nothing about what she finds there.
//!
//! Only brands whose official Spanish site is well known are listed; a brand that
//! is not here simply gets no link (never a guessed domain). Review this table
//! when a brand changes its site.

/// (normalised brand, official domain). Normalised: lowercase, no accents, only
/// letters and digits.
const BRANDS: &[(&str, &str)] = &[
    ("hacendado", "mercadona.es"),
    ("mercadona", "mercadona.es"),
    ("deliplus", "mercadona.es"),
    ("carrefour", "carrefour.es"),
    ("dia", "dia.es"),
    ("eroski", "eroski.es"),
    ("alcampo", "alcampo.es"),
    ("auchan", "alcampo.es"),
    ("lidl", "lidl.es"),
    ("consum", "consum.es"),
    ("elcorteingles", "elcorteingles.es"),
    ("gullon", "gullon.es"),
    ("schar", "schaer.com"),
    ("drschar", "schaer.com"),
    ("schaer", "schaer.com"),
    ("proceli", "proceli.com"),
    ("santiveri", "santiveri.com"),
    ("nestle", "nestle.es"),
    ("bimbo", "bimbo.es"),
    ("danone", "danone.es"),
    ("pascual", "calidadpascual.com"),
    ("gallinablanca", "gallinablanca.es"),
    ("casatarradellas", "casatarradellas.es"),
    ("campofrio", "campofrio.es"),
    ("elpozo", "elpozo.com"),
    ("centrallecheraasturiana", "centrallecheraasturiana.es"),
    ("hero", "hero.es"),
    ("kelloggs", "kelloggs.es"),
    ("colacao", "colacao.es"),
    ("calvo", "calvo.es"),
    ("ybarra", "ybarra.es"),
    ("valor", "valor.es"),
    ("cocacola", "cocacola.es"),
    ("mahou", "mahou.es"),
    ("estrelladamm", "estrelladamm.com"),
];

fn normalise(brand: &str) -> String {
    brand
        .chars()
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'â' => 'a',
            'é' | 'è' | 'ë' | 'ê' => 'e',
            'í' | 'ì' | 'ï' | 'î' => 'i',
            'ó' | 'ò' | 'ö' | 'ô' => 'o',
            'ú' | 'ù' | 'ü' | 'û' => 'u',
            'ñ' => 'n',
            'ç' => 'c',
            other => other,
        })
        .filter(char::is_ascii_alphanumeric)
        .collect()
}

/// The official domain of the first known brand ("Hacendado, Mercadona" lists
/// several, separated by commas as in Open Food Facts).
pub fn official_domain(brands: &str) -> Option<&'static str> {
    brands.split([',', ';', '/']).find_map(|part| {
        let key = normalise(part);
        BRANDS.iter().find(|(b, _)| *b == key).map(|(_, d)| *d)
    })
}

fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 3);
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// A search limited to the brand's official site, or `None` for an unknown brand.
pub fn search_url(brands: &str, product_name: &str) -> Option<(&'static str, String)> {
    let domain = official_domain(brands)?;
    let name: String = product_name.chars().take(80).collect();
    let query = format!("site:{domain} {} ingredientes", name.trim());
    Some((
        domain,
        format!("https://duckduckgo.com/?q={}", percent_encode(&query)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_brands_map_to_their_official_site() {
        assert_eq!(official_domain("Hacendado"), Some("mercadona.es"));
        assert_eq!(official_domain("Gullón"), Some("gullon.es"));
        assert_eq!(official_domain("Dr. Schär"), Some("schaer.com"));
        assert_eq!(official_domain("Schär"), Some("schaer.com"));
        assert_eq!(official_domain("Kellogg's"), Some("kelloggs.es"));
        assert_eq!(official_domain("Coca-Cola"), Some("cocacola.es"));
        // Open Food Facts lists several brands; the first known one counts.
        assert_eq!(official_domain("Marca X, Carrefour"), Some("carrefour.es"));
    }

    #[test]
    fn an_unknown_brand_gets_no_link() {
        assert_eq!(official_domain("Kimbo"), None);
        assert_eq!(official_domain(""), None);
        // Never a partial match: "Diamante" is not Dia.
        assert_eq!(official_domain("Diamante"), None);
        assert!(search_url("Marca desconocida", "Galletas").is_none());
    }

    #[test]
    fn the_search_is_limited_to_the_official_site() {
        let (domain, url) = search_url("Hacendado", "Galletas María & cacao").unwrap();
        assert_eq!(domain, "mercadona.es");
        assert_eq!(
            url,
            "https://duckduckgo.com/?q=site%3Amercadona.es+Galletas+Mar%C3%ADa+%26+cacao+ingredientes"
        );
    }
}
