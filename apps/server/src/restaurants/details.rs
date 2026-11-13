//! Extra facts for a restaurant's page: its kind, its cuisine and a photo.
//!
//! Photos come only from Wikimedia Commons, named by the venue's own OpenStreetMap
//! tags (`wikimedia_commons`, or an `image` URL that points to Commons). Commons files
//! carry a free licence and a page with the author and the licence, which the app links
//! to. Any other `image` URL is ignored: its licence is unknown.

use safeeat_contracts::RestaurantPhoto;
use url::Url;

/// Longest file name accepted (Commons allows 240 bytes).
const MAX_NAME: usize = 240;
/// Width of the thumbnail Commons scales for the app.
const THUMB_WIDTH: u32 = 640;
const EXTENSIONS: &[&str] = &[".jpg", ".jpeg", ".png", ".webp"];

fn percent_decode(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = value.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// The Commons file named by a `wikimedia_commons` or `image` tag, without `File:`,
/// with underscores for spaces. `None` for anything that is not a Commons image.
pub fn commons_file(tag: &str) -> Option<String> {
    let tag = tag.trim();
    let name = if let Some(rest) = tag
        .strip_prefix("File:")
        .or_else(|| tag.strip_prefix("Image:"))
    {
        rest.to_string()
    } else {
        let url = Url::parse(tag).ok()?;
        if url.scheme() != "https" {
            return None;
        }
        let segments: Vec<&str> = url.path_segments()?.collect();
        match url.host_str()? {
            // https://commons.wikimedia.org/wiki/File:Name.jpg
            "commons.wikimedia.org" => {
                let page = percent_decode(segments.get(1).filter(|_| segments[0] == "wiki")?)?;
                page.strip_prefix("File:")?.to_string()
            }
            // https://upload.wikimedia.org/wikipedia/commons/a/ab/Name.jpg
            // https://upload.wikimedia.org/wikipedia/commons/thumb/a/ab/Name.jpg/640px-Name.jpg
            "upload.wikimedia.org" => {
                if segments.first() != Some(&"wikipedia") || segments.get(1) != Some(&"commons") {
                    return None;
                }
                let at = if segments.get(2) == Some(&"thumb") {
                    5
                } else {
                    4
                };
                percent_decode(segments.get(at)?)?
            }
            _ => return None,
        }
    };
    let name = name.trim().replace(' ', "_");
    let lower = name.to_lowercase();
    let valid = !name.is_empty()
        && name.len() <= MAX_NAME
        && !name.contains(['/', '#', '?', '|', '<', '>', '[', ']', '{', '}'])
        && EXTENSIONS.iter().any(|e| lower.ends_with(e));
    valid.then_some(name)
}

/// The photo shown for a Commons file: a scaled thumbnail and the file's page (author
/// and licence).
pub fn photo(file: &str) -> Option<RestaurantPhoto> {
    let mut thumb = Url::parse("https://commons.wikimedia.org/wiki/Special:FilePath/").ok()?;
    thumb.path_segments_mut().ok()?.pop_if_empty().push(file);
    thumb
        .query_pairs_mut()
        .append_pair("width", &THUMB_WIDTH.to_string());
    let mut page = Url::parse("https://commons.wikimedia.org/wiki/").ok()?;
    page.path_segments_mut()
        .ok()?
        .pop_if_empty()
        .push(&format!("File:{file}"));
    Some(RestaurantPhoto {
        url: thumb.into(),
        page_url: page.into(),
        credit: "Wikimedia Commons".into(),
    })
}

/// Photo from a venue's tags: `wikimedia_commons` first, then a Commons `image`.
pub fn photo_file(wikimedia_commons: Option<&str>, image: Option<&str>) -> Option<String> {
    wikimedia_commons
        .and_then(commons_file)
        .or_else(|| image.and_then(commons_file))
}

/// First cuisine of an OSM `cuisine` value (`pizza;italian` → `pizza`), bounded.
pub fn cuisine(tag: Option<&str>) -> Option<String> {
    let first = tag?.split(';').next()?.trim();
    (!first.is_empty()
        && first.len() <= 40
        && first
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == ' '))
    .then(|| first.to_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_commons_images_are_accepted() {
        assert_eq!(
            commons_file("File:Bar Manolo.jpg").as_deref(),
            Some("Bar_Manolo.jpg")
        );
        assert_eq!(
            commons_file("https://commons.wikimedia.org/wiki/File:Caf%C3%A9_Sol.JPG").as_deref(),
            Some("Café_Sol.JPG")
        );
        assert_eq!(
            commons_file("https://upload.wikimedia.org/wikipedia/commons/a/ab/Plaza.png")
                .as_deref(),
            Some("Plaza.png")
        );
        assert_eq!(
            commons_file(
                "https://upload.wikimedia.org/wikipedia/commons/thumb/a/ab/Plaza.png/640px-Plaza.png"
            )
            .as_deref(),
            Some("Plaza.png")
        );
        // Unknown licence, not an image, other wikis, plain http, odd names.
        for bad in [
            "https://example.com/photo.jpg",
            "https://www.mapillary.com/map/im/123",
            "http://commons.wikimedia.org/wiki/File:X.jpg",
            "https://upload.wikimedia.org/wikipedia/en/a/ab/X.jpg",
            "File:Document.pdf",
            "File:",
            "Category:Restaurants",
            "File:a/b.jpg",
        ] {
            assert_eq!(commons_file(bad), None, "{bad}");
        }
        assert_eq!(commons_file(&format!("File:{}.jpg", "x".repeat(300))), None);
    }

    #[test]
    fn the_photo_links_to_its_licence_page() {
        let p = photo("Café_Sol.jpg").unwrap();
        assert_eq!(
            p.url,
            "https://commons.wikimedia.org/wiki/Special:FilePath/Caf%C3%A9_Sol.jpg?width=640"
        );
        assert_eq!(
            p.page_url,
            "https://commons.wikimedia.org/wiki/File:Caf%C3%A9_Sol.jpg"
        );
        assert_eq!(
            photo_file(Some("Category:X"), Some("File:Y.jpg")).as_deref(),
            Some("Y.jpg")
        );
        assert_eq!(photo_file(None, Some("https://example.com/y.jpg")), None);
    }

    #[test]
    fn cuisine_keeps_the_first_value() {
        assert_eq!(cuisine(Some("pizza;italian")).as_deref(), Some("pizza"));
        assert_eq!(cuisine(Some(" Spanish ")).as_deref(), Some("spanish"));
        assert_eq!(cuisine(Some("<script>")), None);
        assert_eq!(cuisine(Some("")), None);
        assert_eq!(cuisine(None), None);
    }
}
