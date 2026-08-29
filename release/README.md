# Track 2 release identity

`text-authenticity.json` is the machine-readable identity of the frozen Track 2 artifact. In the
development monorepo the binary is generated under `scorer/dist/`; in the standalone public
repository the exact verified bytes are tracked at `dist/text_authenticity.wasm`.

Build and verify from `track2/scorer` in the monorepo, or from the repository root in the
standalone release:

```powershell
cargo +1.98.0 build --release --target wasm32-unknown-unknown --no-default-features --features text-authenticity
Get-FileHash -Algorithm SHA256 target/wasm32-unknown-unknown/release/scorer.wasm
openssl dgst -keccak-256 target/wasm32-unknown-unknown/release/scorer.wasm
node verify.mjs target/wasm32-unknown-unknown/release/scorer.wasm
```

The build is releasable only when its size and both hashes equal the manifest. After publishing
the exact WASM in the standalone public repository, download the raw URL and repeat both hashes.
Then fill `publication.hosted_url` and `publication.source_commit`, set
`publication.hosted_bytes_verified` to `true`, and only then ask the user to register it.

The tracked `.cargo/config.toml` normalizes Rust's embedded `src\...` Windows span paths to
`src/...`. Do not remove it: without that flag Windows and Linux produce behaviorally identical
but byte-different modules, invalidating the frozen hash and CI reproduction check.

Source changes after registration are harmless, but they do not change the registered scorer.
Any changed WASM requires a new hash and a fresh `registerWasm` transaction.

The website boundary is separate: edit the intent/miner details there only when that Track 1
metadata changes. README, tests, fixtures, harnesses, and source-only GitHub changes require no
website update. If those source changes produce different Track 2 WASM bytes, publish the new
artifact, verify its hashes, and call `registerWasm` again.

Anyone can run the release's focused public proxy without an incumbent or network call:

```bash
node harness/check-tac.mjs dist/text_authenticity.wasm
node harness/check-tac.mjs dist/text_authenticity.wasm --json
node release/probe-negation.mjs dist/text_authenticity.wasm
node release/probe-model-aliases.mjs dist/text_authenticity.wasm
node release/probe-authenticity-axes.mjs dist/text_authenticity.wasm
node release/probe-authenticity-vocabulary.mjs dist/text_authenticity.wasm
```

A pass covers 256 public semantic orderings and 16 equivalent-answer constraints; it is not a
claim about Telegraph's hidden rotating fixtures. The separate probes cover negation, model-name
normalization, independent semantic axes, and ordinary authenticity vocabulary; all remain outside
the benchmark totals so they cannot inflate the reported 256-pair corpus.
