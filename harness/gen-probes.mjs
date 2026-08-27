#!/usr/bin/env node

/**
 * REAL-PARROT probes: the measured champion pathology, on real inputs.
 *
 *   node track2/harness/gen-probes.mjs [--real DIR] [--out DIR] [--max 8]
 *
 * The synthetic corpus cannot reproduce the headline hole on its own, because a
 * generated ground truth is a short paraphrase of the generated answer, so a
 * data-carrying answer already overlaps it heavily. The measured pathology only
 * appears against REAL ground truths (100-350 words of hedged LLM prose that
 * shares almost no wording with a miner's data payload).
 *
 * So these fixtures pin a real question + real ground truth verbatim, and put
 * three answers against them:
 *   prefix-parrot     the question's own opening ~17 words, ZERO data
 *   real-<slug>-eNN   what a miner actually answered, verbatim (data, no echo)
 *   parrot-plus-data  the parrot concatenated with that same real answer
 *
 * Every generated answer is built from the QUESTION (and, for the third, a real
 * answer). None of them is written against the ground truth, which is what
 * FIXTURES.md's honesty rule forbids.
 *
 * The pair asserted is [real data answer > prefix-parrot]: an answer carrying no
 * data at all must not outrank one carrying data. That is a claim about the
 * scorer, not about whether the miner's numbers were right.
 */

import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import { join, extname } from "node:path";
import { prefixParrot, oneLine } from "./gen-synth.mjs";

function parseArgs(argv) {
  const args = new Map();
  for (const token of argv) {
    const [k, v] = token.includes("=") ? [token.slice(0, token.indexOf("=")), token.slice(token.indexOf("=") + 1)] : [token, "true"];
    args.set(k, v);
  }
  return args;
}

function buildProbe(source, index) {
  const data = source.answers.filter((a) => !a.meta?.looks_like_refusal && a.text.trim().length > 40);
  if (data.length === 0) return null;
  // Keep the best- and worst-scoring recorded answers, not just one: the pair is
  // asserted against the BEST real answer (the strongest form of the claim), and
  // the worst is carried so the report shows the real spread rather than a
  // flattering slice of it.
  const live = (a) => Number(a.meta?.live_score ?? 0);
  const sorted = data.slice().sort((a, b) => live(b) - live(a));
  const top = sorted[0];
  const keep = sorted.length > 1 ? [top, sorted[sorted.length - 1]] : [top];
  const parrot = prefixParrot(source.question);
  return {
    id: `${source.intent.toLowerCase()}-probe-${String(index + 1).padStart(2, "0")}`,
    intent: source.intent,
    class: "REAL-PARROT",
    rationale:
      "the measured champion hole on real inputs: a contentless restatement of the question's opening words " +
      "against a real miner answer that carries data but does not echo the question",
    question: source.question,
    ground_truth: source.ground_truth,
    answers: [
      ...keep.map((a, i) => ({
        id: a.id,
        text: a.text,
        quality: null,
        note: `recorded miner answer, verbatim (${i === 0 ? "highest" : "lowest"} live score of this fixture's data answers); carries data, does not open by restating the question`,
        meta: a.meta,
      })),
      {
        id: "prefix-parrot",
        text: parrot,
        quality: 0.0,
        note: "the question's opening ~17 words restated; contains no data whatsoever",
      },
      {
        id: "parrot-plus-data",
        text: oneLine(`${parrot} ${top.text}`),
        quality: null,
        note: "the same parrot with the real answer appended; isolates what the echo alone is worth",
      },
    ],
    pairs: [[top.id, "prefix-parrot"]],
    constraints: [],
    provenance: {
      source: "scores-api + mechanical question echo",
      derived_from: source.id,
      url: source.provenance?.url ?? null,
      epoch_min: source.provenance?.epoch_min ?? null,
      epoch_max: source.provenance?.epoch_max ?? null,
      captured: source.provenance?.captured ?? null,
      ground_truth_looks_like_refusal: source.provenance?.ground_truth_looks_like_refusal ?? null,
      generator: "track2/harness/gen-probes.mjs",
    },
  };
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  const realDir = args.get("--real") ?? "track2/fixtures/real";
  const outDir = args.get("--out") ?? "track2/fixtures/probe";
  const max = Number(args.get("--max") ?? 8);

  await mkdir(outDir, { recursive: true });
  const names = (await readdir(realDir)).filter((n) => extname(n) === ".json").sort();
  console.log(`probes from ${realDir} -> ${outDir} (max ${max} per intent)`);
  for (const name of names) {
    const document = JSON.parse(await readFile(join(realDir, name), "utf8"));
    const fixtures = [];
    for (const source of document.fixtures) {
      if (fixtures.length >= max) break;
      const probe = buildProbe(source, fixtures.length);
      if (probe) fixtures.push(probe);
    }
    const out = {
      intent: document.intent,
      class: "REAL-PARROT",
      provenance: { source: "derived from track2/fixtures/real", generator: "track2/harness/gen-probes.mjs", real_capture: document.provenance?.captured ?? null },
      fixtures,
    };
    await writeFile(join(outDir, name), `${JSON.stringify(out, null, 2)}\n`);
    console.log(`${document.intent.padEnd(18)} probes ${String(fixtures.length).padStart(3)}`);
  }
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
