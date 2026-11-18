import { useCallback, useEffect, useMemo, useState } from "react";

import { Icon } from "../components/Icons";
import type {
  AkootiApi,
  BackupRun,
  BackupTarget,
  BackupTargetInput,
  BackupProviderKind,
  SyncRun,
  SyncStatus,
} from "../lib/ipc";
import { listenBackupStatus, listenSyncStatus, toIpcError } from "../lib/ipc";
import { absoluteDate, relativeTime } from "../lib/format";

interface SyncPageProps {
  api: AkootiApi;
}

const DEFAULT_INTERVAL = 60;
const DEFAULT_RETENTION = 24;
const PROVIDERS: ReadonlyArray<{ id: BackupProviderKind; label: string }> = [
  { id: "local", label: "Dossier local / NAS" },
  { id: "s3", label: "S3 compatible (AWS, R2, MinIO, Wasabi)" },
  { id: "gdrive", label: "Google Drive" },
];

interface TargetForm {
  id: string | null;
  name: string;
  provider: BackupProviderKind;
  path: string;
  endpoint: string;
  region: string;
  bucket: string;
  accessKeyId: string;
  secretAccessKey: string;
  prefix: string;
  clientId: string;
  clientSecret: string;
  refreshToken: string;
  folderId: string;
  intervalMinutes: number;
  retentionCount: number;
  enabled: boolean;
}

function emptyForm(): TargetForm {
  return {
    id: null,
    name: "",
    provider: "local",
    path: "",
    endpoint: "",
    region: "",
    bucket: "",
    accessKeyId: "",
    secretAccessKey: "",
    prefix: "",
    clientId: "",
    clientSecret: "",
    refreshToken: "",
    folderId: "",
    intervalMinutes: DEFAULT_INTERVAL,
    retentionCount: DEFAULT_RETENTION,
    enabled: true,
  };
}

function formFor(target: BackupTarget): TargetForm {
  return { ...emptyForm(), id: target.id, name: target.name, provider: target.provider };
}

function toInput(form: TargetForm): BackupTargetInput {
  return {
    name: form.name.trim(),
    provider: form.provider,
    path: form.path || null,
    endpoint: form.endpoint || null,
    region: form.region || null,
    bucket: form.bucket || null,
    accessKeyId: form.accessKeyId || null,
    secretAccessKey: form.secretAccessKey || null,
    prefix: form.prefix || null,
    clientId: form.clientId || null,
    clientSecret: form.clientSecret || null,
    refreshToken: form.refreshToken || null,
    folderId: form.folderId || null,
    intervalMinutes: form.intervalMinutes,
    retentionCount: form.retentionCount,
    enabled: form.enabled,
  };
}

/**
 * Synchronisation and backups.
 *
 * Nothing here is automatic: signing in to Supabase stores encrypted tokens,
 * and every export goes through a native dialog. Credentials of a backup target
 * are write-only — the list never returns them.
 */
