"""Place parsed image output on fixed canvases for browser comparison."""

import argparse
import html
import json
from pathlib import Path


def images(value):
    if isinstance(value, dict):
        if value.get('type') == 'image':
            yield value
        else:
            for child in value.values():
                yield from images(child)
    elif isinstance(value, list):
        for child in value:
            yield from images(child)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--model', type=Path, required=True)
    parser.add_argument('--output', default='parser-pages.html')
    args = parser.parse_args()
    manifest = json.loads((args.directory / 'manifest.json').read_text())
    model = json.loads(args.model.read_text())
    pictures = list(images(model['nodes']))
    if len(pictures) != len(manifest['cases']):
        raise SystemExit(f"Expected {len(manifest['cases'])} pictures, received {len(pictures)}")
    frame = manifest['layoutFramePx']
    pages = []
    for case, picture in zip(manifest['cases'], pictures):
        width = picture.get('widthPx', frame['width'])
        height = picture.get('heightPx', frame['height'])
        anchor = picture.get('floating', {})
        x, y = anchor.get('xPx', frame['x']), anchor.get('yPx', frame['y'])
        paint = picture.get('paintBounds')
        if paint:
            scale_x, scale_y = width / paint['layoutWidthPx'], height / paint['layoutHeightPx']
            x += paint['xPx'] * scale_x
            y += paint['yPx'] * scale_y
            width, height = paint['widthPx'] * scale_x, paint['heightPx'] * scale_y
        src = html.escape(picture.get('src', ''), quote=True)
        opacity = picture.get('cssOpacity', 1)
        image = f'<img src="{src}" style="position:absolute;left:{x}px;top:{y}px;width:{width}px;height:{height}px;opacity:{opacity}">'
        crop = picture.get('crop')
        if crop:
            left, right = crop.get('leftFraction', 0), crop.get('rightFraction', 0)
            top, bottom = crop.get('topFraction', 0), crop.get('bottomFraction', 0)
            across, down = 1 - left - right, 1 - top - bottom
            if across > 0 and down > 0:
                iw, ih = width / across, height / down
                image = f'<div style="position:absolute;left:{x}px;top:{y}px;width:{width}px;height:{height}px;overflow:hidden"><img src="{src}" style="position:absolute;left:{-left*iw}px;top:{-top*ih}px;width:{iw}px;height:{ih}px;opacity:{opacity}"></div>'
        pages.append(f'<section id="page-{case["page"]:02d}" data-case="{html.escape(case["id"])}">{image}</section>')
    size = manifest['pageSizePx']
    document = f'<!doctype html><meta charset="utf-8"><title>Parsed picture comparison</title><style>html,body{{margin:0;background:white}}section{{position:relative;width:{size["width"]}px;height:{size["height"]}px;overflow:hidden;background:white}}img{{display:block}}</style>' + ''.join(pages)
    (args.directory / args.output).write_text(document)


if __name__ == '__main__':
    main()
