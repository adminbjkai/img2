# img2

Minimal Rust image sharing service with clipboard paste, auto-delete, QR codes, and SQLite storage.

## Features
- Paste or upload images from the web UI
- Short links with thumbnails
- Optional auto-delete
- SQLite-backed metadata store
- QR code generation for easy sharing

## Build

```bash
cargo build --release
```

## Run

```bash
./run.sh
```

Environment overrides:
- `HOST` (default: 127.0.0.1)
- `PORT` (default: 8097)
- `UPLOAD_DIR` (default: ./uploads)
- `DB_PATH` (default: ./data/images.db)

## Endpoints
- `/` UI
- `/upload` (POST multipart)
- `/i/:id` image
- `/thumb/:id` thumbnail
- `/health`

## Nginx
See `nginx-img2.bjk.ai.conf` for reverse proxy config.

## GitHub Push Notes
This repo excludes runtime data and build outputs via `.gitignore`:
- `target/`
- `data/`
- `uploads/`
- `*.db`
- `.env`
