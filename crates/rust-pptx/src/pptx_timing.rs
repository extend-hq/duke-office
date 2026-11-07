use super::*;
use crate::animation_model::*;
use quick_xml::{
    name::{QName, ResolveResult},
    NsReader,
};

const PML: &str = "http://schemas.openxmlformats.org/presentationml/2006/main";
const PML_STRICT: &str = "http://purl.oclc.org/ooxml/presentationml/main";
const DML: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const DML_STRICT: &str = "http://purl.oclc.org/ooxml/drawingml/main";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const REL_STRICT: &str = "http://purl.oclc.org/ooxml/officeDocument/relationships";
const MC: &str = "http://schemas.openxmlformats.org/markup-compatibility/2006";
const P14: &str = "http://schemas.microsoft.com/office/powerpoint/2010/main";
const P15: &str = "http://schemas.microsoft.com/office/powerpoint/2012/main";
const MORPH: &str = "http://schemas.microsoft.com/office/powerpoint/2015/09/main";

fn supported_namespace(ns: &str) -> bool {
    matches!(
        ns,
        PML | PML_STRICT | DML | DML_STRICT | REL | REL_STRICT | P14 | P15 | MORPH
    )
}

#[derive(Debug)]
struct Element {
    name: String,
    namespace: String,
    attributes: BTreeMap<String, String>,
    children: Vec<Element>,
    text: String,
    supported_choice: bool,
}

impl Element {
    fn is_pml(&self) -> bool {
        matches!(self.namespace.as_str(), PML | PML_STRICT)
    }
    fn is_dml(&self) -> bool {
        matches!(self.namespace.as_str(), DML | DML_STRICT)
    }
    fn attr(&self, name: &str) -> Option<&str> {
        self.attributes.get(name).map(String::as_str)
    }
    fn string(&self, name: &str) -> Option<String> {
        self.attr(name).map(str::to_owned)
    }
    fn uint(&self, name: &str) -> Option<u32> {
        self.attr(name)?.parse().ok()
    }
    fn int(&self, name: &str) -> Option<i32> {
        self.attr(name)?.parse().ok()
    }
    fn boolean(&self, name: &str) -> Option<bool> {
        self.attr(name).and_then(bool_value)
    }
    fn fraction(&self, name: &str) -> Option<f64> {
        self.attr(name).and_then(fraction)
    }
    fn child(&self, name: &str) -> Option<&Self> {
        self.children.iter().find(|c| c.is_pml() && c.name == name)
    }
    fn children<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Self> {
        self.children
            .iter()
            .filter(move |c| c.is_pml() && c.name == name)
    }
    fn rel_id(&self) -> Option<String> {
        [REL, REL_STRICT]
            .iter()
            .find_map(|ns| self.attributes.get(&format!("{{{ns}}}embed")))
            .cloned()
    }
    fn as_color_node(&self) -> XmlDocument {
        XmlDocument::from_owned(self.owned_color_node())
    }

    fn owned_color_node(&self) -> OwnedNode {
        OwnedNode {
            name: self.name.clone(),
            attributes: self
                .attributes
                .iter()
                .filter(|(k, _)| !k.starts_with('{'))
                .map(|(k, v)| {
                    let value = v
                        .strip_suffix('%')
                        .and_then(finite)
                        .map(|v| (v * 1000.0).to_string())
                        .unwrap_or_else(|| v.clone());
                    (k.clone(), value)
                })
                .collect(),
            text: self.text.clone(),
            children: self
                .children
                .iter()
                .filter(|n| n.is_dml())
                .map(Self::owned_color_node)
                .collect(),
            source_xml: None,
        }
    }
}

fn namespace(result: ResolveResult<'_>) -> String {
    match result {
        ResolveResult::Bound(ns) => String::from_utf8_lossy(ns.as_ref()).into_owned(),
        ResolveResult::Unbound => String::new(),
        ResolveResult::Unknown(prefix) => format!("unbound:{}", String::from_utf8_lossy(&prefix)),
    }
}

