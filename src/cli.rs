use crate::config::{Config, load_config};
use crate::layout::compute_layout;
use crate::layout_dump::write_layout_dump;
use crate::parser::parse_mermaid;
use crate::render::{render_svg, write_output_png, write_output_svg};
use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(
    name = "mmdr",
    version,
    about = "Mermaid renderer in Rust (flowchart subset)"
)]
pub struct Args {
    /// Input file (.mmd) or '-' for stdin
    #[arg(short = 'i', long = "input")]
    pub input: Option<PathBuf>,

    /// Output file (svg/png). Defaults to stdout for SVG if omitted.
    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,

    /// Output format
    #[arg(short = 'e', long = "outputFormat", value_enum, default_value = "svg")]
    pub output_format: OutputFormat,

    /// Config JSON file (Mermaid-like themeVariables)
    #[arg(short = 'c', long = "configFile")]
    pub config: Option<PathBuf>,

    /// Width
    #[arg(short = 'w', long = "width", default_value_t = 1200.0)]
    pub width: f32,

    /// Height
    #[arg(short = 'H', long = "height", default_value_t = 800.0)]
    pub height: f32,

    /// Node spacing
    #[arg(long = "nodeSpacing")]
    pub node_spacing: Option<f32>,

    /// Rank spacing
    #[arg(long = "rankSpacing")]
    pub rank_spacing: Option<f32>,

    /// Dump computed layout JSON (file or directory for markdown input)
    #[arg(long = "dumpLayout")]
    pub dump_layout: Option<PathBuf>,

    /// Output timing information as JSON to stderr
    #[arg(long = "timing")]
    pub timing: bool,
}

#[derive(ValueEnum, Debug, Clone, Copy)]
pub enum OutputFormat {
    Svg,
    Png,
}

pub fn run() -> Result<()> {
    let args = Args::parse();
    let mut base_config = load_config(args.config.as_deref())?;
    base_config.render.width = args.width;
    base_config.render.height = args.height;
    if let Some(spacing) = args.node_spacing {
        base_config.layout.node_spacing = spacing;
    }
    if let Some(spacing) = args.rank_spacing {
        base_config.layout.rank_spacing = spacing;
    }

    let (input, is_markdown) = read_input(args.input.as_deref())?;
    let diagrams = if is_markdown {
        extract_mermaid_blocks(&input)
    } else {
        vec![input]
    };

    if diagrams.is_empty() {
        return Err(anyhow::anyhow!("No Mermaid diagrams found in input"));
    }

    let layout_outputs = if args.dump_layout.is_some() {
        Some(resolve_layout_outputs(
            args.dump_layout.as_deref(),
            diagrams.len(),
        )?)
    } else {
        None
    };

    if diagrams.len() == 1 {
        let t_parse_start = std::time::Instant::now();
        let parsed = parse_mermaid(&diagrams[0])?;
        let parse_us = t_parse_start.elapsed().as_micros();

        let mut config = base_config.clone();
        if let Some(init_cfg) = parsed.init_config {
            config = merge_init_config(config, init_cfg);
        }

        let t_layout_start = std::time::Instant::now();
        let layout = compute_layout(&parsed.graph, &config.theme, &config.layout);
        let layout_us = t_layout_start.elapsed().as_micros();

        if let Some(outputs) = layout_outputs.as_ref() {
            if let Some(path) = outputs.first() {
                write_layout_dump(path, &layout, &parsed.graph)?;
            }
        }

        let t_render_start = std::time::Instant::now();
        let svg = render_svg(&layout, &config.theme, &config.layout);
        let render_us = t_render_start.elapsed().as_micros();

        match args.output_format {
            OutputFormat::Svg => {
                write_output_svg(&svg, args.output.as_deref())?;
            }
            OutputFormat::Png => {
                let output = ensure_output(&args.output, "png")?;
                write_output_png(&svg, &output, &config.render, &config.theme)?;
            }
        }

        if args.timing {
            let total_us = parse_us + layout_us + render_us;
            eprintln!(
                r#"{{"parse_us":{},"layout_us":{},"render_us":{},"total_us":{}}}"#,
                parse_us, layout_us, render_us, total_us
            );
        }
        return Ok(());
    }

    // Multiple diagrams (Markdown input)
    let outputs =
        resolve_multi_outputs(args.output.as_deref(), args.output_format, diagrams.len())?;
    for (idx, diagram) in diagrams.iter().enumerate() {
        let parsed = parse_mermaid(diagram)?;
        let mut config = base_config.clone();
        if let Some(init_cfg) = parsed.init_config.clone() {
            config = merge_init_config(config, init_cfg);
        }
        let layout = compute_layout(&parsed.graph, &config.theme, &config.layout);
        if let Some(outputs) = layout_outputs.as_ref() {
            if let Some(path) = outputs.get(idx) {
                write_layout_dump(path, &layout, &parsed.graph)?;
            }
        }
        let svg = render_svg(&layout, &config.theme, &config.layout);
        match args.output_format {
            OutputFormat::Svg => {
                write_output_svg(&svg, Some(&outputs[idx]))?;
            }
            OutputFormat::Png => {
                write_output_png(&svg, &outputs[idx], &config.render, &config.theme)?;
            }
        }
    }

    Ok(())
}

