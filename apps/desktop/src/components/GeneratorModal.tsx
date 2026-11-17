import { useCallback, useEffect, useState } from "react";

import type { GeneratedSecret, PassphraseOptions, PasswordOptions } from "../lib/ipc";
import { toIpcError } from "../lib/ipc";
import { estimatePassword } from "../lib/strength";
import { Icon } from "./Icons";
import { Modal } from "./Modal";
import { PasswordStrength } from "./PasswordStrength";

interface GeneratorModalProps {
  onClose: () => void;
  generatePassword: (options: PasswordOptions) => Promise<GeneratedSecret>;
  generatePassphrase: (options: PassphraseOptions) => Promise<GeneratedSecret>;
  onCopy: (value: string, label: string) => void;
  onUse?: (value: string) => void;
}

const DEFAULT_PASSWORD: PasswordOptions = {
  length: 24,
  lowercase: true,
  uppercase: true,
  digits: true,
  symbols: true,
  excludeAmbiguous: true,
};

const DEFAULT_PASSPHRASE: PassphraseOptions = {
  words: 6,
  separator: "-",
  capitalize: true,
};

/** Password and passphrase generator, with entropy feedback and one-click copy. */
export function GeneratorModal({
  onClose,
  generatePassword,
  generatePassphrase,
  onCopy,
  onUse,
}: GeneratorModalProps) {
  const [mode, setMode] = useState<"password" | "passphrase">("password");
  const [passwordOptions, setPasswordOptions] = useState(DEFAULT_PASSWORD);
  const [passphraseOptions, setPassphraseOptions] = useState(DEFAULT_PASSPHRASE);
  const [generated, setGenerated] = useState<GeneratedSecret | null>(null);
  const [error, setError] = useState<string | null>(null);

  const regenerate = useCallback(async () => {
    setError(null);
    try {
      const next =
        mode === "password"
          ? await generatePassword(passwordOptions)
          : await generatePassphrase(passphraseOptions);
      setGenerated(next);
    } catch (caught) {
      setError(toIpcError(caught).message);
      setGenerated(null);
    }
  }, [generatePassphrase, generatePassword, mode, passphraseOptions, passwordOptions]);

  useEffect(() => {
    void regenerate();
  }, [regenerate]);

  const estimate = generated ? estimatePassword(generated.value) : null;

  function togglePasswordOption(key: keyof PasswordOptions) {
    setPasswordOptions((current) => ({ ...current, [key]: !current[key] }));
  }

  return (
    <Modal
      title="Générateur"
      description="Les valeurs sont produites par le noyau Rust et ne sont jamais enregistrées automatiquement."
      onClose={onClose}
    >
      <div className="generator">
        <div className="segmented" role="tablist" aria-label="Type de secret">
          <button
            type="button"
            role="tab"
            aria-selected={mode === "password"}
            className={
              mode === "password" ? "segmented__item segmented__item--active" : "segmented__item"
            }
            onClick={() => setMode("password")}
          >
            Mot de passe
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={mode === "passphrase"}
            className={
              mode === "passphrase" ? "segmented__item segmented__item--active" : "segmented__item"
            }
            onClick={() => setMode("passphrase")}
          >
            Phrase de passe
          </button>
        </div>

        <output className="generator__preview" aria-live="polite">
          {generated ? generated.value : "…"}
        </output>

        {generated ? (
          <PasswordStrength
            score={estimate?.score ?? null}
            hint={`${generated.entropyBits} bits d'entropie`}
            label="Force du secret généré"
          />
        ) : null}

        {mode === "password" ? (
          <div className="form">
            <label className="field">
              <span className="field__label">
                Longueur <strong>{passwordOptions.length}</strong>
              </span>
              <input
                type="range"
                min={8}
                max={64}
                value={passwordOptions.length}
                onChange={(event) =>
                  setPasswordOptions((current) => ({
                    ...current,
                    length: Number(event.target.value),
                  }))
                }
              />
            </label>
            <div className="checkbox-grid">
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={passwordOptions.lowercase}
                  onChange={() => togglePasswordOption("lowercase")}
                />
                <span>Minuscules (a-z)</span>
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={passwordOptions.uppercase}
                  onChange={() => togglePasswordOption("uppercase")}
                />
                <span>Majuscules (A-Z)</span>
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={passwordOptions.digits}
                  onChange={() => togglePasswordOption("digits")}
                />
                <span>Chiffres (0-9)</span>
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={passwordOptions.symbols}
                  onChange={() => togglePasswordOption("symbols")}
                />
                <span>Symboles (!@#$…)</span>
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={passwordOptions.excludeAmbiguous}
                  onChange={() => togglePasswordOption("excludeAmbiguous")}
                />
                <span>Exclure les caractères ambigus (O/0, l/1)</span>
              </label>
            </div>
          </div>
        ) : (
          <div className="form">
            <label className="field">
              <span className="field__label">
                Nombre de mots <strong>{passphraseOptions.words}</strong>
              </span>
              <input
                type="range"
                min={3}
                max={12}
                value={passphraseOptions.words}
                onChange={(event) =>
                  setPassphraseOptions((current) => ({
                    ...current,
                    words: Number(event.target.value),
                  }))
                }
              />
            </label>
            <div className="form__row">
              <label className="field">
                <span className="field__label">Séparateur</span>
                <input
                  value={passphraseOptions.separator}
                  maxLength={3}
                  onChange={(event) =>
                    setPassphraseOptions((current) => ({
                      ...current,
                      separator: event.target.value,
                    }))
                  }
                />
              </label>
              <label className="checkbox">
                <input
                  type="checkbox"
                  checked={passphraseOptions.capitalize}
                  onChange={(event) =>
                    setPassphraseOptions((current) => ({
                      ...current,
                      capitalize: event.target.checked,
                    }))
                  }
                />
                <span>Capitaliser chaque mot</span>
              </label>
            </div>
          </div>
        )}

        {error ? (
          <p className="form__error" role="alert">
            {error}
          </p>
        ) : null}

        <footer className="form__actions">
          <button type="button" className="button" onClick={() => void regenerate()}>
            <Icon name="refresh" size={15} />
            Régénérer
          </button>
          {onUse && generated ? (
            <button type="button" className="button" onClick={() => onUse(generated.value)}>
              Utiliser
            </button>
          ) : null}
          <button
            type="button"
            className="button button--primary"
            disabled={!generated}
            onClick={() => generated && onCopy(generated.value, "Secret")}
          >
            <Icon name="copy" size={15} />
            Copier
          </button>
        </footer>
      </div>
    </Modal>
  );
}
