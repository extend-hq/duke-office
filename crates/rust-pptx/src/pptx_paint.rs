use std::{collections::HashMap, rc::Rc};

use super::{BTreeMap, NodeParseContext, OfficeArtColors, ThemeDataInternal};
use rust_office_art::RenderedPicture;

#[derive(Eq, PartialEq, Hash)]
pub(super) struct ColorKey {
    theme: Option<String>,
    map: BTreeMap<String, String>,
}

impl ColorKey {
    pub(super) fn new(
        theme: Option<&ThemeDataInternal>,
        map: &BTreeMap<String, String>,
    ) -> Rc<Self> {
        Rc::new(Self {
            theme: theme.map(|theme| theme.id.clone()),
            map: map.clone(),
        })
    }

    fn bytes(&self) -> usize {
        self.theme.as_ref().map_or(0, String::len)
            + self
                .map
                .iter()
                .map(|(key, value)| key.len() + value.len() + 64)
                .sum::<usize>()
    }
}

#[derive(Eq, PartialEq, Hash)]
struct ShapeKey {
    properties: String,
    style: String,
    width: u64,
    height: u64,
    colors: Option<Rc<ColorKey>>,
    relationships: Vec<(String, String)>,
    assets: usize,
}

impl ShapeKey {
    fn bytes(&self) -> usize {
        self.properties.len()
            + self.style.len()
            + self.colors.as_ref().map_or(0, |colors| colors.bytes())
            + self
                .relationships
                .iter()
                .map(|(key, value)| key.len() + value.len() + 64)
                .sum::<usize>()
            + 256
    }
}

#[derive(Default)]
pub(super) struct ShapeCache {
    entries: HashMap<ShapeKey, RenderedPicture>,
    bytes: usize,
}

pub(super) fn render_shape(
    properties: String,
    style: String,
    width: f64,
    height: f64,
    context: &NodeParseContext<'_, '_>,
) -> Option<RenderedPicture> {
    let mut relationships = context
        .rels
        .iter()
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect::<Vec<_>>();
    relationships.sort_unstable();
    let key = ShapeKey {
        properties,
        style,
        width: width.to_bits(),
        height: height.to_bits(),
        colors: context.colors.map(|colors| Rc::clone(&colors.paint_key)),
        relationships,
        assets: context.assets.len(),
    };
    if let Some(paint) = context.shape_cache.borrow().entries.get(&key) {
        return Some(paint.clone());
    }
    let warning_count = context.diagnostics.warnings.borrow().len();
    let color_warning_count = context
        .colors
        .map(|colors| colors.diagnostics.warnings.borrow().len());
    let paint = rust_office_art::render_shape(
        &key.properties,
        &key.style,
        width,
        height,
        &OfficeArtColors {
            colors: context.colors,
            limits: context.limits,
            assets: context.assets,
            relationships: context.rels,
        },
    )?;
    if context.diagnostics.warnings.borrow().len() == warning_count
        && context
            .colors
            .map(|colors| colors.diagnostics.warnings.borrow().len())
            == color_warning_count
    {
        let bytes = key.bytes().saturating_add(paint.svg.len());
        let mut cache = context.shape_cache.borrow_mut();
        if bytes <= 1024 * 1024
            && cache.entries.len() < 4096
            && cache.bytes.saturating_add(bytes) <= 8 * 1024 * 1024
        {
            cache.entries.insert(key, paint.clone());
            cache.bytes += bytes;
        }
    }
    Some(paint)
}
