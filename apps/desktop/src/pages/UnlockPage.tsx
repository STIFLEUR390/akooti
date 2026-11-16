import { useMemo, useState } from "react";

import type { BiometricsStatus } from "../lib/ipc";
import { estimatePassword } from "../lib/strength";
import { Icon } from "../components/Icons";
import { PasswordStrength } from "../components/PasswordStrength";

interface UnlockPageProps {
  hasVault: boolean;
  busy: boolean;
  error: string | null;
  biometrics: BiometricsStatus | null;
  onUnlock: (password: string) => Promise<boolean>;
  onCreate: (password: string) => Promise<boolean>;
}

const MIN_LENGTH = 12;

/**
 * First-run and unlock screen.
 *
 * Failures are deliberately uniform: a wrong password always yields the same
 * message, so the screen cannot be used as an oracle. The biometric control is
 * only rendered when the backend reports a usable provider.
 */
export function UnlockPage({
  hasVault,
  busy,
  error,
  biometrics,
  onUnlock,
  onCreate,
}: UnlockPageProps) {
  const [mode, setMode] = useState<"unlock" | "create">(hasVault ? "unlock" : "create");
  const [password, setPassword] = useState("");
  const [confirmation, setConfirmation] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [localError, setLocalError] = useState<string | null>(null);

  const estimate = useMemo(
    () => (mode === "create" ? estimatePassword(password) : null),
    [mode, password],
  );

  const mismatch = mode === "create" && confirmation.length > 0 && confirmation !== password;
  const tooShort = mode === "create" && password.length > 0 && password.length < MIN_LENGTH;
  const canSubmit =
    !busy && password.length > 0 && (mode === "unlock" || (confirmation === password && !tooShort));

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!canSubmit) {
      return;
    }
    setLocalError(null);
    const done = mode === "unlock" ? await onUnlock(password) : await onCreate(password);
    if (!done) {
      setPassword("");
      setConfirmation("");
    }
  }

  return (
    <main className="unlock">
      <div className="unlock__panel">
        <header className="unlock__brand">
          <span className="unlock__logo" aria-hidden="true">
            <Icon name="shield" size={22} />
          </span>
          <div>
            <h1>Akooti</h1>
            <p>Coffre-fort local-first, zéro connaissance, zéro fuite</p>
          </div>
        </header>

        {hasVault ? (
          <div className="segmented" role="tablist" aria-label="Mode">
            <button
              type="button"
              role="tab"
              aria-selected={mode === "unlock"}
              className={
                mode === "unlock" ? "segmented__item segmented__item--active" : "segmented__item"
              }
              onClick={() => setMode("unlock")}
            >
              Déverrouiller
            </button>
            <button
              type="button"
              role="tab"
              aria-selected={mode === "create"}
              className={
                mode === "create" ? "segmented__item segmented__item--active" : "segmented__item"
              }
              onClick={() => setMode("create")}
            >
              Nouveau coffre
            </button>
          </div>
        ) : null}

        <form className="form" onSubmit={submit}>
          <label className="field">
            <span className="field__label">Mot de passe maître</span>
            <div className="field__inline">
              <input
                type={showPassword ? "text" : "password"}
                value={password}
                onChange={(event) => setPassword(event.target.value)}
                autoFocus
                autoComplete={mode === "unlock" ? "current-password" : "new-password"}
                aria-describedby="master-password-hint"
                spellCheck={false}
              />
              <button
                type="button"
                className="icon-button"
                onClick={() => setShowPassword((current) => !current)}
                aria-label={showPassword ? "Masquer le mot de passe" : "Afficher le mot de passe"}
                aria-pressed={showPassword}
              >
                <Icon name={showPassword ? "eyeOff" : "eye"} size={15} />
              </button>
            </div>
          </label>

          {mode === "create" ? (
            <>
              <label className="field">
                <span className="field__label">Confirmation</span>
                <input
                  type={showPassword ? "text" : "password"}
                  value={confirmation}
                  onChange={(event) => setConfirmation(event.target.value)}
                  autoComplete="new-password"
                  aria-invalid={mismatch}
                  spellCheck={false}
                />
              </label>
              {estimate ? (
                <PasswordStrength score={estimate.score} hint={`${estimate.entropyBits} bits`} />
              ) : null}
              {mismatch ? (
                <p className="form__error" role="alert">
                  Les deux mots de passe ne correspondent pas.
                </p>
              ) : null}
              {tooShort ? (
                <p className="form__error" role="alert">
                  {MIN_LENGTH} caractères minimum.
                </p>
              ) : null}
              <p className="unlock__warning" id="master-password-hint">
                <Icon name="warn" size={14} />
                Ce mot de passe est le seul moyen de déchiffrer le coffre : il n'existe aucune
                récupération. Notez-le hors de cet appareil.
              </p>
            </>
          ) : (
            <p className="field__hint" id="master-password-hint">
              {hasVault
                ? "Saisissez votre mot de passe maître pour déverrouiller le coffre."
                : "Aucun coffre sur cet appareil : créez-en un."}
            </p>
          )}

          {(error ?? localError) ? (
            <p className="form__error" role="alert">
              {error ?? localError}
            </p>
          ) : null}

          <button
            type="submit"
            className="button button--primary button--block"
            disabled={!canSubmit}
          >
            {busy ? "Déchiffrement…" : mode === "unlock" ? "Déverrouiller" : "Créer le coffre"}
          </button>

          {biometrics?.available ? (
            <button type="button" className="button button--block" disabled>
              <Icon name="user" size={15} />
              Déverrouiller avec{" "}
              {biometrics.provider === "windowsHello" ? "Windows Hello" : "la biométrie"}
            </button>
          ) : (
            <p className="unlock__biometrics" role="note">
              <Icon name="lock" size={13} /> {biometrics?.reason ?? "Mot de passe maître requis."}
            </p>
          )}
        </form>

        <footer className="unlock__footer">
          <p>15 minutes d'inactivité verrouillent automatiquement le coffre.</p>
        </footer>
      </div>
    </main>
  );
}
