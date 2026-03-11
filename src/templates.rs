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
        format!("{:.1}G", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1}M", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1}K", bytes as f64 / KB as f64)
    } else {
        format!("{bytes}")
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
