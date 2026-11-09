use std::{env, fmt, str::FromStr};

use anyhow::{Context, Result, bail};
use url::Url;

#[derive(Clone)]
pub struct JwtConfig {
    pub private_key: String,
    pub public_key: String,
    pub algorithm: String,
    pub access_ttl_secs: i64,
    pub refresh_ttl_secs: i64,
    pub audience: String,
    pub issuer: String,
}

#[derive(Clone)]
pub struct AdminConfig {
    pub email: String,
    pub mobile: String,
    pub password: String,
}

#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Requests per minute per client for authentication routes.
    pub auth_per_minute: u32,
    /// Requests per minute per client for every other route.
    pub general_per_minute: u32,
    /// Read the client IP from `X-Forwarded-For`. Enable only behind a proxy that overwrites it.
    pub trust_proxy_headers: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Default)]
pub enum EmailProvider {
    Mailgun,
    #[default]
    LogOnly,
}

impl EmailProvider {
    pub fn from_env_value(val: Option<&str>) -> Self {
        match val.map(|v| v.trim().to_lowercase()).as_deref() {
            Some("mailgun") => Self::Mailgun,
            _ => Self::LogOnly,
        }
    }
}

#[derive(Clone)]
pub struct Config {
    pub environment: String,
    pub host: String,
    pub port: u16,
    pub database_url: String,
    pub frontend_base_url: String,
    pub cors_origins: Vec<String>,
    pub jwt: JwtConfig,
    pub admin: AdminConfig,
    pub rate_limit: RateLimitConfig,
    /// Enables Google sign-in when set.
    pub google_client_id: Option<String>,
    /// Serve Swagger UI (`/docs`) and the OpenAPI document (`/openapi.json`).
    pub docs_enabled: bool,
    pub otp_expiry_minutes: i64,
    /// Email domains allowed to hold staff/admin roles.
    pub allowed_staff_email_domains: Vec<String>,
    /// Directory for uploaded files, served under `/uploads`.
    pub upload_dir: String,
    pub max_upload_bytes: usize,
    /// Object storage for uploads; local disk is used when unset.
    pub spaces: Option<SpacesConfig>,
    pub mailgun_api_key: Option<String>,
    pub mailgun_domain: Option<String>,
    pub mail_from: String,
    pub email_provider: EmailProvider,
    /// Optional `Reply-To` addresses added to every outbound email. Must point at an
    /// inbox on the main domain (the mailer domain is outbound-only).
    pub mail_reply_to: Option<Vec<String>>,
    /// Optional `Cc` addresses added to every outbound email.
    pub mail_cc_to: Option<Vec<String>>,
}

#[derive(Clone)]
pub struct SpacesConfig {
    pub key: String,
    pub secret: String,
    pub region: String,
    pub bucket: String,
    pub endpoint: String,
    pub cdn_endpoint: String,
}

impl SpacesConfig {
    /// Uses Spaces only when every `DO_SPACES_*` value is set (placeholders count as unset).
    fn from_env() -> Option<Self> {
        let get = |k: &str| env::var(k).ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        let key = get("DO_SPACES_KEY").filter(|k| k != "your_key")?;
        let secret = get("DO_SPACES_SECRET").filter(|k| k != "your_secret")?;
        Some(Self {
            key,
            secret,
            region: get("DO_SPACES_REGION")?,
            bucket: get("DO_SPACES_BUCKET")?,
            endpoint: get("DO_SPACES_ENDPOINT")?,
            cdn_endpoint: get("DO_SPACES_CDN_ENDPOINT")?,
        })
    }
}

// Manual Debug impls so secrets never end up in logs.
impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("environment", &self.environment)
            .field("host", &self.host)
            .field("port", &self.port)
            .field("database_url", &"<redacted>")
            .field("cors_origins", &self.cors_origins)
            .field("frontend_base_url", &self.frontend_base_url)
            .field("jwt_algorithm", &self.jwt.algorithm)
            .field("jwt_audience", &self.jwt.audience)
            .field("jwt_issuer", &self.jwt.issuer)
            .field("admin_email", &self.admin.email)
            .field("rate_limit", &self.rate_limit)
            .field("google_enabled", &self.google_client_id.is_some())
            .field("docs_enabled", &self.docs_enabled)
            .field("otp_expiry_minutes", &self.otp_expiry_minutes)
            .field("spaces", &self.spaces.as_ref().map(|s| &s.bucket))
            .field("admin_email", &self.admin.email)
            .field("email_provider", &self.email_provider)
            .finish_non_exhaustive()
    }
}

fn required(key: &str) -> Result<String> {
    env::var(key).with_context(|| format!("{key} must be set"))
}

