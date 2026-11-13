//! Versioned ingredient rule catalog, held in memory.
//!
//! Each rule classifies an ingredient for a coeliac reader and says why, with the
//! document it rests on. The catalog never decides the public status on its own: the
//! core engine (`assess_gluten`) does, and the catalog only
//! - explains each ingredient,
//! - recognises the legal exemptions of Regulation (EU) 1169/2011 Annex II, and
//! - decides whether "no gluten ingredients identified" can be said at all.
//!
//! Every gluten word here that is a *declaration* is also known by the core engine, so
//! the catalog never turns a product red by itself. Words the core engine does not know
//! ("pan rallado", "galleta") are `LikelyGluten`: they block every reassuring message.
//!
//! Matching runs on folded text (lowercase ASCII, see `items::Folded`). Rules are tried
//! in order and each match is masked, so a specific phrase ("trigo sarraceno",
//! "harina de arroz", "jarabe de glucosa de trigo") is consumed before a general word
//! ("trigo", "harina") can match it. An item's class is the worst class it matched.
//!
//! Change a rule ⇒ bump `RULES_VERSION` (cached analyses and AI results are keyed by it).

use once_cell::sync::Lazy;
use regex::{Regex, RegexSet};
pub use safeeat_contracts::IngredientClass;

/// Catalog version. Part of every cached analysis and AI result identity.
pub const RULES_VERSION: &str = "2026.11.13.2";
/// Date the whole catalog was last reviewed against its sources.
pub const REVIEWED: &str = "2026-11-13";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
    Fr,
    It,
    Pt,
    De,
    Ca,
    Nl,
}

/// Documents the rules rest on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Regulation (EU) 1169/2011: art. 9.1.c, art. 21 (emphasis of allergens in the
    /// ingredient list) and Annex II.1 (cereals containing gluten and its exemptions).
    Eu1169,
    /// Implementing Regulation (EU) 828/2014: "sin gluten" (≤ 20 mg/kg), "muy bajo en
    /// gluten" (≤ 100 mg/kg) and the conditions for oats.
    Eu828,
    /// AESAN (Agencia Española de Seguridad Alimentaria y Nutrición): allergen and gluten
    /// labelling guidance.
    Aesan,
    /// FACE (Federación de Asociaciones de Celíacos de España): ingredient guidance.
    Face,
    /// Coeliac UK: ingredients guidance.
    CoeliacUk,
}

impl Source {
    pub fn citation(self) -> &'static str {
        match self {
            Source::Eu1169 => "Reglamento (UE) 1169/2011, art. 21 y anexo II.1",
            Source::Eu828 => "Reglamento de Ejecución (UE) 828/2014",
            Source::Aesan => "AESAN, información sobre alérgenos y gluten",
            Source::Face => "FACE, guía de ingredientes para celíacos",
            Source::CoeliacUk => "Coeliac UK, guía de ingredientes",
        }
    }
}

#[derive(Debug)]
pub struct Rule {
    /// Canonical id ("dextrose", "wheat").
    pub id: &'static str,
    pub class: IngredientClass,
    /// Synonyms and translations, as regex fragments on folded text.
    pub terms: &'static [(Lang, &'static str)],
    /// Possible origins of the ingredient.
    pub origins: &'static str,
    /// Legal exception that applies, if any.
    pub exception: Option<&'static str>,
    /// Context that changes the class (and the class it changes to).
    pub context: Option<Context>,
    pub sources: &'static [Source],
    /// Catalog version in which the rule was last changed.
    pub since: &'static str,
    /// Explanation for the user, shown when the ingredient matters to the verdict.
    pub note: Option<&'static str>,
    /// Same note when another note precedes it ("…tampoco…").
    pub note_also: Option<&'static str>,
}

#[derive(Debug, Clone, Copy)]
pub struct Context {
    pub description: &'static str,
    /// Class when the product is not labelled under EU law (e.g. USDA data).
    pub outside_eu: IngredientClass,
}

use IngredientClass::*;
use Lang::*;
use Source::*;

const V: &str = RULES_VERSION;
const LABEL: &[Source] = &[Eu1169, Aesan, Face];
const OATS_SOURCES: &[Source] = &[Eu1169, Eu828, Face, CoeliacUk];
const EXEMPT: &[Source] = &[Eu1169, Aesan, Face, CoeliacUk];
const CONTAMINATION: &str = "Es naturalmente sin gluten, pero los granos y sus harinas se contaminan con facilidad en el campo y el molino: busca la mención «sin gluten».";

macro_rules! rule {
    ($id:literal, $class:expr, [$(($l:expr, $t:literal)),+ $(,)?] $(, $field:ident = $value:expr)* $(,)?) => {
        Rule {
            id: $id,
            class: $class,
            terms: &[$(($l, $t)),+],
            $($field: $value,)*
            ..BASE
        }
    };
}

const BASE: Rule = Rule {
    id: "",
    class: Unrecognized,
    terms: &[],
    origins: "",
    exception: None,
    context: None,
    sources: LABEL,
    since: V,
    note: None,
    note_also: None,
};

