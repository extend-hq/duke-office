import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { pathToFileURL } from 'node:url';

const run = path.resolve(process.argv[2]);
const modelPath = path.resolve(process.argv[3]);
const { resolveConnectorEndpoint } = await import(pathToFileURL(modelPath));
const receipt = [];
const check = (name, action) => { action(); receipt.push({ name, passed: true }); };
const sourcePart = 'ppt/slides/slide1.xml';
const transform = { x: 0, y: 0, width: 1, height: 1 };
const shape = (sourceId, part = sourcePart) => ({ type: 'shape', id: `display:${sourceId}`, sourceId, sourcePart: part, transform, geometry: {} });
const connector = shape('10');
const endpoint = { shapeId: 2, connectionSite: 0 };
const target = shape('2');
check('simple target', () => assert.equal(resolveConnectorEndpoint([target], connector, endpoint), target));
check('nested group target', () => assert.equal(resolveConnectorEndpoint([{ type: 'group', id: 'group', transform, children: [target] }], connector, endpoint), target));
check('missing target', () => assert.equal(resolveConnectorEndpoint([], connector, endpoint), undefined));
check('part isolation', () => assert.equal(resolveConnectorEndpoint([shape('2', 'ppt/slideLayouts/slideLayout1.xml')], connector, endpoint), undefined));
check('inherited display identity', () => {
  const inherited = { ...target, id: 'master:2', sourcePart: 'ppt/slideMasters/slideMaster1.xml' };
  assert.equal(resolveConnectorEndpoint([target, inherited], { ...connector, sourcePart: inherited.sourcePart }, endpoint), inherited);
});
check('missing source part', () => assert.equal(resolveConnectorEndpoint([target], { ...connector, sourcePart: undefined }, endpoint), undefined));
check('duplicate identity', () => assert.equal(resolveConnectorEndpoint([target, { ...target, id: 'other' }], connector, endpoint), undefined));
check('duplicate nested identity', () => assert.equal(resolveConnectorEndpoint([target, { type: 'group', id: 'group', transform, children: [target] }], connector, endpoint), undefined));
check('same ID different parts', () => assert.equal(resolveConnectorEndpoint([shape('2', 'ppt/slideMasters/slideMaster1.xml'), target], connector, endpoint), target));
for (const lexical of ['2', '0002', '+0002', ' \t+2\r\n']) {
  check(`valid unsigned lexical ${JSON.stringify(lexical)}`, () => {
    const node = shape(lexical);
    assert.equal(resolveConnectorEndpoint([node], connector, endpoint), node);
  });
}
for (const lexical of ['2.0', '2e0', '0x2', '\u00a02', '-2', '２', '', '2 0', '4294967296']) {
  check(`invalid unsigned lexical ${JSON.stringify(lexical)}`, () => assert.equal(resolveConnectorEndpoint([shape(lexical)], connector, endpoint), undefined));
}
check('negative zero', () => {
  const zero = shape('-000');
  assert.equal(resolveConnectorEndpoint([zero], connector, { ...endpoint, shapeId: 0 }), zero);
});
check('maximum identity', () => {
  const maximum = shape('4294967295');
  assert.equal(resolveConnectorEndpoint([maximum], connector, { ...endpoint, shapeId: 4294967295 }), maximum);
});
for (const shapeId of [NaN, Infinity, -1, 0.1, 4294967296]) {
  check(`invalid endpoint ${String(shapeId)}`, () => assert.equal(resolveConnectorEndpoint([shape(String(shapeId))], connector, { ...endpoint, shapeId }), undefined));
}
const rows = JSON.parse(fs.readFileSync(path.join(run, 'editing-probes/browser-models.json')));
const walk = function* (nodes) {
  for (const node of nodes) {
    yield node;
    if (node.type === 'group') yield* walk(node.children);
  }
};
for (const row of rows) {
  check(`native model ${row.id}`, () => {
    const nodes = row.model.slides[0].nodes;
    for (const node of walk(nodes)) {
      const links = node.editing?.connector;
      for (const endpoint of Object.values(links ?? {})) {
        const resolved = resolveConnectorEndpoint(nodes, node, endpoint);
        const candidates = [...walk(nodes)].filter(other => other.sourcePart === node.sourcePart && Number(other.sourceId) === endpoint.shapeId);
        assert.equal(resolved, candidates.length === 1 ? candidates[0] : undefined);
      }
    }
  });
}
const sha = file => createHash('sha256').update(fs.readFileSync(file)).digest('hex');
const result = { total: receipt.length, passed: receipt.length, module_sha256: sha(modelPath), harness_sha256: sha(new URL(import.meta.url)), models_sha256: sha(path.join(run, 'editing-probes/browser-models.json')), receipts: receipt, scope: 'Direct execution of the built React model endpoint resolver; no geometry routing or editing UI assertion.' };
fs.writeFileSync(path.join(run, 'consumer-probes.json'), JSON.stringify(result, null, 2) + '\n');
console.log(JSON.stringify({ total: result.total, passed: result.passed }));
