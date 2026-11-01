# SANCTIONS_SCREENING_MATCH evaluator adapter

A LoRA delta (rank 8, alpha 32, Wqkv/Wo/Wi, one-number reward head) for Telegraph's shared Foundation Model
`modernbert-base-lora16-v1`, class `judgment`, max_tokens 256.

| | |
|---|---|
| Package | `SANCTIONS_SCREENING_MATCH.safetensors`, 6,776,916 bytes |
| keccak256 | `0x49de41078f6bb39ab183a715263c648543945565b4ce97c9a9214de59b09f2f0` |
| sha256 | `547a3f05fcd9f0e49bcbd0685db65f6b099bb34cdb84d8a05d31efe308e5b3e6` |
| Foundation | sha256 `0a49b325…f8e6`, tokenizer `67bc76f8…c456` |
| Checked | Stage 1 and a foundation load with the adapter kit (`telegraph-wasm-baseline` `1ed063e`), class taken from the live node |

## Training data

31,176 preference pairs built from the OFAC SDN list (US Treasury, public domain; list of 2026-10-27). A question
names a person, company, vessel or aircraft: a listed name (primary, a.k.a., or a one-letter misspelling) or an
unlisted name recombined from listed names' own parts and checked against every listed name and alias. The worse
answer gives the opposite verdict, matches an unrelated listed entry, contradicts its own score, screens a
different name, or declines. Answers come in five shapes (prose, the intent's response JSON, thin JSON, a bare
`{"matched": …}`, a one-line verdict), and half the pairs mix two shapes, so a short right answer has to beat a
detailed wrong one. Split by SDN entity: test entities never appear in training. Kit dataset gate: length 52.1%,
surface probe 57.3%. No evaluation-pool rows were used.

## Results

| Measurement | Result |
|---|---|
| Kit test gate, 3,896 pairs, held-out entities | 92.2% (length baseline 52.0%) |
| Node runtime (`adapterexec`), 200 held-out pairs, 0 ties | 93.0% (flipped verdict 86.7%, the other kinds 97.5–100%) |
| Out-of-template probe, 80 pairs on 16 real names (8 listed, 8 not) in shapes the builder never wrote | 72.5% |

**Limitations.**
- The adapter does not hold the SDN list. On a bare verdict about a name, it can only judge whether the
  name looks listed.
- In this build, unlisted names had fewer words than listed ones, and name shape alone predicted the label 71.7%
  of the time. Part of the held-out score comes from that, not from judgement. A later build removes the cue.
- In the probe, an unlisted name's detailed "no entries found" answer lost to a bare "Match" in 8 of 8 pairs.
