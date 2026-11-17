import { useMemo, useState } from "react";

import type { Folder, UriMatchType, VaultItem, VaultItemType } from "@akooti/shared";

import { estimatePassword } from "../lib/strength";
import type { CreateItemInput, GeneratedSecret, UpdateItemInput } from "../lib/ipc";
import { toIpcError } from "../lib/ipc";
import { Modal } from "./Modal";
import { PasswordStrength } from "./PasswordStrength";

interface ItemFormModalProps {
  item: VaultItem | null;
  folders: Folder[];
  defaultFolderId: string | null;
  onClose: () => void;
  onCreate: (draft: CreateItemInput) => Promise<VaultItem | null>;
  onUpdate: (draft: UpdateItemInput) => Promise<VaultItem | null>;
  generatePassword: () => Promise<GeneratedSecret>;
}

const TYPES: { value: VaultItemType; label: string }[] = [
  { value: "login", label: "Identifiant" },
  { value: "api_key", label: "Clé d'API" },
  { value: "note", label: "Note sécurisée" },
  { value: "env_set", label: "Variables d'environnement" },
];

const MATCH_TYPES: { value: UriMatchType; label: string }[] = [
  { value: "domain", label: "Domaine" },
  { value: "host", label: "Hôte exact" },
  { value: "starts_with", label: "Commence par" },
  { value: "exact", label: "URL exacte" },
  { value: "regex", label: "Expression régulière" },
];

/**
 * Create/edit form for the **metadata** of an entry.
 *
 * In edit mode no secret field is rendered: changing a password goes through
 * {@link SecretEditorModal} so the UI never has to load the current value.
 */
