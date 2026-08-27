#!/usr/bin/env node

/**
 * Generate the synthetic fixtures (FIXTURES.md classes 2-10).
 *
 *   node track2/harness/gen-synth.mjs [--intent ALL|NAME] [--seed 20260827] [--out DIR]
 *
 * Every answer is rendered by a generic renderer from a fact record produced by
 * synth-schemas.mjs. Nothing here reads a ground truth and writes an answer to
 * match it -- the wrong answers are the SAME renderer applied to a fact record
 * with one decisive field mutated. That is the honesty rule in FIXTURES.md:
 * hand-written candidates leak the ground truth and inflate scores.
 *
 * Deterministic: same --seed, byte-identical output.
 */

import { mkdir, writeFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { SCHEMAS, pick } from "./synth-schemas.mjs";

const TOL_FORMAT = 0.10;
const TOL_UNIT = 0.10;
const TOL_LENGTH = 0.15;

function mulberry32(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const seedFor = (base, intent) => {
  let h = base >>> 0;
  for (const ch of intent) h = (Math.imul(h, 31) + ch.charCodeAt(0)) >>> 0;
  return h;
};

const cap = (s) => (s.length ? s[0].toUpperCase() + s.slice(1) : s);

/* ---- generic renderers: the only place answer text is produced ----
 *
 * Register matters. The text the node scores is `converted_answer`, and the
 * measured live distribution (2026-08-27 gate recon) is: flat single-paragraph
 * third-person summary, no markdown, no newlines, 86.9% literally opening with
 * "The data". Ground truths are 2.25x longer and often markdown/first-person.
 * Writing synthetic answers in the GROUND TRUTH's register would measure a
 * distribution the node never sees, so every prose renderer below stays in the
 * converted register and `oneLine` strips newlines from all of them.
 *
 * Three renderers deliberately break register because their shape IS the
 * fixture: `json` and `competitor` (FORMAT-EQUIVALENCE, OUR-STYLE-WRONG) and
 * `prefixParrot` (which must open with the question's own words to probe the
 * measured opening-token cliff). */

export const oneLine = (s) => s.replace(/\s*\n+\s*/g, " ").replace(/[ \t]{2,}/g, " ").trim();

const prose = ({ lead, parts }) => oneLine(`The data covers ${lead}. ${parts.map(([, p]) => `${cap(p)}.`).join(" ")}`);
const terse = ({ lead, parts }) => oneLine(`The data shows ${lead}: ${parts.map(([, p]) => p).join("; ")}.`);
const noEcho = ({ parts }) => oneLine(`The data reports the following. ${parts.slice().reverse().map(([, p]) => `${cap(p)}.`).join(" ")}`);
const json = (facts) => JSON.stringify(facts);

const verbose = (surface) =>
  oneLine(
    `${prose(surface)} The values above were taken directly from the upstream provider and are reported without ` +
      `rounding or interpolation. Each field named in the request is answered exactly once, in the order it was ` +
      `asked. No other fields were inferred, and nothing was substituted where a value was unavailable.`,
  );

const livecert = ({ lead, parts }) =>
  oneLine(`verdict: ${parts[0][1]} | confidence: 0.94 | subject: ${lead} | reason: ${parts.slice(1).map(([, p]) => p).join("; ")} | checked_at: recorded`);

const competitor = (facts) =>
  oneLine(`${Object.entries(facts).map(([k, v]) => `${k}=${Array.isArray(v) ? v.join("|") : v}`).join(" ")} grade=A`);

/** First ~17 words of the question restated as a statement, carrying no data. */
export const prefixParrot = (question) => {
  const stripped = question.replace(/^(can you |could you |please |what is |what are |is there |look up |verify |geolocate )/i, "");
  const head = stripped.split(/\s+/).slice(0, 17).join(" ").replace(/[,;:]$/, "");
  return oneLine(
    `${cap(head)} — that is exactly what is addressed here, in the same terms, at the same scope, ` +
      `covering each dimension the request named, in the order the request named them.`,
  );
};

const refusal = (question, keywords, echo) =>
  oneLine(
    `${echo ? `${cap(question.split(/\s+/).slice(0, 14).join(" "))} — ` : ""}The data does not include the exact ` +
      `${keywords.slice(0, 3).join(", ")} requested. The available sources do not contain a complete breakdown for the ` +
      `period and fields named, and the specific values needed to fulfil the request are unavailable. For precise ` +
      `figures a dedicated provider should be consulted closer to the time in question.`,
  );

/** Verbatim from live traffic: the platform's own content-filter placeholder. */
const CONTENT_FILTER = "- The generated text has been blocked by our content filters.";

const stuffing = (question, keywords) => {
  const words = [...new Set(question.toLowerCase().replace(/[^a-z0-9 ]/g, " ").split(/\s+/).filter((w) => w.length > 3))];
  return oneLine(
    `The data covers ${keywords.join(", ")}. ${words.join(" ")}. ${keywords.join(" ")}. Any of the possible outcomes is ` +
      `covered here: the value may be high, low, or anywhere in between, and either condition may or may not hold.`,
  );
};

const contradiction = (surface, wrongSurface, partKey) => {
  const wrong = new Map(wrongSurface.parts).get(partKey);
  const parts = surface.parts.map(([k, p]) => (k === partKey ? [k, `${p}, although other sources indicate ${wrong}`] : [k, p]));
  return prose({ lead: surface.lead, parts });
};

/* ---- class builders: one fact record per fixture ---- */

const answer = (id, text, quality, note) => ({ id, text, quality, note });

function makeRecord(schema, rng, index, klass, rationale, build) {
  const facts = schema.make(rng);
  const record = {
    id: `${schema.intent.toLowerCase()}-synth-${String(index).padStart(2, "0")}`,
    intent: schema.intent,
    class: klass,
    rationale,
    question: schema.question(facts),
    ground_truth: schema.groundTruth(facts),
    answers: [],
    pairs: [],
    constraints: [],
    provenance: { source: "synthetic", generator: "track2/harness/gen-synth.mjs", schema: schema.intent, fact_seed: index },
  };
  build(record, facts, schema.surface(facts));
  return record;
}

function buildIntent(intent, seed) {
  const schema = SCHEMAS[intent];
  const rng = mulberry32(seedFor(seed, intent));
  const records = [];
  let n = 1;
  const add = (klass, rationale, build) => records.push(makeRecord(schema, rng, n++, klass, rationale, build));

  for (const mutation of schema.mutations) {
    add(
      "FACT-SWAP",
      `one decisive fact (${mutation.name}, ${mutation.kind}) changed and nothing else; lexical overlap barely ` +
        `moves, so a vocabulary-matching baseline keeps the wrong answer at the same score`,
      (record, facts, surface) => {
        const wrong = schema.surface(mutation.apply(facts, rng));
        record.answers.push(
          answer("correct-prose", prose(surface), 1.0, "all decisive facts right"),
          answer(`fact-swap-${mutation.name}`, prose(wrong), 0.0, `${mutation.name} (${mutation.kind}) wrong, everything else identical`),
        );
        record.pairs.push(["correct-prose", `fact-swap-${mutation.name}`]);
      },
    );
  }

  for (const echo of [false, true]) {
    add(
      "REFUSAL",
      echo
        ? "a refusal that opens by restating the question's own wording; the archetype of the recorded epoch where a refusal scored 0.99 and a correct forecast 0.007"
        : "a plain refusal in ground-truth-adjacent boilerplate; must score near zero whenever the ground truth contains an answer",
      (record, facts, surface) => {
        const id = echo ? "refusal-echo" : "refusal-plain";
        record.answers.push(
          answer("correct-prose", prose(surface), 1.0, "all decisive facts right"),
          answer(id, refusal(record.question, schema.keywords, echo), 0.0, "no decisive facts; refusal boilerplate only"),
        );
        record.pairs.push(["correct-prose", id]);
      },
    );
  }

  add(
    "STUFFING",
    "question vocabulary plus intent keywords with no decisive facts and a hedge covering every outcome; must lose to any factually correct answer",
    (record, facts, surface) => {
      record.answers.push(
        answer("correct-terse", terse(surface), 1.0, "all decisive facts right, one sentence"),
        answer("stuffing-keywords", stuffing(record.question, schema.keywords), 0.0, "keyword salad, no decisive facts, hedged both ways"),
      );
      record.pairs.push(["correct-terse", "stuffing-keywords"]);
    },
  );

  add(
    "STUFFING",
    "PREFIX-PARROT: the question's own opening ~17 words restated, carrying zero data — the measured champion hole (contentless echo 0.993 vs data-carrying non-echo 0.004 on WEATHER_FORECAST)",
    (record, facts, surface) => {
      record.answers.push(
        answer("correct-prose", prose(surface), 1.0, "echoes the question AND carries the data"),
        answer("correct-nonecho", noEcho(surface), 1.0, "carries every decisive fact but never restates the question"),
        answer("prefix-parrot", prefixParrot(record.question), 0.0, "restates the question's opening words; contains no data at all"),
      );
      record.pairs.push(["correct-prose", "prefix-parrot"], ["correct-nonecho", "prefix-parrot"]);
      record.constraints.push({
        type: "near_equal", ids: ["correct-prose", "correct-nonecho"], tolerance: TOL_LENGTH,
        note: "both carry every decisive fact; only the framing differs, so scores must not diverge by framing alone",
      });
    },
  );

  add(
    "EMPTY-ANSWER",
    "an empty scored text — ~47% of live converted_answers are empty; must score exactly 0.0, and whitespace-only must too (a 0.0007 was a recorded Stage-1 rejection)",
    (record, facts, surface) => {
      record.answers.push(
        answer("correct-prose", prose(surface), 1.0, "all decisive facts right"),
        answer("empty", "", 0.0, "empty converted_answer, the single commonest live row"),
        answer("whitespace-only", "   \t  ", 0.0, "whitespace-only converted_answer"),
      );
      record.pairs.push(["correct-prose", "empty"], ["correct-prose", "whitespace-only"]);
    },
  );

  add(
    "CONTENT-FILTER",
    "the platform's own content-filter placeholder, verbatim from live traffic; it carries no answer and must score ~0",
    (record, facts, surface) => {
      record.answers.push(
        answer("correct-prose", prose(surface), 1.0, "all decisive facts right"),
        answer("content-filter-block", CONTENT_FILTER, 0.0, "verbatim platform placeholder, no decisive facts"),
      );
      record.pairs.push(["correct-prose", "content-filter-block"]);
    },
  );

  add(
    "CONTRADICTION",
    "asserts the right and a wrong value for the same fact; hedging both sides is not knowledge and must not beat a clean correct answer",
    (record, facts, surface) => {
      const mutation = schema.mutations.find((m) => m.kind === "numeric") ?? schema.mutations[0];
      const wrongSurface = schema.surface(mutation.apply(facts, rng));
      record.answers.push(
        answer("correct-prose", prose(surface), 1.0, "all decisive facts right"),
        answer("contradiction-both", contradiction(surface, wrongSurface, mutation.part), 0.0, `states both the right and a wrong ${mutation.name}`),
      );
      record.pairs.push(["correct-prose", "contradiction-both"]);
    },
  );

  add(
    "FORMAT-EQUIVALENCE",
    "the same facts as JSON, full prose and one terse sentence; scores must be near-equal — the fairness/legitimacy exhibit (ARCHITECTURE A4)",
    (record, facts, surface) => {
      record.answers.push(
        answer("correct-prose", prose(surface), 1.0, "full prose"),
        answer("correct-terse", terse(surface), 1.0, "one terse sentence"),
        answer("correct-json", json(schema.jsonFacts(facts)), 1.0, "the same facts as a JSON record"),
      );
      record.constraints.push({
        type: "near_equal", ids: ["correct-prose", "correct-terse", "correct-json"], tolerance: TOL_FORMAT,
        note: "identical facts, three surfaces; a format-sensitive scorer punishes correctness for its shape",
      });
    },
  );

  for (const _ of [0, 1]) {
    add(
      "UNIT/FORM",
      "the same facts in other units and notations (km/h vs m/s, percent vs 0-1, ISO vs prose dates, DMS vs signed decimals); must still count as correct and must still beat a wrong number",
      (record, facts, surface) => {
        const mutation = schema.mutations.find((m) => m.kind === "numeric") ?? schema.mutations[0];
        record.answers.push(
          answer("correct-prose", prose(surface), 1.0, "canonical units"),
          answer("unitform-alt", prose(schema.altSurface(facts)), 1.0, "same facts, converted units and notations"),
          answer(`fact-swap-${mutation.name}`, prose(schema.surface(mutation.apply(facts, rng))), 0.0, `${mutation.name} wrong, canonical units`),
        );
        record.pairs.push(["unitform-alt", `fact-swap-${mutation.name}`], ["correct-prose", `fact-swap-${mutation.name}`]);
        record.constraints.push({
          type: "near_equal", ids: ["correct-prose", "unitform-alt"], tolerance: TOL_UNIT,
          note: "unit normalisation before comparison; a surface-matching scorer marks the converted answer wrong",
        });
      },
    );
  }

  add(
    "TEMPORAL",
    "the right value asserted for the wrong time (point-vs-window, or a shifted window); a right value for the wrong time is a wrong answer",
    (record, facts, surface) => {
      record.answers.push(
        answer("correct-prose", prose(surface), 1.0, "right value, right time"),
        answer("temporal-shift", prose(schema.temporalWrong(facts)), 0.0, "same figures, wrong time semantics"),
      );
      record.pairs.push(["correct-prose", "temporal-shift"]);
    },
  );

  add(
    "LENGTH",
    "correct-terse vs correct-verbose must be near-equal, and a correct verbose answer must beat a wrong terse one; probes a length penalty that rewards brevity over truth",
    (record, facts, surface) => {
      const mutation = schema.mutations.find((m) => m.kind === "categorical") ?? schema.mutations[0];
      const wrong = schema.surface(mutation.apply(facts, rng));
      record.answers.push(
        answer("correct-verbose", verbose(surface), 1.0, "correct, padded with fact-free method prose"),
        answer("correct-terse", terse(surface), 1.0, "correct, one sentence"),
        answer("wrong-terse", terse(wrong), 0.0, `${mutation.name} wrong, one sentence`),
      );
      record.pairs.push(["correct-verbose", "wrong-terse"], ["correct-terse", "wrong-terse"]);
      record.constraints.push({
        type: "near_equal", ids: ["correct-verbose", "correct-terse"], tolerance: TOL_LENGTH,
        note: "same facts, different length; at most a mild style delta is defensible",
      });
    },
  );

  add(
    "OUR-STYLE-WRONG",
    "a livecert-shaped answer (verdict/confidence/reason) with WRONG facts against a competitor-shaped bare record with right facts; the anti-fingerprint proof — our own style must lose when it is wrong",
    (record, facts, surface) => {
      const mutation = pick(rng, schema.mutations);
      const wrongFacts = mutation.apply(facts, rng);
      record.answers.push(
        answer("competitor-right", competitor(schema.jsonFacts(facts)), 1.0, "bare key=value record, grade-report style, facts right"),
        answer("livecert-wrong", livecert(schema.surface(wrongFacts)), 0.0, `our miner's own answer shape, ${mutation.name} wrong`),
      );
      record.pairs.push(["competitor-right", "livecert-wrong"]);
    },
  );

  return records;
}

async function main() {
  const args = new Map();
  for (const token of process.argv.slice(2)) {
    const [k, v] = token.includes("=") ? [token.slice(0, token.indexOf("=")), token.slice(token.indexOf("=") + 1)] : [token, "true"];
    args.set(k, v);
  }
  const intentArg = args.get("--intent") ?? "all";
  const seed = Number(args.get("--seed") ?? 20260827);
  const outDir = args.get("--out") ?? "track2/fixtures/synth";
  const wanted = intentArg === "all" ? Object.keys(SCHEMAS) : intentArg.split(",");

  await mkdir(outDir, { recursive: true });
  console.log(`generate seed=${seed} -> ${outDir}`);
  for (const intent of wanted) {
    if (!SCHEMAS[intent]) throw new Error(`No fact schema for intent ${intent}`);
    const fixtures = buildIntent(intent, seed);
    const document = {
      intent,
      provenance: { source: "synthetic", generator: "track2/harness/gen-synth.mjs", seed, schema_file: "track2/harness/synth-schemas.mjs" },
      fixtures,
    };
    await writeFile(join(outDir, `${intent}.json`), `${JSON.stringify(document, null, 2)}\n`);
    const classes = [...new Set(fixtures.map((f) => f.class))];
    const answers = fixtures.reduce((a, f) => a + f.answers.length, 0);
    const pairs = fixtures.reduce((a, f) => a + f.pairs.length, 0);
    const constraints = fixtures.reduce((a, f) => a + f.constraints.length, 0);
    console.log(
      `${intent.padEnd(18)} fixtures ${String(fixtures.length).padStart(3)}  answers ${String(answers).padStart(3)}  ` +
        `pairs ${String(pairs).padStart(3)}  constraints ${String(constraints).padStart(3)}  classes ${classes.length}`,
    );
  }
}

// Only generate when run directly; gen-probes.mjs imports the renderers from here.
if (process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1])) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
