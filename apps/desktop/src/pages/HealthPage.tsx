import { useCallback, useEffect, useMemo, useState } from "react";

import { Icon } from "../components/Icons";
import { strengthLabel } from "../lib/format";
import type {
  AkootiApi,
  BreachReport,
  HealthFinding,
  HealthProgress,
  HealthReport,
} from "../lib/ipc";
import { listenHealthProgress, toIpcError } from "../lib/ipc";

const AGE_STORAGE_KEY = "akooti.health.maxAgeDays";
const DEFAULT_MAX_AGE_DAYS = 365;
/** A password scoring below 3 is flagged by zxcvbn itself ("too weak"). */
const WEAK_BELOW = 3;

type Filter = "all" | "weak" | "reused" | "compromised" | "old";

const FILTERS: ReadonlyArray<{ id: Filter; label: string }> = [
  { id: "all", label: "Tous" },
  { id: "weak", label: "Faibles" },
  { id: "reused", label: "Réutilisés" },
  { id: "compromised", label: "Compromis" },
  { id: "old", label: "Anciens" },
];

interface HealthPageProps {
  api: AkootiApi;
  /** Opens the entry in the vault screen so the user can fix it there. */
  onOpenItem: (itemId: string) => void;
  /** Reloads the item list so the refreshed badges show up there too. */
  onAudited: () => Promise<void> | void;
}

function storedMaxAgeDays(): number {
  if (typeof localStorage === "undefined") {
    return DEFAULT_MAX_AGE_DAYS;
  }
  const parsed = Number.parseInt(localStorage.getItem(AGE_STORAGE_KEY) ?? "", 10);
  return Number.isFinite(parsed) && parsed > 0 ? parsed : DEFAULT_MAX_AGE_DAYS;
}

/**
 * Vault health dashboard.
 *
 * Nothing runs on its own: the local audit and the HIBP check are explicit
 * gestures, because both decrypt every password (the second one also sends a
 * 5-character SHA-1 prefix to a third party). The report is metadata only.
 */
