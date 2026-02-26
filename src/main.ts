import "./styles.css";
import { invoke } from "@tauri-apps/api/tauri";

type BootstrapState = {
  ready: boolean;
  config_error: string | null;
  app_url: string | null;
  app_host: string | null;
  window_title: string;
  window_width: number;
  window_height: number;
  version: string;
  reachable: boolean;
  reachability_error: string | null;
  web_build_hash?: string | null;
  web_build_time?: string | null;
  required_web_build_hash?: string | null;
  build_parity_ok: boolean;
  build_parity_error?: string | null;
  enforce_web_build: boolean;
};

type AboutInfo = {
  title: string;
  version: string;
  app_host: string;
  app_url: string;
  required_web_build_hash?: string | null;
  enforce_web_build: boolean;
  web_build_hash?: string | null;
  web_build_time?: string | null;
  web_build_error?: string | null;
};

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) {
  throw new Error("App root not found");
}

app.innerHTML = `
  <main class="shell">
    <section class="startup-card">
      <div class="spinner" aria-hidden="true"></div>
      <h1>CRA</h1>
      <p id="subtitle" class="muted">Connecting to server...</p>
      <div id="status" class="status loading">Checking server reachability...</div>
      <p id="details" class="details hidden"></p>

      <div id="actions" class="actions hidden">
        <button id="retry" type="button" disabled>Retry</button>
        <button id="about" type="button">About</button>
      </div>
    </section>

    <dialog id="aboutDialog">
      <form method="dialog">
        <h2>About CRA</h2>
        <p id="aboutBody"></p>
        <div class="actions">
          <button type="submit">Close</button>
        </div>
      </form>
    </dialog>
  </main>
`;

function requiredElement<T extends Element>(selector: string): T {
  const element = document.querySelector<T>(selector);
  if (!element) {
    throw new Error(`Missing required DOM element: ${selector}`);
  }
  return element;
}

const status = requiredElement<HTMLDivElement>("#status");
const subtitle = requiredElement<HTMLParagraphElement>("#subtitle");
const details = requiredElement<HTMLParagraphElement>("#details");
const actions = requiredElement<HTMLDivElement>("#actions");
const retry = requiredElement<HTMLButtonElement>("#retry");
const about = requiredElement<HTMLButtonElement>("#about");
const aboutDialog = requiredElement<HTMLDialogElement>("#aboutDialog");
const aboutBody = requiredElement<HTMLParagraphElement>("#aboutBody");

let windowVisible = false;

function setStatus(kind: "loading" | "ok" | "warning" | "error", message: string): void {
  status.className = `status ${kind}`;
  status.textContent = message;
}

function setDetails(message: string): void {
  details.textContent = message;
}

function setLoaderMode(): void {
  details.classList.add("hidden");
  actions.classList.add("hidden");
  retry.disabled = true;
}

function setErrorMode(message: string): void {
  setDetails(message);
  details.classList.remove("hidden");
  actions.classList.remove("hidden");
  retry.disabled = false;
}

async function ensureMainWindowVisible(): Promise<void> {
  if (windowVisible) {
    return;
  }
  try {
    await invoke("show_main_window");
    windowVisible = true;
  } catch {
    // Keep retry flow available even if visibility command fails.
  }
}

async function showAboutDialog(): Promise<void> {
  try {
    const info = await invoke<AboutInfo>("get_about_info");
    const lines = [
      `${info.title}`,
      `Version: ${info.version}`,
      `Target Host: ${info.app_host}`,
      `URL: ${info.app_url}`,
      `Web Build Hash: ${info.web_build_hash ?? "-"}`,
      `Web Build Time: ${info.web_build_time ?? "-"}`,
      `Required Build Hash: ${info.required_web_build_hash ?? "-"}`,
      `Enforce Build Parity: ${info.enforce_web_build ? "true" : "false"}`,
    ];
    if (info.web_build_error) {
      lines.push(`Build Check Error: ${info.web_build_error}`);
    }
    aboutBody.textContent = lines.join("\n");
  } catch (error) {
    aboutBody.textContent = `About information unavailable: ${String(error)}`;
  }
  aboutDialog.showModal();
}

async function openRemoteApp(): Promise<void> {
  setStatus("loading", "Opening remote app...");
  setLoaderMode();

  try {
    await invoke("launch_app");
  } catch (error) {
    await ensureMainWindowVisible();
    setStatus("error", "Could not open the app.");
    setErrorMode(String(error));
  }
}

async function retryConnection(): Promise<void> {
  setStatus("loading", "Retrying connection...");
  setLoaderMode();

  try {
    await invoke("retry_connect");
  } catch (error) {
    await ensureMainWindowVisible();
    setStatus("error", "Server is still unreachable.");
    setErrorMode(String(error));
  }
}

async function bootstrap(): Promise<void> {
  setStatus("loading", "Checking configuration...");
  setLoaderMode();

  try {
    const state = await invoke<BootstrapState>("bootstrap_state");

    subtitle.textContent = `Version ${state.version} • Web ${state.web_build_hash ?? "-"}`;

    if (!state.ready || state.config_error) {
      await ensureMainWindowVisible();
      setStatus("error", "Configuration error");
      setErrorMode(state.config_error ?? "Runtime configuration is incomplete.");
      retry.disabled = true;
      return;
    }

    if (state.reachable) {
      if (!state.build_parity_ok) {
        const message =
          state.build_parity_error ??
          "Server build is older than required for this CRA Client.";
        const withHints = `${message}\nConnected build: ${state.web_build_hash ?? "-"}\nRequired build: ${state.required_web_build_hash ?? "-"}`;

        if (state.enforce_web_build) {
          await ensureMainWindowVisible();
          setStatus("error", "Build parity check failed");
          setErrorMode(withHints);
          return;
        }

        setStatus("warning", "Build parity warning");
        setDetails(withHints);
        details.classList.remove("hidden");
      }

      await openRemoteApp();
      return;
    }

    await ensureMainWindowVisible();
    setStatus("error", "Server unreachable");
    setErrorMode(state.reachability_error ?? "The server did not respond.");
  } catch (error) {
    await ensureMainWindowVisible();
    setStatus("error", "Bootstrap failed");
    setErrorMode(String(error));
  }
}

retry.addEventListener("click", () => {
  void retryConnection();
});

about.addEventListener("click", () => {
  void showAboutDialog();
});

void bootstrap();
