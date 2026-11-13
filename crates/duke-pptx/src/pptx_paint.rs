use std::{
    collections::{hash_map::RandomState, HashMap},
    hash::{BuildHasher, BuildHasherDefault, Hasher},
    rc::Rc,
    sync::{Arc, OnceLock},
};

use super::{BTreeMap, NodeParseContext, OfficeArtColors, ThemeDataInternal};
use duke_office_art::RenderedPicture;

/// One keyed hasher per process, so hashes memoized on color and relationship keys
/// stay valid for every shape cache.
fn hash_state() -> &'static RandomState {
    static STATE: OnceLock<RandomState> = OnceLock::new();
    STATE.get_or_init(RandomState::new)
}

#[derive(Eq, PartialEq)]
pub(super) struct ColorKey {
    theme: Option<String>,
    map: BTreeMap<String, String>,
    hash: u64,
}

impl ColorKey {
    pub(super) fn new(
        theme: Option<&ThemeDataInternal>,
        map: &BTreeMap<String, String>,
    ) -> Rc<Self> {
        let theme = theme.map(|theme| theme.id.clone());
        let hash = hash_state().hash_one((&theme, map));
        Rc::new(Self {
            theme,
            map: map.clone(),
            hash,
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

/// The sorted `(id, target)` pairs of one relationship part with their keyed hash.
#[derive(Clone, Debug)]
pub(super) struct RelationshipKey {
    pairs: Arc<[(String, String)]>,
    hash: u64,
}

impl RelationshipKey {
    pub(super) fn new<'a>(pairs: impl Iterator<Item = (&'a String, &'a String)>) -> Self {
        let mut pairs = pairs
            .map(|(id, target)| (id.clone(), target.clone()))
            .collect::<Vec<_>>();
        pairs.sort_unstable();
        let hash = hash_state().hash_one(&pairs);
        Self {
            pairs: pairs.into(),
            hash,
        }
    }
}

struct ShapeKey {
    properties: String,
    style: String,
    width: u64,
    height: u64,
    colors: Option<Rc<ColorKey>>,
    relationships: Arc<[(String, String)]>,
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

/// Bucket keys are already keyed hashes, so the map uses them as they are.
#[derive(Default)]
struct Prehashed(u64);

impl Hasher for Prehashed {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(*byte);
        }
    }

    fn write_u64(&mut self, value: u64) {
        self.0 = value;
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// Rendered shapes by the keyed hash of their key. A lookup hashes the appearance XML
/// once and compares borrowed data; the owned key is built only when a shape is stored.
#[derive(Default)]
pub(super) struct ShapeCache {
    entries: HashMap<u64, Vec<(ShapeKey, RenderedPicture)>, BuildHasherDefault<Prehashed>>,
    count: usize,
    bytes: usize,
}

pub(super) fn render_shape(
    properties: String,
    style: String,
    width: f64,
    height: f64,
    context: &NodeParseContext<'_, '_>,
) -> Option<RenderedPicture> {
    let colors = context.colors.map(|colors| &colors.paint_key);
    let relationships = context.rels.paint_key();
    let (width_bits, height_bits) = (width.to_bits(), height.to_bits());
    let assets = context.assets.len();
    let hash = hash_state().hash_one((
        properties.as_str(),
        style.as_str(),
        width_bits,
        height_bits,
        colors.map(|colors| colors.hash),
        relationships.hash,
        assets,
    ));
    let matches = |key: &ShapeKey| {
        key.width == width_bits
            && key.height == height_bits
            && key.assets == assets
            && key.properties == properties
            && key.style == style
            && key.colors.as_ref() == colors
            && key.relationships == relationships.pairs
    };
    if let Some((_, paint)) = context
        .shape_cache
        .borrow()
        .entries
        .get(&hash)
        .and_then(|bucket| bucket.iter().find(|(key, _)| matches(key)))
    {
        return Some(paint.clone());
    }
    let warning_count = context.diagnostics.warnings.borrow().len();
    let color_warning_count = context
        .colors
        .map(|colors| colors.diagnostics.warnings.borrow().len());
    let paint = duke_office_art::render_shape(
        &properties,
        &style,
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
        let key = ShapeKey {
            properties,
            style,
            width: width_bits,
            height: height_bits,
            colors: colors.map(Rc::clone),
            relationships: Arc::clone(&relationships.pairs),
            assets,
        };
        let bytes = key.bytes().saturating_add(paint.svg.len());
        let mut cache = context.shape_cache.borrow_mut();
        if bytes <= 1024 * 1024
            && cache.count < 4096
            && cache.bytes.saturating_add(bytes) <= 8 * 1024 * 1024
        {
            cache.entries.entry(hash).or_default().push((key, paint.clone()));
            cache.count += 1;
            cache.bytes += bytes;
        }
    }
    Some(paint)
}
