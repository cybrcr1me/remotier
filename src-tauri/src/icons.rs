//! App icons for hosts and groups, from the selfh.st catalog (<https://selfh.st/icons>).
//!
//! Everywhere else an icon key is opaque to the backend. `selfhst:<reference>` names one of
//! these, and syncs like any other key; the image itself does not. Each device fetches it
//! from jsDelivr the first time it is shown and keeps it in the app's cache directory, so it
//! still shows offline after that.
//!
//! This lives in Rust rather than the webview for the same reason sync does: the CSP allows
//! no remote origin and the capability set grants no http plugin, both deliberately. The
//! webview gets a `data:` URL, which its CSP does allow.
//!
//! The files are a few kilobytes, so they are read and written with `std::fs` directly.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use base64::Engine;
use serde::{Deserialize, Serialize};
use tauri::Manager;

use crate::error::{Error, Result};

const CDN: &str = "https://cdn.jsdelivr.net/gh/selfhst/icons";
const INDEX_FILE: &str = "index.json";

/// The longest reference in the catalog is 37 characters.
const MAX_REFERENCE_LEN: usize = 64;
/// The index is about 850KB.
const MAX_INDEX_BYTES: usize = 8 * 1024 * 1024;
/// An SVG is a few kilobytes; the largest PNGs run to a few hundred.
const MAX_IMAGE_BYTES: usize = 2 * 1024 * 1024;
/// A cached index older than this is refreshed - and still used if the refresh fails.
const INDEX_MAX_AGE: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Folder on the CDN and MIME type, in the order they are tried. About one icon in six has
/// no SVG, only raster images.
const FORMATS: [(&str, &str); 3] = [
    ("svg", "image/svg+xml"),
    ("png", "image/png"),
    ("webp", "image/webp"),
];

/// One icon in the catalog, as the picker needs it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CatalogEntry {
    pub name: String,
    pub reference: String,
    pub tags: Vec<String>,
}

/// A row of the catalog's own `index.json`. Only the fields the picker uses are read.
#[derive(Deserialize)]
struct IndexRow {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "Reference")]
    reference: String,
    #[serde(rename = "Tags", default)]
    tags: String,
}

/// True for a reference that is safe to put in a URL path and in a file name.
///
/// Every reference in the catalog fits `[a-z0-9._-]`. Holding to exactly that, with a
/// letter or digit first, means no reference can climb out of the cache directory or address
/// anything on the CDN but a file in an icon folder.
pub fn valid_reference(reference: &str) -> bool {
    let bytes = reference.as_bytes();
    let Some(first) = bytes.first() else {
        return false;
    };
    bytes.len() <= MAX_REFERENCE_LEN
        && (first.is_ascii_lowercase() || first.is_ascii_digit())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-'))
}

/// Parses the catalog's `index.json`, dropping any row whose reference is not safe to fetch.
/// Tags arrive as one comma-separated string.
pub fn parse_index(bytes: &[u8]) -> Result<Vec<CatalogEntry>> {
    let rows: Vec<IndexRow> = serde_json::from_slice(bytes)
        .map_err(|e| Error::Icon(format!("the icon catalog could not be read: {e}")))?;

    Ok(rows
        .into_iter()
        .filter(|row| valid_reference(&row.reference))
        .map(|row| CatalogEntry {
            name: row.name,
            reference: row.reference,
            tags: row
                .tags
                .split(',')
                .map(str::trim)
                .filter(|tag| !tag.is_empty())
                .map(String::from)
                .collect(),
        })
        .collect())
}

/// An image the webview can show without fetching anything.
pub fn data_url(mime: &str, bytes: &[u8]) -> String {
    let encoded = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime};base64,{encoded}")
}

/// Where icons are kept. The cache directory rather than app data: everything in it can be
/// fetched again, and none of it is the user's.
pub fn cache_dir(app: &tauri::AppHandle) -> Result<PathBuf> {
    let base = app
        .path()
        .app_cache_dir()
        .map_err(|e| Error::Internal(format!("no cache directory: {e}")))?;
    Ok(base.join("icons").join("selfhst"))
}

