use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axum::{Extension, Json, Router, extract::Query, http::StatusCode, routing::get};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as AsyncMutex;
use url::Url;

use super::{AppStateInner, HttpHandleError, authed_user_id, other_error, users::AuthSession};

const DEFAULT_TILE_URL: &str = "https://tile.openstreetmap.org/{z}/{x}/{y}.png";
const DEFAULT_ATTRIBUTION_TEXT: &str = "© OpenStreetMap contributors";
const DEFAULT_ATTRIBUTION_URL: &str = "https://www.openstreetmap.org/copyright";
const DEFAULT_GEOCODING_URL: &str = "https://photon.komoot.io/reverse";
const REQUEST_INTERVAL: Duration = Duration::from_secs(1);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(8);
const CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);
const CACHE_CAPACITY: usize = 512;
const MAX_RESPONSE_BYTES: usize = 256 * 1024;

#[derive(Clone, Serialize)]
struct MapConfig {
    tile_url: String,
    attribution_text: String,
    attribution_url: String,
    max_zoom: u8,
    reverse_enabled: bool,
}

impl Default for MapConfig {
    fn default() -> Self {
        Self {
            tile_url: DEFAULT_TILE_URL.into(),
            attribution_text: DEFAULT_ATTRIBUTION_TEXT.into(),
            attribution_url: DEFAULT_ATTRIBUTION_URL.into(),
            max_zoom: 19,
            reverse_enabled: false,
        }
    }
}

impl MapConfig {
    fn from_env() -> Self {
        let mut config = Self::default();
        let Some(tile_url) = configured_value("EASYTIER_NODE_MAP_TILE_URL") else {
            return config;
        };
        if tile_url == DEFAULT_TILE_URL {
            return config;
        }
        let attribution_text = configured_value("EASYTIER_NODE_MAP_ATTRIBUTION_TEXT");
        let attribution_url = configured_value("EASYTIER_NODE_MAP_ATTRIBUTION_URL");
        // Keep the template as supplied; URL parsing encodes the Leaflet braces.
        let sample_url = tile_url
            .replace("{z}", "1")
            .replace("{x}", "0")
            .replace("{y}", "0")
            .replace("{s}", "a")
            .replace("{r}", "");
        let valid_template = ["{z}", "{x}", "{y}"]
            .iter()
            .all(|placeholder| tile_url.contains(placeholder))
            && !sample_url.contains('{')
            && !sample_url.contains('}')
            && parse_http_url(&sample_url).is_some();
        if let (true, Some(text), Some(url)) =
            (valid_template, attribution_text, attribution_url)
        {
            if text.chars().count() <= 512 && parse_http_url(&url).is_some() {
                config.tile_url = tile_url;
                config.attribution_text = text;
                config.attribution_url = url;
                return config;
            }
        }
        // Do not accidentally serve a custom provider with OSM-only attribution.
        tracing::warn!(
            "Invalid node map configuration; using the default OpenStreetMap tiles and attribution"
        );
        config
    }
}

