import { useEffect, useRef, useState, type FormEvent } from "react";
import { browser } from "wxt/browser";
import { isTokenShaped } from "../../lib/native/pairing";
import { callRuntime, type RuntimeSend } from "../../lib/runtime";
import type {
  CopyField,
  ExtensionStatus,
  RuntimeErrorCode,
  RuntimeReply,
  RuntimeRequest,
} from "../../lib/messages";
import type { ItemSummary } from "../../lib/protocol";

/**
 * Popup: pairing, search, blind fill and selector capture.
 *
 * The popup owns no secret. A fill or a copy asks the service worker, which asks
 * the daemon, which hands the values to the content script of the active tab;
 * the only thing that comes back here is a report (`fieldsFilled`, `submitted`)
 * or a field name.
 *
 * Typing filters the items already loaded, so the list answers at once without
 * a request. Submitting asks the daemon (audited) and can reach items beyond
 * that first page — the reason the field is not searched on each keystroke.
 */

/** French headline for each failure code; the raw message stays a detail. */
const FAILURE_TITLES: Record<RuntimeErrorCode, string> = {
  not_paired: "Extension non appairée",
  locked: "Coffre verrouillé",
  daemon_unavailable: "Application fermée",
  host_missing: "Pont natif absent",
  protocol: "Incompatibilité de protocole",
  timeout: "Délai dépassé",
  denied: "Action refusée",
  no_tab: "Aucun onglet actif",
  unsupported_page: "Page non prise en charge",
  no_form: "Aucun formulaire",
  context_lost: "Extension rechargée",
  internal: "Erreur interne",
};

/** Failure codes that describe the Native Messaging link, not an action. */
const LINK_FAILURES: ReadonlySet<RuntimeErrorCode> = new Set([
  "host_missing",
  "daemon_unavailable",
  "timeout",
  "protocol",
  "not_paired",
  "context_lost",
]);

/** What the popup shows under a title. */
interface Failure {
  code: RuntimeErrorCode;
  title: string;
  detail: string | null;
}

type Phase = "loading" | "unpaired" | "paired";

const send: RuntimeSend = (message) => browser.runtime.sendMessage(message);

