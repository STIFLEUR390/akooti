# Guide — connecter son projet Supabase

Akooti ne fournit aucun serveur : la synchronisation utilise **votre** projet
Supabase. Le serveur ne reçoit que des blobs chiffrés (voir
[`docs/schema.md`](../schema.md) §3). Comptez dix minutes, une fois.

## 1. Créer le projet

1. Ouvrir le tableau de bord officiel : <https://supabase.com/dashboard>.
2. Créer un compte (GitHub, Google ou e-mail) puis **New project**.
3. Choisir un nom (par exemple `akooti`), un **mot de passe de base de
   données** (géré par Supabase, Akooti ne s'en sert pas) et la région la plus
   proche de vous.
4. Attendre la fin du provisioning (une à deux minutes).

> L'offre gratuite suffit : Akooti ne stocke qu'une petite table de blobs
> chiffrés. Le projet peut être mis en pause par Supabase après une longue
> inactivité ; il suffit de le réveiller depuis le tableau de bord.

## 2. Récupérer l'URL et la clé anon

Dans le projet : **Project Settings** (icône engrenage) → **API** →
**API Settings**.

| Champ Akooti | Où le trouver | Exemple |
| ------------ | ------------- | ------- |
| **URL du projet** | *Project URL* | `https://abcdefghijklm.supabase.co` |
| **Clé anon** | *Project API keys* → `anon` / `public` (ou `sb_publishable_…` sur les projets récents) | `eyJhbGciOi…` |

La clé `anon` est **publique par conception** : elle n'ouvre que ce que les
politiques RLS autorisent. Ne jamais utiliser la clé `service_role` dans
Akooti — elle contourne RLS.

L'URL se colle **sans slash final** (les deux formes fonctionnent, Akooti
normalise).

## 3. Créer le compte utilisateur

Toujours dans le projet : **Authentication** → **Users** → **Add user** →
*Create new user*.

- e-mail + mot de passe : ce sont les identifiants que vous saisirez dans
  Akooti (ils ne sont **pas** ceux de votre compte supabase.com) ;
- cocher *Auto Confirm User* pour éviter l'e-mail de confirmation.

Un seul utilisateur suffit : c'est vous. Les jetons de session sont stockés
chiffrés dans le coffre local, jamais dans un fichier en clair.

## 4. Installer le schéma

**SQL Editor** → **New query** → coller le contenu de
[`supabase/setup.sql`](../../supabase/setup.sql)
→ **Run**.

Le même script est **embarqué dans l'application** : dans « Sync &
sauvegardes », les boutons **Copier le script SQL** et **Enregistrer le
script…** évitent d'ouvrir le dépôt (utile pour une version packagée). Le script est idempotent (il peut être rejoué après une mise à jour),
accorde les droits au rôle `authenticated` (la RLS reste le verrou), recharge le
cache PostgREST et se termine par deux requêtes de vérification : les tables et
les politiques doivent apparaître dans le résultat.

Il crée :

- `akooti_vault_envelopes` (enveloppe publique : sel Argon2id + clé maître
  encapsulée) ;
- `akooti_sync_records` (les blobs `ciphertext`/`nonce` et leurs métadonnées) ;
- les politiques **RLS** (`auth.uid() = user_id`), le trigger
  `server_updated_at` / `version` et la publication Realtime.

## 5. Configurer Akooti

Dans le coffre déverrouillé : **Sync & sauvegardes** → **Synchronisation
Supabase** :

1. URL du projet ;
2. clé `anon` ;
3. e-mail et mot de passe du compte créé au §3 ;
4. **Se connecter** puis **Synchroniser maintenant**.

Le statut passe à « Connecté · active ». Les jetons sont chiffrés par la clé
maître ; « Se déconnecter » les efface.

## 6. Vérifier que le serveur ne lit rien

Dans le **SQL Editor** :

```sql
select record_type, left(ciphertext, 24) as apercu, nonce, is_deleted
from public.akooti_sync_records
order by server_updated_at desc
limit 5;
```

Vous devez voir uniquement du base64 et des métadonnées : aucun titre, aucune
URL, aucun nom de dossier. Un second utilisateur ne verrait **aucune** ligne,
même en connaissant un identifiant (RLS).

## 7. Dépannage

| Symptôme | Cause probable | Correction |
| -------- | -------------- | ---------- |
| « Supabase a refusé les identifiants ou le jeton » | mauvais e-mail/mot de passe, ou utilisateur non confirmé | recréer le mot de passe dans *Authentication → Users*, cocher *Auto Confirm* |
| « Le serveur Supabase est injoignable » | URL erronée, projet en pause, réseau coupé | vérifier l'URL, réveiller le projet |
| Connexion OK mais aucun événement Realtime | RLS ou publication manquante | rejouer le §4, vérifier `alter publication supabase_realtime add table …` |
| Les jetons expirent sans cesse | l'horloge système est très décalée | synchroniser l'heure de la machine |

## 8. Sécurité — ce qu'il faut retenir

- Akooti n'envoie **que** `ciphertext`, `nonce`, `record_type`,
  `client_updated_at`, `is_deleted` et l'enveloppe publique (SEC-12).
- La clé `service_role` ne doit jamais entrer dans Akooti.
- Les tables `mcp_tokens`, `mcp_audit_logs` et `backup_targets` restent locales.
- Supprimer le projet Supabase ne supprime pas le coffre : la base locale reste
  la source de vérité.
