"""Build deterministic, editable OfficeArt feature packs for manual review."""

import argparse
import csv
import fnmatch
import hashlib
import json
import math
import posixpath
from collections import Counter
from pathlib import Path
from xml.etree import ElementTree as ET
from xml.sax.saxutils import escape
from zipfile import ZIP_DEFLATED, ZipFile, ZipInfo

from generate import source_png


ROOT = Path(__file__).resolve().parents[2]
EMU = 9525
WIDTH, HEIGHT = 240, 144
NAMESPACES = {
    'w': 'http://schemas.openxmlformats.org/wordprocessingml/2006/main',
    'r': 'http://schemas.openxmlformats.org/officeDocument/2006/relationships',
    'a': 'http://schemas.openxmlformats.org/drawingml/2006/main',
    'wp': 'http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing',
    'pic': 'http://schemas.openxmlformats.org/drawingml/2006/picture',
    'wps': 'http://schemas.microsoft.com/office/word/2010/wordprocessingShape',
    'wpg': 'http://schemas.microsoft.com/office/word/2010/wordprocessingGroup',
    'w14': 'http://schemas.microsoft.com/office/word/2010/wordml',
    'a14': 'http://schemas.microsoft.com/office/drawing/2010/main',
    'mc': 'http://schemas.openxmlformats.org/markup-compatibility/2006',
    'v': 'urn:schemas-microsoft-com:vml',
    'o': 'urn:schemas-microsoft-com:office:office',
}
NS = ' '.join(f'xmlns:{key}="{value}"' for key, value in NAMESPACES.items())
CATALOG = json.loads(Path(__file__).with_name('office-art-catalog.json').read_text())
ENUMS = CATALOG['enums']
SUPPORT = {
    'supported': 'An implementation path exists; this fixture has not been compared with native Word.',
    'preserved': 'Source markup or cached appearance is retained; regeneration is not implemented.',
    'fallback': 'A simplified appearance or original bitmap is expected; the feature is not fully rendered.',
    'unmeasured': 'Implementation coverage has not been established for this combination.',
}
SOURCES = [
    CATALOG['source'],
    'https://learn.microsoft.com/en-us/dotnet/api/documentformat.openxml.drawing.textshapevalues?view=openxml-3.0.1',
    'https://learn.microsoft.com/en-us/openspecs/office_standards/ms-odrawxml/869af0ef-665b-4f28-a596-0917474ede26',
    'https://learn.microsoft.com/en-us/openspecs/office_standards/ms-odrawxml/987a3a42-96d7-4aff-ad43-fb600752011e',
]


def color(rgb='2166B5', alpha=None, prefix='a'):
    opacity = f'<{prefix}:alpha {"w14:" if prefix == "w14" else ""}val="{alpha}"/>' if alpha is not None else ''
    return f'<{prefix}:srgbClr {"w14:" if prefix == "w14" else ""}val="{rgb}">{opacity}</{prefix}:srgbClr>'


def solid(rgb='2166B5', alpha=None, prefix='a'):
    return f'<{prefix}:solidFill>{color(rgb, alpha, prefix)}</{prefix}:solidFill>'


def gradient(path=None, alpha=False, prefix='a'):
    q = 'w14:' if prefix == 'w14' else ''
    stops = ''.join(f'<{prefix}:gs {q}pos="{p}">{color(c, a if alpha else None, prefix)}</{prefix}:gs>' for p, c, a in [(0, '16B4D9', 25000), (50000, '7448CA', 65000), (100000, 'ED6548', 100000)])
    shade = f'<{prefix}:path {q}path="{path}"><{prefix}:fillToRect {q}l="50000" {q}t="50000" {q}r="50000" {q}b="50000"/></{prefix}:path>' if path else f'<{prefix}:lin {q}ang="2700000" {q}scaled="1"/>'
    return f'<{prefix}:gradFill><{prefix}:gsLst>{stops}</{prefix}:gsLst>{shade}</{prefix}:gradFill>'


def outline(inner=None, attrs='', tail=''):
    return f'<a:ln w="28575" {attrs}>{inner if inner is not None else solid("17334F")}{tail}</a:ln>'


def case(family, key, label, support='unmeasured', note='', **params):
    return dict(id=f'{family}/{key}', family=family, label=label, support=support,
                measurement='unmeasured', note=note, params=params)


def custom_geometry(paths, guides=''):
    return f'<a:custGeom><a:avLst/>{guides}<a:ahLst/><a:cxnLst/><a:rect l="0" t="0" r="r" b="b"/><a:pathLst>{paths}</a:pathLst></a:custGeom>'


