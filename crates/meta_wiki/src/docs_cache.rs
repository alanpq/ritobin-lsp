use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use dashmap::DashMap;
use reqwest::header::{CACHE_CONTROL, HeaderMap};

use crate::client::{
    Client, Error,
    types::{ClassDocs, Error as ApiError, GetDocsNameOrHash},
};

#[derive(Clone)]
pub struct WikiDocs {
    client: Client,
    cache: Arc<DashMap<String, (Arc<ClassDocs>, Instant)>>,
}

impl WikiDocs {
    pub fn new(baseurl: &str) -> Self {
        Self {
            client: Client::new(baseurl),
            cache: Default::default(),
        }
    }

    pub async fn get_docs(
        &self,
        key: &GetDocsNameOrHash,
    ) -> Result<Arc<ClassDocs>, Error<ApiError>> {
        let cache_key = key.to_string();
        if let Some(entry) = self.cache.get(&cache_key)
            && entry.1 > Instant::now()
        {
            return Ok(entry.0.clone());
        }

        let res = self.client.get_docs(key).await?;
        let max_age = max_age(res.headers());
        let docs = Arc::new(res.into_inner());

        if let Some(max_age) = max_age {
            self.cache
                .insert(cache_key, (docs.clone(), Instant::now() + max_age));
        }

        Ok(docs)
    }
}

fn max_age(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(CACHE_CONTROL)?.to_str().ok()?;
    let mut directives = value.split(',').map(str::trim);

    if directives
        .clone()
        .any(|d| d.eq_ignore_ascii_case("no-store") || d.eq_ignore_ascii_case("no-cache"))
    {
        return None;
    }

    directives
        .find_map(|d| d.strip_prefix("max-age="))
        .and_then(|secs| secs.parse().ok())
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(cache_control: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(CACHE_CONTROL, cache_control.parse().unwrap());
        headers
    }

    #[test]
    fn no_header_means_no_cache() {
        assert_eq!(max_age(&HeaderMap::new()), None);
    }

    #[test]
    fn parses_max_age() {
        assert_eq!(
            max_age(&headers("public, max-age=300")),
            Some(Duration::from_secs(300))
        );
    }

    #[test]
    fn no_store_overrides_max_age() {
        assert_eq!(max_age(&headers("max-age=300, no-store")), None);
    }

    #[test]
    fn no_cache_overrides_max_age() {
        assert_eq!(max_age(&headers("no-cache, max-age=300")), None);
    }
}