export function SyncPage({ api }: SyncPageProps) {
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [targets, setTargets] = useState<BackupTarget[]>([]);
  const [runs, setRuns] = useState<BackupRun[]>([]);
  const [form, setForm] = useState<TargetForm>(emptyForm);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [lastRun, setLastRun] = useState<SyncRun | null>(null);
  const [signIn, setSignIn] = useState({ supabaseUrl: "", anonKey: "", email: "", password: "" });
  const [plainPassword, setPlainPassword] = useState("");
  const [showPlain, setShowPlain] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const [nextStatus, nextTargets, nextRuns] = await Promise.all([
        api.syncStatus(),
        api.listBackupTargets(),
        api.listBackupRuns(undefined, 20),
      ]);
      setStatus(nextStatus);
      setTargets(nextTargets);
      setRuns(nextRuns);
    } catch (caught) {
      setError(toIpcError(caught).message);
    }
  }, [api]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let cancelled = false;
    const unlisteners: Array<() => void> = [];
    void listenSyncStatus((event) => {
      if (!cancelled) {
        setNotice(
          event.status === "success"
            ? `Sync : ${event.pushed} envoyé(s), ${event.applied} appliqué(s).`
            : (event.detail ?? "Synchronisation en échec."),
        );
        void refresh();
      }
    }).then((stop) => (cancelled ? stop() : unlisteners.push(stop)));
    void listenBackupStatus((event) => {
      if (!cancelled) {
        setNotice(`Sauvegarde « ${event.name} » : ${event.status}.`);
        void refresh();
      }
    }).then((stop) => (cancelled ? stop() : unlisteners.push(stop)));
    return () => {
      cancelled = true;
      for (const stop of unlisteners) {
        stop();
      }
    };
  }, [refresh]);

  const run = useCallback(async (label: string, operation: () => Promise<void>) => {
    setBusy(label);
    setError(null);
    setNotice(null);
    try {
      await operation();
    } catch (caught) {
      setError(toIpcError(caught).message);
    } finally {
      setBusy(null);
    }
  }, []);

  const submitSignIn = useCallback(
    () =>
      run("signin", async () => {
        const next = await api.syncSignIn(signIn);
        setStatus(next);
        setSignIn({
          supabaseUrl: signIn.supabaseUrl,
          anonKey: signIn.anonKey,
          email: "",
          password: "",
        });
        setNotice("Connecté à Supabase.");
      }),
    [api, run, signIn],
  );

  const syncNow = useCallback(
    (full: boolean) =>
      run("sync", async () => {
        const result = full ? await api.syncResync() : await api.syncNow();
        setLastRun(result);
        await refresh();
      }),
    [api, refresh, run],
  );

  const submitTarget = useCallback(
    () =>
      run("target", async () => {
        const input = toInput(form);
        const saved = form.id
          ? await api.updateBackupTarget(form.id, input)
          : await api.createBackupTarget(input);
        setForm(emptyForm());
        setNotice(`Cible « ${saved.name} » enregistrée.`);
        await refresh();
      }),
    [api, form, refresh, run],
  );

  const testTarget = useCallback(
    () =>
      run("test", async () => {
        setNotice(await api.testBackupTarget(toInput(form)));
      }),
    [api, form, run],
  );

  const historyByTarget = useMemo(() => {
    const result = new Map<string, string>();
    for (const target of targets) {
      result.set(target.id, target.lastStatus ?? "—");
    }
    return result;
  }, [targets]);

  return (
    <section className="page" aria-labelledby="sync-title">
      <header className="page__header">
        <div>
          <h2 id="sync-title">Sync &amp; sauvegardes</h2>
          <p>
            Synchronisation chiffrée de bout en bout via votre projet Supabase, sauvegardes
            planifiées vers un dossier, un bucket S3 ou votre Drive, et export d'urgence toujours
            disponible. Le serveur ne voit que des blobs chiffrés.
          </p>
        </div>
        <div className="form__actions">
          <button
            type="button"
            className="button"
            onClick={() =>
              run("export", async () => {
                const receipt = await api.exportSnapshot();
                if (receipt) {
                  setNotice(`Instantané enregistré (${receipt.sizeBytes} octets).`);
                }
              })
            }
            disabled={busy !== null}
          >
            <Icon name="upload" size={14} />
            Exporter un instantané
          </button>
        </div>
      </header>

      {error ? (
        <p className="form__error" role="alert">
          {error}
        </p>
      ) : null}
      {notice ? (
        <p className="chip chip--quiet" role="status">
          {notice}
        </p>
      ) : null}

      <div className="card">
        <h3>Synchronisation Supabase</h3>
        {status ? (
          <p className="field__label">
            {status.connected ? "Connecté" : "Non connecté"} ·{" "}
            {status.enabled ? "active" : "en pause"} ·{" "}
            {status.lastSyncAt
              ? `dernière sync ${relativeTime(status.lastSyncAt)}`
              : "jamais synchronisé"}
            {status.lastError ? ` · ${status.lastError}` : ""}
          </p>
        ) : null}

        <div className="form__actions">
          <button
            type="button"
            className="button"
            onClick={() =>
              run("copy-sql", async () => {
                const sql = await api.supabaseSetupSql();
                await api.copyText(sql);
                setNotice("Script SQL copié : collez-le dans le SQL Editor du projet.");
              })
            }
            disabled={busy !== null}
          >
            <Icon name="copy" size={14} />
            Copier le script SQL
          </button>
          <button
            type="button"
            className="button"
            onClick={() =>
              run("save-sql", async () => {
                const path = await api.saveSupabaseSetup();
                if (path) {
                  setNotice(`Script enregistré : ${path}`);
                }
              })
            }
            disabled={busy !== null}
          >
            <Icon name="upload" size={14} />
            Enregistrer le script…
          </button>
        </div>

        {status && !status.connected ? (
          <div className="form">
            <p className="field__label">
              Créer un projet : https://supabase.com/dashboard · guide pas-à-pas :
              docs/guides/supabase.md (URL du projet + clé anon, puis un utilisateur e-mail/mot de
              passe).
            </p>
            <label className="field">
              <span className="field__label">URL du projet Supabase</span>
              <input
                value={signIn.supabaseUrl}
                onChange={(event) => setSignIn({ ...signIn, supabaseUrl: event.target.value })}
                placeholder="https://xxxx.supabase.co"
                spellCheck={false}
              />
            </label>
            <label className="field">
              <span className="field__label">Clé anon</span>
              <input
                value={signIn.anonKey}
                onChange={(event) => setSignIn({ ...signIn, anonKey: event.target.value })}
                placeholder="sb_publishable_… ou eyJ…"
                spellCheck={false}
              />
            </label>
            <div className="form__row">
              <label className="field">
                <span className="field__label">E-mail</span>
                <input
                  type="email"
                  value={signIn.email}
                  onChange={(event) => setSignIn({ ...signIn, email: event.target.value })}
                  placeholder="vous@exemple.fr"
                  autoComplete="off"
                />
              </label>
              <label className="field">
                <span className="field__label">Mot de passe Supabase</span>
                <input
                  type="password"
                  value={signIn.password}
                  onChange={(event) => setSignIn({ ...signIn, password: event.target.value })}
                  autoComplete="off"
                />
              </label>
            </div>
            <div className="form__actions">
              <button
                type="button"
                className="button button--primary"
                onClick={() => void submitSignIn()}
                disabled={busy !== null}
              >
                {busy === "signin" ? "Connexion…" : "Se connecter"}
              </button>
            </div>
          </div>
        ) : null}

        {status && status.connected ? (
          <div className="form__actions">
            <button
              type="button"
              className="button button--primary"
              onClick={() => void syncNow(false)}
              disabled={busy !== null}
            >
              <Icon name="refresh" size={14} />
              {busy === "sync" ? "Synchronisation…" : "Synchroniser maintenant"}
            </button>
            <button
              type="button"
              className="button"
              onClick={() => void syncNow(true)}
              disabled={busy !== null}
            >
              Resynchroniser tout
            </button>
            <button
              type="button"
              className="button button--danger"
              onClick={() =>
                run("disconnect", async () => {
                  setStatus(await api.syncDisconnect());
                  setNotice("Session Supabase oubliée.");
                })
              }
              disabled={busy !== null}
            >
              Se déconnecter
            </button>
          </div>
        ) : null}

        {lastRun ? (
          <p className="field__label" role="status">
            {lastRun.pushed} envoyé(s) · {lastRun.pulled} reçu(s) · {lastRun.applied} appliqué(s) ·{" "}
            {lastRun.skipped} ignoré(s)
            {lastRun.full ? " · reprise complète" : ""}
          </p>
        ) : null}
      </div>

      <div className="card">
        <h3>Cibles de sauvegarde</h3>
        {targets.length === 0 ? (
          <p className="field__label">
            Aucune cible : ajoutez un dossier local, un bucket ou un Drive.
          </p>
        ) : (
          <ul className="audit-list">
            {targets.map((target) => (
              <li key={target.id}>
                <div className="audit">
                  <span>{target.name}</span>
                  <span className="audit__target">{target.summary}</span>
                  <span className="chip chip--quiet">
                    {target.intervalMinutes} min · {target.retentionCount} gardés
                  </span>
                  <span className="chip chip--quiet">
                    {historyByTarget.get(target.id) === "success"
                      ? "OK"
                      : (target.lastStatus ?? "jamais")}
                  </span>
                  <button
                    type="button"
                    className="text-button"
                    onClick={() =>
                      run(`run-${target.id}`, async () => {
                        await api.runBackupNow(target.id);
                        await refresh();
                      })
                    }
                    disabled={busy !== null}
                  >
                    {busy === `run-${target.id}` ? "En cours…" : "Sauvegarder"}
                  </button>
                  <button
                    type="button"
                    className="text-button"
                    onClick={() => setForm(formFor(target))}
                  >
                    Modifier
                  </button>
                  <button
                    type="button"
                    className="text-button"
                    onClick={() =>
                      run(`toggle-${target.id}`, async () => {
                        await api.setBackupTargetEnabled(target.id, !target.enabled);
                        await refresh();
                      })
                    }
                    disabled={busy !== null}
                  >
                    {target.enabled ? "Pause" : "Activer"}
                  </button>
                  <button
                    type="button"
                    className="text-button text-button--danger"
                    onClick={() =>
                      run(`delete-${target.id}`, async () => {
                        await api.deleteBackupTarget(target.id);
                        await refresh();
                      })
                    }
                    disabled={busy !== null}
                  >
                    Supprimer
                  </button>
                </div>
              </li>
            ))}
          </ul>
        )}

        <div className="form">
          <p className="field__label">
            Guides de configuration : docs/guides/s3.md et docs/guides/google-drive.md. Les
            identifiants sont chiffrés localement et ne sont jamais réaffichés.
          </p>
          <div className="form__row">
            <label className="field">
              <span className="field__label">Nom</span>
              <input
                value={form.name}
                onChange={(event) => setForm({ ...form, name: event.target.value })}
              />
            </label>
            <label className="field">
              <span className="field__label">Fournisseur</span>
              <select
                value={form.provider}
                onChange={(event) =>
                  setForm({ ...form, provider: event.target.value as BackupProviderKind })
                }
              >
                {PROVIDERS.map((provider) => (
                  <option key={provider.id} value={provider.id}>
                    {provider.label}
                  </option>
                ))}
              </select>
            </label>
          </div>

          {form.provider === "local" ? (
            <label className="field">
              <span className="field__label">Dossier (les identifiants restent locaux)</span>
              <div className="field__inline">
                <input
                  value={form.path}
                  onChange={(event) => setForm({ ...form, path: event.target.value })}
                  placeholder="/mnt/nas/akooti"
                />
                <button
                  type="button"
                  className="button"
                  onClick={() =>
                    run("folder", async () => {
                      const picked = await api.pickBackupFolder();
                      if (picked) {
                        setForm({ ...form, path: picked });
                      }
                    })
                  }
                >
                  Choisir…
                </button>
              </div>
            </label>
          ) : null}

          {form.provider === "s3" ? (
            <>
              <div className="form__row">
                <label className="field">
                  <span className="field__label">Endpoint</span>
                  <input
                    value={form.endpoint}
                    onChange={(event) => setForm({ ...form, endpoint: event.target.value })}
                    placeholder="https://s3.eu-west-3.amazonaws.com"
                    spellCheck={false}
                  />
                </label>
                <label className="field">
                  <span className="field__label">Région</span>
                  <input
                    value={form.region}
                    onChange={(event) => setForm({ ...form, region: event.target.value })}
                    placeholder="eu-west-3"
                    spellCheck={false}
                  />
                </label>
              </div>
              <div className="form__row">
                <label className="field">
                  <span className="field__label">Bucket</span>
                  <input
                    value={form.bucket}
                    onChange={(event) => setForm({ ...form, bucket: event.target.value })}
                  />
                </label>
                <label className="field">
                  <span className="field__label">Préfixe (facultatif)</span>
                  <input
                    value={form.prefix}
                    onChange={(event) => setForm({ ...form, prefix: event.target.value })}
                  />
                </label>
              </div>
              <div className="form__row">
                <label className="field">
                  <span className="field__label">Clé d'accès</span>
                  <input
                    value={form.accessKeyId}
                    onChange={(event) => setForm({ ...form, accessKeyId: event.target.value })}
                    spellCheck={false}
                  />
                </label>
                <label className="field">
                  <span className="field__label">Clé secrète</span>
                  <input
                    type="password"
                    value={form.secretAccessKey}
                    onChange={(event) => setForm({ ...form, secretAccessKey: event.target.value })}
                    autoComplete="off"
                  />
                </label>
              </div>
            </>
          ) : null}

          {form.provider === "gdrive" ? (
            <>
              <div className="form__row">
                <label className="field">
                  <span className="field__label">Client id OAuth2</span>
                  <input
                    value={form.clientId}
                    onChange={(event) => setForm({ ...form, clientId: event.target.value })}
                    spellCheck={false}
                  />
                </label>
                <label className="field">
                  <span className="field__label">Client secret</span>
                  <input
                    type="password"
                    value={form.clientSecret}
                    onChange={(event) => setForm({ ...form, clientSecret: event.target.value })}
                    autoComplete="off"
                  />
                </label>
              </div>
              <label className="field">
                <span className="field__label">Refresh token</span>
                <input
                  type="password"
                  value={form.refreshToken}
                  onChange={(event) => setForm({ ...form, refreshToken: event.target.value })}
                  autoComplete="off"
                />
              </label>
              <label className="field">
                <span className="field__label">Dossier Drive (id, facultatif)</span>
                <input
                  value={form.folderId}
                  onChange={(event) => setForm({ ...form, folderId: event.target.value })}
                  spellCheck={false}
                />
              </label>
            </>
          ) : null}

          <div className="form__row">
            <label className="field">
              <span className="field__label">Intervalle (minutes)</span>
              <input
                type="number"
                min={60}
                value={form.intervalMinutes}
                onChange={(event) =>
                  setForm({ ...form, intervalMinutes: Number(event.target.value) || 60 })
                }
              />
            </label>
            <label className="field">
              <span className="field__label">Instantanés conservés</span>
              <input
                type="number"
                min={1}
                value={form.retentionCount}
                onChange={(event) =>
                  setForm({ ...form, retentionCount: Number(event.target.value) || 1 })
                }
              />
            </label>
          </div>

          <div className="form__actions">
            <button
              type="button"
              className="button button--primary"
              onClick={() => void submitTarget()}
              disabled={busy !== null || form.name.trim().length === 0}
            >
              {form.id ? "Enregistrer les modifications" : "Ajouter la cible"}
            </button>
            <button
              type="button"
              className="button"
              onClick={() => void testTarget()}
              disabled={busy !== null || form.name.trim().length === 0}
            >
              {busy === "test" ? "Test…" : "Tester"}
            </button>
            {form.id ? (
              <button type="button" className="button" onClick={() => setForm(emptyForm())}>
                Annuler
              </button>
            ) : null}
          </div>
          {form.id ? (
            <p className="field__label">
              Les identifiants vides conservent ceux déjà enregistrés pour cette cible.
            </p>
          ) : null}
        </div>
      </div>

      <div className="card">
        <h3>Historique des sauvegardes</h3>
        {runs.length === 0 ? (
          <p className="field__label">Aucune sauvegarde enregistrée.</p>
        ) : (
          <ul className="audit-list">
            {runs.map((entry) => (
              <li key={entry.id}>
                <div className="audit">
                  <span className={entry.status === "success" ? "chip" : "chip chip--warn"}>
                    {entry.status}
                  </span>
                  <span>{absoluteDate(entry.startedAt)}</span>
                  <span className="audit__target">{entry.detail ?? entry.snapshotName ?? "—"}</span>
                  <span className="chip chip--quiet">
                    {entry.sizeBytes ? `${Math.round(entry.sizeBytes / 1024)} Kio` : "—"}
                  </span>
                </div>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="card card--secret">
        <h3>Export d'urgence en clair</h3>
        <p className="field__label">
          Le JSON n'est pas chiffré : il contient les mots de passe. Il n'est produit qu'après
          vérification du mot de passe maître, et les passkeys n'y figurent que sans leur clé privée
          — utilisez l'instantané <code>.akooti</code> pour une restauration complète.
        </p>
        <label className="checkbox">
          <input
            type="checkbox"
            checked={showPlain}
            onChange={(event) => setShowPlain(event.target.checked)}
          />
          Je comprends que ce fichier est lisible par quiconque y a accès
        </label>
        <div className="form__row">
          <label className="field">
            <span className="field__label">Mot de passe maître</span>
            <input
              type="password"
              value={plainPassword}
              onChange={(event) => setPlainPassword(event.target.value)}
              autoComplete="off"
            />
          </label>
          <button
            type="button"
            className="button button--danger"
            onClick={() =>
              run("plain", async () => {
                const receipt = await api.exportPlaintext(plainPassword);
                setPlainPassword("");
                if (receipt) {
                  setNotice(`Export en clair écrit (${receipt.sizeBytes} octets).`);
                }
              })
            }
            disabled={busy !== null || !showPlain || plainPassword.length === 0}
          >
            {busy === "plain" ? "Vérification…" : "Exporter en clair"}
          </button>
        </div>
      </div>
    </section>
  );
}
