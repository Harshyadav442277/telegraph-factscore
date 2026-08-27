#!/usr/bin/env node

/**
 * Fixture corpus loader + statistics used by run-eval.mjs.
 *
 * Record shape is FIXTURES.md's, with two additions the spec asks for in prose
 * but does not put in the example JSON:
 *   - "class"       the fixture class (REAL, FACT-SWAP, ... ) this record probes
 *   - "constraints" near-equality requirements, for the classes FIXTURES.md calls
 *                   constraint classes (FORMAT-EQUIVALENCE, UNIT/FORM, LENGTH):
 *                   { "type": "near_equal", "ids": [...], "tolerance": 0.10, "note": "..." }
 */

import { readdir, readFile } from "node:fs/promises";
import { join, extname } from "node:path";

export async function loadCorpus(dirs, intentFilter) {
  const records = [];
  const files = [];
  for (const dir of dirs) {
    let names;
    try {
      names = (await readdir(dir)).filter((n) => extname(n) === ".json").sort();
    } catch {
      continue; // a missing fixture dir is reported by the caller, not fatal here
    }
    for (const name of names) {
      const path = join(dir, name);
      const body = JSON.parse(await readFile(path, "utf8"));
      const rows = Array.isArray(body) ? body : body.fixtures;
      if (!Array.isArray(rows)) throw new Error(`${path}: expected an array or { fixtures: [...] }`);
      files.push({ path, count: rows.length });
      for (const row of rows) {
        validate(row, path);
        if (intentFilter && intentFilter !== "all" && row.intent !== intentFilter) continue;
        records.push({ ...row, source_file: path });
      }
    }
  }
  return { records, files };
}

function validate(row, path) {
  for (const key of ["intent", "question", "ground_truth", "answers"]) {
    if (row[key] === undefined) throw new Error(`${path}: fixture ${row.id ?? "?"} is missing "${key}"`);
  }
  const ids = new Set();
  for (const answer of row.answers) {
    if (typeof answer.id !== "string" || typeof answer.text !== "string") {
      throw new Error(`${path}: fixture ${row.id ?? "?"} has an answer without id/text`);
    }
    if (ids.has(answer.id)) throw new Error(`${path}: fixture ${row.id ?? "?"} repeats answer id ${answer.id}`);
    ids.add(answer.id);
  }
  for (const [better, worse] of row.pairs ?? []) {
    if (!ids.has(better) || !ids.has(worse)) {
      throw new Error(`${path}: fixture ${row.id ?? "?"} pair [${better}, ${worse}] names an unknown answer`);
    }
  }
  for (const constraint of row.constraints ?? []) {
    for (const id of constraint.ids ?? []) {
      if (!ids.has(id)) throw new Error(`${path}: fixture ${row.id ?? "?"} constraint names unknown answer ${id}`);
    }
  }
}

export function byIntent(records) {
  const map = new Map();
  for (const record of records) {
    if (!map.has(record.intent)) map.set(record.intent, []);
    map.get(record.intent).push(record);
  }
  return map;
}

export function mean(values) {
  if (values.length === 0) return null;
  return values.reduce((a, b) => a + b, 0) / values.length;
}

/** Population standard deviation - the node's score_stddev is a spread of observed scores. */
export function stddev(values) {
  if (values.length < 2) return null;
  const m = mean(values);
  return Math.sqrt(values.reduce((a, b) => a + (b - m) * (b - m), 0) / values.length);
}

/** Ranks with ties averaged, as Spearman requires. */
export function rankOf(values) {
  const order = values.map((v, i) => [v, i]).sort((a, b) => a[0] - b[0]);
  const ranks = new Array(values.length);
  let i = 0;
  while (i < order.length) {
    let j = i;
    while (j + 1 < order.length && order[j + 1][0] === order[i][0]) j += 1;
    const shared = (i + j) / 2 + 1;
    for (let k = i; k <= j; k += 1) ranks[order[k][1]] = shared;
    i = j + 1;
  }
  return ranks;
}

/** Spearman rank correlation; null when either series is constant or too short. */
export function spearman(xs, ys) {
  if (xs.length !== ys.length || xs.length < 3) return null;
  const rx = rankOf(xs);
  const ry = rankOf(ys);
  const mx = mean(rx);
  const my = mean(ry);
  let num = 0;
  let dx = 0;
  let dy = 0;
  for (let i = 0; i < rx.length; i += 1) {
    num += (rx[i] - mx) * (ry[i] - my);
    dx += (rx[i] - mx) ** 2;
    dy += (ry[i] - my) ** 2;
  }
  if (dx === 0 || dy === 0) return null;
  return num / Math.sqrt(dx * dy);
}

export function round(value, places = 6) {
  if (value === null || value === undefined || Number.isNaN(value)) return null;
  const factor = 10 ** places;
  return Math.round(value * factor) / factor;
}
