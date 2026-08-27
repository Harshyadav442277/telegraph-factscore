#!/usr/bin/env node
// Poll the live scorer registry for the current promotion bar per intent.
//
// The bar a challenger must beat is the champion's margin measured on the node's
// CURRENT fixtures — visible only in the eval block of the newest challenger,
// because each challenge re-measures the incumbent (the champion's own stored
// eval is frozen at its promotion). Fixture rotation swings the bar (weather
// moved 0.53 -> 0.99 in 48h), so register at a local low.
//
//   node track2/harness/poll-bar.mjs IP_GEOLOCATION STORM_ALERT
//
// Appends one line per intent to track2/fixtures/bar-history.log so drift is
// visible across polls.

import { appendFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const API = "https://devnode.telegraphprotocol.com/api/wasm";
const LOG = join(dirname(fileURLToPath(import.meta.url)), "..", "fixtures", "bar-history.log");

const intents = process.argv.slice(2);
if (intents.length === 0) {
  console.error("usage: node poll-bar.mjs INTENT [INTENT...]");
  process.exit(2);
}

for (const intent of intents) {
  const res = await fetch(`${API}?intent=${encodeURIComponent(intent)}`);
  if (!res.ok) {
    console.error(`${intent}: registry returned HTTP ${res.status}`);
    continue;
  }
  const body = await res.json();
  const record = body.intents?.[intent];
  if (!record) {
    console.error(`${intent}: not present in the registry response`);
    continue;
  }

  const champ = record.champion;
  const entries = (record.entries ?? [])
    .filter((e) => e.eval && e.registration_id !== champ?.registration_id)
    .sort((a, b) => String(b.registered_at ?? b.created_at ?? "").localeCompare(
      String(a.registered_at ?? a.created_at ?? "")));
  const newest = entries[0];

  const line = {
    polled_at: new Date().toISOString(),
    intent,
    champion_reg: champ?.registration_id ?? null,
    champion_stored_margin: champ?.eval?.candidate_margin ?? null,
    bar_last_measured: newest?.eval?.champion_margin ?? null,
    bar_measured_at: String(newest?.registered_at ?? newest?.created_at ?? "").slice(0, 16) || null,
    bar_measured_by_reg: newest?.registration_id ?? null,
    newest_challenger_margin: newest?.eval?.candidate_margin ?? null,
    comparable_cases: newest?.eval?.comparable_cases ?? null,
    entries: (record.entries ?? []).length,
  };

  console.log(
    `${intent}: bar ${line.bar_last_measured ?? "unknown"} ` +
    `(measured ${line.bar_measured_at ?? "-"} by reg ${line.bar_measured_by_reg ?? "-"}, ` +
    `${line.comparable_cases ?? "?"} cases) | champion reg ${line.champion_reg} ` +
    `stored margin ${line.champion_stored_margin}`,
  );
  appendFileSync(LOG, JSON.stringify(line) + "\n");
}
