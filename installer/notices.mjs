#!/usr/bin/env node
// Writes THIRD-PARTY-NOTICES.txt at the root of the repository: the licences of everything
// Mosaïk's programs are built from (the Rust crates compiled into mosaik.exe,
// mosaik-watchdog.exe and mosaikctl.exe; the JavaScript packages and fonts of the settings
// window, which mosaik.exe embeds), with texts that are word for word the same printed once.
//
//   node installer/notices.mjs           after `cargo fetch` and `npm ci` in apps/desktop
//   node installer/notices.mjs --check   fails when the file is out of date (CI)
//
// Each crate's own licence files are used; for a crate offered under several licences, the
// first of PREFERRED it ships a file for. Crates published without a licence file take the one
// from their repository, kept in installer/licenses (BORROWED).

import { execFileSync } from 'node:child_process';
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const OUT = join(ROOT, 'THIRD-PARTY-NOTICES.txt');
const DESKTOP = join(ROOT, 'apps', 'desktop');
const TARGET = 'x86_64-pc-windows-msvc';
/** The workspace packages whose programs ship, and so everything they link. */
const SHIPPED = ['mosaik-desktop', 'mosaik-watchdog', 'mosaik-ipc'];
/** Which licence of a choice to print: the shortest to honour first. */
const PREFERRED = [
  'MIT',
  'MIT-0',
  '0BSD',
  'ISC',
  'Zlib',
  'BSD-3-Clause',
  'Unlicense',
  'CC0-1.0',
  'Apache-2.0',
  'Unicode-3.0',
  'MPL-2.0',
];
/** Crates published without their licence file: the file from their repository. */
const BORROWED = {
  'webview2-com': 'webview2-rs-MIT.txt',
  'webview2-com-sys': 'webview2-rs-MIT.txt',
  'webview2-com-macros': 'webview2-rs-MIT.txt',
  'defmt-parser': 'defmt-MIT.txt',
  'alloc-stdlib': 'rust-alloc-no-stdlib-BSD-3-Clause.txt',
  selectors: 'MPL-2.0.txt',
};
/** Build tools whose small runtime helpers end up in the settings window's script and styles. */
const BUNDLED_TOOLS = ['tailwindcss', 'rolldown', 'vite'];

/** What a licence text is, from its words. */
function identify(text) {
  const t = text.replace(/\s+/g, ' ');
  const found = [];
  if (/Permission is hereby granted, free of charge/i.test(t)) found.push('MIT');
  if (/Apache License,? Version 2\.0/i.test(t)) found.push('Apache-2.0');
  if (/Mozilla Public License,? Version 2\.0/i.test(t)) found.push('MPL-2.0');
  if (/Redistribution and use in source and binary forms/i.test(t)) found.push('BSD-3-Clause');
  if (/This software is provided ['‘]as-is['’]/i.test(t)) found.push('Zlib');
  if (/free and unencumbered software released into the public domain/i.test(t)) found.push('Unlicense');
  if (/UNICODE LICENSE V3|UNICODE, INC\. LICENSE AGREEMENT|Unicode Data Files/i.test(t)) found.push('Unicode-3.0');
  if (/Creative Commons Legal Code|CC0 1\.0 Universal/i.test(t)) found.push('CC0-1.0');
  if (/Permission to use, copy, modify, and\/or distribute this software for any purpose/i.test(t)) {
    found.push(/above copyright notice and this permission notice appear/i.test(t) ? 'ISC' : '0BSD');
  }
  if (/SIL OPEN FONT LICENSE/i.test(t)) found.push('OFL-1.1');
  return found;
}

/** The licences a file name says it holds (LICENSE-MIT, UNLICENSE...); none for a plain LICENSE. */
function named(file) {
  const f = file.toUpperCase();
  if (/APACHE/.test(f)) return ['Apache-2.0'];
  if (/MIT/.test(f)) return ['MIT'];
  if (/ZLIB/.test(f)) return ['Zlib'];
  if (/UNLICENSE/.test(f)) return ['Unlicense'];
  if (/UNICODE/.test(f)) return ['Unicode-3.0'];
  if (/BSD/.test(f)) return ['BSD-3-Clause'];
  return [];
}

