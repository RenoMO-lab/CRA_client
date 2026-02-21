#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{Manager, State, Window, WindowUrl};
use url::Url;

const DEFAULT_TITLE: &str = "CRA Client";
const DEFAULT_WIDTH: f64 = 1280.0;
const DEFAULT_HEIGHT: f64 = 800.0;
const DEFAULT_APP_URL: &str = "http://192.168.50.55:3000";
const DEFAULT_ALLOWED_HOSTS: &str = "192.168.50.55";
const ENV_APP_URL: &str = "CRA_CLIENT_APP_URL";
const ENV_ALLOWED_HOSTS: &str = "CRA_CLIENT_ALLOWED_HOSTS";
const ENV_WINDOW_TITLE: &str = "CRA_CLIENT_WINDOW_TITLE";
const ENV_WINDOW_WIDTH: &str = "CRA_CLIENT_WINDOW_WIDTH";
const ENV_WINDOW_HEIGHT: &str = "CRA_CLIENT_WINDOW_HEIGHT";
const ENV_ALLOW_LOCALHOST_RELEASE: &str = "CRA_CLIENT_ALLOW_LOCALHOST_RELEASE";

const INIT_SCRIPT: &str = r#"
(() => {
  const invoke = (cmd, payload = {}) => {
    const tauriObj = window.__TAURI__;
    if (tauriObj?.invoke) {
      return tauriObj.invoke(cmd, payload);
    }
    if (tauriObj?.core?.invoke) {
      return tauriObj.core.invoke(cmd, payload);
    }
    return Promise.reject(new Error('Tauri invoke bridge unavailable'));
  };

  window.open = (url) => {
    if (typeof url === 'string' && url.length > 0) {
      window.location.assign(url);
    }
    return null;
  };

  document.addEventListener(
    'click',
    (event) => {
      const target = event.target;
      if (!(target instanceof Element)) {
        return;
      }
      const link = target.closest('a');
      if (!(link instanceof HTMLAnchorElement)) {
        return;
      }
      const href = link.href;
      if (!href) {
        return;
      }
      if (link.target === '_blank') {
        event.preventDefault();
        window.location.assign(href);
      }
    },
    true,
  );

  window.addEventListener('keydown', (event) => {
    if (event.altKey && event.shiftKey && event.code === 'KeyA') {
      void invoke('get_about_info').then((info) => {
        alert(`${info.title}\nVersion: ${info.version}\nTarget Host: ${info.app_host}`);
      });
    }
  });
})();
"#;

#[derive(Clone, Debug)]
struct RuntimeConfig {
    app_url: Url,
    allowed_hosts: HashSet<String>,
    window_title: String,
    window_width: f64,
    window_height: f64,
}

#[derive(Clone, Debug)]
struct AppState {
    config: Option<RuntimeConfig>,
    config_error: Option<String>,
}

#[derive(Serialize)]
struct BootstrapState {
    ready: bool,
    config_error: Option<String>,
    app_url: Option<String>,
    app_host: Option<String>,
    window_title: String,
    window_width: f64,
    window_height: f64,
    version: String,
    reachable: bool,
    reachability_error: Option<String>,
}

#[derive(Serialize)]
struct AboutInfo {
    title: String,
    version: String,
    app_host: String,
    app_url: String,
}

#[tauri::command]
async fn bootstrap_state(state: State<'_, AppState>) -> Result<BootstrapState, String> {
    let version = env!("CARGO_PKG_VERSION").to_string();

    if let Some(config_error) = &state.config_error {
        return Ok(BootstrapState {
            ready: false,
            config_error: Some(config_error.clone()),
            app_url: None,
            app_host: None,
            window_title: DEFAULT_TITLE.to_string(),
            window_width: DEFAULT_WIDTH,
            window_height: DEFAULT_HEIGHT,
            version,
            reachable: false,
            reachability_error: None,
        });
    }

    let Some(config) = &state.config else {
        return Ok(BootstrapState {
            ready: false,
            config_error: Some("Runtime configuration is missing.".to_string()),
            app_url: None,
            app_host: None,
            window_title: DEFAULT_TITLE.to_string(),
            window_width: DEFAULT_WIDTH,
            window_height: DEFAULT_HEIGHT,
            version,
            reachable: false,
            reachability_error: None,
        });
    };

    let reachability = check_server_reachable(&config.app_url).await;

    Ok(BootstrapState {
        ready: true,
        config_error: None,
        app_url: Some(config.app_url.to_string()),
        app_host: config.app_url.host_str().map(ToString::to_string),
        window_title: config.window_title.clone(),
        window_width: config.window_width,
        window_height: config.window_height,
        version,
        reachable: reachability.is_ok(),
        reachability_error: reachability.err(),
    })
}

