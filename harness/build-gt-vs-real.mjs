#!/usr/bin/env node
/**
 * Build good/bad pairs where the GOOD side is the ground truth's own assertion
 * and the BAD side is a real recorded miner answer that states a different
 * quantity.
 *
 *   node track2/harness/build-gt-vs-real.mjs --intent TVL_LOOKUP
 *
 * Why this class exists. `build-factswap.mjs` needs a recorded answer that
 * actually agrees with the ground truth, and for TVL_LOOKUP there is none:
 * the ground truth says $12.5 billion while the miners say $17.1B, $29.29B and
 * $14.4B, because they measure different chains and different sources. Every
 * recorded answer is wrong, so that builder correctly produced nothing.
 *
 * Here the ground truth supplies the correct side, which is legitimate because
 * the ground truth IS the definition of correct for this intent, and recorded
 * miner prose supplies the wrong side, which is not authored by us at all. The
 * bad answers are objectively wrong: each states a quantity that differs from
 * the ground truth's by more than WRONG_TOL.
 *
 * This is also the shape closest to what the node's own fixtures appear to be.
 * On STOCK_PRICE the champion scores 0.074 against recorded prose but 0.87 on
 * this shape, against the 0.6147 it earns on the real fixtures.
 *
 * The recorded live score is metadata only and never decides a label.
 */

import { readFile, writeFile, mkdir } from "node:fs/promises";
import { join } from "node:path";

const WRONG_TOL = 0.01; // 1% — unambiguously a different quantity

function parseArgs(argv) {
  const a = new Map();
  for (let i = 0; i < argv.length; i += 1) {
    const t = argv[i];
    if (!t.startsWith("--")) continue;
    const eq = t.indexOf("=");
    if (eq > 0) a.set(t.slice(0, eq), t.slice(eq + 1));
    else if (argv[i + 1] && !argv[i + 1].startsWith("--")) a.set(t, argv[++i]);
    else a.set(t, "true");
  }
  return a;
}

const SCALE = { trillion: 1e12, billion: 1e9, million: 1e6, thousand: 1e3 };
const NUM =
  /(?<![A-Za-z\d.])(\d{1,3}(?:,\d{3})*(?:\.\d+)?|\d+(?:\.\d+)?)(?!\d)\s*(trillion|billion|million|thousand)?/gi;

function quantities(text) {
  const out = [];
  NUM.lastIndex = 0;
  let m;
  while ((m = NUM.exec(text)) !== null) {
    const v = Number(m[1].replace(/,/g, "")) * (SCALE[(m[2] || "").toLowerCase()] ?? 1);
    if (!Number.isFinite(v) || v === 0) continue;
    if (Number.isInteger(v) && v >= 1900 && v <= 2100) continue; // calendar year
    out.push(v);
  }
  return out;
}

/** The quantity the ground truth asserts: bold markers first, then first figure. */
function headline(gt) {
  const bold = gt.match(/\*\*([^*]+)\*\*/g);
  if (bold) for (const b of bold) { const q = quantities(b); if (q.length) return q[0]; }
  const all = quantities(gt);
  return all.length ? all[0] : null;
}

/** The ground truth's own assertion, trimmed to its first sentence. */
function assertion(gt) {
  const first = gt.replace(/\*\*/g, "").split(/(?<=\.)\s/)[0].trim();
  return first.length > 20 ? first : gt.replace(/\*\*/g, "").slice(0, 400).trim();
}

const args = parseArgs(process.argv.slice(2));
const intent = args.get("--intent");
if (!intent) {
  console.error("usage: build-gt-vs-real.mjs --intent TVL_LOOKUP [--in FILE] [--out DIR]");
  process.exit(2);
}
const outDir = args.get("--out") || join(process.cwd(), "track2", "fixtures", "numeric");

let scores;
if (args.get("--in")) scores = JSON.parse(await readFile(args.get("--in"), "utf8")).scores;
else {
  const res = await fetch(`https://devnode.telegraphprotocol.com/scores?intent=${encodeURIComponent(intent)}&limit=300`);
  scores = (await res.json()).scores;
}

const byQuestion = new Map();
for (const r of scores) {
  if (!byQuestion.has(r.question)) byQuestion.set(r.question, []);
  byQuestion.get(r.question).push(r);
}

const cases = [];
let skipped = 0;
for (const [question, rows] of byQuestion) {
  const groundTruth = rows[0].ground_truth;
  const target = headline(groundTruth);
  if (target === null) { skipped += 1; continue; }

  const bad = [];
  const seen = new Set();
  for (const r of rows) {
    const text = r.converted_answer;
    if (!text || seen.has(text)) continue;
    seen.add(text);
    const qs = quantities(text);
    if (!qs.length) continue;
    // wrong only if EVERY quantity it states is far from the truth's
    const nearest = Math.min(...qs.map((v) => Math.abs(v - target) / Math.abs(target)));
    if (nearest >= WRONG_TOL) {
      bad.push({ text, miner: r.miner_slug, liveScore: Number(r.score), relError: Number(nearest.toFixed(6)) });
    }
  }
  if (!bad.length) continue;

  cases.push({
    id: `${intent.toLowerCase()}-gtreal-${cases.length + 1}`,
    question,
    groundTruth,
    target,
    good: [{ text: assertion(groundTruth), source: "ground-truth assertion" }],
    bad,
  });
}

await mkdir(outDir, { recursive: true });
const file = join(outDir, `${intent}-gtreal.json`);
await writeFile(
  file,
  JSON.stringify(
    {
      intent,
      class: "GT-VS-REAL",
      built: new Date().toISOString(),
      provenance: { source: "scores-api", rows: scores.length, questions: byQuestion.size, skipped_no_target: skipped },
      construction: {
        good: "the ground truth's own first-sentence assertion",
        bad: "verbatim recorded miner answers whose every stated quantity is >=1% from the ground truth's",
        note: "no wording authored by us on either side; the live score never decides a label",
      },
      cases,
    },
    null,
    1,
  ),
);
console.log(`${intent}: ${cases.length} cases, ${cases.reduce((s, c) => s + c.bad.length, 0)} pairs -> ${file}`);
for (const c of cases.slice(0, 2)) {
  console.log(`  ${c.id} target=${c.target}`);
  console.log(`     GOOD ${JSON.stringify(c.good[0].text.slice(0, 92))}`);
  console.log(`     BAD  live=${c.bad[0].liveScore.toFixed(4)} rel=${c.bad[0].relError} ${JSON.stringify(c.bad[0].text.slice(0, 78))}`);
}
