use crate::database::DbManager;
use shared::DownloadProgressPayload;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tauri::{AppHandle, Emitter, Manager};
use tokio::fs::{File, OpenOptions};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::{timeout, Duration as TokioDuration};

// ============================================================================
// GLOBAL FLAGS — lookup theo task_id để pause/resume/cancel
// ============================================================================

struct DownloadFlags {
    paused: Arc<AtomicBool>,
    cancelled: Arc<AtomicBool>,
}

static DOWNLOAD_FLAGS: OnceLock<Mutex<HashMap<String, DownloadFlags>>> = OnceLock::new();

fn get_flags() -> &'static Mutex<HashMap<String, DownloadFlags>> {
    DOWNLOAD_FLAGS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn set_paused(task_id: &str, paused: bool) -> Result<(), String> {
    let map = get_flags().lock().map_err(|_| "flag map poisoned")?;
    let Some(f) = map.get(task_id) else {
        return Err(format!("Unknown download: {}", task_id));
    };
    f.paused.store(paused, Ordering::Release);
    log::info!("[download] task {} paused = {}", task_id, paused);
    Ok(())
}

pub fn cancel(task_id: &str) -> Result<(), String> {
    let map = get_flags().lock().map_err(|_| "flag map poisoned")?;
    let Some(f) = map.get(task_id) else {
        return Err(format!("Unknown download: {}", task_id));
    };
    f.cancelled.store(true, Ordering::Release);
    log::info!("[download] task {} cancelled", task_id);
    Ok(())
}

pub fn unregister(task_id: &str) {
    if let Ok(mut map) = get_flags().lock() {
        map.remove(task_id);
    }
}

// ============================================================================
// ENGINE
// ============================================================================

pub struct DownloadEngine;

impl DownloadEngine {
    fn sanitize_filename(name: &str) -> String {
        let clean = name
            .replace(['/', '\\', ':', '*', '?', '"', '<', '>', '|'], "_")
            .trim_matches(['.', ' '])
            .to_string();

        if clean.is_empty() {
            format!(
                "download_{}.bin",
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis()
            )
        } else {
            clean
        }
    }

    pub async fn start_download(
        app: AppHandle,
        url: String,
        save_dir: PathBuf,
        custom_name: Option<String>,
        connections: usize,
    ) -> Result<String, String> {
        let task_id = format!(
            "dl_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis()
        );

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .map_err(|e| e.to_string())?;

        let head_resp = client
            .head(&url)
            .send()
            .await
            .map_err(|e| format!("HEAD error: {}", e))?;

        let total_size = head_resp
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);

        let supports_ranges = head_resp
            .headers()
            .get(reqwest::header::ACCEPT_RANGES)
            .and_then(|v| v.to_str().ok())
            .map(|v| v.to_lowercase().contains("bytes"))
            .unwrap_or(false);

        let raw_filename = custom_name.unwrap_or_else(|| {
            head_resp
                .url()
                .path_segments()
                .and_then(|mut s| s.next_back())
                .filter(|s| !s.is_empty())
                .unwrap_or("download.bin")
                .to_string()
        });

        let final_filename = Self::sanitize_filename(&raw_filename);
        let target_path = save_dir.join(&final_filename);

        let canonical_dir = tokio::fs::canonicalize(&save_dir)
            .await
            .map_err(|e| e.to_string())?;
        if !target_path.starts_with(&canonical_dir) && !target_path.starts_with(&save_dir) {
            return Err("Invalid download target path".into());
        }

        let active_connections = if supports_ranges && total_size > 1_048_576 {
            connections.clamp(2, 16)
        } else {
            1
        };

        // Register flags trước khi spawn
        let paused = Arc::new(AtomicBool::new(false));
        let cancelled = Arc::new(AtomicBool::new(false));
        {
            let mut map = get_flags().lock().map_err(|_| "flag map poisoned")?;
            map.insert(
                task_id.clone(),
                DownloadFlags {
                    paused: paused.clone(),
                    cancelled: cancelled.clone(),
                },
            );
        }

        let progress_downloaded = Arc::new(AtomicU64::new(0));
        let done_flag = Arc::new(AtomicBool::new(false));

        let task_id_clone = task_id.clone();
        let final_filename_clone = final_filename.clone();
        let target_path_clone = target_path.clone();
        let url_clone = url.clone();
        let app_clone = app.clone();
        let prog_clone = progress_downloaded.clone();
        let paused_clone = paused.clone();
        let cancel_clone = cancelled.clone();
        let done_clone = done_flag.clone();

