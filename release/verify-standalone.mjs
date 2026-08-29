#!/usr/bin/env node

/** Read-only release-surface audit for the standalone Track 2 repository. */

import { createHash } from "node:crypto";
import { readdir, readFile, stat } from "node:fs/promises";
import { dirname, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const ROOT = resolve(process.argv[2] ?? join(HERE, ".."));
const REQUIRED = [
  ".cargo/config.toml",
  ".github/ISSUE_TEMPLATE/benchmark-result.yml",
  ".github/workflows/ci.yml",
  "Cargo.lock",
  "Cargo.toml",
  "JUDGE_BRIEF.md",
  "LICENSE",
  "PROOF.md",
  "README.md",
  "dist/text_authenticity.wasm",
  "fixtures/synth/TEXT_AUTHENTICITY_CHECK_CLEAN_PAIR.json",
  "fixtures/synth/TEXT_AUTHENTICITY_CHECK_LABEL_EQUIVALENCE.json",
  "harness/check-tac.mjs",
  "harness/corpus.mjs",
  "harness/README.md",
  "harness/report.mjs",
  "harness/run-eval.mjs",
  "harness/score-pool.mjs",
  "harness/wasm-abi.mjs",
  "release/README.md",
  "release/probe-negation.mjs",
  "release/probe-model-aliases.mjs",
  "release/probe-authenticity-axes.mjs",
  "release/probe-authenticity-vocabulary.mjs",
  "release/text-authenticity.json",
  "release/verify-standalone.mjs",
  "rust-toolchain.toml",
  "src/lib.rs",
  "verify.mjs",
];
const FORBIDDEN = ["release/PUBLISH.md", "release/standalone-ci.yml", "tune.md"];
const TEXT_EXTENSIONS = new Set([".json", ".md", ".mjs", ".rs", ".toml", ".yml", ".yaml"]);

async function exists(path) {
  try {
    await stat(path);
    return true;
  } catch (error) {
    if (error.code === "ENOENT") return false;
    throw error;
  }
}

async function walk(dir, relative = "") {
  const files = [];
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    if ([".git", "target", "champions"].includes(entry.name)) continue;
    const childRelative = relative ? `${relative}/${entry.name}` : entry.name;
    const child = join(dir, entry.name);
    if (entry.isDirectory()) files.push(...(await walk(child, childRelative)));
    else files.push({ path: child, relative: childRelative });
  }
  return files;
}

function reportFailures(failures) {
  console.error("STANDALONE RELEASE: FAIL");
  for (const failure of failures) console.error(`  - ${failure}`);
  process.exitCode = 1;
}