fn read_source(source: &str, limits: &ParseLimits) -> Result<Element, ParseError> {
    let mut reader = NsReader::from_str(source);
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let mut budget = XmlBudget::default();
    loop {
        let event = reader
            .read_event()
            .map_err(|e| ParseError::Corrupt(format!("animation XML: {e}")))?;
        match event {
            Event::Start(ref start) | Event::Empty(ref start) => {
                if stack.len() > limits.max_xml_depth {
                    return Err(ParseError::ResourceLimit(
                        "animation XML nesting depth".into(),
                    ));
                }
                budget.consume_node(start, limits)?;
                let (ns, local) = reader.resolve_element(start.name());
                let mut node = Element {
                    name: String::from_utf8_lossy(local.as_ref()).into_owned(),
                    namespace: namespace(ns),
                    attributes: BTreeMap::new(),
                    children: Vec::new(),
                    text: String::new(),
                    supported_choice: false,
                };
                for a in start.attributes().with_checks(false).flatten() {
                    if a.key.as_ref() == b"xmlns" || a.key.as_ref().starts_with(b"xmlns:") {
                        continue;
                    }
                    let (ns, local) = reader.resolve_attribute(a.key);
                    let ns = namespace(ns);
                    let name = String::from_utf8_lossy(local.as_ref());
                    let key = if ns.is_empty() {
                        name.into_owned()
                    } else {
                        format!("{{{ns}}}{name}")
                    };
                    let value = a
                        .decode_and_unescape_value(reader.decoder())
                        .map_err(|e| ParseError::Corrupt(format!("animation attribute: {e}")))?;
                    node.attributes.insert(key, value.into_owned());
                }
                if node.namespace == MC && node.name == "Choice" {
                    node.supported_choice = node
                        .attr("Requires")
                        .filter(|v| !v.trim().is_empty())
                        .is_some_and(|requires| {
                            requires.split_whitespace().all(|prefix| {
                                let name = format!("{prefix}:required");
                                let (ns, _) = reader.resolve_element(QName(name.as_bytes()));
                                supported_namespace(&namespace(ns))
                            })
                        });
                }
                if matches!(event, Event::Start(_)) {
                    stack.push(node);
                } else if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    root = Some(node);
                }
            }
            Event::End(_) => {
                let node = stack
                    .pop()
                    .ok_or_else(|| ParseError::Corrupt("unmatched animation closing tag".into()))?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(node);
                } else {
                    root = Some(node);
                }
            }
            Event::Text(t) => {
                if let Some(node) = stack.last_mut() {
                    let text = t
                        .unescape()
                        .map_err(|e| ParseError::Corrupt(format!("animation text: {e}")))?;
                    budget.consume_text(text.len(), limits)?;
                    node.text.push_str(&text);
                }
            }
            Event::CData(t) => {
                if let Some(node) = stack.last_mut() {
                    budget.consume_text(t.len(), limits)?;
                    node.text.push_str(&String::from_utf8_lossy(&t));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut root = root.ok_or_else(|| ParseError::Corrupt("missing animation XML root".into()))?;
    select_branches(&mut root);
    Ok(root)
}

fn select_branches(node: &mut Element) {
    let mut selected = Vec::new();
    for mut child in std::mem::take(&mut node.children) {
        if child.namespace == MC && child.name == "AlternateContent" {
            let index = child
                .children
                .iter()
                .position(|n| n.namespace == MC && n.name == "Choice" && n.supported_choice)
                .or_else(|| {
                    child
                        .children
                        .iter()
                        .position(|n| n.namespace == MC && n.name == "Fallback")
                });
            if let Some(index) = index {
                let mut branch = child.children.remove(index);
                select_branches(&mut branch);
                selected.extend(branch.children);
            }
        } else {
            select_branches(&mut child);
            selected.push(child);
        }
    }
    node.children = selected;
}

fn finite(value: &str) -> Option<f64> {
    value.parse::<f64>().ok().filter(|v| v.is_finite())
}
fn fraction(value: &str) -> Option<f64> {
    if let Some(value) = value.strip_suffix('%') {
        finite(value).map(|v| v / 100.0)
    } else {
        finite(value).map(|v| v / 100_000.0)
    }
}
fn quantity(value: &str, scale: f64) -> Option<AnimationNumber> {
    if value == "indefinite" {
        Some(AnimationNumber::Indefinite(AnimationIndefinite::Indefinite))
    } else {
        value
            .parse::<u32>()
            .ok()
            .map(|v| AnimationNumber::Number(f64::from(v) / scale))
    }
}
fn time(node: &Element, attr: &str) -> Option<AnimationNumber> {
    node.attr(attr).and_then(|s| quantity(s, 1.0))
}
fn sound(
    node: &Element,
    rels: &Relationships,
    assets: &BTreeMap<String, PresentationAsset>,
) -> AnimationSound {
    let relationship_id = node.rel_id();
    let target = relationship_id
        .as_ref()
        .and_then(|id| rels.get(id))
        .cloned();
    let external = relationship_id
        .as_ref()
        .is_some_and(|id| rels.is_external(id));
    let asset_id = target
        .as_ref()
        .filter(|t| !external && assets.contains_key(*t))
        .cloned();
    AnimationSound {
        relationship_id,
        target,
        asset_id,
        external,
        name: node.string("name"),
        builtin: node.boolean("builtIn"),
    }
}

pub(super) fn parse(
    root: XmlNode<'_>,
    rels: &Relationships,
    assets: &BTreeMap<String, PresentationAsset>,
    colors: Option<&ColorContext<'_>>,
    limits: &ParseLimits,
) -> Result<(Option<Box<SlideTransition>>, Option<Box<SlideTiming>>), ParseError> {
    let mut transition = None;
    let mut timing = None;
    for candidate in root.children() {
        if !matches!(
            candidate.name(),
            "transition" | "timing" | "AlternateContent"
        ) {
            continue;
        }
        let Some(source) = candidate.source_xml() else {
            continue;
        };
        let scoped = read_source(source, limits)?;
        for node in &scoped.children {
            if !node.is_pml() {
                continue;
            }
            if node.name == "transition" && transition.is_none() {
                transition = Some(Box::new(parse_transition(node, source, rels, assets)));
            } else if node.name == "timing" && timing.is_none() {
                let context = TimingContext {
                    rels,
                    assets,
                    colors,
                };
                let mut unsupported = Vec::new();
                collect_unsupported(node, &mut unsupported);
                unsupported.sort_unstable();
                unsupported.dedup();
                timing = Some(Box::new(SlideTiming {
                    nodes: node
                        .child("tnLst")
                        .map(|n| parse_nodes(n, &context))
                        .unwrap_or_default(),
                    builds: node
                        .child("bldLst")
                        .map(|n| {
                            n.children
                                .iter()
                                .filter(|n| n.is_pml())
                                .map(|n| parse_build(n, &context))
                                .collect()
                        })
                        .unwrap_or_default(),
                    unsupported,
                    source_xml: source.to_owned(),
                }));
            }
        }
    }
    Ok((transition, timing))
}

fn parse_transition(
    node: &Element,
    source: &str,
    rels: &Relationships,
    assets: &BTreeMap<String, PresentationAsset>,
) -> SlideTransition {
    let effect = node
        .children
        .iter()
        .find(|n| !(n.is_pml() && matches!(n.name.as_str(), "sndAc" | "extLst")))
        .map(|n| {
            let default_dir = if n.is_pml() {
                match n.name.as_str() {
                    "blinds" | "checker" | "comb" | "randomBar" => Some("horz"),
                    "cover" | "pull" | "push" | "wipe" => Some("l"),
                    "strips" => Some("lu"),
                    "split" | "zoom" => Some("out"),
                    _ => None,
                }
            } else {
                None
            };
            TransitionEffect {
                kind: n.name.clone(),
                namespace: n.namespace.clone(),
                direction: n.string("dir").or_else(|| default_dir.map(str::to_owned)),
                orientation: n
                    .string("orient")
                    .or_else(|| (n.is_pml() && n.name == "split").then(|| "horz".into())),
                through_black: n.boolean("thruBlk").or_else(|| {
                    (n.is_pml() && matches!(n.name.as_str(), "cut" | "fade")).then_some(false)
                }),
                spokes: n
                    .uint("spokes")
                    .or_else(|| (n.is_pml() && n.name == "wheel").then_some(4)),
                parameters: n.attributes.clone(),
            }
        });
    let sound = node.child("sndAc").and_then(|a| {
        if a.child("endSnd").is_some() {
            Some(TransitionSound {
                action: "stop".into(),
                loop_sound: false,
                source: None,
            })
        } else {
            a.child("stSnd").map(|n| TransitionSound {
                action: "start".into(),
                loop_sound: n.boolean("loop").unwrap_or(false),
                source: n.child("snd").map(|n| sound(n, rels, assets)),
            })
        }
    });
    SlideTransition {
        effect,
        speed: node.string("spd").unwrap_or_else(|| "fast".into()),
        duration_ms: node
            .attr(&format!("{{{P14}}}dur"))
            .and_then(|n| n.parse().ok()),
        advance_on_click: node.boolean("advClick").unwrap_or(true),
        advance_after_ms: node.uint("advTm"),
        sound,
        source_xml: source.into(),
    }
}

struct TimingContext<'a, 'b> {
    rels: &'a Relationships,
    assets: &'a BTreeMap<String, PresentationAsset>,
    colors: Option<&'a ColorContext<'b>>,
}

fn collect_unsupported(node: &Element, out: &mut Vec<String>) {
    const NAMES: &[&str] = &[
        "timing",
        "tnLst",
        "par",
        "seq",
        "excl",
        "anim",
        "animClr",
        "animEffect",
        "animMotion",
        "animRot",
        "animScale",
        "cmd",
        "set",
        "audio",
        "video",
        "cTn",
        "stCondLst",
        "endCondLst",
        "endSync",
        "iterate",
        "childTnLst",
        "subTnLst",
        "cond",
        "tgtEl",
        "tn",
        "rtn",
        "tmAbs",
        "tmPct",
        "sldTgt",
        "sndTgt",
        "spTgt",
        "inkTgt",
        "bg",
        "subSp",
        "oleChartEl",
        "txEl",
        "graphicEl",
        "charRg",
        "pRg",
        "cBhvr",
        "attrNameLst",
        "attrName",
        "tavLst",
        "tav",
        "val",
        "boolVal",
        "intVal",
        "fltVal",
        "strVal",
        "clrVal",
        "by",
        "from",
        "to",
        "rgb",
        "hsl",
        "progress",
        "rCtr",
        "prevCondLst",
        "nextCondLst",
        "cMediaNode",
        "bldLst",
        "bldP",
        "bldDgm",
        "bldOleChart",
        "bldGraphic",
        "bldAsOne",
        "bldSub",
        "tmplLst",
        "tmpl",
    ];
    if (node.is_pml() && !NAMES.contains(&node.name.as_str())) || (!node.is_pml() && !node.is_dml())
    {
        out.push(format!("{{{}}}{}", node.namespace, node.name));
    }
    for child in &node.children {
        collect_unsupported(child, out);
    }
}

fn parse_target(node: &Element, cx: &TimingContext<'_, '_>) -> Option<AnimationTarget> {
    let n = node.children.iter().find(|n| n.is_pml())?;
    let mut target = AnimationTarget {
        kind: n.name.clone(),
        shape_id: n.string("spid"),
        sub_shape_id: None,
        background: None,
        text: None,
        chart: None,
        diagram: None,
        sound: None,
    };
    if n.name == "sndTgt" {
        target.sound = Some(sound(n, cx.rels, cx.assets));
    }
    if let Some(n) = n.child("subSp") {
        target.sub_shape_id = n.string("spid");
    }
    if n.child("bg").is_some() {
        target.background = Some(true);
    }
    if let Some(n) = n.child("txEl") {
        target.text = Some(
            if let Some(r) = n
                .children
                .iter()
                .find(|r| r.is_pml() && matches!(r.name.as_str(), "charRg" | "pRg"))
            {
                AnimationTextRange {
                    kind: if r.name == "pRg" {
                        "paragraph"
                    } else {
                        "character"
                    }
                    .into(),
                    start: r.uint("st"),
                    end: r.uint("end"),
                }
            } else {
                AnimationTextRange {
                    kind: "all".into(),
                    start: None,
                    end: None,
                }
            },
        );
    }
    if let Some(n) = n.child("oleChartEl") {
        target.chart = Some(AnimationChartTarget {
            kind: n.string("type").unwrap_or_default(),
            level: Some(n.uint("lvl").unwrap_or(0)),
            series_index: None,
            category_index: None,
        });
    }
    if let Some(n) = n
        .child("graphicEl")
        .and_then(|n| n.children.iter().find(|n| n.is_dml()))
    {
        if n.name == "chart" {
            target.chart = Some(AnimationChartTarget {
                kind: n.string("bldStep").unwrap_or_default(),
                level: None,
                series_index: Some(n.int("seriesIdx").unwrap_or(-1)),
                category_index: Some(n.int("categoryIdx").unwrap_or(-1)),
            });
        } else if n.name == "dgm" {
            target.diagram = Some(AnimationDiagramTarget {
                id: Some(
                    n.string("id")
                        .unwrap_or_else(|| "{00000000-0000-0000-0000-000000000000}".into()),
                ),
                build_step: Some(n.string("bldStep").unwrap_or_else(|| "sp".into())),
            });
        }
    }
    Some(target)
}

fn condition(n: &Element, cx: &TimingContext<'_, '_>) -> AnimationCondition {
    AnimationCondition {
        event: n.string("evt"),
        delay_ms: time(n, "delay"),
        target: n.child("tgtEl").and_then(|n| parse_target(n, cx)),
        time_node_id: n.child("tn").and_then(|n| n.uint("val")),
        runtime_node: n.child("rtn").and_then(|n| n.string("val")),
    }
}
fn conditions(n: &Element, name: &str, cx: &TimingContext<'_, '_>) -> Vec<AnimationCondition> {
    n.child(name)
        .map(|n| n.children("cond").map(|n| condition(n, cx)).collect())
        .unwrap_or_default()
}
fn common_time(n: &Element, cx: &TimingContext<'_, '_>) -> AnimationTime {
    AnimationTime {
        id: n.uint("id"),
        preset_id: n.int("presetID"),
        preset_class: n.string("presetClass"),
        preset_subtype: n.int("presetSubtype"),
        duration_ms: time(n, "dur"),
        repeat_count: n
            .attr("repeatCount")
            .and_then(|n| quantity(n, 1000.0))
            .unwrap_or(AnimationNumber::Number(1.0)),
        repeat_duration_ms: time(n, "repeatDur"),
        speed: n.fraction("spd").unwrap_or(1.0),
        acceleration: n.fraction("accel").unwrap_or(0.0),
        deceleration: n.fraction("decel").unwrap_or(0.0),
        auto_reverse: n.boolean("autoRev").unwrap_or(false),
        restart: n.string("restart"),
        fill: n.string("fill"),
        sync_behavior: n.string("syncBehavior"),
        time_filter: n.string("tmFilter"),
        event_filter: n.string("evtFilter"),
        display: n.boolean("display"),
        master_relation: n.string("masterRel"),
        build_level: n.int("bldLvl"),
        group_id: n.uint("grpId"),
        after_effect: n.boolean("afterEffect"),
        node_type: n.string("nodeType"),
        placeholder: n.boolean("nodePh"),
        start_conditions: conditions(n, "stCondLst", cx),
        end_conditions: conditions(n, "endCondLst", cx),
        end_sync: n.child("endSync").map(|n| condition(n, cx)),
        iterate: n.child("iterate").map(|n| AnimationIteration {
            kind: n.string("type").unwrap_or_else(|| "el".into()),
            backwards: n.boolean("backwards").unwrap_or(false),
            interval_ms: n.child("tmAbs").and_then(|n| time(n, "val")),
            interval_fraction: n.child("tmPct").and_then(|n| n.fraction("val")),
        }),
        children: n
            .child("childTnLst")
            .map(|n| parse_nodes(n, cx))
            .unwrap_or_default(),
        sub_nodes: n
            .child("subTnLst")
            .map(|n| parse_nodes(n, cx))
            .unwrap_or_default(),
    }
}
fn behavior(n: &Element, cx: &TimingContext<'_, '_>) -> AnimationBehavior {
    AnimationBehavior {
        target: n.child("tgtEl").and_then(|n| parse_target(n, cx)),
        attribute_names: n
            .child("attrNameLst")
            .map(|n| n.children("attrName").map(|n| n.text.clone()).collect())
            .unwrap_or_default(),
        additive: n.string("additive"),
        accumulate: n.string("accumulate"),
        transform_type: n.string("xfrmType"),
        from: n.string("from"),
        to: n.string("to"),
        by: n.string("by"),
        runtime_context: n.string("rctx"),
        override_mode: n.string("override"),
    }
}
fn point(n: &Element) -> Option<AnimationPoint> {
    Some(AnimationPoint {
        x: n.fraction("x")?,
        y: n.fraction("y")?,
    })
}
fn color(n: &Element, cx: &TimingContext<'_, '_>) -> Option<AnimationValue> {
    n.children
        .iter()
        .find(|n| n.is_dml())
        .and_then(|n| parse_color(n.as_color_node().root(), cx.colors))
        .map(AnimationValue::Color)
}
fn variant(n: &Element, cx: &TimingContext<'_, '_>) -> Option<AnimationValue> {
    let v = n.children.iter().find(|n| n.is_pml())?;
    match v.name.as_str() {
        "boolVal" => v.boolean("val").map(AnimationValue::Boolean),
        "intVal" => v.int("val").map(AnimationValue::Integer),
        "fltVal" => v.attr("val").and_then(finite).map(AnimationValue::Number),
        "strVal" => v.string("val").map(AnimationValue::String),
        "clrVal" => color(v, cx),
        _ => None,
    }
}
fn parse_nodes(n: &Element, cx: &TimingContext<'_, '_>) -> Vec<AnimationNode> {
    n.children
        .iter()
        .filter(|n| n.is_pml())
        .map(|n| parse_node(n, cx))
        .collect()
}
fn parse_node(n: &Element, cx: &TimingContext<'_, '_>) -> AnimationNode {
    let b = n.child("cBhvr");
    let media = n.child("cMediaNode");
    let t = n
        .child("cTn")
        .or_else(|| b.and_then(|n| n.child("cTn")))
        .or_else(|| media.and_then(|n| n.child("cTn")));
    let mut p = AnimationParameters::default();
    match n.name.as_str() {
        "anim" => {
            p.from = n.string("from").map(AnimationValue::String);
            p.to = n.string("to").map(AnimationValue::String);
            p.by = n.string("by").map(AnimationValue::String);
            p.calculation_mode = n.string("calcmode");
            p.value_type = n.string("valueType");
            p.keyframes = n
                .child("tavLst")
                .map(|n| {
                    n.children("tav")
                        .map(|n| AnimationKeyframe {
                            time: n
                                .attr("tm")
                                .and_then(|s| {
                                    if s == "indefinite" {
                                        quantity(s, 1.0)
                                    } else {
                                        fraction(s).map(AnimationNumber::Number)
                                    }
                                })
                                .unwrap_or(AnimationNumber::Indefinite(
                                    AnimationIndefinite::Indefinite,
                                )),
                            formula: n.string("fmla").unwrap_or_default(),
                            value: n.child("val").and_then(|n| variant(n, cx)),
                        })
                        .collect()
                })
                .unwrap_or_default();
        }
        "animClr" => {
            p.from = n.child("from").and_then(|n| color(n, cx));
            p.to = n.child("to").and_then(|n| color(n, cx));
            p.by = n
                .child("by")
                .and_then(|n| n.children.iter().find(|n| n.is_pml()))
                .and_then(|n| match n.name.as_str() {
                    "rgb" => Some(AnimationValue::RgbDelta([
                        n.fraction("r")?,
                        n.fraction("g")?,
                        n.fraction("b")?,
                    ])),
                    "hsl" => Some(AnimationValue::HslDelta([
                        finite(n.attr("h")?)? / 60000.0,
                        n.fraction("s")?,
                        n.fraction("l")?,
                    ])),
                    _ => None,
                });
            p.color_space = n.string("clrSpc");
            p.direction = n.string("dir");
        }
        "animEffect" => {
            p.transition = Some(n.string("transition").unwrap_or_else(|| "in".into()));
            p.filter = n.string("filter");
            p.properties = n.string("prLst");
            p.progress = n.child("progress").and_then(|n| variant(n, cx));
        }
        "animMotion" | "animScale" => {
            p.from = n.child("from").and_then(point).map(AnimationValue::Point);
            p.to = n.child("to").and_then(point).map(AnimationValue::Point);
            p.by = n.child("by").and_then(point).map(AnimationValue::Point);
            p.origin = n.string("origin");
            p.path = n.string("path");
            p.path_edit_mode = n.string("pathEditMode");
            p.rotation_center = n.child("rCtr").and_then(point);
            p.rotation_degrees = n.attr("rAng").and_then(finite).map(|n| n / 60000.0);
            p.point_types = n.string("ptsTypes");
            p.zoom_contents = n.boolean("zoomContents");
        }
        "animRot" => {
            p.from = n
                .attr("from")
                .and_then(finite)
                .map(|n| AnimationValue::Number(n / 60000.0));
            p.to = n
                .attr("to")
                .and_then(finite)
                .map(|n| AnimationValue::Number(n / 60000.0));
            p.by = n
                .attr("by")
                .and_then(finite)
                .map(|n| AnimationValue::Number(n / 60000.0));
        }
        "cmd" => {
            p.command_type = n.string("type");
            p.command = n.string("cmd");
        }
        "set" => {
            p.to = n.child("to").and_then(|n| variant(n, cx));
        }
        _ => {}
    }
    AnimationNode {
        kind: n.name.clone(),
        time: t.map(|n| Box::new(common_time(n, cx))),
        behavior: b.map(|n| behavior(n, cx)),
        sequence: (n.name == "seq").then(|| AnimationSequence {
            concurrent: n.boolean("concurrent"),
            previous_action: n.string("prevAc"),
            next_action: n.string("nextAc"),
            previous_conditions: conditions(n, "prevCondLst", cx),
            next_conditions: conditions(n, "nextCondLst", cx),
        }),
        media: media.map(|m| AnimationMedia {
            target: m.child("tgtEl").and_then(|n| parse_target(n, cx)),
            volume: m.fraction("vol").unwrap_or(0.5),
            mute: m.boolean("mute").unwrap_or(false),
            slide_count: m.uint("numSld").unwrap_or(1),
            show_when_stopped: m.boolean("showWhenStopped").unwrap_or(true),
            narration: (n.name == "audio").then(|| n.boolean("isNarration").unwrap_or(false)),
            full_screen: (n.name == "video").then(|| n.boolean("fullScrn").unwrap_or(false)),
        }),
        parameters: p,
    }
}
fn parse_build(n: &Element, cx: &TimingContext<'_, '_>) -> AnimationBuild {
    let paragraph = n.name == "bldP";
    let chart = n.name == "bldOleChart";
    AnimationBuild {
        kind: n.name.clone(),
        shape_id: n.string("spid"),
        group_id: n.uint("grpId"),
        ui_expand: n.boolean("uiExpand").unwrap_or(false),
        build: if paragraph {
            Some(n.string("build").unwrap_or_else(|| "whole".into()))
        } else if chart || n.name == "bldDgm" {
            Some(
                n.string("bld")
                    .unwrap_or_else(|| if chart { "allAtOnce" } else { "whole" }.into()),
            )
        } else {
            None
        },
        level: paragraph.then(|| n.uint("bldLvl").unwrap_or(1)),
        animate_background: (paragraph || chart).then(|| n.boolean("animBg").unwrap_or(chart)),
        auto_update_background: paragraph.then(|| n.boolean("autoUpdateAnimBg").unwrap_or(true)),
        reverse: paragraph.then(|| n.boolean("rev").unwrap_or(false)),
        advance_after_ms: paragraph.then(|| {
            time(n, "advAuto")
                .unwrap_or(AnimationNumber::Indefinite(AnimationIndefinite::Indefinite))
        }),
        as_one: (n.name == "bldGraphic").then(|| n.child("bldAsOne").is_some()),
        graphic_build: n
            .child("bldSub")
            .and_then(|n| n.children.iter().find(|n| n.is_dml()))
            .map(|n| AnimationGraphicBuild {
                kind: n.name.clone(),
                build: Some(n.string("bld").unwrap_or_else(|| "allAtOnce".into())),
                reverse: (n.name == "bldDgm").then(|| n.boolean("rev").unwrap_or(false)),
                animate_background: (n.name == "bldChart")
                    .then(|| n.boolean("animBg").unwrap_or(true)),
            }),
        templates: n
            .child("tmplLst")
            .map(|n| {
                n.children("tmpl")
                    .map(|n| AnimationTemplate {
                        level: n.uint("lvl").unwrap_or(0),
                        nodes: n
                            .child("tnLst")
                            .map(|n| parse_nodes(n, cx))
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default(),
    }
}