/// Ordered: specific phrases first. See module docs.
pub static RULES: &[Rule] = &[
    // ---- 1. Legal exemptions (Annex II.1 a-d) -----------------------------------
    rule!("wheat-glucose-syrup", LegalExemption, [
        (Es, r"(?:jarabe|sirope)s?\s+de\s+glucosa(?:\s+y\s+fructosa)?\s+(?:de\s+)?(?:trigo|cebada)"),
        (En, r"(?:wheat|barley)\s+glucose\s+syrup|glucose\s+syrup\s+(?:from\s+|de\s+)?(?:wheat|barley)"),
        (Fr, r"sirop\s+de\s+glucose\s+(?:de\s+)?(?:ble|froment|orge)"),
        (It, r"sciroppo\s+di\s+glucosio\s+(?:di\s+)?(?:frumento|grano|orzo)"),
        (Pt, r"xarope\s+de\s+glucose\s+(?:de\s+)?(?:trigo|cevada)"),
        (De, r"(?:weizen|gersten)gl[uy][ck]osesirup|gl[uy][ck]osesirup\s+(?:aus\s+|de\s+)?(?:weizen|gerste)"),
    ],
        origins = "trigo o cebada, por hidrólisis del almidón",
        exception = Some("Anexo II.1.a y II.1.c: jarabes de glucosa a base de trigo (incluida la dextrosa) y de cebada"),
        sources = EXEMPT,
        note = Some("El jarabe de glucosa de trigo o de cebada está exento de declararse como gluten por el Reglamento (UE) 1169/2011: el proceso elimina el gluten."),
    ),
    rule!("wheat-dextrose", LegalExemption, [
        (Es, r"dextrosa\s+(?:de\s+)?trigo"),
        (En, r"wheat\s+dextrose|dextrose\s+(?:from\s+|de\s+)?wheat"),
        (Fr, r"dextrose\s+(?:de\s+)?(?:ble|froment)"),
        (It, r"destrosio\s+(?:di\s+)?(?:frumento|grano)"),
        (Pt, r"dextrose\s+(?:de\s+)?trigo"),
        (De, r"weizendextrose|dextrose\s+(?:aus\s+|de\s+)?weizen"),
    ],
        origins = "trigo",
        exception = Some("Anexo II.1.a: jarabes de glucosa a base de trigo, incluida la dextrosa"),
        sources = EXEMPT,
        note = Some("La dextrosa de trigo está exenta de declararse como gluten por el Reglamento (UE) 1169/2011."),
    ),
    rule!("wheat-maltodextrin", LegalExemption, [
        (Es, r"maltodextrinas?\s+(?:de\s+)?trigo"),
        (En, r"wheat\s+maltodextrin|maltodextrin\s+(?:from\s+|de\s+)?wheat"),
        (Fr, r"maltodextrines?\s+(?:de\s+)?(?:ble|froment)"),
        (It, r"maltodestrin[ae]\s+(?:di\s+)?(?:frumento|grano)"),
        (Pt, r"maltodextrinas?\s+(?:de\s+)?trigo"),
        (De, r"weizenmaltodextrin|maltodextrin\s+(?:aus\s+|de\s+)?weizen"),
    ],
        origins = "trigo",
        exception = Some("Anexo II.1.b: maltodextrinas a base de trigo"),
        sources = EXEMPT,
        note = Some("La maltodextrina de trigo está exenta de declararse como gluten por el Reglamento (UE) 1169/2011."),
    ),
    rule!("cereal-distillate", LegalExemption, [
        (Es, r"alcohol\s+etilico(?:\s+de\s+origen\s+agricola)?(?:\s+de\s+(?:trigo|cebada|centeno))?|alcohol\s+de\s+(?:trigo|cebada|centeno)|destilado\s+de\s+(?:trigo|cebada|centeno)"),
        (En, r"ethyl\s+alcohol(?:\s+of\s+agricultural\s+origin)?|(?:wheat|grain)\s+(?:alcohol|spirit)"),
        (Fr, r"alcool\s+ethylique(?:\s+d\s+origine\s+agricole)?"),
        (It, r"alcool\s+etilico(?:\s+di\s+origine\s+agricola)?"),
        (De, r"ethylalkohol(?:\s+landwirtschaftlichen\s+ursprungs)?"),
    ],
        origins = "cereales con o sin gluten",
        exception = Some("Anexo II.1.d: cereales utilizados para hacer destilados alcohólicos, incluido el alcohol etílico de origen agrícola"),
        sources = EXEMPT,
        note = Some("El alcohol destilado de cereales está exento de declararse como gluten por el Reglamento (UE) 1169/2011."),
    ),
    // ---- 2. Phrases that look like gluten but are not ----------------------------
    rule!("buckwheat", NaturallyGlutenFree, [
        (Es, r"(?:harinas?\s+de\s+)?(?:trigo\s+sarraceno|alforfon)"),
        (En, r"buckwheat(?:\s+flour)?"),
        (Fr, r"(?:farine\s+de\s+)?(?:ble\s+noir|sarrasin)"),
        (It, r"(?:farina\s+di\s+)?grano\s+saraceno"),
        (Pt, r"(?:farinha\s+de\s+)?trigo\s+mourisco"),
        (De, r"buchweizen(?:mehl)?"),
        (Ca, r"blat\s+negre|fajol"),
    ],
        origins = "Fagopyrum esculentum, un pseudocereal sin relación con el trigo",
        sources = &[Face, CoeliacUk],
        note = Some(CONTAMINATION),
    ),
    rule!("gluten-free-flour", NaturallyGlutenFree, [
        (Es, r"(?:harinas?|almidon(?:es)?(?:\s+modificados?)?|feculas?|semolas?|copos|salvado|proteinas?)\s+(?:de\s+)?(?:arroz|maiz|patatas?|papas?|tapioca|mandioca|yuca|garbanzos?|almendras?|coco|castanas?|guisantes?|lentejas?|soja|quinoa|quinua|teff|sorgo|mijo|amaranto|lino|chia|altramuz|avellanas?|cacahuetes?|algarroba|garrofin|leche)(?:\s+integral(?:es)?)?"),
        (En, r"(?:rice|corn|maize|potato|tapioca|chickpea|almond|coconut|pea|soy|soya|sorghum|millet|teff|quinoa)\s+(?:flour|starch|semolina|bran|protein)|(?:flour|starch)\s+of\s+(?:rice|corn|potato)|cornstarch|cornflour"),
        (Fr, r"(?:farine|amidon|fecule|semoule)\s+de\s+(?:riz|mais|pomme\s+de\s+terre|tapioca|pois\s+chiches?|amandes?|coco|chataignes?|soja)"),
        (It, r"(?:farina|amido|fecola|semola)\s+di\s+(?:riso|mais|patate|tapioca|ceci|mandorle|cocco|castagne|soia)"),
        (Pt, r"(?:farinha|amido|fecula)\s+de\s+(?:arroz|milho|batata|tapioca|grao|amendoa|coco|mandioca)"),
        (De, r"(?:reis|mais|kartoffel|tapioka)(?:mehl|starke|griess)|johannisbrotkernmehl|guarkernmehl"),
    ],
        origins = "cereal, tubérculo o semilla sin gluten indicado en el nombre",
        sources = &[Face, CoeliacUk],
        note = Some(CONTAMINATION),
    ),
    rule!("cocoa-and-nut-paste", NaturallyGlutenFree, [
        (Es, r"(?:pasta|crema|manteca|pure)\s+de\s+(?:cacao|avellanas?|almendras?|cacahuetes?|sesamo|pistachos?|tomate|pimientos?|aceitunas?|anchoas?|membrillo|datiles)"),
        (En, r"cocoa\s+(?:mass|butter|paste|liquor)|(?:hazelnut|almond|peanut|sesame)\s+(?:paste|butter)"),
        (Fr, r"(?:pate|beurre)\s+de\s+(?:cacao|noisettes?|amandes?)"),
        (It, r"(?:pasta|burro)\s+di\s+(?:cacao|nocciole|mandorle)"),
        (De, r"kakaomasse|kakaobutter"),
    ]),
    rule!("wine-vinegar", GenerallyCompatible, [
        (Es, r"vinagre\s+de\s+(?:vino|manzana|jerez|modena|arroz|alcohol|sidra)(?:\s+(?:blanco|tinto))?"),
        (En, r"(?:wine|cider|apple|rice|spirit)\s+vinegar"),
        (Fr, r"vinaigre\s+de\s+(?:vin|cidre|riz)"),
        (It, r"aceto\s+(?:di\s+vino|balsamico)"),
        (De, r"(?:wein|apfel|branntwein)essig"),
    ]),
    rule!("soy-protein", GenerallyCompatible, [
        (Es, r"proteinas?\s+(?:de\s+)?(?:soja|guisante|patata|arroz|leche|suero)(?:\s+(?:aislada|concentrada|texturizada))?|inulina|fibras?\s+de\s+(?:achicoria|citricos|guisante|bambu|psyllium|manzana)"),
        (En, r"(?:soy|soya|pea|potato|milk|whey)\s+protein(?:\s+(?:isolate|concentrate))?"),
        (Fr, r"proteines?\s+de\s+(?:soja|pois|lait)"),
    ]),
    rule!("maltitol", GenerallyCompatible, [
        // English first: "maltitol syrup" before the Spanish "maltitol".
        (En, r"maltitol(?:\s+syrup)?|isomalt|polyols"),
        (Es, r"(?:jarabe\s+de\s+)?maltitol(?:es)?|isomalt|isomaltulosa|polioles|polialcoholes"),
        (Pt, r"(?:xarope\s+de\s+)?maltitol|maltitois|poliois"),
    ],
        origins = "almidón hidrolizado (maíz, trigo...) transformado en un poliol",
        sources = &[Face, CoeliacUk],
    ),
    // ---- 3. Gluten sources (also known to the core engine) -----------------------
    rule!("wheat", GlutenSource, [
        (Es, r"trigos?(?:\s+(?:duro|blando|integral|tierno))?|gluten(?:\s+de\s+trigo)?"),
        (En, r"wheat|durum|gluten"),
        (Fr, r"ble|froment"),
        (It, r"frumento|grano\s+(?:duro|tenero)|glutine"),
        (Pt, r"trigo"),
        (De, r"weizen[a-z0-9]*|hartweizen[a-z0-9]*"),
        (Ca, r"blat"),
        (Nl, r"tarwe[a-z0-9]*"),
    ],
        origins = "Triticum: trigo duro y blando y sus harinas, sémolas, almidón, salvado y proteína",
        exception = Some("Salvo jarabes de glucosa, dextrosa y maltodextrinas de trigo (anexo II.1.a-b)"),
        sources = &[Eu1169, Aesan, Face, CoeliacUk],
        note = Some("El trigo contiene gluten."),
    ),
    rule!("spelt", GlutenSource, [
        (Es, r"espelta|escanda|kamut|khorasan|einkorn|emmer|farro|freekeh|frik"),
        (En, r"spelt|khorasan|einkorn|emmer|farro|freekeh"),
        (Fr, r"epeautre|engrain|amidonnier"),
        (It, r"farro|spelta"),
        (De, r"dinkel[a-z0-9]*|grunkern|emmer"),
    ],
        origins = "variedades e híbridos del trigo",
        sources = &[Eu1169, Face, CoeliacUk],
        note = Some("La espelta, la escanda, el kamut y el farro son variedades de trigo y contienen gluten."),
    ),
    rule!("barley", GlutenSource, [
        (Es, r"cebada"),
        (En, r"barley"),
        (Fr, r"orge"),
        (It, r"orzo"),
        (Pt, r"cevada"),
        (De, r"gerste[a-z0-9]*|gersten[a-z0-9]*"),
        (Ca, r"ordi"),
        (Nl, r"gerst"),
    ],
        origins = "Hordeum",
        exception = Some("Salvo jarabes de glucosa de cebada (anexo II.1.c)"),
        sources = &[Eu1169, Face, CoeliacUk],
        note = Some("La cebada contiene gluten."),
    ),
    rule!("malt", GlutenSource, [
        (Es, r"(?:extracto\s+de\s+|jarabe\s+de\s+)?malta(?:\s+de\s+cebada)?|maltead[oa]s?"),
        (En, r"malt(?:ed)?(?:\s+extract)?"),
        (Fr, r"malte"),
        (It, r"malto"),
        (De, r"malz[a-z0-9]*"),
    ],
        origins = "cebada germinada, salvo que se indique otro cereal",
        sources = &[Eu1169, Face, CoeliacUk],
        context = Some(Context {
            description: "Coeliac UK considera apto el vinagre de malta por su contenido muy bajo; la legislación de la UE no lo exime",
            outside_eu: GlutenSource,
        }),
        note = Some("La malta procede de la cebada y contiene gluten salvo que se indique otro cereal."),
    ),
    rule!("rye", GlutenSource, [
        (Es, r"centeno"),
        (En, r"rye"),
        (Fr, r"seigle"),
        (It, r"segale"),
        (Pt, r"centeio"),
        (De, r"roggen[a-z0-9]*"),
        (Ca, r"segol"),
        (Nl, r"rogge"),
    ],
        origins = "Secale",
        sources = &[Eu1169, Face, CoeliacUk],
        note = Some("El centeno contiene gluten."),
    ),
    rule!("triticale", GlutenSource, [(Es, r"triticale")],
        origins = "híbrido de trigo y centeno",
        sources = &[Eu1169, Face],
        note = Some("El triticale es un híbrido de trigo y centeno y contiene gluten."),
    ),
    rule!("wheat-products", GlutenSource, [
        (Es, r"seitan|bulgur|burgol|cuscus|couscous|cous\s+cous"),
        (En, r"seitan|bulgur|couscous"),
        (Fr, r"boulgour|couscous"),
        (It, r"cuscus|bulgur"),
    ],
        origins = "trigo",
        sources = &[Face, CoeliacUk],
        note = Some("El seitán, el bulgur y el cuscús se elaboran con trigo y contienen gluten."),
    ),
    // ---- 4. Oats ---------------------------------------------------------------------
    rule!("oats", Oats, [
        (Es, r"(?:copos\s+de\s+|salvado\s+de\s+|harina\s+de\s+)?avena"),
        (En, r"oats?(?:meal)?|oat\s+(?:flakes|bran|flour)"),
        (Fr, r"avoine"),
        (It, r"avena"),
        (Pt, r"aveia"),
        (De, r"hafer[a-z0-9]*"),
        (Ca, r"civada"),
        (Nl, r"haver[a-z0-9]*"),
    ],
        origins = "Avena sativa",
        sources = OATS_SOURCES,
        note = Some("La avena solo es apta si el producto declara «sin gluten»: casi siempre se contamina con trigo o cebada (Reglamento 828/2014)."),
    ),
    // ---- 5. Usually made with wheat, cereal not given ----------------------------
    rule!("brewers-yeast", LikelyGluten, [
        (Es, r"levadura\s+de\s+cerveza"),
        (En, r"brewer\s*s?\s+yeast"),
        (Fr, r"levure\s+de\s+biere"),
        (De, r"bierhefe"),
    ],
        origins = "residuo de la fabricación de cerveza de cebada",
        sources = &[Face, CoeliacUk],
        note = Some("La levadura de cerveza puede arrastrar cebada: compruébalo en el envase."),
    ),
    rule!("soy-sauce", LikelyGluten, [
        (Es, r"salsa\s+de\s+soja"),
        (En, r"soy(?:a)?\s+sauce"),
        (Fr, r"sauce\s+(?:de\s+)?soja"),
        (It, r"salsa\s+di\s+soia"),
        (Pt, r"molho\s+de\s+soja"),
        (De, r"sojasauce|sojasosse"),
    ],
        origins = "habitualmente soja y trigo fermentados",
        sources = &[Face, CoeliacUk],
        note = Some("La salsa de soja suele elaborarse con trigo; si la etiqueta no lo aclara, compruébalo."),
    ),
    rule!("bread-and-bakery", LikelyGluten, [
        (Es, r"pan(?:es)?(?:\s+rallado|\s+de\s+molde)?|rebozad[oa]s?|empanad[oa]s?|galletas?|bizcochos?|obleas?|barquillos?|hojaldres?|masa(?:\s+madre)?|crutones|picatostes|croquetas?|tortitas?|magdalenas?|harina\s+de\s+fuerza|panko|graham|wraps?"),
        (En, r"bread(?:crumbs?)?|crumbs?|breaded|batter|biscuits?|crackers?|cookies?|wafers?|rusks?|croutons|pastry|dough|panko"),
        (Fr, r"pain|chapelure|panure|gaufrettes?|biscuits?|pate\s+feuilletee"),
        (It, r"pane|pangrattato|biscotti|cialde?"),
        (Pt, r"pao|biscoitos?|bolachas?"),
        (De, r"brot|paniermehl|kekse?|waffeln?|zwieback"),
    ],
        origins = "habitualmente harina de trigo",
        sources = &[Face, CoeliacUk],
        note = Some("Este ingrediente suele elaborarse con harina de trigo y la lista no indica el cereal."),
    ),
    rule!("pasta", LikelyGluten, [
        (Es, r"pastas?|fideos?|tallarines|macarrones|espaguetis?|lasanas?|canelones|raviolis?|noquis"),
        (En, r"pasta|noodles?|spaghetti|macaroni|lasagne"),
        (Fr, r"pates|nouilles"),
        (It, r"pasta|spaghetti|tagliatelle|maccheroni"),
        (De, r"nudeln|teigwaren"),
    ],
        origins = "habitualmente sémola de trigo duro",
        sources = &[Face],
        note = Some("La pasta suele elaborarse con trigo y la lista no indica el cereal."),
    ),
    rule!("beer", LikelyGluten, [
        (Es, r"cervezas?"),
        (En, r"beer|ale|lager|stout"),
        (Fr, r"bieres?"),
        (It, r"birra"),
        (Pt, r"cervejas?"),
        (De, r"bier"),
    ],
        origins = "habitualmente malta de cebada",
        sources = &[Face, CoeliacUk],
        note = Some("La cerveza suele elaborarse con malta de cebada."),
    ),
    rule!("semolina-and-bran", LikelyGluten, [
        (Es, r"semolas?|salvado|germen"),
        (En, r"semolina|bran|germ"),
        (Fr, r"semoule"),
        (It, r"semola"),
        (De, r"griess"),
    ],
        origins = "habitualmente trigo",
        sources = &[Face],
        note = Some("La sémola, el salvado y el germen suelen ser de trigo si no se indica otro cereal."),
    ),
    // ---- 6. Gluten depends on an origin the label does not give -------------------
    rule!("modified-starch-e14xx", NeedsOrigin, [(Es, r"e\s?14[0-9]{2}")],
        origins = "maíz, patata, trigo, tapioca",
        sources = &[Eu1169, Face],
        note = Some("Los almidones modificados (E14xx) pueden proceder del trigo; en la UE debería indicarse."),
    ),
    rule!("flour", NeedsOrigin, [
        (Es, r"harinas?"),
        (En, r"flour"),
        (Fr, r"farines?"),
        (It, r"farina"),
        (Pt, r"farinhas?"),
        (De, r"mehl|[a-z]+mehl"),
    ],
        origins = "cualquier cereal; en la práctica, casi siempre trigo",
        sources = &[Eu1169, Face],
        note = Some("La lista dice «harina» sin indicar el cereal."),
    ),
    rule!("starch", NeedsOrigin, [
        (Es, r"almidon(?:es)?(?:\s+(?:modificados?|hidrolizados?|nativos?))?|feculas?"),
        (En, r"(?:modified\s+)?starch"),
        (Fr, r"amidon(?:\s+modifie)?|fecule"),
        (It, r"amido(?:\s+modificato)?"),
        (Pt, r"amido"),
        (De, r"starke"),
    ],
        origins = "maíz, patata, trigo, tapioca, arroz",
        sources = &[Eu1169, Face],
        note = Some("La lista dice «almidón» sin indicar su origen. En la UE el almidón de trigo debe declararse, pero conviene comprobarlo en el envase."),
    ),
    rule!("vegetable-protein", NeedsOrigin, [
        (Es, r"proteinas?\s+(?:vegetal(?:es)?|hidrolizadas?)(?:\s+hidrolizadas?)?"),
        (En, r"(?:hydroly[sz]ed\s+)?(?:vegetable|plant)\s+protein"),
        (Fr, r"proteines?\s+vegetales?"),
        (De, r"pflanzeneiweiss"),
    ],
        origins = "soja, trigo, maíz, guisante",
        sources = &[Eu1169, Face, CoeliacUk],
        note = Some("La proteína vegetal puede ser de trigo y la lista no indica su origen."),
    ),
    rule!("dextrin", NeedsOrigin, [
        (Es, r"dextrinas?"),
        (En, r"dextrins?"),
        (Fr, r"dextrines?"),
    ],
        origins = "almidón de maíz, patata o trigo",
        sources = &[Face],
        note = Some("La dextrina puede proceder del trigo y no está entre las excepciones legales."),
    ),
    rule!("cereals", NeedsOrigin, [
        (Es, r"cereales"),
        (En, r"cereals?|grains?"),
        (Fr, r"cereales"),
        (It, r"cereali"),
        (De, r"getreide"),
    ],
        origins = "sin especificar",
        sources = &[Eu1169, Face],
        note = Some("La lista menciona cereales sin decir cuáles."),
    ),
    rule!("yeast-extract", NeedsOrigin, [
        (Es, r"extractos?\s+de\s+levadura"),
        (En, r"yeast\s+extract"),
        (Fr, r"extrait\s+de\s+levure"),
        (De, r"hefeextrakt"),
    ],
        origins = "levadura de panadería o de cerveza",
        sources = &[Face, CoeliacUk],
        note = Some("El extracto de levadura puede proceder de la cerveza; la lista no lo aclara."),
    ),
    rule!("fibre", NeedsOrigin, [
        (Es, r"fibras?(?:\s+vegetal(?:es)?|\s+de\s+cereales?)?"),
        (En, r"fib(?:re|er)"),
        (Fr, r"fibres?"),
        (De, r"ballaststoffe"),
    ],
        origins = "trigo, avena, achicoria, cítricos...",
        sources = &[Face],
        note = Some("La fibra puede ser de trigo o de avena y la lista no indica su origen."),
    ),
    rule!("compound-without-list", NeedsOrigin, [
        (Es, r"sazonador(?:es)?|condimentos?|preparados?|mezclas?|rellenos?|coberturas?|glaseados?|caldos?|salsas?|tamari|bechamel"),
        (En, r"seasonings?|fillings?|coatings?|toppings?|stock|bouillon|sauces?|tamari"),
        (Fr, r"assaisonnement|garniture|enrobage|bouillon|sauces?"),
        (It, r"condimento|ripieno|copertura|brodo|salsa"),
        (De, r"wurze|wurzmischung|fullung|brühe|bruhe|sosse|sauce"),
    ],
        origins = "mezcla de ingredientes",
        sources = &[Eu1169, Face],
        note = Some("Es un ingrediente compuesto y la lista no detalla de qué está hecho."),
    ),
    // ---- 7. Context-dependent, no alert by itself under EU labelling -------------
    rule!("natural-flavouring", ContextDependent, [
        (Es, r"aromas?\s+naturales?(?:\s+de\s+[a-z]+)?"),
        (En, r"natural\s+flavou?r(?:ing)?s?"),
        (Fr, r"aromes?\s+naturels?"),
        (It, r"aromi\s+naturali|aroma\s+naturale"),
        (Pt, r"aromas?\s+naturais"),
        (De, r"naturliche[sn]?\s+aromas?|naturliches\s+aroma"),
    ],
        origins = "extractos vegetales o animales",
        context = Some(Context {
            description: "En la UE, un aroma elaborado con un cereal con gluten debe destacar ese cereal (art. 21 del Reglamento 1169/2011); fuera de la UE no siempre",
            outside_eu: NeedsOrigin,
        }),
        sources = &[Eu1169, Aesan, CoeliacUk],
        note = Some("Los aromas naturales no demuestran la presencia de gluten."),
        note_also = Some("Los aromas naturales tampoco demuestran la presencia de gluten."),
    ),
    rule!("flavouring", ContextDependent, [
        (Es, r"aromas?|aromatizantes?|saborizantes?"),
        (En, r"flavou?r(?:ing)?s?"),
        (Fr, r"aromes?"),
        (It, r"aromi|aroma"),
        (Pt, r"aromas?"),
        (De, r"aromas?|aromen"),
    ],
        origins = "extractos o sustancias aromatizantes",
        context = Some(Context {
            description: "En la UE, un aroma elaborado con un cereal con gluten debe destacar ese cereal; fuera de la UE no siempre",
            outside_eu: NeedsOrigin,
        }),
        sources = &[Eu1169, Aesan, CoeliacUk],
        note = Some("Los aromas no demuestran la presencia de gluten."),
        note_also = Some("Los aromas tampoco demuestran la presencia de gluten."),
    ),
    rule!("spices", ContextDependent, [
        (Es, r"especias?|hierbas(?:\s+aromaticas)?|curry"),
        (En, r"spices?|herbs?|curry"),
        (Fr, r"epices|herbes"),
        (It, r"spezie|erbe"),
        (Pt, r"especiarias|ervas"),
        (De, r"gewurze?|krauter"),
    ],
        origins = "plantas aromáticas; las mezclas pueden llevar harina como antiapelmazante",
        context = Some(Context {
            description: "En la UE, una harina de trigo en una mezcla de especias debe declararse",
            outside_eu: NeedsOrigin,
        }),
        sources = &[Eu1169, Face],
        note = Some("Las especias no aportan gluten por sí mismas; en mezclas, la harina debe declararse."),
    ),
    rule!("smoke-and-vinegar", ContextDependent, [
        (Es, r"humo|aroma\s+de\s+humo|vinagres?|maltosa|mostaza"),
        (En, r"smoke|vinegar|maltose|mustard"),
        (Fr, r"fumee|vinaigre|moutarde"),
        (It, r"aceto|senape"),
        (De, r"rauch|essig|senf"),
    ],
        origins = "variable",
        context = Some(Context {
            description: "Fuera de la UE, un vinagre de malta o una mostaza con harina pueden no indicarse",
            outside_eu: NeedsOrigin,
        }),
        sources = &[Eu1169, Face],
    ),
    // ---- 8. Generally compatible ---------------------------------------------------
    rule!("dextrose", GenerallyCompatible, [
        (Es, r"dextrosa|glucosa(?:\s+anhidra)?"),
        (En, r"dextrose|glucose"),
        (Fr, r"dextrose|glucose"),
        (It, r"destrosio|glucosio"),
        (Pt, r"dextrose|glicose|glucose"),
        (De, r"dextrose|gl[uy][ck]ose"),
    ],
        origins = "almidón de maíz, trigo o patata",
        exception = Some("Si es de trigo, anexo II.1.a la exime: el proceso elimina el gluten"),
        sources = EXEMPT,
        note = Some("La dextrosa no representa por sí misma un motivo de alerta por gluten."),
    ),
    rule!("glucose-syrup", GenerallyCompatible, [
        (Es, r"(?:jarabe|sirope)s?\s+de\s+(?:glucosa|fructosa)(?:\s+y\s+(?:glucosa|fructosa))?|fructosa|jarabe\s+de\s+azucar\s+invertido"),
        (En, r"(?:glucose|fructose|glucose\s+fructose)\s+syrup|fructose|invert\s+sugar\s+syrup"),
        (Fr, r"sirop\s+de\s+(?:glucose|fructose)(?:\s+fructose)?|fructose"),
        (It, r"sciroppo\s+di\s+(?:glucosio|fruttosio)|fruttosio"),
        (Pt, r"xarope\s+de\s+(?:glucose|frutose)|frutose"),
        (De, r"(?:gl[uy][ck]ose|fru[ck]tose)(?:\s+gl[uy][ck]ose)?sirup|fru[ck]tose"),
    ],
        origins = "almidón de maíz, trigo o cebada",
        exception = Some("Los de trigo y de cebada están exentos (anexo II.1.a y II.1.c)"),
        sources = EXEMPT,
        note = Some("El jarabe de glucosa no representa un motivo de alerta por gluten, aunque proceda del trigo."),
    ),
    rule!("maltodextrin", GenerallyCompatible, [
        (Es, r"maltodextrinas?"),
        (En, r"maltodextrins?"),
        (Fr, r"maltodextrines?"),
        (It, r"maltodestrin[ae]"),
        (De, r"maltodextrin"),
    ],
        origins = "almidón de maíz, trigo o patata",
        exception = Some("Si es de trigo, anexo II.1.b la exime"),
        sources = EXEMPT,
        note = Some("La maltodextrina no representa un motivo de alerta por gluten, aunque proceda del trigo."),
    ),
    rule!("glycerol", GenerallyCompatible, [
        (Es, r"glicerina|glicerol"),
        (Pt, r"glicerina|glicerol"),
        (En, r"glycerine?|glycerol"),
        (Fr, r"glycerine|glycerol"),
        (It, r"glicerina|glicerolo"),
        (De, r"glyzerin|glycerin"),
    ],
        origins = "grasas vegetales o animales (E 422)",
        sources = &[Face, CoeliacUk],
    ),
    rule!("glazing-waxes", GenerallyCompatible, [
        (Es, r"ceras?\s+de\s+(?:carnauba|abejas?|candelilla)|goma\s+laca|shellac"),
        (Pt, r"ceras?\s+de\s+(?:carnauba|abelhas?|candelila)|goma\s+laca"),
        (En, r"(?:carnauba|candelilla)\s+wax|beeswax|shellac"),
        (Fr, r"cires?\s+(?:de\s+)?(?:carnauba|candelilla|d\s+abeilles?)"),
        (It, r"cera\s+(?:di\s+)?(?:carnauba|candelilla|d\s+api)"),
        (De, r"carnaubawachs|candelillawachs|bienenwachs|schellack"),
    ],
        origins = "ceras vegetales o de abeja (E 901-E 904)",
        sources = &[Face, CoeliacUk],
    ),
    rule!("gum-base", ContextDependent, [
        (Es, r"gomas?\s+base|base\s+de\s+goma(?:\s+de\s+mascar)?|base\s+de\s+chicle"),
        (Pt, r"gomas?\s+base|base\s+de\s+pastilha\s+elastica"),
        (En, r"(?:chewing\s+)?gum\s+base"),
        (Fr, r"gomme\s+base|base\s+de\s+gomme"),
        (It, r"gomma\s+base"),
        (De, r"kaumasse"),
    ],
        origins = "mezcla de polímeros, resinas y ceras cuya composición no se detalla",
        context = Some(Context {
            description: "En la UE, un componente de la goma base elaborado con un cereal con gluten debe destacarse; fuera de la UE no siempre",
            outside_eu: NeedsOrigin,
        }),
        sources = &[Eu1169, Face],
        note = Some("La goma base no detalla su composición; por sí sola no demuestra la presencia de gluten."),
    ),
    rule!("sweeteners", GenerallyCompatible, [
        (Es, r"sorbitol|xilitol|eritritol|manitol|sucralosa|aspartamo|acesulfamo(?:\s+k)?|acesulfame(?:\s+k)?|sacarina|ciclamato|esteviol|glucosidos\s+de\s+esteviol|stevia"),
        (En, r"sorbitol|xylitol|erythritol|mannitol|sucralose|aspartame|acesulfame(?:\s+k)?|saccharin|cyclamate|steviol|stevia"),
        (Pt, r"sorbitol|xilitol|eritritol|manitol|sucralose|aspartame|acessulfame(?:\s+k)?|acesulfame(?:\s+k)?|sacarina|ciclamato|glicosidos\s+de\s+esteviol|stevia"),
    ]),
    rule!("raising-agents", GenerallyCompatible, [
        (Es, r"levaduras?(?:\s+(?:de\s+panaderia|fresca|seca|quimica))?|gasificantes?|impulsor(?:es)?|bicarbonatos?(?:\s+(?:sodico|de\s+sodio|amonico))?"),
        (En, r"yeast|baking\s+(?:soda|powder)|raising\s+agents?"),
        (Fr, r"levure|poudre\s+a\s+lever|bicarbonate"),
        (It, r"lievito"),
        (De, r"hefe|backpulver|natron"),
    ],
        note = None,
    ),
    rule!("additives", GenerallyCompatible, [
        (Es, r"(?:acido|acidos)\s+(?:citrico|ascorbico|lactico|acetico|malico|tartarico|sorbico|fosforico|grasos?)|citratos?|ascorbatos?|lactatos?|acetatos?|sorbatos?|benzoatos?|nitritos?|nitratos?|fosfatos?|difosfatos?|trifosfatos?|polifosfatos?|carbonatos?|sulfitos?|metabisulfitos?|glutamatos?|inosinatos?|guanilatos?|ribonucleotidos?|carragenanos?|carrageninas?|gomas?(?:\s+(?:xantana|guar|garrofin|arabiga|arabica|de\s+alfarroba|gelana|gellan|de\s+acacia|de\s+algarrobo))?|xantana|pectinas?|agar(?:\s+agar)?|alginatos?|gelatinas?|lecitinas?(?:\s+de\s+(?:soja|girasol|colza))?|digliceridos|monogliceridos|esteres|tocoferol(?:es)?|extracto\s+rico\s+en\s+tocoferoles|carotenos?|betacarotenos?|curcumina|carmin|cochinilla|clorofilas?|antocianinas?|riboflavina|caramelo|colorante\s+caramelo|celulosa|metilcelulosa|carboximetilcelulosa|konjac|glucomanano|cloruros?(?:\s+de\s+(?:sodio|potasio|calcio|magnesio))?|vitaminas?(?:\s+[a-z0-9]{1,3})?|calcio|hierro|zinc|magnesio|potasio|sodio|yodo|minerales|e\s?[0-9]{3}[a-z]?"),
        (En, r"(?:citric|ascorbic|lactic|acetic|malic|tartaric|sorbic|phosphoric|fatty)\s+acids?|citrates?|ascorbates?|lactates?|phosphates?|carbonates?|sulphites?|sulfites?|glutamates?|gum\s+(?:arabic|acacia)|(?:xanthan|guar|locust\s+bean|gellan|arabic|acacia)\s+gum|pectins?|alginates?|gelatine?|lecithins?|mono\s+and\s+diglycerides|tocopherols?|carotenes?|curcumin|carmine|caramel(?:\s+colou?r)?|cellulose|vitamins?(?:\s+[a-z0-9]{1,3})?|iron|calcium|zinc"),
        (Fr, r"acide\s+(?:citrique|ascorbique|lactique)|gomme\s+(?:xanthane|guar|arabique)|pectine|gelatine|lecithines?|caramel|vitamines?(?:\s+[a-z0-9]{1,3})?"),
        (It, r"acido\s+(?:citrico|ascorbico|lattico)|gomma\s+(?:xantana|di\s+guar)|pectina|lecitina|caramello"),
        (De, r"(?:zitronen|citronen|ascorbin|milch)saure|xanthan|guarkernmehl|johannisbrotkernmehl|pektin|gelatine|lecithine?|karamell"),
        (Pt, r"acidos?\s+(?:citrico|ascorbico|lactico|acetico|malico|tartarico|sorbico|fosforico)|citratos?|ascorbatos?|lactatos?|fosfatos?|carbonatos?|sulfitos?|tocoferois|tocoferol|extrato\s+rico\s+em\s+tocoferois|gomas?\s+(?:arabica|xantana|guar|de\s+alfarroba|gelana)|pectinas?|alginatos?|gelatina|lecitinas?(?:\s+de\s+(?:soja|girassol|colza))?|carrageninas?|caramelo|celulose|vitaminas?(?:\s+[a-z0-9]{1,3})?"),
    ],
        note = None,
    ),
    // ---- 9. Naturally gluten-free foods -------------------------------------------
    rule!("grains-gluten-free", NaturallyGlutenFree, [
        (Es, r"arroz|maiz|quinoa|quinua|amaranto|mijo|sorgo|teff|tapioca|mandioca|yuca|palomitas"),
        (En, r"rice|corn|maize|quinoa|amaranth|millet|sorghum|teff|tapioca|cassava|popcorn"),
        (Fr, r"riz|mais|quinoa|millet|sorgho|manioc"),
        (It, r"riso|mais|quinoa|miglio|sorgo"),
        (Pt, r"arroz|milho|quinoa|mandioca"),
        (De, r"reis|mais|hirse"),
    ],
        origins = "cereales y pseudocereales sin gluten",
        sources = &[Face, CoeliacUk],
        note = Some(CONTAMINATION),
    ),
    rule!("potato", NaturallyGlutenFree, [
        (Es, r"patatas?|papas?|batatas?|boniatos?"),
        (En, r"potato(?:es)?|sweet\s+potato(?:es)?"),
        (Fr, r"pommes?\s+de\s+terre|patates?\s+douces?"),
        (It, r"patate|patata"),
        (Pt, r"batatas?"),
        (De, r"kartoffeln?"),
    ]),
    rule!("oils-and-fats", NaturallyGlutenFree, [
        (Es, r"aceites?(?:\s+(?:vegetal(?:es)?|refinados?|virgen(?:\s+extra)?|de\s+[a-z]+(?:\s+(?:alto\s+)?oleico)?))*|grasas?(?:\s+(?:vegetal(?:es)?|de\s+[a-z]+))*|margarina|manteca(?:\s+de\s+[a-z]+)?"),
        (En, r"(?:[a-z]+\s+)?oils?|(?:vegetable\s+)?fats?|margarine|shortening"),
        (Fr, r"huiles?(?:\s+de\s+[a-z]+)?|graisses?(?:\s+vegetales?)?|margarine"),
        (It, r"oli[oi](?:\s+(?:di\s+[a-z]+|extra\s+vergine))?|grassi?"),
        (Pt, r"oleos?|azeite|gorduras?"),
        (De, r"(?:raps|sonnenblumen|oliven|palm|palmkern|kokos|soja|mais|distel|lein|erdnuss|sesam|pflanzen|speise)?(?:ol|fett)|margarine"),
    ]),
    rule!("dairy-and-eggs", NaturallyGlutenFree, [
        (Es, r"leches?|nata|crema\s+de\s+leche|mantequilla|queso(?:s)?|yogur(?:es)?|lactosa|suero(?:\s+de\s+leche)?|caseinas?|caseinatos?|fermentos?(?:\s+lacticos)?|cuajo|huevos?|clara(?:s)?|yemas?|ovoproductos"),
        (En, r"milk|cream|butter|cheese|yog(?:h)?urt|lactose|whey|caseinates?|casein|cultures|rennet|eggs?|egg\s+(?:white|yolk)"),
        (Fr, r"lait|creme|beurre|fromages?|yaourt|lactose|lactoserum|ferments|presure|oeufs?"),
        (It, r"latte|panna|burro|formaggio|yogurt|lattosio|siero|caglio|uova|uovo"),
        (Pt, r"leite|natas?|manteiga|queijo|iogurte|lactose|soro|ovos?"),
        (De, r"milch[a-z]*|sahne|butter|kase|joghurt|laktose|molke|eier|ei"),
    ]),
    rule!("meat-and-fish", NaturallyGlutenFree, [
        (Es, r"carnes?(?:\s+de\s+[a-z]+)?|pollo|pavo|cerdo|ternera|vacuno|cordero|conejo|pato|pechugas?|lomo|pescados?|atun|bonito|salmon|bacalao|merluza|sardinas?|anchoas?|caballa|boquerones|gambas?|langostinos?|mejillones|calamar(?:es)?|pulpo|sepia|almejas|berberechos|mariscos?"),
        (En, r"meat|chicken|turkey|pork|beef|lamb|duck|fish|tuna|salmon|cod|sardines?|anchov(?:y|ies)|mackerel|shrimps?|prawns?|mussels|squid"),
        (Fr, r"viande|poulet|dinde|porc|boeuf|agneau|poisson|thon|saumon|cabillaud|sardines?|crevettes?"),
        (It, r"carne|pollo|tacchino|maiale|manzo|pesce|tonno|salmone|merluzzo|gamberi"),
        (Pt, r"carne|frango|peru|porco|vaca|peixe|atum|salmao|bacalhau|camarao"),
        (De, r"fleisch|huhn|hahnchen|pute|schwein[a-z]*|rind[a-z]*|fisch|thunfisch|lachs"),
    ]),
    rule!("vegetables-and-fruit", NaturallyGlutenFree, [
        (Es, r"tomates?|cebollas?|ajos?|pimientos?|pimenton|zanahorias?|espinacas?|lechugas?|calabacin(?:es)?|berenjenas?|pepinos?|apio|puerros?|champinon(?:es)?|setas?|hongos?|brocoli|coliflor|calabazas?|col|repollo|remolacha|alcachofas?|esparragos?|judias?\s+verdes|aceitunas?|olivas?|verduras?|hortalizas?|legumbres?|garbanzos?|lentejas?|judias?|alubias?|frijoles|guisantes?|habas?|soja|altramuz|altramuces|frutas?|manzanas?|naranjas?|limon(?:es)?|limas?|fresas?|platanos?|peras?|uvas?|pasas|melocoton(?:es)?|albaricoques?|pinas?|mangos?|frambuesas?|arandanos?|cerezas?|ciruelas?|higos?|datiles|melon|sandia|kiwi|coco|aguacates?|zumos?(?:\s+de\s+[a-z]+)?|pulpa(?:\s+de\s+[a-z]+)?"),
        (En, r"tomato(?:es)?|onions?|garlic|peppers?|paprika|carrots?|spinach|lettuce|courgettes?|zucchini|aubergines?|eggplants?|cucumbers?|celery|leeks?|mushrooms?|broccoli|cauliflower|pumpkin|olives?|vegetables?|chickpeas?|lentils?|beans?|peas|soy(?:a|beans?)?|fruits?|apples?|oranges?|lemons?|limes?|strawberr(?:y|ies)|bananas?|pears?|grapes?|raisins|peach(?:es)?|apricots?|pineapples?|mangos?|raspberr(?:y|ies)|blueberr(?:y|ies)|cherr(?:y|ies)|plums?|dates|coconut|avocados?|juice"),
        (Fr, r"tomates?|oignons?|ail|poivrons?|carottes?|epinards|courgettes?|aubergines?|champignons?|olives?|legumes|pois\s+chiches|lentilles|haricots|pois|fruits?|pommes?|oranges?|citrons?|fraises?|bananes?|poires?|raisins?|peches?|abricots?|ananas|framboises?|cerises?|jus"),
        (It, r"pomodor[oi]|cipoll[ae]|aglio|peperon[ei]|carot[ae]|spinaci|zucchine|melanzane|funghi|olive|verdure|legumi|ceci|lenticchie|fagioli|piselli|frutta|mel[ae]|arance|limon[ei]|fragole|banane|pere|uva|pesche|albicocche|ananas|lamponi|ciliegie|succo"),
        (Pt, r"tomates?|cebolas?|alho|pimentos?|cenouras?|espinafres?|cogumelos?|azeitonas?|legumes|grao|lentilhas|feijao|ervilhas|frutas?|macas?|laranjas?|limao|morangos?|bananas?|peras?|uvas?|sumo"),
        (De, r"tomaten?|zwiebeln?|knoblauch|paprika|karotten?|mohren?|spinat|zucchini|pilze|oliven|gemuse|kichererbsen|linsen|bohnen|erbsen|soja|obst|fruchte|apfel|orangen|zitronen?|erdbeeren?|bananen?|birnen?|trauben|pfirsiche?|aprikosen|ananas|himbeeren?|kirschen|saft"),
    ]),
    rule!("nuts-and-seeds", NaturallyGlutenFree, [
        (Es, r"almendras?|avellanas?|nuez|nueces|anacardos?|pistachos?|castanas?|pinon(?:es)?|cacahuetes?|mani|sesamo|semillas?(?:\s+de\s+[a-z]+)?|pipas|chia|lino|linaza|cacao(?:\s+(?:en\s+polvo|desgrasado))?|cafe"),
        (En, r"almonds?|hazelnuts?|walnuts?|nuts|cashews?|pistachios?|chestnuts?|pine\s+nuts|peanuts?|sesame|seeds?|chia|linseed|flax(?:seed)?|cocoa|coffee"),
        (Fr, r"amandes?|noisettes?|noix|cajou|pistaches?|chataignes?|arachides?|cacahuetes?|sesame|graines?|lin|cacao|cafe"),
        (It, r"mandorle|nocciole|noci|anacardi|pistacchi|castagne|arachidi|sesamo|semi|lino|cacao|caffe"),
        (Pt, r"amendoas?|avelas?|nozes|cajus?|pistacios?|castanhas?|amendoim|sesamo|sementes?|linhaca|cacau|cafe"),
        (De, r"mandeln?|haselnusse?|walnusse?|nusse|cashew[a-z]*|pistazien|maronen|erdnusse?|sesam|samen|leinsamen|kakao|kaffee"),
    ]),
    rule!("basics", NaturallyGlutenFree, [
        (Es, r"agua|sal|azucar(?:es)?(?:\s+de\s+cana)?|sacarosa|panela|miel|vino|pimienta|comino|canela|nuez\s+moscada|clavo|jengibre|curcuma|azafran|cardamomo|anis|vainilla|perejil|albahaca|oregano|cilantro|romero|tomillo|menta|hierbabuena|laurel|eneldo|cebollino|guindilla|chile|te|lupulo"),
        (En, r"water|salt|sugar|sucrose|honey|wine|hops|pepper|cumin|cinnamon|nutmeg|cloves?|ginger|turmeric|saffron|vanilla|parsley|basil|oregano|coriander|rosemary|thyme|mint|bay\s+leaf|chilli|chili"),
        (Fr, r"eau|sel|sucre|miel|vin|poivre|cumin|cannelle|muscade|gingembre|vanille|persil|basilic|origan|thym|menthe"),
        (It, r"acqua|sale|zucchero|miele|vino|pepe|cannella|zenzero|vaniglia|prezzemolo|basilico|origano|rosmarino|timo|menta"),
        (Pt, r"agua|sal|acucar|mel|vinho|pimenta|canela|gengibre|baunilha|salsa|manjericao|oregaos?|alecrim|tomilho|hortela"),
        (De, r"wasser|salz|zucker|honig|wein|pfeffer|zimt|ingwer|vanille|petersilie|basilikum|oregano|rosmarin|thymian|minze"),
    ]),
];