/** Lines without trailing spaces or a carriage return, no blank lines at either end. */
function clean(text) {
  return text
    .replace(/\r\n?/g, '\n')
    .split('\n')
    .map((l) => l.replace(/\s+$/, ''))
    .join('\n')
    .replace(/^\n+|\n+$/g, '');
}

/** "(MIT OR Apache-2.0) AND Unicode-3.0" -> [["MIT", "Apache-2.0"], ["Unicode-3.0"]]. */
function terms(expression) {
  return expression
    .replace(/[()]/g, ' ')
    .split(/\s+AND\s+/)
    .map((term) =>
      term
        .split(/\s+OR\s+|\s*\/\s*/)
        .map((s) => s.trim())
        .filter(Boolean),
    );
}

/** The texts to print for one package: for every AND term, one licence it ships a file for. */
function textsFor(name, version, dir, expression) {
  const files = existsSync(dir)
    ? readdirSync(dir).filter((f) => /^(licen[cs]e|copying|copyright|unlicense|notice)/i.test(f))
    : [];
  const read = (f) => clean(readFileSync(join(dir, f), 'utf8'));
  const notices = files.filter((f) => /^notice/i.test(f)).map(read);
  const candidates = files
    .filter((f) => !/^notice/i.test(f))
    .map((f) => {
      const text = read(f);
      const says = named(f);
      return { text, licences: says.length ? says : identify(text) };
    })
    .filter((c) => c.licences.length > 0);
  if (BORROWED[name]) {
    const text = clean(readFileSync(join(ROOT, 'installer', 'licenses', BORROWED[name]), 'utf8'));
    candidates.push({ text, licences: identify(text) });
  }
  const texts = [];
  for (const choice of terms(expression)) {
    const order = [...choice].sort((a, b) => rank(a) - rank(b));
    const pick = order.map((l) => candidates.find((c) => c.licences.includes(l))).find(Boolean);
    if (!pick) {
      throw new Error(
        `${name} ${version} (${expression}): no licence file for ${choice.join(' or ')} in ${dir}. Add one to BORROWED.`,
      );
    }
    if (!texts.includes(pick.text)) texts.push(pick.text);
  }
  return [...texts, ...notices];
}

function rank(licence) {
  const i = PREFERRED.indexOf(licence);
  return i < 0 ? PREFERRED.length : i;
}

/** The crates compiled into the shipped programs for Windows x64: `cargo tree` resolves features
 * as the build does, and leaves out build scripts' dependencies, test-only ones and procedural
 * macros (they run while compiling; none of their code ships). `cargo metadata` says where each
 * one's files are. */
function crates() {
  const run = (args) =>
    execFileSync('cargo', args, {
      cwd: ROOT,
      maxBuffer: 1 << 28,
      encoding: 'utf8',
    });
  const tree = run([
    'tree',
    '--locked',
    '--target',
    TARGET,
    '-e',
    'normal,no-proc-macro',
    '--prefix',
    'none',
    '--format',
    '{p}',
    ...SHIPPED.flatMap((p) => ['-p', p]),
  ]);
  const shipped = new Set(
    tree
      .split('\n')
      .map((l) => l.replace(/ \(\*\)$/, '').trim())
      .filter((l) => l && !l.includes('(')) // the workspace's own crates show their path
      .map((l) => l.replace(/ v(\S+)$/, ' $1')),
  );
  const meta = JSON.parse(run(['metadata', '--format-version', '1', '--locked', '--filter-platform', TARGET]));
  const found = meta.packages
    .filter((p) => shipped.has(`${p.name} ${p.version}`))
    .map((p) => ({
      name: p.name,
      version: p.version,
      dir: dirname(p.manifest_path),
      licence: p.license ?? '',
    }));
  if (found.length !== shipped.size) throw new Error('cargo tree and cargo metadata disagree about the crates.');
  return found;
}

