#!/usr/bin/env bun
/**
 * End-to-end scenarios against the real desktop app, driven through the Tauri
 * MCP bridge (WebSocket, debug builds only).
 *
 * The app must be running with `withGlobalTauri` (it is) and a throwaway
 * profile: the script creates the vault itself, so point XDG_DATA_HOME at an
 * empty directory. Nothing here talks to the network.
 *
 * Usage:
 *   XDG_DATA_HOME=/tmp/akooti-e2e bun run tauri dev &
 *   bun scripts/e2e/app.e2e.mjs
 */

const WS_URL = process.env.AKOOTI_E2E_WS ?? "ws://127.0.0.1:9223";
const ARTIFACTS = process.env.AKOOTI_E2E_ARTIFACTS ?? "artifacts/e2e";
const PASSWORD = "akooti-e2e-master-2026";
const PERSONAL = "00000000-0000-4000-8000-000000000001";
const BACKUP_DIR = process.env.AKOOTI_E2E_BACKUP_DIR ?? "/tmp/akooti-e2e-backups";

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

class Bridge {
  constructor(url) {
    this.url = url;
    this.counter = 0;
    this.pending = new Map();
  }

  async connect() {
    this.socket = new WebSocket(this.url);
    await new Promise((resolve, reject) => {
      this.socket.onopen = resolve;
      this.socket.onerror = () => reject(new Error(`bridge unreachable at ${this.url}`));
    });
    this.socket.onmessage = (event) => {
      const message = JSON.parse(event.data);
      const waiter = this.pending.get(message.id);
      if (waiter) {
        this.pending.delete(message.id);
        waiter(message);
      }
    };
  }

  call(command, args = {}) {
    const id = String(++this.counter);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id);
        reject(new Error(`${command} timed out`));
      }, 30_000);
      this.pending.set(id, (message) => {
        clearTimeout(timer);
        if (message.success === false || message.error) {
          reject(new Error(String(message.error ?? `${command} failed`)));
        } else {
          resolve(message.data);
        }
      });
      this.socket.send(JSON.stringify({ id, command, args }));
    });
  }

  close() {
    this.socket?.close();
  }
}

const bridge = new Bridge(WS_URL);
const js = (script) => bridge.call("execute_js", { script });
/** Calls an app command through the webview's own IPC (capabilities apply). */
const invoke = (command, args = {}) =>
  js(`(() => window.__TAURI__.core.invoke(${JSON.stringify(command)}, ${JSON.stringify(args)}))()`);

async function bodyText() {
  const text = await js("document.body.innerText");
  return typeof text === "string" ? text : "";
}

async function waitFor(check, label, timeout = 20_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await check()) {
      return;
    }
    await sleep(250);
  }
  throw new Error(`timed out waiting for ${label}`);
}

const waitText = (needle, timeout) =>
  waitFor(async () => (await bodyText()).includes(needle), `“${needle}”`, timeout);

async function clickText(label) {
  const clicked = await js(`(() => {
    const target = ${JSON.stringify(label)};
    const button = [...document.querySelectorAll("button")].find(
      (candidate) => candidate.textContent.trim() === target,
    );
    if (!button) return false;
    button.click();
    return true;
  })()`);
  if (!clicked) {
    throw new Error(`button “${label}” not found`);
  }
}

async function clickPrefix(prefix) {
  const clicked = await js(`(() => {
    const target = ${JSON.stringify(prefix)};
    const button = [...document.querySelectorAll("button")].find((candidate) =>
      candidate.textContent.trim().startsWith(target),
    );
    if (!button) return false;
    button.click();
    return true;
  })()`);
  if (!clicked) {
    throw new Error(`button starting with “${prefix}” not found`);
  }
}

async function clickSubmit() {
  const clicked = await js(`(() => {
    const button = document.querySelector("button[type=submit]");
    if (!button) return false;
    button.click();
    return true;
  })()`);
  if (!clicked) {
    throw new Error("submit button not found");
  }
}

async function setInput(selector, value, index = 0) {
  const done = await js(`(() => {
    const input = document.querySelectorAll(${JSON.stringify(selector)})[${index}];
    if (!input) return false;
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value").set;
    setter.call(input, ${JSON.stringify(value)});
    input.dispatchEvent(new Event("input", { bubbles: true }));
    return true;
  })()`);
  if (!done) {
    throw new Error(`input ${selector}[${index}] not found`);
  }
}

