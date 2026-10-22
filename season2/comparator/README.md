# Comparator module for deterministic intents

This is the Season II evaluator for deterministic intents (prices, rates, counts). The node reads a value from a
miner's answer and from the epoch's reference, and asks this module how close they are. The module returns
basis points from 0 to 10000. The network fixes the rule, and the docs say "there is no 'better' comparator to
build". A comparator is promoted only if it reproduces that rule exactly. What matters here is correctness,
proved.

## What it is

- `src/lib.rs` is `#![no_std]` Rust, about 70 lines, with no imports.
- The release build is 2,182 bytes.
- It exports `imw_score_deterministic(truth, miner, spec_ptr) -> i64`, `imw_alloc` and `imw_free`.
- It is a line-for-line port of the network's answer key, `ReferenceScore` in telegraph-wasm-baseline
  `pkg/scoring/comparator/reference.go`. That includes the awkward parts:
  - the code is truncated to i32, and an unknown code counts as Exact;
  - subtraction and abs wrap;
  - multiplication saturates;
  - RelTol is computed in 128 bits, and an out-of-range quotient becomes `i64::MAX`.

## Proof

```bash
cargo build --release --target wasm32-unknown-unknown
cd verify && go run . <telegraph_comparator.wasm>
```

`verify/` pins the kit at `992f2650` and loads the module with the node's own runtime,
`runtime.NewDeterministicInterruptible`, which is what the comparator gate uses. It scores every case and
compares the result with `comparator.ReferenceScore`.

The cases follow the published gate:

- all four codes plus unknown and i32-truncated ones;
- tolerances from −5 to `i64::MAX`, including 0;
- truths from 1 to 1e18, and their negatives;
- the miner at an exact match, one unit off, the half-score point, the window edge and past it, on both sides;
- plus 200,000 seeded random cases.

The whole run is repeated, and any difference between the two runs fails.

**Result, 2026-10-22:**

| module | cases | mismatches | max deviation | nondeterministic |
|---|---|---|---|---|
| this module | 234,848 | 0 | 0 bp | 0 |
| a mutant computing `diff * 10000` with i64 saturation, the trap the docs warn about | 234,848 | 8,135 | 9,996 bp | 0 |

The gate tolerates ±10 bp. This check tolerates none. The mutant run shows the check can fail.

## One module, 31 intents

The node refuses the same bytes twice from one address. So `go run . pack <base.wasm> ../dist <INTENTS>` writes one
copy per intent. Each copy adds a `telegraph.adapter` section, `{"abi_version":1,"class":"comparator"}`, and names
the intent in its informational `tool` field. The pack step reads the section back with the node's
`ReadAdapterMeta` and `Check("comparator")`. The arithmetic is byte-identical across copies. Three copies were
re-verified on the full grid, with 0 mismatches.

`dist/manifest.json` lists each intent, file, size and keccak256. Registration hashes with keccak256, not
SHA-256.

The 31 intents are the deterministic intents that the Season II missions buy:

- The first 9 in the manifest are the ones the kit itself wires to the comparator path. They are the surest.
- The other 22 are tiered "A — Deterministic" in `docs/using/intents.md` but are not in the kit's compiled
  map. Whether the node routes them to the comparator path is unverified.

## Registering (operator only)

**Not yet.** On 2026-10-21 at 19:16 UTC devnode still ran the Season I gate, which would reject a module without
`rank_answer`. Register once `/api/evalpool/status` answers.

1. Host `dist/` at a commit-pinned public URL. Hash the hosted bytes again after upload.
2. Call `registerWasm(<keccak256>, <url>, "<INTENT>", <fee address>)` on the Diamond
   `0x5a2324aA18613FAD4e44bDF0d6c73Ec1f6D87ff8` (Base Sepolia).
3. Check each registration with `/engine/validator/v1/wasm/<id>`.

A comparator that passes replaces the previous one, and a later passing one replaces ours. These slots show
correctness and reach. They are not a contest.
