#!/usr/bin/env node

/**
 * Fast, dependency-free TEXT_AUTHENTICITY_CHECK public benchmark.
 *
 * This is deliberately narrower than run-eval.mjs: it needs no incumbent,
 * network access, worker pool, or Rust toolchain. It is a public semantic
 * regression gate, not a prediction of Telegraph's hidden rotating fixtures.
 */

import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { loadCorpus, mean, round, stddev } from "./corpus.mjs";
import { loadScorer } from "./wasm-abi.mjs";

const INTENT = "TEXT_AUTHENTICITY_CHECK";
const SCRIPT_DIR = dirname(fileURLToPath(import.meta.url));
const FIXTURE_DIR = resolve(SCRIPT_DIR, "../fixtures/synth");

function usage() {
  return `Usage: node check-tac.mjs <module.wasm> [--json]
       node check-tac.mjs --scorer <module.wasm> [--json]

Runs the public TAC semantic proxy with no install, incumbent, network, or transaction.
Exit 0 = every public check passed; 1 = benchmark failure; 2 = usage/load/runtime error.`;
}

function parseArgs(argv) {
  let scorerPath = null;
  let json = false;
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--help" || arg === "-h") return { help: true };
    if (arg === "--json") {
      json = true;
    } else if (arg === "--scorer") {
      if (!argv[i + 1] || argv[i + 1].startsWith("--")) throw new Error("--scorer requires a path");
      if (scorerPath) throw new Error("provide exactly one scorer path");
      scorerPath = argv[++i];
    } else if (arg.startsWith("--")) {
      throw new Error(`unknown option: ${arg}`);
    } else {
      if (scorerPath) throw new Error("provide exactly one scorer path");
      scorerPath = arg;
    }
  }
  if (!scorerPath) throw new Error("missing scorer WASM path");
  return { scorerPath: resolve(scorerPath), json };
}

function check(name, passed, detail) {
  return { name, passed, detail };
}

async function corpusVersion(records) {
  const versions = new Set();
  const paths = [...new Set(records.map((record) => record.source_file))];
  for (const path of paths) {
    const body = JSON.parse(await readFile(path, "utf8"));
    if (typeof body.corpus_version !== "string") throw new Error(`${path}: missing corpus_version`);
    versions.add(body.corpus_version);
  }
  if (versions.size !== 1) {
    throw new Error(`TAC fixture files disagree on corpus_version: ${[...versions].join(", ")}`);
  }
  return [...versions][0];
}

