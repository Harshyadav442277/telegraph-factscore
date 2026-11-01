# EVENT_OUTCOME_RESOLUTION evaluator adapter

A LoRA delta (rank 8, alpha 32, Wqkv/Wo/Wi, one-number reward head) for Telegraph's shared Foundation Model
`modernbert-base-lora16-v1`, class `judgment`, max_tokens 256.

| | |
|---|---|
| Package | `EVENT_OUTCOME_RESOLUTION.safetensors`, 6,776,916 bytes |
| keccak256 | `0xc23ad1c6268d27f1a33de975c39b547844fef14e95868b5158f3e020e5b5e9e0` |
| Foundation | sha256 `0a49b325…f8e6`, tokenizer `67bc76f8…c456` |
| Checked | Stage 1 and a foundation load with the adapter kit (`telegraph-wasm-baseline` `1ed063e`), class taken from the live node |

## Training data

19,291 preference pairs built from Wikidata (CC0): settled contests and elections from 1990 to 2025 with at
least 3 Wikipedia sitelinks. Every row cites its Wikidata item. A worse answer is one of: the wrong winner, a
YES/NO market resolved the wrong way, a YES that names someone else as the winner, "unresolved" for a settled
event, a prediction instead of a resolution, or the resolution of a different event in the same series. Both
answers of a pair use the same template, as prose or as the intent's response JSON, so wording and length carry
no signal (kit dataset gate: surface probe 58.2%, length 50.2%). No evaluation-pool rows were used.

## Results, on event series held out of training

| Measurement | Result |
|---|---|
| Kit test gate, 2,566 pairs | 74.7% (length baseline 50.7%) |
| Node runtime (`adapterexec`), 200 pairs, 0 ties | 75.5% |
| — contradiction / prediction / unresolved / wrong event | 100% (103 pairs) |
| — YES/NO flipped | 51.9% (52 pairs) |
| — wrong winner | 46.7% (45 pairs) |

**Limitation.** The adapter checks that an answer resolves the event that was asked about, decisively and
consistently. It does not know who won an event it has not seen: on factual flips it is at chance.
