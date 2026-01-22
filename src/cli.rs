use crate::config::{load_config, Config};
use crate::layout::compute_layout;
use crate::parser::parse_mermaid;
use crate::render::{render_svg, write_output_png, write_output_svg};
use anyhow::Result;
use clap::{Parser, ValueEnum};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(name = "mmdr", version, about = "Mermaid renderer in Rust (flowchart subset)")]
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
}

#[derive(ValueEnum, Debug, Clone)]
pub enum OutputFormat {
    Svg,
    Png,
}

pub fn run() -> Result<()> {
    let args = Args::parse();
    let mut config = load_config(args.config.as_deref())?;
    config.render.width = args.width;
    config.render.height = args.height;

    let input = read_input(args.input.as_deref())?;
    let parsed = parse_mermaid(&input)?;

    if let Some(init_cfg) = parsed.init_config {
        config = merge_init_config(config, init_cfg);
    }

    let layout = compute_layout(&parsed.graph, &config.theme, &config.layout);
    let svg = render_svg(&layout, &config.theme, &config.layout);

    match args.output_format {
        OutputFormat::Svg => {
            write_output_svg(&svg, args.output.as_deref())?;
        }
        OutputFormat::Png => {
            let output = ensure_output(&args.output, "png")?;
            write_output_png(&svg, &output, &config.render, &config.theme)?;
        }
    }

    Ok(())
}

fn read_input(path: Option<&Path>) -> Result<String> {
    if let Some(path) = path {
        if path == Path::new("-") {
            let mut buf = String::new();
            io::stdin().read_to_string(&mut buf)?;
            return Ok(buf);
        }
        return Ok(std::fs::read_to_string(path)?);
    }

    let mut buf = String::new();
    io::stdin().read_to_string(&mut buf)?;
    Ok(buf)
}

fn ensure_output(output: &Option<PathBuf>, ext: &str) -> Result<PathBuf> {
    if let Some(path) = output {
        return Ok(path.clone());
    }
    Err(anyhow::anyhow!(
        "Output path required for {} output",
        ext
    ))
}

fn merge_init_config(mut config: Config, init: serde_json::Value) -> Config {
    if let Some(theme_vars) = init.get("themeVariables") {
        if let Some(val) = theme_vars.get("primaryColor").and_then(|v| v.as_str()) {
            config.theme.primary_color = val.to_string();
        }
        if let Some(val) = theme_vars.get("primaryTextColor").and_then(|v| v.as_str()) {
            config.theme.primary_text_color = val.to_string();
        }
        if let Some(val) = theme_vars.get("primaryBorderColor").and_then(|v| v.as_str()) {
            config.theme.primary_border_color = val.to_string();
        }
        if let Some(val) = theme_vars.get("lineColor").and_then(|v| v.as_str()) {
            config.theme.line_color = val.to_string();
        }
        if let Some(val) = theme_vars.get("fontFamily").and_then(|v| v.as_str()) {
            config.theme.font_family = val.to_string();
        }
        if let Some(val) = theme_vars.get("fontSize").and_then(|v| v.as_f64()) {
            config.theme.font_size = val as f32;
        }
    }
    config
}
