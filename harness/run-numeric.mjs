#!/usr/bin/env node
/**
 * Score a numeric-intent corpus (built by build-numeric-corpus.mjs) with any
 * set of WASM modules and report the two axes the node actually gates on.
 *
 *   node track2/harness/run-numeric.mjs track2/fixtures/numeric/STOCK_PRICE.json \
 *        champion=track2/harness/champions/stock_reg48.wasm \
 *        ours=track2/scorer/dist/generic.wasm
 *
 * Promotion needs BOTH `wins >= champion_wins` and `margin > champion_margin`,
 * where margin is mean(good) - mean(bad) over the comparable cases. Per-case
 * ordering is reported too, because one inverted case is what cost registration
 * 1377 on IP_GEOLOCATION (14/15 against the champion's 15/15).
 */

import { readFile } from "node:fs/promises";
import { loadScorer } from "./wasm-abi.mjs";

const [corpusPath, ...targets] = process.argv.slice(2);
if (!corpusPath || !targets.length) {
  console.error("usage: run-numeric.mjs <corpus.json> label=path.wasm [label=path.wasm ...]");
  process.exit(2);
}

const corpus = JSON.parse(await readFile(corpusPath, "utf8"));
const mean = (a) => a.reduce((s, x) => s + x, 0) / (a.length || 1);

const results = [];
for (const t of targets) {
  const [label, path] = t.split("=");
  let scorer;
  try {
    scorer = await loadScorer(path, label);
  } catch (e) {
    console.error(`${label}: ${e.message}`);
    continue;
  }
  const perCase = [];
  const goodAll = [];
  const badAll = [];
  let wins = 0;
  let pairs = 0;
  for (const c of corpus.cases) {
    const gs = c.good.map((a) => scorer.score(c.question, c.groundTruth, a.text));
    const bs = c.bad.map((a) => scorer.score(c.question, c.groundTruth, a.text));
    let caseWins = 0;
    for (const g of gs) for (const b of bs) { if (g > b) caseWins += 1; pairs += 1; }
    wins += caseWins;
    goodAll.push(...gs);
    badAll.push(...bs);
    // the node compares one good against one bad per case; use the means as the
    // per-case stand-in and record whether that ordering holds
    perCase.push({ id: c.id, good: mean(gs), bad: mean(bs), ok: mean(gs) > mean(bs), caseWins, of: gs.length * bs.length });
  }
  results.push({
    label,
    wins,
    pairs,
    caseWins: perCase.filter((p) => p.ok).length,
    cases: perCase.length,
    margin: mean(goodAll) - mean(badAll),
    mg: mean(goodAll),
    mb: mean(badAll),
    stddev: Math.sqrt(mean([...goodAll, ...badAll].map((x) => (x - mean([...goodAll, ...badAll])) ** 2))),
    perCase,
  });
}

console.log(`\n${corpus.intent} — ${corpus.cases.length} cases from recorded traffic (${corpus.provenance.rows} rows)\n`);
console.log("scorer".padEnd(12), "case wins".padEnd(11), "pair wins".padEnd(14), "margin".padEnd(11), "meanGood".padEnd(10), "meanBad".padEnd(10), "stddev");
for (const r of results) {
  console.log(
    r.label.padEnd(12),
    `${r.caseWins}/${r.cases}`.padEnd(11),
    `${r.wins}/${r.pairs}`.padEnd(14),
    r.margin.toFixed(6).padEnd(11),
    r.mg.toFixed(4).padEnd(10),
    r.mb.toFixed(4).padEnd(10),
    r.stddev.toFixed(4),
  );
}

console.log("\nper case (mean good / mean bad):");
console.log("case".padEnd(16), results.map((r) => r.label.padEnd(22)).join(""));
for (let i = 0; i < results[0].perCase.length; i += 1) {
  console.log(
    results[0].perCase[i].id.padEnd(16),
    results.map((r) => `${r.perCase[i].ok ? " " : "X"}${r.perCase[i].good.toFixed(3)}/${r.perCase[i].bad.toFixed(3)}`.padEnd(22)).join(""),
  );
}
console.log("\nStage-1 style checks (score_stddev must exceed 0.05):");
for (const r of results) console.log(`  ${r.label.padEnd(12)} stddev ${r.stddev.toFixed(4)} ${r.stddev > 0.05 ? "PASS" : "FAIL"}`);
