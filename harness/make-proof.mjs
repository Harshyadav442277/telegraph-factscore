#!/usr/bin/env node

/**
 * One command that regenerates the whole Track 2 review pack.
 *
 *   node track2/harness/make-proof.mjs [--ipgeo-champion PATH] [--storm-champion PATH]
 *        [--weather-champion PATH] [--ipgeo-scorer PATH] [--storm-scorer PATH]
 *        [--weather-scorer PATH] [--workers N] [--reports DIR] [--out PATH]
 *        [--no-poll] [--no-weather] [--reuse] [--help]
 *
 * Runs run-eval.mjs once per target, scores a handful of exhibits in-process, and
 * writes track2/PROOF.md. Every number in that document comes from this run --
 * nothing is transcribed, so re-running it after a rebuild cannot leave a stale
 * figure behind. The document is written for a reviewer who has never seen this
 * repository, so it repeats context the rest of track2/ takes for granted.
 *
 * Champion binaries are NOT in the repo (24 MB each). Default lookup order per
 * target: track2/harness/champions/<name>.wasm, then the session scratchpad copy.
 * Either can be overridden with the flags above. Fetch instructions, and the
 * registry URLs the bytes came from, are printed into PROOF.md itself.
 */

import { spawn } from "node:child_process";
import { createHash } from "node:crypto";
import { mkdir, readFile, readdir, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { loadScorer } from "./wasm-abi.mjs";
import { loadCorpus, round } from "./corpus.mjs";
import { HAND_CASES, renderProof } from "./proof-doc.mjs";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(HERE, "..", "..");
const SCRATCH =
  "C:/Users/hyada/AppData/Local/Temp/claude/C--Users-hyada-OneDrive-Documents-Work-Related-Hackathons-Telegraph/5bf15b88-ade8-4ce4-9eee-7676eea0666d/scratchpad";
const FIXTURE_DIRS = ["real", "synth", "probe"].map((d) => join(ROOT, "track2", "fixtures", d));
const REGISTRY = "https://devnode.telegraphprotocol.com/api/wasm";

/** Registry provenance for each incumbent, so a reviewer can re-download the exact bytes. */
const REGISTRY_PINS = {
  IP_GEOLOCATION: {
    registration_id: 630,
    wasm_hash: "636983a2fd5a72974977d62cbd50ff97df87efa0b16287ce24ca254369703094",
    wasm_url:
      "https://raw.githubusercontent.com/zkasuran/telegraph-salience-scorer/8dcc6b775400c48bf0895923f0407aa3d2b2bfb2/dist/subagent/IP_GEOLOCATION.wasm",
  },
  STORM_ALERT: {
    registration_id: 453,
    wasm_hash: "f30451baead010294d0adcd679bd328c8bc324d175758588028c1c19cb0cb536",
    wasm_url:
      "https://raw.githubusercontent.com/zkasuran/telegraph-salience-scorer/72616195155200974eac9982e3121aa48f5f8373/dist/xfmr/storm_rpen.wasm",
  },
  WEATHER_FORECAST: {
    registration_id: 636,
    wasm_hash: "dd7dc9e9adab581c6f124050bd76a5f88b6f4bcdedf64dbc79993bc055f963ff",
    wasm_url:
      "https://raw.githubusercontent.com/zkasuran/telegraph-salience-scorer/f009d2d778bd49611dcc0a7e3819a8dca74d1aad/dist/xfmr/wf_mini.wasm",
  },
};


function parseArgs(argv) {
  const args = new Map();
  for (let i = 0; i < argv.length; i += 1) {
    const token = argv[i];
    if (!token.startsWith("--")) throw new Error(`Unexpected argument: ${token}`);
    const equal = token.indexOf("=");
    if (equal > 0) {
      args.set(token.slice(0, equal), token.slice(equal + 1));
      continue;
    }
    const next = argv[i + 1];
    if (next === undefined || next.startsWith("--")) args.set(token, "true");
    else {
      args.set(token, next);
      i += 1;
    }
  }
  return args;
}

/** First existing of: explicit flag, repo-local champions/, session scratchpad. */
function champion(args, flag, basename) {
  const explicit = args.get(flag);
  if (explicit && explicit !== "true") return resolve(ROOT, explicit);
  const local = join(HERE, "champions", basename);
  if (existsSync(local)) return local;
  return join(SCRATCH, "wasm", basename);
}

function scorerPath(args, flag, relative) {
  const explicit = args.get(flag);
  return explicit && explicit !== "true" ? resolve(ROOT, explicit) : join(ROOT, relative);
}

async function runEval(target, workers, reportsDir) {
  const argv = [
    join(HERE, "run-eval.mjs"),
    "--scorer", target.scorer,
    "--against", target.champion,
    "--intent", target.intent,
    "--fixtures", FIXTURE_DIRS.join(","),
    "--workers", String(workers),
    "--out", reportsDir,
    "--quiet",
  ];
  process.stderr.write(`  ${target.intent}: running gate proxy (this takes a few minutes on a 24 MB incumbent)\n`);
  const stdout = await new Promise((ok, bad) => {
    const child = spawn(process.execPath, argv, { cwd: ROOT });
    let out = "";
    child.stdout.on("data", (b) => (out += b));
    child.stderr.on("data", (b) => process.stderr.write(b));
    child.on("error", bad);
    child.on("close", () => ok(out));
  });
  const match = stdout.match(/report written: (.+)$/m);
  if (!match) throw new Error(`run-eval produced no report for ${target.intent}:\n${stdout.slice(-2000)}`);
  const path = match[1].trim();
  return { report: JSON.parse(await readFile(path, "utf8")), reportPath: path, text: stdout };
}

/** Newest report per intent, for --reuse. */
async function reuseReports(targets, reportsDir) {
  const names = (await readdir(reportsDir)).filter((n) => n.startsWith("report-") && n.endsWith(".json")).sort();
  const out = [];
  for (const target of targets) {
    let found = null;
    for (const name of names) {
      const path = join(reportsDir, name);
      const report = JSON.parse(await readFile(path, "utf8"));
      if (report.corpus.intent === target.intent) found = { report, reportPath: path };
    }
    if (!found) throw new Error(`--reuse: no report for ${target.intent} in ${reportsDir}`);
    out.push({ target, ...found });
  }
  return out;
}

async function sha256File(path) {
  return createHash("sha256").update(await readFile(path)).digest("hex");
}

/**
 * Every report must describe the bytes that are on disk right now.
 *
 * This is not paranoia. A scorer rebuilt while this tool runs -- or a --reuse
 * against a report from an older build -- produces a document whose gate tables
 * and whose in-process exhibits describe two different modules, with nothing on
 * the page to say so. That failure is silent and it is fatal to the whole point
 * of the pack, so it is checked rather than hoped for.
 */
async function verifyConsistency(runs) {
  const problems = [];
  for (const { target, report, reportPath } of runs) {
    for (const [role, path, recorded] of [
      ["candidate", target.scorer, report.candidate.sha256],
      ["incumbent", target.champion, report.against?.sha256],
    ]) {
      if (!recorded) continue;
      const actual = await sha256File(path);
      if (actual !== recorded) {
        problems.push(
          `${target.intent} ${role}: ${path}\n` +
            `    report ${reportPath.split(/[\\/]/).pop()} scored sha256 ${recorded.slice(0, 16)}…\n` +
            `    the file on disk is now         sha256 ${actual.slice(0, 16)}…`,
        );
      }
    }
  }
  if (problems.length) {
    throw new Error(
      `The modules on disk are not the modules these reports measured:\n\n${problems.join("\n\n")}\n\n` +
        "PROOF.md would mix builds and say nothing about it. Re-run without --reuse (and, if a rebuild is\n" +
        "in flight, wait for it to finish first) so every number in the document comes from one set of bytes.",
    );
  }
}

async function poll(intent) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), 12000);
  try {
    const response = await fetch(`${REGISTRY}?intent=${encodeURIComponent(intent)}`, { signal: controller.signal });
    if (!response.ok) return { error: `HTTP ${response.status}` };
    const body = await response.json();
    const block = body.intents?.[intent];
    if (!block?.champion) return { error: "no champion in response" };
    const c = block.champion;
    return {
      registration_id: c.registration_id,
      wasm_hash: c.wasm_hash,
      eval: c.eval ?? null,
      entries: block.entries?.length ?? null,
      authors: block.entries ? new Set(block.entries.map((e) => e.author_address)).size : null,
      registered_at: c.registered_at,
    };
  } catch (error) {
    return { error: error.name === "AbortError" ? "timed out" : error.message };
  } finally {
    clearTimeout(timer);
  }
}

