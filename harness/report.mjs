#!/usr/bin/env node

/** Plain-text rendering of the run-eval report. No colour, no spinners. */

const n = (v, w = 9) => (v === null || v === undefined ? "—" : Number(v).toFixed(4)).toString().padStart(w);
const i = (v, w = 5) => (v === null || v === undefined ? "—" : String(v)).padStart(w);
const rule = (ch = "─", w = 100) => ch.repeat(w);

function stage1Block(stage1) {
  const lines = [`STAGE 1 — structural (candidate only)   ${stage1.pass ? "PASS" : "FAIL"}`, rule()];
  for (const check of stage1.checks) {
    const state = check.pass ? "PASS" : check.advisory ? "WARN" : "FAIL";
    lines.push(`  ${state}  ${check.name}${check.advisory ? "   [advisory — stricter than the node's gate]" : ""}`);
    lines.push(`        ${check.detail}`);
  }
  return lines.join("\n");
}

function gateTable(gate, hasAgainst) {
  const head =
    "  intent             fixtures  worst_self   stddev   margin   mean_good  mean_bad   wins/pairs   acc   spearman";
  const lines = [head, `  ${rule("-", 98)}`];
  for (const [name, block] of Object.entries(gate)) {
    const rows = [["cand", block.candidate]];
    if (hasAgainst) rows.push(["ref ", block.against]);
    rows.forEach(([tag, m], idx) => {
      if (!m) return;
      const label = idx === 0 ? name.padEnd(18) : "".padEnd(18);
      const spear = idx === 0 && block.spearman ? n(block.spearman.rho, 8) : "        ";
      lines.push(
        `  ${label} ${i(m.fixtures, 4)} ${tag} ${n(m.worst_self_match, 8)} ${n(m.score_stddev, 8)} ` +
          `${n(m.separation, 8)} ${n(m.mean_good, 9)} ${n(m.mean_bad, 9)} ` +
          `${i(m.wins, 5)}/${String(m.pairs).padEnd(4)} ${n(m.accuracy, 5)} ${spear}`,
      );
    });
  }
  return lines.join("\n");
}

function verdictBlock(gate) {
  const lines = [];
  for (const [name, block] of Object.entries(gate)) {
    if (!block.verdict) continue;
    const counts = block.verdict.checks.reduce((acc, c) => ({ ...acc, [c.state]: (acc[c.state] ?? 0) + 1 }), {});
    lines.push(
      `  ${name.padEnd(18)} ${block.verdict.pass ? "would promote" : "WOULD BE REJECTED"}   ` +
        `(${counts.PASS ?? 0} pass, ${counts.FAIL ?? 0} fail, ${counts.SKIP ?? 0} skip; ${block.distinct_miners} miner(s) with history)`,
    );
    for (const check of block.verdict.checks) lines.push(`      ${check.state.padEnd(4)} ${check.name.padEnd(46)} ${check.detail}`);
    lines.push("");
  }
  return lines.join("\n");
}

function classTable(gate, hasAgainst) {
  const classes = new Set();
  for (const block of Object.values(gate)) {
    for (const k of Object.keys(block.candidate.per_class)) classes.add(k);
  }
  const lines = [
    `  class               pairs   cand_acc  cand_margin${hasAgainst ? "    ref_acc   ref_margin   delta_acc" : ""}`,
    `  ${rule("-", hasAgainst ? 84 : 48)}`,
  ];
  const all = gate.ALL;
  for (const klass of [...classes].sort()) {
    const c = all.candidate.per_class[klass];
    const r = hasAgainst ? all.against?.per_class?.[klass] : null;
    if (!c) continue;
    let line = `  ${klass.padEnd(20)} ${i(c.pairs, 4)}   ${n(c.accuracy, 8)}  ${n(c.mean_margin, 10)}`;
    if (hasAgainst && r) line += `  ${n(r.accuracy, 9)}  ${n(r.mean_margin, 11)}  ${n(c.accuracy - r.accuracy, 11)}`;
    lines.push(line);
  }
  return lines.join("\n");
}

