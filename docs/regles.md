### 1. Règles de Sécurité & Cryptographie (Inviolables)

1. **Zero-Leak absolu envers les LLM :**
* Aucun outil MCP (`vault_search`, `vault_blind_autofill`, `vault_cdp_autofill`, `vault_create_and_fill`, `vault_exec`) ne doit **jamais** retourner un mot de passe, une clé API ou un secret TOTP en clair dans sa réponse JSON.
* Les secrets transitent uniquement de la RAM du binaire Rust vers la cible finale (extension WXT via *Native Messaging*, navigateur headless via CDP, ou environnement mémoire d'un processus enfant).


2. **Hygiène stricte de la mémoire vive (RAM) :**
* Toute variable manipulant la clé maître, une clé dérivée ou un mot de passe déchiffré en Rust doit être encapsulée dans `zeroize::Zeroizing<T>` afin d'être effacée à l'octet près dès sa sortie de portée.
* Verrouillage automatique du coffre (effacement de la clé en RAM) après inactivité (ex: 15 min) ou mise en veille de l'OS.


3. **Zero-Knowledge côté Cloud (Supabase, S3, Google Drive) :**
* Tout chiffrement (`XChaCha20-Poly1305`) et déchiffrement s'effectue **exclusivement côté client** en local. Aucune clé de chiffrement ne quitte l'appareil.


4. **Isolation du canal IPC local :**
* Le socket local (`Unix Domain Socket` sous Linux/macOS, `Named Pipe` sous Windows via `interprocess`) reliant le daemon Tauri principal aux sous-processus `akooti mcp` et `akooti native-host` doit être restreint au seul UID de l'utilisateur courant (`chmod 600` / ACL Windows). Interdiction d'ouvrir un serveur HTTP local non authentifié.


5. **Nettoyage post-importation :**
* Lors d'une importation depuis un fichier en clair (`.json` Bitwarden/Proton, `.csv` navigateur), toujours proposer à l'utilisateur l'effacement sécurisé du fichier source une fois la transaction SQLCipher validée.



---

### 2. Règles d'Architecture, de Données & d'IA (MCP)

1. **Séparation stricte UI / Cœur Métier :**
* Le crate `akooti-core` ne doit importer **aucune** dépendance liée à Tauri, WebView ou React. Il reste autonome et testable unitairement (prêt pour un futur portage mobile iOS/Android).


2. **Intégrité des Dossiers Système & Multi-Appartenance (`item_folders`) :**
* Les 3 dossiers système (`Personnel`, `Dev Local`, `Agents & MCP`) sont créés automatiquement à l'initialisation du coffre et protégés (`is_system = 1` : non supprimables, non renommables).
* L'appartenance d'une entrée à plusieurs dossiers passe obligatoirement par la table de liaison **Many-to-Many (`item_folders`)** : ne jamais dupliquer une entrée pour la partager entre deux dossiers.


3. **Routage Automatique `Dev Local` non destructif :**
* Lors de l'importation (case cochée par défaut) ou d'un tri rétroactif, les entrées correspondant aux règles locales (`.local`, `.test`, `.localhost`, `localhost`, `127.0.0.1` et suffixes personnalisés) sont **ajoutées** au dossier `Dev Local` tout en conservant leur rattachement à leur dossier d'origine.


4. **Cloisonnement strict des jetons MCP (Principe du moindre privilège) :**
* Chaque clé API MCP (`akooti mcp --token ...`) est restreinte à une liste explicite de `folder_id` autorisés et à des permissions granulaires (`read_metadata`, `blind_fill`, `create_and_fill`, `exec_env`), avec TTL et journalisation systématique dans `mcp_audit_logs`.
* Les jetons MCP, logs d'audit et configurations de backup restent **strictement locaux** (jamais synchronisés sur Supabase).
* Par défaut, les comptes créés par un agent via `vault_create_and_fill` sont enregistrés dans `Agents & MCP` (avec support d'un TTL d'expiration pour purge automatique des comptes de test).


5. **Synchronisation sans conflit :**
* Ne jamais synchroniser le fichier SQLCipher entier vers Supabase. Synchroniser chaque enregistrement chiffré individuellement via horodatage logique (*Last-Write-Wins*) et suppression logique (`deleted_at` / *tombstone*).



---

### 3. Règles d'Interface & Design (UX/UI)

1. **Ergonomie Desktop Native & Thème Sombre par défaut :**
* Interface réactive pilotable au clavier (raccourcis pour la recherche rapide, copie de l'identifiant, copie du code 2FA TOTP et ouverture du générateur).
* Thème sombre soigné par défaut (avec option thème clair), barre latérale fixe distinguant visuellement les **dossiers système** (`Personnel`, `Dev Local`, `Agents & MCP`) des dossiers utilisateur.


2. **Sécurité visuelle & Presse-papiers :**
* Mots de passe masqués par défaut et effacement automatique du presse-papiers système **30 secondes** après la copie d'un secret ou d'un code TOTP.
* Badge visuel explicite sur tout dossier partagé avec un jeton MCP actif, et possibilité d'exiger une confirmation biométrique (Windows Hello / Touch ID / Polkit) avant toute injection IA sur un dossier critique.


3. **Gestion fluide des sélecteurs DOM :**
* Chaque fiche d'identifiant permet d'afficher, éditer ou capturer (via l'extension WXT) les sélecteurs CSS/JS (`username_selector`, `password_selector`, `totp_selector`, `submit_selector`) utilisés par le MCP.



---

### 4. Limites & Périmètre du Projet (Garde-fous Anti-Dispersion)

1. **Pas de serveur backend propriétaire :** Akooti n'héberge aucune infrastructure cloud ; l'utilisateur utilise son disque local, son propre projet Supabase, son bucket S3 ou son Google Drive.
2. **Pas de modèles d'IA embarqués :** Akooti reste un binaire léger (< 20 Mo) et déterministe. L'intelligence décisionnelle reste du côté des agents externes (Codex, Claude Code, Cursor, Antigravity) guidés par le fichier `SKILL.md`.
3. **Pas de stockage de fichiers lourds :** Uniquement des secrets textuels, TOTP, sélecteurs et Passkeys (pas de pièces jointes volumineuses qui alourdiraient SQLCipher et Supabase).
4. **Réversibilité garantie :** Export d'urgence toujours disponible (JSON chiffré ou en clair après confirmation du mot de passe maître).

---

### 5. Règles de Phasage (Ordre d'Implémentation)

1. **Phase 1 — Noyau & Coffre Local :** `akooti-core` (Argon2id, XChaCha20, SQLCipher, multi-dossiers, dossiers système, générateur, TOTP) + UI Desktop Tauri minimale + importateurs (Buttercup `.bcup`/CSV, Bitwarden, Proton Pass) avec routage auto `Dev Local`.
2. **Phase 2 — Pont IA, CLI & MCP :** `tauri-plugin-cli` + serveur IPC local (`interprocess`) + serveur MCP (`vault_search`, `vault_cdp_autofill`, `vault_create_and_fill`, `vault_exec`) + jetons cloisonnés par dossier + fichier `SKILL.md`.
3. **Phase 3 — Extension Multi-Navigateurs (WXT) & Audit :** Pont *Native Messaging* + capture/injection aveugle par sélecteurs CSS/JS + tableau de bord Santé du Coffre (`zxcvbn` offline + HIBP *k-Anonymity*).
4. **Phase 4 — Synchronisation, Backups & Passkeys :** Synchro temps réel Supabase E2EE + planificateur Cron 1h (Local, S3, Google Drive) + support complet des Passkeys WebAuthn (`MAIN` world).
