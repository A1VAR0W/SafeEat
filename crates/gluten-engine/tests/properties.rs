//! Property-based and fuzz tests with a seeded generator (deterministic, no extra
//! dependencies): thousands of generated lists per property, reproducible by seed.
//!
//! Properties:
//! - the engine never panics, whatever the input (random Unicode, brackets, huge text);
//! - the result is deterministic;
//! - a declared gluten cereal anywhere in a list is always found;
//! - a trace statement is never lost and never ends green;
//! - without a "sin gluten" declaration a product is never green;
//! - buckwheat ("trigo sarraceno") alone is never read as wheat.

use safeeat_contracts::{GlutenFinding, GlutenStatus};
use safeeat_gluten_engine::{analyze, Context, GlutenInput, Origin};

/// xorshift64*: small, seeded, good enough to vary inputs.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[self.below(items.len())]
    }
}

const SAFE: &[&str] = &[
    "patatas",
    "aceite de oliva",
    "sal",
    "azúcar",
    "arroz",
    "maíz",
    "leche entera",
    "huevo",
    "cacao en polvo",
    "aceite de girasol (alto oleico)",
    "tomate (85%)",
    "vinagre de vino",
    "pimentón",
    "ajo",
    "cebolla",
    "agua",
    "potatoes",
    "sunflower oil",
    "salt",
    "rice",
];

const GLUTEN: &[&str] = &[
    "harina de trigo",
    "HARINA DE TRIGO",
    "trigo",
    "cebada",
    "centeno",
    "espelta",
    "malta de cebada",
    "sémola de trigo duro",
    "wheat flour",
    "barley malt",
    "rye flour",
    "gluten de trigo",
];

const SEPARATORS: &[&str] = &[", ", ",", "; ", " , ", ",\n", " y ", ", "];

const NOISE: &[&str] = &[
    "(", ")", "[", "]", "{", "}", ".", ",", ";", ":", "%", "*", "_", "\u{a0}", "\u{200b}", "ñ",
    "É", "ß", "ø", "æ", "€", "🌾", "\u{301}", "0", "9", "1,5", "2.5%", "  ", "\n", "\t", "sin",
    "no", "contiene", "trazas", "gluten", "trigo", "puede", "de", "y", "e",
];

fn provider(text: &str) -> GlutenInput {
    GlutenInput {
        ingredients: Some(text.to_string()),
        source: "Prueba".into(),
        origin: Some(Origin::Provider),
        ..Default::default()
    }
}

fn status(text: &str) -> GlutenStatus {
    analyze(&provider(text), Context::default())
        .assessment
        .status
}

fn safe_list(rng: &mut Rng, n: usize) -> Vec<String> {
    (0..n).map(|_| rng.pick(SAFE).to_string()).collect()
}

fn join(rng: &mut Rng, items: &[String]) -> String {
    let mut out = String::new();
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(rng.pick(SEPARATORS));
        }
        out.push_str(item);
    }
    out
}

#[test]
fn never_panics_and_is_deterministic() {
    let mut rng = Rng(0x05AF_EEA7);
    for _ in 0..3000 {
        let len = rng.below(60);
        let mut text = String::new();
        for _ in 0..len {
            if rng.below(3) == 0 {
                text.push_str(rng.pick(NOISE));
            } else {
                text.push_str(rng.pick(SAFE));
            }
        }
        // Random code points too (valid chars only).
        for _ in 0..rng.below(8) {
            if let Some(c) = char::from_u32((rng.next() % 0x2_FFFF) as u32) {
                text.push(c);
            }
        }
        let a = analyze(&provider(&text), Context::default()).assessment;
        let b = analyze(&provider(&text), Context::default()).assessment;
        assert_eq!(a.status, b.status, "{text:?}");
        assert_eq!(a.finding, b.finding, "{text:?}");
        assert!(a.finding.is_some(), "{text:?}");
        assert!(!a.reasons.is_empty(), "{text:?}");
    }
}

#[test]
fn huge_and_deep_inputs_are_handled() {
    let deep = format!("{}sal{}", "(".repeat(5000), ")".repeat(5000));
    assert_ne!(status(&deep), GlutenStatus::GlutenFree);
    let open = "(".repeat(20000);
    assert_ne!(status(&open), GlutenStatus::GlutenFree);
    let long = format!("{} harina de trigo", "patatas, ".repeat(20000));
    assert_eq!(status(&long), GlutenStatus::ContainsGluten);
}

#[test]
fn a_gluten_cereal_anywhere_is_found() {
    let mut rng = Rng(42);
    for _ in 0..3000 {
        let n = 1 + rng.below(25);
        let mut items = safe_list(&mut rng, n);
        let at = rng.below(items.len() + 1);
        let cereal = rng.pick(GLUTEN).to_string();
        // Sometimes inside a compound ingredient.
        let cereal = if rng.below(4) == 0 {
            format!("galleta ({cereal}, azúcar)")
        } else {
            cereal
        };
        items.insert(at, cereal);
        let text = join(&mut rng, &items);
        assert_eq!(status(&text), GlutenStatus::ContainsGluten, "{text}");
    }
}

#[test]
fn traces_are_never_lost_nor_green() {
    let mut rng = Rng(7);
    let statements = [
        "Puede contener trazas de trigo.",
        "Puede contener gluten.",
        "Trazas de cebada.",
        "May contain wheat.",
        "Elaborado en una fábrica que utiliza trigo.",
    ];
    for _ in 0..2000 {
        let n = 1 + rng.below(15);
        let items = safe_list(&mut rng, n);
        let text = format!("{}. {}", join(&mut rng, &items), rng.pick(&statements));
        let s = status(&text);
        assert!(
            s == GlutenStatus::MayContainTraces || s == GlutenStatus::ContainsGluten,
            "{text} → {s:?}"
        );
    }
}

#[test]
fn never_green_without_a_declaration() {
    let mut rng = Rng(99);
    for _ in 0..3000 {
        let n = 1 + rng.below(20);
        let items = safe_list(&mut rng, n);
        let text = join(&mut rng, &items);
        assert_ne!(status(&text), GlutenStatus::GlutenFree, "{text}");
    }
}

#[test]
fn negated_and_scoped_claims_are_not_declarations() {
    for text in [
        "Arroz, sal. No es sin gluten.",
        "Arroz, sal. Not gluten free.",
        "Arroz, salsa sin gluten, harina de trigo.",
        "Pan sin gluten (harina de trigo).",
    ] {
        assert_ne!(status(text), GlutenStatus::GlutenFree, "{text}");
    }
}

#[test]
fn buckwheat_is_not_wheat() {
    for text in [
        "Trigo sarraceno.",
        "harina de trigo sarraceno, sal",
        "Buckwheat flour, salt",
    ] {
        let a = analyze(&provider(text), Context::default()).assessment;
        assert_ne!(a.status, GlutenStatus::ContainsGluten, "{text}");
        assert_ne!(a.status, GlutenStatus::GlutenFree, "{text}");
    }
}

#[test]
fn kimbo_regression() {
    let a = analyze(
        &provider(
            "Patatas seleccionadas (64%), aceite de oliva (35%), sal, dextrosa y aromas naturales",
        ),
        Context::default(),
    )
    .assessment;
    assert_eq!(a.status, GlutenStatus::Unknown);
    assert_eq!(a.finding, Some(GlutenFinding::NoGlutenIdentified));
    assert!(a.reasons.iter().any(|r| r
        .message
        .starts_with("No se han identificado ingredientes que declaren gluten")));
}