/// Words that describe an ingredient without changing what it is.
static DESCRIPTORS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(concat!(
        r"\b(?:de|del|la|el|los|las|lo|y|e|o|u|con|en|al|a|por|para|of|the|with|from|in|and|or|",
        r"et|du|des|le|les|di|della|delle|dei|und|aus|mit|von|do|da|dos|das|com|",
        r"seleccionad[oa]s?|selected|selectionne(?:e|s|es)?|natural(?:es)?|naturel(?:le)?s?|naturale|naturali|naturlich(?:e[nrs]?)?|",
        r"virgen|extra|vierge|vergine|refinad[oa]s?|refined|raffinee?s?|frit[oa]s?|fried|tostad[oa]s?|toasted|roasted|torrefact[oa]|",
        r"deshidratad[oa]s?|dried|dehydrated|sec[oa]s?|polvo|powder|poudre|concentrad[oa]s?|concentrated?|fresc[oa]s?|fresh|frais|fraiche|",
        r"enter[oa]s?|whole|entier|interi|intero|integral(?:es)?|integrale|desnatad[oa]s?|semidesnatad[oa]s?|skimmed|semi|",
        r"pasteurizad[oa]s?|pasteuri[sz]ed|ecologic[oa]s?|biologic[oa]s?|organic|bio|picad[oa]s?|chopped|triturad[oa]s?|crushed|",
        r"cocid[oa]s?|cooked|blanc[oa]s?|white|blanche?s?|negr[oa]s?|black|noire?s?|moren[oa]s?|brown|marina|sea|yodad[oa]|",
        r"rehidratad[oa]s?|trocead[oa]s?|trozos?|rallad[oa]s?|grated|escamas|copos|flakes|liofilizad[oa]s?|vegetal(?:es)?|vegetable|vegetale?s?|",
        r"pur[oa]s?|pure|fin[oa]s?|molid[oa]s?|ground|pelad[oa]s?|peeled|descascarillad[oa]s?|crud[oa]s?|raw|madur[oa]s?|",
        r"dulces?|sweet|salad[oa]s?|salted|ahumad[oa]s?|smoked|curad[oa]s?|fermentad[oa]s?|hidrogenad[oa]s?|hydrogenated|",
        r"parcialmente|totalmente|sodic[oa]s?|potasic[oa]s?|calcic[oa]s?|sodium|potassium|calcium|mineral(?:es)?|total(?:es)?|exprimid[oa]s?|real|gallina|oveja|cabra|vaca|bufala|camper[oa]s?|",
        r"anadid[oa]s?|added|origen|origin|procedente|variable|proporcion(?:es)?|contiene|contains|minimo|min|maximo|",
        r"g|mg|kg|ml|cl|l|cada|calidad|categoria|primera|nacional|espana|espanola|tipo|variedad|especial|",
        r"clasic[oa]s?|agricultura|ue|agricola|cultivad[oa]s?|salvaje|congelad[oa]s?|laminad[oa]s?|rodajas|dados|cubos|tiras|hojas|",
        r"granos?|en|grano|alto|oleico|rojo|roja|rojos|rojas|verde|verdes|amarill[oa]s?|tierno|tierna|duro|dura|",
        r"anhidr[oa]|instantane[oa]|soluble|natives?|nativ[oa]|enriquecid[oa]s?|fortified|enriched|",
        r"aislad[oa]|isolate|texturizad[oa]|hidrolizad[oa]s?|estabilizad[oa]s?|emulsionad[oa]|reconstituid[oa]s?|",
        r"emulgentes?|emulsionantes?|antioxidantes?|conservador(?:es)?|conservantes?|estabilizantes?|acidulantes?|",
        r"corrector(?:es)?|regulador(?:es)?|acidez|gasificantes?|edulcorantes?|humectantes?|antiaglomerantes?|antiapelmazantes?|",
        r"potenciador(?:es)?|sabor|colorantes?|espesantes?|gelificantes?|agentes?|recubrimiento|endulzantes?|",
        r"antioxidants?|preservatives?|stabili[sz]ers?|emulsifiers?|acidity|regulators?|raising|sweeteners?|thickeners?|",
        r"colou?rs?|enhancers?|gelling|glazing|conservateurs?|emulsifiants?|epaississants?|colorants?|acidifiants?|",
        r"antioxydants?|stabilisants?|edulcorants?|conservanti|emulsionanti|addensanti|coloranti|antiossidanti|",
        r"stabilizzanti|edulcoranti|correttori|acidita|konservierungsstoffe?|emulgatoren?|verdickungsmittel|",
        r"farbstoffe?|antioxidationsmittel|sauerungsmittel|sussungsmittel|stabilisatoren?|backtriebmittel|",
        r"espessantes?|corantes?|acidificantes?|humidificantes?|revestimento|contem|",
        r"ingredientes?|ingredients?|mono)\b",
    ))
    .expect("valid regex")
});

