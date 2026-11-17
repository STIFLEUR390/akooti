import { useEffect, useState, type FormEvent } from "react";
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
 * Every search is audited by the daemon, so a query is only sent on submit —
 * not on each keystroke.
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
  internal: "Erreur interne",
};

/** What the popup shows under a title. */
interface Failure {
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
  const [capture, setCapture] = useState<string | null>(null);

  useEffect(() => {
    let alive = true;
    void (async () => {
      const reply = await callRuntime(send, { type: "akooti:status" });
      if (!alive) {
        return;
      }
      if (!reply.ok) {
        setFailure(toFailure(reply.code, reply.message));
        setPhase("unpaired");
        return;
      }
      if (reply.kind !== "status") {
        setFailure({ title: FAILURE_TITLES.internal, detail: null });
        setPhase("unpaired");
        return;
      }
      setStatus(reply.status);
      if (!reply.status.paired) {
        setPhase("unpaired");
        return;
      }
      setPhase("paired");
      const loaded = await callRuntime(send, { type: "akooti:search", query: "" });
      if (alive && loaded.ok && loaded.kind === "search") {
        setItems(loaded.items);
        setSearched(true);
      }
      const detected = await callRuntime(send, { type: "akooti:detect" });
      if (alive && detected.ok && detected.kind === "detect") {
        setPage({ origin: detected.origin, kind: detected.detection.kind });
      }
    })();
    return () => {
      alive = false;
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
    const fields = reply.outcome.fieldsFilled.join(", ") || "aucun champ";
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

  async function captureSelectors(): Promise<void> {
    const reply = await run({ type: "akooti:capture" });
    if (!reply || reply.kind !== "capture") {
      return;
    }
    setCapture(reply.report.text);
    setNotice("Sélecteurs capturés : collez-les dans l'entrée du coffre.");
  }

  async function copyCapture(): Promise<void> {
    if (capture === null) {
      return;
    }
    await navigator.clipboard.writeText(capture);
    setNotice("Sélecteurs copiés.");
  }

  const pill = describe(status, phase);

  return (
    <div className="popup">
      <header className="toolbar">
        <span className="brand">Akooti</span>
        <span className={`pill pill--${pill.tone}`}>{pill.label}</span>
      </header>

      <div className="alerts" aria-live="polite">
        {failure && (
          <p className="alert alert--danger">
            <strong>{failure.title}</strong>
            {failure.detail && <span className="alert__detail">{failure.detail}</span>}
          </p>
        )}
        {notice && <p className="alert alert--success">{notice}</p>}
      </div>

      {phase === "loading" && <p className="muted">Chargement…</p>}

      {phase === "unpaired" && (
        <form
          className="pair"
          onSubmit={(event) => {
            event.preventDefault();
            void pair();
          }}
        >
          <h1 className="pair__title">Appairer l'extension</h1>
          <p className="muted">
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
          />
          <p className="muted small">
            {token.length > 0 && !isTokenShaped(token)
              ? "Un jeton Akooti commence par ak_mcp_ suivi de 64 caractères hexadécimaux."
              : "Le jeton reste dans ce navigateur et n'est envoyé qu'au daemon local."}
          </p>
          <button
            className="button button--primary"
            type="submit"
            disabled={busy || !isTokenShaped(token)}
          >
            Appairer
          </button>
        </form>
      )}

      {phase === "paired" && (
        <>
          <p className="muted small page">
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
            />
            <button className="button" type="submit" disabled={busy}>
              Chercher
            </button>
          </form>

          <ul className="items">
            {items.map((item) => (
              <li key={item.id} className="item">
                <div className="item__head">
                  <span className="item__title">{item.title}</span>
                  {item.primaryUrl && <span className="item__host">{hostOf(item.primaryUrl)}</span>}
                </div>
                {item.username && <span className="item__username">{item.username}</span>}
                <div className="item__actions">
                  <button
                    className="button button--primary"
                    disabled={busy}
                    onClick={() => void fill(item)}
                  >
                    Remplir
                  </button>
                  <span className="item__copy">
                    Copier
                    <button
                      className="chip"
                      disabled={busy}
                      onClick={() => void copy(item, "username")}
                    >
                      nom
                    </button>
                    <button
                      className="chip"
                      disabled={busy}
                      onClick={() => void copy(item, "password")}
                    >
                      mot de passe
                    </button>
                    {item.hasTotp && (
                      <button
                        className="chip"
                        disabled={busy}
                        onClick={() => void copy(item, "totp")}
                      >
                        code
                      </button>
                    )}
                  </span>
                </div>
              </li>
            ))}
            {searched && items.length === 0 && (
              <li className="muted small">Aucun identifiant ne correspond.</li>
            )}
          </ul>

          <section className="capture">
            <div className="capture__head">
              <span className="muted small">
                Les sélecteurs se collent dans l'entrée du coffre : le pont navigateur ne l'écrit
                jamais.
              </span>
              <button className="chip" disabled={busy} onClick={() => void captureSelectors()}>
                Capturer
              </button>
            </div>
            {capture !== null && (
              <>
                <pre className="capture__text">{capture}</pre>
                <button className="button" onClick={() => void copyCapture()}>
                  Copier les sélecteurs
                </button>
              </>
            )}
          </section>
        </>
      )}

      <footer className="footer">
        <span className="muted small">
          {status?.paired ? `Jeton ${status.maskedToken}` : "Aucun jeton"}
          {status?.label ? ` — ${status.label}` : ""}
        </span>
        {phase === "paired" && (
          <button className="chip" disabled={busy} onClick={() => void unpair()}>
            Dissocier
          </button>
        )}
      </footer>
    </div>
  );
}

/** Header pill: pairing state first, then what the daemon says about the vault. */
function describe(status: ExtensionStatus | null, phase: Phase): { label: string; tone: string } {
  if (phase === "unpaired" || !status?.paired) {
    return { label: "Non appairée", tone: "muted" };
  }
  if (!status.daemon) {
    return { label: "Application fermée", tone: "warn" };
  }
  if (!status.daemon.vaultUnlocked) {
    return { label: "Coffre verrouillé", tone: "warn" };
  }
  return { label: "Coffre déverrouillé", tone: "success" };
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

function toFailure(code: RuntimeErrorCode, detail: string): Failure {
  return { title: FAILURE_TITLES[code], detail: detail.length > 0 ? detail : null };
}

function hostOf(url: string): string {
  try {
    return new URL(url).host;
  } catch {
    return url;
  }
}