def all_cases():
    rows = []
    add = rows.append
    for warp in ENUMS['ST_TextShapeType']:
        add(case('wordart', warp, warp, 'fallback' if warp != 'textNoShape' else 'unmeasured',
                 'Editable text; warped glyph outlines are not currently generated.', kind='text', warp=warp))
    for key, style in [
        ('solid-fill', '<w14:textFill>' + solid('E25C37', prefix='w14') + '</w14:textFill>'),
        ('transparent-fill', '<w14:textFill>' + solid('2166B5', 35000, 'w14') + '</w14:textFill>'),
        ('linear-gradient', '<w14:textFill>' + gradient(prefix='w14') + '</w14:textFill>'),
        ('radial-gradient', '<w14:textFill>' + gradient('circle', prefix='w14') + '</w14:textFill>'),
        ('outline-only', '<w14:textOutline w14:w="19050">' + solid('17334F', prefix='w14') + '</w14:textOutline><w14:textFill><w14:noFill/></w14:textFill>'),
        ('double-outline', '<w14:textOutline w14:w="38100" w14:cmpd="dbl">' + solid('17334F', prefix='w14') + '</w14:textOutline>'),
        ('dashed-outline', '<w14:textOutline w14:w="19050">' + solid('17334F', prefix='w14') + '<w14:prstDash w14:val="dash"/></w14:textOutline>'),
        ('shadow', '<w14:shadow w14:blurRad="38100" w14:dist="50800" w14:dir="2700000">' + color('202040', 65000, 'w14') + '</w14:shadow>'),
        ('glow', '<w14:glow w14:rad="76200">' + color('F3AD22', 80000, 'w14') + '</w14:glow>'),
        ('reflection', '<w14:reflection w14:stA="50000" w14:endA="0" w14:endPos="75000" w14:dist="25400" w14:dir="5400000" w14:sy="-100000" w14:fadeDir="5400000"/>'),
        ('extrusion', '<w14:scene3d><w14:camera w14:prst="perspectiveFront"/><w14:lightRig w14:rig="threePt" w14:dir="t"/></w14:scene3d><w14:props3d w14:extrusionH="152400" w14:prstMaterial="metal"><w14:bevelT w14:w="63500" w14:h="63500" w14:prst="circle"/></w14:props3d>'),
    ]:
        add(case('wordart', key, 'Text ' + key, 'preserved', 'Run effects are preserved; the simplified text-box rendering may omit them.', kind='text', text_style=style))
    for key, attrs in [('rotate-30', 'rot="1800000"'), ('rotate-90', 'rot="5400000"'), ('rotate-359', 'rot="21540000"'), ('flip-horizontal', 'flipH="1"'), ('flip-vertical', 'flipV="1"'), ('rotate-flip', 'rot="1800000" flipH="1" flipV="1"')]:
        add(case('wordart', key, key, kind='text', transform=attrs, warp='textWave1'))
    for key, params in [
        ('wave-adjust-low', dict(warp='textWave1', warp_adjust='<a:gd name="adj1" fmla="val 5000"/><a:gd name="adj2" fmla="val 0"/>')),
        ('wave-adjust-high', dict(warp='textWave1', warp_adjust='<a:gd name="adj1" fmla="val 20000"/><a:gd name="adj2" fmla="val 5000"/>')),
        ('mixed-runs', dict(mixed=True, text='Office Art')),
        ('multiline', dict(text='Office\nArt', warp='textArchUp')),
        ('rtl', dict(text='فن النص', rtl=True)),
        ('unicode', dict(text='Ångström Ω 漢字')),
        ('vertical', dict(body_attrs='vert="vert"', text='Office Art')),
        ('body-rotation', dict(body_attrs='rot="1800000"', text='Office Art')),
        ('tight-tracking', dict(tracking=-20)),
        ('wide-tracking', dict(tracking=80)),
    ]:
        add(case('wordart', key, key, 'unmeasured', 'Font substitution and text shaping depend on the installed fonts.', kind='text', **params))

    for preset in ENUMS['ST_ShapeType']:
        add(case('geometry', preset, preset, 'supported',
                 'Shared preset geometry renders; native visual fidelity is unmeasured.', geometry=preset))
    for preset, adj in [('roundRect', 0), ('roundRect', 50000), ('star5', 10000), ('star5', 45000), ('pie', 0), ('pie', 16200000), ('blockArc', 10000), ('donut', 45000)]:
        name = 'adj1' if preset in {'pie', 'blockArc'} else 'adj'
        add(case('geometry', f'{preset}-adjust-{adj}', f'{preset}: {name}={adj}', 'supported', geometry=preset, adjust=f'<a:gd name="{name}" fmla="val {adj}"/>'))
    path = '<a:path w="1000" h="1000"><a:moveTo><a:pt x="0" y="900"/></a:moveTo><a:lnTo><a:pt x="500" y="0"/></a:lnTo><a:lnTo><a:pt x="1000" y="900"/></a:lnTo><a:close/></a:path>'
    cubic = '<a:path w="1000" h="1000"><a:moveTo><a:pt x="0" y="800"/></a:moveTo><a:cubicBezTo><a:pt x="100" y="-300"/><a:pt x="900" y="1300"/><a:pt x="1000" y="200"/></a:cubicBezTo><a:close/></a:path>'
    quad = '<a:path w="1000" h="1000"><a:moveTo><a:pt x="0" y="800"/></a:moveTo><a:quadBezTo><a:pt x="500" y="-300"/><a:pt x="1000" y="800"/></a:quadBezTo><a:close/></a:path>'
    arc = '<a:path w="1000" h="1000"><a:moveTo><a:pt x="1000" y="500"/></a:moveTo><a:arcTo wR="500" hR="500" stAng="0" swAng="16200000"/><a:close/></a:path>'
    for name, paths, support in [('numeric-lines', path, 'supported'), ('cubic-outside-frame', cubic, 'supported'), ('quadratic', quad, 'supported'), ('arc', arc, 'supported'), ('multiple-paths', path + cubic, 'supported')]:
        add(case('geometry', 'custom-' + name, 'Custom ' + name, support, custom=custom_geometry(paths)))
    formula_path = path.replace('x="500"', 'x="mid"')
    add(case('geometry', 'custom-formula', 'Custom guide formula', 'supported', custom=custom_geometry(formula_path, '<a:gdLst><a:gd name="mid" fmla="*/ w 1 2"/></a:gdLst>')))

    for preset in ENUMS['ST_PresetPatternVal']:
        fill = f'<a:pattFill prst="{preset}"><a:fgClr>{color("17334F")}</a:fgClr><a:bgClr>{color("F2D276")}</a:bgClr></a:pattFill>'
        support = 'fallback' if preset in {'smConfetti', 'lgConfetti', 'dotDmnd', 'plaid', 'sphere', 'weave', 'divot', 'shingle', 'trellis'} else 'supported'
        add(case('fills-outlines', 'pattern-' + preset, 'Pattern ' + preset, support, fill=fill))
    for key, fill in [
        ('none', '<a:noFill/>'), ('solid', solid()), ('alpha', solid('2166B5', 40000)),
        ('linear-gradient', gradient()), ('transparent-gradient', gradient(alpha=True)),
        ('radial-gradient', gradient('circle')), ('rectangular-gradient', gradient('rect')),
        ('shape-gradient', gradient('shape')),
        ('image-stretch', '<a:blipFill><a:blip r:embed="rIdImage"/><a:stretch><a:fillRect/></a:stretch></a:blipFill>'),
        ('image-tile', '<a:blipFill dpi="96"><a:blip r:embed="rIdImage"/><a:tile sx="25000" sy="25000" flip="xy" algn="ctr"/></a:blipFill>'),
    ]:
        support = 'supported' if key in {'none', 'solid', 'alpha', 'linear-gradient', 'transparent-gradient', 'radial-gradient', 'image-stretch', 'image-tile'} else 'fallback'
        add(case('fills-outlines', 'fill-' + key, 'Fill ' + key, support, fill=fill))
    for dash in ENUMS['ST_PresetLineDashVal']:
        add(case('fills-outlines', 'dash-' + dash, 'Outline ' + dash, 'supported', line=outline(tail=f'<a:prstDash val="{dash}"/>')))
    for compound in ['sng', 'dbl', 'thickThin', 'thinThick', 'tri']:
        add(case('fills-outlines', 'compound-' + compound, 'Compound ' + compound, 'supported' if compound == 'sng' else 'fallback', line=outline(attrs=f'cmpd="{compound}"').replace('28575', '76200')))
    for cap in ['rnd', 'sq', 'flat']:
        add(case('fills-outlines', 'cap-' + cap, 'Line cap ' + cap, 'supported', geometry='line', line=outline(attrs=f'cap="{cap}"').replace('28575', '114300')))
    for join in ['round', 'bevel', 'miter']:
        tail = f'<a:{join} lim="800000"/>' if join == 'miter' else f'<a:{join}/>'
        add(case('fills-outlines', 'join-' + join, 'Line join ' + join, 'supported', custom=custom_geometry(path), line=outline(tail=tail).replace('28575', '114300')))
    for arrow in ['none', 'triangle', 'stealth', 'diamond', 'oval', 'arrow']:
        add(case('fills-outlines', 'arrow-' + arrow, 'Line ends ' + arrow, 'supported', geometry='line', line=outline(tail=f'<a:headEnd type="{arrow}" w="lg" len="sm"/><a:tailEnd type="{arrow}" w="sm" len="lg"/>')))
    add(case('fills-outlines', 'gradient-outline', 'Gradient outline', 'supported', line=outline(gradient()).replace('28575', '76200')))
    add(case('fills-outlines', 'pattern-outline', 'Pattern outline', 'supported', line=outline('<a:pattFill prst="diagCross"><a:fgClr>' + color() + '</a:fgClr><a:bgClr>' + color('F2D276') + '</a:bgClr></a:pattFill>').replace('28575', '76200')))
    add(case('fills-outlines', 'custom-dash', 'Custom dash sequence', 'supported', line=outline(tail='<a:custDash><a:ds d="200000" sp="100000"/><a:ds d="50000" sp="100000"/></a:custDash>')))

    effects = [
        ('alphaCeiling', '<a:alphaCeiling/>'), ('alphaFloor', '<a:alphaFloor/>'),
        ('alphaInv-color', '<a:alphaInv>' + color('00A0C0') + '</a:alphaInv>'),
        ('alphaModFix-zero', '<a:alphaModFix amt="0"/>'), ('alphaModFix-double', '<a:alphaModFix amt="200000"/>'),
        ('alphaRepl-zero', '<a:alphaRepl a="0"/>'), ('alphaRepl-full', '<a:alphaRepl a="100000"/>'),
        ('biLevel-zero', '<a:biLevel thresh="0"/>'), ('biLevel-full', '<a:biLevel thresh="100000"/>'),
        ('hsl-defaults', '<a:hsl/>'), ('hsl-wrap', '<a:hsl hue="21540000" sat="100000" lum="100000"/>'),
        ('tint-negative', '<a:tint hue="3600000" amt="-50000"/>'),
        ('lum-darkest', '<a:lum bright="-100000" contrast="0"/>'), ('lum-lightest', '<a:lum bright="100000" contrast="0"/>'),
        ('lum-contrast-high', '<a:lum bright="0" contrast="100000"/>'), ('lum-contrast-low', '<a:lum bright="0" contrast="-100000"/>'),
        ('clrRepl', '<a:clrRepl>' + color('16B4D9', 50000) + '</a:clrRepl>'),
        ('clrChange', '<a:clrChange useA="1"><a:clrFrom>' + color('EB2823') + '</a:clrFrom><a:clrTo>' + color('6D42CA', 25000) + '</a:clrTo></a:clrChange>'),
        ('blur-no-grow', '<a:blur rad="76200" grow="0"/>'),
        ('blur-zero', '<a:blur rad="0" grow="1"/>'),
    ]
    for key, effect in effects:
        add(case('effects', key, key, 'supported', kind='picture', blip_effect=effect))
    dag_nodes = [
        ('alpha-outset', '<a:alphaOutset rad="76200"/>'),
        ('alpha-inset', '<a:alphaOutset rad="-76200"/>'),
        ('alpha-modulate', '<a:alphaMod><a:cont type="tree"><a:alphaModFix amt="50000"/></a:cont></a:alphaMod>'),
        ('blend-multiply', '<a:blend blend="mult"><a:cont type="tree"><a:clrRepl>' + color('E25C37') + '</a:clrRepl></a:cont></a:blend>'),
        ('fill-overlay', '<a:fillOverlay blend="screen">' + solid('2166B5', 50000) + '</a:fillOverlay>'),
        ('fill', '<a:fill>' + gradient() + '</a:fill>'),
        ('relative-offset', '<a:relOff tx="25000" ty="-25000"/>'),
        ('transform', '<a:xfrm sx="75000" sy="100000" kx="900000" ky="0" tx="38100" ty="0"/>'),
        ('soft-edge', '<a:softEdge rad="76200"/>'),
        ('named-reference', '<a:cont name="tone" type="tree"><a:grayscl/></a:cont><a:effect ref="tone"/>'),
        ('sibling-branches', '<a:cont type="sib"><a:grayscl/><a:lum bright="30000" contrast="0"/></a:cont>'),
    ]
    for key, effect in dag_nodes:
        add(case('effects', 'dag-' + key, 'Effect DAG: ' + key, 'preserved', 'General effect graphs are retained but not rendered.', kind='picture', shape_effect=f'<a:effectDag type="tree">{effect}</a:effectDag>'))
    for preset in ENUMS['ST_PresetShadowVal']:
        add(case('effects', preset, 'Preset shadow ' + preset, 'fallback', kind='picture', shape_effect='<a:effectLst><a:prstShdw prst="' + preset + '" dist="76200" dir="2700000">' + color('17334F', 60000) + '</a:prstShdw></a:effectLst>'))
    for radius in [0, 9525, 76200, 228600]:
        add(case('effects', 'softEdge-' + str(radius), f'Soft edge {radius / EMU:g}px', 'supported', 'Small-radius rounding and DPI behavior remain approximate.', kind='picture', shape_effect=f'<a:effectLst><a:softEdge rad="{radius}"/></a:effectLst>'))
    for key, attrs in [('rotated-shadow', 'rot="1800000"'), ('flipped-shadow', 'flipH="1"')]:
        add(case('effects', key, key, 'unmeasured', kind='picture', transform=attrs, shape_effect='<a:effectLst><a:outerShdw blurRad="76200" dist="152400" dir="2700000" rotWithShape="0">' + color('202040', 65000) + '</a:outerShdw></a:effectLst>'))
    add(case('effects', 'nonlinear-plus-blur', 'HSL and blur combination', 'fallback', kind='picture', blip_effect='<a:hsl hue="3600000" sat="100000" lum="100000"/><a:blur rad="38100"/>'))

    for enum, key, attr in [('ST_PresetCameraType', 'camera', 'camera'), ('ST_LightRigType', 'lighting', 'lighting'), ('ST_PresetMaterialType', 'material', 'material'), ('ST_BevelPresetType', 'bevel', 'bevel')]:
        for value in ENUMS[enum]:
            add(case('three-d', key + '-' + value, key + ': ' + value, 'preserved', '3D scene, extrusion, lighting and materials are not rendered.', scene={attr: value}, geometry='roundRect'))
    add(case('three-d', 'camera-rotation', 'Explicit camera and lighting rotation', 'preserved', scene={'rotation': True}))
    add(case('three-d', 'backdrop', '3D backdrop plane', 'preserved', scene={'backdrop': True}))

    artistic = [
        ('artisticBlur', 'radius="25"'), ('artisticCement', 'trans="15000" crackSpacing="32"'),
        ('artisticChalkSketch', 'trans="0" pressure="3"'), ('artisticCrisscrossEtching', 'trans="15000" pressure="30"'),
        ('artisticCutout', 'trans="0" numberOfShades="4"'), ('artisticFilmGrain', 'trans="0" grainSize="40"'),
        ('artisticGlass', 'trans="0" scaling="34"'), ('artisticGlowDiffused', 'trans="0" intensity="5"'),
        ('artisticGlowEdges', 'trans="15000" smoothness="3"'), ('artisticLightScreen', 'trans="0" gridSize="4"'),
        ('artisticLineDrawing', 'trans="25000" pencilSize="30"'), ('artisticMarker', 'trans="0" size="50"'),
        ('artisticMosiaicBubbles', 'trans="0" pressure="25"'), ('artisticPaintStrokes', 'trans="0" intensity="5"'),
        ('artisticPaintBrush', 'trans="0" brushSize="4"'), ('artisticPastelsSmooth', 'trans="0" scaling="34"'),
        ('artisticPencilGrayscale', 'trans="0" pencilSize="27"'), ('artisticPencilSketch', 'trans="0" pressure="22"'),
        ('artisticPhotocopy', 'trans="30000" detail="3"'), ('artisticPlasticWrap', 'trans="0" smoothness="5"'),
        ('artisticTexturizer', 'trans="0" scaling="34"'), ('artisticWatercolorSponge', 'trans="0" brushSize="4"'),
        ('backgroundRemoval', 't="10000" b="90000" l="10000" r="90000"'),
        ('brightnessContrast', 'bright="20000" contrast="30000"'), ('colorTemperature', 'colorTemp="8500"'),
        ('saturation', 'sat="50000"'), ('sharpenSoften', 'amount="50000"'),
    ]
    for name, attrs in artistic:
        add(case('artistic', name, name, 'preserved', 'Metadata-only regeneration probe. The cached image is deliberately unchanged; appearance alone does not prove this effect ran.', kind='picture', artistic=f'<a14:imgEffect><a14:{name} {attrs}/></a14:imgEffect>'))
    add(case('artistic', 'hidden-effect', 'Hidden artistic effect', 'preserved', 'visible=false should retain the unchanged cache.', kind='picture', artistic='<a14:imgEffect visible="0"><a14:artisticBlur radius="25"/></a14:imgEffect>'))
    add(case('artistic', 'ordered-effects', 'Multiple ordered artistic effects', 'preserved', 'Metadata-only regeneration probe; unchanged cached bitmap.', kind='picture', artistic='<a14:imgEffect><a14:brightnessContrast bright="20000"/></a14:imgEffect><a14:imgEffect><a14:artisticPencilSketch pressure="30"/></a14:imgEffect>'))
    add(case('artistic', 'background-marks', 'Background removal marks', 'preserved', 'Metadata-only regeneration probe; unchanged cached bitmap.', kind='picture', artistic='<a14:imgEffect><a14:backgroundRemoval t="0" b="100000" l="0" r="100000"><a14:foregroundMark x1="25000" y1="25000" x2="75000" y2="75000"/><a14:backgroundMark x1="0" y1="0" x2="100000" y2="0"/></a14:backgroundRemoval></a14:imgEffect>'))

    for key, params in [
        ('mixed-children', {}), ('rotated-root', {'transform': 'rot="1800000"'}),
        ('flipped-root', {'transform': 'flipH="1"'}), ('negative-child-origin', {'child_origin': -40000}),
        ('nonuniform-coordinates', {'nonuniform': True}), ('nested-rotation', {'nested': True}),
        ('nested-flip-rotation', {'nested': True, 'inner_transform': 'rot="1800000" flipV="1"'}),
        ('group-fill', {'group_fill': True}),
    ]:
        add(case('groups-legacy', 'group-' + key, 'Group ' + key, 'unmeasured', kind='group', **params))
    for key, params in [
        ('textpath-straight', {'path': 'm0,10800l21600,10800e'}),
        ('textpath-curve', {'path': 'm0,18000c5400,0,16200,0,21600,18000e'}),
        ('textpath-rotation', {'path': 'm0,10800l21600,10800e', 'style': 'rotation:30;flip:x;'}),
        ('textpath-shadow', {'path': 'm0,10800l21600,10800e', 'vml_effect': '<v:shadow on="t" color="#203040" opacity=".5" offset="5pt,5pt"/>'}),
        ('textpath-extrusion', {'path': 'm0,10800l21600,10800e', 'vml_effect': '<o:extrusion on="t" backdepth="12pt" color="#CA6A30"/>'}),
    ]:
        add(case('groups-legacy', 'vml-' + key, 'VML ' + key, 'fallback', 'Legacy editable text-on-path, without a raster replacement.', kind='vml-text', **params))
    for key, attrs in [('gain', 'gain="1.5"'), ('blacklevel', 'blacklevel=".2"'), ('gamma', 'gamma="1.6"'), ('chromakey', 'chromakey="#eb2823"'), ('signed-crop', 'cropleft="-13107f" croptop="6554f"'), ('opacity', 'gain="1"')]:
        add(case('groups-legacy', 'vml-image-' + key, 'VML image ' + key, 'supported' if key == 'signed-crop' else 'fallback', kind='vml-image', vml_attrs=attrs, opacity=key == 'opacity'))
    return rows


