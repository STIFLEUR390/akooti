import { useCallback, useMemo, useRef, useState } from "react";

import type { VaultItem } from "@akooti/shared";

import { GeneratorModal } from "../components/GeneratorModal";
import { Icon } from "../components/Icons";
import { ItemDetail } from "../components/ItemDetail";
import { ItemFormModal } from "../components/ItemFormModal";
import { ItemList } from "../components/ItemList";
import { OnboardingModal } from "../components/OnboardingModal";
import { SecretEditorModal } from "../components/SecretEditorModal";
import { Sidebar } from "../components/Sidebar";
import { Toasts } from "../components/Toasts";
import type { Notices, Toast } from "../hooks/useToasts";
import { useSecretReveal } from "../hooks/useSecretReveal";
import { useShortcuts } from "../hooks/useShortcuts";
import { useTheme } from "../hooks/useTheme";
import { useTotp } from "../hooks/useTotp";
import { useVaultData } from "../hooks/useVaultData";
import type { AkootiApi, BuildInfo, PasswordOptions } from "../lib/ipc";
import { toIpcError } from "../lib/ipc";
import { HealthPage } from "./HealthPage";
import { ImportPage } from "./ImportPage";
import { McpPage } from "./McpPage";
import { RoutingPage } from "./RoutingPage";
import { SyncPage } from "./SyncPage";

interface VaultPageProps {
  api: AkootiApi;
  notices: Notices;
  toasts: Toast[];
  dismissToast: (id: number) => void;
  buildInfo: BuildInfo | null;
  autoLockMinutes: number;
  onLock: () => void;
}

/** No folder is shared with an MCP token yet (phase 5 fills this). */
const EMPTY_SET: ReadonlySet<string> = new Set<string>();

/** First-run guide flag, per machine. */
const GUIDE_KEY = "akooti.onboarding.seen";

type ModalState =
  | { kind: "none" }
  | { kind: "create" }
  | { kind: "edit"; item: VaultItem }
  | { kind: "secrets"; item: VaultItem }
  | { kind: "generator" };

const DEFAULT_PASSWORD_OPTIONS: PasswordOptions = {
  length: 24,
  lowercase: true,
  uppercase: true,
  digits: true,
  symbols: true,
  excludeAmbiguous: true,
};

/**
 * Unlocked shell: sidebar, list, detail, and the three modals.
 *
 * This container owns the shortcuts so every action stays reachable from the
 * keyboard (`docs/regles.md` §3.1), and it is the only place that wires the
 * reveal/copy hooks to a component.
 */