export function ItemFormModal({
  item,
  folders,
  defaultFolderId,
  onClose,
  onCreate,
  onUpdate,
  generatePassword,
}: ItemFormModalProps) {
  const editing = item !== null;
  const [itemType, setItemType] = useState<VaultItemType>(item?.itemType ?? "login");
  const [title, setTitle] = useState(item?.title ?? "");
  const [primaryUrl, setPrimaryUrl] = useState(item?.primaryUrl ?? "");
  const [username, setUsername] = useState(item?.username ?? "");
  const [password, setPassword] = useState("");
  const [favorite, setFavorite] = useState(item?.favorite ?? false);
  const [folderIds, setFolderIds] = useState<string[]>(
    item?.folderIds ?? (defaultFolderId ? [defaultFolderId] : []),
  );
  const [selectors, setSelectors] = useState({
    username: item?.selectors.username ?? "",
    password: item?.selectors.password ?? "",
    totp: item?.selectors.totp ?? "",
    submit: item?.selectors.submit ?? "",
  });
  const [showSelectors, setShowSelectors] = useState(
    Boolean(
      item?.selectors.username ||
      item?.selectors.password ||
      item?.selectors.totp ||
      item?.selectors.submit,
    ),
  );
  const [uris, setUris] = useState(
    item?.uris.map((uri) => ({ uri: uri.uri, matchType: uri.matchType })) ?? [],
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const strength = useMemo(
    () => (password.length > 0 ? estimatePassword(password) : null),
    [password],
  );

  function toggleFolder(id: string) {
    setFolderIds((current) =>
      current.includes(id) ? current.filter((value) => value !== id) : [...current, id],
    );
  }

  async function handleGenerate() {
    try {
      const generated = await generatePassword();
      setPassword(generated.value);
    } catch (caught) {
      setError(toIpcError(caught).message);
    }
  }

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    const trimmed = title.trim();
    if (trimmed.length === 0) {
      setError("Le titre est obligatoire.");
      return;
    }
    if (folderIds.length === 0) {
      setError("Sélectionnez au moins un dossier.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const normalizedUris = uris
        .map((uri) => ({ uri: uri.uri.trim(), matchType: uri.matchType }))
        .filter((uri) => uri.uri.length > 0);
      if (editing && item) {
        const updated = await onUpdate({
          id: item.id,
          itemType,
          title: trimmed,
          primaryUrl: primaryUrl.trim() || null,
          username: username.trim() || null,
          selectors,
          favorite,
          expiresAt: item.expiresAt,
          folderIds,
          uris: normalizedUris,
        });
        if (updated) {
          onClose();
        }
      } else {
        const created = await onCreate({
          itemType,
          title: trimmed,
          primaryUrl: primaryUrl.trim() || null,
          username: username.trim() || null,
          password: password.length > 0 ? password : null,
          totpSecret: null,
          notes: null,
          selectors,
          favorite,
          expiresAt: null,
          folderIds,
          uris: normalizedUris,
          customFields: [],
        });
        if (created) {
          onClose();
        }
      }
    } finally {
      setBusy(false);
    }
  }

  return (
    <Modal
      wide
      title={editing ? `Modifier « ${item.title} »` : "Nouvelle entrée"}
      description={
        editing
          ? "Le mot de passe et les autres secrets ne sont jamais chargés par ce formulaire."
          : "Les champs marqués d'un astérisque sont obligatoires."
      }
      onClose={onClose}
    >
      <form className="form" onSubmit={submit} id="item-form">
        <div className="form__row">
          <label className="field">
            <span className="field__label">Type *</span>
            <select
              value={itemType}
              onChange={(event) => setItemType(event.target.value as VaultItemType)}
            >
              {TYPES.map((type) => (
                <option key={type.value} value={type.value}>
                  {type.label}
                </option>
              ))}
            </select>
          </label>
          <label className="field">
            <span className="field__label">Titre *</span>
            <input
              value={title}
              onChange={(event) => setTitle(event.target.value)}
              maxLength={200}
              required
            />
          </label>
        </div>

        <div className="form__row">
          <label className="field">
            <span className="field__label">URL principale</span>
            <input
              value={primaryUrl}
              onChange={(event) => setPrimaryUrl(event.target.value)}
              placeholder="https://exemple.test"
              inputMode="url"
              spellCheck={false}
            />
          </label>
          <label className="field">
            <span className="field__label">Identifiant</span>
            <input
              value={username}
              onChange={(event) => setUsername(event.target.value)}
              autoComplete="off"
              spellCheck={false}
            />
          </label>
        </div>

        {!editing ? (
          <div className="field">
            <span className="field__label">Mot de passe</span>
            <div className="field__inline">
              <input
                type="password"
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                autoComplete="new-password"
                aria-label="Mot de passe"
                spellCheck={false}
              />
              <button type="button" className="button" onClick={handleGenerate}>
                Générer
              </button>
            </div>
            {strength ? (
              <PasswordStrength score={strength.score} hint={`${strength.entropyBits} bits`} />
            ) : null}
          </div>
        ) : null}

        <fieldset className="fieldset">
          <legend>Dossiers</legend>
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

        <label className="checkbox">
          <input
            type="checkbox"
            checked={favorite}
            onChange={(event) => setFavorite(event.target.checked)}
          />
          <span>Marquer comme favori</span>
        </label>

        <fieldset className="fieldset">
          <legend>URL associées</legend>
          {uris.length === 0 ? (
            <p className="field__hint">Aucune URL associée.</p>
          ) : (
            <ul className="uri-editor">
              {uris.map((uri, index) => (
                <li key={`${uri.uri}-${index}`} className="field__inline">
                  <input
                    value={uri.uri}
                    onChange={(event) =>
                      setUris((current) =>
                        current.map((value, position) =>
                          position === index ? { ...value, uri: event.target.value } : value,
                        ),
                      )
                    }
                    aria-label={`URL ${index + 1}`}
                    spellCheck={false}
                  />
                  <select
                    value={uri.matchType}
                    onChange={(event) =>
                      setUris((current) =>
                        current.map((value, position) =>
                          position === index
                            ? { ...value, matchType: event.target.value as UriMatchType }
                            : value,
                        ),
                      )
                    }
                    aria-label={`Type de correspondance de l'URL ${index + 1}`}
                  >
                    {MATCH_TYPES.map((type) => (
                      <option key={type.value} value={type.value}>
                        {type.label}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    className="button button--danger"
                    onClick={() =>
                      setUris((current) => current.filter((_, position) => position !== index))
                    }
                  >
                    Retirer
                  </button>
                </li>
              ))}
            </ul>
          )}
          <button
            type="button"
            className="button"
            onClick={() =>
              setUris((current) => [...current, { uri: "", matchType: "domain" as UriMatchType }])
            }
          >
            Ajouter une URL
          </button>
        </fieldset>

        <fieldset className="fieldset">
          <legend>
            <button
              type="button"
              className="text-button"
              onClick={() => setShowSelectors((current) => !current)}
              aria-expanded={showSelectors}
            >
              {showSelectors ? "Masquer" : "Afficher"} les sélecteurs d'autofill
            </button>
          </legend>
          {showSelectors ? (
            <div className="form__row">
              <label className="field">
                <span className="field__label">Sélecteur identifiant</span>
                <input
                  value={selectors.username}
                  onChange={(event) =>
                    setSelectors((current) => ({ ...current, username: event.target.value }))
                  }
                  placeholder="#email"
                  spellCheck={false}
                />
              </label>
              <label className="field">
                <span className="field__label">Sélecteur mot de passe</span>
                <input
                  value={selectors.password}
                  onChange={(event) =>
                    setSelectors((current) => ({ ...current, password: event.target.value }))
                  }
                  placeholder="#password"
                  spellCheck={false}
                />
              </label>
              <label className="field">
                <span className="field__label">Sélecteur TOTP</span>
                <input
                  value={selectors.totp}
                  onChange={(event) =>
                    setSelectors((current) => ({ ...current, totp: event.target.value }))
                  }
                  placeholder="#otp"
                  spellCheck={false}
                />
              </label>
              <label className="field">
                <span className="field__label">Sélecteur validation</span>
                <input
                  value={selectors.submit}
                  onChange={(event) =>
                    setSelectors((current) => ({ ...current, submit: event.target.value }))
                  }
                  placeholder="button[type=submit]"
                  spellCheck={false}
                />
              </label>
            </div>
          ) : null}
        </fieldset>

        {error ? (
          <p className="form__error" role="alert">
            {error}
          </p>
        ) : null}

        <footer className="form__actions">
          <button type="button" className="button" onClick={onClose}>
            Annuler
          </button>
          <button type="submit" className="button button--primary" disabled={busy}>
            {editing ? "Enregistrer" : "Créer l'entrée"}
          </button>
        </footer>
      </form>
    </Modal>
  );
}