/// Claim phrases inside an item ("harina de arroz sin gluten"): masked, never a gluten word.
static CLAIM: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\b(?:sin\s+gluten|gluten\s+free|libre\s+de\s+gluten|sans\s+gluten|glutenfrei|senza\s+glutine|sem\s+gluten|sense\s+gluten|glutenvrij)\b")
        .expect("valid regex")
});

/// An unknown word one or two letters away from a gluten word ("cebaba", "harlna",
/// "trlgo"): probably a reading error. Never left to the AI.
pub static MISSPELLING: Rule = Rule {
    id: "possible-misspelling",
    class: NeedsOrigin,
    terms: &[],
    origins: "error de lectura (OCR) o de transcripción",
    note: Some("Parece un cereal con gluten mal escrito o mal leído: compruébalo en el envase."),
    ..BASE
};

const GLUTEN_WORDS: &[&str] = &[
    "trigo",
    "cebada",
    "centeno",
    "avena",
    "espelta",
    "escanda",
    "malta",
    "harina",
    "semola",
    "kamut",
    "triticale",
    "seitan",
    "bulgur",
    "cuscus",
    "gluten",
    "wheat",
    "barley",
    "spelt",
    "flour",
    "farine",
    "froment",
    "seigle",
    "avoine",
    "frumento",
    "farina",
    "segale",
    "weizen",
    "gerste",
    "roggen",
    "dinkel",
    "hafer",
    "cevada",
    "centeio",
    "aveia",
    "farinha",
    "cerveza",
    "galleta",
    "malteada",
    "malteado",
    "glutine",
];

