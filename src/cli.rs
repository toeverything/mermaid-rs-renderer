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

    let (input, is_markdown) = read_input(args.input.as_deref())?;
    let diagrams = if is_markdown {
        extract_mermaid_blocks(&input)
    } else {
        vec![input]
    };

    if diagrams.is_empty() {
        return Err(anyhow::anyhow!("No Mermaid diagrams found in input"));
    }

    if diagrams.len() == 1 {
        let parsed = parse_mermaid(&diagrams[0])?;
        let mut config = base_config.clone();
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
        return Ok(());
    }

    // Multiple diagrams (Markdown input)
    let outputs = resolve_multi_outputs(args.output.as_deref(), args.output_format, diagrams.len())?;
    for (idx, diagram) in diagrams.iter().enumerate() {
        let parsed = parse_mermaid(diagram)?;
        let mut config = base_config.clone();
        if let Some(init_cfg) = parsed.init_config.clone() {
            config = merge_init_config(config, init_cfg);
        }
        let layout = compute_layout(&parsed.graph, &config.theme, &config.layout);
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
            .map(|ext| matches!(ext, "md" | "markdown"))
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
    Err(anyhow::anyhow!(
        "Output path required for {} output",
        ext
    ))
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
    let stem = base.file_stem().and_then(|s| s.to_str()).unwrap_or("diagram");
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    let mut outputs = Vec::new();
    for idx in 0..count {
        outputs.push(parent.join(format!("{}-{}.{}", stem, idx + 1, ext)));
    }
    Ok(outputs)
}

#[cfg(test)]
mod tests {
    use super::*;

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