fn read_input(path: Option<&Path>) -> Result<(String, bool)> {
    if let Some(path) = path {
        if path == Path::new("-") {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            return Ok((buf, false));
        }
        let content = std::fs::read_to_string(path)?;
        let is_md = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|ext| {
                let ext = ext.to_ascii_lowercase();
                matches!(ext.as_str(), "md" | "markdown")
            })
            .unwrap_or(false);
        return Ok((content, is_md));
    }

    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    Ok((buf, false))
}

fn ensure_output(output: &Option<PathBuf>, ext: &str) -> Result<PathBuf> {
    if let Some(path) = output {
        return Ok(path.clone());
    }
    Err(anyhow::anyhow!("Output path required for {} output", ext))
}

fn extract_mermaid_blocks(input: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut in_block = false;
    let mut current = Vec::new();
    let mut fence = String::new();

    for line in input.lines() {
        let trimmed = line.trim();
        if !in_block {
            if let Some(start_fence) = detect_mermaid_fence(trimmed) {
                in_block = true;
                fence = start_fence;
                continue;
            }
        } else if is_fence_end(trimmed, &fence) {
            in_block = false;
            blocks.push(current.join("\n"));
            current.clear();
            continue;
        }

        if in_block {
            current.push(line.to_string());
        }
    }

    blocks
}

fn detect_mermaid_fence(line: &str) -> Option<String> {
    if line.starts_with("```") {
        let rest = line.trim_start_matches('`').trim();
        if rest.starts_with("mermaid") {
            return Some("```".to_string());
        }
    }
    if line.starts_with("~~~") {
        let rest = line.trim_start_matches('~').trim();
        if rest.starts_with("mermaid") {
            return Some("~~~".to_string());
        }
    }
    if line.starts_with(":::") {
        let rest = line.trim_start_matches(':').trim();
        if rest.starts_with("mermaid") {
            return Some(":::".to_string());
        }
    }
    None
}

fn is_fence_end(line: &str, fence: &str) -> bool {
    if !line.starts_with(fence) {
        return false;
    }
    line[fence.len()..].trim().is_empty()
}

fn resolve_multi_outputs(
    output: Option<&Path>,
    format: OutputFormat,
    count: usize,
) -> Result<Vec<PathBuf>> {
    let ext = match format {
        OutputFormat::Svg => "svg",
        OutputFormat::Png => "png",
    };
    let base = output.ok_or_else(|| anyhow::anyhow!("Output path required for markdown input"))?;
    if base.is_dir() {
        let mut outputs = Vec::new();
        for idx in 0..count {
            outputs.push(base.join(format!("diagram-{}.{}", idx + 1, ext)));
        }
        return Ok(outputs);
    }
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("diagram");
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    let mut outputs = Vec::new();
    for idx in 0..count {
        outputs.push(parent.join(format!("{}-{}.{}", stem, idx + 1, ext)));
    }
    Ok(outputs)
}

