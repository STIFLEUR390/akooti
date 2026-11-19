import { useRef, useState } from "react";

import type { Folder } from "@akooti/shared";

import type { ThemeMode } from "../hooks/useTheme";
import type { Screen } from "../hooks/useVaultData";
import type { BuildInfo } from "../lib/ipc";
import { Icon } from "./Icons";
import type { IconName } from "./Icons";

interface SidebarProps {
  folders: Folder[];
  counts: Record<string, number>;
  totalItems: number;
  activeFolderId: string | null;
  onSelectFolder: (id: string | null) => void;
  screen: Screen;
  onScreen: (screen: Screen) => void;
  onCreateFolder: (name: string) => void;
  onRenameFolder: (id: string, name: string) => void;
  onDeleteFolder: (id: string) => void;
  theme: ThemeMode;
  onToggleTheme: () => void;
  onLock: () => void;
  buildInfo: BuildInfo | null;
  /** Inactivity timeout enforced by the backend, in minutes. */
  autoLockMinutes: number;
  /** Folders shared with an active MCP token (phase 5); empty for now. */
  sharedFolderIds: ReadonlySet<string>;
  onCheckUpdate: () => void;
  onOpenGuide: () => void;
}

const SYSTEM_ICONS: Record<string, IconName> = {
  user: "user",
  terminal: "terminal",
  bot: "bot",
};

function iconFor(folder: Folder): IconName {
  return SYSTEM_ICONS[folder.icon] ?? "folder";
}

/**
 * Folder navigation.
 *
 * The three protected folders sit apart from user folders: they get their own
 * heading, their colour, a lock glyph, and no rename/delete affordance
 * (`docs/regles.md` §3.1).
 */
