import { useState } from "react";

import type { Folder, SecretField, VaultItem } from "@akooti/shared";

import { absoluteDate, hostOf, itemTypeLabel, relativeTime } from "../lib/format";
import type { CustomFieldInput } from "../lib/ipc";
import { Icon } from "./Icons";

interface ItemDetailProps {
  item: VaultItem;
  folders: Folder[];
  revealedValue: string | null;
  revealedField: SecretField | null;
  revealedCustomFieldId: string | null;
  revealRemaining: number;
  revealPending: boolean;
  onReveal: (field: SecretField, customFieldId?: string) => void;
  onHideReveal: () => void;
  onCopy: (value: string, label: string) => void;
  totpCode: string | null;
  totpSeconds: number;
  onEdit: () => void;
  onEditSecrets: () => void;
  onDelete: () => void;
  onToggleFavorite: (favorite: boolean) => void;
  onAddToFolder: (folderId: string) => void;
  onRemoveFromFolder: (folderId: string) => void;
  onAddCustomField: (field: CustomFieldInput) => void;
  onRemoveCustomField: (fieldId: string) => void;
  onOpenGenerator: () => void;
}

const MASK = "••••••••••••";

/**
 * Right column: everything about one entry.
 *
 * Secrets are masked by default (`docs/regles.md` §3.2). Revealing is an
 * explicit gesture with a visible countdown, and copying hands the value to the
 * backend, which clears the clipboard 30 seconds later.
 */
