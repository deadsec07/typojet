# typojet

`typojet` is a small open-source search engine written in Rust for local apps and websites. It aims for the first-release feel: typo-tolerant, prefix-friendly, schema-aware, easy to run, and small enough for one developer to understand in an afternoon.

## Why this exists

`typojet` is inspired by Meilisearch's instant-search developer experience, but the implementation is intentionally much smaller:

- JSON documents with unique string ids
- multiple named indexes
- searchable fields with per-field boosts
- inverted index with lowercase tokenization
- prefix search for autocomplete
- typo tolerance with Levenshtein distance `<= 1` or `<= 2`
- simple exact-match filters on scalar and array fields
- explicit filterable and sortable field settings
- simple ranking with exact-match boost, prefix boost, field boost, and typo penalty
- persistence to a local JSON snapshot
- single binary HTTP server

## Folder structure

```text
.
├── .gitignore
├── Cargo.toml
├── LICENSE
├── README.md
├── data
│   └── sample-documents.json
├── src
│   ├── api.rs
│   ├── engine.rs
│   ├── lib.rs
│   └── main.rs
└── tests
    └── api_tests.rs
```

## Run locally

```bash
cargo run
```

The server starts on `http://127.0.0.1:7700` and persists data to `./data/indexes.json`.

You can override the bind address and data directory:

```bash
cargo run -- --bind 0.0.0.0:8800 --data-dir ./tmp/typojet
```

Env vars work too:

```bash
TYPOJET_BIND=127.0.0.1:8800 TYPOJET_DATA_DIR=./tmp/typojet cargo run
```

Optional bearer auth:

```bash
cargo run -- --api-key dev-secret
```

or:

```bash
TYPOJET_API_KEY=dev-secret cargo run
```

When set, write and management routes require:

```bash
Authorization: Bearer dev-secret
```

`GET /health` remains open for basic liveness checks.

## API

The main API is index-scoped. The original `/index`, `/documents`, and `/search` routes still work as aliases for the built-in `default` index.

### `POST /indexes`

Create a named index.

```bash
curl -X POST http://127.0.0.1:7700/indexes \
  -H "content-type: application/json" \
  -d '{
    "name": "books",
    "searchable_fields": [
      { "name": "title", "boost": 3.0 },
      { "name": "tags", "boost": 2.0 },
      { "name": "description", "boost": 1.0 }
    ],
    "filterable_fields": ["tags", "category", "published"],
    "sortable_fields": ["title", "published_at"]
  }'
```

### `GET /indexes`

List all indexes.

```bash
curl http://127.0.0.1:7700/indexes
```

### `GET /indexes/:name`

Get one index summary.

```bash
curl http://127.0.0.1:7700/indexes/books
```

### `GET /indexes/:name/stats`

Get index stats and capabilities.

```bash
curl http://127.0.0.1:7700/indexes/books/stats
```

### `PUT /indexes/:name`

Update an index schema.

```bash
curl -X PUT http://127.0.0.1:7700/indexes/books \
  -H "content-type: application/json" \
  -d '{
    "searchable_fields": [
      { "name": "title", "boost": 3.0 },
      { "name": "tags", "boost": 2.0 },
      { "name": "description", "boost": 1.0 }
    ]
  }'
```

### `DELETE /indexes/:name`

Delete an index.

```bash
curl -X DELETE http://127.0.0.1:7700/indexes/books
```

### `POST /indexes/:name/documents`

Add or replace documents by id. The endpoint accepts either a raw JSON array or an object with a `documents` field.

```bash
curl -X POST http://127.0.0.1:7700/indexes/books/documents \
  -H "content-type: application/json" \
  -d '{
    "documents": [
      {
        "id": "doc-1",
        "title": "Rust Search Engine",
        "tags": ["rust", "search"],
        "description": "A tiny typo-tolerant search engine."
      },
      {
        "id": "doc-2",
        "title": "Autocomplete Patterns",
        "tags": ["ux", "search"],
        "description": "Prefix-friendly suggestions."
      }
    ]
  }'
```

### `GET /indexes/:name/documents`

List all documents in an index.

```bash
curl http://127.0.0.1:7700/indexes/books/documents
```

### `GET /indexes/:name/documents/:id`

Fetch one document by id.

```bash
curl http://127.0.0.1:7700/indexes/books/documents/book-1
```

### `PATCH /indexes/:name/documents/:id`

Partially update one document. The `id` field is preserved from the path.

```bash
curl -X PATCH http://127.0.0.1:7700/indexes/books/documents/book-1 \
  -H "content-type: application/json" \
  -d '{
    "description": "Updated instant search handbook"
  }'
```

### `DELETE /indexes/:name/documents/:id`

Delete one document by id.

```bash
curl -X DELETE http://127.0.0.1:7700/indexes/books/documents/book-1
```

### `GET /indexes/:name/search?q=`

Search across the configured fields.

```bash
curl "http://127.0.0.1:7700/indexes/books/search?q=rust"
curl "http://127.0.0.1:7700/indexes/books/search?q=autoc"
curl "http://127.0.0.1:7700/indexes/books/search?q=serch"
curl "http://127.0.0.1:7700/indexes/books/search?q=schema%20search"
curl "http://127.0.0.1:7700/indexes/books/search?q=guide&limit=10&offset=20"
```

