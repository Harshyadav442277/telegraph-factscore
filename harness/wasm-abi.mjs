#!/usr/bin/env node

/**
 * Telegraph scoring-module ABI loader.
 *
 * Mirrors the node's call path exactly, as reproduced by
 * track1-miner/docs/codex-worklog/probe-champion.mjs:
 *   WebAssembly.instantiate(bytes, {})   -- freestanding, no imports
 *   exports: memory, alloc, rank_answer  (dealloc is required by the node, unused here)
 *   UTF-8 encode -> alloc(len) -> write at pointer -> rank_answer(q,qlen,gt,gtlen,ma,malen) -> f32
 *
 * The one thing this adds over the probe: the modules use a fixed-size bump
 * allocator whose offset WRAPS at the heap limit. Scoring a whole corpus in one
 * process would silently overwrite live buffers, so the instance is rebuilt
 * before the heap can wrap and pointer monotonicity is asserted every call.
 * Re-instantiating a compiled 24 MB module costs ~5 ms.
 */

import { readFile } from "node:fs/promises";
import { createHash } from "node:crypto";

const ENCODER = new TextEncoder();
const INITIAL_BUDGET = 512 * 1024; // conservative: docs example heap is 1 MB, champion 4 MB
const MIN_BUDGET = 8 * 1024;

export function utf8(text) {
  return ENCODER.encode(text);
}

/** Raw bytes that are deliberately not valid UTF-8 (adversarial Stage-1 input). */
export function rawBytes(values) {
  return Uint8Array.from(values);
}

class Scorer {
  constructor(label, path, module, sha256, sizeBytes) {
    this.label = label;
    this.path = path;
    this.sha256 = sha256;
    this.sizeBytes = sizeBytes;
    this.calls = 0;
    this.resets = 0;
    this._module = module;
    this._budget = INITIAL_BUDGET;
    this._used = 0;
    this._instance = null;
  }

  _instantiate() {
    this._instance = new WebAssembly.Instance(this._module, {});
    const { memory, alloc, rank_answer: rankAnswer } = this._instance.exports;
    if (!(memory instanceof WebAssembly.Memory) || typeof alloc !== "function" || typeof rankAnswer !== "function") {
      throw new Error(
        `${this.label}: WASM does not expose the Telegraph memory/alloc/rank_answer ABI ` +
          `(exports: ${Object.keys(this._instance.exports).join(", ") || "none"})`,
      );
    }
    this._used = 0;
    this.resets += 1;
  }

  /** Names of the module's exports, for Stage-1 structural reporting. */
  exportNames() {
    if (!this._instance) this._instantiate();
    return Object.keys(this._instance.exports).sort();
  }

  /** Declared parameter counts; V8 reflects WASM arity as Function.length. */
  arity() {
    if (!this._instance) this._instantiate();
    const { alloc, dealloc, rank_answer: rankAnswer } = this._instance.exports;
    return {
      rank_answer: typeof rankAnswer === "function" ? rankAnswer.length : null,
      alloc: typeof alloc === "function" ? alloc.length : null,
      dealloc: typeof dealloc === "function" ? dealloc.length : null,
    };
  }

  _put(bytes) {
    // The host passes ptr=0, len=0 for an empty string and never calls alloc for
    // it. Mirror that exactly: a module may only look at len, and calling alloc(0)
    // here would drift from the node's real call.
    if (bytes.length === 0) return [0, 0];
    const { memory, alloc } = this._instance.exports;
    const pointer = Number(alloc(bytes.length));
    new Uint8Array(memory.buffer, pointer, bytes.length).set(bytes);
    this._used += bytes.length + 8;
    return [pointer, bytes.length];
  }

  _attempt(qBytes, gtBytes, aBytes) {
    const q = this._put(qBytes);
    const gt = this._put(gtBytes);
    const a = this._put(aBytes);
    // A wrapped bump allocator hands back a lower pointer; buffers written
    // earlier in this same call would then be corrupt. Detect and retry.
    // Zero-length buffers sit at pointer 0 and are exempt.
    const live = [q, gt, a].filter(([, len]) => len > 0);
    for (let i = 1; i < live.length; i += 1) {
      if (live[i - 1][0] + live[i - 1][1] > live[i][0]) return null;
    }
    const { rank_answer: rankAnswer } = this._instance.exports;
    return Number(rankAnswer(...q, ...gt, ...a));
  }

  _attemptBreakdown(qBytes, gtBytes, aBytes) {
    const q = this._put(qBytes);
    const gt = this._put(gtBytes);
    const a = this._put(aBytes);
    const { memory, alloc, breakdown_answer: breakdownAnswer } = this._instance.exports;
    if (typeof breakdownAnswer !== "function") {
      throw new Error(`${this.label}: WASM does not expose breakdown_answer`);
    }
    const out = [Number(alloc(5 * Float32Array.BYTES_PER_ELEMENT)), 5 * Float32Array.BYTES_PER_ELEMENT];
    const live = [q, gt, a, out].filter(([, len]) => len > 0);
    for (let i = 1; i < live.length; i += 1) {
      if (live[i - 1][0] + live[i - 1][1] > live[i][0]) return null;
    }
    if (Number(breakdownAnswer(...q, ...gt, ...a, out[0])) !== 5) {
      throw new Error(`${this.label}: breakdown_answer did not write five values`);
    }
    const view = new DataView(memory.buffer, out[0], out[1]);
    const values = Array.from({ length: 5 }, (_, index) => view.getFloat32(index * 4, true));
    return Object.fromEntries(["precision", "fact", "answered", "raw", "score"].map((key, index) => [key, values[index]]));
  }

  scoreBytes(qBytes, gtBytes, aBytes) {
    const needed = qBytes.length + gtBytes.length + aBytes.length + 32;
    if (!this._instance || this._used + needed > this._budget) this._instantiate();
    let value = this._attempt(qBytes, gtBytes, aBytes);
    if (value === null) {
      this._budget = Math.max(MIN_BUDGET, Math.floor(this._used / 2));
      this._instantiate();
      value = this._attempt(qBytes, gtBytes, aBytes);
      if (value === null) throw new Error(`${this.label}: allocator wrapped even on a fresh instance`);
    }
    this.calls += 1;
    return value;
  }

  score(question, groundTruth, answer) {
    return this.scoreBytes(utf8(question), utf8(groundTruth), utf8(answer));
  }

  breakdown(question, groundTruth, answer) {
    const inputs = [question, groundTruth, answer].map(utf8);
    const needed = inputs.reduce((sum, bytes) => sum + bytes.length, 0) + 64;
    if (!this._instance || this._used + needed > this._budget) this._instantiate();
    let result = this._attemptBreakdown(...inputs);
    if (result === null) {
      this._instantiate();
      result = this._attemptBreakdown(...inputs);
      if (result === null) throw new Error(`${this.label}: allocator wrapped even on a fresh instance`);
    }
    return result;
  }
}

export async function loadScorer(path, label) {
  let bytes;
  try {
    bytes = await readFile(path);
  } catch (error) {
    throw new Error(`Cannot read scorer WASM at ${path}: ${error.message}`);
  }
  let module;
  try {
    module = await WebAssembly.compile(bytes);
  } catch (error) {
    throw new Error(`${path} is not a loadable WASM module: ${error.message}`);
  }
  const sha256 = createHash("sha256").update(bytes).digest("hex");
  const scorer = new Scorer(label ?? path, path, module, sha256, bytes.length);
  scorer.exportNames(); // instantiate once now so a bad ABI fails loudly and early
  return scorer;
}
