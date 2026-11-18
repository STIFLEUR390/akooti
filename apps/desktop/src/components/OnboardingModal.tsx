import type { Screen } from "../hooks/useVaultData";
import { Icon } from "./Icons";
import { Modal } from "./Modal";

interface OnboardingModalProps {
  onClose: () => void;
  onCreateItem: () => void;
  onOpenScreen: (screen: Screen) => void;
}

interface Step {
  title: string;
  body: string;
  action: string;
  target: "create" | Screen;
}

const STEPS: ReadonlyArray<Step> = [
  {
    title: "Créez votre première entrée",
    body: "Identifiant, mot de passe, TOTP ou note : tout est chiffré champ par champ.",
    action: "Nouvelle entrée",
    target: "create",
  },
  {
    title: "Importez vos coffres existants",
    body: "Bitwarden, 1Password, KeePass, Proton Pass, Buttercup et exports de navigateurs — sans passer par le réseau.",
    action: "Importer",
    target: "import",
  },
  {
    title: "Branchez un agent IA",
    body: "Créez un jeton restreint à des dossiers et à des actions ; l'agent ne voit jamais un secret.",
    action: "Accès IA",
    target: "mcp",
  },
  {
    title: "Sauvegardez et synchronisez",
    body: "Instantanés .akooti vers un dossier, un S3 ou Drive ; sync chiffrée via votre projet Supabase.",
    action: "Sync & sauvegardes",
    target: "sync",
  },
  {
    title: "Auditez la santé du coffre",
    body: "Score zxcvbn, mots de passe réutilisés ou anciens, fuites connues (HIBP) — calculé localement.",
    action: "Santé",
    target: "health",
  },
];

/**
 * First-run guide.
 *
 * Shown once after the first unlock (a localStorage flag), and reopenable from
 * the sidebar. Every step jumps to the real screen instead of duplicating a
 * tour: the screen is the documentation.
 */
export function OnboardingModal({ onClose, onCreateItem, onOpenScreen }: OnboardingModalProps) {
  return (
    <Modal
      title="Bienvenue dans Akooti"
      description="Cinq étapes pour prendre le coffre en main. Ce guide reste accessible depuis la barre latérale."
      onClose={onClose}
      wide
      footer={
        <button type="button" className="button button--primary" onClick={onClose}>
          <Icon name="check" size={14} />
          C'est parti
        </button>
      }
    >
      <ol className="onboarding">
        {STEPS.map((step, index) => (
          <li key={step.title} className="onboarding__step">
            <span className="onboarding__index" aria-hidden="true">
              {index + 1}
            </span>
            <div>
              <strong>{step.title}</strong>
              <p>{step.body}</p>
            </div>
            <button
              type="button"
              className="button"
              onClick={() => {
                if (step.target === "create") {
                  onCreateItem();
                } else {
                  onOpenScreen(step.target);
                }
              }}
            >
              {step.action}
            </button>
          </li>
        ))}
      </ol>
      <p className="field__label">
        Pensez aussi à l'export d'urgence (Sync & sauvegardes) : un coffre n'a aucune récupération
        sans son mot de passe maître.
      </p>
    </Modal>
  );
}