def transform(attrs='', width=WIDTH, height=HEIGHT, x=0, y=0):
    return f'<a:xfrm {attrs}><a:off x="{x}" y="{y}"/><a:ext cx="{round(width * EMU)}" cy="{round(height * EMU)}"/></a:xfrm>'


def scene_xml(params):
    rotation = '<a:rot lat="1200000" lon="1800000" rev="600000"/>' if params.get('rotation') else ''
    camera = f'<a:camera prst="{params.get("camera", "perspectiveFront")}">{rotation}</a:camera>'
    lighting = f'<a:lightRig rig="{params.get("lighting", "threePt")}" dir="t">{rotation}</a:lightRig>'
    backdrop = '<a:backdrop><a:anchor x="0" y="0" z="0"/><a:norm dx="0" dy="0" dz="100000"/><a:up dx="0" dy="100000" dz="0"/></a:backdrop>' if params.get('backdrop') else ''
    bevel = params.get('bevel', 'circle')
    return f'<a:scene3d>{camera}{lighting}{backdrop}</a:scene3d><a:sp3d extrusionH="152400" contourW="12700" prstMaterial="{params.get("material", "plastic")}"><a:bevelT w="63500" h="63500" prst="{bevel}"/><a:bevelB w="38100" h="38100" prst="{bevel}"/><a:extrusionClr>{color("C56E24")}</a:extrusionClr><a:contourClr>{color("17334F")}</a:contourClr></a:sp3d>'


