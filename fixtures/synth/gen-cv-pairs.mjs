#!/usr/bin/env node
/**
 * CONTENT_VERIFICATION clean-pair generator.
 *
 * The node's fixture gate tests clean good-vs-bad ordering, not adversarial
 * pathologies (established by the rejection of registration 1377: our corpus
 * measured the incumbent at 0.438 where the node measured 0.992). So the wrong
 * answers here are FLUENT one-fact counterfactuals -- a flipped verdict, a
 * different similarity figure, a different matched source -- never mangled
 * strings, which would be trivially separable and would overstate our margin.
 *
 * Slots counterfactualled, one at a time:
 *   verdict      plagiarised <-> original, AI-generated <-> human-written
 *   similarity   the percentage overlap
 *   source       the matched publication / URL
 *   matches      number of matching passages
 */
import { createHash } from "node:crypto";
import { writeFileSync } from "node:fs";

const TRUTHS = [
  { doc: "submitted essay 'Coastal Erosion in Kerala'", verdict: "plagiarised", vAlt: "original",
    sim: 68, src: "Journal of Coastal Research, 2019", matches: 7, conf: 0.94 },
  { doc: "manuscript 'Neural Substrates of Memory'", verdict: "original", vAlt: "plagiarised",
    sim: 4, src: "no significant source", matches: 0, conf: 0.91 },
  { doc: "blog post 'Ten Rules for Remote Teams'", verdict: "AI-generated", vAlt: "human-written",
    sim: 22, src: "GPT-style language model", matches: 3, conf: 0.88 },
  { doc: "student report 'Monetary Policy 2021'", verdict: "plagiarised", vAlt: "original",
    sim: 41, src: "Reserve Bank of India Annual Report 2021", matches: 5, conf: 0.9 },
  { doc: "article 'Solar Adoption in Rajasthan'", verdict: "human-written", vAlt: "AI-generated",
    sim: 9, src: "no significant source", matches: 1, conf: 0.86 },
  { doc: "thesis chapter 'Graph Neural Networks'", verdict: "plagiarised", vAlt: "original",
    sim: 55, src: "arXiv:1810.00826", matches: 6, conf: 0.93 },
  { doc: "press release 'Q3 Earnings Summary'", verdict: "original", vAlt: "plagiarised",
    sim: 12, src: "no significant source", matches: 1, conf: 0.89 },
  { doc: "review 'A Survey of Federated Learning'", verdict: "plagiarised", vAlt: "original",
    sim: 73, src: "IEEE Communications Surveys, 2020", matches: 9, conf: 0.95 },
  { doc: "op-ed 'Water Rights in the Cauvery Basin'", verdict: "human-written", vAlt: "AI-generated",
    sim: 6, src: "no significant source", matches: 0, conf: 0.87 },
  { doc: "lab writeup 'Enzyme Kinetics of Amylase'", verdict: "plagiarised", vAlt: "original",
    sim: 34, src: "Biochemistry Lab Manual, 3rd ed.", matches: 4, conf: 0.9 },
  { doc: "whitepaper 'Zero-Knowledge Rollups'", verdict: "AI-generated", vAlt: "human-written",
    sim: 28, src: "large language model output", matches: 2, conf: 0.85 },
  { doc: "case note 'Kesavananda Bharati Revisited'", verdict: "original", vAlt: "plagiarised",
    sim: 8, src: "no significant source", matches: 1, conf: 0.92 },
];

const WRONG_SRC = [
  "Nature Climate Change, 2016", "Wikipedia article 'Erosion'",
  "Proceedings of NeurIPS 2017", "World Bank Development Report 1998",
];

const gt = (t) =>
  `The ${t.doc} is assessed as ${t.verdict}. Similarity to the reference corpus is ${t.sim}%, ` +
  `derived from ${t.matches} matching passage${t.matches === 1 ? "" : "s"} against ${t.src}. ` +
  `Confidence in this assessment is ${t.conf}.`;

