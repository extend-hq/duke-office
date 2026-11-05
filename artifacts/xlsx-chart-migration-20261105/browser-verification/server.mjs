import { createServer } from '/Users/andrewluo/react-xlsx/apps/playground/node_modules/vite/dist/node/index.js';
import { readFile, realpath } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = dirname(fileURLToPath(import.meta.url));
const source = process.env.XLSX_SOURCE || '/private/tmp/react-xlsx-rust-migration-20261105';
const office = process.env.OFFICE_SOURCE || '/Users/andrewluo/extend-office';
const corpus = '/Users/andrewluo/Documents/xlsx-testing';
const server = await createServer({
  configFile: false,
  root,
  cacheDir: resolve(root, '.vite'),
  resolve: {
    alias: [
      { find: '@xlsx', replacement: resolve(source, 'packages/react-xlsx/src') },
      { find: /^@extend-ai\/rust-office-charts\/full$/, replacement: resolve(office, 'browser/full.js') },
      { find: /^@extend-ai\/rust-office-charts$/, replacement: resolve(office, 'browser/index.js') },
      { find: /^react-dom\/client$/, replacement: resolve(source, 'packages/react-xlsx/node_modules/react-dom/client.js') },
      { find: /^react$/, replacement: resolve(source, 'packages/react-xlsx/node_modules/react/index.js') },
    ],
    dedupe: ['react', 'react-dom'],
  },
  esbuild: { jsx: 'transform' },
  server: { host: '127.0.0.1', port: Number(process.env.PORT || 4189), strictPort: true, fs: { allow: [root, source, office, '/Users/andrewluo/react-xlsx'] } },
  plugins: [{
    name: 'chart-verification-input',
    configureServer(server) {
      server.middlewares.use(async (req, res, next) => {
        const url = new URL(req.url, 'http://127.0.0.1');
        if (url.pathname !== '/workbook') return next();
        try {
          const path = await realpath(url.searchParams.get('path') || '');
          if (!path.startsWith(corpus + '/')) throw new Error('Input is outside the corpus');
          res.setHeader('Content-Type', 'application/octet-stream');
          res.end(await readFile(path));
        } catch (error) { res.statusCode = 400; res.end(String(error)); }
      });
    },
  }],
});
await server.listen();
console.log('Chart verification server: ' + server.resolvedUrls.local[0]);