def text_body(params):
    text = params.get('text', 'Office Art')
    style = '<w:rFonts w:ascii="Arial" w:hAnsi="Arial" w:eastAsia="Arial" w:cs="Arial"/><w:b/><w:color w:val="2166B5"/>'
    style += f'<w:spacing w:val="{params.get("tracking", 0)}"/><w:sz w:val="56"/><w:szCs w:val="56"/>'
    if params.get('rtl'):
        style += '<w:rtl/>'
    style += params.get('text_style', '')
    paragraphs = []
    for value in text.split('\n'):
        runs = f'<w:r><w:rPr>{style}</w:rPr><w:t xml:space="preserve">{escape(value)}</w:t></w:r>'
        if params.get('mixed'):
            runs = '<w:r><w:rPr>' + style + '</w:rPr><w:t>Office </w:t></w:r><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/><w:i/><w:color w:val="E25C37"/><w:sz w:val="64"/></w:rPr><w:t>Art</w:t></w:r>'
        paragraphs.append('<w:p><w:pPr><w:spacing w:before="0" w:after="0"/><w:jc w:val="center"/></w:pPr>' + runs + '</w:p>')
    return '<wps:txbx><w:txbxContent>' + ''.join(paragraphs) + '</w:txbxContent></wps:txbx>'