export function HealthPage({ api, onOpenItem, onAudited }: HealthPageProps) {
  const [report, setReport] = useState<HealthReport | null>(null);
  const [breach, setBreach] = useState<BreachReport | null>(null);
  const [busy, setBusy] = useState<"scan" | "breach" | null>(null);
  const [progress, setProgress] = useState<HealthProgress | null>(null);
  const [filter, setFilter] = useState<Filter>("all");
  const [error, setError] = useState<string | null>(null);
  const [savedPath, setSavedPath] = useState<string | null>(null);
  const [maxAgeDays, setMaxAgeDays] = useState(storedMaxAgeDays);

  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    void listenHealthProgress((value) => {
      if (!cancelled) {
        setProgress(value);
      }
    }).then((stop) => {
      if (cancelled) {
        stop();
      } else {
        unlisten = stop;
      }
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  const runAudit = useCallback(async () => {
    setBusy("scan");
    setError(null);
    setProgress(null);
    setSavedPath(null);
    try {
      const next = await api.auditHealth();
      setReport(next);
      await onAudited();
    } catch (caught) {
      setError(toIpcError(caught).message);
    } finally {
      setBusy(null);
      setProgress(null);
    }
  }, [api, onAudited]);

  const runBreachCheck = useCallback(async () => {
    setBusy("breach");
    setError(null);
    setProgress(null);
    try {
      const result = await api.checkBreaches();
      setBreach(result);
      // The flags live in the vault: read them back before displaying them.
      setReport(await api.auditHealth());
      await onAudited();
    } catch (caught) {
      setError(toIpcError(caught).message);
    } finally {
      setBusy(null);
      setProgress(null);
    }
  }, [api, onAudited]);

  const exportReport = useCallback(async () => {
    if (!report) {
      return;
    }
    setError(null);
    try {
      const path = await api.exportHealthReport(report);
      if (path) {
        setSavedPath(path);
      }
    } catch (caught) {
      setError(toIpcError(caught).message);
    }
  }, [api, report]);

  const updateMaxAgeDays = useCallback((value: number) => {
    const clamped = Math.min(Math.max(Math.round(value), 1), 3650);
    setMaxAgeDays(clamped);
    try {
      localStorage.setItem(AGE_STORAGE_KEY, String(clamped));
    } catch {
      // Private browsing: the threshold still applies to this session.
    }
  }, []);

  const isOld = useCallback(
    (finding: HealthFinding) => finding.ageDays !== null && finding.ageDays > maxAgeDays,
    [maxAgeDays],
  );

  const counts = useMemo(() => {
    const findings = report?.findings ?? [];
    return {
      all: findings.length,
      weak: findings.filter((finding) => finding.score < WEAK_BELOW).length,
      reused: findings.filter((finding) => finding.reused).length,
      compromised: findings.filter((finding) => finding.compromised).length,
      old: findings.filter(isOld).length,
    };
  }, [isOld, report]);

  const visible = useMemo(() => {
    const findings = report?.findings ?? [];
    const matches = (finding: HealthFinding) => {
      switch (filter) {
        case "weak":
          return finding.score < WEAK_BELOW;
        case "reused":
          return finding.reused;
        case "compromised":
          return finding.compromised;
        case "old":
          return isOld(finding);
        default:
          return true;
      }
    };
    return findings
      .filter(matches)
      .sort((left, right) => left.score - right.score || left.title.localeCompare(right.title));
  }, [filter, isOld, report]);

  const phaseLabel = progress?.phase === "breach" ? "Vérification HIBP" : "Analyse locale";

  return (
    <section className="page" aria-labelledby="health-title">
      <header className="page__header">
        <div>
          <h2 id="health-title">Santé du coffre</h2>
          <p>
            Force, réutilisation et ancienneté sont calculées localement avec zxcvbn. Aucun mot de
            passe, aucun score et aucune valeur dérivée ne quittent la machine.
          </p>
        </div>
        <div className="form__actions">
          <button
            type="button"
            className="button button--primary"
            onClick={() => void runAudit()}
            disabled={busy !== null}
          >
            <Icon name="shield" size={14} />
            {busy === "scan" ? "Analyse…" : "Analyser le coffre"}
          </button>
          <button
            type="button"
            className="button"
            onClick={() => void exportReport()}
            disabled={!report || busy !== null}
          >
            <Icon name="copy" size={14} />
            Exporter le rapport
          </button>
        </div>
      </header>

      {error ? (
        <p className="form__error" role="alert">
          {error}
        </p>
      ) : null}

      {progress ? (
        <div className="card" role="status" aria-live="polite">
          <span>
            {phaseLabel} · {progress.done}/{Math.max(progress.total, 1)}
          </span>
          <progress max={Math.max(progress.total, 1)} value={progress.done} />
        </div>
      ) : null}

      {savedPath ? (
        <p className="chip chip--quiet" role="status">
          Rapport enregistré dans {savedPath}
        </p>
      ) : null}

      {report ? (
        <>
          <dl className="stats">
            <div>
              <dt>Comptes analysés</dt>
              <dd>{counts.all}</dd>
            </div>
            <div>
              <dt>Score moyen</dt>
              <dd>{report.averageScore === null ? "—" : report.averageScore.toFixed(1)}</dd>
            </div>
            <div>
              <dt>Faibles</dt>
              <dd>{counts.weak}</dd>
            </div>
            <div>
              <dt>Réutilisés</dt>
              <dd>{counts.reused}</dd>
            </div>
            <div>
              <dt>Compromis</dt>
              <dd>{counts.compromised}</dd>
            </div>
            <div>
              <dt>Anciens</dt>
              <dd>{counts.old}</dd>
            </div>
          </dl>

          <div className="card">
            <div className="form__row">
              <label className="field">
                <span className="field__label">Ancienneté à surveiller (jours)</span>
                <input
                  type="number"
                  min={1}
                  max={3650}
                  value={maxAgeDays}
                  onChange={(event) => updateMaxAgeDays(Number(event.target.value))}
                />
              </label>
              <p className="field__label">
                {report.withoutPassword} entrée(s) sans mot de passe, hors analyse.
              </p>
            </div>

            <div className="chips" role="group" aria-label="Filtrer le rapport">
              {FILTERS.map((entry) => (
                <button
                  key={entry.id}
                  type="button"
                  className={filter === entry.id ? "chip chip--action" : "chip"}
                  aria-pressed={filter === entry.id}
                  onClick={() => setFilter(entry.id)}
                >
                  {entry.label} ({counts[entry.id]})
                </button>
              ))}
            </div>
          </div>

          <div className="card">
            <h3>Vérification des fuites connues</h3>
            <p className="field__label">
              Envoie uniquement les 5 premiers caractères du SHA-1 à l'API HIBP (k-anonymity) ; la
              comparaison des suffixes se fait ici. Hors ligne, le résultat reste « inconnu ».
            </p>
            {breach ? (
              <p role="status">
                {breach.checked} compte(s) vérifié(s) · {breach.compromised} compromis ·{" "}
                {breach.unknown} inconnu(s)
              </p>
            ) : null}
            <div className="form__actions">
              <button
                type="button"
                className="button"
                onClick={() => void runBreachCheck()}
                disabled={busy !== null}
              >
                <Icon name="warn" size={14} />
                {busy === "breach" ? "Vérification…" : "Vérifier les fuites (HIBP)"}
              </button>
            </div>
          </div>

          {visible.length === 0 ? (
            <div className="empty-state">
              <Icon name="check" size={24} />
              <p>Aucun compte dans ce filtre.</p>
            </div>
          ) : (
            <ul className="audit-list">
              {visible.map((finding) => (
                <li key={finding.itemId}>
                  <button
                    type="button"
                    className="audit health__row"
                    onClick={() => onOpenItem(finding.itemId)}
                  >
                    <span className={`health__score health__score--${finding.score}`}>
                      {finding.score}
                    </span>
                    <span className="health__title">{finding.title}</span>
                    <span className="audit__target">
                      {finding.username ?? finding.primaryUrl ?? "—"}
                    </span>
                    <span className="chip chip--quiet">{strengthLabel(finding.score)}</span>
                    {finding.reused ? <span className="chip chip--warn">Réutilisé</span> : null}
                    {finding.compromised ? (
                      <span className="chip chip--warn">Compromis</span>
                    ) : null}
                    {isOld(finding) ? (
                      <span className="chip chip--warn">{finding.ageDays} j</span>
                    ) : null}
                  </button>
                </li>
              ))}
            </ul>
          )}
        </>
      ) : (
        <div className="empty-state">
          <Icon name="shield" size={28} />
          <p>Lancez une analyse pour obtenir le rapport de santé du coffre.</p>
        </div>
      )}
    </section>
  );
}
