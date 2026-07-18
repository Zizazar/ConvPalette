mod ai;
mod ffmpeg;
mod integration;
mod presets;
mod settings;

use ffmpeg::Jobs;
use settings::Settings;
use std::sync::atomic::Ordering;
use std::sync::Mutex;
use std::time::Instant;
use tauri::{AppHandle, Manager, State};

// ── Состояние контекста (подтянутые из Проводника пути) ──────────────────────

struct ContextInner {
    paths: Vec<String>,
    last_update: Option<Instant>,
}

struct ContextState {
    inner: Mutex<ContextInner>,
}

/// Отобрать из аргументов запуска реально существующие пути (файлы/папки).
fn extract_paths(argv: &[String]) -> Vec<String> {
    argv.iter()
        .skip(1) // первый аргумент — путь к самому exe
        .filter(|a| !a.starts_with('-'))
        .filter(|a| std::path::Path::new(a).exists())
        .cloned()
        .collect()
}

/// Обновить контекст новыми путями.
/// Если с прошлого вызова прошло > 1.5 с — считаем это новой выборкой и заменяем,
/// иначе дополняем (агрегация нескольких вызовов из контекстного меню).
fn update_context(app: &AppHandle, new_paths: Vec<String>) {
    if new_paths.is_empty() {
        return;
    }
    {
        let state = app.state::<ContextState>();
        let mut inner = state.inner.lock().unwrap();
        let reset = inner
            .last_update
            .map(|t| t.elapsed().as_millis() > 1500)
            .unwrap_or(true);
        if reset {
            inner.paths.clear();
        }
        for p in new_paths {
            if !inner.paths.contains(&p) {
                inner.paths.push(p);
            }
        }
        inner.last_update = Some(Instant::now());
    }
    show_window(app);
    use tauri::Emitter;
    let _ = app.emit("context-updated", ());
}

// ── Управление окном-палитрой ────────────────────────────────────────────────

fn show_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
        use tauri::Emitter;
        let _ = app.emit("palette-shown", ());
    }
}

fn hide_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
}

fn toggle_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(false) {
            let _ = w.hide();
        } else {
            let _ = w.center();
            let _ = w.show();
            let _ = w.set_focus();
            use tauri::Emitter;
            let _ = app.emit("palette-shown", ());
        }
    }
}

// ── Команды для фронтенда ────────────────────────────────────────────────────

#[tauri::command]
fn get_context(state: State<ContextState>) -> Vec<String> {
    state.inner.lock().unwrap().paths.clone()
}

#[tauri::command]
fn get_recommendations(paths: Vec<String>) -> Vec<presets::Preset> {
    presets::recommendations_for_paths(&paths)
}

/// Конфигурация вывода (папка + шаблон имени) из настроек.
fn out_cfg(app: &AppHandle) -> presets::OutputConfig {
    let s = settings::load(app);
    presets::OutputConfig {
        dir: s.output_dir,
        template: s.filename_template,
    }
}

/// Предпросмотр команд ffmpeg (по строке на файл) для показа пользователю.
#[tauri::command]
fn command_preview(app: AppHandle, preset_id: String, paths: Vec<String>) -> Vec<String> {
    let cfg = out_cfg(&app);
    paths
        .iter()
        .filter_map(|input| {
            presets::build_ffmpeg_args(&preset_id, input, &cfg)
                .map(|(args, _out)| format!("ffmpeg {}", args.join(" ")))
        })
        .collect()
}

#[tauri::command]
fn ffmpeg_available(app: AppHandle) -> bool {
    ffmpeg::resolve_ffmpeg(&app).is_some()
}

/// Запустить конвертацию всех файлов по выбранному пресету.
/// Каждый файл обрабатывается в своём потоке, прогресс — событиями.
/// Сбросить флаг отмены и очистить реестр перед новой пачкой задач.
fn reset_jobs(app: &AppHandle) {
    let jobs = app.state::<Jobs>();
    jobs.cancel.store(false, Ordering::SeqCst);
    jobs.pids.lock().unwrap().clear();
}

