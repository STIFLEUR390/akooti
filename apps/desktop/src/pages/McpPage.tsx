import { useCallback, useEffect, useState } from "react";

import type { Folder } from "@akooti/shared";

import { Icon } from "../components/Icons";
import type { Notices } from "../hooks/useToasts";
import { countLabel } from "../lib/format";
import type {
  AkootiApi,
  DaemonStatus,
  IssuedMcpToken,
  McpAuditEntry,
  McpScope,
  McpToken,
} from "../lib/ipc";
import { toIpcError } from "../lib/ipc";

interface McpPageProps {
  api: AkootiApi;
  folders: Folder[];
  notices: Notices;
}

const SCOPES: { value: McpScope; label: string; help: string }[] = [
  {
    value: "read_metadata",
    label: "Lire les métadonnées",
    help: "Titre, identifiant et URL — jamais un secret.",
  },
  {
    value: "blind_fill",
    label: "Remplir aveuglément",
    help: "Remplit un formulaire sans jamais renvoyer la valeur à l'agent.",
  },
  {
    value: "create_and_fill",
    label: "Créer un compte éphémère",
    help: "Génère un mot de passe et inscrit le compte, avec une durée de vie.",
  },
  {
    value: "exec_env",
    label: "Injecter dans un processus",
    help: "Exporte les secrets en variables d'environnement, sortie masquée.",
  },
];

const STATUS_LABELS: Record<McpAuditEntry["status"], string> = {
  success: "succès",
  denied: "refusé",
  error: "erreur",
};

const SCOPE_LABELS = Object.fromEntries(
  SCOPES.map((scope) => [scope.value, scope.label]),
) as Record<McpScope, string>;

function formatTime(seconds: number | null): string {
  if (seconds === null) {
    return "—";
  }
  return new Date(seconds * 1_000).toLocaleString("fr-FR", {
    dateStyle: "short",
    timeStyle: "short",
  });
}

/**
 * AI access screen: scoped tokens, the one-time secret, and the audit trail.
 *
 * The secret is rendered once, from the creation response, and never re-read
 * from the backend (`docs/regles.md` §2). Revoking is immediate; the row stays
 * visible so the audit trail still names it.
 */