export function ItemDetail({
  item,
  folders,
  revealedValue,
  revealedField,
  revealedCustomFieldId,
  revealRemaining,
  revealPending,
  onReveal,
  onHideReveal,
  onCopy,
  totpCode,
  totpSeconds,
  onEdit,
  onEditSecrets,
  onDelete,
  onToggleFavorite,
  onAddToFolder,
  onRemoveFromFolder,
  onAddCustomField,
  onRemoveCustomField,
  onOpenGenerator,
}: ItemDetailProps) {
  const [addingFolder, setAddingFolder] = useState(false);
  const [newField, setNewField] = useState<CustomFieldInput>({
    fieldName: "",
    value: "",
    isSecret: true,
    sortOrder: 0,
  });
  const [showFieldForm, setShowFieldForm] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const passwordShown = revealedField === "password";
  const notesShown = revealedField === "notes";
  const missingFolders = folders.filter((folder) => !item.folderIds.includes(folder.id));

  return (
    <section className="detail" aria-label={`Détail de ${item.title}`}>
      <header className="detail__header">
        <div>
          <p className="detail__type">{itemTypeLabel(item.itemType)}</p>
          <h2>{item.title}</h2>
          {item.primaryUrl ? (
            <p className="detail__url">
              <Icon name="folder" size={13} />
              {hostOf(item.primaryUrl)}
            </p>
          ) : null}
        </div>
        <div className="detail__header-actions">
          <button
            type="button"
            className="icon-button"
            onClick={() => onToggleFavorite(!item.favorite)}
            aria-pressed={item.favorite}
            aria-label={item.favorite ? "Retirer des favoris" : "Ajouter aux favoris"}
            title={item.favorite ? "Retirer des favoris" : "Ajouter aux favoris"}
          >
            <Icon name="star" size={16} className={item.favorite ? "icon--filled" : undefined} />
          </button>
          <button type="button" className="button" onClick={onEdit}>
            Modifier
          </button>
          <button type="button" className="button" onClick={onEditSecrets}>
            Secrets
          </button>
          <button
            type="button"
            className="button button--danger"
            onClick={() => setConfirmDelete(true)}
          >
            <Icon name="trash" size={15} />
            Supprimer
          </button>
        </div>
      </header>

      {confirmDelete ? (
        <div className="confirm" role="group" aria-label="Confirmer la suppression">
          <p>
            Supprimer « {item.title} » ? L'entrée part en corbeille et peut être restaurée plus
            tard.
          </p>
          <div className="confirm__actions">
            <button type="button" className="button" onClick={() => setConfirmDelete(false)}>
              Annuler
            </button>
            <button
              type="button"
              className="button button--danger"
              onClick={() => {
                setConfirmDelete(false);
                onDelete();
              }}
            >
              Confirmer la suppression
            </button>
          </div>
        </div>
      ) : null}

      <div className="detail__grid">
        <div className="detail__card">
          <h3>Identifiant</h3>
          <div className="detail__value">
            <code>{item.username ?? "—"}</code>
            {item.username ? (
              <button
                type="button"
                className="icon-button"
                onClick={() => onCopy(item.username ?? "", "Identifiant")}
                aria-label="Copier l'identifiant"
                title="Copier (Ctrl B)"
              >
                <Icon name="copy" size={14} />
              </button>
            ) : null}
          </div>
        </div>

        <div className="detail__card">
          <h3>Mot de passe</h3>
          {item.hasPassword ? (
            <>
              <div className="detail__value">
                <code className={passwordShown ? "secret secret--visible" : "secret"}>
                  {passwordShown ? revealedValue : MASK}
                </code>
                <button
                  type="button"
                  className="icon-button"
                  onClick={() =>
                    passwordShown && revealedValue
                      ? onCopy(revealedValue, "Mot de passe")
                      : onReveal("password")
                  }
                  disabled={revealPending && !passwordShown}
                  aria-label={
                    passwordShown ? "Copier le mot de passe" : "Révéler puis copier le mot de passe"
                  }
                  title={passwordShown ? "Copier" : "Révéler 30 s"}
                >
                  <Icon name={passwordShown ? "copy" : "eye"} size={14} />
                </button>
                {passwordShown ? (
                  <button
                    type="button"
                    className="icon-button"
                    onClick={onHideReveal}
                    aria-label="Masquer le mot de passe"
                  >
                    <Icon name="eyeOff" size={14} />
                  </button>
                ) : null}
              </div>
              {passwordShown ? (
                <p className="detail__countdown" role="timer" aria-live="off">
                  <Icon name="clock" size={12} /> Masqué dans {revealRemaining} s
                </p>
              ) : (
                <p className="field__hint">
                  Masqué · dernier changement {relativeTime(item.passwordUpdatedAt)}
                </p>
              )}
              <button type="button" className="text-button" onClick={onOpenGenerator}>
                <Icon name="wand" size={12} /> Changer avec le générateur
              </button>
            </>
          ) : (
            <p className="field__hint">Aucun mot de passe enregistré.</p>
          )}
        </div>

        <div className="detail__card">
          <h3>Code TOTP</h3>
          {item.hasTotp ? (
            <>
              <div className="detail__value">
                <code className="totp" aria-live="polite">
                  {totpCode ? `${totpCode.slice(0, 3)} ${totpCode.slice(3)}` : "— — —"}
                </code>
                <button
                  type="button"
                  className="icon-button"
                  onClick={() => totpCode && onCopy(totpCode, "Code TOTP")}
                  disabled={!totpCode}
                  aria-label="Copier le code TOTP"
                  title="Copier (Ctrl T)"
                >
                  <Icon name="copy" size={14} />
                </button>
              </div>
              <div
                className="totp-bar"
                role="progressbar"
                aria-valuemin={0}
                aria-valuemax={30}
                aria-valuenow={totpSeconds}
                aria-label="Validité du code"
              >
                <span style={{ width: `${(totpSeconds / 30) * 100}%` }} />
              </div>
              <p className="field__hint">Renouvelé dans {totpSeconds} s</p>
            </>
          ) : (
            <p className="field__hint">Aucune graine TOTP enregistrée.</p>
          )}
        </div>

        <div className="detail__card">
          <h3>Notes sécurisées</h3>
          {item.hasNotes ? (
            <div className="detail__value detail__value--column">
              <code className={notesShown ? "secret secret--visible" : "secret"}>
                {notesShown ? revealedValue : MASK}
              </code>
              <div className="field__inline">
                {notesShown ? (
                  <>
                    <button type="button" className="text-button" onClick={onHideReveal}>
                      <Icon name="eyeOff" size={12} /> Masquer
                    </button>
                    <button
                      type="button"
                      className="text-button"
                      onClick={() => revealedValue && onCopy(revealedValue, "Notes")}
                    >
                      <Icon name="copy" size={12} /> Copier
                    </button>
                  </>
                ) : (
                  <button type="button" className="text-button" onClick={() => onReveal("notes")}>
                    <Icon name="eye" size={12} /> Révéler 30 s
                  </button>
                )}
              </div>
            </div>
          ) : (
            <p className="field__hint">Aucune note enregistrée.</p>
          )}
        </div>
      </div>

      <section className="detail__section">
        <h3>Dossiers</h3>
        <div className="chips">
          {item.folderIds.map((folderId) => {
            const folder = folders.find((candidate) => candidate.id === folderId);
            return (
              <span key={folderId} className="chip">
                <Icon name={folder?.icon === "terminal" ? "terminal" : "folder"} size={12} />
                {folder?.name ?? "Dossier supprimé"}
                <button
                  type="button"
                  className="chip__remove"
                  onClick={() => onRemoveFromFolder(folderId)}
                  disabled={item.folderIds.length === 1}
                  title="Au moins un dossier est requis"
                  aria-label={`Retirer du dossier ${folder?.name ?? folderId}`}
                >
                  <Icon name="close" size={11} />
                </button>
              </span>
            );
          })}
          {item.folderIds.length === 1 ? (
            <span className="field__hint">Au moins un dossier est requis.</span>
          ) : null}
        </div>
        {missingFolders.length > 0 ? (
          addingFolder ? (
            <div className="chips">
              {missingFolders.map((folder) => (
                <button
                  key={folder.id}
                  type="button"
                  className="chip chip--action"
                  onClick={() => {
                    onAddToFolder(folder.id);
                    setAddingFolder(false);
                  }}
                >
                  <Icon name="plus" size={11} />
                  {folder.name}
                </button>
              ))}
              <button type="button" className="text-button" onClick={() => setAddingFolder(false)}>
                Annuler
              </button>
            </div>
          ) : (
            <button type="button" className="text-button" onClick={() => setAddingFolder(true)}>
              <Icon name="plus" size={12} /> Ajouter à un dossier
            </button>
          )
        ) : null}
      </section>

      <section className="detail__section">
        <h3>URL associées</h3>
        {item.uris.length === 0 ? (
          <p className="field__hint">Aucune URL associée.</p>
        ) : (
          <ul className="plain-list">
            {item.uris.map((uri) => (
              <li key={uri.id}>
                <code>{uri.uri}</code>
                <span className="chip chip--quiet">{uri.matchType}</span>
              </li>
            ))}
          </ul>
        )}
      </section>

      <section className="detail__section">
        <h3>
          Champs personnalisés
          <button
            type="button"
            className="icon-button icon-button--small"
            onClick={() => setShowFieldForm((current) => !current)}
            aria-label="Ajouter un champ personnalisé"
            aria-expanded={showFieldForm}
          >
            <Icon name="plus" size={13} />
          </button>
        </h3>

        {showFieldForm ? (
          <form
            className="field__inline"
            onSubmit={(event) => {
              event.preventDefault();
              if (newField.fieldName.trim().length === 0) {
                return;
              }
              onAddCustomField({
                ...newField,
                fieldName: newField.fieldName.trim(),
                sortOrder: item.customFields.length,
              });
              setNewField({ fieldName: "", value: "", isSecret: true, sortOrder: 0 });
              setShowFieldForm(false);
            }}
          >
            <input
              value={newField.fieldName}
              onChange={(event) =>
                setNewField((current) => ({ ...current, fieldName: event.target.value }))
              }
              placeholder="Nom du champ"
              aria-label="Nom du champ"
              maxLength={128}
            />
            <input
              type={newField.isSecret ? "password" : "text"}
              value={newField.value}
              onChange={(event) =>
                setNewField((current) => ({ ...current, value: event.target.value }))
              }
              placeholder="Valeur"
              aria-label="Valeur du champ"
            />
            <label className="checkbox">
              <input
                type="checkbox"
                checked={newField.isSecret}
                onChange={(event) =>
                  setNewField((current) => ({ ...current, isSecret: event.target.checked }))
                }
              />
              <span>Secret</span>
            </label>
            <button type="submit" className="button button--primary">
              Ajouter
            </button>
          </form>
        ) : null}

        {item.customFields.length === 0 ? (
          <p className="field__hint">Aucun champ personnalisé.</p>
        ) : (
          <ul className="plain-list">
            {item.customFields.map((field) => {
              const isRevealed =
                revealedField === "custom_field" &&
                revealedCustomFieldId === field.id &&
                revealedValue !== null &&
                field.isSecret;
              return (
                <li key={field.id}>
                  <span>{field.fieldName}</span>
                  {field.isSecret ? (
                    <>
                      <code className={isRevealed ? "secret secret--visible" : "secret"}>
                        {isRevealed ? revealedValue : MASK}
                      </code>
                      <button
                        type="button"
                        className="icon-button icon-button--small"
                        onClick={() => onReveal("custom_field", field.id)}
                        aria-label={`Révéler ${field.fieldName}`}
                      >
                        <Icon name="eye" size={13} />
                      </button>
                    </>
                  ) : (
                    <span className="chip chip--quiet">non secret</span>
                  )}
                  <button
                    type="button"
                    className="icon-button icon-button--small"
                    onClick={() => onRemoveCustomField(field.id)}
                    aria-label={`Supprimer le champ ${field.fieldName}`}
                  >
                    <Icon name="trash" size={13} />
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      <footer className="detail__footer">
        <span>Créé le {absoluteDate(item.createdAt)}</span>
        <span>Modifié {relativeTime(item.updatedAt)}</span>
        {item.createdByAgent ? <span>Agent : {item.createdByAgent}</span> : null}
        {item.expiresAt ? <span>Expire le {absoluteDate(item.expiresAt)}</span> : null}
      </footer>
    </section>
  );
}
