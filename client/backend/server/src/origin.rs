//! The absolute origin this deploy is served on, and the one place it is decided.
//!
//! One image is built and deployed to both staging and production — `ci.yaml`
//! pushes `:latest` and `:<sha>` to a single Artifact Registry repository, and
//! both releases pull the same tag. The build therefore has no origin to bake
//! in, which is why the served shell carries [`ORIGIN_PLACEHOLDER`] where an
//! absolute URL is needed and this module fills it in when the server reads
//! `index.html` at startup.
//!
//! That matters most for `<link rel="canonical">`, and the reason is not
//! cosmetic. `rel=canonical` is a **per-URL assertion** about where the page
//! you are looking at is canonically located — it is not a hint that
//! consolidates several hosts onto one preferred copy. Served on staging, a
//! canonical naming production asserts that staging is a duplicate of
//! production: a consolidation signal pointing the wrong way for any future
//! decision to surface staging, and inert on production. One artifact, two
//! hosts, so each has to answer with its own origin.
//!
//! Staging is `noindex, nofollow` via `X-Robots-Tag` regardless (see
//! [`crate::seo`]), so this was never a leak — it is a correctness fix, and one
//! that stops mattering the moment anyone decides to surface staging.
//!
//! ## One mapping, two consumers
//!
//! [`Mailer`](crate::mailer::Mailer) builds the absolute links in outbound
//! email from the same origin, and it did so first. Rather than let a second
//! copy of the host table grow next to it — the exact drift this module exists
//! to prevent — the mailer calls [`from_env`] too. A new environment is one
//! line here, not one per consumer.

/// The token the build writes where the served document needs an absolute
/// origin, and this module replaces with the deploy's own.
///
/// Deliberately not a `sed` placeholder like `__BUNDLE_HASH__`: that one is a
/// *build* fact (the content hash is known while the bundle is being written),
/// while this one is a *deploy* fact. The image is the same bytes on both
/// hosts, so the substitution has to happen after the image is built — which is
/// what reading the document at startup does.
pub const ORIGIN_PLACEHOLDER: &str = "__ORIGIN__";

/// `APP_ORIGIN` — an explicit override of the origin derived from `APP_ENV`.
pub const APP_ORIGIN: &str = "APP_ORIGIN";

/// The default origin for each deploy environment.
///
/// `local` is the `PORT`-default `3001` server, matching what the mailer has
/// always used; nothing crawls a localhost document, so it is here to keep the
/// function total rather than to be correct about a public host.
fn default_for(app_env: &str) -> &'static str {
    match app_env {
        "production" => "https://crosswords.casazza.io",
        "staging" => "https://crosswords-staging.casazza.io",
        _ => "http://localhost:3001",
    }
}

/// The origin for `app_env`: `APP_ORIGIN` if set, otherwise the environment's
/// own host.
///
/// Trailing slashes are stripped so callers can append a rooted path without
/// producing a doubled separator. `mail.origin` in the chart is operator-set
/// and documented as `https://crosswords-staging.casazza.io`, but nothing
/// enforced that shape, and `https://host/` would otherwise render as
/// `https://host//auth/verify-email` — a link that 404s at the one moment a
/// user is relying on it.
pub fn for_env(app_env: &str) -> String {
    let raw = std::env::var(APP_ORIGIN).unwrap_or_else(|_| default_for(app_env).to_string());
    raw.trim_end_matches('/').to_string()
}

/// [`for_env`], from the `APP_ENV` this process was started with.
///
/// `APP_ENV` defaults to `production`, matching `main.rs` and `seo.rs`: if the
/// variable is ever missing, the closed, most-locked-down answer is the right
/// one to fall back to.
pub fn from_env() -> String {
    let app_env = std::env::var("APP_ENV").unwrap_or_else(|_| "production".into());
    for_env(&app_env)
}

/// Substitute [`ORIGIN_PLACEHOLDER`] in the served shell with `origin`.
///
/// Returns the document unchanged if the placeholder is absent, so a server
/// running against a dist built before this existed keeps serving the old
/// bytes rather than panicking. That is the right degradation: the old document
/// is the one this change is fixing, but it is still a valid document, and a
/// hard failure here would take down every client-side route on a host whose
/// only problem is a stale index.html.
pub fn substitute(shell: &str, origin: &str) -> String {
    shell.replace(ORIGIN_PLACEHOLDER, origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole defect in one assertion: two hosts, one artifact, and the
    /// canonical each serves has to be its own.
    #[test]
    fn each_environment_names_its_own_host() {
        assert_eq!(default_for("production"), "https://crosswords.casazza.io");
        assert_eq!(
            default_for("staging"),
            "https://crosswords-staging.casazza.io",
        );
        // Anything unrecognised is treated as local, which is the pre-announcement,
        // least-reachable answer — never production.
        assert_eq!(default_for("local"), "http://localhost:3001");
        assert_eq!(default_for("prodution"), "http://localhost:3001");
    }

    /// `rel=canonical` is per-URL. Served on staging it must not claim
    /// production; served on production it must still claim production.
    #[test]
    fn a_canonical_is_the_serving_host_not_the_preferred_one() {
        let shell = "<link rel=\"canonical\" href=\"__ORIGIN__/\" />";
        let staging = substitute(shell, default_for("staging"));
        assert!(staging.contains("href=\"https://crosswords-staging.casazza.io/\""));
        assert!(
            !staging.contains("crosswords.casazza.io"),
            "staging claimed production"
        );

        let production = substitute(shell, default_for("production"));
        assert!(production.contains("href=\"https://crosswords.casazza.io/\""));
    }

    /// A host is not a prefix of its own staging sibling in a way that matters
    /// here: the assertion above greps for `crosswords.casazza.io`, which is a
    /// substring of `crosswords-staging.casazza.io`. Pin that so a future edit
    /// to the hosts cannot quietly make that test vacuous.
    #[test]
    fn the_production_host_is_not_a_substring_of_the_staging_one() {
        assert!(default_for("staging").contains("casazza.io"));
        assert!(!default_for("staging").contains(default_for("production")));
    }

    /// The og:image path is rooted, so the placeholder is substituted directly
    /// in front of it and a trailing slash on the origin would double up.
    #[test]
    fn a_trailing_slash_does_not_double_the_separator() {
        assert_eq!(for_env("staging"), "https://crosswords-staging.casazza.io");
        let shell = "<meta property=\"og:image\" content=\"__ORIGIN__/_assets/deadbeef/og.png\" />";
        let rendered = substitute(shell, &for_env("staging"));
        assert!(rendered.contains("https://crosswords-staging.casazza.io/_assets/deadbeef/og.png"));
        assert!(!rendered.contains("//_assets"));
    }

    /// A shell with no placeholder is served as-is. This is what a rolling
    /// update looks like from the new pod's side, and it must not be an error.
    #[test]
    fn a_shell_without_the_placeholder_is_untouched() {
        let old = "<link rel=\"canonical\" href=\"https://crosswords.casazza.io/\" />";
        assert_eq!(substitute(old, "https://elsewhere.example"), old);
    }
}
