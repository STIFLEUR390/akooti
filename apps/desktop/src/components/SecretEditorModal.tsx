import { useMemo, useState } from "react";

import type { VaultItem } from "@akooti/shared";

import type { GeneratedSecret } from "../lib/ipc";
import { toIpcError } from "../lib/ipc";
import { estimatePassword } from "../lib/strength";
import { Modal } from "./Modal";
import { PasswordStrength } from "./PasswordStrength";

type SecretFieldName = "password" | "totp" | "notes";

interface SecretEditorModalProps {
  item: VaultItem;
  onClose: () => void;
  onSetSecret: (field: SecretFieldName, value: string | null) => Promise<boolean>;
  generatePassword: () => Promise<GeneratedSecret>;
}

/**
 * Writes one encrypted field at a time.
 *
 * The form never receives the stored value: an empty input means "leave it
 * alone", and clearing is an explicit, separate gesture.
 */
export function SecretEditorModal({
  item,
  onClose,
  onSetSecret,
  generatePassword,
}: SecretEditorModalProps) {
  const [password, setPassword] = useState("");
  const [totp, setTotp] = useState("");
  const [notes, setNotes] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState<SecretFieldName | null>(null);

  const strength = useMemo(
    () => (password.length > 0 ? estimatePassword(password) : null),
    [password],
  );

  async function write(field: SecretFieldName, value: string | null) {
    setBusy(field);
    setError(null);
    try {
      const done = await onSetSecret(field, value);
      if (!done) {
        setError("L'enregistrement a échoué.");
      }
    } catch (caught) {
      setError(toIpcError(caught).message);
    } finally {
      setBusy(null);
    }
  }

  return (
    <Modal
      title={`Secrets de « ${item.title} »`}
      description="Chaque champ est chiffré séparément. Laisser vide ne modifie rien."
      onClose={onClose}
    >
      <div className="form">
        <section className="secret-editor">
          <h3>Mot de passe</h3>
          <p className="field__hint">
            {item.hasPassword
              ? "Un mot de passe est enregistré. Il n'est jamais préchargé ici."
              : "Aucun mot de passe enregistré."}
          </p>
          <div className="field__inline">
            <input
              type="password"
              value={password}
              onChange={(event) => setPassword(event.target.value)}
              autoComplete="new-password"
              aria-label="Nouveau mot de passe"
              spellCheck={false}
            />
            <button
              type="button"
              className="button"
              onClick={async () => {
                try {
                  const generated = await generatePassword();
                  setPassword(generated.value);
                } catch (caught) {
                  setError(toIpcError(caught).message);
                }
              }}
            >
              Générer
            </button>
            <button
              type="button"
              className="button button--primary"
              disabled={password.length === 0 || busy !== null}
              onClick={async () => {
                await write("password", password);
                setPassword("");
              }}
            >
              Enregistrer
            </button>
            <button
              type="button"
              className="button button--danger"
              disabled={!item.hasPassword || busy !== null}
              onClick={() => write("password", null)}
            >
              Effacer
            </button>
          </div>
          {strength ? (
            <PasswordStrength score={strength.score} hint={`${strength.entropyBits} bits`} />
          ) : null}
        </section>

        <section className="secret-editor">
          <h3>Secret TOTP</h3>
          <p className="field__hint">
            {item.hasTotp ? "Une graine TOTP est enregistrée." : "Aucune graine TOTP enregistrée."}
          </p>
          <div className="field__inline">
            <input
              type="password"
              value={totp}
              onChange={(event) => setTotp(event.target.value)}
              placeholder="Base32 (ex. JBSWY3DPEHPK3PXP)"
              aria-label="Nouvelle graine TOTP"
              spellCheck={false}
              autoComplete="off"
            />
            <button
              type="button"
              className="button button--primary"
              disabled={totp.length === 0 || busy !== null}
              onClick={async () => {
                await write("totp", totp.trim().toUpperCase());
                setTotp("");
              }}
            >
              Enregistrer
            </button>
            <button
              type="button"
              className="button button--danger"
              disabled={!item.hasTotp || busy !== null}
              onClick={() => write("totp", null)}
            >
              Effacer
            </button>
          </div>
        </section>

        <section className="secret-editor">
          <h3>Notes sécurisées</h3>
          <div className="field">
            <textarea
              rows={4}
              value={notes}
              onChange={(event) => setNotes(event.target.value)}
              aria-label="Nouvelles notes sécurisées"
            />
          </div>
          <div className="field__inline">
            <button
              type="button"
              className="button button--primary"
              disabled={notes.length === 0 || busy !== null}
              onClick={async () => {
                await write("notes", notes);
                setNotes("");
              }}
            >
              Enregistrer les notes
            </button>
            <button
              type="button"
              className="button button--danger"
              disabled={!item.hasNotes || busy !== null}
              onClick={() => write("notes", null)}
            >
              Effacer
            </button>
          </div>
        </section>

        {error ? (
          <p className="form__error" role="alert">
            {error}
          </p>
        ) : null}

        <footer className="form__actions">
          <button type="button" className="button" onClick={onClose}>
            Fermer
          </button>
        </footer>
      </div>
    </Modal>
  );
}
