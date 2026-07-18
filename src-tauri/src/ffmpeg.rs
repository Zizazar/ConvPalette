//! Поиск/загрузка бинарников ffmpeg и ffprobe, запуск конвертации с прогрессом и отменой.

use serde::Serialize;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

/// Реестр запущенных задач конвертации (для отмены).
#[derive(Default)]
pub struct Jobs {
    pub cancel: AtomicBool,
    pub pids: Mutex<HashSet<u32>>,
}

/// Событие прогресса конвертации.
#[derive(Clone, Serialize)]
pub struct ConversionEvent {
    pub input: String,
    /// "start" | "line" | "done" | "error" | "cancelled"
    pub stage: String,
    pub message: String,
    pub output: Option<String>,
}

/// Событие загрузки ffmpeg.
#[derive(Clone, Serialize)]
pub struct DownloadEvent {
    /// "start" | "progress" | "extract" | "done" | "error"
    pub stage: String,
    pub message: String,
    /// 0..100, если известно.
    pub progress: Option<f64>,
}

#[cfg(target_os = "windows")]
pub fn hide_console_cmd(cmd: &mut Command) {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    cmd.creation_flags(CREATE_NO_WINDOW);
}

#[cfg(not(target_os = "windows"))]
pub fn hide_console_cmd(_cmd: &mut Command) {}

fn exe_name(base: &str) -> String {
    if cfg!(target_os = "windows") {
        format!("{base}.exe")
    } else {
        base.to_string()
    }
}

pub fn bundled_bin_dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|dir| dir.join("bin"))
}

fn resolve_tool(app: &AppHandle, base: &str) -> Option<PathBuf> {
    // 1. Бандл-папка приложения.
    if let Some(bin_dir) = bundled_bin_dir(app) {
        let candidate = bin_dir.join(exe_name(base));
        if candidate.exists() {
            return Some(candidate);
        }
    }
    // 2. PATH — проверяем, что бинарник запускается.
    let mut cmd = Command::new(exe_name(base));
    cmd.arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console_cmd(&mut cmd);
    if cmd.status().map(|s| s.success()).unwrap_or(false) {
        return Some(PathBuf::from(exe_name(base)));
    }
    None
}

pub fn resolve_ffmpeg(app: &AppHandle) -> Option<PathBuf> {
    resolve_tool(app, "ffmpeg")
}

pub fn resolve_ffprobe(app: &AppHandle) -> Option<PathBuf> {
    resolve_tool(app, "ffprobe")
}

// ── Загрузка ffmpeg ──────────────────────────────────────────────────────────

const FFMPEG_URL: &str =
    "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-win64-gpl.zip";

/// Скачать и распаковать ffmpeg/ffprobe в бандл-папку. Блокирующая функция.
pub fn download_ffmpeg(app: &AppHandle) -> Result<(), String> {
    let emit = |stage: &str, message: String, progress: Option<f64>| {
        let _ = app.emit(
            "ffmpeg://download",
            DownloadEvent {
                stage: stage.to_string(),
                message,
                progress,
            },
        );
    };

    let bin_dir = bundled_bin_dir(app).ok_or("Не удалось определить папку данных")?;
    std::fs::create_dir_all(&bin_dir).map_err(|e| e.to_string())?;

    emit("start", "Загрузка ffmpeg…".into(), Some(0.0));

    let resp = ureq::get(FFMPEG_URL)
        .call()
        .map_err(|e| format!("Ошибка загрузки: {e}"))?;

    let total: Option<u64> = resp
        .header("Content-Length")
        .and_then(|v| v.parse::<u64>().ok());

    let zip_path = bin_dir.join("ffmpeg_download.zip");
    let mut file = std::fs::File::create(&zip_path).map_err(|e| e.to_string())?;
    let mut reader = resp.into_reader();
    let mut buf = [0u8; 64 * 1024];
    let mut downloaded: u64 = 0;
    let mut last_pct = -1i64;

    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        downloaded += n as u64;
        if let Some(total) = total {
            let pct = (downloaded as f64 / total as f64 * 100.0).floor();
            if pct as i64 != last_pct {
                last_pct = pct as i64;
                emit("progress", format!("Загрузка ffmpeg… {pct:.0}%"), Some(pct));
            }
        } else {
            emit(
                "progress",
                format!("Загружено {} МБ", downloaded / 1_048_576),
                None,
            );
        }
    }
    drop(file);

    emit("extract", "Распаковка…".into(), None);
    extract_tools(&zip_path, &bin_dir)?;
    let _ = std::fs::remove_file(&zip_path);

    // Проверка: бинарник должен запускаться.
    if resolve_ffmpeg(app).is_none() {
        emit("error", "ffmpeg распакован, но не запускается".into(), None);
        return Err("ffmpeg не прошёл проверку после распаковки".into());
    }

    emit("done", "ffmpeg готов к работе".into(), Some(100.0));
    Ok(())
}

