//! Template engine for HTML and text output
//!
//! Uses minijinja for a lightweight Jinja2-like syntax.
//! Templates are embedded at compile time for portability.

use minijinja::{Environment, Value};
use std::sync::OnceLock;

/// Global template environment
static TEMPLATES: OnceLock<Environment<'static>> = OnceLock::new();

/// Initialize the template environment with all templates
pub fn init() -> &'static Environment<'static> {
    TEMPLATES.get_or_init(|| {
        let mut env = Environment::new();

        // Register custom filters
        env.add_filter("filesize", filter_filesize);
        env.add_filter("datetime", filter_datetime);

        // HTTP Templates
        env.add_template(
            "http/directory.html",
            include_str!("templates/http/directory.html.j2"),
        )
        .expect("Failed to load directory template");
        env.add_template(
            "http/error.html",
            include_str!("templates/http/error.html.j2"),
        )
        .expect("Failed to load error template");

        // Admin Templates
        env.add_template(
            "admin/dashboard.html",
            include_str!("templates/admin/dashboard.html.j2"),
        )
        .expect("Failed to load admin dashboard template");
        env.add_template(
            "admin/shares.html",
            include_str!("templates/admin/shares.html.j2"),
        )
        .expect("Failed to load admin shares template");
        env.add_template(
            "admin/config.html",
            include_str!("templates/admin/config.html.j2"),
        )
        .expect("Failed to load admin config template");

        // FTP Templates (plain text)
        env.add_template(
            "ftp/welcome.txt",
            include_str!("templates/ftp/welcome.txt.j2"),
        )
        .expect("Failed to load FTP welcome template");

        env
    })
}

/// Get the template environment
pub fn get() -> &'static Environment<'static> {
    TEMPLATES
        .get()
        .expect("Templates not initialized - call templates::init() first")
}

/// Render a template with the given context
pub fn render(template_name: &str, ctx: Value) -> Result<String, minijinja::Error> {
    let env = get();
    let template = env.get_template(template_name)?;
    template.render(ctx)
}

/// Filter: Format bytes as human-readable file size
fn filter_filesize(value: Value) -> Result<String, minijinja::Error> {
    let bytes: u64 = value.try_into().unwrap_or(0);

    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    let result = if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} kB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    };

    Ok(result)
}

/// Filter: Format timestamp as date string
fn filter_datetime(value: Value) -> Result<String, minijinja::Error> {
    // Value should be a unix timestamp (seconds)
    let timestamp: i64 = value.try_into().unwrap_or(0);

    if timestamp == 0 {
        return Ok("-".to_string());
    }

    use chrono::{Local, TimeZone};
    let datetime = Local
        .timestamp_opt(timestamp, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string());

    Ok(datetime)
}

/// Helper to convert SystemTime to unix timestamp for templates
pub fn systemtime_to_timestamp(time: Option<std::time::SystemTime>) -> i64 {
    time.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn test_filter_filesize_bytes() {
        assert_eq!(filter_filesize(Value::from(0u64)).unwrap(), "0 B");
        assert_eq!(filter_filesize(Value::from(512u64)).unwrap(), "512 B");
        assert_eq!(filter_filesize(Value::from(1023u64)).unwrap(), "1023 B");
    }

    #[test]
    fn test_filter_filesize_kilobytes() {
        assert_eq!(filter_filesize(Value::from(1024u64)).unwrap(), "1.0 kB");
        assert_eq!(filter_filesize(Value::from(1536u64)).unwrap(), "1.5 kB");
        assert_eq!(filter_filesize(Value::from(10240u64)).unwrap(), "10.0 kB");
    }

    #[test]
    fn test_filter_filesize_megabytes() {
        assert_eq!(filter_filesize(Value::from(1048576u64)).unwrap(), "1.0 MB");
        assert_eq!(filter_filesize(Value::from(1572864u64)).unwrap(), "1.5 MB");
        assert_eq!(
            filter_filesize(Value::from(104857600u64)).unwrap(),
            "100.0 MB"
        );
    }

    #[test]
    fn test_filter_filesize_gigabytes() {
        assert_eq!(
            filter_filesize(Value::from(1073741824u64)).unwrap(),
            "1.0 GB"
        );
        assert_eq!(
            filter_filesize(Value::from(5368709120u64)).unwrap(),
            "5.0 GB"
        );
    }

    #[test]
    fn test_filter_datetime_zero() {
        assert_eq!(filter_datetime(Value::from(0i64)).unwrap(), "-");
    }

    #[test]
    fn test_filter_datetime_valid() {
        // Use a known timestamp: 2024-01-15 12:00:00 UTC = 1705320000
        let result = filter_datetime(Value::from(1705320000i64)).unwrap();
        // Just verify it's not "-" and contains expected format
        assert_ne!(result, "-");
        assert!(result.contains("2024") || result.contains("2025")); // timezone may vary
    }

    #[test]
    fn test_systemtime_to_timestamp_none() {
        assert_eq!(systemtime_to_timestamp(None), 0);
    }

    #[test]
    fn test_systemtime_to_timestamp_some() {
        let time = UNIX_EPOCH + Duration::from_secs(1705320000);
        assert_eq!(systemtime_to_timestamp(Some(time)), 1705320000);
    }

    #[test]
    fn test_systemtime_to_timestamp_epoch() {
        assert_eq!(systemtime_to_timestamp(Some(UNIX_EPOCH)), 0);
    }
}
