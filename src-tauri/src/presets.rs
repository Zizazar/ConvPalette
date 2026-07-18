//! Классификация файлов и каталог пресетов конвертации.
//! Здесь же — построение аргументов ffmpeg для каждого пресета.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Категория файла, определяется по расширению.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Category {
    Video,
    Audio,
    Image,
    Other,
}

impl Category {
    pub fn as_str(&self) -> &'static str {
        match self {
            Category::Video => "video",
            Category::Audio => "audio",
            Category::Image => "image",
            Category::Other => "other",
        }
    }
}

/// Определить категорию по расширению файла.
pub fn category_for_ext(ext: &str) -> Category {
    match ext.to_lowercase().as_str() {
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "flv" | "m4v" | "wmv" | "mpg" | "mpeg" | "ts"
        | "3gp" => Category::Video,
        "mp3" | "wav" | "flac" | "m4a" | "aac" | "ogg" | "opus" | "wma" | "aiff" => Category::Audio,
        "png" | "jpg" | "jpeg" | "webp" | "bmp" | "gif" | "tiff" | "tif" | "heic" | "heif" => {
            Category::Image
        }
        _ => Category::Other,
    }
}

pub fn category_for_path(path: &str) -> Category {
    let ext = Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    category_for_ext(ext)
}

/// Одна рекомендация/пресет, отдаётся во фронтенд.
#[derive(Debug, Clone, Serialize)]
pub struct Preset {
    pub id: String,
    pub label: String,
    pub description: String,
    /// Расширение результата (без точки).
    pub target_ext: String,
    /// Категория, к которой применим пресет.
    pub category: String,
}

fn p(id: &str, label: &str, description: &str, target_ext: &str, category: Category) -> Preset {
    Preset {
        id: id.to_string(),
        label: label.to_string(),
        description: description.to_string(),
        target_ext: target_ext.to_string(),
        category: category.as_str().to_string(),
    }
}

/// Пресеты для конкретной категории.
fn presets_for_category(cat: Category) -> Vec<Preset> {
    match cat {
        Category::Video => vec![
            p(
                "video_mp4",
                "В MP4 (H.264)",
                "Универсальный формат, H.264 + AAC",
                "mp4",
                cat,
            ),
            p(
                "video_compress",
                "Сжать (720p)",
                "Уменьшить размер, 720p, CRF 28",
                "mp4",
                cat,
            ),
            p(
                "video_extract_audio",
                "Извлечь аудио (MP3)",
                "Сохранить только звук",
                "mp3",
                cat,
            ),
            p(
                "video_gif",
                "В GIF",
                "Анимированный GIF из видео",
                "gif",
                cat,
            ),
            p(
                "video_webm",
                "В WebM (VP9)",
                "Открытый формат для веба",
                "webm",
                cat,
            ),
            p(
                "video_mute",
                "Убрать звук",
                "Тот же формат без аудиодорожки",
                "mp4",
                cat,
            ),
        ],
        Category::Audio => vec![
            p("audio_mp3", "В MP3", "Сжатый звук, 192 kbps", "mp3", cat),
            p("audio_wav", "В WAV", "Без потерь, PCM", "wav", cat),
            p("audio_flac", "В FLAC", "Сжатие без потерь", "flac", cat),
            p("audio_m4a", "В M4A (AAC)", "Компактный AAC", "m4a", cat),
            p(
                "audio_normalize",
                "Нормализовать громкость",
                "Выровнять громкость (loudnorm)",
                "mp3",
                cat,
            ),
        ],
        Category::Image => vec![
            p(
                "image_jpg",
                "В JPG",
                "Сжатие с потерями, качество 90",
                "jpg",
                cat,
            ),
            p("image_png", "В PNG", "Без потерь", "png", cat),
            p(
                "image_webp",
                "В WebP",
                "Компактный формат для веба",
                "webp",
                cat,
            ),
            p(
                "image_resize",
                "Уменьшить (1080p)",
                "Вписать в 1920×1080",
                "jpg",
                cat,
            ),
        ],
        Category::Other => vec![],
    }
}

