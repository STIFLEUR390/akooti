import { strengthLabel } from "../lib/format";

interface PasswordStrengthProps {
  /** 0-4; `null` when nothing has been typed yet. */
  score: number | null;
  hint?: string;
  /** Accessible name of the meter. */
  label?: string;
}

/** Five-segment strength meter with a text label, never colour-only. */
export function PasswordStrength({
  score,
  hint,
  label = "Force du mot de passe",
}: PasswordStrengthProps) {
  const value = score ?? 0;
  return (
    <div className="strength">
      <div
        className="strength__track"
        role="meter"
        aria-valuemin={0}
        aria-valuemax={4}
        aria-valuenow={value}
        aria-valuetext={strengthLabel(score)}
        aria-label={label}
      >
        {[0, 1, 2, 3, 4].map((index) => (
          <span
            key={index}
            className={index < value ? `strength__cell strength__cell--${value}` : "strength__cell"}
          />
        ))}
      </div>
      <span className="strength__label">
        {strengthLabel(score)}
        {hint ? <span className="strength__hint"> · {hint}</span> : null}
      </span>
    </div>
  );
}