/// Максимум одновременно работающих процессов ffmpeg. Остальные файлы ждут
/// в очереди — иначе пачка тяжёлых видео забивает CPU и диск.
const MAX_PARALLEL: usize = 2;

/// Прогнать задачи (input, args, output) через пул из MAX_PARALLEL воркеров.
/// Отмена работает как раньше: cancel_all убивает запущенные процессы, а
/// run_single для ещё не начатых задач сразу шлёт событие "cancelled".
fn spawn_queue(app: &AppHandle, ffmpeg: std::path::PathBuf, jobs: Vec<(String, Vec<String>, String)>) {
    use std::collections::VecDeque;
    use std::sync::Arc;

    let workers = MAX_PARALLEL.min(jobs.len()).max(1);
    let queue = Arc::new(Mutex::new(jobs.into_iter().collect::<VecDeque<_>>()));
    for _ in 0..workers {
        let app2 = app.clone();
        let ffmpeg2 = ffmpeg.clone();
        let q = Arc::clone(&queue);
        std::thread::spawn(move || loop {
            let job = q.lock().unwrap().pop_front();
            let Some((input, args, out)) = job else { break };
            ffmpeg::run_single(&app2, &ffmpeg2, &input, &args, &out);
        });
    }
}

#[tauri::command]
fn start_conversion(app: AppHandle, preset_id: String, paths: Vec<String>) -> Result<(), String> {
    let ffmpeg = ffmpeg::resolve_ffmpeg(&app)
        .ok_or_else(|| "ffmpeg не найден. Установите ffmpeg или скачайте его в приложении.".to_string())?;
    reset_jobs(&app);
    let cfg = out_cfg(&app);

    let jobs: Vec<(String, Vec<String>, String)> = paths
        .into_iter()
        .filter_map(|input| {
            presets::build_ffmpeg_args(&preset_id, &input, &cfg)
                .map(|(args, out)| (input, args, out.to_string_lossy().to_string()))
        })
        .collect();
    spawn_queue(&app, ffmpeg, jobs);
    Ok(())
}

#[tauri::command]
fn cancel_conversion(app: AppHandle) {
    ffmpeg::cancel_all(&app);
}

/// Запустить установку ffmpeg (скачивание + распаковка) в отдельном потоке.
#[tauri::command]
fn download_ffmpeg(app: AppHandle) {
    std::thread::spawn(move || {
        let _ = ffmpeg::download_ffmpeg(&app);
    });
}

// ── Настройки и ключ ─────────────────────────────────────────────────────────

#[tauri::command]
fn get_settings(app: AppHandle) -> Settings {
    settings::load(&app)
}

/// Сохранить настройки. Возвращает Some(предупреждение), если желаемый хоткей
/// занят и зарегистрирован запасной.
#[tauri::command]
fn save_settings(app: AppHandle, new_settings: Settings) -> Result<Option<String>, String> {
    let old = settings::load(&app);
    settings::save(&app, &new_settings)?;
    // Хоткей меняется на лету.
    #[cfg(desktop)]
    if old.hotkey != new_settings.hotkey {
        return apply_hotkey(&app, &new_settings.hotkey);
    }
    Ok(None)
}

// ── Интеграция с Windows (контекстное меню, автозапуск) ──────────────────────

#[tauri::command]
fn context_menu_status() -> bool {
    integration::context_menu_registered()
}

#[tauri::command]
fn set_context_menu(enabled: bool) -> Result<(), String> {
    if enabled {
        integration::register_context_menu()
    } else {
        integration::unregister_context_menu()
    }
}

#[tauri::command]
fn autostart_status() -> bool {
    integration::autostart_enabled()
}

#[tauri::command]
fn set_autostart(enabled: bool) -> Result<(), String> {
    integration::set_autostart(enabled)
}

