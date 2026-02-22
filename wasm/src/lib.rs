use mermaid_rs_renderer::{RenderOptions, render_with_options};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MermaidRenderOptions {
    theme: Option<String>,
    font_family: Option<String>,
    font_size: Option<f32>,
    fast_text: Option<bool>,
    svg_only: Option<bool>,
}

fn build_render_options(options: MermaidRenderOptions) -> RenderOptions {
    let mut render_options = if options.theme.as_deref() == Some("default") {
        RenderOptions::mermaid_default()
    } else {
        RenderOptions::modern()
    };

    if let Some(font_family) = options.font_family {
        render_options.theme.font_family = font_family;
    }
    if let Some(font_size) = options.font_size {
        render_options.theme.font_size = font_size;
    }

    let _ = options.fast_text;
    let _ = options.svg_only;

    render_options
}

#[wasm_bindgen]
pub fn render_mermaid_svg(code: &str, options_json: Option<String>) -> Result<String, JsValue> {
    let options = if let Some(raw_options) = options_json {
        serde_json::from_str::<MermaidRenderOptions>(&raw_options)
            .map_err(|error| JsValue::from_str(&error.to_string()))?
    } else {
        MermaidRenderOptions::default()
    };

    let render_options = build_render_options(options);
    render_with_options(code, render_options).map_err(|error| JsValue::from_str(&error.to_string()))
}

#[cfg(test)]
mod tests {
    use mermaid_rs_renderer::render_with_options;

    use crate::{MermaidRenderOptions, build_render_options};

    #[test]
    fn renders_flowchart_with_edge_labels_and_subgraph() {
        let code = r#"flowchart TD
    A[Start] --> B{Decide}
    B -->|yes| C[Do work]
    C --> D{More?}
    D -->|yes| B
    D -->|no| E[End]

    subgraph LoopBlock
        direction LR
        F[One] --> G[Two]
        G --> H[Three]
        H --> F
    end

    E --> F"#;

        let svg = render_with_options(code, build_render_options(MermaidRenderOptions::default()))
            .expect("flowchart with edge labels should render");

        assert!(svg.contains("<svg"));
        assert!(svg.contains("yes"));
        assert!(svg.contains("no"));
    }
}
