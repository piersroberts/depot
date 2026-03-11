//! CSS theme definitions
//!
//! Each theme defines colors and fonts that get passed to templates.

use serde::Serialize;

/// Theme CSS properties passed to templates
#[derive(Debug, Clone, Serialize)]
pub struct Theme {
    pub name: &'static str,
    // Background & text
    pub bg: &'static str,
    pub fg: &'static str,
    // Table header
    pub th_bg: &'static str,
    pub th_border: &'static str,
    // Row hover
    pub row_hover: &'static str,
    // Links
    pub link: &'static str,
    pub link_hover: &'static str,
    // Footer/misc
    pub border: &'static str,
    pub muted: &'static str,
    // Font
    pub font: &'static str,
    // Error heading color
    pub error: &'static str,
    // Optional extras
    pub td_border: Option<&'static str>,
    pub link_shadow: Option<&'static str>,
    pub font_size: Option<&'static str>,
}

impl Default for Theme {
    fn default() -> Self {
        get_theme("modern")
    }
}

/// Get a theme by name, falling back to "modern" if not found
pub fn get_theme(name: &str) -> Theme {
    match name.to_lowercase().as_str() {
        "modern" => MODERN,
        "terminal" => TERMINAL,
        "paperwhite" => PAPERWHITE,
        "ocean" => OCEAN,
        "midnight" => MIDNIGHT,
        "amber" => AMBER,
        "solarized" => SOLARIZED,
        "highcontrast" => HIGH_CONTRAST,
        _ => MODERN,
    }
}

/// List all available theme names
#[allow(dead_code)]
pub fn list_themes() -> Vec<&'static str> {
    vec![
        "modern",
        "terminal", 
        "paperwhite",
        "ocean",
        "midnight",
        "amber",
        "solarized",
        "highcontrast",
    ]
}

// ============================================================================
// Theme Definitions
// ============================================================================

const MODERN: Theme = Theme {
    name: "modern",
    bg: "#fafafa",
    fg: "#333",
    th_bg: "#f5f5f5",
    th_border: "#ddd",
    row_hover: "#f0f7ff",
    link: "#0066cc",
    link_hover: "#0066cc",
    border: "#ddd",
    muted: "#666",
    font: "sans-serif",
    error: "#c00",
    td_border: None,
    link_shadow: None,
    font_size: None,
};

const TERMINAL: Theme = Theme {
    name: "terminal",
    bg: "#0a0a0a",
    fg: "#00ff00",
    th_bg: "#0a0a0a",
    th_border: "#00ff00",
    row_hover: "#001a00",
    link: "#00ff00",
    link_hover: "#00ff88",
    border: "#003300",
    muted: "#008800",
    font: "'Courier New', monospace",
    error: "#ff0000",
    td_border: Some("#003300"),
    link_shadow: None,
    font_size: None,
};

const PAPERWHITE: Theme = Theme {
    name: "paperwhite",
    bg: "#f8f6f1",
    fg: "#2c2c2c",
    th_bg: "#eeebe3",
    th_border: "#d0ccc0",
    row_hover: "#f0ede5",
    link: "#444",
    link_hover: "#000",
    border: "#d0ccc0",
    muted: "#888",
    font: "Georgia, serif",
    error: "#8b0000",
    td_border: None,
    link_shadow: None,
    font_size: None,
};

const OCEAN: Theme = Theme {
    name: "ocean",
    bg: "#e8f4f8",
    fg: "#1a3a4a",
    th_bg: "#d0e8f0",
    th_border: "#b8d4e3",
    row_hover: "#c8e0eb",
    link: "#2077a0",
    link_hover: "#104060",
    border: "#b8d4e3",
    muted: "#5a8a9a",
    font: "sans-serif",
    error: "#a04040",
    td_border: None,
    link_shadow: None,
    font_size: None,
};

const MIDNIGHT: Theme = Theme {
    name: "midnight",
    bg: "#1a1a2e",
    fg: "#eaeaea",
    th_bg: "#25254a",
    th_border: "#4a4a6a",
    row_hover: "#2a2a4e",
    link: "#a78bfa",
    link_hover: "#c4b5fd",
    border: "#4a4a6a",
    muted: "#8888aa",
    font: "sans-serif",
    error: "#ff6b6b",
    td_border: Some("#2a2a4a"),
    link_shadow: None,
    font_size: None,
};

const AMBER: Theme = Theme {
    name: "amber",
    bg: "#1a1400",
    fg: "#ffb000",
    th_bg: "#1a1400",
    th_border: "#ffb000",
    row_hover: "#2a2000",
    link: "#ffb000",
    link_hover: "#ffd060",
    border: "#4a3800",
    muted: "#906000",
    font: "'Courier New', monospace",
    error: "#ff4000",
    td_border: Some("#4a3800"),
    link_shadow: Some("0 0 5px #ffb000"),
    font_size: None,
};

const SOLARIZED: Theme = Theme {
    name: "solarized",
    bg: "#002b36",
    fg: "#839496",
    th_bg: "#073642",
    th_border: "#073642",
    row_hover: "#073642",
    link: "#268bd2",
    link_hover: "#2aa198",
    border: "#073642",
    muted: "#586e75",
    font: "sans-serif",
    error: "#dc322f",
    td_border: Some("#073642"),
    link_shadow: None,
    font_size: None,
};

const HIGH_CONTRAST: Theme = Theme {
    name: "highcontrast",
    bg: "#000",
    fg: "#fff",
    th_bg: "#000",
    th_border: "#fff",
    row_hover: "#333",
    link: "#ffff00",
    link_hover: "#00ffff",
    border: "#fff",
    muted: "#ccc",
    font: "sans-serif",
    error: "#ff0",
    td_border: Some("#444"),
    link_shadow: None,
    font_size: Some("18px"),
};