        tokio::spawn(async move {
            let target_path_bg = target_path_clone.clone();

            let result = if active_connections > 1 {
                Self::download_multi_threaded(
                    client,
                    url_clone.clone(),
                    target_path_clone.clone(),
                    total_size,
                    active_connections,
                    prog_clone.clone(),
                    cancel_clone.clone(),
                    paused_clone.clone(),
                )
                .await
            } else {
                Self::download_single_stream(
                    client,
                    url_clone.clone(),
                    target_path_clone.clone(),
                    prog_clone.clone(),
                    cancel_clone.clone(),
                    paused_clone.clone(),
                )
                .await
            };

            match result {
                Ok(_) => {
                    let db = app_clone.state::<DbManager>();
                    let size_str = format!("{:.2} MB", total_size as f64 / (1024.0 * 1024.0));
                    let _ = db.insert_download(
                        &final_filename_clone,
                        &url_clone,
                        target_path_bg.to_str().unwrap_or(""),
                        &size_str,
                        "Completed",
                    );
                    let _ = app_clone.emit(
                        "download-progress",
                        DownloadProgressPayload {
                            id: task_id_clone.clone(),
                            filename: final_filename_clone,
                            downloaded_bytes: total_size,
                            total_bytes: total_size,
                            speed_mbps: 0.0,
                            progress_percent: 100.0,
                            status: "Completed".into(),
                            threads: active_connections,
                        },
                    );
                }
                Err(err) => {
                    let is_cancel = cancel_clone.load(Ordering::Relaxed) && err == "Download cancelled";
                    let _ = app_clone.emit(
                        "download-progress",
                        DownloadProgressPayload {
                            id: task_id_clone.clone(),
                            filename: final_filename_clone,
                            downloaded_bytes: prog_clone.load(Ordering::Relaxed),
                            total_bytes: total_size,
                            speed_mbps: 0.0,
                            progress_percent: 0.0,
                            status: if is_cancel {
                                "Cancelled".into()
                            } else {
                                format!("Failed: {}", err)
                            },
                            threads: active_connections,
                        },
                    );
                }
            }
            done_clone.store(true, Ordering::Relaxed);
            unregister(&task_id_clone);
        });

        // Progress ticker
        let app_ticker = app.clone();
        let task_id_ticker = task_id.clone();
        let filename_ticker = final_filename.clone();
        let cancel_ticker = cancelled.clone();
        let paused_ticker = paused.clone();
        let prog_ticker = progress_downloaded.clone();
        let done_ticker = done_flag.clone();

        tokio::spawn(async move {
            let mut last_bytes = 0u64;
            let mut last_time = Instant::now();

            loop {
                tokio::time::sleep(TokioDuration::from_millis(500)).await;
                if cancel_ticker.load(Ordering::Relaxed) || done_ticker.load(Ordering::Relaxed) {
                    break;
                }

                let current_bytes = prog_ticker.load(Ordering::Relaxed);
                let elapsed = last_time.elapsed().as_secs_f64();
                let bytes_diff = current_bytes.saturating_sub(last_bytes);
                let speed_mbps = if elapsed > 0.0 {
                    (bytes_diff as f64 * 8.0) / (elapsed * 1_000_000.0)
                } else {
                    0.0
                };

                last_bytes = current_bytes;
                last_time = Instant::now();

                let percent = if total_size > 0 {
                    ((current_bytes as f64 / total_size as f64) * 100.0) as f32
                } else {
                    0.0
                };

                let status = if paused_ticker.load(Ordering::Relaxed) {
                    "Paused"
                } else {
                    "Downloading"
                };

                let _ = app_ticker.emit(
                    "download-progress",
                    DownloadProgressPayload {
                        id: task_id_ticker.clone(),
                        filename: filename_ticker.clone(),
                        downloaded_bytes: current_bytes,
                        total_bytes: total_size,
                        speed_mbps: (speed_mbps * 10.0).round() / 10.0,
                        progress_percent: (percent * 10.0).round() / 10.0,
                        status: status.into(),
                        threads: active_connections,
                    },
                );

                if total_size > 0 && current_bytes >= total_size {
                    break;
                }
            }
        });

