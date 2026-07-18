//! ИИ-провайдеры (OpenRouter / Ollama): по запросу пользователя и метаданным файлов
//! генерируют «опции» ffmpeg. Пути и бинарник задаёт приложение — ИИ их не контролирует.

use crate::ffmpeg;
use crate::presets;
use crate::settings::Settings;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

#[derive(Debug, Clone, Serialize)]
pub struct AiSuggestion {
    pub target_ext: String,
    pub options: Vec<String>,
    pub explanation: String,
}

#[derive(Deserialize)]
struct RawSuggestion {
    target_ext: String,
    options: Vec<String>,
    #[serde(default)]
    explanation: String,
}

fn system_prompt() -> &'static str {
    "Ты — генератор параметров ffmpeg. По запросу пользователя об обработке медиафайлов \
верни СТРОГО один JSON-объект без пояснений вне JSON, в формате: \
{\"target_ext\":\"mp4\",\"options\":[\"-c:v\",\"libx264\"],\"explanation\":\"кратко по-русски\"}. \
Поле options — это аргументы ffmpeg МЕЖДУ '-i <вход>' и выходным файлом. \
Запрещено включать: сам 'ffmpeg', '-i', '-y', любые пути к файлам, выходной файл, \
фильтры movie=/amovie=, протоколы http/https/file/pipe/concat, устройства. \
Одна операция применяется ко всем выбранным файлам одинаково. \
target_ext — расширение результата без точки."
}

/// Кэш метаданных: путь → (mtime исходника, результат ffprobe). Повторные
/// запросы к ИИ по тем же файлам не перезапускают ffprobe, пока файл не изменён.
static PROBE_CACHE: std::sync::OnceLock<
    std::sync::Mutex<std::collections::HashMap<String, (u64, String)>>,
> = std::sync::OnceLock::new();

fn file_mtime_secs(path: &str) -> u64 {
    std::fs::metadata(path)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Короткое описание файла через ffprobe (для промпта). Пустая строка, если ffprobe нет.
pub fn probe_metadata(app: &AppHandle, input: &str) -> String {
    let cache = PROBE_CACHE.get_or_init(Default::default);
    let mtime = file_mtime_secs(input);
    if let Some((cached_mtime, meta)) = cache.lock().unwrap().get(input) {
        if *cached_mtime == mtime {
            return meta.clone();
        }
    }

    let Some(ffprobe) = ffmpeg::resolve_ffprobe(app) else {
        return String::new();
    };
    use std::process::Command;
    let mut cmd = Command::new(ffprobe);
    cmd.args([
        "-v",
        "error",
        "-show_entries",
        "format=format_name,duration:stream=codec_type,codec_name,width,height",
        "-of",
        "default=noprint_wrappers=1",
        input,
    ]);
    ffmpeg::hide_console_cmd(&mut cmd);
    let meta = match cmd.output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout)
            .replace('\n', "; ")
            .trim()
            .to_string(),
        _ => String::new(),
    };

    let mut map = cache.lock().unwrap();
    if map.len() > 200 {
        map.clear();
    }
    map.insert(input.to_string(), (mtime, meta.clone()));
    meta
}

fn strip_fences(s: &str) -> String {
    let t = s.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    let t = t.strip_suffix("```").unwrap_or(t);
    t.trim().to_string()
}

/// Разобрать текст модели в предложение и провалидировать опции.
fn parse_suggestion(content: &str) -> Result<AiSuggestion, String> {
    let cleaned = strip_fences(content);
    let raw: RawSuggestion = serde_json::from_str(&cleaned)
        .map_err(|e| format!("Не удалось разобрать ответ ИИ как JSON: {e}"))?;
    presets::validate_ai_options(&raw.options)?;
    Ok(AiSuggestion {
        target_ext: raw.target_ext,
        options: raw.options,
        explanation: raw.explanation,
    })
}

/// Собрать пользовательское сообщение: запрос + список файлов с метаданными.
fn build_user_message(query: &str, files: &[(String, String)]) -> String {
    let mut msg = format!("Запрос: {query}\n\nФайлы:");
    for (name, meta) in files {
        if meta.is_empty() {
            msg.push_str(&format!("\n- {name}"));
        } else {
            msg.push_str(&format!("\n- {name} ({meta})"));
        }
    }
    msg
}

