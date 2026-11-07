from common import norm, paragraph, plain, slide


ENGINES = ['pptxboss', 'betteroffice', 'rustypptx']
LABELS = ['pptxboss-core 2.3.0', 'BetterOffice 0.5.1', 'rustypptx 0.2.0']


def model(engine, data):
    if engine == 'pptxboss':
        for source in data['slides']:
            for table in source['tables']:
                for cell in table:
                    cell['text'] = norm(cell['text'])
        return data
    if engine == 'rustypptx':
        output = dict(slides=[])
        for source in data['slides']:
            current = slide()
            current['paragraphs'] = [plain(element['text']) for element in source['text_elements']]
            current['images'] = [image.get('dataSha256', 'missing') for image in source['images']]
            output['slides'].append(current)
        return output
    if engine != 'betteroffice':
        raise ValueError(engine)
    output = dict(slides=[], size=[data['presentation']['widthEmu'], data['presentation']['heightEmu']])
    images = {item['partPath']: item.get('bytesSha256', 'missing') for item in data['media']}
    charts = {item['partPath']: item['chart'] for item in data.get('charts', [])}
    diagrams = {item['partPath']: item['shapes'] for item in data.get('diagramDrawings', [])}
    for source in data['slides']:
        current = slide()
        current['hidden'] = source.get('hidden') is True
        relationships = {item['id']: item for item in data['relationships'].get(source['partPath'], [])}

        def paragraphs(body):
            result = []
            for item in (body or {}).get('paragraphs', []):
                runs = []
                for run in item['runs']:
                    properties = run['properties']
                    link = relationships.get(properties.get('hyperlinkRelationshipId'), {})
                    runs.append(dict(text='\n' if run.get('lineBreak') else run['text'],
                                     bold=properties.get('bold'), italic=properties.get('italic'),
                                     link=link.get('target') if link.get('targetMode') == 'external' else link.get('resolvedTarget')))
                result.append(paragraph(runs))
            return result

        def walk(nodes, grouped=False, seen=()):
            for node in nodes:
                kind = node['kind']
                if not grouped and kind != 'group':
                    transform = node['transform']
                    current['shapes'].append(dict(id=str(node['id']), transform=[transform[k] for k in ['x', 'y', 'width', 'height']]))
                if kind == 'shape':
                    current['paragraphs'].extend(paragraphs(node.get('text')))
                elif kind == 'group':
                    walk(node['children'], True, seen)
                elif kind == 'picture':
                    current['images'].append(images.get(node.get('mediaPartPath'), 'missing'))
                elif kind == 'graphicFrame':
                    frame = node['data']
                    if frame['type'] == 'table':
                        cells = []
                        for row in frame['rows']:
                            for cell in row if isinstance(row, list) else row['cells']:
                                body = cell if 'paragraphs' in cell else cell['text']
                                ps = paragraphs(body)
                                current['paragraphs'].extend(ps)
                                cells.append(dict(text=norm(''.join(p['text'] for p in ps)),
                                                  col_span=cell.get('gridSpan', 1), row_span=cell.get('rowSpan', 1)))
                        current['tables'].append(cells)
                    elif frame['type'] == 'chart':
                        chart = charts.get(frame.get('partPath'), {})
                        current['charts'].append([series['values'] for series in chart.get('series', [])])
                    elif frame['type'] == 'diagram':
                        part = frame.get('drawingPartPath')
                        if part and part not in seen:
                            walk(diagrams.get(part, []), True, (*seen, part))
                    elif frame['type'] == 'unknown' and frame.get('picture'):
                        current['images'].append(images.get(frame['picture'].get('mediaPartPath'), 'missing'))
        walk(source['shapes'])
        current['notes'] = [plain(text) for text in source.get('notes', '').splitlines()]
        current['comments'] = [item['text'] for item in data.get('comments', []) if item['slidePartPath'] == source['partPath']]
        output['slides'].append(current)
    return output


def unscored(engine, feature):
    return engine == 'rustypptx' and feature in {'shape_positions', 'slide_size', 'hidden_slides', 'comments'}