// ── История ──────────────────────────────────────────────────────────────────

#[tauri::command]
fn get_history(app: AppHandle) -> Vec<settings::HistoryEntry> {
    settings::load_history(&app)
}

#[tauri::command]
fn add_history(app: AppHandle, kind: String, label: String, target_ext: String, files: usize) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = settings::push_history(
        &app,
        settings::HistoryEntry {
            ts,
            kind,
            label,
            target_ext,
            files,
        },
    );
}

#[tauri::command]
fn clear_history(app: AppHandle) -> Result<(), String> {
    settings::clear_history(&app)
}

#[tauri::command]
fn set_api_key(provider: String, key: String) -> Result<(), String> {
    settings::set_api_key(&provider, &key)
}

#[tauri::command]
fn has_api_key(provider: String) -> bool {
    settings::has_api_key(&provider)
}

// ── ИИ ───────────────────────────────────────────────────────────────────────

/// Метаданные файлов для контекста промпта (имя + вывод ffprobe).
fn probe_files(app: &AppHandle, paths: &[String]) -> Vec<(String, String)> {
    paths
        .iter()
        .map(|p| {
            let name = std::path::Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(p)
                .to_string();
            (name, ai::probe_metadata(app, p))
        })
        .collect()
}

/// Сгенерировать предложение ffmpeg-опций по запросу пользователя.
/// Команда асинхронная, а сетевой запрос уходит в blocking-пул: синхронная
/// команда выполнялась бы на главном потоке и замораживала окно на время HTTP.
#[tauri::command]
async fn ai_generate(
    app: AppHandle,
    query: String,
    paths: Vec<String>,
) -> Result<ai::AiSuggestion, String> {
    if query.trim().is_empty() {
        return Err("Введите запрос для ИИ".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = settings::load(&app);
        let api_key = settings::get_api_key(&cfg.provider);
        let files = probe_files(&app, &paths);
        ai::generate(&cfg, api_key, &query, &files)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Попросить ИИ исправить опции, на которых ffmpeg упал (авто-ретрай).
#[tauri::command]
async fn ai_fix(
    app: AppHandle,
    query: String,
    paths: Vec<String>,
    prev_options: Vec<String>,
    target_ext: String,
    error: String,
) -> Result<ai::AiSuggestion, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let cfg = settings::load(&app);
        let api_key = settings::get_api_key(&cfg.provider);
        let files = probe_files(&app, &paths);
        ai::generate_fix(&cfg, api_key, &query, &files, &prev_options, &target_ext, &error)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Предпросмотр команд по опциям ИИ (по строке на файл).
#[tauri::command]
fn ai_command_preview(
    app: AppHandle,
    options: Vec<String>,
    target_ext: String,
    paths: Vec<String>,
) -> Vec<String> {
    let cfg = out_cfg(&app);
    paths
        .iter()
        .filter_map(|input| {
            presets::build_ai_args(&options, &target_ext, input, &cfg)
                .ok()
                .map(|(args, _)| format!("ffmpeg {}", args.join(" ")))
        })
        .collect()
}

/// Запустить конвертацию по опциям, сгенерированным ИИ.
#[tauri::command]
fn run_ai_conversion(
    app: AppHandle,
    options: Vec<String>,
    target_ext: String,
    paths: Vec<String>,
) -> Result<(), String> {
    // Валидация до запуска — на весь набор опций.
    presets::validate_ai_options(&options)?;
    let ffmpeg = ffmpeg::resolve_ffmpeg(&app)
        .ok_or_else(|| "ffmpeg не найден. Скачайте его в приложении.".to_string())?;
    reset_jobs(&app);
    let cfg = out_cfg(&app);

    let mut jobs: Vec<(String, Vec<String>, String)> = Vec::new();
    for input in paths {
        match presets::build_ai_args(&options, &target_ext, &input, &cfg) {
            Ok((args, out)) => jobs.push((input, args, out.to_string_lossy().to_string())),
            Err(e) => return Err(e),
        }
    }
    spawn_queue(&app, ffmpeg, jobs);
    Ok(())
}

#[tauri::command]
fn hide_palette(app: AppHandle) {
    hide_window(&app);
}

// ── Точка входа ──────────────────────────────────────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default();

    // Single-instance регистрируется ПЕРВЫМ. Повторные запуски (напр. из контекстного
    // меню на нескольких файлах) прокидывают свои пути в уже работающий экземпляр.
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            let paths = extract_paths(&argv);
            update_context(app, paths);
        }));
    }

    builder
        .plugin(tauri_plugin_opener::init())
        .manage(ContextState {
            inner: Mutex::new(ContextInner {
                paths: vec![],
                last_update: None,
            }),
        })
        .manage(Jobs::default())
        .invoke_handler(tauri::generate_handler![
            get_context,
            get_recommendations,
            command_preview,
            ffmpeg_available,
            start_conversion,
            cancel_conversion,
            download_ffmpeg,
            get_settings,
            save_settings,
            set_api_key,
            has_api_key,
            ai_generate,
            ai_fix,
            ai_command_preview,
            run_ai_conversion,
            hide_palette,
            context_menu_status,
            set_context_menu,
            autostart_status,
            set_autostart,
            get_history,
            add_history,
            clear_history
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Контекст из аргументов первого запуска (запуск из контекстного меню).
            let args: Vec<String> = std::env::args().collect();
            let paths = extract_paths(&args);
            if !paths.is_empty() {
                update_context(&handle, paths);
            }

            // Прятать палитру при потере фокуса — только если включено в настройках.
            // По умолчанию выключено: скрытие по blur ломает перетаскивание файлов
            // из Проводника (клик по файлу уводит фокус — окно исчезало бы).
            if let Some(win) = app.get_webview_window("main") {
                let win2 = win.clone();
                let handle2 = handle.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::Focused(false) = event {
                        if settings::load(&handle2).hide_on_blur {
                            let _ = win2.hide();
                        }
                    }
                });
            }

            #[cfg(desktop)]
            {
                setup_tray(&handle)?;
                setup_global_shortcut(&handle)?;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

// ── Трей ─────────────────────────────────────────────────────────────────────

#[cfg(desktop)]
fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
    use tauri::Emitter;

    let show_i = MenuItem::with_id(app, "show", "Показать палитру", true, None::<&str>)?;
    let settings_i = MenuItem::with_id(app, "settings", "Настройки", true, None::<&str>)?;
    let quit_i = MenuItem::with_id(app, "quit", "Выход", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_i, &settings_i, &quit_i])?;

    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("ConvPalette")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => show_window(app),
            "quit" => app.exit(0),
            "settings" => {
                show_window(app);
                let _ = app.emit("open-settings", ());
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                toggle_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

// ── Глобальный хоткей ────────────────────────────────────────────────────────

/// Разобрать строку вида "ctrl+alt+space" в Shortcut.
#[cfg(desktop)]
fn parse_hotkey(spec: &str) -> Option<tauri_plugin_global_shortcut::Shortcut> {
    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut};

    let mut mods = Modifiers::empty();
    let mut code: Option<Code> = None;

    for token in spec.to_lowercase().split('+').map(str::trim) {
        match token {
            "ctrl" | "control" => mods |= Modifiers::CONTROL,
            "alt" => mods |= Modifiers::ALT,
            "shift" => mods |= Modifiers::SHIFT,
            "win" | "super" | "meta" => mods |= Modifiers::SUPER,
            "space" => code = Some(Code::Space),
            "f1" => code = Some(Code::F1),
            "f2" => code = Some(Code::F2),
            "f3" => code = Some(Code::F3),
            "f4" => code = Some(Code::F4),
            "f5" => code = Some(Code::F5),
            "f6" => code = Some(Code::F6),
            "f7" => code = Some(Code::F7),
            "f8" => code = Some(Code::F8),
            "f9" => code = Some(Code::F9),
            "f10" => code = Some(Code::F10),
            "f11" => code = Some(Code::F11),
            "f12" => code = Some(Code::F12),
            t if t.len() == 1 => {
                let ch = t.chars().next()?;
                code = Some(match ch {
                    'a' => Code::KeyA, 'b' => Code::KeyB, 'c' => Code::KeyC,
                    'd' => Code::KeyD, 'e' => Code::KeyE, 'f' => Code::KeyF,
                    'g' => Code::KeyG, 'h' => Code::KeyH, 'i' => Code::KeyI,
                    'j' => Code::KeyJ, 'k' => Code::KeyK, 'l' => Code::KeyL,
                    'm' => Code::KeyM, 'n' => Code::KeyN, 'o' => Code::KeyO,
                    'p' => Code::KeyP, 'q' => Code::KeyQ, 'r' => Code::KeyR,
                    's' => Code::KeyS, 't' => Code::KeyT, 'u' => Code::KeyU,
                    'v' => Code::KeyV, 'w' => Code::KeyW, 'x' => Code::KeyX,
                    'y' => Code::KeyY, 'z' => Code::KeyZ,
                    '0' => Code::Digit0, '1' => Code::Digit1, '2' => Code::Digit2,
                    '3' => Code::Digit3, '4' => Code::Digit4, '5' => Code::Digit5,
                    '6' => Code::Digit6, '7' => Code::Digit7, '8' => Code::Digit8,
                    '9' => Code::Digit9,
                    _ => return None,
                });
            }
            _ => return None,
        }
    }

    // Голая клавиша без модификаторов глобальным хоткеем быть не должна.
    if mods.is_empty() {
        return None;
    }
    code.map(|c| Shortcut::new(Some(mods), c))
}