/* ---------------- in-process exhibits ---------------- */

/** Score one HAND_CASES block (its question, ground truth and answers) with both modules. */
function scorePairs(cand, ref, block) {
  const { question: q, ground_truth: gt, cases } = block;
  return cases.map(([label, text]) => ({
    label,
    candidate: round(cand.score(q, gt, text)),
    reference: round(ref.score(q, gt, text)),
  }));
}

/**
 * Serial, single-threaded latency — the number the node's 600 s budget is spent in.
 * Scores an ordinary answer, never the ground truth: both modules short-circuit an
 * exact self-match, so timing that would measure the short-circuit, not the work.
 */
function latency(scorer, records) {
  const sample = records.filter((r) => r.answers.some((a) => a.text.trim().length > 0)).slice(0, 6);
  const start = process.hrtime.bigint();
  for (const record of sample) {
    const answer = record.answers.find((a) => a.text.trim().length > 0);
    scorer.score(record.question, record.ground_truth, answer.text);
  }
  return Number(process.hrtime.bigint() - start) / 1e9 / Math.max(1, sample.length);
}

/** One corpus fixture rendered verbatim with both modules' scores on every answer. */
function verbatim(cand, ref, record) {
  return {
    id: record.id,
    intent: record.intent,
    class: record.class,
    question: record.question,
    ground_truth: record.ground_truth,
    answers: record.answers.map((a) => ({
      id: a.id,
      note: a.note ?? "",
      quality: a.quality,
      text: a.text,
      candidate: round(cand.score(record.question, record.ground_truth, a.text)),
      reference: round(ref.score(record.question, record.ground_truth, a.text)),
    })),
  };
}

