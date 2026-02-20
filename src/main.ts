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
};

type AboutInfo = {
  title: string;
  version: string;
  app_host: string;
  app_url: string;
};

const app = document.querySelector<HTMLDivElement>("#app");
if (!app) {
  throw new Error("App root not found");
}

app.innerHTML = `
  <main class="shell">
    <section class="card">
      <h1>CRA Client</h1>
      <p id="subtitle" class="muted">Initializing desktop client...</p>

      <div id="status" class="status loading">Checking server reachability...</div>

      <div class="actions">
        <button id="retry" type="button" disabled>Retry</button>
        <button id="about" type="button">About</button>
      </div>

      <p id="details" class="details"></p>
    </section>

    <dialog id="aboutDialog">
      <form method="dialog">
        <h2>About CRA Client</h2>
        <p id="aboutBody"></p>
        <div class="actions">
          <button type="submit">Close</button>
        </div>
      </form>
    </dialog>
  </main>
`;

const statusEl = document.querySelector<HTMLDivElement>("#status");
const subtitleEl = document.querySelector<HTMLParagraphElement>("#subtitle");
const detailsEl = document.querySelector<HTMLParagraphElement>("#details");
const retryBtn = document.querySelector<HTMLButtonElement>("#retry");
const aboutBtn = document.querySelector<HTMLButtonElement>("#about");
const aboutDialog = document.querySelector<HTMLDialogElement>("#aboutDialog");
const aboutBody = document.querySelector<HTMLParagraphElement>("#aboutBody");

if (!statusEl || !subtitleEl || !detailsEl || !retryBtn || !aboutBtn || !aboutDialog || !aboutBody) {
  throw new Error("Missing required DOM elements");
}

function setStatus(kind: "loading" | "ok" | "error", message: string): void {
  statusEl.className = `status ${kind}`;
  statusEl.textContent = message;
}

function setDetails(message: string): void {
  detailsEl.textContent = message;
}

async function showAboutDialog(): Promise<void> {
  try {
    const info = await invoke<AboutInfo>("get_about_info");
    aboutBody.textContent = `${info.title}\nVersion: ${info.version}\nTarget Host: ${info.app_host}\nURL: ${info.app_url}`;
  } catch (error) {
    aboutBody.textContent = `About information unavailable: ${String(error)}`;
  }
  aboutDialog.showModal();
}

async function openRemoteApp(): Promise<void> {
  setStatus("loading", "Opening remote app...");
  setDetails("Please wait while the desktop client switches to the web application.");
  retryBtn.disabled = true;

  try {
    await invoke("launch_app");
  } catch (error) {
    setStatus("error", "Could not open the app.");
    setDetails(String(error));
    retryBtn.disabled = false;
  }
}

async function retryConnection(): Promise<void> {
  setStatus("loading", "Retrying connection...");
  setDetails("Attempting to reach the server again.");
  retryBtn.disabled = true;

  try {
    await invoke("retry_connect");
  } catch (error) {
    setStatus("error", "Server is still unreachable.");
    setDetails(String(error));
    retryBtn.disabled = false;
  }
}

async function bootstrap(): Promise<void> {
  setStatus("loading", "Checking configuration...");
  setDetails("Loading runtime settings and validating connectivity.");

  try {
    const state = await invoke<BootstrapState>("bootstrap_state");

    subtitleEl.textContent = `Version ${state.version}`;

    if (!state.ready || state.config_error) {
      setStatus("error", "Configuration error");
      setDetails(state.config_error ?? "Runtime configuration is incomplete.");
      retryBtn.disabled = true;
      return;
    }

    if (state.reachable) {
      await openRemoteApp();
      return;
    }

    setStatus("error", "Server unreachable");
    setDetails(state.reachability_error ?? "The server did not respond.");
    retryBtn.disabled = false;
  } catch (error) {
    setStatus("error", "Bootstrap failed");
    setDetails(String(error));
    retryBtn.disabled = false;
  }
}

retryBtn.addEventListener("click", () => {
  void retryConnection();
});

aboutBtn.addEventListener("click", () => {
  void showAboutDialog();
});

void bootstrap();
