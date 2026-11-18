# Installation et premier lancement

Akooti est un binaire unique (Rust + WebView système). Aucun service n'est
installé, aucun compte n'est créé, rien n'est envoyé sans votre geste.

## 1. Installer

### Depuis une release

1. Télécharger le bundle de votre plateforme depuis
   <https://github.com/STIFLEUR390/akooti/releases> :
   - **Linux** : `.deb` (`sudo apt install ./Akooti_*.deb`) ou `.AppImage`
     (`chmod +x` puis exécution) ;
   - **macOS** : `.dmg` (glisser dans Applications) ;
   - **Windows** : `.msi` ou l'installateur NSIS.
2. Vérifier la signature ou l'empreinte publiée à côté de l'archive
   (`SHA256SUMS`).
3. Lancer **Akooti**.

Prérequis Linux : WebKitGTK 4.1 (`libwebkit2gtk-4.1-0`) et
`libayatana-appindicator3` — déjà présents sur la plupart des distributions
récentes ; le paquet `.deb` les déclare.

### Depuis les sources

```bash
git clone https://github.com/STIFLEUR390/akooti
cd akooti
bun install
bun run tauri dev        # développement (Vite + app)
bun run tauri build      # bundles dans target/release/bundle/
```

Dépendances système Linux (Fedora) :

```bash
sudo dnf install webkit2gtk4.1-devel libappindicator-gtk3-devel \
  librsvg2-devel openssl-devel gcc pkg-config
```

## 2. Créer le coffre

1. Écran d'accueil → **Nouveau coffre** (si aucun coffre n'existe).
2. Choisir un **mot de passe maître** long (12 caractères minimum ; une phrase
   de passe générée par Akooti est idéale) et le noter **hors de cette
   machine** : il n'existe aucune récupération.
3. Le guide de démarrage s'ouvre : cinq étapes vers les écrans réels.

Le message « aucun lecteur d'empreintes branché » est normal : la biométrie
n'est pas encore livrée (ADR 0005).

## 3. Restaurer un coffre existant

Sur l'écran de déverrouillage → **Restaurer un instantané** → choisir un fichier
`.akooti`. La restauration **remplace** le coffre local ; elle exige ensuite le
mot de passe maître de l'instantané.

## 4. Premiers réglages utiles

| Écran | À faire |
| ----- | ------- |
| **Importer** | Bitwarden, 1Password, KeePass, Proton Pass, Buttercup, CSV navigateurs |
| **Accès IA** | Créer un jeton restreint (dossiers + portées) pour votre agent |
| **Sync & sauvegardes** | Un dossier local ou un bucket S3/Drive, et éventuellement votre projet Supabase |
| **Santé** | Lancer un audit (score, réutilisation, ancienneté, HIBP) |

## 5. Mettre à jour

**Akooti → Vérifier les mises à jour** dans la barre latérale (ou la release).
L'application vérifie un manifeste signé et refuse toute mise à jour dont la
signature ne correspond pas à la clé embarquée.

## 6. Désinstaller proprement

- Désinstaller le paquet retire le binaire, **pas** vos données.
- Le coffre vit dans le dossier de données de l'application :
  - Linux : `~/.local/share/com.akooti.desktop/`
  - macOS : `~/Library/Application Support/com.akooti.desktop/`
  - Windows : `%APPDATA%\com.akooti.desktop\`
- Sauvegardez d'abord un `.akooti`, puis supprimez ce dossier : il n'y a aucune
  récupération possible ensuite.
