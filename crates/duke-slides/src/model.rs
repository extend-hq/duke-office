use std::collections::BTreeMap;

use crate::{SlideTiming, SlideTransition};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PictureArtwork {
    pub svg: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint_bounds: Option<duke_office_art::PaintBounds>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PresentationFormat {
    Pptx,
    Ppt,
    /// OpenDocument presentation, template or flat XML.
    Odp,
    /// OpenDocument drawing, template or flat XML.
    Odg,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSize {
    pub width_emu: u64,
    pub height_emu: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationWarning {
    pub code: String,
    pub message: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slide_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature: Option<String>,
}

impl PresentationWarning {
    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            severity: "warning".into(),
            slide_index: None,
            node_id: None,
            part_name: None,
            feature: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subject: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationAsset {
    pub id: String,
    pub content_type: String,
    pub byte_length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_bytes::ByteBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationEmbeddedFont {
    pub family: String,
    pub asset_id: String,
    pub style: String,
    pub weight: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    pub x: i64,
    pub y: i64,
    pub width: i64,
    pub height: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flip_horizontal: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flip_vertical: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "kebab-case",
    rename_all_fields = "camelCase"
)]
#[allow(clippy::large_enum_variant)]
pub enum SlideNode {
    Shape {
        #[serde(flatten, default)]
        metadata: Option<Box<NodeMetadata>>,
        id: String,
        name: String,
        transform: Transform,
        geometry: ShapeGeometry,
        #[serde(skip_serializing_if = "Option::is_none")]
        shape_svg: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        paint_bounds: Option<duke_office_art::PaintBounds>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_xml: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        fill: Option<FillStyle>,
        #[serde(skip_serializing_if = "Option::is_none")]
        line: Option<LineStyle>,
        paragraphs: Vec<TextParagraph>,
        #[serde(skip_serializing_if = "Option::is_none")]
        vertical_alignment: Option<VerticalAlignment>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text_insets: Option<TextInsets>,
        #[serde(skip_serializing_if = "Option::is_none")]
        autofit: Option<TextAutofit>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text_rotation: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        text_transform: Option<Transform>,
        #[serde(skip_serializing_if = "Option::is_none")]
        vertical_text: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        horizontal_overflow: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        vertical_overflow: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        text_wrap: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        column_count: Option<usize>,
        #[serde(skip_serializing_if = "Option::is_none")]
        column_spacing: Option<i64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        right_to_left_columns: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        space_first_last_paragraph: Option<bool>,
    },
    Image {
        #[serde(flatten, default)]
        metadata: Option<Box<NodeMetadata>>,
        #[serde(default, rename = "model3d", skip_serializing_if = "Option::is_none")]
        model_3d: Option<Box<duke_office_art::model3d::Model3D>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ink: Option<Box<InkContent>>,
        id: String,
        name: String,
        transform: Transform,
        asset_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fallback_asset_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        crop: Option<ImageCrop>,
        #[serde(skip_serializing_if = "Option::is_none")]
        opacity: Option<f64>,
        preserve_aspect_ratio: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        effects: Option<ImageEffects>,
        #[serde(skip_serializing_if = "Option::is_none")]
        picture_svg: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        fallback_picture: Option<Box<PictureArtwork>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        paint_bounds: Option<duke_office_art::PaintBounds>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_xml: Option<String>,
    },
    Group {
        #[serde(flatten, default)]
        metadata: Option<Box<NodeMetadata>>,
        id: String,
        name: String,
        transform: Transform,
        children: Vec<SlideNode>,
        #[serde(skip_serializing_if = "Option::is_none")]
        child_transform: Option<Transform>,
    },
    Table {
        #[serde(flatten, default)]
        metadata: Option<Box<NodeMetadata>>,
        id: String,
        name: String,
        transform: Transform,
        rows: Vec<Vec<TableCell>>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        column_widths: Vec<i64>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        row_heights: Vec<i64>,
    },
    Chart {
        #[serde(flatten, default)]
        metadata: Option<Box<NodeMetadata>>,
        id: String,
        name: String,
        transform: Transform,
        chart_type: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        series: Vec<ChartSeries>,
        #[serde(skip_serializing_if = "Option::is_none")]
        has_legend: Option<bool>,
        /// Raw DrawingML chart part XML (`c:chartSpace` or `cx:chartSpace`) so
        /// viewers can render with full PowerPoint fidelity.
        #[serde(skip_serializing_if = "Option::is_none")]
        chart_xml: Option<String>,
        /// Native chart data when the source format does not use DrawingML.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chart_data: Option<Box<crate::ChartData>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        chart_part_name: Option<String>,
        /// Chart artwork at the dimensions of the slide frame.
        #[serde(skip_serializing_if = "Option::is_none")]
        chart_svg: Option<String>,
        /// Optional companion chart style definition.
        #[serde(skip_serializing_if = "Option::is_none")]
        chart_style_xml: Option<String>,
        /// Optional companion chart color style definition.
        #[serde(skip_serializing_if = "Option::is_none")]
        chart_colors_xml: Option<String>,
    },
    Media {
        #[serde(flatten, default)]
        metadata: Option<Box<NodeMetadata>>,
        id: String,
        name: String,
        transform: Transform,
        media_type: MediaType,
        #[serde(skip_serializing_if = "Option::is_none")]
        asset_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_url: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        poster_asset_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        poster: Option<Box<SlideNode>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        playback: Option<MediaPlayback>,
        #[serde(skip_serializing_if = "Option::is_none")]
        embedded_object: Option<EmbeddedObject>,
        #[serde(skip_serializing_if = "Option::is_none")]
        source_xml: Option<String>,
    },
    Unknown {
        #[serde(flatten, default)]
        metadata: Option<Box<NodeMetadata>>,
        id: String,
        name: String,
        transform: Transform,
        feature: String,
    },
}

impl SlideNode {
    pub fn metadata_mut(&mut self) -> &mut NodeMetadata {
        match self {
            Self::Shape { metadata, .. }
            | Self::Image { metadata, .. }
            | Self::Group { metadata, .. }
            | Self::Table { metadata, .. }
            | Self::Chart { metadata, .. }
            | Self::Media { metadata, .. }
            | Self::Unknown { metadata, .. } => metadata.get_or_insert_with(Default::default),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InkContent {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_xml: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drawing: Option<Box<duke_office_art::InkDrawing>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub editing: Option<Box<ObjectEditing>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessibility_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decorative: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub click_action: Option<Box<ObjectAction>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_action: Option<Box<ObjectAction>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_part: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectEditing {
    pub kind: ObjectEditingKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locks: Option<ObjectLocks>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connector: Option<ConnectorConnections>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_box: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prefer_relative_resize: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_comment: Option<bool>,
    pub source_xml: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectEditingKind {
    Shape,
    Connector,
    Picture,
    Group,
    GraphicFrame,
    ContentPart,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectLocks {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_grouping: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_ungrouping: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_selection: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_rotation: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_change_aspect: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_move: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_resize: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_edit_points: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_adjust_handles: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_change_arrowheads: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_change_shape_type: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_text_edit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_crop: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_drilldown: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorConnections {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<ConnectorEndpoint>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<ConnectorEndpoint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectorEndpoint {
    pub shape_id: u32,
    pub connection_site: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectAction {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub external: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tooltip: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_frame: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalid_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub history: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highlight_click: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_sound: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound_asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound_builtin: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MediaType {
    Audio,
    Video,
    Ole,
    Control,
    Unknown,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaPlayback {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim_start_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trim_end_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fade_in_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fade_out_ms: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bookmarks: Vec<MediaBookmark>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaBookmark {
    pub name: String,
    pub time_ms: f64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedObject {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control: Option<Box<EmbeddedControl>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationship_id: Option<String>,
    pub linked: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub show_as_icon: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_width_emu: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_height_emu: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub follow_color_scheme: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_automatic: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddedControl {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub persistence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_xml: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub binary_asset_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub properties: Vec<ControlProperty>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlProperty {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCell {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge: Option<Box<TableCellMerge>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub row_span: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col_span: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill: Option<FillStyle>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub borders: BTreeMap<String, LineStyle>,
    pub paragraphs: Vec<TextParagraph>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_insets: Option<TextInsets>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertical_alignment: Option<VerticalAlignment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_rotation: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableCellMerge {
    pub horizontal: bool,
    pub vertical: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<TableCellPosition>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableCellPosition {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextInsets {
    pub top: i64,
    pub right: i64,
    pub bottom: i64,
    pub left: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageCrop {
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
    pub left: f64,
}

/// DrawingML picture color effects applied to an `a:blip`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageEffects {
    /// `a:biLevel` black/white threshold, normalized to 0..1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bi_level_threshold: Option<f64>,
    /// `a:grayscl` grayscale recolor.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grayscale: Option<bool>,
    /// `a:duotone` two-color ramp, dark then light.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duotone: Option<Vec<ColorValue>>,
    /// `a:lum` brightness, normalized to -1..1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub brightness: Option<f64>,
    /// `a:lum` contrast, normalized to -1..1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contrast: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChartSeries {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<Vec<serde_json::Value>>,
    pub values: Vec<Option<f64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorValue>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ShapeGeometry {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// SVG path data normalized to a 0..1 view box for DrawingML custom geometry.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorValue {
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GradientStop {
    pub position: f64,
    pub color: ColorValue,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FillImageMode {
    Stretch,
    Tile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum FillStyle {
    None,
    Solid {
        color: ColorValue,
    },
    Gradient {
        #[serde(skip_serializing_if = "Option::is_none")]
        angle: Option<f64>,
        stops: Vec<GradientStop>,
    },
    Pattern {
        preset: String,
        foreground: ColorValue,
        background: ColorValue,
    },
    Image {
        asset_id: String,
        mode: FillImageMode,
        #[serde(skip_serializing_if = "Option::is_none")]
        crop: Option<ImageCrop>,
        #[serde(skip_serializing_if = "Option::is_none")]
        opacity: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        effects: Option<ImageEffects>,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineStyle {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_arrow: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_arrow: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerticalAlignment {
    Top,
    Middle,
    Bottom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextAutoFitMode {
    None,
    Normal,
    Shape,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextAutofit {
    pub mode: TextAutoFitMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_scale: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_spacing_reduction: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextParagraph {
    pub runs: Vec<TextRun>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet: Option<TextBullet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_spacing: Option<TextSpacing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_before: Option<TextSpacing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_after: Option<TextSpacing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_left_emu: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent_emu: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_tab_size_emu: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_stops: Option<Vec<TextTabStop>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub east_asian_line_break: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latin_line_break: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hanging_punctuation: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextTabAlignment {
    Left,
    Center,
    Right,
    Decimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextTabStop {
    pub position_emu: i64,
    pub alignment: TextTabAlignment,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSpacing {
    pub value: f64,
    pub unit: TextSpacingUnit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextSpacingUnit {
    Points,
    Percent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextBullet {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size_pt: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_percent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_at: Option<usize>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRunProperties {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub east_asian_font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complex_script_font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size_pt: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternative_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_to_left: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_spacing_pt: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kerning_threshold_pt: Option<f64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextFieldParagraphProperties {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet: Option<TextBullet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line_spacing: Option<TextSpacing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_before: Option<TextSpacing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub space_after: Option<TextSpacing>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rtl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_left_emu: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indent_emu: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_tab_size_emu: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tab_stops: Option<Vec<TextTabStop>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub east_asian_line_break: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latin_line_break: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hanging_punctuation: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_right_emu: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_alignment: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet_none: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet_color: Option<ColorValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet_color_follow_text: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet_font_follow_text: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet_size_follow_text: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bullet_image_asset_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_run_properties: Option<TextRunProperties>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextField {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub field_type: Option<String>,
    pub has_cached_text: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paragraph_properties: Option<Box<TextFieldParagraphProperties>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_part: Option<String>,
    pub source_xml: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextRun {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub field: Option<Box<TextField>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub math: Option<Box<duke_office_art::OfficeMath>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub east_asian_font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complex_script_font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub symbol_font_family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_size_pt: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bold: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub underline: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strike: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorValue>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alternative_language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub right_to_left: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hyperlink: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub character_spacing_pt: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kerning_threshold_pt: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationTheme {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub colors: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub major_fonts: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub minor_fonts: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format_scheme_xml: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideMaster {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<Box<SlideTransition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<Box<SlideTiming>>,
    pub nodes: Vec<SlideNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideLayout {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<Box<SlideTransition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<Box<SlideTiming>>,
    pub nodes: Vec<SlideNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSlide {
    pub id: String,
    pub index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub layout_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<FillStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition: Option<Box<SlideTransition>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timing: Option<Box<SlideTiming>>,
    pub nodes: Vec<SlideNode>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub notes: Vec<SlideNote>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub comments: Vec<SlideComment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_part: Option<String>,
    pub warnings: Vec<PresentationWarning>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideNote {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paragraphs: Vec<TextParagraph>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideComment {
    pub id: String,
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paragraphs: Vec<TextParagraph>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replies: Vec<SlideComment>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modern: Option<Box<ModernComment>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_initials: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_index: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_part: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<SlideCommentPosition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub y: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SlideCommentPosition {
    pub x: i64,
    pub y: i64,
    pub unit: SlideCommentPositionUnit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SlideCommentPositionUnit {
    EighthPoint,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModernComment {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author_extensions_xml: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub anchors: Vec<CommentAnchor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_emu: Option<CommentAnchorPosition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task: Option<CommentTask>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions_xml: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentTask {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub due_date: Option<String>,
    pub assigned_to: Vec<String>,
    pub title: String,
    pub completion: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completion_fraction: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentAnchorPosition {
    pub x: f64,
    pub y: f64,
    pub source_x: String,
    pub source_y: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommentAnchor {
    pub kind: CommentAnchorKind,
    pub monikers: Vec<CommentMoniker>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommentAnchorKind {
    Slide,
    DrawingElement,
    TextRange,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentMoniker {
    pub namespace: String,
    pub name: String,
    pub attributes: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<CommentMoniker>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationDocument {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    pub format: PresentationFormat,
    pub size: PresentationSize,
    pub slides: Vec<PresentationSlide>,
    pub masters: Vec<SlideMaster>,
    pub layouts: Vec<SlideLayout>,
    pub themes: Vec<PresentationTheme>,
    pub assets: BTreeMap<String, PresentationAsset>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub embedded_fonts: Vec<PresentationEmbeddedFont>,
    pub warnings: Vec<PresentationWarning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<PresentationMetadata>,
}
