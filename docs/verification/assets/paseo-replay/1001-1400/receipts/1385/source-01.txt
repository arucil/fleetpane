import AsyncStorage from "@react-native-async-storage/async-storage";
import { create } from "zustand";
import { createJSONStorage, persist } from "zustand/middleware";

export type WorkspaceTabTarget =
  | { kind: "agent"; agentId: string }
  | { kind: "terminal"; terminalId: string };

function trimNonEmpty(value: string | null | undefined): string | null {
  if (typeof value !== "string") {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

function normalizeWorkspaceTab(
  value: WorkspaceTabTarget | null | undefined
): WorkspaceTabTarget | null {
  if (!value || typeof value !== "object" || typeof value.kind !== "string") {
    return null;
  }
  if (value.kind === "agent") {
    const agentId = trimNonEmpty(value.agentId);
    if (!agentId) {
      return null;
    }
    return { kind: "agent", agentId };
  }
  if (value.kind === "terminal") {
    const terminalId = trimNonEmpty(value.terminalId);
    if (!terminalId) {
      return null;
    }
    return { kind: "terminal", terminalId };
  }
  return null;
}

export function buildWorkspaceTabPersistenceKey(input: {
  serverId: string;
  workspaceId: string;
}): string | null {
  const serverId = trimNonEmpty(input.serverId);
  const workspaceId = trimNonEmpty(input.workspaceId);
  if (!serverId || !workspaceId) {
    return null;
  }
  return `${serverId}:${workspaceId}`;
}

type WorkspaceTabsState = {
  lastFocusedTabByWorkspace: Record<string, WorkspaceTabTarget>;
  setLastFocusedTab: (input: {
    serverId: string;
    workspaceId: string;
    tab: WorkspaceTabTarget;
  }) => void;
  getLastFocusedTab: (input: {
    serverId: string;
    workspaceId: string;
  }) => WorkspaceTabTarget | null;
};

export const useWorkspaceTabsStore = create<WorkspaceTabsState>()(
  persist(
    (set, get) => ({
      lastFocusedTabByWorkspace: {},
      setLastFocusedTab: ({ serverId, workspaceId, tab }) => {
        const key = buildWorkspaceTabPersistenceKey({ serverId, workspaceId });
        const normalizedTab = normalizeWorkspaceTab(tab);
        if (!key || !normalizedTab) {
          return;
        }

        set((state) => {
          const current = state.lastFocusedTabByWorkspace[key];
          if (
            current &&
            current.kind === normalizedTab.kind &&
            ((current.kind === "agent" && normalizedTab.kind === "agent"
              ? current.agentId === normalizedTab.agentId
              : current.kind === "terminal" && normalizedTab.kind === "terminal"
                ? current.terminalId === normalizedTab.terminalId
                : false))
          ) {
            return state;
          }

          return {
            lastFocusedTabByWorkspace: {
              ...state.lastFocusedTabByWorkspace,
              [key]: normalizedTab,
            },
          };
        });
      },
      getLastFocusedTab: ({ serverId, workspaceId }) => {
        const key = buildWorkspaceTabPersistenceKey({ serverId, workspaceId });
        if (!key) {
          return null;
        }
        const value = get().lastFocusedTabByWorkspace[key];
        return normalizeWorkspaceTab(value);
      },
    }),
    {
      name: "workspace-tabs-state",
      version: 1,
      storage: createJSONStorage(() => AsyncStorage),
      migrate: (persistedState) => {
        const state = persistedState as
          | {
              lastFocusedTabByWorkspace?: Record<string, WorkspaceTabTarget>;
            }
          | undefined;

        const raw = state?.lastFocusedTabByWorkspace ?? {};
        const next: Record<string, WorkspaceTabTarget> = {};

        for (const key in raw) {
          const value = raw[key];
          const normalized = normalizeWorkspaceTab(value);
          if (normalized) {
            next[key] = normalized;
          }
        }

        return {
          ...state,
          lastFocusedTabByWorkspace: next,
        };
      },
    }
  )
);
