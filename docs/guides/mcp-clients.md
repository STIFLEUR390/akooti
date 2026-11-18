# Configurer un client MCP (Claude Code, Codex, Cursor, OpenCode)

Le serveur MCP est un **client du coffre** : il ne détient ni clé ni secret. Il
relaie chaque appel vers le socket local du daemon, qui vérifie le jeton, les
portées et les dossiers.

## 1. Créer un jeton

Dans le coffre déverrouillé → **Accès IA** :

1. nom (visible dans l'audit) ;
2. durée de vie (0 = illimité ; préférer 30 jours) ;
3. portées : `read_metadata`, `blind_fill`, `create_and_fill`, `exec_env` ;
4. dossiers autorisés (le cloisonnement est strict) ;
5. **Créer le jeton** → copier le secret **une seule fois** (`ak_mcp_…`, 64
   caractères hexadécimaux).

Le daemon doit tourner (application ouverte et déverrouillée) : sinon chaque
appel répond `daemon_unavailable`. Un jeton expiré ou révoqué répond
`invalid_credentials`.

## 2. Claude Code

`.mcp.json` à la racine du projet (ou configuration globale) :

```json
{
  "mcpServers": {
    "akooti": {
      "command": "akooti",
      "args": ["mcp", "--token", "ak_mcp_…"]
    }
  }
}
```

## 3. Codex (OpenAI)

`~/.codex/config.toml` :

```toml
[mcp_servers.akooti]
command = "akooti"
args = ["mcp", "--token", "ak_mcp_…"]
startup_timeout_sec = 20
```

## 4. Cursor

`.cursor/mcp.json` :

```json
{
  "mcpServers": {
    "akooti": {
      "command": "akooti",
      "args": ["mcp", "--token", "ak_mcp_…"]
    }
  }
}
```

## 5. OpenCode

`opencode.json` (projet) ou configuration globale :

```json
{
  "mcp": {
    "akooti": {
      "type": "local",
      "command": ["akooti", "mcp", "--token", "ak_mcp_…"],
      "enabled": true
    }
  }
}
```

## 6. Les six outils

| Outil | Portée | Retour |
| ----- | ------ | ------ |
| `vault_status` | — | nom, portées, dossiers, expiration du jeton |
| `vault_search` | `read_metadata` | identifiants et métadonnées, jamais un secret |
| `vault_cdp_autofill` | `blind_fill` | champs remplis dans une page pilotée par CDP |
| `vault_blind_autofill` | `blind_fill` | réservé au pont de l'extension (`unavailable` côté agent) |
| `vault_create_and_fill` | `create_and_fill` | compte éphémère + remplissage du formulaire |
| `vault_exec` | `exec_env` | sortie de commande masquée (`***`) |

Ajoutez `skills/akooti/SKILL.md` au contexte de votre agent : il décrit les
règles d'usage (ne jamais demander un secret, vérifier l'origine, etc.).

## 7. Dépannage

| Code | Sens | Action |
| ---- | ---- | ------ |
| `daemon_unavailable` | application fermée ou daemon arrêté | lancer/déverrouiller Akooti |
| `locked` | coffre verrouillé | déverrouiller (l'auto-lock est de 15 min) |
| `invalid_credentials` | jeton inconnu, expiré ou révoqué | réémettre dans **Accès IA** |
| `forbidden` | portée ou dossier hors du jeton | élargir le jeton, jamais la demande |
| `not_found` | entrée hors des dossiers autorisés ou supprimée | vérifier la recherche |
| `invalid_input` | origine absente, sélecteurs invalides, TTL hors bornes | corriger l'appel |

Vérifier le dernier appel dans **Accès IA → Journal d'audit** : une ligne par
action, jamais une valeur.
