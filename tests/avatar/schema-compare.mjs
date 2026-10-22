// Compares avatar-core 0.1.0's `parseAvatarDefinition` with `notch-hud --check-avatar` on a corpus of definitions:
// valid fixtures, many single-rule mutations (one per rule of CONTRACT.md section 10 "Avatar file" 1 to 3), and raw
// text cases (duplicate keys, byte order mark, number forms, escapes, size at and past the limit). It fails when the
// two verdicts (accept or reject) differ for any file, and prints each disagreement.
//
// Usage: node schema-compare.mjs <avatar-core dist/index.js> <notch-hud binary>
// Exit 0 when every verdict agrees, 1 when one differs, 2 on a usage error.
import { readFileSync, writeFileSync, mkdtempSync, mkdirSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const [corePath, bin] = process.argv.slice(2);
if (!corePath || !bin) { console.error('usage: schema-compare.mjs <avatar-core dist/index.js> <notch-hud binary>'); process.exit(2); }
const core = await import(pathToFileURL(corePath).href);
const fixtures = join(dirname(fileURLToPath(import.meta.url)), 'fixtures');
const MAX_BYTES = 262144;

// --- corpus -----------------------------------------------------------------------------------------------------
const cases = []; // { name, bytes: Buffer | string, mustReject?: true }
const names = new Set();
const add = (name, text, extra = {}) => {
  let n = name, i = 2;
  while (names.has(n)) n = `${name} #${i++}`;
  names.add(n);
  cases.push({ name: n, text, ...extra });
};
const clone = (v) => structuredClone(v);
const nextUp = (x) => { const f = new Float64Array([x]); const b = new BigInt64Array(f.buffer); b[0] += x >= 0 ? 1n : -1n; return f[0]; };
const nextDown = (x) => { const f = new Float64Array([x]); const b = new BigInt64Array(f.buffer); b[0] += x > 0 ? -1n : 1n; return f[0]; };

const fx = (n) => JSON.parse(readFileSync(join(fixtures, n + '.json'), 'utf8'));
const rich = fx('ball');
rich.name = 'rich';
rich.standardAnimationSet = 1;
rich.body.primary.morphRoundness = 0.2;
rich.body.primary.tipRoundness = 0.3;
rich.body.primary.baseRoundness = 0.4;
rich.body.nodes = [{ surface: { type: 'cube', width: 10, height: 10, depth: 10, roundness: 0.5, morphRoundness: 0.2, tipRoundness: 0.3, baseRoundness: 0.4 }, position: [1, 2, 3], rotation: [10, 20, 30] }];
rich.expressions.calm.colors = { body: '#112233', eyes: '#445566' };
rich.animations.idle.metadata = { label: 'Idle', description: 'resting', group: 'base' };
rich.animations.idle.blink = { enabled: true, initialDelayMs: 100, minIntervalMs: 300, maxIntervalMs: 900, durationMs: 120 };

// Raw numbers: a value {raw: '1e4'} is written as that text.
const RAW = Symbol('raw');
const raw = (s) => ({ [RAW]: s });
const dump = (v) => {
  const raws = [];
  const t = JSON.stringify(v, (k, x) => (x && typeof x === 'object' && x[RAW] !== undefined ? (raws.push(x[RAW]), `__RAW${raws.length - 1}__`) : x));
  return t.replace(/"__RAW(\d+)__"/g, (_, i) => raws[Number(i)]);
};
const put = (base, path, value) => {
  const v = clone(base);
  const parts = path.split('/').slice(1);
  let o = v;
  for (const p of parts.slice(0, -1)) o = o[p];
  if (value === undefined) delete o[parts.at(-1)]; else o[parts.at(-1)] = value;
  return v;
};
const addAt = (name, base, path, value) => add(name, dump(put(base, path, value)));

for (const n of ['ball', 'drift', 'blink', 'nomatch']) add(`fixture ${n}`, JSON.stringify(fx(n), null, 2));
add('fixture rich', JSON.stringify(rich, null, 1));
add('fixture rich, compact', JSON.stringify(rich));
add('fixture rich, tabs', JSON.stringify(rich, null, '\t'));
add('fixture rich, CRLF', JSON.stringify(rich, null, 2).replace(/\n/g, '\r\n'));

// Numeric bounds: at each limit and one double past it.
const dim = [0.001, 10000], unit = [0, 1], big = [-10000, 10000];
const numeric = [];
for (const k of ['width', 'height', 'depth']) { numeric.push([`/body/primary/${k}`, ...dim]); numeric.push([`/body/nodes/0/surface/${k}`, ...dim]); }
for (const k of ['roundness', 'morphRoundness', 'tipRoundness', 'baseRoundness']) { numeric.push([`/body/primary/${k}`, ...unit]); numeric.push([`/body/nodes/0/surface/${k}`, ...unit]); }
for (let i = 0; i < 3; i++) { numeric.push([`/body/nodes/0/position/${i}`, ...big]); numeric.push([`/body/nodes/0/rotation/${i}`, -360, 360]); }
for (const k of ['x', 'y', 'z']) numeric.push([`/expressions/calm/head/${k}`, ...big]);
numeric.push(['/expressions/calm/eyes/spacing', ...big], ['/expressions/calm/perspective', 0.1, 10]);
for (const side of ['left', 'right']) for (const k of ['width', 'height', 'x', 'y', 'angle']) numeric.push([`/expressions/calm/eyes/${side}/${k}`, ...big]);
numeric.push(['/animations/idle/steps/0/holdMs', 100, 60000], ['/animations/idle/steps/0/transitionMs', 0, 5000]);
numeric.push(['/animations/idle/blink/initialDelayMs', 0, 60000], ['/animations/idle/blink/durationMs', 50, 2000]);
for (const [path, lo, hi] of numeric) {
  for (const [label, v] of [['min', lo], ['max', hi], ['below min', nextDown(lo)], ['above max', nextUp(hi)], ['far below', lo - 1e6], ['far above', hi + 1e6]]) addAt(`${path} ${label} (${v})`, rich, path, v);
}
// Blink intervals: min and max are each 250..120000 and min <= max.
for (const [label, lo, hi] of [
  ['min 250 max 250', 250, 250], ['min 120000 max 120000', 120000, 120000], ['min 249 max 250', 249, 250], ['min 250 max 249', 250, 249],
  ['min above max by one', 301, 300], ['min just above 120000', nextUp(120000), 120000], ['max just above 120000', 250, nextUp(120000)],
  ['min 250 max 120000', 250, 120000], ['min 0 max 0', 0, 0],
]) { const v = put(rich, '/animations/idle/blink/minIntervalMs', lo); v.animations.idle.blink.maxIntervalMs = hi; add(`blink ${label}`, dump(v)); }
// Number spellings.
for (const [label, text, path] of [
  ['exponent 1e4 for width', '1e4', '/body/primary/width'], ['exponent 1E4', '1E4', '/body/primary/width'], ['10000.0', '10000.0', '/body/primary/width'],
  ['1e-3 for width', '1e-3', '/body/primary/width'], ['9.99999e-4 for width', '9.99999e-4', '/body/primary/width'],
  ['0.00099999999999999999 rounds up to 0.001', '0.00099999999999999999', '/body/primary/width'],
  ['10000.0000000000001 rounds down to 10000', '10000.0000000000001', '/body/primary/width'],
  ['1e400 is not finite', '1e400', '/body/primary/width'], ['-1e400', '-1e400', '/expressions/calm/head/x'],
  ['1e20 is out of range', '1e20', '/body/primary/width'], ['100000000000000000000 integer', '100000000000000000000', '/body/primary/width'],
  ['-0 for a rotation', '-0', '/body/nodes/0/rotation/0'], ['-0 for a head x', '-0', '/expressions/calm/head/x'], ['-0.0 for transitionMs', '-0.0', '/animations/idle/steps/0/transitionMs'],
  ['leading zero 01', '01', '/body/primary/width'], ['plus sign +1', '+1', '/body/primary/width'], ['.5', '.5', '/body/primary/width'], ['5.', '5.', '/body/primary/width'],
  ['hex 0x10', '0x10', '/body/primary/width'], ['NaN', 'NaN', '/body/primary/width'], ['Infinity', 'Infinity', '/body/primary/width'],
  ['schemaVersion 1.0', '1.0', '/schemaVersion'], ['schemaVersion 1e0', '1e0', '/schemaVersion'], ['schemaVersion 10e-1', '10e-1', '/schemaVersion'],
  ['schemaVersion 1.0000000000000001 rounds to 1', '1.0000000000000001', '/schemaVersion'], ['schemaVersion 1.0000000000000002', '1.0000000000000002', '/schemaVersion'],
  ['standardAnimationSet 1.0', '1.0', '/standardAnimationSet'], ['standardAnimationSet 1e0', '1e0', '/standardAnimationSet'],
  ['holdMs 1e2', '1e2', '/animations/idle/steps/0/holdMs'], ['holdMs 6e4', '6e4', '/animations/idle/steps/0/holdMs'],
]) addAt(`number text ${label}`, rich, path, raw(text));

// Enums.
for (const [path, good, bad] of [
  ['/body/primary/type', ['sphere', 'mickey', 'cursor', 'cube', 'capsule', 'cylinder', 'cone', 'diamond'], ['torus', 'Sphere', '', 'sphere ', 'cube\n']],
  ['/body/nodes/0/surface/type', ['sphere', 'cube', 'capsule', 'cylinder', 'cone', 'diamond'], ['mickey', 'cursor', 'torus', '']],
  ['/expressions/calm/motion/eyes', ['none', 'microSaccades', 'shake'], ['slowDrift', 'None', 'microsaccades', '']],
  ['/expressions/calm/motion/body', ['none', 'slowDrift', 'shake'], ['microSaccades', 'slowdrift', '']],
  ['/animations/idle/playbackMode', ['loop', 'once', 'pingPong'], ['bounce', 'pingpong', 'Loop', '']],
  ['/animations/idle/steps/0/transition', ['spring', 'smooth', 'snappy'], ['linear', 'Spring', '']],
]) {
  for (const g of good) addAt(`${path} = ${JSON.stringify(g)}`, rich, path, g);
  for (const b of bad) addAt(`${path} = ${JSON.stringify(b)}`, rich, path, b);
  for (const b of [1, null, true, ['none'], {}]) addAt(`${path} = ${JSON.stringify(b)}`, rich, path, b);
}

// Types of required members.
for (const [path, vals] of [
  ['/schema', ['bible-strong/avatar-definition ', 'Bible-Strong/avatar-definition', 1, null]], ['/schemaVersion', [2, 0, '1', null, true, -1]],
  ['/name', [7, null, '', 'x'.repeat(120), 'x'.repeat(121), 'é'.repeat(120), 'é'.repeat(121), '😀'.repeat(120), '😀'.repeat(121), 'é'.repeat(60), 'é'.repeat(61), 'a\nb', '\u0000'.repeat(1)]],
  ['/standardAnimationSet', [1, 2, 0, true, '1', null]],
  ['/animations/idle/blink/enabled', [true, false, 0, 1, 'true', null]],
  ['/body/nodes', [[], {}, null, 'x']], ['/colors', [[], null, 'x']], ['/expressions', [[], null]], ['/animations/idle/steps', [[], {}, null]],
]) for (const v of vals) addAt(`${path} = ${JSON.stringify(v)}`, rich, path, v);

// Colors.
for (const c of ['#000000', '#ffffff', '#0a1b2c', '#ABCDEF', '#AbCdEf', '#abc', '#abcd', '#abcde', '#abcdefa', '#abcdef12', 'abcdef', '#abcdeg', '# abcde', '#abcdef ', ' #abcdef', '#abcdef\n', '#ａｂｃｄｅｆ', 'red', '', 'rgb(0,0,0)']) {
  addAt(`/colors/body = ${JSON.stringify(c)}`, rich, '/colors/body', c);
  addAt(`/colors/eyes = ${JSON.stringify(c)}`, rich, '/colors/eyes', c);
  addAt(`expression body color = ${JSON.stringify(c)}`, rich, '/expressions/calm/colors/body', c);
}
for (const v of [{}, { body: '#112233' }, { eyes: '#112233' }, { body: '#112233', eyes: '#445566' }, { other: '#112233' }, { body: '#112233', extra: 1 }, [], null]) addAt(`expression colors = ${JSON.stringify(v)}`, rich, '/expressions/calm/colors', v);
for (const v of [{}, { label: 'x' }, { description: 'x' }, { group: 'x' }, { label: 'l'.repeat(120) }, { label: 'l'.repeat(121) }, { description: 'd'.repeat(512) }, { description: 'd'.repeat(513) }, { group: 'g'.repeat(64) }, { group: 'g'.repeat(65) }, { label: '😀'.repeat(120) }, { label: '😀'.repeat(121) }, { description: '😀'.repeat(512) }, { description: '😀'.repeat(513) }, { other: 'x' }, { label: 1 }, [], null]) addAt(`metadata = ${JSON.stringify(v).slice(0, 60)}`, rich, '/animations/idle/metadata', v);

// Semantic keys: as an expression name, an animation name, a step's expression and in the order lists.
const keyCases = ['calm', 'a', 'a1', 'a-b', 'a-1', 'a1-b2-c3', 'a'.repeat(64), 'a'.repeat(65), 'Calm', 'cAlm', '1a', '-a', 'a-', 'a--b', 'a_b', 'a b', 'a.b', 'a/b', 'a~b', '', 'é', 'aé', 'calm\n', 'calm ', ' calm', 'a\u0000', '😀', 'ａ', 'a-٣', 'neutral2', 'x-'.repeat(32) + 'x'];
for (const k of keyCases) {
  const v = clone(rich);
  v.expressions[k] = v.expressions.calm;
  v.expressionOrder.push(k);
  add(`extra expression key ${JSON.stringify(k).slice(0, 40)}`, dump(v));
  const a = clone(rich);
  a.animations[k] = a.animations.idle;
  a.animationOrder.push(k);
  add(`extra animation key ${JSON.stringify(k).slice(0, 40)}`, dump(a));
  addAt(`step expression ${JSON.stringify(k).slice(0, 40)}`, rich, '/animations/idle/steps/0/expression', k);
}
// Counts.
const grow = (n, what) => {
  const v = clone(rich);
  if (what === 'expressions') { for (let i = 0; Object.keys(v.expressions).length < n; i++) { v.expressions[`e${i}`] = v.expressions.calm; v.expressionOrder.push(`e${i}`); } }
  if (what === 'animations') { v.animations = {}; v.animationOrder = []; for (let i = 0; i < n; i++) { v.animations[`a${i}`] = clone(rich.animations.idle); v.animationOrder.push(`a${i}`); } }
  if (what === 'steps') v.animations.idle.steps = Array.from({ length: n }, () => clone(rich.animations.idle.steps[0]));
  if (what === 'nodes') v.body.nodes = Array.from({ length: n }, () => clone(rich.body.nodes[0]));
  return v;
};
for (const [what, ns] of [['expressions', [127, 128, 129, 200]], ['animations', [0, 1, 63, 64, 65, 100]], ['steps', [1, 127, 128, 129, 500]], ['nodes', [0, 15, 16, 17, 40]]]) {
  for (const n of ns) add(`${n} ${what}`, dump(grow(n, what)));
}
{
  const v = clone(rich); v.animations = {}; v.animationOrder = []; add('no animations', dump(v));
  const w = clone(rich); w.expressions = { neutral: w.expressions.neutral }; w.expressionOrder = ['neutral']; w.animations.idle.steps = [{ expression: 'neutral', holdMs: 500, transitionMs: 100, transition: 'smooth' }]; w.animations = { idle: w.animations.idle }; w.animationOrder = ['idle']; add('only neutral', dump(w));
  const x = clone(rich); delete x.expressions.neutral; x.expressionOrder = x.expressionOrder.filter((k) => k !== 'neutral'); x.animations = { idle: { ...x.animations.idle, steps: [{ ...x.animations.idle.steps[0], expression: 'calm' }] } }; x.animationOrder = ['idle']; add('no neutral', dump(x));
}
// Orders (rule 3).
{
  const o = rich.expressionOrder;
  for (const [label, v] of [
    ['expressionOrder neutral first', o], ['expressionOrder reversed', [...o].reverse()], ['expressionOrder neutral second', [o[1], o[0], ...o.slice(2)]],
    ['expressionOrder missing a key', o.slice(0, -1)], ['expressionOrder extra unknown', [...o, 'ghost']], ['expressionOrder duplicate', [...o, o[1]]],
    ['expressionOrder only neutral', ['neutral']], ['expressionOrder empty', []], ['expressionOrder a string', 'neutral'], ['expressionOrder with a number', [...o, 1]],
    ['expressionOrder unknown instead of a key', [o[0], 'ghost', ...o.slice(2)]],
  ]) addAt(label, rich, '/expressionOrder', v);
  const a = rich.animationOrder;
  for (const [label, v] of [
    ['animationOrder as is', a], ['animationOrder reversed', [...a].reverse()], ['animationOrder missing', a.slice(0, -1)], ['animationOrder extra unknown', [...a, 'ghost']],
    ['animationOrder duplicate', [...a, a[0]]], ['animationOrder empty', []], ['animationOrder with a number', [...a, 1]],
  ]) addAt(label, rich, '/animationOrder', v);
}
// Unknown and missing members, in every object.
const objects = ['', '/body', '/body/primary', '/body/nodes/0', '/body/nodes/0/surface', '/colors', '/expressions', '/expressions/calm', '/expressions/calm/head', '/expressions/calm/eyes', '/expressions/calm/eyes/left', '/expressions/calm/motion', '/expressions/calm/colors', '/animations', '/animations/idle', '/animations/idle/steps/0', '/animations/idle/blink', '/animations/idle/metadata'];
for (const p of objects) {
  const get = (v) => p.split('/').slice(1).reduce((o, k) => o[k], v);
  for (const k of ['extra', 'a/b', 'a~b', 'é', 'constructor', 'Extra', '']) {
    if (p === '/expressions' || p === '/animations') continue;
    const v = clone(rich); get(v)[k] = 1; add(`unknown member ${JSON.stringify(k)} in ${p || '/'}`, dump(v));
  }
  for (const k of Object.keys(get(rich))) {
    const v = clone(rich); delete get(v)[k]; add(`missing member ${k} in ${p || '/'}`, dump(v));
  }
}
add('unknown member __proto__ at the root, raw', dump(rich).replace(/^\{/, '{"__proto__":{"x":1},'));
add('unknown member __proto__ as an object in colors, raw', dump(rich).replace('"colors":{', '"colors":{"__proto__":{"body":"#000000"},'));
// Raw text: JSON syntax, duplicate keys, whitespace, escapes, size.
const text = JSON.stringify(rich);
const raws = [
  ['empty', ''], ['whitespace only', ' \n'], ['BOM before the object', '﻿' + text], ['trailing garbage', text + ' x'], ['trailing newline', text + '\n'], ['leading whitespace', '\n\t ' + text],
  ['two documents', text + text], ['trailing comma in the root', text.replace(/\}$/, ',}')], ['trailing comma in an array', text.replace('["neutral"', '[,"neutral"')],
  ['comment', '/* c */' + text], ['line comment', text + '// c'], ['single quotes', text.replace(/"/g, "'")], ['unquoted keys', text.replace('"schema":', 'schema:')],
  ['truncated', text.slice(0, text.length - 5)], ['truncated mid string', text.slice(0, 30)], ['bare null', 'null'], ['bare number', '1'], ['bare string', '"x"'], ['array root', '[]'], ['true', 'true'],
  ['vertical tab between tokens', text.replace('"schema":', '"schema":\u000b')], ['form feed between tokens', text.replace('"schema":', '"schema":\f')], ['no-break space between tokens', text.replace('"schema":', '"schema": ')],
  ['ideographic space', text.replace('"schema":', '"schema":　')], ['line separator between tokens', text.replace('"schema":', '"schema": ')],
  ['unescaped newline in a string', text.replace('"rich"', '"ri\nch"')], ['unescaped tab in a string', text.replace('"rich"', '"ri\tch"')], ['unescaped control char', text.replace('"rich"', '"ri\u0001ch"')], ['DEL in a string', text.replace('"rich"', '"ri\u007fch"')],
  ['escaped name \\u00e9', text.replace('"rich"', '"r\\u00e9"')], ['escaped surrogate pair', text.replace('"rich"', '"\\ud83d\\ude00"')], ['lone high surrogate escape', text.replace('"rich"', '"\\ud83d"')],
  ['lone low surrogate escape', text.replace('"rich"', '"\\ude00"')], ['invalid escape \\x', text.replace('"rich"', '"\\x41"')], ['bad unicode escape', text.replace('"rich"', '"\\u12"')], ['escaped slash', text.replace('"rich"', '"a\\/b"')],
  ['escaped quote', text.replace('"rich"', '"a\\"b"')], ['\\u0000 in a string', text.replace('"rich"', '"a\\u0000b"')],
  ['schema key spelled with escapes', text.replace('"schema":', '"\\u0073chema":')], ['expression key spelled with escapes', text.replace('"calm":{', '"ca\\u006cm":{')],
  ['duplicate key by escape', text.replace('"schemaVersion":1', '"schemaVersion":1,"schema\\u0056ersion":1')], ['duplicate root key', text.replace('"schemaVersion":1', '"schemaVersion":1,"schemaVersion":1')],
  ['duplicate root key with another value', text.replace('"schemaVersion":1', '"schemaVersion":1,"schemaVersion":2')], ['duplicate name', text.replace('"name":"rich"', '"name":"rich","name":"again"')],
  ['duplicate in colors', text.replace('"colors":{', '"colors":{"body":"#000000",')], ['duplicate in body', text.replace('"body":{"primary"', '"body":{"nodes":[],"primary"')],
  ['duplicate expression name', text.replace('"calm":{', '"calm":{"head":{"x":0,"y":0,"z":0},"eyes":{"left":{"width":1,"height":1,"x":0,"y":0,"angle":0},"right":{"width":1,"height":1,"x":0,"y":0,"angle":0},"spacing":0},"perspective":1,"motion":{"eyes":"none","body":"none"}},"calm":{')],
  ['duplicate animation name', text.replace('"idle":{', '"idle":{"playbackMode":"loop","steps":[{"expression":"calm","holdMs":500,"transitionMs":100,"transition":"smooth"}],"blink":{"enabled":false,"initialDelayMs":0,"minIntervalMs":250,"maxIntervalMs":250,"durationMs":50}},"idle":{')],
  ['duplicate step member', text.replace('"holdMs":400', '"holdMs":400,"holdMs":400')], ['duplicate in a position array object', text.replace('"surface":{"type":"cube"', '"surface":{"type":"cube","type":"cube"')],
  ['duplicate key inside an unknown member', text.replace(/\}$/, ',"extra":{"a":1,"a":2}}')], ['duplicate in metadata', text.replace('"label":"Idle"', '"label":"Idle","label":"Idle"')],
  ['depth 33 in an unknown member', text.replace(/\}$/, ',"extra":' + '['.repeat(40) + ']'.repeat(40) + '}')], ['depth 33 array in a valid place', text.replace('"nodes":[{', '"nodes":[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[[{')],
  ['uppercase TRUE', text.replace('"enabled":true', '"enabled":TRUE')], ['undefined', text.replace('"enabled":true', '"enabled":undefined')],
];
for (const [name, t] of raws) add(`raw ${name}`, t);
// Size: exactly MAX_BYTES, one more, and multibyte accounting.
{
  const pad = (n, extra = '') => text + ' '.repeat(n - Buffer.byteLength(text + extra)) + extra;
  add('size exactly 262144 bytes (trailing spaces)', pad(MAX_BYTES));
  add('size 262145 bytes (trailing spaces)', pad(MAX_BYTES + 1));
  add('size 262143 bytes (trailing spaces)', pad(MAX_BYTES - 1));
  add('size 1 MiB (trailing spaces)', pad(1 << 20));
  const e = clone(rich); e.name = 'é'.repeat(120);
  const et = JSON.stringify(e);
  add('multibyte name, exactly 262144 bytes', et + ' '.repeat(MAX_BYTES - Buffer.byteLength(et)));
  add('multibyte name, 262145 bytes', et + ' '.repeat(MAX_BYTES + 1 - Buffer.byteLength(et)));
  const w = clone(rich); w.name = '😀'.repeat(120);
  const wt = JSON.stringify(w);
  add('four-byte name, exactly 262144 bytes', wt + ' '.repeat(MAX_BYTES - Buffer.byteLength(wt)));
  add('four-byte name, 262145 bytes', wt + ' '.repeat(MAX_BYTES + 1 - Buffer.byteLength(wt)));
  // a big valid document: 128 expressions and 64 animations with 128 steps each is well past 262144 bytes
  const b = clone(rich); b.animations = {}; b.animationOrder = [];
  for (let i = 0; i < 64; i++) { b.animations[`a${i}`] = { ...clone(rich.animations.idle), steps: Array.from({ length: 128 }, () => clone(rich.animations.idle.steps[0])) }; b.animationOrder.push(`a${i}`); }
  add('64 animations of 128 steps (large, compact)', JSON.stringify(b));
  add('64 animations of 128 steps (pretty, over the limit)', JSON.stringify(b, null, 4));
}
// Not UTF-8: Rust must reject; the package reads text, so there is no verdict to compare.
cases.push({ name: 'bytes that are not UTF-8', text: Buffer.concat([Buffer.from(text.slice(0, 20)), Buffer.from([0xff, 0xfe]), Buffer.from(text.slice(20))]), mustReject: true });
cases.push({ name: 'overlong UTF-8 in the name', text: Buffer.concat([Buffer.from(text.replace('"rich"', '"r')), Buffer.from([0xc0, 0xaf]), Buffer.from('"' + text.split('"rich"')[1])]), mustReject: true });
cases.push({ name: 'a lone UTF-8 continuation byte', text: Buffer.concat([Buffer.from(text.slice(0, 20)), Buffer.from([0x80]), Buffer.from(text.slice(20))]), mustReject: true });

// --- run --------------------------------------------------------------------------------------------------------
const dir = mkdtempSync(join(tmpdir(), 'avatar-schema-'));
mkdirSync(dir, { recursive: true });
let accepted = 0, rejected = 0;
const bad = [];
// CONTRACT.md section 10: the one deliberate difference. A string holding an unpaired UTF-16 surrogate escape is
// rejected by notch-hud and accepted by avatar-core.
const EXPECTED = new Set(['raw lone high surrogate escape', 'raw lone low surrogate escape']);
let expected = 0;
cases.forEach((c, i) => {
  const file = join(dir, `case-${i}.json`);
  const bytes = Buffer.isBuffer(c.text) ? c.text : Buffer.from(c.text, 'utf8');
  writeFileSync(file, bytes);
  let official;
  if (c.mustReject) official = { ok: false, why: 'must be rejected (not UTF-8)' };
  else {
    const r = core.parseAvatarDefinition(c.text);
    official = r.ok ? { ok: true } : { ok: false, why: `${r.errors[0].path || '/'}: ${r.errors[0].code}` };
  }
  const run = spawnSync(bin, ['--check-avatar', file], { encoding: 'utf8', timeout: 5000, env: { ...process.env, NOTCH_DIR: join(dir, 'sessions') } });
  const ours = run.status === 0 ? 'accept' : run.status === 1 ? 'reject' : `exit ${run.status} signal ${run.signal}`;
  official.ok ? accepted++ : rejected++;
  if (EXPECTED.has(c.name) && official.ok && ours === 'reject') {
    expected++;
    console.log(`EXPECTED ${c.name}: avatar-core accepts; notch-hud rejects (contract section 10)`);
  } else if ((official.ok && ours !== 'accept') || (!official.ok && ours !== 'reject')) {
    bad.push(c.name);
    console.log(`DISAGREE ${c.name}: avatar-core ${official.ok ? 'accepts' : 'rejects (' + official.why + ')'}; notch-hud ${ours}${run.stderr ? ' [' + run.stderr.trim().slice(0, 160) + ']' : ''}`);
  }
});
console.log(`${cases.length} files: avatar-core accepts ${accepted}, rejects ${rejected}; ${bad.length} disagreement(s), ${expected} expected`);
if (expected !== EXPECTED.size) { console.log(`FAIL want exactly ${EXPECTED.size} expected disagreements, saw ${expected}`); process.exit(1); }
if (accepted < 20 || rejected < 200) { console.log(`FAIL the corpus is lopsided (accept ${accepted}, reject ${rejected}); a mutation helper is broken`); process.exit(1); }
const fixtureBad = cases.slice(0, 9).filter((c) => !core.parseAvatarDefinition(c.text).ok).map((c) => c.name);
if (fixtureBad.length) { console.log(`FAIL the official parser rejects our own fixtures: ${fixtureBad.join(', ')}`); process.exit(1); }
process.exit(bad.length ? 1 : 0);