/// The whole catalog. Served from the cache while it is fresh, refreshed from the CDN when
/// it is not, and served stale when the CDN cannot be reached - a week-old catalog beats none.
pub async fn catalog(dir: &Path) -> Result<Vec<CatalogEntry>> {
    let path = dir.join(INDEX_FILE);
    if is_fresh(&path) {
        if let Some(entries) = read_index(&path) {
            return Ok(entries);
        }
    }

    let fetched = match fetch(&format!("{CDN}/{INDEX_FILE}"), MAX_INDEX_BYTES, "application/json").await
    {
        Ok(Some(bytes)) => parse_index(&bytes).map(|entries| (entries, bytes)),
        Ok(None) => Err(Error::Icon("the icon catalog is not on the CDN".into())),
        Err(e) => Err(e),
    };

    match fetched {
        Ok((entries, bytes)) => {
            // A cache that cannot be written costs a download next time, nothing more.
            if let Err(e) = write_atomically(&path, &bytes) {
                log::warn!("could not cache the icon catalog: {e}");
            }
            Ok(entries)
        }
        Err(e) => read_index(&path).ok_or(e),
    }
}

/// One icon as a `data:` URL: from the cache when it is there, otherwise from the CDN,
/// cached on the way through.
pub async fn image(dir: &Path, reference: &str) -> Result<String> {
    if !valid_reference(reference) {
        return Err(Error::Invalid(format!("not an icon reference: {reference:?}")));
    }

    if let Some(url) = cached_image(dir, reference) {
        return Ok(url);
    }

    for (format, mime) in FORMATS {
        let url = format!("{CDN}/{format}/{reference}.{format}");
        let Some(bytes) = fetch(&url, MAX_IMAGE_BYTES, "image/").await? else {
            continue;
        };
        if let Err(e) = write_atomically(&dir.join(format!("{reference}.{format}")), &bytes) {
            log::warn!("could not cache the {reference} icon: {e}");
        }
        return Ok(data_url(mime, &bytes));
    }

    Err(Error::NotFound("icon", reference.to_string()))
}

fn cached_image(dir: &Path, reference: &str) -> Option<String> {
    FORMATS.iter().find_map(|(format, mime)| {
        let bytes = std::fs::read(dir.join(format!("{reference}.{format}"))).ok()?;
        Some(data_url(mime, &bytes))
    })
}

fn read_index(path: &Path) -> Option<Vec<CatalogEntry>> {
    std::fs::read(path).ok().and_then(|bytes| parse_index(&bytes).ok())
}

fn is_fresh(path: &Path) -> bool {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| SystemTime::now().duration_since(modified).ok())
        .is_some_and(|age| age < INDEX_MAX_AGE)
}

/// Written beside the target and renamed over it, so a concurrent read never sees half a file.
fn write_atomically(path: &Path, bytes: &[u8]) -> Result<()> {
    let dir = path
        .parent()
        .ok_or_else(|| Error::Internal(format!("{} has no parent", path.display())))?;
    std::fs::create_dir_all(dir)?;

    // A leading dot keeps it clear of every name a valid reference can produce.
    let temp = dir.join(format!(".{}.tmp", uuid::Uuid::new_v4()));
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&temp);
    })?;
    Ok(())
}

