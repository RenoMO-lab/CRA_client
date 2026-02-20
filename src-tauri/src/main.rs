#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tauri::{Manager, State, Window, WindowUrl};
use url::Url;

const DEFAULT_TITLE: &str = "CRA Client";
const DEFAULT_WIDTH: f64 = 1280.0;
const DEFAULT_HEIGHT: f64 = 800.0;
const DEFAULT_APP_URL: &str = "http://192.168.50.55:3000";
const DEFAULT_ALLOWED_HOSTS: &str = "192.168.50.55";

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
  state
    .config
    .as_ref()
    .cloned()
    .ok_or_else(|| {
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
  if status.is_success() || status.is_redirection() || status.as_u16() == 401 || status.as_u16() == 403 {
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

fn read_optional_value(key: &str, file_values: &HashMap<String, String>) -> Option<String> {
  if let Ok(value) = std::env::var(key) {
    let trimmed = value.trim();
    if !trimmed.is_empty() {
      return Some(trimmed.to_string());
    }
  }

  file_values
    .get(key)
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
}

fn read_required_value(key: &str, file_values: &HashMap<String, String>) -> Result<String, String> {
  read_optional_value(key, file_values)
    .ok_or_else(|| format!("Missing required setting: {key}. Set it via environment variable or client.env."))
}

fn parse_window_dimension(key: &str, fallback: f64, file_values: &HashMap<String, String>) -> Result<f64, String> {
  let Some(raw) = read_optional_value(key, file_values) else {
    return Ok(fallback);
  };

  raw
    .parse::<f64>()
    .map_err(|_| format!("{key} must be numeric, got '{raw}'."))
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
  std::env::var("APPDATA")
    .ok()
    .map(|app_data| PathBuf::from(app_data).join("CRA Client").join("client.env"))
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

fn load_runtime_config() -> Result<RuntimeConfig, String> {
  migrate_legacy_default_client_env_file()?;
  ensure_default_client_env_file()?;
  let file_values = load_client_env_values();

  let app_url_raw = read_required_value("APP_URL", &file_values)?;
  let app_url = Url::parse(&app_url_raw)
    .map_err(|error| format!("APP_URL must be a valid URL: {error}"))?;

  if app_url.scheme() != "http" && app_url.scheme() != "https" {
    return Err("APP_URL must use HTTP or HTTPS.".to_string());
  }

  let app_host = app_url
    .host_str()
    .ok_or_else(|| "APP_URL must include a host.".to_string())?;

  let allowed_hosts_raw = read_required_value("ALLOWED_HOSTS", &file_values)?;
  let allowed_hosts: HashSet<String> = allowed_hosts_raw
    .split(',')
    .map(normalize_host)
    .filter(|value| !value.is_empty())
    .collect();

  if allowed_hosts.is_empty() {
    return Err("ALLOWED_HOSTS must include at least one host.".to_string());
  }

  if !allowed_hosts.contains(&normalize_host(app_host)) {
    return Err("ALLOWED_HOSTS must include the APP_URL host.".to_string());
  }

  let window_title = read_optional_value("WINDOW_TITLE", &file_values)
    .unwrap_or_else(|| DEFAULT_TITLE.to_string());
  let window_width = parse_window_dimension("WINDOW_WIDTH", DEFAULT_WIDTH, &file_values)?;
  let window_height = parse_window_dimension("WINDOW_HEIGHT", DEFAULT_HEIGHT, &file_values)?;

  Ok(RuntimeConfig {
    app_url,
    allowed_hosts,
    window_title,
    window_width,
    window_height,
  })
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
  let app_state = match load_runtime_config() {
    Ok(config) => AppState {
      config: Some(config),
      config_error: None,
    },
    Err(error) => AppState {
      config: None,
      config_error: Some(error),
    },
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
      let app_icon = tauri::Icon::Raw(include_bytes!("../icons/icon.png").to_vec());

      let mut window_builder = tauri::WindowBuilder::new(app, "main", WindowUrl::App("index.html".into()))
        .title(window_title)
        .inner_size(window_width, window_height)
        .resizable(true)
        .initialization_script(INIT_SCRIPT)
        .on_navigation(move |url| {
          if is_allowed_navigation(&url, &allowed_hosts) {
            return true;
          }

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
