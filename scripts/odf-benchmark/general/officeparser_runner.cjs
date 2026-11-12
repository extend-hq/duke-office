// Warm in-process officeparser timings, in the same record shape as the Rust runners.
// Usage: node officeparser_runner.cjs <list file> <output directory>  (run from a directory where officeparser is installed)
const fs = require('fs');
const path = require('path');
const officeParser = require(path.join(process.cwd(), 'node_modules', 'officeparser'));

async function extract(buffer) {
  const ast = await officeParser.parseOffice(buffer);
  return (await ast.to('text')).value;
}

(async () => {
  const [list, output] = process.argv.slice(2);
  for (const line of fs.readFileSync(list, 'utf8').split('\n')) {
    if (!line) continue;
    const family = path.basename(path.dirname(line));
    const name = path.basename(line);
    const bytes = fs.readFileSync(line);
    let text = null;
    let error = '';
    const firstStart = process.hrtime.bigint();
    try { text = await extract(bytes); } catch (problem) { error = String(problem); }
    const first = Number(process.hrtime.bigint() - firstStart) / 1e6;
    let total = 0, min = Infinity, reps = 0;
    if (text !== null && first < 5000) {
      const begin = process.hrtime.bigint();
      while (reps < 3 || (Number(process.hrtime.bigint() - begin) < 250e6 && reps < 200)) {
        const start = process.hrtime.bigint();
        try { await extract(bytes); } catch (ignored) {}
        const elapsed = Number(process.hrtime.bigint() - start) / 1e6;
        total += elapsed; min = Math.min(min, elapsed); reps += 1;
      }
    }
    if (reps === 0) { total = first; min = first; reps = 1; }
    if (text !== null) {
      fs.mkdirSync(path.join(output, family), { recursive: true });
      fs.writeFileSync(path.join(output, family, name + '.txt'), text);
    }
    console.log(JSON.stringify({ file: `${family}/${name}`, ok: text !== null, error, bytes: bytes.length,
      chars: text === null ? 0 : text.length, first_ms: first, mean_ms: total / reps, min_ms: min, reps }));
  }
})();