export function App() {
  const [phase, setPhase] = useState<Phase>("loading");
  const [status, setStatus] = useState<ExtensionStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [token, setToken] = useState("");
  const [query, setQuery] = useState("");
  const [items, setItems] = useState<ItemSummary[]>([]);
  const [searched, setSearched] = useState(false);
  const [page, setPage] = useState<{ origin: string; kind: string } | null>(null);
  const [capture, setCapture] = useState<{ itemId: string; text: string } | null>(null);
  const alive = useRef(true);

  /** Loads the first page of items and what the active tab looks like. */
  async function loadVault(): Promise<void> {
    const loaded = await callRuntime(send, { type: "akooti:search", query: "" });
    if (!alive.current) {
      return;
    }
    if (loaded.ok && loaded.kind === "search") {
      setItems(loaded.items);
      setSearched(true);
    }
    const detected = await callRuntime(send, { type: "akooti:detect" });
    if (!alive.current) {
      return;
    }
    if (detected.ok && detected.kind === "detect") {
      setPage({ origin: detected.origin, kind: detected.detection.kind });
    }
  }

  useEffect(() => {
    alive.current = true;
    void (async () => {
      const reply = await callRuntime(send, { type: "akooti:status" });
      if (!alive.current) {
        return;
      }
      if (!reply.ok) {
        setFailure(toFailure(reply.code, reply.message));
        setPhase("unpaired");
        return;
      }
      if (reply.kind !== "status") {
        setFailure({
          code: "internal",
          title: FAILURE_TITLES.internal,
          detail: null,
        });
        setPhase("unpaired");
        return;
      }
      setStatus(reply.status);
      if (!reply.status.paired) {
        setPhase("unpaired");
        return;
      }
      setPhase("paired");
      await loadVault();
    })();
    return () => {
      alive.current = false;
    };
  }, []);

  async function run(request: RuntimeRequest): Promise<RuntimeReply | null> {
    setBusy(true);
    setFailure(null);
    const reply = await callRuntime(send, request);
    setBusy(false);
    if (!reply.ok) {
      setFailure(toFailure(reply.code, reply.message));
      return null;
    }
    return reply;
  }

  async function pair(): Promise<void> {
    const reply = await run({ type: "akooti:pair", token, label: null });
    if (!reply || reply.kind !== "paired") {
      return;
    }
    // The token field is cleared before the status refresh, so the secret never
    // survives longer than the request that used it.
    setToken("");
    setNotice(`Appairée${reply.label ? ` — ${reply.label}` : ""}.`);
    setPhase("loading");
    const refreshed = await run({ type: "akooti:status" });
    if (refreshed && refreshed.kind === "status") {
      setStatus(refreshed.status);
      setPhase(refreshed.status.paired ? "paired" : "unpaired");
      if (refreshed.status.paired) {
        await loadVault();
      }
    } else {
      setPhase("unpaired");
    }
  }

  async function unpair(): Promise<void> {
    const reply = await run({ type: "akooti:unpair" });
    if (!reply) {
      return;
    }
    setStatus(null);
    setItems([]);
    setSearched(false);
    setCapture(null);
    setNotice("Extension dissociée.");
    setPhase("unpaired");
  }

  async function search(event: FormEvent): Promise<void> {
    event.preventDefault();
    const reply = await run({ type: "akooti:search", query });
    if (!reply || reply.kind !== "search") {
      return;
    }
    setItems(reply.items);
    setSearched(true);
  }

  async function fill(item: ItemSummary): Promise<void> {
    const reply = await run({ type: "akooti:fill", itemId: item.id });
    if (!reply || reply.kind !== "fill") {
      return;
    }
    const fields = reply.outcome.fieldsFilled.map(fieldLabel).join(", ") || "aucun champ";
    setNotice(
      reply.outcome.submitted
        ? `Champs remplis (${fields}) et formulaire soumis.`
        : `Champs remplis (${fields}).`,
    );
  }

  async function copy(item: ItemSummary, field: CopyField): Promise<void> {
    const reply = await run({ type: "akooti:copy", itemId: item.id, field });
    if (!reply || reply.kind !== "copy") {
      return;
    }
    setNotice(`${labelOf(reply.field)} copié — effacé du presse-papiers dans 30 s.`);
  }

  async function captureSelectors(item: ItemSummary): Promise<void> {
    if (capture?.itemId === item.id) {
      setCapture(null);
      return;
    }
    const reply = await run({ type: "akooti:capture" });
    if (!reply || reply.kind !== "capture") {
      return;
    }
    setCapture({ itemId: item.id, text: reply.report.text });
    setNotice("Sélecteurs capturés : collez-les dans l'entrée du coffre.");
  }

  async function copyCapture(): Promise<void> {
    if (capture === null) {
      return;
    }
    await navigator.clipboard.writeText(capture.text);
    setNotice("Sélecteurs copiés.");
  }

  const link = describeLink(status, phase, failure);
  const tabHost = page ? hostOf(page.origin) : null;
  const filtered = items.filter((item) => matches(item, query));
  const tabItems = tabHost
    ? filtered.filter((item) => hostOf(item.primaryUrl ?? "") === tabHost)
    : [];
  const otherItems = filtered.filter((item) => !tabItems.includes(item));

  function itemRow(item: ItemSummary) {
    const expanded = capture?.itemId === item.id;
    return (
      <li key={item.id} className="item">
        <div className="item__head">
          <span className="item__title" title={item.title}>
            {item.title}
          </span>
          {item.primaryUrl && (
            <span className="item__host" title={hostOf(item.primaryUrl)}>
              {hostOf(item.primaryUrl)}
            </span>
          )}
        </div>
        {item.username && (
          <div className="item__meta">
            <span className="item__username" title={item.username}>
              {item.username}
            </span>
            <button
              type="button"
              className="icon-button"
              disabled={busy}
              onClick={() => void copy(item, "username")}
              aria-label={`Copier l'identifiant de ${item.title}`}
              title="Copier l'identifiant"
            >
              <CopyIcon />
            </button>
          </div>
        )}
        <div className="item__actions">
          <button
            type="button"
            className="button button--primary"
            disabled={busy}
            onClick={() => void fill(item)}
          >
            Remplir
          </button>
          <button
            type="button"
            className="chip chip--action"
            disabled={busy}
            onClick={() => void copy(item, "password")}
            aria-label={`Copier le mot de passe de ${item.title}`}
            title="Copier le mot de passe"
          >
            Mot de passe
          </button>
          {item.hasTotp && (
            <button
              type="button"
              className="chip chip--action"
              disabled={busy}
              onClick={() => void copy(item, "totp")}
              aria-label={`Copier le code TOTP de ${item.title}`}
              title="Copier le code TOTP"
            >
              TOTP
            </button>
          )}
          <button
            type="button"
            className="chip"
            disabled={busy}
            onClick={() => void captureSelectors(item)}
            aria-expanded={expanded}
            title="Capturer les sélecteurs CSS de cet onglet"
          >
            Sélecteurs
          </button>
        </div>
        {expanded && capture && (
          <div className="capture" aria-label="Sélecteurs capturés">
            <pre className="capture__text">{capture.text}</pre>
            <button type="button" className="button" onClick={() => void copyCapture()}>
              Copier les sélecteurs
            </button>
          </div>
        )}
      </li>
    );
  }

  return (
    <div className="popup">
      <header className="toolbar">
        <h1 className="brand">
          <img src="/icon/48.png" alt="" width={22} height={22} className="brand__logo" />
          Akooti
        </h1>
        <span className={`pill pill--${link.tone}`} role="status" title={link.detail ?? undefined}>
          {link.label}
        </span>
      </header>

      <main className="body">
        <div className="alerts" aria-live="polite">
          {failure && (
            <p className="alert alert--danger">
              <strong>{failure.title}</strong>
              {failure.detail && <span className="alert__detail">{failure.detail}</span>}
            </p>
          )}
          {notice && <p className="alert alert--success">{notice}</p>}
        </div>

        {phase === "loading" && <p className="muted small">Chargement…</p>}

        {phase === "unpaired" && (
          <form
            className="pair"
            onSubmit={(event) => {
              event.preventDefault();
              void pair();
            }}
          >
            <h2 className="pair__title">Appairer l'extension</h2>
            <p className="muted small">
              Dans Akooti, ouvrez <strong>Accès IA</strong>, créez un jeton avec les portées{" "}
              <code>read_metadata</code> et <code>blind_fill</code>, puis collez-le ici.
            </p>
            <input
              className="field"
              type="password"
              value={token}
              onChange={(event) => setToken(event.target.value)}
              placeholder="ak_mcp_…"
              aria-label="Jeton d'accès"
              autoComplete="off"
              spellCheck={false}
              autoFocus
            />
            <p className="muted small">
              {token.length > 0 && !isTokenShaped(token)
                ? "Un jeton Akooti commence par ak_mcp_ suivi de 64 caractères hexadécimaux."
                : "Le jeton reste dans ce navigateur et n'est envoyé qu'au daemon local."}
            </p>
            <button
              className="button button--primary button--block"
              type="submit"
              disabled={busy || !isTokenShaped(token)}
            >
              Appairer
            </button>
          </form>
        )}

        {phase === "paired" && (
          <>
            <p className="page" title={page?.origin}>
              {page
                ? `${describeForm(page.kind)} sur ${page.origin}`
                : "Aucun formulaire de connexion détecté sur cette page."}
            </p>

            <form className="search" onSubmit={(event) => void search(event)}>
              <input
                className="field"
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Rechercher un identifiant…"
                aria-label="Rechercher un identifiant"
                autoComplete="off"
                spellCheck={false}
                autoFocus
              />
              <button className="button" type="submit" disabled={busy}>
                Chercher
              </button>
            </form>

            <div className="results">
              {tabItems.length > 0 && <h2 className="group">Pour cet onglet</h2>}
              {tabItems.length > 0 && <ul className="items">{tabItems.map(itemRow)}</ul>}
              {otherItems.length > 0 && (
                <h2 className="group">
                  {tabItems.length > 0 ? "Autres identifiants" : "Identifiants"}
                </h2>
              )}
              {otherItems.length > 0 && <ul className="items">{otherItems.map(itemRow)}</ul>}
              {filtered.length === 0 &&
                (status?.daemon && !status.daemon.vaultUnlocked ? (
                  <p className="empty">
                    Coffre verrouillé : déverrouillez Akooti pour voir les identifiants.
                  </p>
                ) : searched ? (
                  <p className="empty">
                    {query.length > 0
                      ? "Aucun identifiant ne correspond à cette recherche."
                      : "Aucun identifiant dans le coffre."}
                  </p>
                ) : null)}
            </div>
          </>
        )}
      </main>

      <footer className="footer">
        <span
          className="footer__token"
          title={
            status?.paired
              ? `Jeton ${status.maskedToken}${status.label ? ` — ${status.label}` : ""}`
              : undefined
          }
        >
          {status?.paired ? `Jeton ${status.maskedToken}` : "Aucun jeton"}
          {status?.label ? ` — ${status.label}` : ""}
        </span>
        {phase === "paired" && (
          <button type="button" className="chip" disabled={busy} onClick={() => void unpair()}>
            Dissocier
          </button>
        )}
      </footer>
    </div>
  );
}

