import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const projectRoot = path.dirname(fileURLToPath(import.meta.url));
const productionAssets = path.resolve(projectRoot, '../../server/internal/webclient/assets');

function replaceOnce(source, before, after) {
  if (source.split(before).length !== 2) throw new Error('The production viewer markup changed; review the readiness gate before building.');
  return source.replace(before, after);
}

// Keep the controller, protocol decoder and styles byte-for-byte identical to
// the coordinator's production viewer. Only deployment admission is added.
export async function buildViewer({ outputRoot = path.join(projectRoot, 'dist'),
  publicRoot = path.join(projectRoot, 'public') } = {}) {
  const target = path.join(outputRoot, 'connect');
  await mkdir(target, { recursive: true });
  // The gateway serves these assets with its exact URL/method/header rules.
  // Vercel's static output stays empty so it cannot bypass those checks.
  await mkdir(publicRoot, { recursive: true });
  const files = new Map(await Promise.all(['app.js', 'core.mjs', 'style.css'].map(async name =>
    [name, await readFile(path.join(productionAssets, name))])));
  let html = await readFile(path.join(productionAssets, 'index.html'), 'utf8');
  html = replaceOnce(html, 'src="/connect/app.js"', 'src="/connect/boot.mjs"');
  html = replaceOnce(html, '<input id="code" ', '<input id="code" disabled ');
  html = replaceOnce(html, '<input id="pin" ', '<input id="pin" disabled ');
  html = replaceOnce(html, '<button id="connect" class="primary" type="submit">Connect remote desktop ',
    '<button id="connect" class="primary" type="button" disabled>Checking connection service… ');
  html = replaceOnce(html, '    <div id="status" class="status" role="status" aria-live="polite" hidden></div>\n', '');
  html = replaceOnce(html, '        <form id="connect-form" autocomplete="off">',
    '        <div id="status" class="status" role="status" aria-live="polite">Checking the connection service…</div>\n' +
    '        <form id="connect-form" autocomplete="off">');
  files.set('index.html', Buffer.from(html));
  files.set('boot.mjs', await readFile(path.join(projectRoot, 'web/boot.mjs')));
  for (const [name, bytes] of files) await writeFile(path.join(target, name), bytes);
  return Object.fromEntries([...files].map(([name, bytes]) =>
    ['connect/' + name, createHash('sha256').update(bytes).digest('hex')]));
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const hashes = await buildViewer();
  process.stdout.write(JSON.stringify({ viewerFiles: hashes }, null, 2) + '\n');
}