/// Достать из zip только ffmpeg.exe и ffprobe.exe в bin_dir (без структуры папок).
fn extract_tools(zip_path: &Path, bin_dir: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    let wanted = [exe_name("ffmpeg"), exe_name("ffprobe")];
    let mut found = 0;

    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let name = entry.name().replace('\\', "/");
        let base = name.rsplit('/').next().unwrap_or("");
        if wanted.iter().any(|w| w == base) {
            let out_path = bin_dir.join(base);
            let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
            found += 1;
        }
    }

    if found == 0 {
        return Err("В архиве не найдены ffmpeg/ffprobe".into());
    }
    Ok(())
}

// ── Выполнение конвертации ───────────────────────────────────────────────────

/// Запустить одну конвертацию (блокирующая функция, вызывать в отдельном потоке).
pub fn run_single(app: &AppHandle, ffmpeg: &PathBuf, input: &str, args: &[String], output: &str) {
    let emit = |stage: &str, message: String, output: Option<String>| {
        let _ = app.emit(
            "conversion://progress",
            ConversionEvent {
                input: input.to_string(),
                stage: stage.to_string(),
                message,
                output,
            },
        );
    };

    let jobs = app.state::<Jobs>();
    if jobs.cancel.load(Ordering::SeqCst) {
        emit("cancelled", "Отменено".into(), None);
        return;
    }

    emit("start", format!("Запуск: {input}"), None);

    let mut cmd = Command::new(ffmpeg);
    cmd.args(args).stdout(Stdio::null()).stderr(Stdio::piped());
    hide_console_cmd(&mut cmd);

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            emit("error", format!("Не удалось запустить ffmpeg: {e}"), None);
            return;
        }
    };

    let pid = child.id();
    jobs.pids.lock().unwrap().insert(pid);

    // Хвост диагностики ffmpeg (без прогресс-строк): попадает в сообщение об
    // ошибке, чтобы ИИ мог исправить неудачные опции.
    let mut tail: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    if let Some(stderr) = child.stderr.take() {
        let reader = BufReader::new(stderr);
        for line in reader.lines().map_while(Result::ok) {
            if line.contains("time=") || line.contains("frame=") {
                emit("line", line, None);
            } else if !line.trim().is_empty() {
                if tail.len() >= 8 {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
        }
    }

    let status = child.wait();
    jobs.pids.lock().unwrap().remove(&pid);
    let cancelled = jobs.cancel.load(Ordering::SeqCst);

    let mut diag = tail.into_iter().collect::<Vec<_>>().join("\n");
    if diag.len() > 700 {
        diag = diag.chars().skip(diag.chars().count() - 700).collect();
    }

    match status {
        Ok(s) if s.success() => {
            emit(
                "done",
                format!("Готово: {output}"),
                Some(output.to_string()),
            );
        }
        _ if cancelled => {
            emit("cancelled", "Отменено".into(), None);
        }
        Ok(s) => emit(
            "error",
            format!("ffmpeg завершился с кодом {s}\n{diag}"),
            None,
        ),
        Err(e) => emit("error", format!("Ошибка процесса: {e}"), None),
    }
}

/// Отменить все запущенные конвертации: снять флаг и убить процессы.
pub fn cancel_all(app: &AppHandle) {
    let jobs = app.state::<Jobs>();
    jobs.cancel.store(true, Ordering::SeqCst);
    let pids: Vec<u32> = jobs.pids.lock().unwrap().iter().copied().collect();
    for pid in pids {
        kill_pid(pid);
    }
}

#[cfg(target_os = "windows")]
fn kill_pid(pid: u32) {
    let mut cmd = Command::new("taskkill");
    cmd.args(["/F", "/T", "/PID", &pid.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    hide_console_cmd(&mut cmd);
    let _ = cmd.status();
}

#[cfg(not(target_os = "windows"))]
fn kill_pid(pid: u32) {
    let _ = Command::new("kill").arg(pid.to_string()).status();
}
