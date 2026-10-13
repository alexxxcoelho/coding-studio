# Examples

Everything here is fictional: the retailer, the people and the figures are invented, and nothing corresponds to a real interview.

| File | Use |
|---|---|
| `library/INT-01_Example_Retailer/02_transcript_cleaned_diarised.md` | One cleaned transcript in the format the tool reads: turns written as `**INT `[mm:ss]`:**` and `**RESP `[mm:ss]`:**`, with a front matter block |
| `codebook.csv` | Ten codes with definitions, TOE attachment, origin and research questions, in the CSV format of the codebook import |
| `providers.example.json` | One entry of each provider kind (CLI, OpenAI-compatible, ChatGPT sign-in). It contains no secrets: keys are named by environment variable, and the paths are placeholders |

## Try the library

From the root of the repository:

```bash
cargo run --release -p cs-app -- --library examples/library --db /tmp/example.db
```

The browser opens with `INT-01` in the library. Segmenting it gives 28 units. To load the codebook, send:

```bash
curl -X POST http://127.0.0.1:7878/api/codebook/import \
  -H 'content-type: application/json' \
  -d '{"kind": "csv", "path": "examples/codebook.csv"}'
```

Keep experiments in a throw-away database, as above, so that the example stays clean.