export function VaultPage({
  api,
  notices,
  toasts,
  dismissToast,
  buildInfo,
  autoLockMinutes,
  onLock,
}: VaultPageProps) {
  const { theme, toggle } = useTheme();
  const vault = useVaultData(api, notices, onLock);
  const secrets = useSecretReveal(api, notices);
  const [modal, setModal] = useState<ModalState>({ kind: "none" });
  const [guideOpen, setGuideOpen] = useState(() => {
    if (typeof localStorage === "undefined") {
      return false;
    }
    return localStorage.getItem(GUIDE_KEY) !== "seen";
  });
  const searchRef = useRef<HTMLInputElement>(null);

  const selected = vault.selected;
  const totp = useTotp(api, selected?.id ?? null, Boolean(selected?.hasTotp));

  const counts = useMemo(() => {
    const result: Record<string, number> = {};
    for (const item of vault.items) {
      for (const folderId of item.folderIds) {
        result[folderId] = (result[folderId] ?? 0) + 1;
      }
    }
    return result;
  }, [vault.items]);

  const activeFolderName =
    vault.activeFolderId === null
      ? null
      : (vault.folders.find((folder) => folder.id === vault.activeFolderId)?.name ?? null);

  const closeModal = useCallback(() => setModal({ kind: "none" }), []);

  const closeGuide = useCallback(() => {
    try {
      localStorage.setItem(GUIDE_KEY, "seen");
    } catch {
      // Private browsing: the guide reappears next launch, nothing is lost.
    }
    setGuideOpen(false);
  }, []);

  const copyTotp = useCallback(() => {
    if (totp.code) {
      void secrets.copy(totp.code, "Code TOTP");
    }
  }, [secrets, totp.code]);

  const shortcuts = useMemo(
    () => ({
      "/": () => searchRef.current?.focus(),
      "mod+k": () => searchRef.current?.focus(),
      "mod+b": () => {
        if (selected?.username) {
          void secrets.copy(selected.username, "Identifiant");
        }
      },
      "mod+t": () => copyTotp(),
      "mod+g": () => setModal({ kind: "generator" }),
      "mod+n": () => setModal({ kind: "create" }),
      "mod+l": onLock,
      escape: () => {
        if (modal.kind !== "none") {
          closeModal();
        } else if (secrets.revealed) {
          secrets.hide();
        } else if (vault.query.length > 0) {
          vault.setQuery("");
        }
      },
    }),
    [closeModal, copyTotp, modal.kind, onLock, secrets, selected?.username, vault],
  );

  useShortcuts(shortcuts, modal.kind === "none");

  const folders = vault.folders;

  return (
    <div className="app">
      <Sidebar
        folders={folders}
        counts={counts}
        activeFolderId={vault.activeFolderId}
        onSelectFolder={vault.setActiveFolderId}
        screen={vault.screen}
        onScreen={vault.setScreen}
        onCreateFolder={(name) => {
          void vault.createFolder({ name, requireBiometricForMcp: false, parentId: null });
        }}
        onRenameFolder={(id, name) => {
          void vault.renameFolder(id, name);
        }}
        onDeleteFolder={(id) => {
          void vault.deleteFolder(id);
        }}
        theme={theme}
        onToggleTheme={toggle}
        onLock={onLock}
        buildInfo={buildInfo}
        autoLockMinutes={autoLockMinutes}
        sharedFolderIds={EMPTY_SET}
        onOpenGuide={() => setGuideOpen(true)}
        onCheckUpdate={() => {
          void (async () => {
            try {
              const update = await api.checkUpdate();
              if (!update) {
                notices.success("Akooti est à jour.");
                return;
              }
              notices.success(`Mise à jour ${update.version} disponible.`);
              if (window.confirm(`Installer Akooti ${update.version} puis redémarrer ?`)) {
                await api.installUpdate();
              }
            } catch (error) {
              notices.failure(toIpcError(error).message);
            }
          })();
        }}
      />

      <main className="content">
        {vault.screen === "vault" ? (
          <>
            <ItemList
              items={vault.visibleItems}
              folders={folders}
              selectedId={vault.selectedId}
              onSelect={vault.setSelectedId}
              query={vault.query}
              onQuery={vault.setQuery}
              onCreate={() => setModal({ kind: "create" })}
              onOpenGenerator={() => setModal({ kind: "generator" })}
              searchRef={searchRef}
              loading={vault.loading}
              activeFolderName={activeFolderName}
            />

            {selected ? (
              <ItemDetail
                item={selected}
                folders={folders}
                revealedValue={secrets.revealed?.value ?? null}
                revealedField={secrets.revealed?.field ?? null}
                revealedCustomFieldId={secrets.revealed?.customFieldId ?? null}
                revealRemaining={secrets.remaining}
                revealPending={secrets.pending}
                onReveal={(field, customFieldId) => {
                  void secrets.reveal(selected.id, field, customFieldId);
                }}
                onHideReveal={secrets.hide}
                onCopy={(value, label) => {
                  void secrets.copy(value, label);
                }}
                totpCode={totp.code}
                totpSeconds={totp.seconds}
                onEdit={() => setModal({ kind: "edit", item: selected })}
                onEditSecrets={() => setModal({ kind: "secrets", item: selected })}
                onDelete={() => {
                  secrets.hide();
                  void vault.deleteItem(selected.id);
                }}
                onToggleFavorite={(favorite) => {
                  void vault.updateItem({
                    id: selected.id,
                    itemType: selected.itemType,
                    title: selected.title,
                    primaryUrl: selected.primaryUrl,
                    username: selected.username,
                    selectors: selected.selectors,
                    favorite,
                    expiresAt: selected.expiresAt,
                    folderIds: selected.folderIds,
                    uris: selected.uris.map((uri) => ({
                      uri: uri.uri,
                      matchType: uri.matchType,
                    })),
                  });
                }}
                onAddToFolder={(folderId) => {
                  void vault.addToFolder(selected.id, folderId);
                }}
                onRemoveFromFolder={(folderId) => {
                  void vault.removeFromFolder(selected.id, folderId);
                }}
                onAddCustomField={(field) => {
                  void vault.addCustomField(selected.id, field);
                }}
                onRemoveCustomField={(fieldId) => {
                  void vault.removeCustomField(selected.id, fieldId);
                }}
                onOpenGenerator={() => setModal({ kind: "generator" })}
              />
            ) : (
              <div className="detail detail--empty">
                <Icon name="shield" size={28} />
                <h2>Sélectionnez une entrée</h2>
                <p>
                  Choisissez une entrée à gauche, ou créez-en une avec <kbd>Ctrl N</kbd>. Aucun
                  secret n'est affiché tant que vous ne le demandez pas.
                </p>
              </div>
            )}
          </>
        ) : null}

        {vault.screen === "import" ? (
          <ImportPage api={api} notices={notices} onImported={() => void vault.reload()} />
        ) : null}

        {vault.screen === "routing" ? (
          <RoutingPage
            api={api}
            folders={folders}
            notices={notices}
            onApplied={() => void vault.reload()}
          />
        ) : null}

        {vault.screen === "mcp" ? <McpPage api={api} folders={folders} notices={notices} /> : null}

        {vault.screen === "sync" ? <SyncPage api={api} /> : null}

        {vault.screen === "health" ? (
          <HealthPage
            api={api}
            onAudited={vault.reload}
            onOpenItem={(itemId) => {
              vault.setActiveFolderId(null);
              vault.setSelectedId(itemId);
              vault.setScreen("vault");
            }}
          />
        ) : null}
      </main>

      {modal.kind === "create" || modal.kind === "edit" ? (
        <ItemFormModal
          item={modal.kind === "edit" ? modal.item : null}
          folders={folders}
          defaultFolderId={vault.activeFolderId}
          onClose={closeModal}
          onCreate={vault.createItem}
          onUpdate={vault.updateItem}
          generatePassword={() => api.generatePassword(DEFAULT_PASSWORD_OPTIONS)}
        />
      ) : null}

      {modal.kind === "secrets" ? (
        <SecretEditorModal
          item={modal.item}
          onClose={closeModal}
          onSetSecret={(field, value) => vault.setSecret(modal.item.id, field, value)}
          generatePassword={() => api.generatePassword(DEFAULT_PASSWORD_OPTIONS)}
        />
      ) : null}

      {modal.kind === "generator" ? (
        <GeneratorModal
          onClose={closeModal}
          generatePassword={api.generatePassword}
          generatePassphrase={api.generatePassphrase}
          onCopy={(value, label) => {
            void secrets.copy(value, label);
          }}
        />
      ) : null}

      {guideOpen ? (
        <OnboardingModal
          onClose={closeGuide}
          onCreateItem={() => {
            closeGuide();
            setModal({ kind: "create" });
          }}
          onOpenScreen={(screen) => {
            closeGuide();
            vault.setScreen(screen);
          }}
        />
      ) : null}

      <Toasts toasts={toasts} onDismiss={dismissToast} />

      {secrets.clipboardSeconds > 0 ? (
        <div className="clipboard-badge" role="status">
          <Icon name="clock" size={13} />
          Presse-papiers effacé dans {secrets.clipboardSeconds} s
        </div>
      ) : null}
    </div>
  );
}
