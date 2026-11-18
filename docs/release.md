# Release et mises à jour

Ce document décrit comment produire une version d'Akooti, la signer et publier
le canal de mise à jour. La CI fait le gros du travail ; les clés, elles, ne
sont jamais dans le dépôt.

## 1. Versionnement

- SemVer : `MAJOR.MINOR.PATCH`. La version vit dans **six** fichiers :
  `Cargo.toml` (workspace), `src-tauri/tauri.conf.json`, `package.json`,
  `apps/desktop/package.json`, `apps/extension/package.json`,
  `packages/shared/package.json`.
- Une version livrée = un tag `vX.Y.Z` sur `main` + une entrée `CHANGELOG.md`.
- Les migrations SQLCipher ne sont jamais modifiées : on en ajoute une et on
  incrémente `CURRENT_SCHEMA_VERSION`.

## 2. Clés de signature

Deux secrets, jamais commités :

| Secret | Rôle | Où |
| ------ | ---- | -- |
| `TAURI_SIGNING_PRIVATE_KEY` | signe les artefacts de mise à jour (Ed25519) | GitHub Actions secret / `~/.tauri/akooti.key` en local |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | mot de passe de la clé | idem |

Génération (une fois) :

```bash
bunx @tauri-apps/cli signer generate -w ~/.tauri/akooti.key -p ""
# la clé PUBLIQUE va dans tauri.conf.json → plugins.updater.pubkey
```

La clé publique est déjà dans `src-tauri/tauri.conf.json`. Si vous la
régénérez, mettez-la à jour **et** publiez une release : les binaires installés
vérifieront la signature avec l'ancienne clé et refuseront la mise à jour.

## 3. Construire en local

```bash
# Linux (deb + AppImage), artefacts de mise à jour signés
TAURI_SIGNING_PRIVATE_KEY="$HOME/.tauri/akooti.key" \
TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
bun run tauri build

# Un seul format, plus rapide
bun run tauri build --bundles deb
```

Sorties :

- `target/release/bundle/deb/Akooti_<version>_amd64.deb`
- `target/release/bundle/appimage/Akooti_<version>_amd64.AppImage`
- `target/release/bundle/*/…​.sig` (signature de l'artefact)
- `target/release/bundle/latest.json` (manifeste de mise à jour, si
  `createUpdaterArtifacts` est actif)

Sans la variable de signature, le build de bundle échoue dès qu'il doit signer
un artefact de mise à jour : c'est volontaire (pas de release silencieusement
non signée). Le workflow de release désactive ces artefacts quand le secret est
absent.

**Fedora (local)** : le CLI Tauri ne trouve pas la bibliothèque appindicator
(seul le `.so.1` runtime est installé, sans en-tête de développement). Créer un
shim `pkg-config` et deux liens symboliques vers `/lib64` :

```bash
mkdir -p /tmp/appindicator-lib /tmp/pkgconfig
ln -sf /lib64/libayatana-appindicator3.so.1 /tmp/appindicator-lib/libayatana-appindicator3.so
ln -sf /lib64/libayatana-appindicator3.so.1 /tmp/appindicator-lib/libayatana-appindicator3.so.1
# /tmp/pkgconfig/ayatana-appindicator3-0.1.pc : libdir=/tmp/appindicator-lib, Libs: -layatana-appindicator3
PKG_CONFIG_PATH=/tmp/pkgconfig bun run tauri build
```

Sur Ubuntu (CI), `libayatana-appindicator3-dev` fournit tout ; le shim n'est
qu'un dépannage local.

## 4. Publier

```bash
git tag v1.0.0
git push origin v1.0.0
```

Le workflow [`.github/workflows/release.yml`](../.github/workflows/release.yml)
construit Linux / macOS (Intel + ARM) / Windows, signe avec les secrets quand ils
existent, et ouvre une **release draft** avec les bundles et `latest.json`.
Relire la draft, vérifier les artefacts, publier.

Sans secrets de signature, la CI produit des bundles **non signés** : utile pour
un test, jamais pour une release publique.

## 5. Signature par plateforme

### macOS (notarisation)

Secrets : `APPLE_CERTIFICATE` (base64 du `.p12`), `APPLE_CERTIFICATE_PASSWORD`,
`APPLE_SIGNING_IDENTITY`, `APPLE_ID`, `APPLE_PASSWORD` (mot de passe
d'application), `APPLE_TEAM_ID`. `tauri-action` fait la signature puis la
soumission à la notarisation.

### Windows

Secrets : certificat de signature de code (`WINDOWS_CERTIFICATE` +
`WINDOWS_CERTIFICATE_PASSWORD`) si vous en avez un ; sinon les installateurs
NSIS/MSI restent non signés et SmartScreen affichera un avertissement.

### Linux

Les bundles `.deb`/`.AppImage` ne sont pas signés par défaut. Publier un
`SHA256SUMS` à côté des artefacts et le signer avec la clé de release
(`gpg --detach-sign`).

## 6. Canal de mise à jour

- Endpoint : `https://github.com/STIFLEUR390/akooti/releases/latest/download/latest.json`.
- `latest.json` contient la version, la date, les notes et l'URL + signature de
  chaque plateforme.
- L'application vérifie la signature Ed25519 avec la clé publique embarquée
  avant d'installer (`commands/updates.rs`). Une signature absente ou invalide
  est refusée.
- En cas d'incident : dépublier la release, publier une `latest.json` pointant
  sur la version précédente, puis corriger. Les installations existantes
  gardent leur version locale.

## 7. Checklist

- [ ] `cargo fmt --check`, `clippy -D warnings`, `cargo test --workspace`
- [ ] `bun run lint`, `format:check`, `typecheck`, `bun test`, `bun run build`
- [ ] `bun scripts/e2e/app.e2e.mjs` sur un profil jetable (ou job E2E)
- [ ] `CHANGELOG.md` à jour, docs de phase relues
- [ ] version alignée dans les six fichiers
- [ ] build local signé, taille du binaire < 20 Mo (hors WebView système)
- [ ] tag `vX.Y.Z`, draft de release relue, `latest.json` présent
- [ ] issues de suivi créées pour ce qui est resté hors périmètre