async function inProcessExhibits(runs) {
  const out = { verbatim: [], cvss: null, wind: null, latency: [] };
  for (const run of runs) {
    if (run.target.role !== "gate") continue;
    const cand = await loadScorer(run.target.scorer, "candidate");
    const ref = await loadScorer(run.target.champion, "incumbent");
    const { records } = await loadCorpus(FIXTURE_DIRS, run.target.intent);
    for (const klass of ["FACT-SWAP", "UNIT/FORM"]) {
      const record = records.find((r) => r.class === klass);
      if (record) out.verbatim.push(verbatim(cand, ref, record));
    }
    out.latency.push({
      intent: run.target.intent,
      candidate: round(latency(cand, records), 6),
      reference: round(latency(ref, records), 4),
    });
    if (run.target.intent === "IP_GEOLOCATION") {
      out.cvss = { rows: scorePairs(cand, ref, HAND_CASES.cvss), pair: run.target };
    }
    if (run.target.intent === "STORM_ALERT") {
      out.wind = { rows: scorePairs(cand, ref, HAND_CASES.wind), pair: run.target };
    }
  }
  return out;
}

/* ---------------- main ---------------- */

const HELP = `make-proof.mjs — regenerate track2/PROOF.md from a live run

  --ipgeo-scorer PATH      default track2/scorer/dist/ip_geolocation.wasm
  --ipgeo-champion PATH    default champions/ipgeo_reg630.wasm, then the scratchpad copy
  --storm-scorer PATH      default track2/scorer/dist/storm_alert.wasm
  --storm-champion PATH    default champions/storm_rpen_reg453.wasm, then the scratchpad copy
  --weather-scorer PATH    default track2/scorer/dist/generic.wasm      (exhibit only)
  --weather-champion PATH  default champions/wf_mini_reg636.wasm        (exhibit only)
  --no-weather             skip the WEATHER_FORECAST exhibit run
  --workers N              default 8
  --reports DIR            where run-eval writes JSON   (default track2/fixtures)
  --out PATH               default track2/PROOF.md
  --no-poll                do not contact the live registry
  --reuse                  rebuild PROOF.md from the newest existing reports, no re-scoring
`;

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.has("--help") || args.has("-h")) {
    process.stdout.write(HELP);
    return;
  }
  const workers = Number(args.get("--workers") ?? 8);
  const reportsDir = resolve(ROOT, args.get("--reports") ?? "track2/fixtures");
  const outPath = resolve(ROOT, args.get("--out") ?? "track2/PROOF.md");

  const targets = [
    {
      intent: "IP_GEOLOCATION",
      role: "gate",
      scorer: scorerPath(args, "--ipgeo-scorer", "track2/scorer/dist/ip_geolocation.wasm"),
      champion: champion(args, "--ipgeo-champion", "ipgeo_reg630.wasm"),
      championName: "ipgeo_reg630.wasm",
    },
    {
      intent: "STORM_ALERT",
      role: "gate",
      scorer: scorerPath(args, "--storm-scorer", "track2/scorer/dist/storm_alert.wasm"),
      champion: champion(args, "--storm-champion", "storm_rpen_reg453.wasm"),
      championName: "storm_rpen_reg453.wasm",
    },
  ];
  if (!args.has("--no-weather")) {
    targets.push({
      intent: "WEATHER_FORECAST",
      role: "exhibit",
      scorer: scorerPath(args, "--weather-scorer", "track2/scorer/dist/generic.wasm"),
      champion: champion(args, "--weather-champion", "wf_mini_reg636.wasm"),
      championName: "wf_mini_reg636.wasm",
    });
  }
  for (const target of targets) {
    target.pin = REGISTRY_PINS[target.intent];
    target.scorerName = target.scorer.split(/[\\/]/).pop();
    // The command printed into PROOF.md must be the command that ran, so it is
    // derived from the resolved path rather than the default it usually equals.
    target.scorerRel = target.scorer.startsWith(ROOT)
      ? target.scorer.slice(ROOT.length).replace(/^[\\/]/, "").replace(/\\/g, "/")
      : target.scorer.replace(/\\/g, "/");
    if (!existsSync(target.scorer)) throw new Error(`candidate module not found: ${target.scorer}`);
    if (!existsSync(target.champion)) {
      throw new Error(
        `incumbent binary not found: ${target.champion}\n` +
          `  fetch it with:  curl -sL -o track2/harness/champions/${target.championName} "${target.pin.wasm_url}"`,
      );
    }
  }

  await mkdir(reportsDir, { recursive: true });
  const started = Date.now();
  let runs;
  if (args.has("--reuse")) {
    process.stderr.write("reusing the newest existing reports (no re-scoring)\n");
    runs = await reuseReports(targets, reportsDir);
  } else {
    runs = [];
    for (const target of targets) runs.push({ target, ...(await runEval(target, workers, reportsDir)) });
  }

  await verifyConsistency(runs);

  process.stderr.write("  scoring the verbatim exhibits in-process\n");
  const gateRuns = runs.filter((r) => r.target.role === "gate");
  const exhibits = await inProcessExhibits(runs);

  const polls = {};
  if (!args.has("--no-poll")) {
    for (const target of targets) {
      process.stderr.write(`  polling the registry for ${target.intent}\n`);
      polls[target.intent] = await poll(target.intent);
    }
  } else {
    for (const target of targets) polls[target.intent] = { error: "skipped (--no-poll)" };
  }

  const markdown = renderProof({
    runs,
    gateRuns,
    exhibits,
    polls,
    workers,
    reportsDir: (reportsDir.replace(ROOT, "").replace(/^[\\/]/, "") || reportsDir).replace(/\\/g, "/"),
    generatedAt: new Date().toISOString(),
  });
  await writeFile(outPath, markdown);
  process.stderr.write(`\nPROOF.md written: ${outPath}  (${Math.round((Date.now() - started) / 1000)}s)\n`);
}

main().catch((error) => {
  console.error(error instanceof Error ? error.message : String(error));
  process.exitCode = 1;
});
