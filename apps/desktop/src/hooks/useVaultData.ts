import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import type { Folder, VaultItem } from "@akooti/shared";

import { sortItems } from "../lib/format";
import type {
  AkootiApi,
  CreateItemInput,
  CustomFieldInput,
  FolderInput,
  UpdateItemInput,
} from "../lib/ipc";
import { toIpcError } from "../lib/ipc";
import type { Notices } from "./useToasts";

/** Top-level views of the unlocked application. */
export type Screen = "vault" | "import" | "routing" | "mcp" | "health" | "sync";

/**
 * Loads and mutates the vault contents.
 *
 * Every failure funnels through here: a `locked` code tells the caller to drop
 * back to the unlock screen, anything else becomes a toast. No command result
 * is ever cached in a ref, so a lock immediately invalidates the view.
 */
export function useVaultData(api: AkootiApi, notices: Notices, onLocked: () => void) {
  const [folders, setFolders] = useState<Folder[]>([]);
  const [items, setItems] = useState<VaultItem[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [activeFolderId, setActiveFolderId] = useState<string | null>(null);
  const [screen, setScreen] = useState<Screen>("vault");
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(false);

  const { success, failure } = notices;
  const lockedRef = useRef(onLocked);
  lockedRef.current = onLocked;

  const handle = useCallback(
    async <T>(operation: () => Promise<T>): Promise<T | null> => {
      try {
        return await operation();
      } catch (error) {
        const ipc = toIpcError(error);
        if (ipc.isLocked) {
          lockedRef.current();
        } else {
          failure(ipc.message);
        }
        return null;
      }
    },
    [failure],
  );

  const reload = useCallback(async () => {
    setLoading(true);
    try {
      const [nextFolders, nextItems] = await Promise.all([
        handle(() => api.listFolders(false)),
        handle(() => api.listItems({})),
      ]);
      if (nextFolders) {
        setFolders(nextFolders);
      }
      if (nextItems) {
        setItems(sortItems(nextItems));
      }
    } finally {
      setLoading(false);
    }
  }, [api, handle]);

  useEffect(() => {
    void reload();
  }, [reload]);

  const selected = useMemo(
    () => items.find((item) => item.id === selectedId) ?? null,
    [items, selectedId],
  );

  const replaceItem = useCallback((item: VaultItem) => {
    setItems((current) =>
      sortItems(current.map((candidate) => (candidate.id === item.id ? item : candidate))),
    );
  }, []);

  const createItem = useCallback(
    async (draft: CreateItemInput) => {
      const created = await handle(() => api.createItem(draft));
      if (created) {
        setItems((current) => sortItems([...current, created]));
        setSelectedId(created.id);
        success(`« ${created.title} » ajouté au coffre.`);
      }
      return created;
    },
    [api, handle, success],
  );

  const updateItem = useCallback(
    async (draft: UpdateItemInput) => {
      const updated = await handle(() => api.updateItem(draft));
      if (updated) {
        replaceItem(updated);
        success("Entrée mise à jour.");
      }
      return updated;
    },
    [api, handle, replaceItem, success],
  );

  const deleteItem = useCallback(
    async (id: string) => {
      const done = await handle(async () => {
        await api.deleteItem(id);
        return true;
      });
      if (done) {
        setItems((current) => current.filter((item) => item.id !== id));
        setSelectedId((current) => (current === id ? null : current));
        success("Entrée déplacée dans la corbeille.");
      }
    },
    [api, handle, success],
  );

  const setSecret = useCallback(
    async (id: string, field: "password" | "totp" | "notes", value: string | null) => {
      const done = await handle(async () => {
        await api.setItemSecret(id, field, value);
        return true;
      });
      if (done) {
        const refreshed = await handle(() => api.getItem(id));
        if (refreshed) {
          replaceItem(refreshed);
        }
      }
      return done === true;
    },
    [api, handle, replaceItem],
  );

  const addCustomField = useCallback(
    async (itemId: string, field: CustomFieldInput) => {
      const updated = await handle(() => api.addItemCustomField(itemId, field));
      if (updated) {
        replaceItem(updated);
        success("Champ ajouté.");
      }
      return updated;
    },
    [api, handle, replaceItem, success],
  );

  const removeCustomField = useCallback(
    async (itemId: string, fieldId: string) => {
      const done = await handle(async () => {
        await api.removeItemCustomField(itemId, fieldId);
        return true;
      });
      if (done) {
        const refreshed = await handle(() => api.getItem(itemId));
        if (refreshed) {
          replaceItem(refreshed);
        }
        success("Champ supprimé.");
      }
    },
    [api, handle, replaceItem, success],
  );

  const addToFolder = useCallback(
    async (itemId: string, folderId: string) => {
      const done = await handle(async () => {
        await api.addItemToFolder(itemId, folderId);
        return true;
      });
      if (done) {
        const refreshed = await handle(() => api.getItem(itemId));
        if (refreshed) {
          replaceItem(refreshed);
        }
        success("Dossier ajouté à l'entrée.");
      }
    },
    [api, handle, replaceItem, success],
  );

  const removeFromFolder = useCallback(
    async (itemId: string, folderId: string) => {
      const done = await handle(async () => {
        await api.removeItemFromFolder(itemId, folderId);
        return true;
      });
      if (done) {
        const refreshed = await handle(() => api.getItem(itemId));
        if (refreshed) {
          replaceItem(refreshed);
        }
        success("Dossier retiré de l'entrée.");
      }
    },
    [api, handle, replaceItem, success],
  );

  const createFolder = useCallback(
    async (draft: FolderInput) => {
      const created = await handle(() => api.createFolder(draft));
      if (created) {
        setFolders((current) => [...current, created]);
        success(`Dossier « ${created.name} » créé.`);
      }
      return created;
    },
    [api, handle, success],
  );

  const renameFolder = useCallback(
    async (id: string, name: string) => {
      const updated = await handle(() => api.renameFolder(id, name));
      if (updated) {
        setFolders((current) =>
          current.map((folder) => (folder.id === updated.id ? updated : folder)),
        );
        success("Dossier renommé.");
      }
      return updated;
    },
    [api, handle, success],
  );

  const deleteFolder = useCallback(
    async (id: string) => {
      const done = await handle(async () => {
        await api.deleteFolder(id);
        return true;
      });
      if (done) {
        setFolders((current) => current.filter((folder) => folder.id !== id));
        setActiveFolderId((current) => (current === id ? null : current));
        success("Dossier supprimé.");
      }
    },
    [api, handle, success],
  );

  const visibleItems = useMemo(() => {
    const folderId = activeFolderId;
    if (!folderId) {
      return items;
    }
    return items.filter((item) => item.folderIds.includes(folderId));
  }, [activeFolderId, items]);

  return {
    folders,
    items,
    visibleItems,
    selected,
    selectedId,
    setSelectedId,
    activeFolderId,
    setActiveFolderId,
    screen,
    setScreen,
    query,
    setQuery,
    loading,
    reload,
    createItem,
    updateItem,
    deleteItem,
    setSecret,
    addCustomField,
    removeCustomField,
    addToFolder,
    removeFromFolder,
    createFolder,
    renameFolder,
    deleteFolder,
  };
}