/// GETs `url`, or `None` when the CDN has no such file. Any other failure status, a body that is not
/// `expected_type`, or one over `max` bytes is an error.
///
/// The type check is what stops a captive portal's login page, served with a 200, being
/// cached as an icon for good.
async fn fetch(url: &str, max: usize, expected_type: &str) -> Result<Option<Vec<u8>>> {
    let mut response = client()?.get(url).send().await.map_err(transport)?;

    // jsDelivr answers 403, not 404, for a file its GitHub mirror does not have - which is
    // exactly how an icon with no SVG looks. Treating it as fatal would stop at the SVG.
    let status = response.status();
    if matches!(status, reqwest::StatusCode::NOT_FOUND | reqwest::StatusCode::FORBIDDEN) {
        return Ok(None);
    }
    if !status.is_success() {
        return Err(Error::Icon(format!("cdn.jsdelivr.net answered {status}")));
    }

    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    if !content_type.starts_with(expected_type) {
        return Err(Error::Icon(format!(
            "cdn.jsdelivr.net sent {content_type:?} where {expected_type} was expected"
        )));
    }

    let too_large = || Error::Icon(format!("refused a download over {max} bytes"));
    if response.content_length().is_some_and(|length| length > max as u64) {
        return Err(too_large());
    }

    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(transport)? {
        if body.len() + chunk.len() > max {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Some(body))
}

/// Phrased here rather than through `From<reqwest::Error>`, which describes every transport
/// failure as the sync server's.
fn transport(e: reqwest::Error) -> Error {
    log::debug!("icon transport error: {e}");
    if e.is_timeout() {
        Error::Icon("cdn.jsdelivr.net did not answer in time".into())
    } else if e.is_connect() {
        Error::Icon("could not reach cdn.jsdelivr.net".into())
    } else {
        Error::Icon(e.to_string())
    }
}

fn client() -> Result<&'static reqwest::Client> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }

    crate::sync::client::install_crypto_provider();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .connect_timeout(Duration::from_secs(10))
        .user_agent(concat!("remotier/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error::Icon(format!("could not build the http client: {e}")))?;
    Ok(CLIENT.get_or_init(|| client))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("remotier-icons-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn accepts_every_shape_of_reference_the_catalog_uses() {
        for reference in ["portainer", "home-assistant", "1password", "node-red", "a.b_c"] {
            assert!(valid_reference(reference), "{reference}");
        }
    }

    #[test]
    fn refuses_a_reference_that_could_leave_the_icon_folder() {
        let long = "a".repeat(MAX_REFERENCE_LEN + 1);
        for reference in [
            "", ".", "..", ".hidden", "../index", "a/b", "a\\b", "Portainer", "a b", "%2e%2e",
            "-flag", &long,
        ] {
            assert!(!valid_reference(reference), "{reference:?}");
        }
    }

    #[test]
    fn reads_the_catalogs_own_index_format() {
        let index = br#"[
            {"Name": "Uptime Kuma", "Reference": "uptime-kuma", "SVG": "Yes", "PNG": "Yes",
             "WebP": "Yes", "Light": "No", "Dark": "No", "Category": "Self-Hosted",
             "Tags": "Monitoring, Status Pages", "CreatedAt": "2024-08-16 00:27:23+00:00"},
            {"Name": "Portainer", "Reference": "portainer", "Tags": ""}
        ]"#;

        assert_eq!(
            parse_index(index).unwrap(),
            vec![
                CatalogEntry {
                    name: "Uptime Kuma".into(),
                    reference: "uptime-kuma".into(),
                    tags: vec!["Monitoring".into(), "Status Pages".into()],
                },
                CatalogEntry {
                    name: "Portainer".into(),
                    reference: "portainer".into(),
                    tags: vec![],
                },
            ]
        );
    }

    #[test]
    fn drops_an_index_row_whose_reference_is_unsafe() {
        let index = br#"[{"Name": "Evil", "Reference": "../../escape"}, {"Name": "Ok", "Reference": "ok"}]"#;
        let references: Vec<_> = parse_index(index)
            .unwrap()
            .into_iter()
            .map(|entry| entry.reference)
            .collect();
        assert_eq!(references, ["ok"]);
    }

    #[test]
    fn encodes_an_image_as_a_data_url() {
        assert_eq!(
            data_url("image/svg+xml", b"<svg/>"),
            "data:image/svg+xml;base64,PHN2Zy8+"
        );
    }

    #[tokio::test]
    async fn serves_a_cached_icon_without_the_network() {
        let dir = temp_dir();
        std::fs::write(dir.join("portainer.png"), b"png").unwrap();
        std::fs::write(dir.join("portainer.svg"), b"<svg/>").unwrap();

        // SVG wins when both are cached, the same order they are fetched in. A cache miss
        // would fetch the real icon and fail the comparison.
        assert_eq!(
            image(&dir, "portainer").await.unwrap(),
            data_url("image/svg+xml", b"<svg/>")
        );

        std::fs::remove_dir_all(dir).unwrap();
    }

    #[tokio::test]
    async fn refuses_an_unsafe_reference_before_touching_anything() {
        let dir = temp_dir();
        let error = image(&dir, "../secret").await.unwrap_err();
        assert_eq!(error.kind(), "invalid");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn writes_the_cache_atomically_into_a_folder_it_creates() {
        let root = temp_dir();
        let dir = root.join("nested");
        write_atomically(&dir.join("x.svg"), b"<svg/>").unwrap();

        assert_eq!(std::fs::read(dir.join("x.svg")).unwrap(), b"<svg/>");
        // No temporary file left behind.
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);

        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn a_just_written_index_is_fresh_and_a_missing_one_is_not() {
        let dir = temp_dir();
        let path = dir.join(INDEX_FILE);

        assert!(!is_fresh(&path));
        std::fs::write(&path, b"[]").unwrap();
        assert!(is_fresh(&path));

        std::fs::remove_dir_all(dir).unwrap();
    }
}