/// Собрать рекомендации для набора путей.
/// Если все файлы одной категории — отдаём её пресеты, иначе — безопасное пересечение.
pub fn recommendations_for_paths(paths: &[String]) -> Vec<Preset> {
    let cats: Vec<Category> = paths.iter().map(|p| category_for_path(p)).collect();
    if cats.is_empty() {
        return vec![];
    }
    let first = cats[0];
    let all_same = cats.iter().all(|c| *c == first);
    if all_same {
        presets_for_category(first)
    } else {
        // Смешанный выбор: пока не предлагаем ничего автоматически —
        // пользователь уточнит запросом к ИИ.
        vec![]
    }
}

/// Найти пресет по id (нужно при запуске конвертации).
pub fn preset_by_id(id: &str) -> Option<Preset> {
    for cat in [Category::Video, Category::Audio, Category::Image] {
        if let Some(found) = presets_for_category(cat).into_iter().find(|p| p.id == id) {
            return Some(found);
        }
    }
    None
}

/// Куда и под каким именем класть результат (из настроек).
#[derive(Debug, Clone, Default)]
pub struct OutputConfig {
    /// Папка вывода; None или несуществующая — рядом с исходником.
    pub dir: Option<String>,
    /// Шаблон имени; `{name}` — имя исходника без расширения.
    pub template: String,
}

/// Построить путь результата по настройкам, с авто-инкрементом при коллизии.
pub fn output_path_for(input: &str, target_ext: &str, cfg: &OutputConfig) -> PathBuf {
    let input_path = Path::new(input);
    let stem = input_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");

    // Папка: заданная в настройках (если существует), иначе рядом с исходником.
    let dir: PathBuf = match cfg.dir.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) if Path::new(d).is_dir() => PathBuf::from(d),
        _ => input_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    };

    // Имя по шаблону; пустой или бессмысленный шаблон — дефолт.
    let template = if cfg.template.trim().is_empty() {
        "{name}_converted"
    } else {
        cfg.template.trim()
    };
    let mut name = template.replace("{name}", stem);
    // Убрать символы, недопустимые в именах файлов Windows.
    name.retain(|c| !matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*'));
    if name.trim().is_empty() {
        name = format!("{stem}_converted");
    }

    let mut candidate = dir.join(format!("{name}.{target_ext}"));
    let mut n = 1;
    while candidate.exists() {
        candidate = dir.join(format!("{name}_{n}.{target_ext}"));
        n += 1;
    }
    candidate
}

/// Проверка «опций» ffmpeg, сгенерированных ИИ, на безопасность.
/// ИИ отдаёт только опции между `-i input` и `output`; пути и бинарник задаём мы.
/// Возвращает Err с причиной, если найдено что-то подозрительное.
pub fn validate_ai_options(options: &[String]) -> Result<(), String> {
    // Токены, которые ИИ не должен присылать: доп. вводы, произвольные протоколы,
    // фильтры, читающие/пишущие файлы, доступ к устройствам.
    let banned_exact = ["-i", "-y", "-f"];
    let banned_substr = [
        "movie=",
        "amovie=",
        "concat:",
        "subfile",
        "/dev/",
        "\\\\.\\",
        "http://",
        "https://",
        "file:",
        "pipe:",
        "-attach",
        "-map_metadata:",
    ];

    if options.len() > 40 {
        return Err("Слишком длинный список опций от ИИ".into());
    }
    for opt in options {
        let low = opt.to_lowercase();
        if banned_exact.iter().any(|b| low == *b) {
            return Err(format!("Опция '{opt}' запрещена"));
        }
        if banned_substr.iter().any(|b| low.contains(b)) {
            return Err(format!("Опция '{opt}' содержит запрещённую конструкцию"));
        }
    }
    Ok(())
}

