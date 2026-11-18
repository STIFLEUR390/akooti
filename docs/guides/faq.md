# FAQ — sécurité et limites assumées

## Le serveur Supabase peut-il lire mes mots de passe ?

Non. Chaque dossier, entrée, passkey et règle est sérialisé puis chiffré en
XChaCha20-Poly1305 **côté client** avec la clé maître. Le serveur ne reçoit que
`ciphertext`, `nonce` et des métadonnées techniques (type, version,
horodatages). Il peut compter les enregistrements et voir quand ils changent,
pas ce qu'ils contiennent. L'enveloppe publique (sel Argon2id + clé encapsulée)
est, elle, lisible : c'est ce qui permet à un autre appareil de dériver la même
clé.

## Et le fournisseur S3 / Google Drive ?

Il reçoit des fichiers `.akooti` : la base SQLCipher et l'en-tête public, déjà
chiffrés. Un backup volé ne s'ouvre qu'avec votre mot de passe maître.

## Que se passe-t-il si je perds mon mot de passe maître ?

Le coffre est définitivement inaccessible. Il n'existe ni porte dérobée ni
récupération. C'est pourquoi l'application propose un export `.akooti` (chiffré,
restaurable) et un export JSON (en clair, après re-saisie du mot de passe). Une
sauvegarde non testée n'est pas une sauvegarde : restaurez-en une au moins une
fois.

## L'agent IA peut-il lire un secret ?

Non. Aucun outil MCP ne renvoie de valeur : il renvoie un identifiant, un
compte-rendu de remplissage (`filled`/`partial`) ou une sortie masquée par
`***`. Les valeurs voyagent du daemon vers la page (CDP/extension) ou vers
l'environnement d'un processus enfant, jamais vers le modèle.

## Le HIBP n'envoie-t-il pas mes mots de passe ?

Il n'envoie que les **5 premiers caractères du SHA-1** (k-anonymity, ~1 million
de candidats) et compare les suffixes localement. L'appel est déclenché par
vous, jamais en arrière-plan, et mis en cache 24 h. Hors ligne, le résultat
reste « inconnu » — rien ne bloque.

## Où vivent mes données ?

- Coffre : dossier de données de l'application (`com.akooti.desktop`).
- Synchronisation : votre projet Supabase.
- Sauvegardes : la cible que vous choisissez.
- Jetons MCP, journal d'audit, configuration de sauvegarde : **uniquement** en
  local, jamais synchronisés ni exportés.

## Que voit un jeton MCP ?

Uniquement les dossiers cochés et les portées accordées, jusqu'à son expiration
ou sa révocation. Chaque appel écrit une ligne d'audit locale. Un jeton ne peut
pas lire de secret, seulement déclencher un remplissage ou une exécution
masquée.

## Pourquoi la biométrie ne marche-t-elle pas ?

Le trait d'union existe, aucun fournisseur n'est branché : l'interface masque le
contrôle (ADR 0005). Le mot de passe maître reste la seule ouverture.

## Le coffre se verrouille-t-il tout seul ?

Oui : 15 minutes d'inactivité, mise en veille, fermeture de l'application. La
clé quitte la RAM ; toute commande répond `locked`. Le verrouillage de session
du système d'exploitation n'est pas encore un déclencheur (suivi post-release).

## Quelles sont les limites connues ?

- Windows (named pipe, manifeste de registre) et Safari ne sont pas livrés.
- La recette Google Drive sur un compte réel reste à faire (le pilote est validé
  contre un émulateur d'API v3).
- Les passkeys ne sont ni découvrables ni attestées, et ne vérifient pas
  l'utilisateur : la cérémonie retombe sur le navigateur si un site l'exige.
- Un OS compromis ou un keylogger peut lire ce que vous tapez : aucun coffre
  local ne s'en protège.
- La synchronisation révèle le nombre d'enregistrements et leurs horodatages,
  jamais leur contenu.

## Comment signaler une vulnérabilité ?

Voir [`SECURITY.md`](../../SECURITY.md) : GitHub Security Advisories, en
privé, avec des secrets factices. Le modèle de menace complet est dans
[`docs/threat-model.md`](../threat-model.md).