def shape(params, width=WIDTH, height=HEIGHT, x=0, y=0):
    text = params.get('kind') == 'text'
    geometry = params.get('custom') or f'<a:prstGeom prst="{params.get("geometry", "rect")}"><a:avLst>{params.get("adjust", "")}</a:avLst></a:prstGeom>'
    fill = params.get('fill', '<a:noFill/>' if text else solid())
    line = params.get('line', '<a:ln><a:noFill/></a:ln>' if text else outline())
    props = transform(params.get('transform', ''), width, height, x, y) + geometry + fill + line + params.get('shape_effect', '')
    if params.get('scene'):
        props += scene_xml(params['scene'])
    warp = ''
    if text:
        warp = f'<a:prstTxWarp prst="{params.get("warp", "textNoShape")}"><a:avLst>{params.get("warp_adjust", "")}</a:avLst></a:prstTxWarp>'
    body = text_body(params) if text else ''
    body_attrs = 'anchor="ctr" lIns="0" tIns="0" rIns="0" bIns="0" wrap="none" fromWordArt="1"' if text else ''
    body_attrs += ' ' + params.get('body_attrs', '')
    return f'<wps:wsp><wps:cNvSpPr txBox="{int(text)}"/><wps:spPr>{props}</wps:spPr>{body}<wps:bodyPr {body_attrs}>{warp}<a:noAutofit/></wps:bodyPr></wps:wsp>'


def picture(params, ordinal, width=WIDTH, height=HEIGHT, x=0, y=0):
    ext = ''
    if params.get('artistic'):
        ext = '<a:extLst><a:ext uri="{BEBA8EAE-BF5A-486c-A8C5-ECC9F3942E4B}"><a14:imgProps><a14:imgLayer r:embed="rIdOriginal">' + params['artistic'] + '</a14:imgLayer></a14:imgProps></a:ext></a:extLst>'
    return f'<pic:pic><pic:nvPicPr><pic:cNvPr id="{ordinal}" name="Embedded source"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed="rIdImage">{params.get("blip_effect", "")}{ext}</a:blip><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr>{transform(params.get("transform", ""), width, height, x, y)}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom>{params.get("shape_effect", "")}</pic:spPr></pic:pic>'


