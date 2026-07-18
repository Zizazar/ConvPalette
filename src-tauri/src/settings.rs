//! Настройки приложения (JSON в app_data_dir) и API-ключ (Windows Credential Manager).

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

const KEYRING_SERVICE: &str = "ConvPalette";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// "openrouter" | "ollama"
    pub provider: String,
    pub openrouter_model: String,
    pub ollama_url: String,
    pub ollama_model: String,
    /// Папка вывода; None — рядом с исходным файлом.
    pub output_dir: Option<String>,
    /// Шаблон имени результата; `{name}` — имя исходника без расширения.
    pub filename_template: String,
    /// "system" | "dark" | "light"
    pub theme: String,
    /// "ru" | "en"
    pub language: String,
    /// Глобальный хоткей, напр. "ctrl+alt+space".
    pub hotkey: String,
    /// Прятать палитру при потере фокуса. Выключено по умолчанию:
    /// иначе окно исчезает, как только начинаешь тянуть файл из Проводника.
    pub hide_on_blur: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            provider: "openrouter".into(),
            openrouter_model: "openai/gpt-4o-mini".into(),
            ollama_url: "http://localhost:11434".into(),
            ollama_model: "qwen2.5:7b".into(),
            output_dir: None,
            filename_template: "{name}_converted".into(),
            theme: "system".into(),
            language: "ru".into(),
            hotkey: "ctrl+alt+space".into(),
            hide_on_blur: false,
        }
    }
}

fn settings_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("settings.json"))
}

pub fn load(app: &AppHandle) -> Settings {
    if let Some(path) = settings_path(app) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(s) = serde_json::from_str::<Settings>(&text) {
                return s;
            }
        }
    }
    Settings::default()
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    let path = settings_path(app).ok_or("Не удалось определить папку данных")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let text = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())?;
    Ok(())
}

/// Имя «пользователя» в Credential Manager — по провайдеру.
fn keyring_user(provider: &str) -> &'static str {
    match provider {
        "openrouter" => "openrouter",
        _ => "generic",
    }
}

pub fn set_api_key(provider: &str, key: &str) -> Result<(), String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, keyring_user(provider))
        .map_err(|e| e.to_string())?;
    if key.is_empty() {
        // Пустой ключ — удалить сохранённый.
        let _ = entry.delete_credential();
        return Ok(());
    }
    entry.set_password(key).map_err(|e| e.to_string())
}

pub fn get_api_key(provider: &str) -> Option<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, keyring_user(provider)).ok()?;
    entry.get_password().ok()
}

pub fn has_api_key(provider: &str) -> bool {
    get_api_key(provider).map(|k| !k.is_empty()).unwrap_or(false)
}

// ── История запросов/конвертаций ─────────────────────────────────────────────

const HISTORY_LIMIT: usize = 30;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// Unix-время (секунды).
    pub ts: u64,
    /// "preset" | "ai"
    pub kind: String,
    /// Название пресета или запрос к ИИ.
    pub label: String,
    /// Расширение результата.
    pub target_ext: String,
    /// Сколько файлов конвертировалось.
    pub files: usize,
}

fn history_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("history.json"))
}

pub fn load_history(app: &AppHandle) -> Vec<HistoryEntry> {
    if let Some(path) = history_path(app) {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Ok(h) = serde_json::from_str::<Vec<HistoryEntry>>(&text) {
                return h;
            }
        }
    }
    vec![]
}

/// Добавить запись в начало истории (с ограничением размера).
pub fn push_history(app: &AppHandle, entry: HistoryEntry) -> Result<(), String> {
    let path = history_path(app).ok_or("Не удалось определить папку данных")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let mut items = load_history(app);
    items.insert(0, entry);
    items.truncate(HISTORY_LIMIT);
    let text = serde_json::to_string_pretty(&items).map_err(|e| e.to_string())?;
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

pub fn clear_history(app: &AppHandle) -> Result<(), String> {
    if let Some(path) = history_path(app) {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}