/// Снять текущие хоткеи и зарегистрировать новый из настроек.
/// При неудаче пробуются запасные комбинации; Ok(Some(msg)) — сработал запасной.
#[cfg(desktop)]
fn apply_hotkey(app: &AppHandle, spec: &str) -> Result<Option<String>, String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;

    let gs = app.global_shortcut();
    let _ = gs.unregister_all();

    // Желаемый + запасные (основной часто занят другими программами).
    let mut specs = vec![spec.to_string()];
    for fallback in ["ctrl+alt+space", "ctrl+shift+space", "ctrl+alt+c", "alt+shift+c"] {
        if fallback != spec {
            specs.push(fallback.to_string());
        }
    }

    for s in &specs {
        if let Some(shortcut) = parse_hotkey(s) {
            if gs.register(shortcut).is_ok() {
                eprintln!("ConvPalette: глобальный хоткей зарегистрирован: {s}");
                if s != spec {
                    return Ok(Some(format!(
                        "Хоткей «{spec}» занят или некорректен — используется «{s}»"
                    )));
                }
                return Ok(None);
            }
        }
    }
    Err("Не удалось зарегистрировать ни один хоткей — используйте трей.".into())
}

#[cfg(desktop)]
fn setup_global_shortcut(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri_plugin_global_shortcut::ShortcutState;

    app.plugin(
        tauri_plugin_global_shortcut::Builder::new()
            .with_handler(move |app, _shortcut, event| {
                // Зарегистрирован только один хоткей, так что достаточно проверить нажатие.
                if event.state == ShortcutState::Pressed {
                    toggle_window(app);
                }
            })
            .build(),
    )?;

    // Хоткей может быть занят другим приложением — это не повод падать.
    // Палитра всё равно доступна из трея и из контекстного меню.
    let spec = settings::load(app).hotkey;
    if let Err(e) = apply_hotkey(app, &spec) {
        eprintln!("ConvPalette: {e}");
    }
    Ok(())
}
