use crate::theme::Theme;
use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct LayoutConfig {
    pub node_spacing: f32,
    pub rank_spacing: f32,
    pub node_padding_x: f32,
    pub node_padding_y: f32,
    pub label_line_height: f32,
    pub max_label_width_chars: usize,
}

impl Default for LayoutConfig {
    fn default() -> Self {
        Self {
            node_spacing: 50.0,
            rank_spacing: 50.0,
            node_padding_x: 30.0,
            node_padding_y: 15.0,
            label_line_height: 1.5,
            max_label_width_chars: 22,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RenderConfig {
    pub width: f32,
    pub height: f32,
    pub background: String,
}

impl Default for RenderConfig {
    fn default() -> Self {
        Self {
            width: 1200.0,
            height: 800.0,
            background: "#FFFFFF".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub theme: Theme,
    pub layout: LayoutConfig,
    pub render: RenderConfig,
}

impl Default for Config {
    fn default() -> Self {
        let theme = Theme::mermaid_default();
        let render = RenderConfig {
            background: theme.background.clone(),
            ..Default::default()
        };
        Self {
            theme,
            layout: LayoutConfig::default(),
            render,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ThemeVariables {
    font_family: Option<String>,
    font_size: Option<f32>,
    primary_color: Option<String>,
    primary_text_color: Option<String>,
    primary_border_color: Option<String>,
    line_color: Option<String>,
    secondary_color: Option<String>,
    tertiary_color: Option<String>,
    edge_label_background: Option<String>,
    cluster_bkg: Option<String>,
    cluster_border: Option<String>,
    background: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlowchartConfig {
    node_spacing: Option<f32>,
    rank_spacing: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ConfigFile {
    theme: Option<String>,
    theme_variables: Option<ThemeVariables>,
    flowchart: Option<FlowchartConfig>,
}

pub fn load_config(path: Option<&Path>) -> anyhow::Result<Config> {
    let mut config = Config::default();
    let Some(path) = path else {
        return Ok(config);
    };

    let contents = std::fs::read_to_string(path)?;
    let parsed: ConfigFile = serde_json::from_str(&contents)?;

    if let Some(theme_name) = parsed.theme.as_deref() {
        if theme_name == "modern" {
            config.theme = Theme::modern();
        } else if theme_name == "base" || theme_name == "default" || theme_name == "mermaid" {
            config.theme = Theme::mermaid_default();
        }
    }

    if let Some(vars) = parsed.theme_variables {
        if let Some(v) = vars.font_family {
            config.theme.font_family = v;
        }
        if let Some(v) = vars.font_size {
            config.theme.font_size = v;
        }
        if let Some(v) = vars.primary_color {
            config.theme.primary_color = v;
        }
        if let Some(v) = vars.primary_text_color {
            config.theme.primary_text_color = v;
        }
        if let Some(v) = vars.primary_border_color {
            config.theme.primary_border_color = v;
        }
        if let Some(v) = vars.line_color {
            config.theme.line_color = v;
        }
        if let Some(v) = vars.secondary_color {
            config.theme.secondary_color = v;
        }
        if let Some(v) = vars.tertiary_color {
            config.theme.tertiary_color = v;
        }
        if let Some(v) = vars.edge_label_background {
            config.theme.edge_label_background = v;
        }
        if let Some(v) = vars.cluster_bkg {
            config.theme.cluster_background = v;
        }
        if let Some(v) = vars.cluster_border {
            config.theme.cluster_border = v;
        }
        if let Some(v) = vars.background {
            config.theme.background = v;
        }
    }

    if let Some(flow) = parsed.flowchart {
        if let Some(v) = flow.node_spacing {
            config.layout.node_spacing = v;
        }
        if let Some(v) = flow.rank_spacing {
            config.layout.rank_spacing = v;
        }
    }

    config.render.background = config.theme.background.clone();

    Ok(config)
}