/// Сообщение для исправления: прежние опции + ошибка ffmpeg, которую они вызвали.
fn build_fix_message(
    query: &str,
    files: &[(String, String)],
    prev_options: &[String],
    target_ext: &str,
    error: &str,
) -> String {
    let mut msg = build_user_message(query, files);
    msg.push_str(&format!(
        "\n\nТвой предыдущий вариант options {prev_options:?} (target_ext \"{target_ext}\") \
завершился ошибкой ffmpeg:\n{error}\n\n\
Исправь options так, чтобы конвертация прошла успешно (при необходимости поменяй \
кодек/фильтры/target_ext). Верни тот же JSON-формат и кратко объясни, что изменил."
    ));
    msg
}

/// Один запрос к выбранному провайдеру: текст сообщения → текст ответа модели.
fn complete(
    settings: &Settings,
    api_key: Option<String>,
    user_msg: &str,
) -> Result<String, String> {
    match settings.provider.as_str() {
        "ollama" => call_ollama(settings, user_msg),
        _ => {
            let key = api_key.ok_or(
                "Не задан API-ключ OpenRouter. Добавьте его в настройках.".to_string(),
            )?;
            call_openrouter(settings, &key, user_msg)
        }
    }
}

/// Сгенерировать предложение через выбранного провайдера.
pub fn generate(
    settings: &Settings,
    api_key: Option<String>,
    query: &str,
    files: &[(String, String)],
) -> Result<AiSuggestion, String> {
    let user_msg = build_user_message(query, files);
    parse_suggestion(&complete(settings, api_key, &user_msg)?)
}

/// Попросить модель исправить опции, вызвавшие ошибку ffmpeg.
pub fn generate_fix(
    settings: &Settings,
    api_key: Option<String>,
    query: &str,
    files: &[(String, String)],
    prev_options: &[String],
    target_ext: &str,
    error: &str,
) -> Result<AiSuggestion, String> {
    let user_msg = build_fix_message(query, files, prev_options, target_ext, error);
    parse_suggestion(&complete(settings, api_key, &user_msg)?)
}

fn call_openrouter(settings: &Settings, api_key: &str, user_msg: &str) -> Result<String, String> {
    let body = serde_json::json!({
        "model": settings.openrouter_model,
        "response_format": { "type": "json_object" },
        "messages": [
            { "role": "system", "content": system_prompt() },
            { "role": "user", "content": user_msg }
        ]
    });

    let resp = ureq::post("https://openrouter.ai/api/v1/chat/completions")
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .set("HTTP-Referer", "https://convpalette.app")
        .set("X-Title", "ConvPalette")
        .send_json(body);

    let value = handle_response(resp)?;
    value["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Пустой ответ OpenRouter".to_string())
}

fn call_ollama(settings: &Settings, user_msg: &str) -> Result<String, String> {
    let url = format!("{}/api/chat", settings.ollama_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": settings.ollama_model,
        "stream": false,
        "format": "json",
        "messages": [
            { "role": "system", "content": system_prompt() },
            { "role": "user", "content": user_msg }
        ]
    });

    let resp = ureq::post(&url)
        .set("Content-Type", "application/json")
        .send_json(body);

    let value = handle_response(resp)?;
    value["message"]["content"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Пустой ответ Ollama".to_string())
}

/// Привести ответ ureq к JSON или к понятной ошибке.
fn handle_response(resp: Result<ureq::Response, ureq::Error>) -> Result<serde_json::Value, String> {
    match resp {
        Ok(r) => r
            .into_json::<serde_json::Value>()
            .map_err(|e| format!("Некорректный JSON от провайдера: {e}")),
        Err(ureq::Error::Status(code, r)) => {
            let text = r.into_string().unwrap_or_default();
            Err(format!("Провайдер вернул ошибку {code}: {text}"))
        }
        Err(e) => Err(format!("Ошибка запроса к провайдеру: {e}")),
    }
}
