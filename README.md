# typojet

`typojet` is a Rust search engine by A A Hasnat for local apps and websites. It is designed to feel like a compact first-release search product: typo-tolerant, prefix-friendly, schema-aware, easy to run, and small enough for one developer to understand in an afternoon.

Links:
- Live site: https://deadsec07.github.io/typojet/
- GitHub: https://github.com/deadsec07/typojet
- Main site: https://hnetechnologies.com/
- Creator profile: https://deadsec07.github.io/

## Why this exists

`typojet` is inspired by Meilisearch's instant-search developer experience, but the implementation is intentionally much smaller:

- JSON documents with unique string ids
- Multiple named indexes
- Searchable fields with per-field boosts
- Inverted index with lowercase tokenization
- Prefix search for autocomplete
- Typo tolerance with Levenshtein distance `<= 1` or `<= 2`
- Simple exact-match filters on scalar and array fields
- Explicit filterable and sortable field settings
- Ranking with exact-match boost, prefix boost, field boost, and typo penalty
- Persistence to a local JSON snapshot
- Single-binary HTTP server

## Run locally

```bash
cargo run
```

The server starts on `http://127.0.0.1:7700` and persists data to `./data/indexes.json`.

You can override the bind address and data directory:

```bash
cargo run -- --bind 0.0.0.0:8800 --data-dir ./tmp/typojet
```

Optional bearer auth:

```bash
cargo run -- --api-key dev-secret
```

## API

The main API is index-scoped. The original `/index`, `/documents`, and `/search` routes still work as aliases for the built-in `default` index.

### `POST /indexes`

Create a named index.

### `GET /indexes`

List all indexes.

### `GET /indexes/:name/stats`

Get index stats and capabilities.

### `POST /indexes/:name/documents`

Add or replace documents by id.

### `GET /indexes/:name/search?q=`

Search across the configured fields.

See the repository examples and live site for the full request and response samples.
