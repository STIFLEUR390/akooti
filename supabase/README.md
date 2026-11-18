# Supabase — synchronisation Akooti

Ce dossier ne contient **que** le schéma serveur de la synchronisation
chiffrée de bout en bout. Akooti n'héberge rien : chaque utilisateur applique ce
fichier à **son** projet Supabase (SCOPE-1).

## Guide utilisateur

Le pas-à-pas complet (création du compte, URL + clé `anon`, utilisateur,
application du SQL, dépannage) est dans
[`docs/guides/supabase.md`](../docs/guides/supabase.md).

## Mise en place

1. Créer un projet Supabase (offre gratuite suffisante) et récupérer l'URL du
   projet et la clé `anon` dans _Project Settings → API_.
2. Appliquer [`setup.sql`](./setup.sql) — le même script est embarqué dans
   l'application (boutons « Copier le script SQL » / « Enregistrer le script… »)
   dans le SQL Editor (ou `supabase db push` avec la CLI).
3. Activer l'authentification e-mail/mot de passe dans _Authentication →
   Providers_, créer le compte de l'utilisateur.
4. Dans Akooti, page **Sync & sauvegardes** : URL du projet, clé `anon`,
   e-mail et mot de passe Supabase. Akooti ne stocke que les jetons de session,
   chiffrés par la clé maître.

## Ce que le serveur voit

| Colonne                                                                                | Contenu                                               |
| -------------------------------------------------------------------------------------- | ----------------------------------------------------- |
| `id`, `record_type`, `version`, `client_updated_at`, `server_updated_at`, `is_deleted` | métadonnées techniques                                |
| `ciphertext`, `nonce`                                                                  | blob XChaCha20-Poly1305, illisible sans la clé maître |
| `akooti_vault_envelopes.*`                                                             | paramètres Argon2id publics et clé maître encapsulée  |

Aucune ligne ne contient de titre, d'URL, de nom de dossier, de nombre de
dossiers ou de secret en clair. Les tables `mcp_tokens`, `mcp_audit_logs` et
`backup_targets` ne sont **jamais** synchronisées (règle AI-5).

## Delta et conflits

- `server_updated_at` est avancé par le trigger à chaque écriture ; le client
  garde le dernier reçu comme curseur (`sync_config.cursor`).
- `client_updated_at` porte l'horodatage de l'appareil (LWW). Une suppression
  voyage en tombstone (`is_deleted = true`) pour ne rien révéler.
- Realtime diffuse les changements de la table ; le client retombe sur un delta
  complet s'il a manqué un événement.

## Conformité au SDK Supabase

Les requêtes d'Akooti ont été recoupées avec le client Rust
[`supabase-lib-rs`](https://github.com/nizovtsevnv/supabase-lib-rs) :

| Point            | SDK                                                                                                                                                        | Akooti                                                               |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------- |
| Connexion        | `POST {url}/auth/v1/token?grant_type=password`, en-têtes `apikey` + `Content-Type`                                                                         | identique                                                            |
| Rafraîchissement | `POST …/token?grant_type=refresh_token`                                                                                                                    | identique                                                            |
| Base             | `{url}/rest/v1/<table>`, `apikey`, `Authorization: Bearer <access_token>`                                                                                  | identique                                                            |
| Upsert           | `on_conflict=<colonnes>` + `Prefer: resolution=merge-duplicates`                                                                                           | identique (`return=minimal` au lieu de `return=representation`)      |
| Temps réel       | `{ws}/realtime/v1/websocket?apikey=…&vsn=1.0.0`, topic `realtime:<schema>:<table>`, `phx_join` avec `config.postgres_changes[{event,schema,table,filter}]` | identique, avec en plus `access_token` dans le payload de `phx_join` |

Deux écarts assumés :

- le SDK est **async/Tokio** ; Akooti reste sur `ureq` + threads standard, comme
  le reste du shell (aucun runtime async dans l'application) ;
- le SDK ne joint que la clé `anon` : sous RLS, les changements ne sont alors
  visibles qu'avec le jeton utilisateur. Akooti envoie `access_token` dans
  `phx_join`, comme le font les clients JS officiels.

## Vérification

```sql
-- RLS : avec la clé anon et une session utilisateur, seules ses lignes sortent.
select count(*) from public.akooti_sync_records;
-- Un autre utilisateur ne doit voir aucune ligne, même en connaissant un id.
```
