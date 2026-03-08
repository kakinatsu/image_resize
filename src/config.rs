use std::{env, net::SocketAddr, path::PathBuf};

pub const DEFAULT_UPLOAD_MAX_FILE_BYTES: usize = 10_000_000;

pub struct Config {
    pub app_addr: SocketAddr,
    pub public_base_url: String,
    pub sqlite_path: PathBuf,
    pub static_dir: PathBuf,
    pub upload_max_file_bytes: usize,
    pub r2_endpoint: String,
    pub r2_bucket: String,
    pub r2_access_key_id: String,
    pub r2_secret_access_key: String,
    pub r2_region: String,
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        let app_addr = env::var("APP_ADDR")
            .unwrap_or_else(|_| "127.0.0.1:3000".to_owned())
            .parse()
            .map_err(ConfigError::InvalidSocketAddr)?;

        let public_base_url =
            env::var("PUBLIC_BASE_URL").unwrap_or_else(|_| "http://127.0.0.1:3000".to_owned());

        let sqlite_path = env::var("SQLITE_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("data/image_upload.db"));

        let static_dir = env::var("STATIC_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("static"));

        let upload_max_file_bytes = match env::var("UPLOAD_MAX_FILE_BYTES") {
            Ok(value) => parse_positive_usize("UPLOAD_MAX_FILE_BYTES", &value)?,
            Err(env::VarError::NotPresent) => DEFAULT_UPLOAD_MAX_FILE_BYTES,
            Err(_) => {
                return Err(ConfigError::InvalidPositiveInteger {
                    key: "UPLOAD_MAX_FILE_BYTES",
                    value: "<non-unicode>".to_owned(),
                });
            }
        };

        let r2_endpoint = required_var("R2_ENDPOINT")?;
        let r2_bucket = required_var("R2_BUCKET")?;
        let r2_access_key_id = required_var("R2_ACCESS_KEY_ID")?;
        let r2_secret_access_key = required_var("R2_SECRET_ACCESS_KEY")?;
        let r2_region = env::var("R2_REGION").unwrap_or_else(|_| "auto".to_owned());

        Ok(Self {
            app_addr,
            public_base_url,
            sqlite_path,
            static_dir,
            upload_max_file_bytes,
            r2_endpoint,
            r2_bucket,
            r2_access_key_id,
            r2_secret_access_key,
            r2_region,
        })
    }
}

fn required_var(key: &'static str) -> Result<String, ConfigError> {
    match env::var(key) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(ConfigError::MissingEnv(key)),
    }
}

fn parse_positive_usize(key: &'static str, value: &str) -> Result<usize, ConfigError> {
    match value.parse::<usize>() {
        Ok(parsed) if parsed > 0 => Ok(parsed),
        _ => Err(ConfigError::InvalidPositiveInteger {
            key,
            value: value.to_owned(),
        }),
    }
}

#[derive(Debug)]
pub enum ConfigError {
    InvalidSocketAddr(std::net::AddrParseError),
    MissingEnv(&'static str),
    InvalidPositiveInteger { key: &'static str, value: String },
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidSocketAddr(err) => write!(f, "invalid APP_ADDR: {err}"),
            Self::MissingEnv(key) => write!(f, "missing required environment variable {key}"),
            Self::InvalidPositiveInteger { key, value } => {
                write!(f, "invalid positive integer for {key}: {value}")
            }
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::{ConfigError, parse_positive_usize};

    #[test]
    fn parse_positive_usize_accepts_positive_integers() {
        let parsed = parse_positive_usize("UPLOAD_MAX_FILE_BYTES", "25000000").unwrap();
        assert_eq!(parsed, 25_000_000);
    }

    #[test]
    fn parse_positive_usize_rejects_zero() {
        let result = parse_positive_usize("UPLOAD_MAX_FILE_BYTES", "0");
        assert!(matches!(
            result,
            Err(ConfigError::InvalidPositiveInteger {
                key: "UPLOAD_MAX_FILE_BYTES",
                ..
            })
        ));
    }
}