#[tauri::command]
async fn launch_app(window: Window, state: State<'_, AppState>) -> Result<(), String> {
    let config = get_config(&state)?;
    check_server_reachable(&config.app_url).await?;
    let target = config
        .app_url
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");

    window
        .eval(&format!("window.location.replace(\"{}\");", target))
        .map_err(|error| format!("Failed to navigate to APP_URL: {error}"))
}

#[tauri::command]
async fn retry_connect(window: Window, state: State<'_, AppState>) -> Result<(), String> {
    launch_app(window, state).await
}

#[tauri::command]
fn get_about_info(state: State<'_, AppState>) -> AboutInfo {
    if let Some(config) = &state.config {
        return AboutInfo {
            title: config.window_title.clone(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            app_host: config
                .app_url
                .host_str()
                .unwrap_or("unknown-host")
                .to_string(),
            app_url: config.app_url.to_string(),
        };
    }

    AboutInfo {
        title: DEFAULT_TITLE.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        app_host: "not-configured".to_string(),
        app_url: "not-configured".to_string(),
    }
}

fn get_config(state: &AppState) -> Result<RuntimeConfig, String> {
    state.config.as_ref().cloned().ok_or_else(|| {
        state
            .config_error
            .clone()
            .unwrap_or_else(|| "Runtime configuration missing.".to_string())
    })
}

async fn check_server_reachable(url: &Url) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(8))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|error| format!("HTTP client init failed: {error}"))?;

    let response = client
        .get(url.clone())
        .send()
        .await
        .map_err(|error| format!("Could not reach server at {url}: {error}"))?;

    let status = response.status();
    if status.is_success()
        || status.is_redirection()
        || status.as_u16() == 401
        || status.as_u16() == 403
    {
        return Ok(());
    }

    Err(format!(
        "Server responded with status {} when requesting {}",
        status, url
    ))
}

fn normalize_host(value: &str) -> String {
    value.trim().to_ascii_lowercase()
}

fn current_timestamp() -> String {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs().to_string(),
        Err(_) => "0".to_string(),
    }
}

fn appdata_logs_dir_path() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .map(|app_data| PathBuf::from(app_data).join("CRA Client").join("logs"))
}

fn startup_log_path() -> Option<PathBuf> {
    appdata_logs_dir_path().map(|path| path.join("startup.log"))
}

fn append_startup_log_entry(message: &str) {
    let Some(log_path) = startup_log_path() else {
        return;
    };

    if let Some(parent) = log_path.parent() {
        if fs::create_dir_all(parent).is_err() {
            return;
        }
    }

    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(log_path) {
        let _ = writeln!(file, "{message}");
    }
}

