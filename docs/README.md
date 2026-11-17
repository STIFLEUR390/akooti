# Documentation Akooti

Ce dossier est la référence écrite d'Akooti : ce que fait le produit, comment il
est construit, quelles règles sont non négociables et comment il est vérifié.
Le [README](../README.md) racine présente le projet ; ce dossier entre dans le
détail.

## Sommaire

| Document                                 | Contenu                                                                   | À lire quand…                                       |
| ---------------------------------------- | ------------------------------------------------------------------------- | --------------------------------------------------- |
| [architecture.md](./architecture.md)     | Processus, frontières de confiance, flux de données, carte du code        | on touche à un composant ou à un contrat            |
| [fonctionalites.md](./fonctionalites.md) | Périmètre fonctionnel, fonction par fonction, avec son statut             | on se demande « est-ce que ça existe déjà ? »        |
| [regles.md](./regles.md)                 | Règles inviolables : sécurité, architecture, données, UX, périmètre       | avant d'écrire la première ligne d'un changement    |
| [schema.md](./schema.md)                 | Schéma SQLCipher local (source de vérité) et conception Supabase E2EE     | on modifie une table, une migration ou la sync      |
| [testing.md](./testing.md)               | Stratégie de test, couverture des contrats critiques, règles de fixtures  | on ajoute un test ou on cherche où couvrir quoi     |
| [desktop-tests.md](./desktop-tests.md)   | Plan de recette manuelle piloté par le pont MCP Tauri                     | on valide une phase dans le binaire réel            |
| [adr/](./adr/README.md)                  | Décisions d'architecture datées, avec contexte et conséquences            | une décision structurante est prise ou contestée    |

## Parcours de lecture

- **Découvrir le projet** : [README](../README.md) → `fonctionalites.md` →
  `architecture.md`.
- **Contribuer** : [`AGENTS.md`](../AGENTS.md) → `regles.md` → `testing.md` →
  `schema.md` si la donnée change.
- **Valider une phase** : `desktop-tests.md` puis `testing.md` pour la partie
  automatisée.
- **Relire une décision** : `adr/README.md` donne l'index.

## Conventions d'écriture

1. **Le code fait foi.** En cas de divergence entre un document et `crates/`,
   `src-tauri/` ou `apps/`, c'est le code qui a raison : la correction du
   document fait partie de la même PR que le changement de code (voir
   [`AGENTS.md`](../AGENTS.md) et la règle ARCH-5 de [regles.md](./regles.md)).
2. **Chaque affirmation porte un statut.** Les documents distinguent
   explicitement :

   - ✅ **implémenté** — présent dans `main`, couvert par des tests ;
   - 🧭 **prévu** — décidé et documenté, pas encore écrit (la phase est donnée) ;
   - ⚠️ **limite connue** — comportement volontairement partiel, avec la
     référence de l'ADR ou de la tâche.

   Un paragraphe non marqué décrit l'état actuel.
3. **Aucun secret, aucun exemple réel.** Les identifiants cités sont des
   espaces réservés (`ak_mcp_…`, `example.test`). Les fixtures de test et les
   coffres d'exemple ne contiennent jamais de valeur réelle.
4. **Langue.** La documentation est en français ; le code, les commentaires, les
   messages de commit et les noms d'API sont en anglais.
5. **Format.** Markdown, titres hiérarchisés à partir de `##`, tableaux pour les
   inventaires, et blocs de code annotés (`bash`, `sql`, `text`, `json`) pour
   tout ce qui est exécutable ou lisible par un outil.

## Où en est le projet

Le suivi exécutable vit dans le projet Kaneo **AKO — Akooti** (voir
[`AGENTS.md`](../AGENTS.md) pour les identifiants). Les phases, telles qu'elles
sont suivies :

| Phase | Périmètre                                                     | Statut |
| ----- | ------------------------------------------------------------- | ------ |
| 0     | Fondations : monorepo Cargo + Bun, CI, conventions, bridge MCP | ✅     |
| 1     | Cryptographie et stockage chiffré (`akooti-core`)              | ✅     |
| 2     | Fonctions du coffre : générateur, TOTP, routage                | ✅     |
| 3     | Importateurs universels et hygiène post-import                 | ✅     |
| 4     | Application desktop Tauri v2 (interface du coffre)             | ✅     |
| 5     | Pont IA : CLI, daemon IPC, serveur MCP Zero-Leak               | ✅     |
| 6     | Extension multi-navigateurs WXT et passkeys                    | 🧭     |
| 7+    | Synchronisation Supabase E2EE, sauvegardes planifiées, santé    | 🧭     |

Les fonctionnalités de synchronisation, de sauvegarde planifiée et de santé du
coffre sont documentées comme cible ([fonctionalites.md](./fonctionalites.md))
et ne sont pas encore implémentées.
