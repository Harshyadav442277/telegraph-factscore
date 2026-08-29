#!/usr/bin/env node
/**
 * Build a good/bad pair corpus for the numeric intents out of RECORDED TRAFFIC.
 *
 *   node track2/harness/build-numeric-corpus.mjs --intent STOCK_PRICE [--limit 300]
 *
 * Why this exists: for TEXT_AUTHENTICITY_CHECK we had no traffic, so every
 * fixture was written by us, and the corpus turned out anti-correlated with the
 * node's (GAPS G13 — the cause of registrations 1671 and 1673). For the numeric
 * intents there IS traffic, so both sides of every pair are text a real miner
 * actually emitted and the node actually scored.
 *
 * Labelling is OBJECTIVE and never looks at the live score:
 *   - pull the headline quantity out of the ground truth (bold markers first,
 *     then the first currency/number token)
 *   - pull the same-shaped quantity out of each recorded answer
 *   - GOOD  = agrees with the ground truth within RELATIVE_TOL
 *   - BAD   = states a quantity that disagrees beyond WRONG_TOL
 *   - answers between the two tolerances are DISCARDED, not forced into a class
 *
 * The recorded score is copied into the output as metadata only, so the
 * incumbent's opinion can be reported without ever having defined the label.
 * Deriving labels from the champion's score would only teach us to imitate it.
 */

import { readFile, writeFile, mkdir } from "node:fs/promises";
import { join } from "node:path";

const RELATIVE_TOL = 0.002; // 0.2% — "the same number, differently rounded"
const WRONG_TOL = 0.01; // 1% — unambiguously a different number

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

const SCALE = { trillion: 1e12, billion: 1e9, million: 1e6, thousand: 1e3, t: 1e12, b: 1e9, m: 1e6, k: 1e3 };

/** Every number in `text`, scaled by any magnitude word or suffix following it. */
function quantities(text) {
  const out = [];
  // (?<![A-Za-z\d.]) keeps version tokens ("Aave V3") and mid-number fragments out
  const re = /(\$?\s*)(?<![A-Za-z\d.])(\d{1,3}(?:,\d{3})*(?:\.\d+)?|\d+(?:\.\d+)?)(?!\d)\s*(trillion|billion|million|thousand|[TBMK]\b)?/gi;
  let m;
  while ((m = re.exec(text)) !== null) {
    const raw = Number(m[2].replace(/,/g, ""));
    if (!Number.isFinite(raw)) continue;
    const word = (m[3] || "").toLowerCase();
    const scale = SCALE[word] ?? 1;
    out.push({ value: raw * scale, hadCurrency: Boolean(m[1].trim()), scaled: Boolean(word), index: m.index });
  }
  return out;
}

/**
 * The quantity the ground truth is actually asserting. Telegraph ground truths
 * put it in **bold** markers; fall back to the first currency-marked number,
 * then to the first number that is not a bare calendar year.
 */
function headline(groundTruth) {
  const bold = groundTruth.match(/\*\*([^*]+)\*\*/g);
  if (bold) {
    for (const b of bold) {
      const q = quantities(b);
      if (q.length) return q[0].value;
    }
  }
  const all = quantities(groundTruth);
  const cur = all.find((q) => q.hadCurrency || q.scaled);
  if (cur) return cur.value;
  const plain = all.find((q) => !(q.value >= 1900 && q.value <= 2100 && Number.isInteger(q.value)));
  return plain ? plain.value : null;
}

/** Closest relative distance between any quantity in `text` and `target`. */
function closest(text, target) {
  let best = null;
  for (const q of quantities(text)) {
    if (q.value === 0 && target !== 0) continue;
    const rel = Math.abs(q.value - target) / Math.abs(target || 1);
    if (best === null || rel < best) best = rel;
  }
  return best;
}

const args = parseArgs(process.argv.slice(2));
const intent = args.get("--intent");
if (!intent) {
  console.error("usage: build-numeric-corpus.mjs --intent STOCK_PRICE [--limit 300] [--in FILE] [--out DIR]");
  process.exit(2);
}
const limit = Number(args.get("--limit") || 300);
const outDir = args.get("--out") || join(process.cwd(), "track2", "fixtures", "numeric");

let scores;
if (args.get("--in")) {
  scores = JSON.parse(await readFile(args.get("--in"), "utf8")).scores;
} else {
  const url = `https://devnode.telegraphprotocol.com/scores?intent=${encodeURIComponent(intent)}&limit=${limit}`;
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${url} -> ${res.status}`);
  scores = (await res.json()).scores;
}

const byQuestion = new Map();
for (const r of scores) {
  const key = r.question;
  if (!byQuestion.has(key)) byQuestion.set(key, []);
  byQuestion.get(key).push(r);
}

const cases = [];
let discarded = 0;
for (const [question, rows] of byQuestion) {
  const groundTruth = rows[0].ground_truth;
  const target = headline(groundTruth);
  if (target === null) continue;

  const good = [];
  const bad = [];
  const seen = new Set();
  for (const r of rows) {
    // The node converts a miner payload to prose before scoring, so the raw
    // payload is not what the module sees. Rows without a conversion are
    // skipped rather than substituted: TVL_LOOKUP registrations 1587 and 1681
    // both errored with "miner_answer too large" on a 10 MB raw blob.
    const text = r.converted_answer;
    if (!text || seen.has(text)) continue;
    seen.add(text);
    const rel = closest(text, target);
    if (rel === null) continue;
    const entry = { text, miner: r.miner_slug, liveScore: Number(r.score), relError: Number(rel.toFixed(6)) };
    if (rel <= RELATIVE_TOL) good.push(entry);
    else if (rel >= WRONG_TOL) bad.push(entry);
    else discarded += 1;
  }
  if (!good.length || !bad.length) continue;
  cases.push({ id: `${intent.toLowerCase()}-${cases.length + 1}`, question, groundTruth, target, good, bad });
}

await mkdir(outDir, { recursive: true });
const payload = {
  intent,
  class: "REAL-NUMERIC",
  built: new Date().toISOString(),
  provenance: {
    source: "scores-api",
    url: `https://devnode.telegraphprotocol.com/scores?intent=${intent}&limit=${limit}`,
    rows: scores.length,
    questions: byQuestion.size,
    scored_text_field: "converted_answer",
  },
  labelling: {
    rule: "headline quantity of the ground truth vs the closest quantity in the answer",
    good_within_relative: RELATIVE_TOL,
    bad_beyond_relative: WRONG_TOL,
    ambiguous_discarded: discarded,
    note: "live score is metadata; it never participates in labelling",
  },
  cases,
};
const file = join(outDir, `${intent}.json`);
await writeFile(file, JSON.stringify(payload, null, 1));

const pairs = cases.reduce((s, c) => s + c.good.length * c.bad.length, 0);
console.log(`${intent}: ${cases.length} cases, ${pairs} pairs, ${discarded} ambiguous answers discarded`);
console.log(`  -> ${file}`);
for (const c of cases.slice(0, 3)) {
  console.log(`  ${c.id} target=${c.target}`);
  console.log(`     GOOD ${c.good[0].liveScore.toFixed(4)} rel=${c.good[0].relError} ${JSON.stringify(c.good[0].text.slice(0, 78))}`);
  console.log(`     BAD  ${c.bad[0].liveScore.toFixed(4)} rel=${c.bad[0].relError} ${JSON.stringify(c.bad[0].text.slice(0, 78))}`);
}
