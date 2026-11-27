//! Shared SEO and crawler policy: the canonical origin, whether this instance
//! may be indexed, and the public URLs referenced by metadata.

/// Public product name used in titles, social tags, and structured data.
pub const SITE_NAME: &str = "Hex Ports";

/// Android Trusted Web Activity listing on Google Play.
pub const PLAY_STORE_URL: &str =
    "https://play.google.com/store/apps/details?id=org.playcatanou.twa";

/// Public source repository, used as an `Organization` `sameAs` link.
pub const REPO_URL: &str = "https://github.com/ukirdeomkar/Catan-Game";

/// Canonical public origin for absolute URLs (`robots.txt`, `/sitemap.xml`,
/// `rel=canonical`, and social images).
///
/// Staging runs the same binary on a different host. Absolute URLs always point
/// at production so a crawler that finds a preview build is sent to the real
/// site instead of a duplicate; moving to a real domain is one environment
/// change (`CATAN_PUBLIC_URL`) rather than an edit in several files.
pub fn public_base_url() -> String {
    std::env::var("CATAN_PUBLIC_URL")
        .ok()
        .map(|url| url.trim().trim_end_matches('/').to_string())
        .filter(|url| url.starts_with("http://") || url.starts_with("https://"))
        .unwrap_or_else(|| "https://hexports.duckdns.org".to_string())
}

/// Whether this instance must stay out of search indexes.
///
/// Staging sets `CATAN_NOINDEX` so the preview host cannot compete with prod as
/// duplicate content; production leaves it unset.
pub fn noindex_enabled() -> bool {
    noindex_from(std::env::var("CATAN_NOINDEX").ok().as_deref())
}

/// Parse the `CATAN_NOINDEX` value. Extracted so the accepted spellings are
/// testable without mutating the process environment.
pub fn noindex_from(raw: Option<&str>) -> bool {
    matches!(
        raw.map(|value| value.trim().to_ascii_lowercase()).as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}