fn read_process_env_value(key: &str) -> Option<String> {
    std::env::var(key).ok().and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn read_file_value(key: &str, file_values: &HashMap<String, String>) -> Option<String> {
    file_values
        .get(key)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn read_optional_value(
    file_key: &str,
    env_key: Option<&str>,
    file_values: &HashMap<String, String>,
) -> Option<(String, String)> {
    if let Some(key) = env_key {
        if let Some(value) = read_process_env_value(key) {
            return Some((value, format!("process env {key}")));
        }
    }

    read_file_value(file_key, file_values).map(|value| (value, format!("client.env {file_key}")))
}

fn read_required_value(
    file_key: &str,
    env_key: Option<&str>,
    file_values: &HashMap<String, String>,
) -> Result<(String, String), String> {
    read_optional_value(file_key, env_key, file_values).ok_or_else(|| {
        format!(
            "Missing required setting: {file_key}. Set it via {} or client.env.",
            env_key.unwrap_or("environment variable")
        )
    })
}

fn parse_bool_value(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

fn read_bool_value(
    file_key: &str,
    env_key: Option<&str>,
    fallback: bool,
    file_values: &HashMap<String, String>,
) -> Result<(bool, String), String> {
    let Some((raw, source)) = read_optional_value(file_key, env_key, file_values) else {
        return Ok((fallback, format!("default {fallback}")));
    };

    let Some(value) = parse_bool_value(&raw) else {
        return Err(format!(
            "{file_key} must be a boolean (true/false/1/0), got '{raw}'."
        ));
    };

    Ok((value, source))
}

fn parse_window_dimension(
    file_key: &str,
    env_key: Option<&str>,
    fallback: f64,
    file_values: &HashMap<String, String>,
) -> Result<(f64, String), String> {
    let Some((raw, source)) = read_optional_value(file_key, env_key, file_values) else {
        return Ok((fallback, format!("default {fallback}")));
    };

    raw.parse::<f64>()
        .map(|value| (value, source))
        .map_err(|_| format!("{file_key} must be numeric, got '{raw}'."))
}

fn candidate_client_env_files() -> Vec<PathBuf> {
    let mut files = Vec::new();

    files.push(PathBuf::from("client.env"));

    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            files.push(dir.join("client.env"));
        }
    }

    if let Some(path) = appdata_client_env_path() {
        files.push(path);
    }

    files
}

fn appdata_client_env_path() -> Option<PathBuf> {
    std::env::var("APPDATA").ok().map(|app_data| {
        PathBuf::from(app_data)
            .join("CRA Client")
            .join("client.env")
    })
}

fn default_client_env_contents() -> String {
    format!(
        "# Auto-generated default configuration for CRA Client.\n\
# Update APP_URL and ALLOWED_HOSTS if your deployment target changes.\n\
APP_URL={}\n\
ALLOWED_HOSTS={}\n\
WINDOW_TITLE={}\n\
WINDOW_WIDTH={}\n\
WINDOW_HEIGHT={}\n",
        DEFAULT_APP_URL,
        DEFAULT_ALLOWED_HOSTS,
        DEFAULT_TITLE,
        DEFAULT_WIDTH as i64,
        DEFAULT_HEIGHT as i64
    )
}

fn ensure_default_client_env_file() -> Result<(), String> {
    let Some(path) = appdata_client_env_path() else {
        return Ok(());
    };

    if path.exists() {
        return Ok(());
    }

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Could not create config directory '{}': {error}",
                parent.display()
            )
        })?;
    }

    fs::write(&path, default_client_env_contents()).map_err(|error| {
        format!(
            "Could not create default config file '{}': {error}",
            path.display()
        )
    })?;

    Ok(())
}

fn migrate_legacy_default_client_env_file() -> Result<(), String> {
    let Some(path) = appdata_client_env_path() else {
        return Ok(());
    };

    if !path.exists() {
        return Ok(());
    }

    let content = match fs::read_to_string(&path) {
        Ok(value) => value,
        Err(_) => return Ok(()),
    };

    if !content.contains("# Auto-generated default configuration for CRA Client.") {
        return Ok(());
    }

    let legacy = "APP_URL=https://192.168.50.55";
    if !content.contains(legacy) {
        return Ok(());
    }

    let updated = content.replace(legacy, "APP_URL=http://192.168.50.55:3000");
    fs::write(&path, updated).map_err(|error| {
        format!(
            "Could not migrate legacy config file '{}': {error}",
            path.display()
        )
    })?;

    Ok(())
}

fn parse_client_env_file(content: &str, output: &mut HashMap<String, String>) {
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }

        if let Some((key, value)) = trimmed.split_once('=') {
            let key_trimmed = key.trim();
            if key_trimmed.is_empty() {
                continue;
            }

            let cleaned_value = value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string();

            output.insert(key_trimmed.to_string(), cleaned_value);
        }
    }
}

fn load_client_env_values() -> HashMap<String, String> {
    let mut values = HashMap::new();

    for file in candidate_client_env_files() {
        if let Ok(content) = fs::read_to_string(file) {
            parse_client_env_file(&content, &mut values);
        }
    }

    values
}