def group(params, ordinal):
    origin = params.get('child_origin', 0)
    cx, cy = WIDTH * EMU, HEIGHT * EMU
    child_cx = cx * 2 if params.get('nonuniform') else cx
    group_fill = gradient() if params.get('group_fill') else ''
    inner = shape({'geometry': 'ellipse', 'fill': '<a:grpFill/>' if params.get('group_fill') else solid('E25C37', 70000)}, 100, 100, origin, origin)
    inner += picture({}, ordinal + 1000, 100, 70, origin + 110 * EMU, origin + 35 * EMU)
    inner += shape({'kind': 'text', 'text': 'Art', 'transform': 'rot="-900000"'}, 100, 40, origin + 50 * EMU, origin + 100 * EMU)
    if params.get('nested'):
        inner_transform = params.get('inner_transform', 'rot="1800000"')
        inner = f'<wpg:grpSp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm {inner_transform}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/><a:chOff x="{origin}" y="{origin}"/><a:chExt cx="{child_cx}" cy="{cy}"/></a:xfrm></wpg:grpSpPr>{inner}</wpg:grpSp>'
    return f'<wpg:wgp><wpg:cNvGrpSpPr/><wpg:grpSpPr><a:xfrm {params.get("transform", "")}><a:off x="0" y="0"/><a:ext cx="{cx}" cy="{cy}"/><a:chOff x="{origin}" y="{origin}"/><a:chExt cx="{child_cx}" cy="{cy}"/></a:xfrm>{group_fill}</wpg:grpSpPr>{inner}</wpg:wgp>'


def drawing(row, ordinal):
    params = row['params']
    kind = params.get('kind', 'shape')
    if kind.startswith('vml-'):
        style = f'width:{WIDTH * .75:g}pt;height:{HEIGHT * .75:g}pt;' + params.get('style', '')
        if kind == 'vml-text':
            inner = '<v:path textpathok="t"/><v:textpath on="t" fitshape="t" style="font-family:Arial;font-size:32pt;font-weight:bold" string="Office Art"/>' + params.get('vml_effect', '')
            return f'<w:pict><v:shape id="vml-{ordinal}" coordsize="21600,21600" path="{params["path"]}" style="{style}" fillcolor="#2166B5" strokecolor="#17334F" strokeweight="1pt">{inner}</v:shape></w:pict>'
        fill = '<v:fill opacity=".4"/>' if params.get('opacity') else ''
        return f'<w:pict><v:shape id="vml-{ordinal}" coordsize="21600,21600" path="m0,0l21600,0,21600,21600,0,21600xe" style="{style}" stroked="f">{fill}<v:imagedata r:id="rIdImage" {params.get("vml_attrs", "")}/></v:shape></w:pict>'
    if kind == 'picture':
        content, namespace = picture(params, ordinal), NAMESPACES['pic']
    elif kind == 'group':
        content, namespace = group(params, ordinal), NAMESPACES['wpg']
    else:
        content, namespace = shape(params), NAMESPACES['wps']
    return f'<w:drawing><wp:inline distT="0" distB="0" distL="0" distR="0"><wp:extent cx="{WIDTH * EMU}" cy="{HEIGHT * EMU}"/><wp:effectExtent l="0" t="0" r="0" b="0"/><wp:docPr id="{ordinal}" name="{escape(row["id"])}" descr="{escape(row["label"])}"/><a:graphic><a:graphicData uri="{namespace}">{content}</a:graphicData></a:graphic></wp:inline></w:drawing>'


def paragraph(text='', size=20, bold=False, before=False, align='left', raw='', before_space=0, after_space=100):
    return '<w:p><w:pPr>' + ('<w:pageBreakBefore/>' if before else '') + f'<w:spacing w:before="{before_space}" w:after="{after_space}"/><w:jc w:val="{align}"/></w:pPr><w:r><w:rPr><w:rFonts w:ascii="Arial" w:hAnsi="Arial"/>{"<w:b/>" if bold else ""}<w:sz w:val="{size}"/></w:rPr><w:t xml:space="preserve">{escape(text)}</w:t>{raw}</w:r></w:p>'


def document_xml(family, rows, per_page):
    body = []
    pages = math.ceil(len(rows) / per_page)
    for start in range(0, len(rows), per_page):
        page = start // per_page + 1
        body.append(paragraph(f'OfficeArt atlas · {family} · {page}/{pages}', 28, True, start > 0))
        body.append(paragraph('Editable probes. Labels describe parser coverage; native appearance is unmeasured.', 17))
        cells = []
        cell_width = 5580 if per_page == 4 else 11160
        for offset, row in enumerate(rows[start:start + per_page]):
            ordinal = start + offset + 1
            row['page'] = page
            row['cell'] = offset + 1
            contents = paragraph(row['id'], 17, True) + paragraph(row['label'], 19)
            contents += paragraph('Coverage: ' + row['support'] + ' · native: unmeasured', 16)
            contents += paragraph(raw=drawing(row, ordinal), align='center', before_space=720, after_space=720)
            if row['note']:
                contents += paragraph(row['note'], 16)
            contents += paragraph('Review: __________________________', 16)
            cells.append(f'<w:tc><w:tcPr><w:tcW w:w="{cell_width}" w:type="dxa"/><w:vAlign w:val="top"/></w:tcPr>' + contents + '</w:tc>')
        if per_page == 4 and len(cells) % 2:
            cells.append('<w:tc><w:tcPr><w:tcW w:w="5580" w:type="dxa"/></w:tcPr><w:p/></w:tc>')
        columns = 2 if per_page == 4 else 1
        table_rows = ''.join('<w:tr><w:trPr><w:cantSplit/><w:trHeight w:val="5600" w:hRule="atLeast"/></w:trPr>' + ''.join(cells[i:i + columns]) + '</w:tr>' for i in range(0, len(cells), columns))
        grid_width = 5580 if columns == 2 else 11160
        grid = f'<w:gridCol w:w="{grid_width}"/>' * columns
        borders = ''.join(f'<w:{edge} w:val="single" w:sz="4" w:color="CFD7E1"/>' for edge in ['top', 'left', 'bottom', 'right', 'insideH', 'insideV'])
        body.append(f'<w:tbl><w:tblPr><w:tblW w:w="11160" w:type="dxa"/><w:tblBorders>{borders}</w:tblBorders><w:tblLayout w:type="fixed"/><w:tblCellMar><w:top w:w="120" w:type="dxa"/><w:left w:w="160" w:type="dxa"/><w:bottom w:w="120" w:type="dxa"/><w:right w:w="160" w:type="dxa"/></w:tblCellMar></w:tblPr><w:tblGrid>{grid}</w:tblGrid>{table_rows}</w:tbl>')
        body.append('<w:p><w:pPr><w:spacing w:after="0" w:line="20" w:lineRule="exact"/></w:pPr><w:r><w:rPr><w:sz w:val="2"/></w:rPr><w:t> </w:t></w:r></w:p>')
    section = '<w:sectPr><w:pgSz w:w="12240" w:h="15840"/><w:pgMar w:top="540" w:right="540" w:bottom="540" w:left="540" w:header="0" w:footer="0" w:gutter="0"/></w:sectPr>'
    return f'<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document {NS} mc:Ignorable="w14 wps wpg a14"><w:body>{"".join(body)}{section}</w:body></w:document>'