function constraintTable(gate, hasAgainst) {
  const all = gate.ALL;
  const keys = new Set([...Object.keys(all.candidate.constraints), ...Object.keys(all.against?.constraints ?? {})]);
  if (keys.size === 0) return "  (no near-equality constraints in scope)";
  const lines = [`  class               tol    cand_ok   cand_worst${hasAgainst ? "   ref_ok    ref_worst" : ""}`, `  ${rule("-", hasAgainst ? 70 : 48)}`];
  for (const klass of [...keys].sort()) {
    const c = all.candidate.constraints[klass];
    const r = all.against?.constraints?.[klass];
    let line = `  ${klass.padEnd(20)} ${String(c?.tolerance ?? "—").padEnd(6)} ${String(`${c?.satisfied ?? 0}/${c?.total ?? 0}`).padStart(7)}   ${n(c?.worst_spread, 9)}`;
    if (hasAgainst && r) line += `   ${String(`${r.satisfied}/${r.total}`).padStart(7)}  ${n(r.worst_spread, 9)}`;
    lines.push(line);
  }
  return lines.join("\n");
}

function exhibitBlock(rows, hasAgainst) {
  if (rows.length === 0) return "  (none triggered)";
  const lines = [];
  const byKind = new Map();
  for (const row of rows) {
    if (!byKind.has(row.kind)) byKind.set(row.kind, []);
    byKind.get(row.kind).push(row);
  }
  for (const [kind, list] of byKind) {
    lines.push(`  ${kind}  (${list.length})`);
    for (const row of list) {
      lines.push(`    ${row.intent} / ${row.fixture}`);
      for (const [label, value] of Object.entries(row.scores)) {
        if (!value) continue;
        lines.push(`      ${label.padEnd(34)} candidate ${n(value.candidate, 9)}${hasAgainst ? `   reference ${n(value.against, 9)}` : ""}`);
      }
      if (row.live_scores) {
        lines.push(`      live scores from the node: ${Object.entries(row.live_scores).map(([k, v]) => `${k}=${v}`).join(", ")}`);
      }
    }
    lines.push("");
  }
  return lines.join("\n");
}

function timingBlock(report) {
  const rows = [["candidate", report.candidate], ["reference", report.against]].filter(([, v]) => v);
  const lines = [];
  for (const [tag, block] of rows) {
    const t = block.timing;
    lines.push(`  ${tag.padEnd(10)} corpus run: ${t.wall_seconds}s wall over ${report.corpus.workers} workers (${t.worker_seconds_per_call}s worker-seconds/call)`);
  }
  lines.push(
    `  serial in-process latency (candidate): ${report.stage1.serial_seconds_per_call}s/call  ->  ` +
      `a ~66-call gate projects to ~${report.stage1.projected_gate_seconds}s of the 600s budget`,
  );
  lines.push("  worker-seconds are inflated by memory contention across 24 MB modules; the serial number is the one that matters.");
  return lines.join("\n");
}

export function renderReport(report) {
  const hasAgainst = Boolean(report.against);
  return [
    rule("="),
    "TRACK 2 GATE-PROXY REPORT",
    rule("="),
    `generated   ${report.generated_at}`,
    `candidate   ${report.candidate.path}`,
    `            sha256 ${report.candidate.sha256}  (${report.candidate.bytes} bytes)`,
    hasAgainst ? `reference   ${report.against.path}` : "reference   (none — Stage-2 comparisons are SKIPPED, not PASSED)",
    hasAgainst ? `            sha256 ${report.against.sha256}  (${report.against.bytes} bytes)` : null,
    `corpus      ${report.corpus.fixtures} fixtures from ${report.corpus.dirs.join(", ")} (intent=${report.corpus.intent}), ` +
      `${report.corpus.calls_per_module} calls/module, REAL answers capped at ${report.corpus.max_real_answers}`,
    `thresholds  ${report.thresholds.source}`,
    `            ${report.thresholds.status}`,
    "",
    stage1Block(report.stage1),
    "",
    `STAGE 2 — separation proxy on this corpus (NOT the node's benchmark)`,
    rule(),
    gateTable(report.gate_proxy, hasAgainst),
    "",
    "GATE VERDICT (node Stage-2 conditions applied to this corpus)",
    rule(),
    verdictBlock(report.gate_proxy),
    "PER-CLASS PAIRWISE RANKING ACCURACY (all intents pooled)",
    rule(),
    classTable(report.gate_proxy, hasAgainst),
    "",
    "NEAR-EQUALITY CONSTRAINTS (score spread within a fixture's equivalent answers)",
    rule(),
    constraintTable(report.gate_proxy, hasAgainst),
    "",
    "HEADLINE EXHIBITS",
    rule(),
    exhibitBlock(report.exhibits, hasAgainst),
    "TIMING vs the gate's wall clock",
    rule(),
    timingBlock(report),
    rule("="),
  ]
    .filter((line) => line !== null)
    .join("\n");
}