fn load_runtime_config() -> (Result<RuntimeConfig, String>, Vec<String>) {
    let mut diagnostics = vec![
        format!("timestamp={}", current_timestamp()),
        format!("version={}", env!("CARGO_PKG_VERSION")),
    ];

    if let Err(error) = migrate_legacy_default_client_env_file() {
        diagnostics.push(format!(
            "migrate_legacy_default_client_env_file=error:{error}"
        ));
        return (Err(error), diagnostics);
    }

    if let Err(error) = ensure_default_client_env_file() {
        diagnostics.push(format!("ensure_default_client_env_file=error:{error}"));
        return (Err(error), diagnostics);
    }

    let file_values = load_client_env_values();

    let (app_url_raw, app_url_source) =
        match read_required_value("APP_URL", Some(ENV_APP_URL), &file_values) {
            Ok(value) => value,
            Err(error) => {
                diagnostics.push(format!(
                    "app_url_source=missing ({ENV_APP_URL} or APP_URL in client.env)"
                ));
                return (Err(error), diagnostics);
            }
        };
    diagnostics.push(format!("app_url_source={app_url_source}"));

    let app_url = match Url::parse(&app_url_raw) {
        Ok(value) => value,
        Err(error) => {
            return (
                Err(format!("APP_URL must be a valid URL: {error}")),
                diagnostics,
            )
        }
    };

    if app_url.scheme() != "http" && app_url.scheme() != "https" {
        return (
            Err("APP_URL must use HTTP or HTTPS.".to_string()),
            diagnostics,
        );
    }

    let app_host = match app_url.host_str() {
        Some(value) => value,
        None => return (Err("APP_URL must include a host.".to_string()), diagnostics),
    };
    let normalized_app_host = normalize_host(app_host);

    let (allowed_hosts_raw, allowed_hosts_source) =
        match read_required_value("ALLOWED_HOSTS", Some(ENV_ALLOWED_HOSTS), &file_values) {
            Ok(value) => value,
            Err(error) => {
                diagnostics.push(format!(
          "allowed_hosts_source=missing ({ENV_ALLOWED_HOSTS} or ALLOWED_HOSTS in client.env)"
        ));
                return (Err(error), diagnostics);
            }
        };
    diagnostics.push(format!("allowed_hosts_source={allowed_hosts_source}"));

    let allowed_hosts: HashSet<String> = allowed_hosts_raw
        .split(',')
        .map(normalize_host)
        .filter(|value| !value.is_empty())
        .collect();

    if allowed_hosts.is_empty() {
        return (
            Err("ALLOWED_HOSTS must include at least one host.".to_string()),
            diagnostics,
        );
    }

    if !allowed_hosts.contains(&normalized_app_host) {
        return (
            Err("ALLOWED_HOSTS must include the APP_URL host.".to_string()),
            diagnostics,
        );
    }

    let (allow_localhost_release, allow_localhost_release_source) = match read_bool_value(
        ENV_ALLOW_LOCALHOST_RELEASE,
        Some(ENV_ALLOW_LOCALHOST_RELEASE),
        false,
        &file_values,
    ) {
        Ok(value) => value,
        Err(error) => return (Err(error), diagnostics),
    };
    diagnostics.push(format!(
        "localhost_release_override={} ({allow_localhost_release_source})",
        allow_localhost_release
    ));

    if !cfg!(debug_assertions) {
        let blocked_release_localhost = matches!(
            normalized_app_host.as_str(),
            "localhost" | "127.0.0.1" | "::1" | "tauri.localhost"
        );
        if blocked_release_localhost && !allow_localhost_release {
            diagnostics.push("release_localhost_guard=blocked".to_string());
            return (
        Err(
          "APP_URL host resolves to localhost in release build. Use a non-localhost target, or set CRA_CLIENT_ALLOW_LOCALHOST_RELEASE=true for diagnostic builds."
            .to_string(),
        ),
        diagnostics,
      );
        }
        diagnostics.push("release_localhost_guard=pass".to_string());
    } else {
        diagnostics.push("release_localhost_guard=debug-skip".to_string());
    }

    let (window_title, window_title_source) =
        read_optional_value("WINDOW_TITLE", Some(ENV_WINDOW_TITLE), &file_values).unwrap_or_else(
            || {
                (
                    DEFAULT_TITLE.to_string(),
                    format!("default {DEFAULT_TITLE}"),
                )
            },
        );
    diagnostics.push(format!("window_title_source={window_title_source}"));

    let (window_width, window_width_source) = match parse_window_dimension(
        "WINDOW_WIDTH",
        Some(ENV_WINDOW_WIDTH),
        DEFAULT_WIDTH,
        &file_values,
    ) {
        Ok(value) => value,
        Err(error) => return (Err(error), diagnostics),
    };
    diagnostics.push(format!("window_width_source={window_width_source}"));

    let (window_height, window_height_source) = match parse_window_dimension(
        "WINDOW_HEIGHT",
        Some(ENV_WINDOW_HEIGHT),
        DEFAULT_HEIGHT,
        &file_values,
    ) {
        Ok(value) => value,
        Err(error) => return (Err(error), diagnostics),
    };
    diagnostics.push(format!("window_height_source={window_height_source}"));

    diagnostics.push(format!("resolved_app_url={app_url}"));
    diagnostics.push(format!("resolved_allowed_hosts={}", {
        let mut hosts: Vec<String> = allowed_hosts.iter().cloned().collect();
        hosts.sort();
        hosts.join(",")
    }));

    (
        Ok(RuntimeConfig {
            app_url,
            allowed_hosts,
            window_title,
            window_width,
            window_height,
        }),
        diagnostics,
    )
}