def build_package(path, document, image):
    rel_ns = 'http://schemas.openxmlformats.org/package/2006/relationships'
    rel_base = NAMESPACES['r']
    parts = {
        '[Content_Types].xml': '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/settings.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.settings+xml"/></Types>',
        '_rels/.rels': f'<Relationships xmlns="{rel_ns}"><Relationship Id="rIdDocument" Type="{rel_base}/officeDocument" Target="word/document.xml"/></Relationships>',
        'word/_rels/document.xml.rels': f'<Relationships xmlns="{rel_ns}"><Relationship Id="rIdImage" Type="{rel_base}/image" Target="media/source.png"/><Relationship Id="rIdOriginal" Type="{rel_base}/image" Target="media/original.png"/><Relationship Id="rIdSettings" Type="{rel_base}/settings" Target="settings.xml"/></Relationships>',
        'word/document.xml': document,
        'word/settings.xml': f'<w:settings xmlns:w="{NAMESPACES["w"]}"><w:compat><w:compatSetting w:name="compatibilityMode" w:uri="http://schemas.microsoft.com/office/word" w:val="15"/></w:compat></w:settings>',
        'word/media/source.png': image,
        'word/media/original.png': image,
    }
    with ZipFile(path, 'w', compression=ZIP_DEFLATED) as package:
        for name, data in parts.items():
            info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            package.writestr(info, data)
    return validate_package(path)


def validate_package(path):
    with ZipFile(path) as package:
        names = set(package.namelist())
        if package.testzip() is not None:
            raise ValueError('ZIP integrity check failed')
        xml_parts = {name: ET.fromstring(package.read(name)) for name in names if name.endswith(('.xml', '.rels'))}
        for name, root in xml_parts.items():
            if name.endswith('.rels'):
                base = '' if name == '_rels/.rels' else name.rsplit('/_rels/', 1)[0] + '/'
                ids = set()
                for relationship in root:
                    if relationship.get('TargetMode') == 'External':
                        raise ValueError('External relationships are not allowed')
                    if relationship.get('Id') in ids:
                        raise ValueError('Duplicate relationship identifier')
                    ids.add(relationship.get('Id'))
                    target = posixpath.normpath(base + relationship.attrib['Target'])
                    if target not in names:
                        raise ValueError('Unresolved relationship target: ' + target)
        document = xml_parts['word/document.xml']
        relation_ids = {element.attrib['Id'] for element in xml_parts['word/_rels/document.xml.rels']}
        for element in document.iter():
            for key, value in element.attrib.items():
                if key.startswith('{' + NAMESPACES['r'] + '}') and value not in relation_ids:
                    raise ValueError('Unresolved document relationship: ' + value)
        drawing_ids = [element.attrib['id'] for element in document.findall('.//wp:docPr', NAMESPACES)]
        if len(set(drawing_ids)) != len(drawing_ids):
            raise ValueError('Duplicate drawing identifier')
        for tag, enum in [('a:prstTxWarp', 'ST_TextShapeType'), ('a:prstGeom', 'ST_ShapeType'), ('a:pattFill', 'ST_PresetPatternVal'), ('a:camera', 'ST_PresetCameraType'), ('a:bevelT', 'ST_BevelPresetType')]:
            for element in document.findall('.//' + tag, NAMESPACES):
                if element.attrib['prst'] not in ENUMS[enum]:
                    raise ValueError('Invalid preset: ' + element.attrib['prst'])
        return {'zipIntegrity': 'passed', 'xmlWellFormedness': 'passed', 'internalRelationships': 'passed',
                'uniqueDrawingIds': 'passed', 'presetEnums': 'passed', 'schemaValidation': 'not performed',
                'nativeWordOpen': 'not performed', 'visualLayout': 'unmeasured', 'parts': len(names)}


