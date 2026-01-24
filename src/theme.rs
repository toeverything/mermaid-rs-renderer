use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Theme {
    pub font_family: String,
    pub font_size: f32,
    pub primary_color: String,
    pub primary_text_color: String,
    pub primary_border_color: String,
    pub line_color: String,
    pub secondary_color: String,
    pub tertiary_color: String,
    pub edge_label_background: String,
    pub cluster_background: String,
    pub cluster_border: String,
    pub background: String,
    pub sequence_actor_fill: String,
    pub sequence_actor_border: String,
    pub sequence_actor_line: String,
    pub sequence_note_fill: String,
    pub sequence_note_border: String,
    pub sequence_activation_fill: String,
    pub sequence_activation_border: String,
}

impl Theme {
    pub fn mermaid_default() -> Self {
        Self {
            font_family: "'trebuchet ms', verdana, arial, sans-serif".to_string(),
            font_size: 16.0,
            primary_color: "#ECECFF".to_string(),
            primary_text_color: "#333333".to_string(),
            primary_border_color: "#9370DB".to_string(),
            line_color: "#333333".to_string(),
            secondary_color: "#FFFFDE".to_string(),
            tertiary_color: "#ECECFF".to_string(),
            edge_label_background: "#E8E8E8".to_string(),
            cluster_background: "#FFFFDE".to_string(),
            cluster_border: "#AAAA33".to_string(),
            background: "#FFFFFF".to_string(),
            sequence_actor_fill: "#EAEAEA".to_string(),
            sequence_actor_border: "#666666".to_string(),
            sequence_actor_line: "#999999".to_string(),
            sequence_note_fill: "#FFF5AD".to_string(),
            sequence_note_border: "#AAAA33".to_string(),
            sequence_activation_fill: "#F4F4F4".to_string(),
            sequence_activation_border: "#666666".to_string(),
        }
    }

    pub fn modern() -> Self {
        Self {
            font_family: "Inter, Segoe UI, system-ui, -apple-system, sans-serif".to_string(),
            font_size: 13.0,
            primary_color: "#F8FAFF".to_string(),
            primary_text_color: "#1C2430".to_string(),
            primary_border_color: "#C7D2E5".to_string(),
            line_color: "#7A8AA6".to_string(),
            secondary_color: "#EEF2F8".to_string(),
            tertiary_color: "#FFFFFF".to_string(),
            edge_label_background: "#FFFFFF".to_string(),
            cluster_background: "#F7FAFF".to_string(),
            cluster_border: "#D7E0F0".to_string(),
            background: "#FFFFFF".to_string(),
            sequence_actor_fill: "#F8FAFF".to_string(),
            sequence_actor_border: "#C7D2E5".to_string(),
            sequence_actor_line: "#7A8AA6".to_string(),
            sequence_note_fill: "#F7FAFF".to_string(),
            sequence_note_border: "#D7E0F0".to_string(),
            sequence_activation_fill: "#EEF2F8".to_string(),
            sequence_activation_border: "#7A8AA6".to_string(),
        }
    }
}