/** The settings window's packages: runtime dependencies of apps/desktop, and what they need. */
function packagesOfTheInterface() {
  const modules = join(DESKTOP, 'node_modules');
  const manifest = (name) => JSON.parse(readFileSync(join(modules, name, 'package.json'), 'utf8'));
  const todo = [
    ...Object.keys(JSON.parse(readFileSync(join(DESKTOP, 'package.json'), 'utf8')).dependencies),
    ...BUNDLED_TOOLS,
  ];
  const seen = new Map();
  while (todo.length) {
    const name = todo.pop();
    if (seen.has(name)) continue;
    if (!existsSync(join(modules, name, 'package.json')))
      throw new Error(`${name} is not installed: run npm ci in apps/desktop first.`);
    const m = manifest(name);
    seen.set(name, m);
    if (!BUNDLED_TOOLS.includes(name)) todo.push(...Object.keys(m.dependencies ?? {}));
  }
  return [...seen.entries()].map(([name, m]) => ({
    name,
    version: m.version,
    dir: join(modules, name),
    licence: typeof m.license === 'string' ? m.license : '',
  }));
}

/** Vite's licence file also lists the licences of the tools it bundles for itself: only its own. */
function trimmed(name, text) {
  return name === 'vite'
    ? clean(text.split(/^# Licenses of bundled dependencies/m)[0].replace(/^# Vite core license\n/, ''))
    : text;
}

function notices() {
  const groups = new Map(); // text -> packages
  const add = (section, list) => {
    for (const p of list) {
      for (const raw of textsFor(p.name, p.version, p.dir, p.licence)) {
        const text = trimmed(p.name, raw);
        const key = `${section}\u0000${text}`;
        if (!groups.has(key)) groups.set(key, { section, text, names: [] });
        groups.get(key).names.push(`${p.name} ${p.version}`);
      }
    }
  };
  const rust = crates();
  const web = packagesOfTheInterface();
  add('rust', rust);
  add('web', web);
  const rule = '='.repeat(78);
  const lines = [
    'Mosaïk: third-party notices',
    '',
    'Mosaïk itself is dedicated to the public domain (CC0 1.0). It is built with the open-source',
    'software below, which keeps its own licences. Each licence text is printed once, after the',
    'list of everything it applies to. The source of each component is published under its name',
    'and version on crates.io (Rust crates) or npmjs.com (JavaScript packages and fonts).',
    '',
    `Rust crates compiled into mosaik.exe, mosaik-watchdog.exe and mosaikctl.exe: ${rust.length}.`,
    `JavaScript packages and fonts of the settings window, inside mosaik.exe: ${web.length}.`,
    '',
    'This file is generated by installer/notices.mjs.',
  ];
  const sorted = [...groups.values()]
    .map((g) => ({ ...g, names: g.names.sort((a, b) => a.localeCompare(b)) }))
    .sort((a, b) => a.section.localeCompare(b.section) || a.names[0].localeCompare(b.names[0]));
  for (const g of sorted) lines.push('', rule, ...wrap(g.names.join(', ')), rule, '', g.text);
  return `${lines.join('\n')}\n`;
}

/** Lines of at most 78 characters, broken after commas. */
function wrap(list) {
  const out = [];
  let line = '';
  for (const part of list.split(', ')) {
    const next = line ? `${line}, ${part}` : part;
    if (next.length > 78 && line) {
      out.push(`${line},`);
      line = part;
    } else {
      line = next;
    }
  }
  if (line) out.push(line);
  return out;
}

const text = notices();
if (process.argv.includes('--check')) {
  const current = existsSync(OUT) ? readFileSync(OUT, 'utf8') : '';
  if (current !== text) {
    console.error('THIRD-PARTY-NOTICES.txt is out of date: run node installer/notices.mjs and commit it.');
    process.exit(1);
  }
  console.log('THIRD-PARTY-NOTICES.txt is up to date.');
} else {
  writeFileSync(OUT, text);
  console.log(`Wrote THIRD-PARTY-NOTICES.txt (${Math.round(text.length / 1024)} KB).`);
}