def write_readme(output, manifest):
    lines = ['# OfficeArt manual fixture atlas', '',
             'Editable Word documents for manual opening, modification, save/reopen, parser inspection and visual comparison. This is a feature catalog, not a full-parity certification.', '',
             '| Pack | Cases | Planned pages |', '| --- | ---: | ---: |']
    lines += [f'| [{item["document"]}]({item["document"]}) | {item["cases"]} | {item["plannedPages"]} |' for item in manifest['documents']]
    lines += ['', 'Pages use four labeled cells by default. Page numbers are planned from explicit breaks; native pagination has not been measured. Record observations in `review.csv`; manifest IDs locate the exact document/page/cell and generation parameters.', '',
              '## Coverage labels', '']
    lines += [f'- **{key}**: {value}' for key, value in SUPPORT.items()]
    lines += ['', 'All new cases have `measurement: unmeasured`. A supported label means an existing code path, not a successful pixel comparison. Source preservation is based on the package/source-XML path and is not proof of normalized model round-trip fidelity.', '',
              '## Artistic-effect cache probes', '',
              'The artistic pack embeds both an original and an unchanged cached bitmap. Effect parameters are editable metadata. Word may trust the cache until an effect is changed or regenerated; identical pictures do not establish that an effect is unsupported. Change a parameter in Word, save, and compare the resulting cached media and rendering. No fake processed cache is supplied.', '',
              '## Reproduce or select', '',
              '```sh', '/usr/bin/python3 scripts/image-fidelity/generate_office_art_atlas.py --output artifacts/office-art-fixtures-20261105',
              '/usr/bin/python3 scripts/image-fidelity/generate_office_art_atlas.py --families wordart,artistic --output /tmp/office-art-selected',
              "/usr/bin/python3 scripts/image-fidelity/generate_office_art_atlas.py --cases 'wordart/textWave*' --per-page 1 --output /tmp/office-art-waves", '```', '',
              'Requires Python and Pillow, as does the existing image-fidelity generator. No network access is needed to regenerate. ZIP timestamps, media, IDs, catalogs and case ordering are deterministic. Existing output is protected unless `--overwrite` is supplied.', '',
              '## Validation and limits', '',
              'Generation checks ZIP integrity, XML well-formedness, internal relationship targets, drawing IDs and selected preset enumerations. It does not run a complete OOXML validator, native Word, or a visual pagination check. No external relationships, macros or OLE payloads are included.', '',
              'The full generator catalog covers all 41 text-warp and 187 geometry enum values, all 54 pattern and 11 dash presets, all 62 camera, 27 lighting, 15 material and 12 bevel presets, all 20 preset shadows, and all 27 Office 2010 picture-effect extension choices, with selected parameter and combination edges. The pack table above describes the current selection. The catalog does not exhaust parameter products, invalid markup, binary DOC records, codecs, SmartArt, charts, ink, media/OLE, arbitrary font shaping, or every Office version. Existing picture/effect-order/group matrices remain separate references.', '',
              '## Primary schema sources', '']
    lines += [f'- {url}' for url in SOURCES]
    (output / 'README.md').write_text('\n'.join(lines) + '\n')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--families', help='Comma-separated family names')
    parser.add_argument('--cases', action='append', default=[], help='Case ID glob; repeat to select multiple patterns')
    parser.add_argument('--per-page', type=int, choices=[1, 4], default=4)
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--overwrite', action='store_true')
    args = parser.parse_args()
    rows = all_cases()
    if len({row['id'] for row in rows}) != len(rows):
        raise ValueError('Duplicate case identifier')
    families = list(dict.fromkeys(row['family'] for row in rows))
    if args.families:
        selected = set(args.families.split(','))
        if selected - set(families):
            parser.error('Unknown family: ' + ', '.join(sorted(selected - set(families))))
        rows = [row for row in rows if row['family'] in selected]
    if args.cases:
        rows = [row for row in rows if any(fnmatch.fnmatchcase(row['id'], pattern) for pattern in args.cases)]
    if not rows:
        parser.error('The selection contains no cases')
    if args.list:
        print('\n'.join(f'{row["id"]}\t{row["support"]}' for row in rows))
        return
    if not args.output:
        parser.error('--output is required when generating documents')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if any(output.iterdir()) and not args.overwrite:
        parser.error('Output directory is not empty; choose a new directory or use --overwrite')
    image = source_png()
    manifest = dict(schemaVersion=1, generator=str(Path(__file__).relative_to(ROOT)),
                    coverageVersion='rust-office-art 0.1.2', coverageBasis='Source review and DOCX adapter smoke; native visual fidelity remains unmeasured.',
                    catalogSource=CATALOG, sources=SOURCES, pageSizePx={'width': 816, 'height': 1056},
                    perPage=args.per_page, supportDefinitions=SUPPORT, cases=[], documents=[])
    for family in families:
        selected = [row for row in rows if row['family'] == family]
        if not selected:
            continue
        filename = 'office-art-' + family + '.docx'
        document = document_xml(family, selected, args.per_page)
        checks = build_package(output / filename, document, image)
        manifest['documents'].append(dict(document=filename, cases=len(selected),
            plannedPages=math.ceil(len(selected) / args.per_page), sha256=hashlib.sha256((output / filename).read_bytes()).hexdigest(), validation=checks))
        manifest['cases'].extend(dict(document=filename, **row) for row in selected)
    manifest['counts'] = dict(total=len(rows), families=dict(Counter(row['family'] for row in rows)), support=dict(Counter(row['support'] for row in rows)))
    manifest['limitations'] = ['New fixtures are not yet opened or visually compared in native Word.', 'Artistic metadata is paired with unchanged cache bytes.', 'Preset enumeration coverage is not rendering parity or exhaustive parameter coverage.']
    (output / 'manifest.json').write_text(json.dumps(manifest, indent=2, ensure_ascii=False) + '\n')
    with (output / 'review.csv').open('w', newline='') as target:
        writer = csv.writer(target)
        writer.writerow(['case_id', 'document', 'planned_page', 'cell', 'coverage', 'native_open', 'native_appearance', 'parser_appearance', 'save_reopen', 'notes'])
        for row in manifest['cases']:
            writer.writerow([row['id'], row['document'], row['page'], row['cell'], row['support'], '', '', '', '', ''])
    write_readme(output, manifest)
    bundle_parts = ['README.md', 'manifest.json', 'review.csv'] + [item['document'] for item in manifest['documents']]
    with ZipFile(output / 'office-art-manual-atlas.zip', 'w', compression=ZIP_DEFLATED) as bundle:
        for name in bundle_parts:
            info = ZipInfo(name, date_time=(2026, 1, 1, 0, 0, 0))
            info.compress_type = ZIP_DEFLATED
            bundle.writestr(info, (output / name).read_bytes())
    print(json.dumps(dict(output=str(output), **manifest['counts']), indent=2))


if __name__ == '__main__':
    main()