fn configured_value(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

fn parse_http_url(value: &str) -> Option<Url> {
    let url = Url::parse(value).ok()?;
    (matches!(url.scheme(), "http" | "https")
        && url.host_str().is_some()
        && url.username().is_empty()
        && url.password().is_none()
        && url.fragment().is_none())
    .then_some(url)
}

#[derive(Clone, Copy, Default, Deserialize, Eq, Hash, PartialEq)]
#[serde(rename_all = "lowercase")]
enum Language {
    #[default]
    Local,
    En,
    De,
    Fr,
}

impl Language {
    fn photon_parameter(self) -> Option<&'static str> {
        match self {
            Self::Local => None,
            Self::En => Some("en"),
            Self::De => Some("de"),
            Self::Fr => Some("fr"),
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReverseQuery {
    latitude: f64,
    longitude: f64,
    #[serde(default)]
    language: Language,
}

#[derive(Clone, Serialize)]
struct Address {
    country: Option<String>,
    region: Option<String>,
    city: Option<String>,
    district: Option<String>,
}

#[derive(Deserialize)]
struct PhotonResponse {
    features: Vec<PhotonFeature>,
}

#[derive(Deserialize)]
struct PhotonFeature {
    properties: serde_json::Map<String, serde_json::Value>,
}

impl PhotonFeature {
    fn text(&self, key: &str) -> Option<String> {
        self.properties
            .get(key)
            .and_then(serde_json::Value::as_str)
            .map(|value| value.chars().filter(|character| !character.is_control()).collect::<String>())
            .map(|value| value.trim().chars().take(128).collect::<String>())
            .filter(|value| !value.is_empty())
    }

    fn address(&self) -> Option<Address> {
        let kind = self.properties.get("type").and_then(serde_json::Value::as_str);
        let osm_key = self.properties.get("osm_key").and_then(serde_json::Value::as_str);
        let osm_value = self.properties.get("osm_value").and_then(serde_json::Value::as_str);
        let is_place = |values: &[&str]| {
            kind.is_some_and(|value| values.contains(&value))
                || (osm_key == Some("place")
                    && osm_value.is_some_and(|value| values.contains(&value)))
        };
        let name_for = |values: &[&str]| is_place(values).then(|| self.text("name")).flatten();
        let address = Address {
            country: self.text("country").or_else(|| name_for(&["country"])),
            region: self.text("state").or_else(|| name_for(&["state", "province"])),
            city: self.text("city").or_else(|| name_for(&["city", "town", "village"])),
            district: self
                .text("district")
                .or_else(|| self.text("county"))
                .or_else(|| name_for(&["district", "county", "suburb"])),
        };
        (address.country.is_some()
            || address.region.is_some()
            || address.city.is_some()
            || address.district.is_some())
        .then_some(address)
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
struct CacheKey {
    latitude: i64,
    longitude: i64,
    language: Language,
}

struct CachedAddress {
    stored_at: Instant,
    address: Address,
}

struct GeocodingService {
    map_config: MapConfig,
    endpoint: Option<Url>,
    client: Option<reqwest::Client>,
    // Held for the entire upstream request, but never for node attribute writes.
    request_cooldown: AsyncMutex<Option<(Instant, Duration)>>,
    cache: Mutex<HashMap<CacheKey, CachedAddress>>,
}

impl GeocodingService {
    fn from_env() -> Self {
        let mut map_config = MapConfig::from_env();
        // An explicitly empty value disables reverse lookup. An invalid value
        // must not redirect node coordinates to an unintended fallback service.
        let endpoint_value = match std::env::var("EASYTIER_NODE_GEOCODING_URL") {
            Ok(value) => value.trim().to_owned(),
            Err(std::env::VarError::NotPresent) => DEFAULT_GEOCODING_URL.into(),
            Err(_) => String::new(),
        };
        let mut endpoint = parse_http_url(&endpoint_value);
        if !endpoint_value.is_empty() && endpoint.is_none() {
            tracing::warn!("Invalid node geocoding configuration; reverse lookup is disabled");
        }
        if let Some(url) = endpoint.as_mut() {
            // Server-owned query options may contain provider credentials, but
            // coordinates/language/result limits always come from this handler.
            let options: Vec<(String, String)> = url
                .query_pairs()
                .filter(|(name, _)| !matches!(name.as_ref(), "lat" | "lon" | "lang" | "limit"))
                .map(|(name, value)| (name.into_owned(), value.into_owned()))
                .collect();
            url.set_query(None);
            if !options.is_empty() {
                url.query_pairs_mut().extend_pairs(options);
            }
        }
        let client = if endpoint.is_some() {
            easytier::utils::init_crypto_provider();
            match reqwest::Client::builder()
                .user_agent(concat!(
                    "EasyTier-Web/",
                    env!("CARGO_PKG_VERSION"),
                    " (node-location; https://github.com/EasyTier/EasyTier)"
                ))
                .timeout(REQUEST_TIMEOUT)
                .connect_timeout(Duration::from_secs(4))
                .redirect(reqwest::redirect::Policy::none())
                .retry(reqwest::retry::never())
                .build()
            {
                Ok(client) => Some(client),
                Err(_) => {
                    tracing::warn!("Node geocoding HTTP client is unavailable; reverse lookup is disabled");
                    None
                }
            }
        } else {
            None
        };
        map_config.reverse_enabled = endpoint.is_some() && client.is_some();
        Self {
            map_config,
            endpoint,
            client,
            request_cooldown: AsyncMutex::new(None),
            cache: Mutex::new(HashMap::new()),
        }
    }

    async fn reverse(&self, query: ReverseQuery) -> Result<Address, HttpHandleError> {
        if !query.latitude.is_finite()
            || !query.longitude.is_finite()
            || !(-90.0..=90.0).contains(&query.latitude)
            || !(-180.0..=180.0).contains(&query.longitude)
        {
            return Err(api_error(
                StatusCode::BAD_REQUEST,
                "invalid_coordinates",
                "Latitude must be between -90 and 90 and longitude between -180 and 180",
            ));
        }
        let (Some(endpoint), Some(client)) = (&self.endpoint, &self.client) else {
            return Err(api_error(
                StatusCode::SERVICE_UNAVAILABLE,
                "geocoding_disabled",
                "Address lookup is disabled; enter the location manually",
            ));
        };
        let key = CacheKey {
            latitude: (query.latitude * 1_000_000.0).round() as i64,
            longitude: (query.longitude * 1_000_000.0).round() as i64,
            language: query.language,
        };
        if let Some(address) = self.cached_address(&key) {
            return Ok(address);
        }
        // Never queue unbounded interactive requests behind a slow public server.
        let mut cooldown = self.request_cooldown.try_lock().map_err(|_| busy_error())?;
        if cooldown.is_some_and(|(started_at, delay)| started_at.elapsed() < delay) {
            return Err(busy_error());
        }
        *cooldown = Some((Instant::now(), REQUEST_INTERVAL));
        let mut url = endpoint.clone();
        {
            let mut params = url.query_pairs_mut();
            params.append_pair("lat", &query.latitude.to_string());
            params.append_pair("lon", &query.longitude.to_string());
            params.append_pair("limit", "1");
            if let Some(language) = query.language.photon_parameter() {
                params.append_pair("lang", language);
            }
        }
        let mut response = client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .await
            .map_err(transport_error)?;
        if response.status() == reqwest::StatusCode::TOO_MANY_REQUESTS {
            // Respect public-server throttling without retrying automatically.
            let cooldown_seconds = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .map(str::trim)
                .filter(|value| !value.is_empty() && value.bytes().all(|byte| byte.is_ascii_digit()))
                .map(|value| value.parse::<u64>().unwrap_or(u64::MAX))
                .unwrap_or(30)
                .max(30);
            // Compare elapsed time rather than adding to Instant: even an
            // extreme Retry-After safely prevents further upstream requests.
            *cooldown = Some((Instant::now(), Duration::from_secs(cooldown_seconds)));
            return Err(busy_error());
        }
        if !response.status().is_success() {
            return Err(upstream_error());
        }
        if response.content_length().is_some_and(|length| length > MAX_RESPONSE_BYTES as u64) {
            return Err(upstream_error());
        }
        let mut body = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(transport_error)? {
            if chunk.len() > MAX_RESPONSE_BYTES.saturating_sub(body.len()) {
                return Err(upstream_error());
            }
            body.extend_from_slice(&chunk);
        }
        let result: PhotonResponse = serde_json::from_slice(&body).map_err(|_| upstream_error())?;
        let address = result.features.first().and_then(PhotonFeature::address).ok_or_else(|| {
            api_error(
                StatusCode::NOT_FOUND,
                "geocoding_no_result",
                "No address was found for this point; enter the location manually",
            )
        })?;
        self.cache_address(key, address.clone());
        Ok(address)
    }

    fn cached_address(&self, key: &CacheKey) -> Option<Address> {
        let mut cache = self.cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.retain(|_, entry| entry.stored_at.elapsed() < CACHE_TTL);
        cache.get(key).map(|entry| entry.address.clone())
    }

    fn cache_address(&self, key: CacheKey, address: Address) {
        let mut cache = self.cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        cache.retain(|_, entry| entry.stored_at.elapsed() < CACHE_TTL);
        if cache.len() >= CACHE_CAPACITY {
            if let Some(oldest) = cache
                .iter()
                .min_by_key(|(_, entry)| entry.stored_at)
                .map(|(key, _)| *key)
            {
                cache.remove(&oldest);
            }
        }
        cache.insert(key, CachedAddress { stored_at: Instant::now(), address });
    }
}

fn api_error(status: StatusCode, code: &str, message: &str) -> HttpHandleError {
    let mut error = other_error(message);
    error.code = Some(code.into());
    (status, Json(error))
}

fn busy_error() -> HttpHandleError {
    api_error(
        StatusCode::TOO_MANY_REQUESTS,
        "geocoding_busy",
        "Address lookup is busy; retry shortly or enter the location manually",
    )
}

fn upstream_error() -> HttpHandleError {
    api_error(
        StatusCode::BAD_GATEWAY,
        "geocoding_unavailable",
        "Address lookup is temporarily unavailable; enter the location manually",
    )
}

fn transport_error(error: reqwest::Error) -> HttpHandleError {
    // Error Display may contain the requested coordinates or upstream credentials.
    if error.is_timeout() {
        api_error(
            StatusCode::GATEWAY_TIMEOUT,
            "geocoding_timeout",
            "Address lookup timed out; retry or enter the location manually",
        )
    } else {
        upstream_error()
    }
}

async fn get_config(
    auth_session: AuthSession,
    Extension(service): Extension<Arc<GeocodingService>>,
) -> Result<Json<MapConfig>, HttpHandleError> {
    authed_user_id(&auth_session)?;
    Ok(Json(service.map_config.clone()))
}

async fn reverse(
    auth_session: AuthSession,
    Extension(service): Extension<Arc<GeocodingService>>,
    Query(query): Query<ReverseQuery>,
) -> Result<Json<Address>, HttpHandleError> {
    authed_user_id(&auth_session)?;
    service.reverse(query).await.map(Json)
}

pub(super) fn router() -> Router<AppStateInner> {
    Router::new()
        .route("/api/v1/geocoding/config", get(get_config))
        .route("/api/v1/geocoding/reverse", get(reverse))
        .layer(Extension(Arc::new(GeocodingService::from_env())))
}
