use crate::ColorValue;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A finite quantity or an indefinite duration/repetition. Percentages are fractions.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AnimationNumber {
    Number(f64),
    Indefinite(AnimationIndefinite),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnimationIndefinite {
    Indefinite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum AnimationValue {
    String(String),
    Boolean(bool),
    Integer(i32),
    Number(f64),
    Color(ColorValue),
    Point(AnimationPoint),
    RgbDelta([f64; 3]),
    HslDelta([f64; 3]),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideTransition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effect: Option<TransitionEffect>,
    pub speed: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    pub advance_on_click: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advance_after_ms: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<TransitionSound>,
    pub source_xml: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionEffect {
    pub kind: String,
    pub namespace: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub through_black: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spokes: Option<u32>,
    pub parameters: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransitionSound {
    pub action: String,
    pub loop_sound: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<AnimationSound>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationSound {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    pub external: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub builtin: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideTiming {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<AnimationNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub builds: Vec<AnimationBuild>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unsupported: Vec<String>,
    pub source_xml: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationNode {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time: Option<Box<AnimationTime>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub behavior: Option<AnimationBehavior>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<AnimationSequence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<AnimationMedia>,
    pub parameters: AnimationParameters,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationTime {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset_id: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset_class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset_subtype: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<AnimationNumber>,
    pub repeat_count: AnimationNumber,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_duration_ms: Option<AnimationNumber>,
    pub speed: f64,
    pub acceleration: f64,
    pub deceleration: f64,
    pub auto_reverse: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restart: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_behavior: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_relation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_level: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_effect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub placeholder: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub start_conditions: Vec<AnimationCondition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub end_conditions: Vec<AnimationCondition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_sync: Option<AnimationCondition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub iterate: Option<AnimationIteration>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<AnimationNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_nodes: Vec<AnimationNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationCondition {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<AnimationNumber>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<AnimationTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_node_id: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_node: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationIteration {
    pub kind: String,
    pub backwards: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_ms: Option<AnimationNumber>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_fraction: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationTarget {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Identifier scoped to the owning part, matching object source identity.
    pub shape_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub_shape_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<AnimationTextRange>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chart: Option<AnimationChartTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diagram: Option<AnimationDiagramTarget>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<AnimationSound>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationTextRange {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationChartTarget {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_index: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category_index: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationDiagramTarget {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build_step: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationBehavior {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<AnimationTarget>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attribute_names: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additive: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accumulate: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transform_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime_context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub override_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationSequence {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub concurrent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_action: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub previous_conditions: Vec<AnimationCondition>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub next_conditions: Vec<AnimationCondition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationMedia {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<AnimationTarget>,
    pub volume: f64,
    pub mute: bool,
    pub slide_count: u32,
    pub show_when_stopped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub narration: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub full_screen: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationParameters {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub from: Option<AnimationValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<AnimationValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub by: Option<AnimationValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub calculation_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_type: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keyframes: Vec<AnimationKeyframe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color_space: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub direction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub properties: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress: Option<AnimationValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path_edit_mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_center: Option<AnimationPoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation_degrees: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub point_types: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zoom_contents: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationPoint {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationKeyframe {
    pub time: AnimationNumber,
    pub formula: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<AnimationValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationBuild {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    /// Identifier scoped to the owning part, matching object source identity.
    pub shape_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_id: Option<u32>,
    pub ui_expand: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animate_background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_update_background: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advance_after_ms: Option<AnimationNumber>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub as_one: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub graphic_build: Option<AnimationGraphicBuild>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub templates: Vec<AnimationTemplate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationGraphicBuild {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub build: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reverse: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animate_background: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnimationTemplate {
    pub level: u32,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<AnimationNode>,
}