async function main() {
  const failures = [];
  for (const relative of REQUIRED) {
    if (!(await exists(join(ROOT, relative)))) failures.push(`missing required file: ${relative}`);
  }
  for (const relative of FORBIDDEN) {
    if (await exists(join(ROOT, relative))) failures.push(`development-only file is public: ${relative}`);
  }
  if (failures.some((failure) => failure.startsWith("missing required file:"))) {
    reportFailures(failures);
    return;
  }

  const dist = (await readdir(join(ROOT, "dist"))).filter((name) => extname(name) === ".wasm").sort();
  if (dist.length !== 1 || dist[0] !== "text_authenticity.wasm") {
    failures.push(`dist must contain only text_authenticity.wasm; found: ${dist.join(", ") || "none"}`);
  }

  const expectedHarness = new Set([
    "README.md",
    "check-tac.mjs",
    "corpus.mjs",
    "report.mjs",
    "run-eval.mjs",
    "score-pool.mjs",
    "wasm-abi.mjs",
  ]);
  const harnessFiles = (await readdir(join(ROOT, "harness"))).sort();
  const extraHarness = harnessFiles.filter((name) => !expectedHarness.has(name));
  if (extraHarness.length) failures.push(`non-release harness files present: ${extraHarness.join(", ")}`);

  const fixtureRootEntries = await readdir(join(ROOT, "fixtures"), { withFileTypes: true });
  const extraFixtureRoots = fixtureRootEntries
    .filter((entry) => entry.name !== "synth")
    .map((entry) => entry.name)
    .sort();
  if (extraFixtureRoots.length) failures.push(`non-TAC fixture roots present: ${extraFixtureRoots.join(", ")}`);
  const expectedFixtures = new Set([
    "TEXT_AUTHENTICITY_CHECK_CLEAN_PAIR.json",
    "TEXT_AUTHENTICITY_CHECK_LABEL_EQUIVALENCE.json",
  ]);
  const synthFiles = (await readdir(join(ROOT, "fixtures", "synth"))).sort();
  const extraFixtures = synthFiles.filter((name) => !expectedFixtures.has(name));
  if (extraFixtures.length) failures.push(`non-release TAC fixtures present: ${extraFixtures.join(", ")}`);

  const manifest = JSON.parse(await readFile(join(ROOT, "release/text-authenticity.json"), "utf8"));
  const wasm = await readFile(join(ROOT, "dist/text_authenticity.wasm"));
  const sha256 = createHash("sha256").update(wasm).digest("hex");
  if (wasm.length !== manifest.bytes) failures.push(`manifest bytes ${manifest.bytes} != artifact ${wasm.length}`);
  if (sha256 !== manifest.sha256) failures.push(`manifest sha256 ${manifest.sha256} != artifact ${sha256}`);
  if (manifest.intent !== "TEXT_AUTHENTICITY_CHECK") failures.push(`unexpected manifest intent: ${manifest.intent}`);

  const fixtureNames = [
    "TEXT_AUTHENTICITY_CHECK_CLEAN_PAIR.json",
    "TEXT_AUTHENTICITY_CHECK_LABEL_EQUIVALENCE.json",
  ];
  let fixtureCount = 0;
  let pairCount = 0;
  const corpusVersions = new Set();
  for (const name of fixtureNames) {
    const fixture = JSON.parse(await readFile(join(ROOT, "fixtures/synth", name), "utf8"));
    corpusVersions.add(fixture.corpus_version);
    fixtureCount += fixture.fixtures.length;
    pairCount += fixture.fixtures.reduce((sum, row) => sum + (row.pairs ?? []).length, 0);
  }
  if (corpusVersions.size !== 1 || !corpusVersions.has(manifest.offline_evidence.corpus_version)) {
    failures.push(`fixture corpus version does not match manifest: ${[...corpusVersions].join(", ")}`);
  }
  if (fixtureCount !== manifest.offline_evidence.native_fixtures) {
    failures.push(`manifest fixtures ${manifest.offline_evidence.native_fixtures} != corpus ${fixtureCount}`);
  }
  if (pairCount !== manifest.offline_evidence.native_pairs) {
    failures.push(`manifest pairs ${manifest.offline_evidence.native_pairs} != corpus ${pairCount}`);
  }

  for (const relative of ["README.md", "PROOF.md", "JUDGE_BRIEF.md"]) {
    const body = await readFile(join(ROOT, relative), "utf8");
    if (!body.includes(manifest.sha256)) failures.push(`${relative} does not bind the full release SHA-256`);
  }

  const localPathPattern = /(?:[A-Za-z]:[\\/]Users[\\/]|\/Users\/|\/home\/)[^\s"'`)]+/i;
  for (const file of await walk(ROOT)) {
    if (!TEXT_EXTENSIONS.has(extname(file.relative))) continue;
    const body = await readFile(file.path, "utf8");
    const leak = body.match(localPathPattern);
    if (leak) failures.push(`local path leaked in ${file.relative}: ${leak[0]}`);
  }

  if (failures.length) {
    reportFailures(failures);
    return;
  }
  console.log(`STANDALONE RELEASE: PASS | ${wasm.length} bytes | sha256 ${sha256}`);
}

await main();
