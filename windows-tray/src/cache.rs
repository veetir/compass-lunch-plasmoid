use crate::restaurant::{provider_key, Provider};
use std::fs;
use std::path::{Path, PathBuf};

pub fn cache_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA").unwrap_or_else(|_| ".".to_string());
    Path::new(&base).join("compass-lunch").join("cache")
}

pub fn cache_path(provider: Provider, code: &str, language: &str) -> PathBuf {
    let ext = match provider {
        Provider::Compass => "json",
        Provider::Antell => "html",
    };
    let filename = format!("{}|{}|{}.{}", provider_key(provider), code, language, ext);
    cache_dir().join(filename)
}

pub fn read_cache(provider: Provider, code: &str, language: &str) -> Option<String> {
    let path = cache_path(provider, code, language);
    fs::read_to_string(path).ok()
}

pub fn cache_mtime_ms(provider: Provider, code: &str, language: &str) -> Option<i64> {
    let path = cache_path(provider, code, language);
    let metadata = fs::metadata(path).ok()?;
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(duration.as_millis() as i64)
}

pub fn write_cache(provider: Provider, code: &str, language: &str, payload: &str) -> anyhow::Result<()> {
    let dir = cache_dir();
    fs::create_dir_all(&dir)?;
    let path = cache_path(provider, code, language);
    fs::write(path, payload)?;
    Ok(())
}
