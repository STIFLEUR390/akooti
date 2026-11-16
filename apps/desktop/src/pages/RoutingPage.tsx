import { useCallback, useEffect, useState } from "react";

import type { Folder, RoutingPatternType, RoutingRule } from "@akooti/shared";

import type { Notices } from "../hooks/useToasts";
import type { AkootiApi } from "../lib/ipc";
import { toIpcError } from "../lib/ipc";
import { Icon } from "../components/Icons";

interface RoutingPageProps {
  api: AkootiApi;
  folders: Folder[];
  notices: Notices;
  onApplied: () => void;
}

const PATTERN_TYPES: { value: RoutingPatternType; label: string; placeholder: string }[] = [
  { value: "suffix", label: "Suffixe de domaine", placeholder: ".internal" },
  { value: "exact_host", label: "Hôte exact", placeholder: "mon-service.test" },
  { value: "cidr", label: "Plage CIDR", placeholder: "10.0.0.0/8" },
  { value: "regex", label: "Expression régulière", placeholder: "^https?://.*\\.svc\\." },
];

const DEV_LOCAL_ID = "00000000-0000-4000-8000-000000000002";

/**
 * `Dev Local` routing rules.
 *
 * Routing is additive by construction: a rule can only add a folder
 * membership, never remove one (`docs/regles.md` §2). The retroactive pass runs
 * explicitly and reports how many memberships it added.
 */
export function RoutingPage({ api, folders, notices, onApplied }: RoutingPageProps) {
  const [rules, setRules] = useState<RoutingRule[]>([]);
  const [pattern, setPattern] = useState("");
  const [patternType, setPatternType] = useState<RoutingPatternType>("suffix");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [lastReport, setLastReport] = useState<number | null>(null);

  const targetFolderId = folders.find((folder) => folder.slug === "dev-local")?.id ?? DEV_LOCAL_ID;

  const load = useCallback(async () => {
    try {
      setRules(await api.listRoutingRules());
    } catch (caught) {
      setError(toIpcError(caught).message);
    }
  }, [api]);

  useEffect(() => {
    void load();
  }, [load]);

  const toggle = useCallback(
    async (rule: RoutingRule) => {
      try {
        const updated = await api.setRoutingRuleEnabled(rule.id, !rule.enabled);
        setRules((current) =>
          current.map((candidate) => (candidate.id === updated.id ? updated : candidate)),
        );
        notices.success(
          `Règle « ${updated.pattern} » ${updated.enabled ? "activée" : "désactivée"}.`,
        );
      } catch (caught) {
        notices.failure(toIpcError(caught).message);
      }
    },
    [api, notices],
  );

  const add = useCallback(async () => {
    const trimmed = pattern.trim();
    if (trimmed.length === 0) {
      setError("Indiquez un motif.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const created = await api.addRoutingRule({
        pattern: trimmed,
        patternType,
        targetFolderId,
        enabled: true,
      });
      setRules((current) => [...current, created]);
      setPattern("");
      notices.success(`Règle « ${created.pattern} » ajoutée.`);
    } catch (caught) {
      setError(toIpcError(caught).message);
    } finally {
      setBusy(false);
    }
  }, [api, notices, pattern, patternType, targetFolderId]);

  const apply = useCallback(async () => {
    setBusy(true);
    setError(null);
    try {
      const added = await api.applyRoutingRules();
      setLastReport(added);
      notices.success(
        added === 0
          ? "Aucune nouvelle entrée à router."
          : `${added} entrée(s) ajoutée(s) à « Dev Local ».`,
      );
      onApplied();
    } catch (caught) {
      setError(toIpcError(caught).message);
    } finally {
      setBusy(false);
    }
  }, [api, notices, onApplied]);

  return (
    <section className="page" aria-labelledby="routing-title">
      <header className="page__header">
        <div>
          <h2 id="routing-title">Routage Dev Local</h2>
          <p>
            Les motifs ci-dessous classent automatiquement les entrées locales dans le dossier
            protégé « Dev Local ». Le routage <strong>ajoute</strong> un dossier et n'en retire
            jamais.
          </p>
        </div>
        <button
          type="button"
          className="button button--primary"
          onClick={() => void apply()}
          disabled={busy}
        >
          <Icon name="refresh" size={14} />
          Appliquer le tri rétroactif
        </button>
      </header>

      {lastReport !== null ? (
        <p className="field__hint" role="status">
          Dernier passage : {lastReport} appartenance(s) ajoutée(s).
        </p>
      ) : null}

      {error ? (
        <p className="form__error" role="alert">
          {error}
        </p>
      ) : null}

      {rules.length === 0 ? (
        <div className="card">
          <p className="empty-state">
            <Icon name="sliders" size={20} />
            Aucune règle de routage. Ajoutez un suffixe ci-dessous pour commencer.
          </p>
        </div>
      ) : (
        <ul className="rule-list">
          {rules.map((rule) => (
            <li key={rule.id} className="rule">
              <div className="rule__pattern">
                <code>{rule.pattern}</code>
                <span className="chip chip--quiet">{rule.patternType}</span>
                {rule.isDefault ? <span className="chip chip--quiet">par défaut</span> : null}
              </div>
              <label className="switch">
                <input
                  type="checkbox"
                  checked={rule.enabled}
                  onChange={() => void toggle(rule)}
                  aria-label={`${rule.enabled ? "Désactiver" : "Activer"} la règle ${rule.pattern}`}
                />
                <span className="switch__track" aria-hidden="true">
                  <span className="switch__thumb" />
                </span>
                <span className="switch__label">{rule.enabled ? "Active" : "Inactive"}</span>
              </label>
            </li>
          ))}
        </ul>
      )}

      <form
        className="card"
        onSubmit={(event) => {
          event.preventDefault();
          void add();
        }}
      >
        <h3>Ajouter une règle</h3>
        <div className="form__row">
          <label className="field">
            <span className="field__label">Motif</span>
            <input
              value={pattern}
              onChange={(event) => setPattern(event.target.value)}
              placeholder={PATTERN_TYPES.find((type) => type.value === patternType)?.placeholder}
              maxLength={1024}
              spellCheck={false}
            />
          </label>
          <label className="field">
            <span className="field__label">Type de motif</span>
            <select
              value={patternType}
              onChange={(event) => setPatternType(event.target.value as RoutingPatternType)}
            >
              {PATTERN_TYPES.map((type) => (
                <option key={type.value} value={type.value}>
                  {type.label}
                </option>
              ))}
            </select>
          </label>
        </div>
        <p className="field__hint">
          Cible : {folders.find((folder) => folder.id === targetFolderId)?.name ?? "Dev Local"}
        </p>
        <div className="form__actions">
          <button type="submit" className="button button--primary" disabled={busy}>
            <Icon name="plus" size={14} />
            Ajouter la règle
          </button>
        </div>
      </form>
    </section>
  );
}
