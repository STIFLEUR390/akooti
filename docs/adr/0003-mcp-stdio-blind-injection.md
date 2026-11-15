# 0003 — MCP stdio, daemon IPC et injection aveugle

- **Statut :** acceptée
- **Date :** 2026-11-15

## Contexte

Les agents de codage doivent pouvoir chercher un identifiant et remplir un
formulaire, mais un secret transmis au modèle est un secret compromis (contexte
LLM, logs, télémétrie, historique). Il faut un canal qui donne l'action sans
donner la valeur.

## Décision

- **Serveur MCP en stdio** (`rmcp`), lancé par le client agent :
  `akooti mcp --token ak_mcp_…`.
- Les sous-processus n'ont pas la clé maître : ils parlent au daemon via un
  **socket local** (`interprocess`, permissions `600` / ACL Windows). Aucun
  serveur HTTP local.
- **Injection aveugle** : l'agent fournit des sélecteurs CSS/JS ; le daemon
  remplit le DOM via l'extension (Native Messaging) ou via **CDP** pour un
  navigateur piloté par IA.
- **Jetons cloisonnés** : liste blanche de dossiers, scopes granulaires
  (`read_metadata`, `blind_fill`, `create_and_fill`, `exec_env`), TTL, audit.
- Les secrets peuvent aussi être injectés dans l'environnement d'un processus
  enfant (`vault_exec`) avec masquage des logs.

## Alternatives écartées

- Serveur HTTP local : surface d'attaque et authentification fragile.
- Retour du secret au client MCP : viole Zero-Leak par construction.
- Stockage des jetons côté cloud : un jeton local ne doit jamais être répliqué.

## Conséquences

- Chaque outil MCP doit avoir un test prouvant qu'aucun secret ne figure dans
  sa réponse, y compris en cas d'erreur.
- Les erreurs d'injection ne doivent jamais inclure la valeur du secret.
- Le socket doit être créé avec des permissions restrictives avant d'accepter
  la première connexion.
- Les clients doivent être configurés avec le chemin du binaire et un jeton
  révoquable.