/** Native Messaging link state, then what the daemon says about the vault. */
function describeLink(
  status: ExtensionStatus | null,
  phase: Phase,
  failure: Failure | null,
): { label: string; tone: string; detail: string | null } {
  if (failure?.code === "locked") {
    return {
      label: FAILURE_TITLES.locked,
      tone: "warn",
      detail: "Déverrouillez Akooti pour remplir ou copier.",
    };
  }
  const linkFailure = failure && LINK_FAILURES.has(failure.code) ? failure : null;
  if (linkFailure) {
    const tones: Partial<Record<RuntimeErrorCode, string>> = {
      daemon_unavailable: "warn",
      timeout: "warn",
      protocol: "danger",
      host_missing: "danger",
    };
    return {
      label: FAILURE_TITLES[linkFailure.code],
      tone: tones[linkFailure.code] ?? "muted",
      detail: linkFailure.detail,
    };
  }
  if (phase === "unpaired" || !status?.paired) {
    return {
      label: "Non appairée",
      tone: "muted",
      detail: "Aucun jeton dans ce navigateur.",
    };
  }
  if (!status.daemon) {
    return {
      label: "Application fermée",
      tone: "warn",
      detail: status.daemonError ?? "Le pont natif ne joint pas le daemon.",
    };
  }
  if (!status.daemon.vaultUnlocked) {
    return {
      label: "Coffre verrouillé",
      tone: "warn",
      detail: "Déverrouillez Akooti pour remplir ou copier.",
    };
  }
  return {
    label: "Daemon connecté",
    tone: "success",
    detail: "Native Messaging joignable, coffre déverrouillé.",
  };
}