/// Text in the ingredient field that is not an ingredient: a fragment of the
/// nutrition table. Never decides anything; it only keeps the list readable.
pub static NUTRITION_TABLE: Rule = Rule {
    id: "nutrition-table",
    class: NotIngredient,
    terms: &[],
    origins: "tabla de información nutricional copiada junto a los ingredientes",
    ..BASE
};

/// A consumer warning required for some additives ("contiene una fuente de
/// fenilalanina", "un consumo excesivo puede producir efectos laxantes").
pub static CONSUMER_WARNING: Rule = Rule {
    id: "consumer-warning",
    class: NotIngredient,
    terms: &[],
    origins: "advertencia obligatoria de consumo (Reglamento (UE) 1169/2011, anexo III)",
    sources: &[Eu1169],
    ..BASE
};

/// A consumer warning with words that are not part of it: never reassuring.
pub static UNCLEAR_WARNING: Rule = Rule {
    id: "consumer-warning-unclear",
    class: NotIngredient,
    terms: &[],
    origins: "advertencia de consumo mezclada con otro texto",
    sources: &[Eu1169],
    ..BASE
};

/// Words found only in a nutrition table ("valor energético", "de las cuales").
const TABLE_ONLY: &[&str] = &[
    "energetico",
    "energetica",
    "energ",
    "energia",
    "kcal",
    "kj",
    "lipidos",
    "saturadas",
    "saturados",
    "saturada",
    "saturates",
    "monoinsaturadas",
    "poliinsaturadas",
    "hidratos",
    "carbohidratos",
    "carboidratos",
    "cuales",
    "quais",
    "polialcoholes",
    "poliois",
    "polyols",
    "nutricional",
    "nutricionales",
    "nutrition",
];
/// Words a nutrition table shares with ingredient lists.
const TABLE_TOO: &[&str] = &[
    "valor",
    "valores",
    "grasas",
    "gorduras",
    "carbono",
    "carbon",
    "azucares",
    "acucares",
    "sugars",
    "proteinas",
    "protein",
    "proteins",
    "fibra",
    "fibras",
    "alimentaria",
    "sal",
    "salt",
    "fat",
    "fats",
    "which",
    "porcion",
    "racion",
];
/// Words of a consumer warning.
const WARNING_WORDS: &[&str] = &[
    "contiene",
    "contem",
    "contains",
    "una",
    "uma",
    "un",
    "um",
    "fuente",
    "fonte",
    "source",
    "fenilalanina",
    "phenylalanine",
    "consumo",
    "excesivo",
    "excessivo",
    "excessive",
    "consumption",
    "puede",
    "pode",
    "may",
    "producir",
    "provocar",
    "ter",
    "have",
    "cause",
    "efectos",
    "efeitos",
    "effects",
    "laxantes",
    "laxante",
    "laxativos",
    "laxativo",
    "laxative",
    "seu",
    "su",
    "its",
    "an",
];
/// Words that never count either way.
const FILLER: &[&str] = &[
    "de", "del", "la", "el", "los", "las", "dos", "das", "da", "do", "e", "y", "o", "a", "of",
    "the", "and", "por", "per", "en", "em", "g", "mg", "ml", "100",
];