const fixtures = TRUTHS.map((t, i) => {
  const n = String(i + 1).padStart(2, "0");
  const truth = gt(t);
  const wrongSrc = WRONG_SRC[i % WRONG_SRC.length];
  const answers = [
    { id: "correct-verbatim", text: truth, quality: 1, note: "the ground truth itself" },
    { id: "correct-reworded", text:
      `The data shows the ${t.doc} to be ${t.verdict}, with ${t.sim}% similarity to the reference ` +
      `corpus across ${t.matches} matching passage${t.matches === 1 ? "" : "s"} traced to ${t.src}, ` +
      `at a confidence of ${t.conf}.`, quality: 1, note: "same facts, different wording" },
    { id: "correct-terse", text:
      `${t.verdict}; ${t.sim}% similar; ${t.matches} match${t.matches === 1 ? "" : "es"}; ${t.src}.`,
      quality: 1, note: "same facts, minimal prose" },
    { id: "wrong-verdict", text:
      `The data shows the ${t.doc} to be ${t.vAlt}, with ${t.sim}% similarity to the reference ` +
      `corpus across ${t.matches} matching passage${t.matches === 1 ? "" : "s"} traced to ${t.src}, ` +
      `at a confidence of ${t.conf}.`, quality: 0, note: "verdict flipped, everything else right" },
    { id: "wrong-similarity", text:
      `The data shows the ${t.doc} to be ${t.verdict}, with ${t.sim < 50 ? t.sim + 47 : t.sim - 31}% ` +
      `similarity to the reference corpus across ${t.matches} matching passage` +
      `${t.matches === 1 ? "" : "s"} traced to ${t.src}, at a confidence of ${t.conf}.`,
      quality: 0, note: "similarity figure wrong, everything else right" },
    { id: "wrong-source", text:
      `The data shows the ${t.doc} to be ${t.verdict}, with ${t.sim}% similarity to the reference ` +
      `corpus across ${t.matches} matching passage${t.matches === 1 ? "" : "s"} traced to ${wrongSrc}, ` +
      `at a confidence of ${t.conf}.`, quality: 0, note: "matched source wrong, everything else right" },
    { id: "wrong-unrelated", text:
      `The data shows the current gas price on Ethereum mainnet is 14 gwei, with a base fee of ` +
      `12 gwei and a priority tip of 2 gwei.`, quality: 0, note: "off-topic control" },
  ];
  const correct = answers.filter(a => a.quality === 1).map(a => a.id);
  const wrong = answers.filter(a => a.quality === 0).map(a => a.id);
  const pairs = [];
  for (const c of correct) for (const w of wrong) pairs.push([c, w]);
  return {
    id: `content_verification-cleanpair-${n}`,
    intent: "CONTENT_VERIFICATION",
    class: "CLEAN-PAIR",
    rationale:
      "Every wrong answer differs from a correct one in exactly one decisive slot (verdict, " +
      "similarity, or matched source) and is otherwise fluent and identically worded, so ordering " +
      "it requires reading the fact rather than noticing broken prose.",
    question: `Verify the originality of the ${t.doc}. Report the verdict, the similarity percentage, the number of matching passages, and the matched source.`,
    ground_truth: truth,
    answers,
    pairs,
    constraints: [{ kind: "near-equal", ids: correct, tolerance: 0.05,
      note: "all correct phrasings must score within 0.05 of each other" }],
    provenance: { source: "synthetic", generator: "gen-cv-pairs.mjs", created: "2026-08-28" },
  };
});

const body = { generator: "gen-cv-pairs.mjs", generated_from: "TRUTHS table, 12 documents", fixtures };
body.corpus_version = createHash("sha256")
  .update(JSON.stringify(fixtures)).digest("hex").slice(0, 16);
writeFileSync("CONTENT_VERIFICATION_CLEAN_PAIR.json", JSON.stringify(body, null, 1));
const np = fixtures.reduce((a, f) => a + f.pairs.length, 0);
console.log(`wrote ${fixtures.length} fixtures, ${np} pairs, corpus_version ${body.corpus_version}`);