export function McpPage({ api, folders, notices }: McpPageProps) {
  const [tokens, setTokens] = useState<McpToken[]>([]);
  const [audit, setAudit] = useState<McpAuditEntry[]>([]);
  const [daemon, setDaemon] = useState<DaemonStatus | null>(null);
  const [issued, setIssued] = useState<IssuedMcpToken | null>(null);
  const [name, setName] = useState("");
  const [scopes, setScopes] = useState<McpScope[]>(["read_metadata"]);
  const [folderIds, setFolderIds] = useState<string[]>([]);
  const [defaultFolderId, setDefaultFolderId] = useState<string>("");
  const [ttlDays, setTtlDays] = useState(30);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const load = useCallback(async () => {
    try {
      const [nextTokens, nextAudit] = await Promise.all([
        api.listMcpTokens(),
        api.listMcpAuditLog(50),
      ]);
      setTokens(nextTokens);
      setAudit(nextAudit);
    } catch (caught) {
      setError(toIpcError(caught).message);
    }
    try {
      setDaemon(await api.daemonStatus());
    } catch {
      setDaemon(null);
    }
  }, [api]);

  useEffect(() => {
    void load();
  }, [load]);

  function toggleScope(scope: McpScope) {
    setScopes((current) =>
      current.includes(scope) ? current.filter((value) => value !== scope) : [...current, scope],
    );
  }

  function toggleFolder(folderId: string) {
    setFolderIds((current) =>
      current.includes(folderId)
        ? current.filter((value) => value !== folderId)
        : [...current, folderId],
    );
  }

  async function create() {
    setBusy(true);
    setError(null);
    try {
      const created = await api.createMcpToken({
        name,
        scopes,
        folderIds,
        defaultCreateFolderId: defaultFolderId === "" ? null : defaultFolderId,
        ttlDays,
      });
      setIssued(created);
      setTokens((current) => [created.token, ...current]);
      setName("");
      notices.success("Jeton créé. Copiez le secret maintenant : il ne sera plus affiché.");
      await load();
    } catch (caught) {
      setError(toIpcError(caught).message);
    } finally {
      setBusy(false);
    }
  }

  async function revoke(token: McpToken) {
    setError(null);
    try {
      const updated = await api.revokeMcpToken(token.id);
      setTokens((current) => current.map((item) => (item.id === updated.id ? updated : item)));
      notices.success(`Jeton « ${token.name} » révoqué.`);
    } catch (caught) {
      setError(toIpcError(caught).message);
    }
  }

  async function remove(token: McpToken) {
    setError(null);
    try {
      await api.deleteMcpToken(token.id);
      setTokens((current) => current.filter((item) => item.id !== token.id));
      notices.success(`Jeton « ${token.name} » supprimé.`);
    } catch (caught) {
      setError(toIpcError(caught).message);
    }
  }

  async function copySecret() {
    if (!issued) {
      return;
    }
    try {
      await api.copySecret(issued.secret);
      notices.success("Secret copié. Il sera effacé du presse-papiers dans 30 secondes.");
    } catch (caught) {
      setError(toIpcError(caught).message);
    }
  }

  return (
    <section className="page" aria-labelledby="mcp-title">
      <header className="page__header">
        <div>
          <h2 id="mcp-title">Accès IA</h2>
          <p>
            Chaque agent reçoit un jeton cloisonné : il ne voit que les dossiers cochés et les
            actions autorisées. Aucun outil ne renvoie de secret — l'injection se fait dans le
            navigateur ou dans un processus enfant.
          </p>
        </div>
        <span className={`chip ${daemon?.running ? "chip--action" : "chip--quiet"}`}>
          <Icon name="bot" size={14} />
          {daemon?.running ? "daemon actif" : "daemon arrêté"}
        </span>
      </header>

      {daemon?.socketPath ? (
        <p className="field__hint">
          Socket local : <code>{daemon.socketPath}</code> — l'accès est refusé à tout autre
          utilisateur du système.
        </p>
      ) : null}

      {error ? (
        <p className="form__error" role="alert">
          {error}
        </p>
      ) : null}

      {issued ? (
        <div className="card card--secret" role="status">
          <h3>
            <Icon name="key" size={16} /> Secret du jeton{" "}
            <span className="card__name">« {issued.token.name} »</span>
          </h3>
          <p>
            Ce secret n'est affiché qu'une fois. Collez-le dans la configuration du client MCP :
            <code> akooti mcp --token …</code>
          </p>
          <div className="secret-box">
            <code>{issued.secret}</code>
            <button type="button" className="button" onClick={() => void copySecret()}>
              <Icon name="copy" size={14} />
              Copier
            </button>
          </div>
          <button type="button" className="text-button" onClick={() => setIssued(null)}>
            J'ai enregistré le secret
          </button>
        </div>
      ) : null}

      <form
        className="card"
        onSubmit={(event) => {
          event.preventDefault();
          void create();
        }}
      >
        <h3>Nouveau jeton</h3>
        <div className="form__row">
          <label className="field">
            <span className="field__label">Nom</span>
            <input
              value={name}
              onChange={(event) => setName(event.target.value)}
              placeholder="Claude Code — projet local"
              maxLength={100}
              required
            />
          </label>
          <label className="field">
            <span className="field__label">Durée de vie (jours, 0 = illimité)</span>
            <input
              type="number"
              min={0}
              max={365}
              value={ttlDays}
              onChange={(event) => setTtlDays(Number(event.target.value))}
            />
          </label>
        </div>

        <fieldset className="fieldset">
          <legend>Portées autorisées</legend>
          {SCOPES.map((scope) => (
            <label key={scope.value} className="checkbox">
              <input
                type="checkbox"
                checked={scopes.includes(scope.value)}
                onChange={() => toggleScope(scope.value)}
              />
              <span>
                <strong>{scope.label}</strong>
                <span className="field__hint"> {scope.help}</span>
              </span>
            </label>
          ))}
        </fieldset>

        <fieldset className="fieldset">
          <legend>Dossiers accessibles</legend>
          <div className="checkbox-grid">
            {folders.map((folder) => (
              <label key={folder.id} className="checkbox">
                <input
                  type="checkbox"
                  checked={folderIds.includes(folder.id)}
                  onChange={() => toggleFolder(folder.id)}
                />
                <span>{folder.name}</span>
              </label>
            ))}
          </div>
        </fieldset>

        <label className="field">
          <span className="field__label">Dossier des comptes créés par l'agent</span>
          <select
            value={defaultFolderId}
            onChange={(event) => setDefaultFolderId(event.target.value)}
          >
            <option value="">Premier dossier autorisé</option>
            {folders
              .filter((folder) => folderIds.includes(folder.id))
              .map((folder) => (
                <option key={folder.id} value={folder.id}>
                  {folder.name}
                </option>
              ))}
          </select>
        </label>

        <button type="submit" className="button button--primary" disabled={busy}>
          <Icon name="plus" size={14} />
          Créer le jeton
        </button>
      </form>

      <h3>Jetons</h3>
      {tokens.length === 0 ? (
        <div className="card">
          <p className="empty-state">
            <Icon name="bot" size={20} />
            Aucun jeton. Un agent ne peut rien lire ni remplir tant que vous n'en créez pas un.
          </p>
        </div>
      ) : (
        <ul className="rule-list">
          {tokens.map((token) => (
            <li key={token.id} className="rule">
              <div className="rule__main">
                <div className="rule__pattern">
                  <strong>{token.name}</strong>
                  <code>{token.tokenPrefix}…</code>
                </div>
                <div className="rule__meta">
                  {token.scopes.map((scope) => (
                    <span key={scope} className="chip chip--quiet">
                      {SCOPE_LABELS[scope]}
                    </span>
                  ))}
                  <span>
                    {countLabel(token.folderIds.length, "dossier")} ·{" "}
                    {token.expiresAt === null
                      ? "sans expiration"
                      : `expire le ${formatTime(token.expiresAt)}`}{" "}
                    · dernier usage {formatTime(token.lastUsedAt)}
                  </span>
                </div>
              </div>
              {token.active ? (
                <button
                  type="button"
                  className="text-button text-button--danger"
                  onClick={() => void revoke(token)}
                >
                  Révoquer
                </button>
              ) : (
                <button
                  type="button"
                  className="text-button text-button--danger"
                  onClick={() => void remove(token)}
                >
                  Supprimer
                </button>
              )}
            </li>
          ))}
        </ul>
      )}

      <h3>Activité des agents</h3>
      {audit.length === 0 ? (
        <div className="card">
          <p className="empty-state">
            <Icon name="shield" size={20} />
            Aucune action enregistrée. Chaque recherche, remplissage et exécution apparaîtra ici.
          </p>
        </div>
      ) : (
        <ul className="audit-list">
          {audit.map((entry) => (
            <li key={entry.id} className="audit">
              <span className={`chip chip--quiet audit__status audit__status--${entry.status}`}>
                {STATUS_LABELS[entry.status]}
              </span>
              <code>{entry.action}</code>
              <span className="audit__target" title={entry.targetOrigin ?? entry.details ?? "—"}>
                {entry.targetOrigin ?? entry.details ?? "—"}
              </span>
              <time dateTime={new Date(entry.createdAt * 1_000).toISOString()}>
                {formatTime(entry.createdAt)}
              </time>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
