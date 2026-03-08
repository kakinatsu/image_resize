use std::{
    fs, io,
    path::{Path, PathBuf},
};

use sha2::{Digest, Sha256};

const INDEX_SCRIPT_PLACEHOLDER: &str = "__APP_SCRIPT_SRC__";
const APP_FINGERPRINT_HEX_LENGTH: usize = 16;

pub struct UiAssets {
    pub index_html: String,
    pub app_asset_route: String,
    pub app_script: Vec<u8>,
}

pub fn load(static_dir: &Path) -> Result<UiAssets, UiAssetError> {
    let index_path = static_dir.join("index.html");
    let app_script_path = static_dir.join("app.js");

    let index_template = fs::read_to_string(&index_path).map_err(|source| UiAssetError::Read {
        path: index_path.clone(),
        source,
    })?;
    let app_script = fs::read(&app_script_path).map_err(|source| UiAssetError::Read {
        path: app_script_path,
        source,
    })?;

    if !index_template.contains(INDEX_SCRIPT_PLACEHOLDER) {
        return Err(UiAssetError::MissingPlaceholder {
            path: index_path,
            placeholder: INDEX_SCRIPT_PLACEHOLDER,
        });
    }

    let fingerprint = fingerprint(&app_script);
    let app_asset_route = format!("/assets/app.{fingerprint}.js");
    let index_html = index_template.replace(INDEX_SCRIPT_PLACEHOLDER, &app_asset_route);

    Ok(UiAssets {
        index_html,
        app_asset_route,
        app_script,
    })
}

fn fingerprint(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(APP_FINGERPRINT_HEX_LENGTH);

    for byte in digest.iter().take(APP_FINGERPRINT_HEX_LENGTH / 2) {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }

    output
}

#[derive(Debug)]
pub enum UiAssetError {
    Read {
        path: PathBuf,
        source: io::Error,
    },
    MissingPlaceholder {
        path: PathBuf,
        placeholder: &'static str,
    },
}

impl std::fmt::Display for UiAssetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(f, "failed to read ui asset {}: {source}", path.display())
            }
            Self::MissingPlaceholder { path, placeholder } => write!(
                f,
                "ui template {} is missing placeholder {placeholder}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for UiAssetError {}

#[cfg(test)]
mod tests {
    use std::{env, fs, path::PathBuf};

    use ulid::Ulid;

    use super::load;

    #[test]
    fn load_renders_fingerprinted_app_asset_path() {
        let temp_dir = temp_static_dir("render_index");
        fs::write(
            temp_dir.join("index.html"),
            r#"<!doctype html><script src="__APP_SCRIPT_SRC__"></script>"#,
        )
        .unwrap();
        fs::write(temp_dir.join("app.js"), "console.log('asset');").unwrap();

        let assets = load(&temp_dir).unwrap();

        assert!(assets.app_asset_route.starts_with("/assets/app."));
        assert!(assets.app_asset_route.ends_with(".js"));
        assert!(assets.index_html.contains(&assets.app_asset_route));
        assert_eq!(assets.app_script, b"console.log('asset');");

        cleanup_dir(&temp_dir);
    }

    fn temp_static_dir(test_name: &str) -> PathBuf {
        let dir = env::temp_dir().join(format!("image_resize_ui_{test_name}_{}", Ulid::new()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cleanup_dir(path: &PathBuf) {
        let _ = fs::remove_dir_all(path);
    }
}
