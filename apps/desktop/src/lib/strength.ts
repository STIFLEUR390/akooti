/**
 * Offline password strength estimate.
 *
 * `zxcvbn` is a Rust-side dependency of `akooti-core`; the UI must not ship a
 * second copy. This heuristic is deliberately conservative and only used to
 * drive the inline meter, never to make a security decision.
 */

/** Result of {@link estimatePassword}. */
export interface StrengthEstimate {
  /** 0-4, same scale as zxcvbn. */
  score: number;
  /** Estimated entropy in bits. */
  entropyBits: number;
  /** Short, actionable remarks (French). */
  warnings: string[];
}

const COMMON = [
  "password",
  "motdepasse",
  "azerty",
  "qwerty",
  "123456",
  "admin",
  "letmein",
  "welcome",
  "akooti",
  "soleil",
  "bonjour",
];

const SEQUENCES = ["abcdefghijklmnopqrstuvwxyz", "0123456789", "azertyuiop", "qwertyuiop"];

/** Character-class based alphabet size. */
function alphabetSize(password: string): number {
  let size = 0;
  if (/[a-z]/.test(password)) size += 26;
  if (/[A-Z]/.test(password)) size += 26;
  if (/\d/.test(password)) size += 10;
  if (/[^A-Za-z0-9]/.test(password)) size += 32;
  return size;
}

function hasRun(password: string): boolean {
  const lowered = password.toLowerCase();
  return SEQUENCES.some((sequence) =>
    Array.from({ length: Math.max(sequence.length - 3, 0) }, (_, index) =>
      sequence.slice(index, index + 4),
    ).some((slice) => slice.length === 4 && lowered.includes(slice)),
  );
}

function hasTripleRepeat(password: string): boolean {
  return /(.)\1\1/.test(password);
}

/**
 * Estimates strength from length, character classes and three cheap penalties.
 *
 * @example
 * estimatePassword("correct horse battery staple").score; // 4
 */
export function estimatePassword(password: string): StrengthEstimate {
  const warnings: string[] = [];
  if (password.length === 0) {
    return { score: 0, entropyBits: 0, warnings: [] };
  }

  const size = alphabetSize(password);
  let bits = size > 1 ? password.length * Math.log2(size) : 0;

  const lowered = password.toLowerCase();
  if (COMMON.some((word) => lowered.includes(word))) {
    bits -= 24;
    warnings.push("Motif courant détecté");
  }
  if (hasRun(password)) {
    bits -= 12;
    warnings.push("Suite de caractères prévisible");
  }
  if (hasTripleRepeat(password)) {
    bits -= 10;
    warnings.push("Caractère répété trois fois");
  }
  if (!/[^A-Za-z0-9]/.test(password)) {
    warnings.push("Ajoutez un symbole");
  }
  if (password.length < 12) {
    warnings.push("12 caractères minimum");
  }

  const entropyBits = Math.max(Math.round(bits * 10) / 10, 0);
  let score: number;
  if (entropyBits < 28) {
    score = 0;
  } else if (entropyBits < 45) {
    score = 1;
  } else if (entropyBits < 70) {
    score = 2;
  } else if (entropyBits < 100) {
    score = 3;
  } else {
    score = 4;
  }

  return { score, entropyBits, warnings };
}