fn resolve_layout_outputs(output: Option<&Path>, count: usize) -> Result<Vec<PathBuf>> {
    let base = output.ok_or_else(|| anyhow::anyhow!("Dump layout path required"))?;
    if base.is_dir() {
        let mut outputs = Vec::new();
        for idx in 0..count {
            outputs.push(base.join(format!("diagram-{}.layout.json", idx + 1)));
        }
        return Ok(outputs);
    }
    if count == 1 {
        return Ok(vec![base.to_path_buf()]);
    }
    let stem = base
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("diagram");
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    let mut outputs = Vec::new();
    for idx in 0..count {
        outputs.push(parent.join(format!("{}-{}.layout.json", stem, idx + 1)));
    }
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_mermaid_blocks() {
        let input = r#"
text
``` mermaid
flowchart LR
  A --> B
```
more
~~~mermaid
flowchart TD
  X --> Y
~~~
::: mermaid
sequenceDiagram
  A->>B: hi
:::
"#;
        let blocks = extract_mermaid_blocks(input);
        assert_eq!(blocks.len(), 3);
        assert!(blocks[0].contains("flowchart"));
        assert!(blocks[1].contains("flowchart"));
        assert!(blocks[2].contains("sequenceDiagram"));
    }

    #[test]
    fn merge_init_config_updates_layout() {
        let config = Config::default();
        let init = json!({
            "flowchart": {
                "nodeSpacing": 55,
                "rankSpacing": 90
            }
        });
        let merged = merge_init_config(config, init);
        assert_eq!(merged.layout.node_spacing, 55.0);
        assert_eq!(merged.layout.rank_spacing, 90.0);
    }

    #[test]
    fn merge_init_config_theme_variables() {
        let config = Config::default();
        let init = json!({
            "themeVariables": {
                "secondaryColor": "#ff00ff",
                "tertiaryColor": "#00ffff",
                "edgeLabelBackground": "#222222",
                "clusterBkg": "#333333",
                "clusterBorder": "#444444",
                "background": "#101010"
            }
        });
        let merged = merge_init_config(config, init);
        assert_eq!(merged.theme.secondary_color, "#ff00ff");
        assert_eq!(merged.theme.tertiary_color, "#00ffff");
        assert_eq!(merged.theme.edge_label_background, "#222222");
        assert_eq!(merged.theme.cluster_background, "#333333");
        assert_eq!(merged.theme.cluster_border, "#444444");
        assert_eq!(merged.theme.background, "#101010");
        assert_eq!(merged.render.background, "#101010");
    }
}

fn merge_init_config(mut config: Config, init: serde_json::Value) -> Config {
    if let Some(theme_name) = init.get("theme").and_then(|v| v.as_str()) {
        if theme_name == "modern" {
            config.theme = crate::theme::Theme::modern();
        } else if theme_name == "base" || theme_name == "default" || theme_name == "mermaid" {
            config.theme = crate::theme::Theme::mermaid_default();
        }
    }
    if let Some(theme_vars) = init.get("themeVariables") {
        if let Some(val) = theme_vars.get("primaryColor").and_then(|v| v.as_str()) {
            config.theme.primary_color = val.to_string();
        }
        if let Some(val) = theme_vars.get("primaryTextColor").and_then(|v| v.as_str()) {
            config.theme.primary_text_color = val.to_string();
        }
        if let Some(val) = theme_vars
            .get("primaryBorderColor")
            .and_then(|v| v.as_str())
        {
            config.theme.primary_border_color = val.to_string();
        }
        if let Some(val) = theme_vars.get("lineColor").and_then(|v| v.as_str()) {
            config.theme.line_color = val.to_string();
        }
        if let Some(val) = theme_vars.get("secondaryColor").and_then(|v| v.as_str()) {
            config.theme.secondary_color = val.to_string();
        }
        if let Some(val) = theme_vars.get("tertiaryColor").and_then(|v| v.as_str()) {
            config.theme.tertiary_color = val.to_string();
        }
        if let Some(val) = theme_vars
            .get("edgeLabelBackground")
            .and_then(|v| v.as_str())
        {
            config.theme.edge_label_background = val.to_string();
        }
        if let Some(val) = theme_vars.get("clusterBkg").and_then(|v| v.as_str()) {
            config.theme.cluster_background = val.to_string();
        }
        if let Some(val) = theme_vars.get("clusterBorder").and_then(|v| v.as_str()) {
            config.theme.cluster_border = val.to_string();
        }
        if let Some(val) = theme_vars.get("background").and_then(|v| v.as_str()) {
            config.theme.background = val.to_string();
        }
        if let Some(val) = theme_vars.get("fontFamily").and_then(|v| v.as_str()) {
            config.theme.font_family = val.to_string();
        }
        if let Some(val) = theme_vars.get("fontSize").and_then(|v| v.as_f64()) {
            config.theme.font_size = val as f32;
        }
    }
    if let Some(flowchart) = init.get("flowchart") {
        if let Some(val) = flowchart.get("nodeSpacing").and_then(|v| v.as_f64()) {
            config.layout.node_spacing = val as f32;
        }
        if let Some(val) = flowchart.get("rankSpacing").and_then(|v| v.as_f64()) {
            config.layout.rank_spacing = val as f32;
        }
    }
    config.render.background = config.theme.background.clone();
    config
}