export function Sidebar({
  folders,
  counts,
  totalItems,
  activeFolderId,
  onSelectFolder,
  screen,
  onScreen,
  onCreateFolder,
  onRenameFolder,
  onDeleteFolder,
  theme,
  onToggleTheme,
  onLock,
  buildInfo,
  autoLockMinutes,
  sharedFolderIds,
  onCheckUpdate,
  onOpenGuide,
}: SidebarProps) {
  const [creating, setCreating] = useState(false);
  const [draftName, setDraftName] = useState("");
  const [renaming, setRenaming] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const renameCancelled = useRef(false);

  const systemFolders = folders.filter((folder) => folder.isSystem);
  const userFolders = folders.filter((folder) => !folder.isSystem);

  function submitFolder() {
    const name = draftName.trim();
    if (name.length > 0) {
      onCreateFolder(name);
    }
    setDraftName("");
    setCreating(false);
  }

  function startRename(folder: Folder) {
    renameCancelled.current = false;
    setRenameValue(folder.name);
    setRenaming(folder.id);
  }

  function submitRename(folder: Folder) {
    const cancelled = renameCancelled.current;
    renameCancelled.current = false;
    const name = renameValue.trim();
    if (!cancelled && name.length > 0 && name !== folder.name) {
      onRenameFolder(folder.id, name);
    }
    setRenaming(null);
  }

  return (
    <nav className="sidebar" aria-label="Navigation du coffre">
      <div className="sidebar__brand">
        <span className="sidebar__logo" aria-hidden="true">
          <img src="/logo.png" alt="" width={30} height={30} />
        </span>
        <h1 className="sidebar__brand-name">Akooti</h1>
      </div>

      <button
        type="button"
        className={
          activeFolderId === null && screen === "vault" ? "nav-item nav-item--active" : "nav-item"
        }
        onClick={() => {
          onSelectFolder(null);
          onScreen("vault");
        }}
        aria-current={activeFolderId === null && screen === "vault" ? "page" : undefined}
      >
        <Icon name="key" />
        <span className="nav-item__label">Toutes les entrées</span>
        <span className="nav-item__count">{totalItems}</span>
      </button>

      <h2 className="sidebar__heading">Dossiers système</h2>
      <ul className="nav-list">
        {systemFolders.map((folder) => {
          const isActive = screen === "vault" && activeFolderId === folder.id;
          return (
            <li key={folder.id}>
              <button
                type="button"
                className={isActive ? "nav-item nav-item--active" : "nav-item"}
                onClick={() => {
                  onSelectFolder(folder.id);
                  onScreen("vault");
                }}
                aria-current={isActive ? "page" : undefined}
              >
                <span
                  className="nav-item__icon nav-item__icon--system"
                  style={folder.color ? { color: folder.color } : undefined}
                >
                  <Icon name={iconFor(folder)} />
                </span>
                <span className="nav-item__label">
                  <span className="nav-item__name" title={folder.name}>
                    {folder.name}
                  </span>
                  <span
                    className="nav-item__tag"
                    role="img"
                    aria-label="Dossier système"
                    title="Dossier protégé"
                  >
                    <Icon name="lock" size={11} />
                  </span>
                </span>
                {sharedFolderIds.has(folder.id) ? (
                  <span className="nav-item__shared" title="Partagé avec un jeton MCP">
                    <Icon name="bot" size={12} />
                  </span>
                ) : null}
                <span className="nav-item__count">{counts[folder.id] ?? 0}</span>
              </button>
            </li>
          );
        })}
      </ul>

      <h2 className="sidebar__heading">
        Mes dossiers
        <button
          type="button"
          className="icon-button icon-button--small"
          onClick={() => setCreating((current) => !current)}
          aria-label="Créer un dossier"
          aria-expanded={creating}
        >
          <Icon name="plus" size={14} />
        </button>
      </h2>

      {creating ? (
        <form
          className="sidebar__inline-form"
          onSubmit={(event) => {
            event.preventDefault();
            submitFolder();
          }}
        >
          <input
            autoFocus
            value={draftName}
            onChange={(event) => setDraftName(event.target.value)}
            onBlur={submitFolder}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                event.stopPropagation();
                setDraftName("");
                setCreating(false);
              }
            }}
            placeholder="Nom du dossier"
            aria-label="Nom du nouveau dossier"
            maxLength={64}
          />
        </form>
      ) : null}

      {userFolders.length === 0 ? (
        <p className="sidebar__empty">
          Aucun dossier personnel. Créez-en un pour organiser vos entrées.
        </p>
      ) : (
        <ul className="nav-list">
          {userFolders.map((folder) => {
            const isActive = screen === "vault" && activeFolderId === folder.id;
            return (
              <li key={folder.id}>
                {renaming === folder.id ? (
                  <form
                    className="sidebar__inline-form"
                    onSubmit={(event) => {
                      event.preventDefault();
                      submitRename(folder);
                    }}
                  >
                    <input
                      autoFocus
                      value={renameValue}
                      onChange={(event) => setRenameValue(event.target.value)}
                      onBlur={() => submitRename(folder)}
                      onKeyDown={(event) => {
                        if (event.key === "Escape") {
                          event.stopPropagation();
                          renameCancelled.current = true;
                          setRenaming(null);
                        }
                      }}
                      aria-label={`Nouveau nom pour ${folder.name}`}
                      maxLength={64}
                    />
                  </form>
                ) : (
                  <button
                    type="button"
                    className={isActive ? "nav-item nav-item--active" : "nav-item"}
                    onClick={() => {
                      onSelectFolder(folder.id);
                      onScreen("vault");
                    }}
                    aria-current={isActive ? "page" : undefined}
                    onDoubleClick={() => startRename(folder)}
                  >
                    <span
                      className="nav-item__icon"
                      style={folder.color ? { color: folder.color } : undefined}
                    >
                      <Icon name={iconFor(folder)} />
                    </span>
                    <span className="nav-item__label">
                      <span className="nav-item__name" title={folder.name}>
                        {folder.name}
                      </span>
                    </span>
                    <span className="nav-item__count">{counts[folder.id] ?? 0}</span>
                  </button>
                )}
                <div className="nav-item__actions">
                  <button type="button" className="text-button" onClick={() => startRename(folder)}>
                    Renommer
                  </button>
                  <button
                    type="button"
                    className="text-button text-button--danger"
                    onClick={() => onDeleteFolder(folder.id)}
                  >
                    Supprimer
                  </button>
                </div>
              </li>
            );
          })}
        </ul>
      )}

      <div className="sidebar__tools">
        <button
          type="button"
          className={screen === "import" ? "nav-item nav-item--active" : "nav-item"}
          onClick={() => onScreen("import")}
          aria-current={screen === "import" ? "page" : undefined}
        >
          <Icon name="upload" />
          <span className="nav-item__label">Importer</span>
        </button>
        <button
          type="button"
          className={screen === "routing" ? "nav-item nav-item--active" : "nav-item"}
          onClick={() => onScreen("routing")}
          aria-current={screen === "routing" ? "page" : undefined}
        >
          <Icon name="sliders" />
          <span className="nav-item__label">Routage Dev Local</span>
        </button>
        <button
          type="button"
          className={screen === "mcp" ? "nav-item nav-item--active" : "nav-item"}
          onClick={() => onScreen("mcp")}
          aria-current={screen === "mcp" ? "page" : undefined}
        >
          <Icon name="bot" />
          <span className="nav-item__label">Accès IA</span>
        </button>
        <button
          type="button"
          className={screen === "health" ? "nav-item nav-item--active" : "nav-item"}
          onClick={() => onScreen("health")}
          aria-current={screen === "health" ? "page" : undefined}
        >
          <Icon name="shield" />
          <span className="nav-item__label">Santé</span>
        </button>
        <button
          type="button"
          className={screen === "sync" ? "nav-item nav-item--active" : "nav-item"}
          onClick={() => onScreen("sync")}
          aria-current={screen === "sync" ? "page" : undefined}
        >
          <Icon name="refresh" />
          <span className="nav-item__label">Sync &amp; sauvegardes</span>
        </button>
      </div>

      <footer className="sidebar__footer">
        <div className="sidebar__links">
          <button type="button" className="text-button" onClick={onOpenGuide}>
            Guide de démarrage
          </button>
          <button type="button" className="text-button" onClick={onCheckUpdate}>
            Vérifier les mises à jour
          </button>
        </div>
        <button type="button" className="nav-item" onClick={onLock}>
          <Icon name="lock" />
          <span className="nav-item__label">Verrouiller</span>
          <kbd>Ctrl L</kbd>
        </button>
        <button type="button" className="nav-item" onClick={onToggleTheme}>
          <Icon name={theme === "dark" ? "sun" : "moon"} />
          <span className="nav-item__label">
            {theme === "dark" ? "Thème clair" : "Thème sombre"}
          </span>
        </button>
        <p className="sidebar__build">Verrouillage auto : {autoLockMinutes} min</p>
        {buildInfo ? (
          <p className="sidebar__build">
            v{buildInfo.appVersion} · noyau {buildInfo.coreVersion} · {buildInfo.platform}
          </p>
        ) : null}
      </footer>
    </nav>
  );
}
