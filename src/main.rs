use axum::{
    extract::{Multipart, Path, State},
    http::{HeaderMap, HeaderValue, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use base64::engine::general_purpose::STANDARD as base64_engine;
use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use image::{DynamicImage, ImageFormat};
use rand::RngCore;
use rusqlite::{params, Connection};
use serde::Serialize;
use std::{
    collections::HashMap,
    io::Cursor,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::{fs, time};

const MAX_CONTENT_LENGTH: usize = 50 * 1024 * 1024;

#[derive(Clone)]
struct AppState {
    db: Arc<Mutex<Connection>>,
    upload_dir: PathBuf,
}

#[derive(Serialize)]
struct UploadResponse {
    success: bool,
    id: String,
    url: String,
    qr_code: String,
    filename: String,
    size: u64,
    delete_at: Option<String>,
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

#[tokio::main]
async fn main() {
    let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(8127);
    let upload_dir = PathBuf::from(std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".to_string()));
    let db_path = PathBuf::from(std::env::var("DB_PATH").unwrap_or_else(|_| "./data/images.db".to_string()));

    if let Some(parent) = upload_dir.parent() {
        let _ = fs::create_dir_all(parent).await;
    }
    let _ = fs::create_dir_all(&upload_dir).await;
    if let Some(parent) = db_path.parent() {
        let _ = fs::create_dir_all(parent).await;
    }

    let conn = Connection::open(&db_path).expect("open db");
    conn.execute(
        "CREATE TABLE IF NOT EXISTS images (
            id TEXT PRIMARY KEY,
            filename TEXT NOT NULL,
            original_name TEXT NOT NULL,
            upload_time TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            delete_at INTEGER,
            file_size INTEGER,
            mime_type TEXT
        )",
        [],
    )
    .expect("create table");

    let state = AppState {
        db: Arc::new(Mutex::new(conn)),
        upload_dir: upload_dir.clone(),
    };

    spawn_cleanup(state.clone());

    let app = Router::new()
        .route("/", get(index))
        .route("/upload", post(upload))
        .route("/i/:id", get(serve_image))
        .route("/thumb/:id", get(serve_thumbnail))
        .route("/health", get(health))
        .with_state(state)
        .layer(axum::extract::DefaultBodyLimit::max(MAX_CONTENT_LENGTH));

    let addr = format!("{}:{}", host, port);
    println!("img2 listening on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

async fn index() -> impl IntoResponse {
    Html(include_str!("../templates/index.html"))
}

async fn upload(
    State(state): State<AppState>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename: Option<String> = None;
    let mut content_type: Option<String> = None;
    let mut delete_after: String = "0".to_string();

    while let Ok(Some(field)) = multipart.next_field().await {
        let name = field.name().unwrap_or("").to_string();
        if name == "delete_after" {
            if let Ok(text) = field.text().await {
                delete_after = text;
            }
            continue;
        }
        if name == "file" {
            filename = field.file_name().map(|v| v.to_string());
            content_type = field.content_type().map(|v| v.to_string());
            if let Ok(bytes) = field.bytes().await {
                if bytes.len() > MAX_CONTENT_LENGTH {
                    return json_error(StatusCode::PAYLOAD_TOO_LARGE, "File too large");
                }
                file_bytes = Some(bytes.to_vec());
            }
        }
    }

    let bytes = match file_bytes {
        Some(b) => b,
        None => return json_error(StatusCode::BAD_REQUEST, "No file provided"),
    };

    let original_name = filename.clone().unwrap_or_else(|| "clipboard".to_string());
    let ext = determine_extension(&filename, &content_type)
        .ok_or_else(|| json_error(StatusCode::BAD_REQUEST, "File type not allowed"));
    let ext = match ext {
        Ok(v) => v,
        Err(resp) => return resp,
    };

    let image_id = generate_id();
    let stored_name = format!("{}.{}", image_id, ext);
    let filepath = state.upload_dir.join(&stored_name);

    if let Err(_) = fs::write(&filepath, &bytes).await {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, "Failed to save file");
    }

    let file_size = bytes.len() as u64;
    let mime_type = content_type.unwrap_or_else(|| format!("image/{}", ext));

    let delete_at = parse_delete_after(&delete_after).map(|dt| dt.timestamp());
    if let Ok(mut conn) = state.db.lock() {
        let _ = conn.execute(
            "INSERT INTO images (id, filename, original_name, delete_at, file_size, mime_type) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![image_id, stored_name, original_name, delete_at, file_size, mime_type],
        );
    }

    let base_url = base_url_from_headers(&headers);
    let image_url = format!("{}/i/{}", base_url, image_id);
    let qr_code = generate_qr_code(&image_url);

    let delete_at_iso = delete_at.map(|ts| {
        DateTime::<Utc>::from_utc(
            chrono::NaiveDateTime::from_timestamp_opt(ts, 0).unwrap_or_default(),
            Utc,
        )
        .to_rfc3339()
    });

    Json(UploadResponse {
        success: true,
        id: image_id,
        url: image_url,
        qr_code,
        filename: filename.unwrap_or_else(|| "clipboard".to_string()),
        size: file_size,
        delete_at: delete_at_iso,
    })
    .into_response()
}

async fn serve_image(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let row = fetch_image_row(&state, &id);
    let (filename, mime_type) = match row {
        Some(v) => v,
        None => return json_error(StatusCode::NOT_FOUND, "Image not found"),
    };
    let filepath = state.upload_dir.join(filename);
    match fs::read(&filepath).await {
        Ok(bytes) => {
            let mut headers = HeaderMap::new();
            if let Ok(val) = HeaderValue::from_str(&mime_type) {
                headers.insert(axum::http::header::CONTENT_TYPE, val);
            }
            (StatusCode::OK, headers, bytes).into_response()
        }
        Err(_) => json_error(StatusCode::NOT_FOUND, "Image not found"),
    }
}

async fn serve_thumbnail(State(state): State<AppState>, Path(id): Path<String>) -> impl IntoResponse {
    let row = fetch_image_row(&state, &id);
    let (filename, _mime_type) = match row {
        Some(v) => v,
        None => return json_error(StatusCode::NOT_FOUND, "Image not found"),
    };
    let filepath = state.upload_dir.join(filename);
    let bytes = match fs::read(&filepath).await {
        Ok(b) => b,
        Err(_) => return json_error(StatusCode::NOT_FOUND, "Image not found"),
    };

    let img = match image::load_from_memory(&bytes) {
        Ok(i) => i,
        Err(_) => return json_error(StatusCode::BAD_REQUEST, "Invalid image"),
    };
    let thumb = img.thumbnail(300, 300);
    let mut buffer = Cursor::new(Vec::new());
    let _ = thumb.write_to(&mut buffer, ImageFormat::Png);

    let mut headers = HeaderMap::new();
    headers.insert(axum::http::header::CONTENT_TYPE, HeaderValue::from_static("image/png"));
    (StatusCode::OK, headers, buffer.into_inner()).into_response()
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({"status": "ok", "service": "img2"}))
}

fn fetch_image_row(state: &AppState, id: &str) -> Option<(String, String)> {
    let conn = state.db.lock().ok()?;
    let mut stmt = conn
        .prepare("SELECT filename, mime_type FROM images WHERE id = ?1")
        .ok()?;
    let mut rows = stmt.query(params![id]).ok()?;
    let row = rows.next().ok()?;
    let row = row?;
    let filename: String = row.get(0).ok()?;
    let mime_type: String = row.get(1).ok()?;
    Some((filename, mime_type))
}

fn determine_extension(filename: &Option<String>, content_type: &Option<String>) -> Option<String> {
    let mut allowed = HashMap::new();
    allowed.insert("png", "image/png");
    allowed.insert("jpg", "image/jpeg");
    allowed.insert("jpeg", "image/jpeg");
    allowed.insert("gif", "image/gif");
    allowed.insert("webp", "image/webp");
    allowed.insert("bmp", "image/bmp");

    if let Some(name) = filename {
        if let Some(ext) = name.rsplit('.').next() {
            let ext_lc = ext.to_lowercase();
            if allowed.contains_key(ext_lc.as_str()) {
                return Some(ext_lc);
            }
        }
    }
    if let Some(ct) = content_type {
        for (ext, mime) in allowed.iter() {
            if ct == mime {
                return Some(ext.to_string());
            }
        }
    }
    None
}

fn generate_id() -> String {
    let mut bytes = [0u8; 6];
    rand::thread_rng().fill_bytes(&mut bytes);
    hex::encode(bytes)
}

fn parse_delete_after(input: &str) -> Option<DateTime<Utc>> {
    let minutes: i64 = match input {
        "0" => return None,
        "5" => 5,
        "15" => 15,
        "30" => 30,
        "60" => 60,
        "180" => 180,
        "360" => 360,
        "720" => 720,
        "1440" => 1440,
        "2880" => 2880,
        "7200" => 7200,
        "10080" => 10080,
        "14400" => 14400,
        "43200" => 43200,
        "129600" => 129600,
        _ => return None,
    };
    Some(Utc::now() + Duration::minutes(minutes))
}

fn generate_qr_code(url: &str) -> String {
    let code = qrcode::QrCode::new(url.as_bytes()).unwrap();
    let svg = code.render::<qrcode::render::svg::Color>().build();
    let encoded = base64_engine.encode(svg.as_bytes());
    format!("data:image/svg+xml;base64,{}", encoded)
}

fn base_url_from_headers(headers: &HeaderMap) -> String {
    let proto = headers
        .get("x-forwarded-proto")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("http");
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("localhost");
    format!("{}://{}", proto, host)
}

fn json_error(status: StatusCode, msg: &str) -> axum::response::Response {
    let body = Json(ErrorResponse {
        error: msg.to_string(),
    });
    (status, body).into_response()
}

fn spawn_cleanup(state: AppState) {
    tokio::spawn(async move {
        let mut interval = time::interval(time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let now = Utc::now().timestamp();
            let rows = {
                let conn = match state.db.lock() {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                let mut stmt = match conn.prepare(
                    "SELECT id, filename FROM images WHERE delete_at IS NOT NULL AND delete_at <= ?1",
                ) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                let mut rows = match stmt.query(params![now]) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let mut expired = Vec::new();
                while let Ok(Some(row)) = rows.next() {
                    let id: String = row.get(0).unwrap_or_default();
                    let filename: String = row.get(1).unwrap_or_default();
                    expired.push((id, filename));
                }
                expired
            };

            for (id, filename) in rows {
                let path = state.upload_dir.join(&filename);
                let _ = fs::remove_file(&path).await;
                if let Ok(conn) = state.db.lock() {
                    let _ = conn.execute("DELETE FROM images WHERE id = ?1", params![id]);
                }
            }
        }
    });
}
