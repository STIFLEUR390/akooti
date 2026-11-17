import { useCallback, useEffect, useState } from "react";

import { describeCleanup, describeImportIssue } from "../lib/format";
import type {
  AkootiApi,
  ImportFormat,
  ImportOptions,
  ImportPreview,
  ImportReport,
} from "../lib/ipc";
import { toIpcError } from "../lib/ipc";
import type { Notices } from "../hooks/useToasts";
import { Icon } from "../components/Icons";

interface ImportPageProps {
  api: AkootiApi;
  notices: Notices;
  onImported: () => void;
}

type Step = "source" | "options" | "report";

/**
 * Import wizard.
 *
 * Nothing is written until the last step: the preview parses the file
 * read-only, and the report only appears after a committed transaction. The
 * plaintext file never reaches the webview — the backend receives a path.
 */
export function ImportPage({ api, notices, onImported }: ImportPageProps) {
  const [formats, setFormats] = useState<ImportFormat[]>([]);
  const [path, setPath] = useState<string | null>(null);
  const [format, setFormat] = useState("auto");
  const [password, setPassword] = useState("");
  const [preview, setPreview] = useState<ImportPreview | null>(null);
  const [report, setReport] = useState<ImportReport | null>(null);
  const [step, setStep] = useState<Step>("source");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [options, setOptions] = useState<ImportOptions>({
    routeDevLocal: true,
    importDuplicates: false,
    secureDeleteSource: false,
    password: null,
  });

  useEffect(() => {
    let cancelled = false;
    api
      .listImportFormats()
      .then((value) => {
        if (!cancelled) {
          setFormats(value);
        }
      })
      .catch((caught: unknown) => {
        if (!cancelled) {
          setError(toIpcError(caught).message);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [api]);

  const chooseFile = useCallback(async () => {
    setError(null);
    try {
      const picked = await api.pickImportFile();
      if (picked) {
        setPath(picked);
        setPreview(null);
        setReport(null);
        setStep("source");
      }
    } catch (caught) {
      setError(toIpcError(caught).message);
    }
  }, [api]);

  const runPreview = useCallback(async () => {
    if (!path) {
      setError("Sélectionnez d'abord un fichier.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await api.previewImportFile(
        path,
        format,
        password.length > 0 ? password : undefined,
      );
      setPreview(result);
      setStep("options");
    } catch (caught) {
      setPreview(null);
      setError(toIpcError(caught).message);
    } finally {
      setBusy(false);
    }
  }, [api, format, password, path]);

  const runImport = useCallback(async () => {
    if (!path) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await api.importFile(path, format, options);
      setReport(result);
      setStep("report");
      notices.success(`${result.imported} entrée(s) importée(s).`);
      onImported();
    } catch (caught) {
      const ipc = toIpcError(caught);
      setError(ipc.message);
      notices.failure(ipc.message);
    } finally {
      setBusy(false);
    }
  }, [api, format, notices, onImported, options, path]);

  function reset() {
    setPath(null);
    setPreview(null);
    setReport(null);
    setPassword("");
    setFormat("auto");
    setStep("source");
    setError(null);
  }

  return (
    <section className="page" aria-labelledby="import-title">
      <header className="page__header">
        <div>
          <h2 id="import-title">Importer un coffre</h2>
          <p>
            Bitwarden, Proton Pass, KeePass, 1Password, Buttercup et exports de navigateurs. Le
            fichier est lu localement, jamais envoyé sur le réseau.
          </p>
        </div>
        {path ? (
          <button type="button" className="button" onClick={reset}>
            <Icon name="refresh" size={14} />
            Recommencer
          </button>
        ) : null}
      </header>

      <ol className="steps" aria-label="Étapes de l'import">
        <li className={step === "source" ? "steps__item steps__item--active" : "steps__item"}>
          1. Fichier
        </li>
        <li className={step === "options" ? "steps__item steps__item--active" : "steps__item"}>
          2. Options
        </li>
        <li className={step === "report" ? "steps__item steps__item--active" : "steps__item"}>
          3. Rapport
        </li>
      </ol>

      {error ? (
        <p className="form__error" role="alert">
          {error}
        </p>
      ) : null}

      {step === "source" ? (
        <div className="card">
          <div className="form__row">
            <label className="field">
              <span className="field__label">Fichier source</span>
              <div className="field__inline">
                <input
                  value={path ?? ""}
                  readOnly
                  placeholder="Aucun fichier sélectionné"
                  aria-label="Chemin du fichier source"
                />
                <button type="button" className="button" onClick={() => void chooseFile()}>
                  <Icon name="upload" size={14} />
                  Choisir…
                </button>
              </div>
            </label>
            <label className="field">
              <span className="field__label">Format</span>
              <select value={format} onChange={(event) => setFormat(event.target.value)}>
                {formats.map((entry) => (
                  <option key={entry.id} value={entry.id}>
                    {entry.label}
                  </option>
                ))}
              </select>
            </label>
          </div>

          <label className="field">
            <span className="field__label">Mot de passe du fichier (si chiffré)</span>
            <input
              type="password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              autoComplete="off"
              spellCheck={false}
            />
          </label>

          <div className="form__actions">
            <button
              type="button"
              className="button button--primary"
              onClick={() => void runPreview()}
              disabled={!path || busy}
            >
              {busy ? "Analyse…" : "Analyser le fichier"}
            </button>
          </div>
        </div>
      ) : null}

      {step === "options" && preview ? (
        <div className="card">
          <dl className="stats">
            <div>
              <dt>Entrées détectées</dt>
              <dd>{preview.itemCount}</dd>
            </div>
            <div>
              <dt>Dossiers</dt>
              <dd>{preview.folderCount}</dd>
            </div>
            <div>
              <dt>Doublons probables</dt>
              <dd>{preview.duplicates.length}</dd>
            </div>
          </dl>

          {preview.duplicates.length > 0 ? (
            <details className="details">
              <summary>Voir les doublons détectés</summary>
              <ul className="plain-list">
                {preview.duplicates.map((duplicate) => (
                  <li key={duplicate.importedExternalId}>
                    <code>{duplicate.importedExternalId}</code>
                    <span className="chip chip--quiet">{duplicate.reason}</span>
                    <span>{Math.round(duplicate.score * 100)} % de similarité</span>
                  </li>
                ))}
              </ul>
            </details>
          ) : null}

          <div className="form">
            <label className="checkbox">
              <input
                type="checkbox"
                checked={options.routeDevLocal}
                onChange={(event) =>
                  setOptions((current) => ({ ...current, routeDevLocal: event.target.checked }))
                }
              />
              <span>Router les entrées locales vers « Dev Local »</span>
            </label>
            <label className="checkbox">
              <input
                type="checkbox"
                checked={options.importDuplicates}
                onChange={(event) =>
                  setOptions((current) => ({ ...current, importDuplicates: event.target.checked }))
                }
              />
              <span>Importer les doublons au lieu de les ignorer</span>
            </label>
            <label className="checkbox">
              <input
                type="checkbox"
                checked={options.secureDeleteSource}
                onChange={(event) =>
                  setOptions((current) => ({
                    ...current,
                    secureDeleteSource: event.target.checked,
                  }))
                }
              />
              <span>Effacer le fichier source après un import réussi</span>
            </label>
            {options.secureDeleteSource ? (
              <p className="field__warn">
                <Icon name="warn" size={13} /> Le fichier sera écrasé puis supprimé. Cette action
                est irréversible.
              </p>
            ) : null}
          </div>

          <div className="form__actions">
            <button type="button" className="button" onClick={() => setStep("source")}>
              Retour
            </button>
            <button
              type="button"
              className="button button--primary"
              onClick={() => void runImport()}
              disabled={busy}
            >
              {busy ? "Import en cours…" : `Importer ${preview.itemCount} entrée(s)`}
            </button>
          </div>
        </div>
      ) : null}

      {step === "report" && report ? (
        <div className="card">
          <h3>Import terminé</h3>
          <dl className="stats">
            <div>
              <dt>Importées</dt>
              <dd>{report.imported}</dd>
            </div>
            <div>
              <dt>Ignorées</dt>
              <dd>{report.skipped}</dd>
            </div>
            <div>
              <dt>Routées Dev Local</dt>
              <dd>{report.routed}</dd>
            </div>
          </dl>
          <p className="field__hint">{describeCleanup(report.cleanup)}</p>
          {report.issues.length > 0 ? (
            <ul className="plain-list">
              {report.issues.map((issue, index) => (
                <li key={`${issue.kind}-${index}`}>
                  <span className="chip chip--warn">{describeImportIssue(issue.kind)}</span>
                  {issue.detail ? <span>{issue.detail}</span> : null}
                </li>
              ))}
            </ul>
          ) : (
            <p className="field__hint">Aucun avertissement.</p>
          )}
          <div className="form__actions">
            <button type="button" className="button button--primary" onClick={reset}>
              Importer un autre fichier
            </button>
          </div>
        </div>
      ) : null}
    </section>
  );
}