fn words(matcher: &str) -> Vec<String> {
    crate::normalize(matcher)
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty() && !FILLER.contains(w))
        .map(str::to_string)
        .collect()
}

/// For an item no rule could place (`Unrecognized`, nothing worse than a generally
/// compatible ingredient matched): whether it is a nutrition-table fragment or a
/// consumer warning instead of an ingredient. Cautious by construction: an item with
/// a gluten word, a misread one, or any rule above "generally compatible" never
/// qualifies, so setting it aside cannot hide a risk.
pub fn set_aside(matcher: &str, classified: &Classification) -> Option<&'static Rule> {
    if classified.class != Unrecognized
        || classified
            .matched
            .iter()
            .any(|r| severity(r.class) > severity(GenerallyCompatible))
    {
        return None;
    }
    let all = words(matcher);
    if all.is_empty()
        || all
            .iter()
            .any(|w| GLUTEN_WORDS.contains(&w.as_str()) || looks_like_gluten_word(w))
    {
        return None;
    }
    // Descriptors ("edulcorantes", "antioxidante") are neutral too.
    let content: Vec<&String> = all.iter().filter(|w| !is_only_descriptors(w)).collect();
    let total = content.len();
    let table_only = content
        .iter()
        .filter(|w| TABLE_ONLY.contains(&w.as_str()))
        .count();
    let table = table_only
        + content
            .iter()
            .filter(|w| TABLE_TOO.contains(&w.as_str()))
            .count();
    if table_only >= 2 && table * 3 >= total * 2 {
        return Some(&NUTRITION_TABLE);
    }
    let anchored = content.iter().any(|w| {
        w.starts_with("fenilalanin") || w.starts_with("phenylalanin") || w.starts_with("laxa")
    });
    let foreign = content
        .iter()
        .filter(|w| !WARNING_WORDS.contains(&w.as_str()))
        .count();
    if !(anchored && total <= 14 && foreign <= 2 && classified.matched.is_empty()) {
        return None;
    }
    // Words outside the warning ("excessi", OCR noise or an ingredient): set apart
    // but never reassuring.
    Some(if foreign == 0 {
        &CONSUMER_WARNING
    } else {
        &UNCLEAR_WARNING
    })
}