function describeForm(kind: string): string {
  switch (kind) {
    case "login":
      return "Formulaire de connexion";
    case "signup":
      return "Formulaire d'inscription";
    case "totp":
      return "Demande de code à deux facteurs";
    default:
      return "Formulaire";
  }
}

function labelOf(field: CopyField): string {
  switch (field) {
    case "username":
      return "Identifiant";
    case "password":
      return "Mot de passe";
    case "totp":
      return "Code";
  }
}

/** Fill reports use the page role names; the popup speaks French. */
function fieldLabel(field: string): string {
  switch (field) {
    case "username":
      return "identifiant";
    case "password":
      return "mot de passe";
    case "totp":
      return "code";
    default:
      return field;
  }
}

function toFailure(code: RuntimeErrorCode, detail: string): Failure {
  return { code, title: FAILURE_TITLES[code], detail: detail.length > 0 ? detail : null };
}

/** Host of a URL, or "" when it does not parse. */
function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return "";
  }
}

/** Accent- and case-insensitive filter over the loaded items. */
function matches(item: ItemSummary, query: string): boolean {
  const needle = normalize(query);
  if (needle.length === 0) {
    return true;
  }
  return normalize(
    [item.title, item.username ?? "", item.primaryUrl ?? "", hostOf(item.primaryUrl ?? "")].join(
      " ",
    ),
  ).includes(needle);
}

function normalize(value: string): string {
  return value
    .normalize("NFD")
    .replace(/[\u0300-\u036f]/g, "")
    .toLowerCase()
    .trim();
}

function CopyIcon() {
  return (
    <svg viewBox="0 0 12 12" width="11" height="11" aria-hidden="true" focusable="false">
      <rect x="4" y="4" width="7" height="7" rx="1.2" fill="none" stroke="currentColor" />
      <path
        d="M8 4V2.2A1.2 1.2 0 0 0 6.8 1H2.2A1.2 1.2 0 0 0 1 2.2v4.6A1.2 1.2 0 0 0 2.2 8H4"
        fill="none"
        stroke="currentColor"
      />
    </svg>
  );
}
