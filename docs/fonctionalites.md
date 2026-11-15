### Vue d'ensemble & Architecture

* **Nom du projet :** **Akooti** (exécutable unique `akooti`)
* **Philosophie :** *Local-First* (100 % autonome hors ligne), *Zero-Knowledge* (chiffrement de bout en bout) et *Zero-Leak* (aucun mot de passe exposé en clair aux modèles d'IA).
* **Stack technologique :**
* **Application Desktop & CLI :** **Tauri v2 (Rust + React/TypeScript)** avec `tauri-plugin-cli` (un binaire unique pour l'interface graphique, le daemon en arrière-plan, le pont navigateur *Native Messaging* et le serveur MCP).
* **Extension Navigateur :** **WXT (TypeScript/React)** compilant une base unique vers Chrome, Brave, Edge, Firefox et Safari (Manifest V3 & V2).
* **Stockage & Cryptographie :** Base locale **SQLCipher** + dérivation de clé **Argon2id** + chiffrement **XChaCha20-Poly1305** + effacement mémoire immédiat (**`zeroize`**).



---

### 1. Organisation du Coffre-Fort & Authentification Moderne

* **Appartenance Multi-Dossiers (*Many-to-Many*) :** Une même entrée peut être classée dans **plusieurs dossiers simultanément** depuis l'interface (sans duplication des données : toute modification est synchronisée partout).
* **Dossiers Système Intégrés (créés automatiquement) :**
* **`Personnel` :** Dossier racine par défaut pour les comptes quotidiens.
* **`Dev Local` :** Dossier dédié aux outils et services locaux (Docker, bases de données, interfaces d'administration locales).
* **`Agents & MCP` :** Dossier bac à sable où sont enregistrés par défaut les comptes créés à la volée par les agents de codage ou via CLI.


* **Gestion des Comptes de Test Éphémères (TTL) :** Possibilité d'attribuer une durée de vie limitée aux comptes de test créés par les agents IA pour purge automatique.
* **Historique des Mots de Passe :** Conservation chiffrée des anciennes versions de mots de passe sur chaque fiche.
* **Déverrouillage Biométrique Natif :** Support de Windows Hello, macOS Touch ID et Linux Polkit/FIDO2, avec verrouillage automatique après inactivité ou mise en veille.
* **Double Authentification (2FA / TOTP) :** Générateur de codes TOTP à 6 chiffres intégré (copie rapide, remplissage par l'extension et injection aveugle via MCP).
* **Gestion des Passkeys (WebAuthn / FIDO2) :** Interception de `navigator.credentials.create()` et `get()` dans le navigateur via l'extension WXT (`world: 'MAIN'`), signature cryptographique et stockage des clés privées dans Akooti.

---

### 2. Routage Intelligent & Importation Universelle

* **Importation Multi-Sources :**
* **Buttercup :** Lecture native des coffres chiffrés **`.bcup`** (AES-256-CBC + HMAC-SHA256) et des exports `.csv`.
* **Bitwarden / Vaultwarden :** Imports `.json` (chiffrés ou en clair) incluant dossiers, URI multiples, TOTP et champs personnalisés.
* **Proton Pass :** Imports `.json` et `.csv` (coffres, alias, TOTP, notes).
* **KeePass / KeePassXC :** Lecture directe des bases chiffrées **`.kdbx`** (v3/v4) ainsi que `.xml` et `.csv`.
* **1Password :** Lecture des archives **`.1pux`** et `.csv`.
* **Navigateurs :** Imports `.csv` standards (Chrome, Brave, Edge, Firefox).


* **Routage Automatique vers `Dev Local` (à l'import et rétroactivement) :**
* **Case à cocher activée par défaut lors de chaque importation :** Détecte automatiquement les URLs locales (`.local`, `.test`, `.localhost`, `localhost`, `127.0.0.1`) et les affecte au dossier **`Dev Local`** (tout en conservant leur dossier d'origine si souhaité).
* **Règles de sous-domaines personnalisables :** Ajout libre de nouveaux suffixes ou motifs locaux (ex: `*.docker`, `*.internal`, `*.dev.monprojet`) dans les paramètres.
* **Application rétroactive :** Action en un clic pour analyser les entrées déjà importées et les classer automatiquement selon les règles `Dev Local`.


* **Hygiène Post-Import :** Aperçu avant importation, détection des doublons et option d'effacement sécurisé du fichier source en clair.

---

### 3. Intégration IA Sécurisée (Serveur MCP, CLI & Garde-Fous Zero-Leak)

Akooti intègre un serveur **Model Context Protocol (MCP)** (`akooti mcp --token ...`) permettant aux agents de codage et navigateurs autonomes (Claude Code, OpenAI Codex, Cursor, Antigravity, OpenClaw) d'utiliser et de créer des identifiants **sans jamais lire un seul secret en clair**.

* **Clés d'Accès Cloisonnées par Dossier (*Scoped Tokens* / RBAC) :**
* Génération de jetons API limités à **un ou plusieurs dossiers précis** (ex: uniquement `Dev Local` et `Agents & MCP`).
* Permissions granulaires (`read_metadata`, `blind_fill`, `create_and_fill`, `exec_env`), expiration automatique (TTL), confirmation biométrique optionnelle pour les dossiers sensibles, journal d'audit complet et révocation en un clic.


* **Injection Aveugle via Extension (`vault_blind_autofill`) :**
* L'IA récupère uniquement les métadonnées (ID, titre, URL, identifiant) et fournit les **sélecteurs CSS/JavaScript** (`username_selector`, `password_selector`, `totp_selector`, `submit_selector`).
* Akooti pilote l'extension WXT via *Native Messaging* pour remplir les champs directement dans le DOM (compatible React/Vue/Angular) sans exposer le mot de passe au LLM.


* **Mémorisation des Sélecteurs DOM :** Possibilité d'enregistrer les sélecteurs CSS/JS directement sur chaque entrée (manuellement ou via l'extension) pour accélérer les connexions futures.
* **Injection Aveugle sans Extension (`vault_cdp_autofill`) :**
* Connexion directe au **Chrome DevTools Protocol (CDP)** pour les navigateurs pilotés par IA (`agent-browser`, Playwright, Puppeteer, Obscura) dépourvus d'extension.


* **Création & Sauvegarde de Comptes de Test (`vault_create_and_fill`) :**
* Lors des tests d'applications en local, l'IA demande la création d'un compte : Akooti génère un mot de passe fort en RAM, l'enregistre dans le dossier **`Agents & MCP`** (ou le dossier cible autorisé) et remplit le formulaire d'inscription en aveugle.


* **Exécution CLI & Variables d'Environnement Masquées (`vault_exec`) :**
* Injection des secrets dans la mémoire d'un processus enfant (ex: `docker compose up`, scripts de test, génération `.env` éphémère) avec masquage automatique des valeurs secrètes dans `stdout`/`stderr`.


* **Agent Skill Embarqué (`SKILL.md`) :**
* Fichier d'instructions standardisé prêt à l'emploi pour guider Codex, Claude Code, Cursor et Antigravity sur l'utilisation sécurisée du MCP et du CLI Akooti.



---

### 4. Synchronisation Temps Réel & Sauvegardes Hybrides

* **Base Locale Primaire :** Le coffre SQLCipher local est la source de vérité et fonctionne à 100 % hors ligne.
* **Synchronisation Temps Réel via Supabase (Optionnelle) :**
* Connexion à son propre projet Supabase (*Bring Your Own Supabase*) avec chiffrement **Zero-Knowledge** côté client et synchro granulaire par entrée via *Supabase Realtime* (WebSockets).


* **Sauvegardes Programmées Multi-Cibles (Cron interne) :**
* Export automatique périodique (ex: toutes les **1 heure**) d'instantanés chiffrés (`.akooti`) avec politique de rétention ($N$ dernières versions) vers :
* Dossier local ou lecteur réseau (NAS).
* **S3** (AWS S3, Cloudflare R2, MinIO, Wasabi).
* **Google Drive** (OAuth2).





---

### 5. Génération de Mots de Passe & Audit de Santé du Coffre

* **Générateur Double Mode (GUI, Extension, CLI & MCP) :**
* Mots de passe aléatoires haute entropie (`OsRng`, exclusion optionnelle des caractères ambigus `0/O`, `1/l/I`).
* Passphrases mémorisables (Diceware).


* **Tableau de Bord "Santé du Coffre" (100 % Offline) :**
* Détection des mots de passe faibles ou génériques via **`zxcvbn`** (repère les mots du dictionnaire, dates, suites clavier et motifs prévisibles).
* Détection des mots de passe réutilisés sur plusieurs comptes et alertes d'ancienneté.


* **Vérification des Fuites sans Fuite (*k-Anonymity* HIBP) :**
* Interrogation de *Have I Been Pwned* en n'envoyant que les **5 premiers caractères du hash SHA-1** : Akooti vérifie localement si le mot de passe est compromis sans jamais transmettre le secret ni son hash complet sur Internet.