/// Optimal string alignment distance: a swap of two letters ("espleta") counts as one.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let mut best = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + cost);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(d[i - 2][j - 2] + 1);
            }
            d[i][j] = best;
        }
    }
    d[a.len()][b.len()]
}

/// True for a word that is probably a misread gluten word.
pub fn looks_like_gluten_word(word: &str) -> bool {
    let length = word.chars().count();
    if length < 5 {
        return false;
    }
    let limit = if length >= 8 { 2 } else { 1 };
    GLUTEN_WORDS
        .iter()
        .any(|w| w.len().abs_diff(length) <= limit && *w != word && edit_distance(w, word) <= limit)
}

struct Compiled {
    set: RegexSet,
    each: Vec<Regex>,
}

static COMPILED: Lazy<Compiled> = Lazy::new(|| {
    let patterns: Vec<String> = RULES
        .iter()
        .map(|r| {
            let alternatives: Vec<&str> = r.terms.iter().map(|(_, t)| *t).collect();
            format!(r"\b(?:{})\b", alternatives.join("|"))
        })
        .collect();
    Compiled {
        set: RegexSet::new(&patterns).expect("valid catalog"),
        each: patterns
            .iter()
            .map(|p| Regex::new(p).expect("valid rule"))
            .collect(),
    }
});

/// Higher is worse for a coeliac reader.
pub fn severity(class: IngredientClass) -> u8 {
    match class {
        GlutenSource => 9,
        LikelyGluten => 8,
        Oats => 7,
        NeedsOrigin => 6,
        Unrecognized | Other => 5,
        ContextDependent => 4,
        LegalExemption => 3,
        GenerallyCompatible => 2,
        NaturallyGlutenFree => 1,
        NotIngredient => 0,
    }
}

#[derive(Debug, Clone)]
pub struct Classification {
    pub class: IngredientClass,
    pub rule: Option<&'static Rule>,
    /// Every rule that matched, in catalog order.
    pub matched: Vec<&'static Rule>,
    /// Words no rule recognised.
    pub leftover: String,
    /// The item itself says "sin gluten".
    pub claims_gluten_free: bool,
}

/// Classifies one item (folded text). `eu` selects the EU-labelling context.
pub fn classify(matcher: &str, eu: bool) -> Classification {
    let mut text = crate::normalize(matcher)
        .chars()
        .map(|c| if "().,;:!?".contains(c) { ' ' } else { c })
        .collect::<String>();
    let claims_gluten_free = CLAIM.is_match(&text);
    text = CLAIM.replace_all(&text, " ").into_owned();
    let compiled = &*COMPILED;
    let mut matched: Vec<&'static Rule> = Vec::new();
    let candidates = compiled.set.matches(&text);
    for index in candidates.iter() {
        let regex = &compiled.each[index];
        if !regex.is_match(&text) {
            continue; // already masked by an earlier, more specific rule
        }
        matched.push(&RULES[index]);
        text = regex.replace_all(&text, " ").into_owned();
    }
    let leftover = DESCRIPTORS
        .replace_all(&text, " ")
        .split_whitespace()
        .filter(|w| w.chars().any(|c| c.is_ascii_alphabetic()))
        .collect::<Vec<_>>()
        .join(" ");
    let effective = |r: &Rule| match r.context {
        Some(c) if !eu => c.outside_eu,
        _ => r.class,
    };
    // The first rule of the worst class (catalog order), for its explanation.
    let worst = matched
        .iter()
        .copied()
        .min_by_key(|r| std::cmp::Reverse(severity(effective(r))));
    let mut class = worst.map_or(Unrecognized, effective);
    let mut rule = worst.filter(|r| effective(r) == class);
    if !leftover.is_empty() && severity(Unrecognized) > severity(class) {
        class = Unrecognized;
        rule = None;
    }
    // Digits inside a word are a reading error too ("tr1go").
    let misread = leftover.split_whitespace().any(|w| {
        looks_like_gluten_word(w)
            || (w.chars().any(|c| c.is_ascii_digit())
                && looks_like_gluten_word(&w.replace('1', "i").replace('0', "o")))
            || GLUTEN_WORDS.contains(&w.replace('1', "i").replace('0', "o").as_str())
    });
    if misread && severity(NeedsOrigin) > severity(class) {
        class = NeedsOrigin;
        rule = Some(&MISSPELLING);
    }
    Classification {
        class,
        rule,
        matched,
        leftover,
        claims_gluten_free,
    }
}