async function run(scorerPath) {
  const scorer = await loadScorer(scorerPath, "candidate");
  const { records } = await loadCorpus([FIXTURE_DIR], INTENT);
  if (records.length === 0) throw new Error(`no ${INTENT} fixtures found in ${FIXTURE_DIR}`);

  const ids = new Set();
  const checks = [];
  const scores = [];
  const goodScores = [];
  const badScores = [];
  const pairFailures = [];
  const constraintFailures = [];
  const selfFailures = [];
  const selfScores = [];
  const byClass = new Map();

  const exports = scorer.exportNames();
  const arity = scorer.arity();
  const requiredExports = ["alloc", "dealloc", "memory", "rank_answer"];
  const missingExports = requiredExports.filter((name) => !exports.includes(name));
  checks.push(
    check(
      "required exports",
      missingExports.length === 0,
      missingExports.length ? `missing ${missingExports.join(", ")}` : requiredExports.join(", "),
    ),
  );
  checks.push(check("rank_answer arity", arity.rank_answer === 6, `observed ${arity.rank_answer}, required 6`));
  checks.push(
    check(
      "alloc/dealloc arity",
      arity.alloc === 1 && arity.dealloc === 2,
      `observed ${arity.alloc}/${arity.dealloc}, required 1/2`,
    ),
  );

  const empty = scorer.score("", "", "");
  const whitespace = scorer.score("question", "truth", " \t\r\n");
  checks.push(
    check(
      "blank answers",
      empty === 0 && whitespace === 0,
      `empty=${empty}, ASCII-whitespace=${whitespace}`,
    ),
  );

  for (const record of records) {
    if (ids.has(record.id)) throw new Error(`duplicate fixture id: ${record.id}`);
    ids.add(record.id);
    const fixtureScores = new Map();
    const classRow = byClass.get(record.class) ?? { fixtures: 0, pairs: 0, wins: 0 };
    classRow.fixtures += 1;
    byClass.set(record.class, classRow);

    for (const answer of record.answers) {
      const value = scorer.score(record.question, record.ground_truth, answer.text);
      fixtureScores.set(answer.id, value);
      scores.push(value);
      if (answer.quality === 1) goodScores.push(value);
      if (answer.quality === 0) badScores.push(value);
    }

    const self = scorer.score(record.question, record.ground_truth, record.ground_truth);
    selfScores.push(self);
    if (!Number.isFinite(self) || self < 0.75 || self > 1) selfFailures.push({ fixture: record.id, score: self });

    for (const [betterId, worseId] of record.pairs ?? []) {
      const better = fixtureScores.get(betterId);
      const worse = fixtureScores.get(worseId);
      classRow.pairs += 1;
      if (better > worse) {
        classRow.wins += 1;
      } else {
        pairFailures.push({
          fixture: record.id,
          better: betterId,
          worse: worseId,
          better_score: round(better),
          worse_score: round(worse),
          delta: round(better - worse),
        });
      }
    }

    for (const constraint of record.constraints ?? []) {
      if (constraint.type !== "near_equal") continue;
      const values = constraint.ids.map((id) => fixtureScores.get(id));
      const spread = Math.max(...values) - Math.min(...values);
      if (spread > constraint.tolerance) {
        constraintFailures.push({
          fixture: record.id,
          ids: constraint.ids,
          spread: round(spread),
          tolerance: constraint.tolerance,
        });
      }
    }
  }

  const invalidScores = scores.filter((value) => !Number.isFinite(value) || value < 0 || value > 1);
  checks.push(
    check(
      "finite scores in [0,1]",
      invalidScores.length === 0,
      `${scores.length - invalidScores.length}/${scores.length}`,
    ),
  );
  checks.push(
    check(
      "self-match floor",
      selfFailures.length === 0,
      `${records.length - selfFailures.length}/${records.length} at or above 0.75`,
    ),
  );

  const totalPairs = [...byClass.values()].reduce((sum, row) => sum + row.pairs, 0);
  const wins = totalPairs - pairFailures.length;
  checks.push(check("strict semantic pair wins", pairFailures.length === 0, `${wins}/${totalPairs}`));
  const totalConstraints = records.reduce(
    (sum, record) => sum + (record.constraints ?? []).filter((item) => item.type === "near_equal").length,
    0,
  );
  checks.push(
    check(
      "equivalent-answer constraints",
      constraintFailures.length === 0,
      `${totalConstraints - constraintFailures.length}/${totalConstraints}`,
    ),
  );

  const meanGood = mean(goodScores);
  const meanBad = mean(badScores);
  const passed = checks.every((item) => item.passed);
  return {
    schema_version: 1,
    benchmark: "telegraph-public-tac-proxy",
    scope_notice: "Public semantic regression proxy; not Telegraph's hidden rotating fixture gate.",
    intent: INTENT,
    corpus_version: await corpusVersion(records),
    artifact: {
      path: scorerPath,
      bytes: scorer.sizeBytes,
      sha256: scorer.sha256,
      exports,
      arity,
    },
    corpus: { fixtures: records.length, answers: scores.length, pairs: totalPairs, constraints: totalConstraints },
    metrics: {
      wins,
      total_pairs: totalPairs,
      mean_good: round(meanGood),
      mean_bad: round(meanBad),
      separation: round(meanGood - meanBad),
      score_stddev: round(stddev(scores)),
      worst_self_match: round(Math.min(...selfScores)),
    },
    classes: Object.fromEntries([...byClass.entries()].sort().map(([name, row]) => [name, row])),
    checks,
    failures: { pairs: pairFailures, constraints: constraintFailures, self_match: selfFailures },
    passed,
  };
}

function render(result) {
  const lines = [
    "TAC PUBLIC SEMANTIC PROXY (not the hidden node gate)",
    `  artifact  ${result.artifact.bytes} bytes | sha256 ${result.artifact.sha256}`,
    `  corpus    v${result.corpus_version} | ${result.corpus.fixtures} fixtures | ${result.corpus.answers} answers | ${result.corpus.pairs} pairs`,
    "",
  ];
  for (const item of result.checks) {
    lines.push(`  ${item.passed ? "PASS" : "FAIL"}  ${item.name}: ${item.detail}`);
  }
  lines.push(
    "",
    `  separation ${result.metrics.separation} (good ${result.metrics.mean_good} - bad ${result.metrics.mean_bad})`,
    `  spread     ${result.metrics.score_stddev} score stddev | worst self-match ${result.metrics.worst_self_match}`,
  );
  for (const [name, row] of Object.entries(result.classes)) {
    lines.push(`  ${name.padEnd(19)} ${row.wins}/${row.pairs} pair wins`);
  }
  if (result.failures.pairs.length) {
    lines.push("", "  First pair failures:");
    for (const failure of result.failures.pairs.slice(0, 5)) {
      lines.push(
        `    ${failure.fixture}: ${failure.better} ${failure.better_score} <= ` +
          `${failure.worse} ${failure.worse_score}`,
      );
    }
  }
  lines.push("", `VERDICT: ${result.passed ? "PASS" : "FAIL"}`, result.scope_notice);
  return lines.join("\n");
}

async function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (error) {
    console.error(`ERROR: ${error.message}\n\n${usage()}`);
    process.exitCode = 2;
    return;
  }
  if (args.help) {
    console.log(usage());
    return;
  }
  try {
    const result = await run(args.scorerPath);
    console.log(args.json ? JSON.stringify(result, null, 2) : render(result));
    process.exitCode = result.passed ? 0 : 1;
  } catch (error) {
    console.error(`ERROR: ${error.message}`);
    process.exitCode = 2;
  }
}

await main();