fn var_or<T: FromStr>(key: &str, default: T) -> Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    match env::var(key) {
        Ok(v) => v.trim().parse().with_context(|| format!("invalid value for {key}")),
        Err(_) => Ok(default),
    }
}

fn bool_or(key: &str, default: bool) -> Result<bool> {
    match env::var(key) {
        Ok(v) => match v.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(true),
            "0" | "false" | "no" | "off" => Ok(false),
            _ => bail!("invalid boolean for {key}: {v}"),
        },
        Err(_) => Ok(default),
    }
}

/// PEM values are often stored on one line with literal `\n` sequences.
fn pem(key: &str) -> Result<String> {
    Ok(required(key)?.replace("\\n", "\n").trim().to_string())
}

fn parse_address_list(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut in_angle = false;
    for ch in raw.chars() {
        match ch {
            '"' => in_quotes = !in_quotes,
            '<' if !in_quotes => in_angle = true,
            '>' if !in_quotes => in_angle = false,
            ',' | ';' if !in_quotes && !in_angle => {
                out.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(ch);
    }
    out.push(current);
    out.into_iter().map(|a| a.trim().to_string()).filter(|a| !a.is_empty()).collect()
}

fn bare_address(addr: &str) -> String {
    let addr = addr.trim();
    let inner = match (addr.rfind('<'), addr.rfind('>')) {
        (Some(start), Some(end)) if start < end => &addr[start + 1..end],
        _ => addr,
    };
    inner.trim().trim_matches('"').to_lowercase()
}

/// Parses a comma-separated list of addresses such as `Name <a@x.com>, b@x.com`.
/// Returns `None` when the value is blank.
pub fn parse_mail_addresses(key: &str, raw: &str) -> Result<Option<Vec<String>>> {
    let list = parse_address_list(raw);
    if list.is_empty() {
        return Ok(None);
    }
    for addr in &list {
        let mailbox = bare_address(addr);
        let valid = mailbox.split_once('@').is_some_and(|(user, domain)| !user.is_empty() && domain.contains('.'));
        if !valid {
            bail!("invalid email address in {key}: {addr}");
        }
    }
    Ok(Some(list))
}

/// Optional address list; unset or blank yields `None`.
fn mail_addresses(key: &str) -> Result<Option<Vec<String>>> {
    match env::var(key) {
        Ok(raw) => parse_mail_addresses(key, &raw),
        Err(_) => Ok(None),
    }
}

/// Accepts a JSON array (`["http://a","http://b"]`) or a comma-separated list.
pub fn parse_origins(raw: &str) -> Result<Vec<String>> {
    let raw = raw.trim();
    let list: Vec<String> = if raw.starts_with('[') {
        serde_json::from_str(raw).context("CORS_ORIGINS is not a valid JSON array")?
    } else {
        raw.split(',').map(str::to_string).collect()
    };
    Ok(list.into_iter().map(|o| o.trim().trim_end_matches('/').to_string()).filter(|o| !o.is_empty()).collect())
}

fn database_url() -> Result<String> {
    if let Ok(url) = env::var("DATABASE_URL") {
        return Ok(url);
    }
    let mut url = Url::parse("postgres://localhost").expect("static url");
    url.set_host(Some(&var_or("POSTGRES_HOST", "localhost".to_string())?))?;
    url.set_port(Some(var_or("POSTGRES_PORT", 5432)?)).ok();
    url.set_username(&required("POSTGRES_USER")?).ok();
    url.set_password(Some(&required("POSTGRES_PASSWORD")?)).ok();
    url.set_path(&required("POSTGRES_DB")?);
    let ssl = if bool_or("POSTGRES_USE_SSL", false)? { "require" } else { "disable" };
    url.query_pairs_mut().append_pair("sslmode", ssl);
    Ok(url.into())
}

impl Config {
    /// Reads configuration from the environment. `DATABASE_URL`, when set, overrides `POSTGRES_*`.
    pub fn from_env() -> Result<Self> {
        let environment = var_or("ENVIRONMENT", "development".to_string())?.to_lowercase();
        let frontend_base_url = var_or("FRONTEND_BASE_URL", "https://www.hrpropertymaintenance.ca".to_string())?;
        let cors_origins = parse_origins(&env::var("CORS_ORIGINS").unwrap_or_default())?;
        if environment == "production" && cors_origins.iter().any(|o| o == "*") {
            bail!("CORS_ORIGINS must not contain '*' in production");
        }
        let jwt = JwtConfig {
            private_key: pem("JWT_PRIVATE_KEY")?,
            public_key: pem("JWT_PUBLIC_KEY")?,
            algorithm: var_or("JWT_ALGORITHM", "RS256".to_string())?,
            access_ttl_secs: var_or("JWT_ACCESS_TTL_SECS", 64800)?,
            refresh_ttl_secs: var_or("JWT_REFRESH_TTL_SECS", 28 * 24 * 3600)?,
            audience: required("JWT_AUDIENCE")?,
            issuer: required("JWT_ISSUER")?,
        };
        let admin = AdminConfig {
            email: required("ADMIN_EMAIL")?.trim().to_lowercase(),
            mobile: required("ADMIN_MOBILE")?.trim().to_string(),
            password: required("ADMIN_PASSWORD")?,
        };
        if admin.password.len() < 8 {
            bail!("ADMIN_PASSWORD must be at least 8 characters");
        }
        let docs_enabled = bool_or("DOCS_ENABLED", environment != "production")?;
        let google_client_id =
            env::var("GOOGLE_CLIENT_ID").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty());
        Ok(Self {
            environment,
            host: var_or("HOST", "0.0.0.0".to_string())?,
            port: var_or("PORT", 8080)?,
            database_url: database_url()?,
            frontend_base_url,
            cors_origins,
            jwt,
            admin,
            google_client_id,
            docs_enabled,
            otp_expiry_minutes: var_or("OTP_EXPIRY_MINUTES", 5)?,
            allowed_staff_email_domains: parse_origins(&var_or(
                "ALLOWED_STAFF_EMAIL_DOMAINS",
                r#"["hrpropertymaintenance.ca","impacttxtech.com"]"#.to_string(),
            )?)?
            .into_iter()
            .map(|d| d.trim_start_matches('@').to_lowercase())
            .collect(),
            spaces: SpacesConfig::from_env(),
            upload_dir: var_or("UPLOAD_DIR", "uploads".to_string())?,
            max_upload_bytes: var_or::<usize>("MAX_UPLOAD_FILE_SIZE", 10)? * 1024 * 1024,
            email_provider: EmailProvider::from_env_value(
                env::var("EMAIL_PROVIDER").ok().filter(|v| !v.trim().is_empty()).as_deref(),
            ),
            mailgun_api_key: env::var("MAILGUN_API_KEY").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()),
            mailgun_domain: env::var("MAILGUN_DOMAIN").ok().map(|v| v.trim().to_string()).filter(|v| !v.is_empty()),
            mail_from: var_or("MAIL_FROM", "noreply@mailer.hrpropertymaintenance.ca".to_string())?,
            mail_reply_to: mail_addresses("MAIL_REPLY_TO")?,
            mail_cc_to: mail_addresses("MAIL_CC_TO")?,
            rate_limit: RateLimitConfig {
                auth_per_minute: var_or("RATE_LIMIT_AUTH_PER_MINUTE", 10)?,
                general_per_minute: var_or("RATE_LIMIT_GENERAL_PER_MINUTE", 300)?,
                trust_proxy_headers: bool_or("RATE_LIMIT_TRUST_PROXY_HEADERS", false)?,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_json_and_csv_origins() {
        assert_eq!(
            parse_origins(r#"["http://localhost:3000","http://localhost:8082/"]"#).unwrap(),
            ["http://localhost:3000", "http://localhost:8082"]
        );
        assert_eq!(parse_origins("http://a.com, http://b.com").unwrap().len(), 2);
        assert!(parse_origins("").unwrap().is_empty());
        assert!(parse_origins("[oops").is_err());
    }

    #[test]
    fn parses_mail_address_lists() {
        assert_eq!(
            parse_mail_addresses("K", r#"User <hello@x.com>, "Doe, J" <j@x.com>"#).unwrap().unwrap(),
            ["User <hello@x.com>", r#""Doe, J" <j@x.com>"#]
        );
        assert_eq!(parse_mail_addresses("K", "").unwrap(), None);
        assert_eq!(parse_mail_addresses("K", " , ").unwrap(), None);
        assert!(parse_mail_addresses("K", "Name <not-an-email>").is_err());
    }

    #[test]
    fn parses_email_provider() {
        assert_eq!(EmailProvider::from_env_value(Some("Mailgun")), EmailProvider::Mailgun);
        assert_eq!(EmailProvider::from_env_value(Some("mailgun")), EmailProvider::Mailgun);
        assert_eq!(EmailProvider::from_env_value(Some("  MAILGUN  ")), EmailProvider::Mailgun);
        assert_eq!(EmailProvider::from_env_value(Some("log_only")), EmailProvider::LogOnly);
        assert_eq!(EmailProvider::from_env_value(Some("")), EmailProvider::LogOnly);
        assert_eq!(EmailProvider::from_env_value(None), EmailProvider::LogOnly);
    }
}
