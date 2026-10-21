import {
  buildHostDraftRoute,
  buildHostAgentDetailRoute,
  buildHostWorkspaceAgentTabRoute,
} from "@/utils/host-routes";

type NotificationData = Record<string, unknown> | null | undefined;

function readNonEmptyString(
  data: NotificationData,
  key: string
): string | null {
  const value = data?.[key];
  if (typeof value !== "string") {
    return null;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export function resolveNotificationTarget(data: NotificationData): {
  serverId: string | null;
  agentId: string | null;
  workspaceId: string | null;
} {
  return {
    serverId: readNonEmptyString(data, "serverId"),
    agentId: readNonEmptyString(data, "agentId"),
    workspaceId:
      readNonEmptyString(data, "workspaceId") ??
      readNonEmptyString(data, "cwd"),
  };
}

export function buildNotificationRoute(data: NotificationData): string {
  const { serverId, agentId, workspaceId } = resolveNotificationTarget(data);
  if (serverId && agentId) {
    if (workspaceId) {
      return buildHostWorkspaceAgentTabRoute(serverId, workspaceId, agentId);
    }
    return buildHostAgentDetailRoute(serverId, agentId);
  }
  if (serverId) {
    return buildHostDraftRoute(serverId);
  }
  return "/";
}