Filters use `filter.<field>=value`:

```bash
curl "http://127.0.0.1:7700/indexes/books/search?q=rust&filter.tags=web"
curl "http://127.0.0.1:7700/indexes/books/search?q=guide&filter.category=lifestyle"
curl "http://127.0.0.1:7700/indexes/books/search?q=rust&filter.published=true"
```

Sorting uses `sort=<field>:asc|desc` on configured sortable fields:

```bash
curl "http://127.0.0.1:7700/indexes/books/search?q=rust&sort=title:asc"
curl "http://127.0.0.1:7700/indexes/books/search?q=rust&sort=published_at:desc"
```

Search responses include:

- `total`: total matching hits before pagination
- `offset`: current page offset
- `limit`: current page size
- `hits`: returned hits for this page
- `hits[].snippets`: lightweight matched field previews

Filtering and sorting are explicit:

- only fields listed in `filterable_fields` can be used with `filter.<field>=...`
- only fields listed in `sortable_fields` can be used with `sort=field:asc|desc`

### `GET /health`

```bash
curl http://127.0.0.1:7700/health
```

## Load the sample dataset

Start the server, then run:

```bash
curl -X POST http://127.0.0.1:7700/indexes \
  -H "content-type: application/json" \
  -d '{
    "name": "docs",
    "searchable_fields": [
      { "name": "title", "boost": 3.0 },
      { "name": "tags", "boost": 2.0 },
      { "name": "description", "boost": 1.0 }
    ],
    "filterable_fields": ["tags"],
    "sortable_fields": ["title"]
  }'

curl -X POST http://127.0.0.1:7700/indexes/docs/documents \
  -H "content-type: application/json" \
  -d @data/sample-documents.json
```

## Legacy default index

If you only want one index, the original shortcuts still work against `default`:

```bash
curl -X POST http://127.0.0.1:7700/index \
  -H "content-type: application/json" \
  -d '{
    "searchable_fields": [
      { "name": "title", "boost": 3.0 },
      { "name": "tags", "boost": 2.0 },
      { "name": "description", "boost": 1.0 }
    ]
  }'

curl -X POST http://127.0.0.1:7700/documents \
  -H "content-type: application/json" \
  -d @data/sample-documents.json

curl "http://127.0.0.1:7700/search?q=rust"
```


## Ranking model

Each query token contributes to a document score:

- exact token match gets the strongest base boost
- prefix match gets a smaller boost
- typo matches get a lower base score and a distance penalty
- field boost multiplies the contribution of `title`, `tags`, or any configured field

This is intentionally simple. The goal is instant search for small datasets, not exhaustive ranking science.

## Release Notes

- `v0.1.0`: single-binary local search for small apps and websites
- named indexes, document CRUD, prefix search, typo tolerance, filters, sorting, pagination, and snippets
- compatibility routes `/index`, `/documents`, and `/search` for the built-in `default` index

## Tests

```bash
cargo test
```

The test suite covers tokenization, typo distance, basic ranking behavior, persistence, and the REST API.

## Future work

- incremental indexing instead of full rebuilds
- faster prefix lookup with a trie or finite state structure
- richer filters and faceting
- document replacement batching without full index rebuild
- highlights and snippet generation
- configurable storage path and bind address

## Deferred non-goals

These are intentionally out of scope for the current project shape, but worth keeping on the long-term radar:

- distributed cluster support
- vector search
- bearer auth beyond the current single static token setup
- enterprise feature sets
- analytics dashboard

## Linux deployment

Build the binary:

```bash
git clone https://github.com/deadsec07/typojet
cd typojet
cargo build --release
```

Run it directly:

```bash
./target/release/typojet --bind 0.0.0.0:7700 --data-dir /var/lib/typojet
```

With optional auth:

```bash
TYPOJET_API_KEY=dev-secret ./target/release/typojet \
  --bind 0.0.0.0:7700 \
  --data-dir /var/lib/typojet
```

### systemd

A ready-to-edit service file is included at `deploy/systemd/typojet.service`.

Typical install flow:

```bash
sudo useradd --system --home /var/lib/typojet --shell /usr/sbin/nologin typojet
sudo mkdir -p /var/lib/typojet
sudo cp target/release/typojet /usr/local/bin/typojet
sudo cp deploy/systemd/typojet.service /etc/systemd/system/typojet.service
sudo systemctl daemon-reload
sudo systemctl enable --now typojet
```

### Docker

A `Dockerfile` is included for containerized Linux deployment.

Build the image:

```bash
docker build -t typojet .
```

Run it:

```bash
docker run --rm -p 7700:7700 \
  -e TYPOJET_API_KEY=dev-secret \
  typojet
```

Mount a host directory to `/var/lib/typojet` if you want persistent index data.

## Release automation

The repo now includes:

- `.github/workflows/release.yml` to build and attach Linux `x86_64` binaries on tags like `v0.1.0`
- `.github/workflows/pages.yml` to deploy the marketing site from `site/` to GitHub Pages on pushes to `main`

## Deployment assets

The repo now contains these deploy-oriented files:

- `Dockerfile`
- `deploy/systemd/typojet.service`
- `.github/workflows/release.yml`
- `.github/workflows/pages.yml`
- `site/index.html`
- `site/styles.css`
