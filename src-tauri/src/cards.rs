//! Карточки файлов для левой панели: метаданные (размер, длительность,
//! разрешение) через ffprobe и миниатюры через ffmpeg (кадр видео, уменьшенная
//! картинка, волновая форма аудио). Миниатюры кэшируются в app_data/thumbs
//! по хэшу (путь + mtime) — пересоздаются только после изменения файла.

use crate::ffmpeg;
use crate::presets::{self, Category};
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Serialize)]
pub struct FolderStats {
    pub video: usize,
    pub audio: usize,
    pub image: usize,
    pub other: usize,
    pub dirs: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct FileCard {
    pub path: String,
    pub name: String,
    /// video | audio | image | other | folder
    pub kind: String,
    pub ext: String,
    pub size: u64,
    /// Длительность в секундах (видео/аудио).
    pub duration: Option<f64>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    /// Путь к файлу миниатюры — фронтенд показывает его через asset-протокол.
    pub thumb: Option<String>,
    /// Содержимое папки по категориям (только для kind == "folder").
    pub folder: Option<FolderStats>,
}

// ── Структурированный ffprobe с кэшем ────────────────────────────────────────

type MetaTuple = (Option<f64>, Option<u32>, Option<u32>); // duration, width, height

static META_CACHE: OnceLock<Mutex<HashMap<String, (u64, MetaTuple)>>> = OnceLock::new();

fn file_mtime_secs(path: &str) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Длительность и разрешение через ffprobe (JSON-вывод). Всё опционально:
/// без ffprobe или на битом файле карточка просто остаётся без метаданных.
fn probe_media(app: &AppHandle, input: &str) -> MetaTuple {
    let cache = META_CACHE.get_or_init(Default::default);
    let mtime = file_mtime_secs(input);
    if let Some((cached_mtime, meta)) = cache.lock().unwrap().get(input) {
        if *cached_mtime == mtime {
            return *meta;
        }
    }

    let mut meta: MetaTuple = (None, None, None);
    if let Some(ffprobe) = ffmpeg::resolve_ffprobe(app) {
        let mut cmd = Command::new(ffprobe);
        cmd.args([
            "-v",
            "error",
            "-show_entries",
            "format=duration:stream=codec_type,width,height",
            "-of",
            "json",
            input,
        ]);
        ffmpeg::hide_console_cmd(&mut cmd);
        if let Ok(out) = cmd.output() {
            if out.status.success() {
                if let Ok(v) =
                    serde_json::from_slice::<serde_json::Value>(&out.stdout)
                {
                    meta.0 = v["format"]["duration"]
                        .as_str()
                        .and_then(|s| s.parse::<f64>().ok())
                        .filter(|d| d.is_finite() && *d > 0.0);
                    if let Some(streams) = v["streams"].as_array() {
                        for s in streams {
                            if let (Some(w), Some(h)) =
                                (s["width"].as_u64(), s["height"].as_u64())
                            {
                                meta.1 = Some(w as u32);
                                meta.2 = Some(h as u32);
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    let mut map = cache.lock().unwrap();
    if map.len() > 300 {
        map.clear();
    }
    map.insert(input.to_string(), (mtime, meta));
    meta
}

// ── Миниатюры ────────────────────────────────────────────────────────────────

fn thumbs_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?.join("thumbs");
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// Имя миниатюры — хэш (путь + mtime): смена файла даёт новую миниатюру,
/// старые постепенно вычищаются prune_thumbs.
fn thumb_file(dir: &Path, input: &str, ext: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    input.hash(&mut h);
    file_mtime_secs(input).hash(&mut h);
    dir.join(format!("{:016x}.{ext}", h.finish()))
}

/// Не дать кэшу миниатюр расти бесконечно: при переполнении удалить всё.
fn prune_thumbs(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        let files: Vec<_> = entries.flatten().collect();
        if files.len() > 800 {
            for f in files {
                let _ = std::fs::remove_file(f.path());
            }
        }
    }
}

/// Сгенерировать миниатюру: кадр видео / уменьшенное изображение / волна аудио.
fn make_thumb(app: &AppHandle, input: &str, kind: Category) -> Option<String> {
    let ffmpeg_bin = ffmpeg::resolve_ffmpeg(app)?;
    let dir = thumbs_dir(app)?;
    // Волне аудио нужна прозрачность — png; остальным хватает jpg.
    let ext = if kind == Category::Audio { "png" } else { "jpg" };
    let out = thumb_file(&dir, input, ext);
    if out.exists() {
        return Some(out.to_string_lossy().to_string());
    }
    prune_thumbs(&dir);

    let run = |seek: bool| -> bool {
        let mut cmd = Command::new(&ffmpeg_bin);
        cmd.arg("-y");
        match kind {
            Category::Video => {
                if seek {
                    cmd.args(["-ss", "1"]);
                }
                cmd.args(["-i", input, "-frames:v", "1", "-vf", "scale=320:-2", "-q:v", "4"]);
            }
            Category::Image => {
                cmd.args(["-i", input, "-frames:v", "1", "-vf", "scale=320:-2", "-q:v", "4"]);
            }
            Category::Audio => {
                cmd.args([
                    "-i",
                    input,
                    "-filter_complex",
                    "showwavespic=s=320x72:colors=0x7c86ff",
                    "-frames:v",
                    "1",
                ]);
            }
            _ => return false,
        }
        cmd.arg(&out);
        ffmpeg::hide_console_cmd(&mut cmd);
        cmd.output().map(|o| o.status.success()).unwrap_or(false) && out.exists()
    };

    // Кадр берём с 1-й секунды; для роликов короче секунды — с начала.
    let ok = run(true) || (kind == Category::Video && run(false));
    ok.then(|| out.to_string_lossy().to_string())
}

/// Увеличенное jpg-превью изображения (для форматов, которые WebView не
/// рендерит сам: HEIC, TIFF и т.п.). До 1280 px по ширине.
#[tauri::command]
pub async fn image_preview(app: AppHandle, path: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let ffmpeg_bin =
            ffmpeg::resolve_ffmpeg(&app).ok_or_else(|| "ffmpeg не найден".to_string())?;
        let dir = thumbs_dir(&app).ok_or_else(|| "Нет папки кэша".to_string())?;
        let out = thumb_file(&dir, &path, "preview.jpg");
        if !out.exists() {
            let mut cmd = Command::new(ffmpeg_bin);
            cmd.args([
                "-y",
                "-i",
                &path,
                "-frames:v",
                "1",
                "-vf",
                "scale='min(1280,iw)':-2",
                "-q:v",
                "3",
            ]);
            cmd.arg(&out);
            ffmpeg::hide_console_cmd(&mut cmd);
            let ok = cmd.output().map(|o| o.status.success()).unwrap_or(false);
            if !ok || !out.exists() {
                return Err("Не удалось построить предпросмотр".to_string());
            }
        }
        Ok(out.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Сборка карточек ──────────────────────────────────────────────────────────

/// Содержимое папки по категориям (без рекурсии, максимум 2000 записей —
/// чтобы гигантская папка не подвешивала сборку карточек).
fn folder_stats(path: &Path) -> FolderStats {
    let mut st = FolderStats {
        video: 0,
        audio: 0,
        image: 0,
        other: 0,
        dirs: 0,
    };
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten().take(2000) {
            let p = entry.path();
            if p.is_dir() {
                st.dirs += 1;
                continue;
            }
            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("");
            match presets::category_for_ext(ext) {
                Category::Video => st.video += 1,
                Category::Audio => st.audio += 1,
                Category::Image => st.image += 1,
                Category::Other => st.other += 1,
            }
        }
    }
    st
}

fn build_card(app: &AppHandle, path: &str) -> FileCard {
    let p = Path::new(path);
    let name = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path)
        .to_string();

    if p.is_dir() {
        return FileCard {
            path: path.to_string(),
            name,
            kind: "folder".into(),
            ext: String::new(),
            size: 0,
            duration: None,
            width: None,
            height: None,
            thumb: None,
            folder: Some(folder_stats(p)),
        };
    }

    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    let kind = presets::category_for_ext(&ext);
    let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    let (duration, width, height) = if kind != Category::Other {
        probe_media(app, path)
    } else {
        (None, None, None)
    };
    let thumb = make_thumb(app, path, kind);

    FileCard {
        path: path.to_string(),
        name,
        kind: kind.as_str().to_string(),
        ext,
        size,
        duration,
        width,
        height,
        thumb,
        folder: None,
    }
}

/// Карточки для панели файлов. Блокирующая работа (ffprobe + миниатюры)
/// уходит в пул, чтобы не морозить окно.
#[tauri::command]
pub async fn file_cards(app: AppHandle, paths: Vec<String>) -> Vec<FileCard> {
    tauri::async_runtime::spawn_blocking(move || {
        paths.iter().map(|p| build_card(&app, p)).collect()
    })
    .await
    .unwrap_or_default()
}
