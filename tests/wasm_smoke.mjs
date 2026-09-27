// Smoke test for the WebAssembly engine used by exported games:
//   node tests/wasm_smoke.mjs
// Loads tests/fixtures/coin-dash, holds "right" for 3 ticks, and checks the knight moved exactly
// like the native engine does (x 2 -> 5). Exits non-zero on failure.
import { readFileSync, readdirSync, existsSync } from 'node:fs';
import { join } from 'node:path';

const root = new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const wasm = readFileSync(join(root, 'target/wasm32-unknown-unknown/release/forge_web.wasm'));
const { instance } = await WebAssembly.instantiate(wasm, {});
const ex = instance.exports, enc = new TextEncoder(), dec = new TextDecoder();
const send = (fn, s) => { const b = enc.encode(s); const p = ex.fg_alloc(b.length); new Uint8Array(ex.memory.buffer).set(b, p); return ex[fn](p, b.length); };
const take = packed => { const big = BigInt(packed); return dec.decode(new Uint8Array(ex.memory.buffer, Number(big >> 32n), Number(big & 0xffffffffn))); };

const dir = join(root, 'tests/fixtures/coin-dash');
const world = JSON.parse(readFileSync(join(dir, 'world.json'), 'utf8'));
world.scripts = {};
for (const f of readdirSync(join(dir, 'scripts'))) world.scripts[f.replace(/\.rhai$/, '')] = readFileSync(join(dir, 'scripts', f), 'utf8');

const fail = m => { console.error('FAIL:', m); process.exit(1); };
if (!send('fg_load', JSON.stringify(world))) fail(take(ex.fg_error()));
send('fg_input', JSON.stringify({ down: ['ArrowRight'] }));
ex.fg_tick(3);
const f = JSON.parse(take(ex.fg_frame()));
const hero = f.ents.find(e => e[4] === 'hero');
if (!hero) fail('no hero in frame');
if (hero[2] !== 5 || hero[3] !== 7) fail(`hero at ${hero[2]},${hero[3]}, expected 5,7`);
if (f.tick !== 3) fail(`tick ${f.tick}`);
if (!String(f.hud.hp || '').includes('♥')) fail('HUD missing');
const map = JSON.parse(take(ex.fg_map()));
if (map.length !== 15) fail('map rows');
const t0 = performance.now(); ex.fg_tick(600); const ms = performance.now() - t0;
console.log(`ok: hero moved to (5,7), HUD "${f.hud.hp}", 600 ticks in ${ms.toFixed(0)} ms`);
if (existsSync(join(root, 'exports'))) console.log('(exports folder present)');
