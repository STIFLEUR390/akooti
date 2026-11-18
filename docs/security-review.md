# Revue de sécurité — phase 9.1

Date : 2026-11-18 · Version : 1.0.0 · Portée : code livré, pas l'infrastructure
de l'utilisateur.

Méthode : relecture des surfaces (IPC, Native Messaging, CDP, MCP, sync,
sauvegardes), exécution des suites de non-fuite, test d'intrusion interne sur
les jetons MCP, revue de la configuration Tauri et des permissions.

## 1. Constats

| # | Sévérité | Constat | État |
| - | -------- | ------- | ---- |
| F1 | moyenne | `app.security.csp` valait `null` : aucune politique de contenu, donc une injection dans la webview pouvait charger du script distant. | **corrigé** — CSP stricte (`default-src 'self'`, `script-src 'self'`) et `devCsp` séparée pour Vite ; vérifié dans l'app réelle (aucune erreur console, IPC fonctionnel). |
| F2 | faible | La capability accordait `clipboard-manager:allow-read-text`, jamais utilisé par l'interface. | **corrigé** — permission retirée (moindre privilège). |
| F3 | faible | `withGlobalTauri: true` expose `window.__TAURI__` (dont l'IPC) à tout script de la page. | **accepté** — la page est locale et la CSP bloque le script distant ; l'alternative (wrapper + permissions par commande) coûterait l'accès au bridge pour `ipc.ts`. Documenté. |
| F4 | faible | Les overrides de test Google Drive (`AKOOTI_GDRIVE_API_BASE`, `AKOOTI_GDRIVE_STATIC_TOKEN`) contournent OAuth. | **corrigé par construction** — compilés uniquement en `debug_assertions` ; une release ignore les variables. |
| F5 | faible | Le bridge MCP Tauri écoute sur un port local. | **corrigé** — plugin enregistré sous `#[cfg(debug_assertions)]`, bind `127.0.0.1`, capability dans le build de dev uniquement (le plugin n'existe pas en release). |
| F6 | faible | Un `.akooti` est aussi sensible que le coffre (même chiffrement). | **accepté** — documenté : protéger la cible (permissions, bucket privé) et tester la restauration. |
| F7 | faible | Le verrouillage automatique repose sur l'horloge murale et l'inactivité, pas sur le verrouillage de session OS. | **accepté** — mise en veille et `ExitRequested` verrouillent ; un verrouillage OS explicite relève d'une intégration par plateforme (suivi post-release). |
| F8 | info | Le serveur de sync voit le nombre d'enregistrements, leur type et leurs horodatages. | **accepté** — inhérent au delta ; les titres, URLs et secrets restent chiffrés (test RLS réel). |
| F9 | info | HIBP apprend qu'un préfixe SHA-1 est vérifié. | **accepté** — 5 caractères, action explicite, SEC-11. |
| F10 | info | Le presse-papiers est partagé avec le reste du système pendant 30 s. | **accepté** — effacement si la valeur n'a pas changé, compte à rebours affiché (UX-4). |

Aucun constat critique ou élevé. Aucun correctif de sécurité n'est laissé
ouvert.

## 2. Vérifications exécutées

| Surface | Test | Résultat |
| ------- | ---- | -------- |
| Zero-Leak MCP | `cargo test -p akooti-core --test mcp_access_test`, tests in-file `akooti-mcp` | 21 tests, aucun secret dans une réponse |
| Scopes MCP | jetons sans `blind_fill`/`create_and_fill`/`exec_env`, dossiers hors portée, jeton expiré/révoqué | refus typés (`forbidden`, `locked`, `invalid_credentials`), audit `access_denied` |
| Passkeys | `cargo test -p akooti-core --test passkey_test` | signature ES256 vérifiée indépendamment, origine et `rpId` contrôlés, compteur croissant |
| Sync | `cargo test -p akooti-core --test sync_test` | charge utile opaque, AAD anti-déplacement, LWW, tombstones |
| RLS Supabase | recette réelle (2 utilisateurs) | l'utilisateur 2 voit 0 ligne, supprimé ensuite |
| HIBP | `cargo test -p akooti-core --test health_test` | préfixe de 5 caractères, comparaison locale |
| Sauvegardes | `cargo test -p akooti-core --test backup_test` | `.akooti` refusé s'il est altéré, export clair sous mot de passe |
| Masquage `vault_exec` | tests `src/daemon/exec.rs` | valeur brute, URL-encodée et JSON remplacées par `***` |
| Logs | revue des `tracing::` et des messages d'erreur | aucun secret interpolé ; erreurs statiques |
| Fichiers temporaires | `backup/mod.rs` | l'instantané temporaire est supprimé après l'envoi, succès ou échec |
| CSP | lancement réel + console | aucune violation bloquante, styles et IPC fonctionnels |

## 3. Suites de non-fuite (référence)

```bash
cargo test -p akooti-core --test mcp_access_test
cargo test -p akooti-core --test passkey_test
cargo test -p akooti-core --test sync_test
cargo test -p akooti-core --test backup_test
cargo test -p akooti-core --test health_test
cargo test -p akooti-mcp
```

## 4. Suivi post-release

- Verrouillage de session OS (GNOME/KDE/macOS/Windows) comme déclencheur
  supplémentaire du verrouillage.
- Recette Google Drive sur un compte réel (OAuth, quotas).
- Notarisation macOS et signature Windows dans le canal de release.
- Test multi-appareils de la sync (deux profils, conflits réels).

Voir les issues ouvertes sur le dépôt après la v1.0.0.