/// True when nothing but descriptor words is left ("antioxidante", "en polvo").
pub fn is_only_descriptors(matcher: &str) -> bool {
    let text = crate::normalize(matcher);
    DESCRIPTORS
        .replace_all(&text, " ")
        .split_whitespace()
        .all(|w| !w.chars().any(|c| c.is_ascii_alphabetic()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(text: &str) -> IngredientClass {
        classify(text, true).class
    }

    #[test]
    fn every_rule_compiles_and_has_metadata() {
        assert_eq!(COMPILED.each.len(), RULES.len());
        let mut ids = std::collections::HashSet::new();
        for rule in RULES {
            assert!(ids.insert(rule.id), "duplicate id {}", rule.id);
            assert!(!rule.terms.is_empty());
            assert!(!rule.sources.is_empty());
            assert_eq!(rule.since, RULES_VERSION);
            if matches!(
                rule.class,
                GlutenSource | LikelyGluten | Oats | NeedsOrigin | LegalExemption
            ) {
                assert!(rule.note.is_some(), "{} needs an explanation", rule.id);
            }
            if rule.class == LegalExemption {
                assert!(
                    rule.exception.is_some(),
                    "{} needs its legal basis",
                    rule.id
                );
            }
        }
    }

    #[test]
    fn chewing_gum_ingredients_in_three_languages() {
        for (text, expected) in [
            // Spanish
            ("maltitoles", GenerallyCompatible),
            ("jarabe de maltitol", GenerallyCompatible),
            ("polialcoholes", GenerallyCompatible),
            ("glicerina", GenerallyCompatible),
            ("glicerol", GenerallyCompatible),
            ("acido citrico", GenerallyCompatible),
            ("acido malico", GenerallyCompatible),
            ("acesulfamo k", GenerallyCompatible),
            ("acesulfame k", GenerallyCompatible),
            ("sucralosa", GenerallyCompatible),
            ("cera de carnauba", GenerallyCompatible),
            ("cera de abejas", GenerallyCompatible),
            ("goma arabiga", GenerallyCompatible),
            ("tocoferoles", GenerallyCompatible),
            ("extracto rico en tocoferoles", GenerallyCompatible),
            ("goma base", ContextDependent),
            ("aromas", ContextDependent),
            // Portuguese
            ("maltitois", GenerallyCompatible),
            ("poliois", GenerallyCompatible),
            ("extrato rico em tocoferois", GenerallyCompatible),
            ("acessulfame k", GenerallyCompatible),
            ("sucralose", GenerallyCompatible),
            ("goma arabica", GenerallyCompatible),
            // English
            ("maltitol syrup", GenerallyCompatible),
            ("glycerol", GenerallyCompatible),
            ("glycerine", GenerallyCompatible),
            ("citric acid", GenerallyCompatible),
            ("carnauba wax", GenerallyCompatible),
            ("gum arabic", GenerallyCompatible),
            ("tocopherols", GenerallyCompatible),
            ("gum base", ContextDependent),
        ] {
            assert_eq!(class(text), expected, "{text}");
        }
        assert_eq!(
            classify("cera de carnauba", true).rule.unwrap().id,
            "glazing-waxes"
        );
        // Outside EU labelling a gum base says nothing about its components.
        assert_eq!(classify("gum base", false).class, NeedsOrigin);
    }

    fn aside(text: &str) -> Option<&'static str> {
        set_aside(text, &classify(text, true)).map(|r| r.id)
    }

    #[test]
    fn nutrition_tables_and_warnings_are_not_ingredients() {
        for (text, kind) in [
            (
                "Energético/Energ Grasas/Lipidos Edulcorantes",
                "nutrition-table",
            ),
            (
                "Hidratos de Carbono de los cuales Azúcares Polialcoholes Proteínas Sal",
                "nutrition-table",
            ),
            ("Valor energético kJ kcal", "nutrition-table"),
            ("Contiene una fuente de fenilalanina", "consumer-warning"),
            ("Contém uma fonte de fenilalanina", "consumer-warning"),
            ("Contains a source of phenylalanine", "consumer-warning"),
            (
                "Un consumo excesivo puede producir efectos laxantes",
                "consumer-warning",
            ),
            (
                "Excessive consumption may have laxative effects",
                "consumer-warning",
            ),
        ] {
            assert_eq!(aside(text), Some(kind), "{text}");
        }
    }

    #[test]
    fn setting_aside_never_hides_a_risk() {
        // A gluten word, a misread one or an ambiguous ingredient keeps the item.
        for text in [
            "Grasas Lipidos Saturadas harina de trigo",
            "Hidratos de carbono de los cuales azucares trigo",
            "Energetico lipidos saturadas almidon",
            "Hidratos de carbono de los cuales cebaba",
            "Contiene una fuente de fenilalanina y malta",
            "fenilalanina harlna",
            "Grasas saturadas aromas",
            // Real ingredients that only share words with a table.
            "proteinas de leche",
            "grasa vegetal",
            "sal",
            // Unknown words are not turned into a nutrition table either.
            "grasas xyzolina fructina",
        ] {
            assert_eq!(aside(text), None, "{text}");
        }
    }

    #[test]
    fn kimbo_items() {
        assert_eq!(class("patatas seleccionadas"), NaturallyGlutenFree);
        assert_eq!(class("aceite de oliva"), NaturallyGlutenFree);
        assert_eq!(class("sal"), NaturallyGlutenFree);
        assert_eq!(class("dextrosa"), GenerallyCompatible);
        assert_eq!(class("aromas naturales"), ContextDependent);
        assert_eq!(classify("dextrosa", true).rule.unwrap().id, "dextrose");
    }

    #[test]
    fn legal_exemptions_are_narrow() {
        assert_eq!(class("jarabe de glucosa de trigo"), LegalExemption);
        assert_eq!(class("dextrosa de trigo"), LegalExemption);
        assert_eq!(class("maltodextrina de trigo"), LegalExemption);
        assert_eq!(class("jarabe de glucosa de cebada"), LegalExemption);
        assert_eq!(class("wheat glucose syrup"), LegalExemption);
        assert_eq!(class("sirop de glucose de ble"), LegalExemption);
        assert_eq!(class("weizenglukosesirup"), LegalExemption);
        // Not exempt: starch, flour, malt and barley maltodextrin.
        assert_eq!(class("almidon de trigo"), GlutenSource);
        assert_eq!(class("harina de trigo"), GlutenSource);
        assert_eq!(class("maltodextrina de cebada"), GlutenSource);
        assert_eq!(class("extracto de malta de cebada"), GlutenSource);
        assert_eq!(class("dextrina de trigo"), GlutenSource);
        // A second cereal in the same item is never hidden by the exemption.
        assert_eq!(
            class("jarabe de glucosa de trigo y harina de trigo"),
            GlutenSource
        );
    }

    #[test]
    fn specific_phrases_beat_general_words() {
        assert_eq!(class("trigo sarraceno"), NaturallyGlutenFree);
        assert_eq!(class("harina de trigo sarraceno"), NaturallyGlutenFree);
        assert_eq!(class("harina de trigo sarraceno y trigo"), GlutenSource);
        assert_eq!(class("harina de arroz"), NaturallyGlutenFree);
        assert_eq!(class("almidon de maiz"), NaturallyGlutenFree);
        assert_eq!(class("pasta de cacao"), NaturallyGlutenFree);
        assert_eq!(class("buchweizenmehl"), NaturallyGlutenFree);
        assert_eq!(class("levadura de cerveza"), LikelyGluten);
        assert_eq!(class("levadura"), GenerallyCompatible);
        assert_eq!(class("salsa de soja"), LikelyGluten);
        assert_eq!(class("soja"), NaturallyGlutenFree);
        assert_eq!(class("maltodextrina"), GenerallyCompatible);
        assert_eq!(class("malta"), GlutenSource);
    }

    #[test]
    fn vague_and_contextual_ingredients() {
        assert_eq!(class("harina"), NeedsOrigin);
        assert_eq!(class("almidon"), NeedsOrigin);
        assert_eq!(class("almidon modificado"), NeedsOrigin);
        assert_eq!(class("e1422"), NeedsOrigin);
        assert_eq!(class("e 330"), GenerallyCompatible);
        assert_eq!(class("proteina vegetal hidrolizada"), NeedsOrigin);
        assert_eq!(class("cereales"), NeedsOrigin);
        assert_eq!(class("pan rallado"), LikelyGluten);
        assert_eq!(class("galletas"), LikelyGluten);
        assert_eq!(class("aroma"), ContextDependent);
        assert_eq!(class("especias"), ContextDependent);
        assert_eq!(class("copos de avena"), Oats);
        assert_eq!(class("avena sin gluten"), Oats);
        assert!(classify("avena sin gluten", true).claims_gluten_free);
    }

    #[test]
    fn context_outside_eu_labelling() {
        assert_eq!(classify("natural flavors", false).class, NeedsOrigin);
        assert_eq!(classify("natural flavors", true).class, ContextDependent);
    }

    #[test]
    fn multilingual_terms() {
        for (text, expected) in [
            ("farine de ble", GlutenSource),
            ("farina di frumento", GlutenSource),
            ("weizenmehl", GlutenSource),
            ("wheat flour", GlutenSource),
            ("farinha de trigo", GlutenSource),
            ("orge", GlutenSource),
            ("seigle", GlutenSource),
            ("dinkelmehl", GlutenSource),
            ("flocons d avoine", Oats),
            ("haferflocken", Oats),
            ("riz", NaturallyGlutenFree),
            ("pommes de terre", NaturallyGlutenFree),
            ("huile de tournesol", NaturallyGlutenFree),
            ("sucre", NaturallyGlutenFree),
        ] {
            assert_eq!(class(text), expected, "{text}");
        }
    }

    #[test]
    fn misread_gluten_words_are_never_unrecognized() {
        for text in [
            "cebaba malteada",
            "harlna",
            "tr1go",
            "centen0",
            "espleta",
            "weizn",
        ] {
            let c = classify(text, true);
            assert!(
                severity(c.class) >= severity(NeedsOrigin),
                "{text}: {:?}",
                c.class
            );
        }
        // Real safe words are not mistaken for gluten words.
        for text in ["arena", "patatas", "maiz", "girasol", "dextrosa", "harissa"] {
            assert!(
                !looks_like_gluten_word(text) || class(text) != Unrecognized,
                "{text}"
            );
        }
        assert!(is_only_descriptors("corrector de acidez"));
        assert!(is_only_descriptors(
            "Agua mineral natural"
                .to_lowercase()
                .replace("agua", "")
                .as_str()
        ));
    }

    #[test]
    fn unknown_words_are_unrecognized_not_safe() {
        assert_eq!(class("xqzt"), Unrecognized);
        assert_eq!(class("patatas xqzt"), Unrecognized);
        // Unrecognized never hides a worse class.
        assert_eq!(class("harina xqzt"), NeedsOrigin);
        assert!(is_only_descriptors("antioxidante"));
        assert!(!is_only_descriptors("antioxidante xqzt"));
    }
}
