//! The selfh.st icon catalog. See `crate::icons` for how fetching and caching work.

use tauri::AppHandle;

use crate::error::Result;
use crate::icons::{self, CatalogEntry};

/// The whole catalog, for the picker to search.
#[tauri::command]
pub async fn icon_catalog(app: AppHandle) -> Result<Vec<CatalogEntry>> {
    icons::catalog(&icons::cache_dir(&app)?).await
}

/// One icon as a `data:` URL, fetched on first use and served from the cache after.
#[tauri::command]
pub async fn icon_image(app: AppHandle, reference: String) -> Result<String> {
    icons::image(&icons::cache_dir(&app)?, &reference).await
}