async function screenshot(path) {
  try {
    const dataUrl = await bridge.call("capture_native_screenshot", { format: "png" });
    if (typeof dataUrl === "string" && dataUrl.startsWith("data:")) {
      const base64 = dataUrl.slice(dataUrl.indexOf(",") + 1);
      await Bun.write(path, Buffer.from(base64, "base64"));
    }
  } catch {
    // A screenshot is a convenience: never mask the real failure.
  }
}

const scenarios = [];
const scenario = (name, run) => scenarios.push({ name, run });

scenario("création du coffre", async () => {
  const status = await invoke("vault_status");
  if (status?.hasVault) {
    if (status.status !== "unlocked") {
      // The webview can lag behind a backend lock driven by IPC: resync it.
      if (!(await js("!!document.querySelector('input[type=password]')"))) {
        await js("location.reload()");
        await sleep(2_000);
      }
      await setInput("input[type=password]", PASSWORD, 0);
      await clickSubmit();
    }
    await waitText("Personnel", 30_000);
    return;
  }
  await waitText("Créer le coffre");
  await setInput("input[type=password]", PASSWORD, 0);
  await setInput("input[type=password]", PASSWORD, 1);
  await clickText("Créer le coffre");
  await waitText("Personnel", 30_000);
});

scenario("onboarding au premier déverrouillage", async () => {
  if (!(await bodyText()).includes("Bienvenue dans Akooti")) {
    // Already dismissed on this profile (the flag is per machine).
    return;
  }
  await clickText("C'est parti");
  await waitFor(
    async () => !(await bodyText()).includes("Bienvenue dans Akooti"),
    "onboarding closed",
  );
});

scenario("ajout d'une entrée", async () => {
  const item = await invoke("create_item", {
    draft: {
      itemType: "login",
      title: "E2E — portail",
      primaryUrl: "https://e2e.test",
      username: "e2e@test.local",
      password: "E2E-secret-42!",
      selectors: {},
      favorite: false,
      folderIds: [PERSONAL],
      uris: [],
      customFields: [],
    },
  });
  if (!item?.id) {
    throw new Error("create_item returned no id");
  }
  const items = await invoke("list_items", { filter: null });
  if (!Array.isArray(items) || !items.some((item) => item.title === "E2E — portail")) {
    throw new Error(`unexpected item list: ${JSON.stringify(items)}`);
  }
});

scenario("verrouillage puis déverrouillage", async () => {
  await clickPrefix("Verrouiller");
  await waitFor(
    async () => (await js("!!document.querySelector('input[type=password]')")) === true,
    "unlock screen",
  );
  await setInput("input[type=password]", PASSWORD, 0);
  await clickSubmit();
  await waitText("Personnel", 30_000);
  const items = await invoke("list_items", { filter: null });
  if (!Array.isArray(items) || !items.some((item) => item.title === "E2E — portail")) {
    throw new Error("the item did not survive the lock/unlock cycle");
  }
});

scenario("audit de santé", async () => {
  const report = await invoke("audit_health");
  const finding = report?.findings?.[0];
  if (!finding || typeof finding.score !== "number") {
    throw new Error(`unexpected health report: ${JSON.stringify(report)}`);
  }
});

scenario("sauvegarde locale", async () => {
  const target = await invoke("create_backup_target", {
    input: {
      name: "E2E local",
      provider: "local",
      path: BACKUP_DIR,
      intervalMinutes: 60,
      retentionCount: 2,
      enabled: false,
    },
  });
  const run = await invoke("run_backup_now", { id: target.id });
  if (run?.status !== "success") {
    throw new Error(`backup failed: ${JSON.stringify(run)}`);
  }
});

scenario("jeton MCP cloisonné", async () => {
  const issued = await invoke("create_mcp_token", {
    draft: {
      name: "E2E token",
      scopes: ["read_metadata"],
      folderIds: [PERSONAL],
      ttlDays: 1,
    },
  });
  if (!/^ak_mcp_[0-9a-f]{64}$/.test(issued?.secret ?? "")) {
    throw new Error("token secret has an unexpected shape");
  }
  await invoke("delete_mcp_token", { id: issued.token.id });
});

async function main() {
  await bridge.connect();
  const failures = [];
  for (const [index, { name, run }] of scenarios.entries()) {
    try {
      await run();
      console.log(`ok   ${name}`);
    } catch (error) {
      failures.push({ name, error });
      console.error(`FAIL ${name}: ${error.message}`);
      await screenshot(`${ARTIFACTS}/failure-${index}-${name.replaceAll(" ", "-")}.png`);
    }
  }
  bridge.close();
  if (failures.length > 0) {
    console.error(`\n${failures.length}/${scenarios.length} scénario(s) en échec`);
    process.exit(1);
  }
  console.log(`\n${scenarios.length} scénarios E2E verts`);
}

await main();
