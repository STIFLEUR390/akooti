# Architecture Decision Records

Décisions structurantes d'Akooti, au format [ADR](https://adr.github.io/).

## Règles

- Une ADR décrit **une** décision : contexte, décision, alternatives écartées,
  conséquences.
- Une décision acceptée **ne se réécrit pas**. Si elle change, une nouvelle ADR
  la remplace et la référence explicitement ; l'ancienne passe au statut
  « remplacée par 00xx ».
- Une ADR entre dans la même PR que le code qu'elle justifie (règle ARCH-5 de
  [regles.md](../regles.md)).
- Les écarts entre un document et le code ont vocation à devenir une ADR dès
  qu'ils sont durables : c'est ainsi que l'écart reste visible au lieu de
  devenir un oubli.

Statuts utilisés : **acceptée**, **remplacée**, **proposée**, **abandonnée**.

## Index

| #                                                    | Décision                                                                  | Statut   |
| ---------------------------------------------------- | ------------------------------------------------------------------------- | -------- |
| [0001](0001-monorepo-rust-bun.md)                    | Monorepo Cargo + Bun, un binaire multi-modes                              | Acceptée |
| [0002](0002-sqlcipher-local-vault.md)                | Coffre local SQLCipher + chiffrement de champ XChaCha20                   | Acceptée |
| [0003](0003-mcp-stdio-blind-injection.md)            | MCP stdio, daemon IPC et injection aveugle                                | Acceptée |
| [0004](0004-vault-bootstrap-header.md)               | En-tête public d'amorçage du coffre                                       | Acceptée |
| [0005](0005-biometrie-deverrouillage.md)             | Biométrie : trait présent, fournisseur non branché                        | Acceptée |
| [0006](0006-transport-ipc-socket-unix.md)            | Transport IPC : socket Unix natif, named pipe différé                     | Acceptée |
| [0007](0007-fills-et-secrets-totp-courts.md)         | Remplissage d'un compte neuf, secrets TOTP courts, audit fidèle           | Acceptée |
| [0008](0008-extension-native-messaging.md)           | Extension navigateur : pont natif, remplissage piloté par l'extension     | Acceptée |

## Par où commencer

- **Comprendre la frontière de sécurité** : 0002 (stockage), 0003 (MCP), 0006
  (transport), 0008 (pont navigateur : un hôte qui ne décode rien).
- **Comprendre un écart assumé** : 0005 (biométrie), 0006 (Windows), 0007
  (TOTP courts), 0008 (passkeys servies par le navigateur).
- **Comprendre l'amorçage du coffre** : 0004 (en-tête public et dérivation de
  clé).
