# Examples

Everything here is fictional: the retailer, the people and the figures are invented, and nothing corresponds to a real interview.

| File | Use |
|---|---|
| `library/INT-01_Example_Retailer/02_transcript_cleaned_diarised.md` | One cleaned transcript in the format the tool reads: turns written as `**INT `[mm:ss]`:**` and `**RESP `[mm:ss]`:**`, with a front matter block |
| `codebook.csv` | Ten codes with definitions, TOE attachment, origin and research questions, in the CSV format of the codebook import |
| `project_template.example.json` | The configuration of a fictional library study as a project template: three research questions (Q1 to Q3), four interview attributes, a second memo template, two matrix dimensions, the default lists. Import it with `POST /api/project/template/import` (body `{"template": <file>, "dry_run": true}` first to see the report). It holds no interview data |
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
