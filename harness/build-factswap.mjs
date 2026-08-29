#!/usr/bin/env node
/**
 * Build clean counterfactual pairs for a numeric intent.
 *
 *   node track2/harness/build-factswap.mjs --intent STOCK_PRICE
 *
 * Why this is legitimate here, when the equivalent construction was fatal for
 * TEXT_AUTHENTICITY_CHECK: for a numeric intent, correctness IS the numeric
 * value, so replacing the headline quantity and leaving every other word intact
 * produces an answer that is wrong for exactly one objective reason. Nothing
 * about style, phrasing or length is being asserted. For TAC the same trick
 * encoded *our opinion* about which verdict wording was right, which is why the
 * corpus ended up anti-correlated with the node's (GAPS G13).
 *
 * The base text is always a REAL recorded miner answer, never written by us, so
 * the surface form stays in-distribution. Only the number moves.
 *
 * The recorded live score is carried through as metadata; it plays no part in
 * constructing or labelling anything.
 */

import { readFile, writeFile, mkdir } from "node:fs/promises";
import { join } from "node:path";

const SWAP_FACTORS = [0.82, 0.91, 1.09, 1.23]; // unambiguously different, still plausible

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
// (?<![A-Za-z\d.]) keeps version tokens ("Aave V3"), tickers and mid-number
// fragments out: only a number that starts a token can be the headline quantity.
const NUM = /(?<![A-Za-z\d.])(\d{1,3}(?:,\d{3})*(?:\.\d+)?|\d+(?:\.\d+)?)(?!\d)(\s*)(trillion|billion|million|thousand)?/gi;

function headlineTarget(groundTruth) {
  const bold = groundTruth.match(/\*\*([^*]+)\*\*/);
  const source = bold ? bold[1] : groundTruth;
  NUM.lastIndex = 0;
  let m;
  while ((m = NUM.exec(source)) !== null) {
    const v = Number(m[1].replace(/,/g, "")) * (SCALE[(m[3] || "").toLowerCase()] ?? 1);
    if (!Number.isFinite(v)) continue;
    if (Number.isInteger(v) && v >= 1900 && v <= 2100) continue; // calendar year
    return v;
  }
  return null;
}

/** Format a swapped value the way the original token was written. */
function formatLike(original, value) {
  const hasComma = original.includes(",");
  const dp = (original.split(".")[1] || "").length;
  let s = value.toFixed(dp);
  if (hasComma) s = Number(s).toLocaleString("en-US", { minimumFractionDigits: dp, maximumFractionDigits: dp });
  return s;
}

/**
 * Replace every occurrence of the answer's own headline quantity with a scaled
 * one. Only tokens whose value is within 0.5% of the target move, so dates,
 * percentages and volumes are left alone.
 */
function swap(text, target, factor) {
  let touched = 0;
  const out = text.replace(NUM, (whole, digits, gap, word) => {
    const scale = SCALE[(word || "").toLowerCase()] ?? 1;
    const v = Number(digits.replace(/,/g, "")) * scale;
    if (!Number.isFinite(v) || v === 0) return whole;
    if (Math.abs(v - target) / Math.abs(target) > 0.005) return whole;
    touched += 1;
    return `${formatLike(digits, (v * factor) / scale)}${gap}${word || ""}`;
  });
  return touched ? out : null;
}

const args = parseArgs(process.argv.slice(2));
const intent = args.get("--intent");
if (!intent) {
  console.error("usage: build-factswap.mjs --intent STOCK_PRICE [--in FILE] [--out DIR]");
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
for (const [question, rows] of byQuestion) {
  const groundTruth = rows[0].ground_truth;
  const target = headlineTarget(groundTruth);
  if (target === null) continue;

  // the recorded answer that states the ground truth's own quantity most nearly
  let base = null;
  for (const r of rows) {
    // The node converts a miner payload to prose before scoring, so the raw
    // payload is not what the module sees. Rows without a conversion are
    // skipped rather than substituted: TVL_LOOKUP registrations 1587 and 1681
    // both errored with "miner_answer too large" on a 10 MB raw blob.
    const text = r.converted_answer;
    if (!text) continue;
    NUM.lastIndex = 0;
    let bestRel = Infinity;
    let m;
    while ((m = NUM.exec(text)) !== null) {
      const v = Number(m[1].replace(/,/g, "")) * (SCALE[(m[3] || "").toLowerCase()] ?? 1);
      if (!Number.isFinite(v) || v === 0) continue;
      bestRel = Math.min(bestRel, Math.abs(v - target) / Math.abs(target));
    }
    if (bestRel <= 0.005 && (!base || bestRel < base.rel)) base = { text, rel: bestRel, miner: r.miner_slug, live: Number(r.score) };
  }
  if (!base) continue;

  const bad = [];
  for (const f of SWAP_FACTORS) {
    const t = swap(base.text, target, f);
    if (t && t !== base.text) bad.push({ text: t, factor: f, derivedFrom: base.miner });
  }
  if (!bad.length) continue;

  cases.push({
    id: `${intent.toLowerCase()}-swap-${cases.length + 1}`,
    question,
    groundTruth,
    target,
    good: [{ text: base.text, miner: base.miner, liveScore: base.live, relError: Number(base.rel.toFixed(6)) }],
    bad,
  });
}

await mkdir(outDir, { recursive: true });
const file = join(outDir, `${intent}-factswap.json`);
await writeFile(
  file,
  JSON.stringify(
    {
      intent,
      class: "FACT-SWAP",
      built: new Date().toISOString(),
      provenance: { source: "scores-api", rows: scores.length, questions: byQuestion.size },
      construction: {
        good: "verbatim recorded miner answer whose quantity matches the ground truth within 0.5%",
        bad: "the same answer with only the headline quantity rescaled",
        factors: SWAP_FACTORS,
        note: "one objective difference per pair; no wording authored by us",
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
  console.log(`     GOOD ${JSON.stringify(c.good[0].text.slice(0, 96))}`);
  console.log(`     BAD  ${JSON.stringify(c.bad[0].text.slice(0, 96))}`);
}