        Ok(task_id)
    }

    /// Wait nếu đang pause. Trả Err nếu bị cancel.
    async fn wait_if_paused(
        paused: &AtomicBool,
        cancelled: &AtomicBool,
    ) -> Result<(), String> {
        while paused.load(Ordering::Acquire) {
            if cancelled.load(Ordering::Acquire) {
                return Err("Download cancelled".into());
            }
            tokio::time::sleep(TokioDuration::from_millis(200)).await;
        }
        if cancelled.load(Ordering::Acquire) {
            return Err("Download cancelled".into());
        }
        Ok(())
    }

    async fn download_multi_threaded(
        client: reqwest::Client,
        url: String,
        target_path: PathBuf,
        total_size: u64,
        connections: usize,
        progress: Arc<AtomicU64>,
        cancelled: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> Result<(), String> {
        let chunk_size = total_size / connections as u64;
        let mut handles = Vec::new();
        let mut part_files = Vec::new();

        for i in 0..connections {
            let start = i as u64 * chunk_size;
            let end = if i == connections - 1 {
                total_size - 1
            } else {
                (i as u64 + 1) * chunk_size - 1
            };
            let part_path = target_path.with_extension(format!("part{}", i));
            part_files.push(part_path.clone());

            let client_c = client.clone();
            let url_c = url.clone();
            let prog_c = progress.clone();
            let cancel_c = cancelled.clone();
            let pause_c = paused.clone();

            let handle = tokio::spawn(async move {
                // Pause check trước khi mở connection.
                Self::wait_if_paused(&pause_c, &cancel_c).await?;

                let resp = client_c
                    .get(&url_c)
                    .header("Range", format!("bytes={}-{}", start, end))
                    .send()
                    .await
                    .map_err(|e| e.to_string())?;

                let mut file = OpenOptions::new()
                    .create(true)
                    .write(true)
                    .truncate(true)
                    .open(&part_path)
                    .await
                    .map_err(|e| e.to_string())?;

                let mut stream = resp.bytes_stream();
                use futures_util::StreamExt;

                loop {
                    // Check pause/cancel trước mỗi chunk.
                    Self::wait_if_paused(&pause_c, &cancel_c).await?;

                    // Timeout 500ms cho stream.next() để có thể check pause trong lúc chờ data.
                    let chunk_opt = match timeout(TokioDuration::from_millis(500), stream.next())
                        .await
                    {
                        Ok(Some(res)) => Some(res),
                        Ok(None) => None, // stream hết
                        Err(_) => continue, // timeout → loop check pause
                    };

                    match chunk_opt {
                        Some(Ok(chunk)) => {
                            file.write_all(&chunk)
                                .await
                                .map_err(|e| e.to_string())?;
                            prog_c.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                        }
                        Some(Err(e)) => return Err(e.to_string()),
                        None => break,
                    }
                }

                file.flush().await.map_err(|e| e.to_string())?;
                Ok::<(), String>(())
            });
            handles.push(handle);
        }

        let mut any_err: Option<String> = None;
        for h in handles {
            match h.await {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    if any_err.is_none() {
                        any_err = Some(e);
                    }
                }
                Err(e) => {
                    if any_err.is_none() {
                        any_err = Some(e.to_string());
                    }
                }
            }
        }

        if let Some(e) = any_err {
            for p in &part_files {
                let _ = tokio::fs::remove_file(p).await;
            }
            let _ = tokio::fs::remove_file(&target_path).await;
            return Err(e);
        }

        // Merge parts
        let mut final_file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&target_path)
            .await
            .map_err(|e| e.to_string())?;

        for p in &part_files {
            let mut part_f = File::open(p).await.map_err(|e| e.to_string())?;
            let mut buf = vec![0u8; 1024 * 1024];
            loop {
                let n = part_f.read(&mut buf).await.map_err(|e| e.to_string())?;
                if n == 0 {
                    break;
                }
                final_file
                    .write_all(&buf[..n])
                    .await
                    .map_err(|e| e.to_string())?;
            }
            let _ = tokio::fs::remove_file(p).await;
        }
        final_file.flush().await.map_err(|e| e.to_string())?;

        Ok(())
    }

    async fn download_single_stream(
        client: reqwest::Client,
        url: String,
        target_path: PathBuf,
        progress: Arc<AtomicU64>,
        cancelled: Arc<AtomicBool>,
        paused: Arc<AtomicBool>,
    ) -> Result<(), String> {
        Self::wait_if_paused(&paused, &cancelled).await?;

        let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&target_path)
            .await
            .map_err(|e| e.to_string())?;

        let mut stream = resp.bytes_stream();
        use futures_util::StreamExt;

        loop {
            Self::wait_if_paused(&paused, &cancelled).await?;

            let chunk_opt =
                match timeout(TokioDuration::from_millis(500), stream.next()).await {
                    Ok(Some(res)) => Some(res),
                    Ok(None) => None,
                    Err(_) => continue,
                };

            match chunk_opt {
                Some(Ok(chunk)) => {
                    file.write_all(&chunk).await.map_err(|e| e.to_string())?;
                    progress.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                }
                Some(Err(e)) => return Err(e.to_string()),
                None => break,
            }
        }

        file.flush().await.map_err(|e| e.to_string())?;
        Ok(())
    }
}
