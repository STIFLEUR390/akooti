# Guide — sauvegarder vers Google Drive

Akooti dépose les instantanés `.akooti` dans **votre** Drive avec un jeton
OAuth2 que vous fournissez. Aucun client OAuth n'est embarqué dans
l'application : vous créez le vôtre, une fois.

## 1. Créer un projet Google Cloud

1. <https://console.cloud.google.com/> → sélecteur de projet → **Nouveau
   projet** (par exemple `akooti-backup`).
2. **API et services** → **Bibliothèque** → activer **Google Drive API**.
3. **Écran de consentement OAuth** : type *Externe*, nom de l'application
   `Akooti`, votre e-mail comme contact. Ajouter votre adresse dans
   *Utilisateurs test* (mode test : le jeton reste valable, sans vérification
   Google pour un usage personnel).
4. **Identifiants** → **Créer des identifiants** → **ID client OAuth** →
   type **Application de bureau**.
5. Noter le **client id** et le **client secret**.

## 2. Obtenir un refresh token

Le plus simple, sans écrire de code : le **OAuth 2.0 Playground**.

1. <https://developers.google.com/oauthplayground/> → icône engrenage →
   cocher **Use your own OAuth credentials** → coller client id et secret.
2. Dans la liste des API, chercher `Drive API v3` → cocher
   `https://www.googleapis.com/auth/drive.file` (accès limité aux fichiers
   créés par Akooti ; `drive` complet n'est pas nécessaire).
3. **Authorize APIs** → se connecter avec le compte Google cible → accepter.
4. **Exchange authorization code for tokens** → copier le
   **Refresh token** (`1//0…`).

> Le refresh token ne change pas à chaque usage. Le révoquer depuis
> <https://myaccount.google.com/permissions> coupe l'accès d'Akooti
> immédiatement.

Variante en ligne de commande (même flux, sans Playground) :

```bash
# 1. Ouvrir l'URL affichée, autoriser, récupérer ?code=…
echo "https://accounts.google.com/o/oauth2/v2/auth?client_id=ID\
&redirect_uri=http://localhost&response_type=code\
&scope=https://www.googleapis.com/auth/drive.file&access_type=offline&prompt=consent"

# 2. Échanger le code contre les jetons
curl -s https://oauth2.googleapis.com/token \
  -d client_id=ID -d client_secret=SECRET \
  -d code=CODE -d grant_type=authorization_code \
  -d redirect_uri=http://localhost | jq -r .refresh_token
```

## 3. (Facultatif) Dossier de destination

Créer un dossier `Akooti` dans Drive, l'ouvrir, puis copier l'identifiant dans
l'URL : `https://drive.google.com/drive/folders/<ID>`. Sans identifiant,
Akooti écrit à la racine du Drive.

## 4. Remplir Akooti

**Sync & sauvegardes** → **Cibles de sauvegarde** :

| Champ | Valeur |
| ----- | ------ |
| Fournisseur | `Google Drive` |
| Client id OAuth2 | celui de l'étape 1 |
| Client secret | celui de l'étape 1 |
| Refresh token | celui de l'étape 2 |
| Dossier Drive (id) | l'identifiant de l'étape 3, ou vide |
| Intervalle / conservation | 60 min / 2 à 24 |

**Tester** doit répondre « Google Drive accessible ». Puis **Ajouter la
cible** et **Sauvegarder** une fois.

## 5. Validation côté Akooti

Le pilote a été vérifié contre l'émulateur d'API Google Drive v3
[`gfeh-drive`](https://docs.rs/gfeh-drive/) : test de connexion, trois envois,
rétention (2 objets conservés), téléchargement d'un objet et restauration du
`.akooti` téléchargé (ouverture du coffre avec le mot de passe maître). La
recette sur un vrai compte Google (OAuth, quotas, dossier) reste à faire — voir
[`desktop-tests.md`](../desktop-tests.md) §8.2.

## 6. Vérifier

Dans Drive, les fichiers `akooti-<epoch-ms>.akooti` apparaissent dans le
dossier choisi. Leur contenu est chiffré : Drive ne lit rien de plus que des
octets.

## 7. Dépannage

| Symptôme | Cause | Correction |
| -------- | ----- | ---------- |
| « Google a refusé le jeton de rafraîchissement » | jeton révoqué, client id/secret erronés, écran de consentement en test avec un autre compte | refaire l'étape 2 avec le bon compte |
| `403 insufficientPermissions` | scope trop étroit pour le dossier visé | utiliser `drive.file` et créer le fichier depuis Akooti, ou accorder `drive` |
| `Google Drive a refusé l'envoi (404)` | identifiant de dossier erroné | vider le champ (racine) ou recopier l'id |
| `Google Drive injoignable` | réseau/proxy | tester `curl -I https://www.googleapis.com` |

## 8. Sécurité

- Le refresh token est **chiffré** dans la base locale (clé maître) et n'est
  jamais renvoyé à l'interface.
- Préférer le scope `drive.file` : Akooti ne voit pas les autres fichiers du
  Drive.
- Révoquer l'accès (lien ci-dessus) si vous changez d'avis ; supprimer la cible
  dans Akooti efface la configuration chiffrée.