/// Построить аргументы ffmpeg из «опций» ИИ для одного файла.
/// Формат команды: `-y -i <input> <options...> <output.target_ext>`.
pub fn build_ai_args(
    options: &[String],
    target_ext: &str,
    input: &str,
    out_cfg: &OutputConfig,
) -> Result<(Vec<String>, PathBuf), String> {
    validate_ai_options(options)?;
    let safe_ext: String = target_ext
        .trim_start_matches('.')
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    if safe_ext.is_empty() {
        return Err("ИИ не указал корректное расширение результата".into());
    }
    let out = output_path_for(input, &safe_ext, out_cfg);
    let mut args: Vec<String> = vec!["-y".into(), "-i".into(), input.to_string()];
    args.extend(options.iter().cloned());
    args.push(out.to_string_lossy().to_string());
    Ok((args, out))
}

/// Построить аргументы ffmpeg для пресета и одного входного файла.
/// Возвращает (аргументы, путь_результата).
pub fn build_ffmpeg_args(
    preset_id: &str,
    input: &str,
    out_cfg: &OutputConfig,
) -> Option<(Vec<String>, PathBuf)> {
    let preset = preset_by_id(preset_id)?;
    let out = output_path_for(input, &preset.target_ext, out_cfg);
    let out_str = out.to_string_lossy().to_string();

    // Базовые аргументы: -y (перезапись результата — он новый), -i input
    let mut args: Vec<String> = vec!["-y".into(), "-i".into(), input.to_string()];

    let extra: Vec<String> = match preset_id {
        "video_mp4" => vec![
            "-c:v", "libx264", "-preset", "medium", "-crf", "20", "-c:a", "aac", "-b:a", "192k",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        "video_compress" => vec![
            "-vf",
            "scale=-2:720",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "28",
            "-c:a",
            "aac",
            "-b:a",
            "128k",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        "video_extract_audio" => vec!["-vn", "-c:a", "libmp3lame", "-b:a", "192k"]
            .into_iter()
            .map(String::from)
            .collect(),
        "video_gif" => vec!["-vf", "fps=12,scale=480:-1:flags=lanczos"]
            .into_iter()
            .map(String::from)
            .collect(),
        "video_webm" => vec![
            "-c:v",
            "libvpx-vp9",
            "-crf",
            "32",
            "-b:v",
            "0",
            "-c:a",
            "libopus",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        "video_mute" => vec!["-an", "-c:v", "copy"]
            .into_iter()
            .map(String::from)
            .collect(),

        "audio_mp3" => vec!["-c:a", "libmp3lame", "-b:a", "192k"]
            .into_iter()
            .map(String::from)
            .collect(),
        "audio_wav" => vec!["-c:a", "pcm_s16le"]
            .into_iter()
            .map(String::from)
            .collect(),
        "audio_flac" => vec!["-c:a", "flac"].into_iter().map(String::from).collect(),
        "audio_m4a" => vec!["-c:a", "aac", "-b:a", "192k"]
            .into_iter()
            .map(String::from)
            .collect(),
        "audio_normalize" => vec!["-af", "loudnorm", "-c:a", "libmp3lame", "-b:a", "192k"]
            .into_iter()
            .map(String::from)
            .collect(),

        "image_jpg" => vec!["-q:v", "2"].into_iter().map(String::from).collect(),
        "image_png" => vec![],
        "image_webp" => vec!["-quality", "85"]
            .into_iter()
            .map(String::from)
            .collect(),
        "image_resize" => vec![
            "-vf",
            "scale='min(1920,iw)':'min(1080,ih)':force_original_aspect_ratio=decrease",
            "-q:v",
            "2",
        ]
        .into_iter()
        .map(String::from)
        .collect(),

        _ => return None,
    };

    args.extend(extra);
    args.push(out_str);
    Some((args, out))
}
