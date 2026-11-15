# Rapport de sécurité

**Ne créez pas d'issue publique pour une vulnérabilité.**

Merci d'utiliser le canal privé des **GitHub Security Advisories** :

<https://github.com/STIFLEUR390/akooti/security/advisories/new>

## À inclure

- Version / commit concerné et plateforme.
- Scénario d'attaque et impact (fuite de secret, contournement de scope MCP,
  accès au socket IPC, exécution de code, …).
- Étapes de reproduction minimales.
- Preuve de concept **expurgée** (aucune donnée réelle de coffre).

## Périmètre prioritaire

| Surface       | Exemples                                                             |
| ------------- | -------------------------------------------------------------------- |
| MCP           | réponse contenant un secret, scope non appliqué, audit manquant      |
| Daemon IPC    | accès au socket par un autre utilisateur, absence d'authentification |
| Cryptographie | nonce réutilisé, mauvaise dérivation, secret non `zeroized`          |
| Extension     | injection cassant la CSP, fuite vers la page, passkey falsifiée      |
| Sync / backup | lecture de ciphertext, clé envoyée au serveur, configuration fuîtée  |

## Engagements

- Accusé de réception sous 72 h ouvrées.
- Correctif ou plan de mitigation communiqué avant publication.
- Crédit dans l'advisory si vous le souhaitez.