fn is_internal_navigation_host(host: &str) -> bool {
    matches!(host, "tauri.localhost" | "localhost" | "127.0.0.1" | "::1")
}

fn is_allowed_navigation(url: &Url, allowed_hosts: &HashSet<String>) -> bool {
    match url.scheme() {
        "tauri" | "asset" | "about" | "data" | "blob" => true,
        "http" | "https" => url
            .host_str()
            .map(normalize_host)
            .map(|host| is_internal_navigation_host(&host) || allowed_hosts.contains(&host))
            .unwrap_or(false),
        _ => false,
    }
}

fn main() {
    let (runtime_config_result, startup_diagnostics) = load_runtime_config();

    append_startup_log_entry("----- CRA Client startup -----");
    for entry in &startup_diagnostics {
        append_startup_log_entry(entry);
    }

    let app_state = match runtime_config_result {
        Ok(config) => {
            append_startup_log_entry("startup_result=ok");
            AppState {
                config: Some(config),
                config_error: None,
            }
        }
        Err(error) => {
            append_startup_log_entry(&format!("startup_result=error:{error}"));
            AppState {
                config: None,
                config_error: Some(error),
            }
        }
    };

    tauri::Builder::default()
        .manage(app_state)
        .setup(|app| {
            let state = app.state::<AppState>();
            let config = state.config.clone();

            let window_title = config
                .as_ref()
                .map(|value| value.window_title.clone())
                .unwrap_or_else(|| DEFAULT_TITLE.to_string());
            let window_width = config
                .as_ref()
                .map(|value| value.window_width)
                .unwrap_or(DEFAULT_WIDTH);
            let window_height = config
                .as_ref()
                .map(|value| value.window_height)
                .unwrap_or(DEFAULT_HEIGHT);
            let allowed_hosts = config
                .as_ref()
                .map(|value| value.allowed_hosts.clone())
                .unwrap_or_default();
            let mut allowed_hosts_for_log: Vec<String> = allowed_hosts.iter().cloned().collect();
            allowed_hosts_for_log.sort();
            let allowed_hosts_for_log = allowed_hosts_for_log.join(",");
            let app_icon = tauri::Icon::Raw(include_bytes!("../icons/icon.png").to_vec());

            let mut window_builder =
                tauri::WindowBuilder::new(app, "main", WindowUrl::App("index.html".into()))
                    .title(window_title)
                    .inner_size(window_width, window_height)
                    .resizable(true)
                    .initialization_script(INIT_SCRIPT)
                    .on_navigation(move |url| {
                        if is_allowed_navigation(&url, &allowed_hosts) {
                            return true;
                        }

                        append_startup_log_entry(&format!(
                            "blocked_navigation timestamp={} url={} allowed_hosts={}",
                            current_timestamp(),
                            url,
                            allowed_hosts_for_log
                        ));
                        false
                    });

            window_builder = window_builder
                .icon(app_icon)
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;

            window_builder
                .build()
                .map_err(|error| -> Box<dyn std::error::Error> { Box::new(error) })?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_state,
            launch_app,
            retry_connect,
            get_about_info
        ])
        .run(tauri::generate_context!())
        .expect("error while running CRA Client desktop app");
}
