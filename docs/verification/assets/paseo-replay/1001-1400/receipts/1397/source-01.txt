import { create } from "zustand";
import { buildWorkspaceTabPersistenceKey } from "./workspace-tabs-store";

function trimNonEmpty(value: string | null | undefined): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function normalizeFilePath(value: string | null | undefined): string | null {
  const trimmed = trimNonEmpty(value);
  if (!trimmed) {
    return null;
  }
  return trimmed.replace(/\\/g, "/");
}

type WorkspaceFileTabsState = {
  openFilePathsByWorkspace: Record<string, string[]>;
  openFileTab: (input: {
    serverId: string;
    workspaceId: string;
    filePath: string;
  }) => void;
  closeFileTab: (input: {
    serverId: string;
    workspaceId: string;
    filePath: string;
  }) => void;
  isFileTabOpen: (input: {
    serverId: string;
    workspaceId: string;
    filePath: string;
  }) => boolean;
};

export const useWorkspaceFileTabsStore = create<WorkspaceFileTabsState>()(
  (set, get) => ({
    openFilePathsByWorkspace: {},
    openFileTab: ({ serverId, workspaceId, filePath }) => {
      const key = buildWorkspaceTabPersistenceKey({ serverId, workspaceId });
      const normalizedFilePath = normalizeFilePath(filePath);
      if (!key || !normalizedFilePath) {
        return;
      }

      set((state) => {
        const current = state.openFilePathsByWorkspace[key] ?? [];
        if (current.includes(normalizedFilePath)) {
          return state;
        }
        return {
          openFilePathsByWorkspace: {
            ...state.openFilePathsByWorkspace,
            [key]: [...current, normalizedFilePath],
          },
        };
      });
    },
    closeFileTab: ({ serverId, workspaceId, filePath }) => {
      const key = buildWorkspaceTabPersistenceKey({ serverId, workspaceId });
      const normalizedFilePath = normalizeFilePath(filePath);
      if (!key || !normalizedFilePath) {
        return;
      }

      set((state) => {
        const current = state.openFilePathsByWorkspace[key] ?? [];
        if (current.length === 0 || !current.includes(normalizedFilePath)) {
          return state;
        }
        const next = current.filter((path) => path !== normalizedFilePath);
        if (next.length === 0) {
          const { [key]: _removed, ...rest } = state.openFilePathsByWorkspace;
          return { openFilePathsByWorkspace: rest };
        }
        return {
          openFilePathsByWorkspace: {
            ...state.openFilePathsByWorkspace,
            [key]: next,
          },
        };
      });
    },
    isFileTabOpen: ({ serverId, workspaceId, filePath }) => {
      const key = buildWorkspaceTabPersistenceKey({ serverId, workspaceId });
      const normalizedFilePath = normalizeFilePath(filePath);
      if (!key || !normalizedFilePath) {
        return false;
      }
      const current = get().openFilePathsByWorkspace[key] ?? [];
      return current.includes(normalizedFilePath);
    },
  })
);

