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
} {
  return {
    serverId: readNonEmptyString(data, "serverId"),
    agentId: readNonEmptyString(data, "agentId"),
  };
}

export function buildNotificationRoute(data: NotificationData): string {
  const { serverId, agentId } = resolveNotificationTarget(data);
  if (serverId && agentId) {
    return `/agent/${encodeURIComponent(serverId)}/${encodeURIComponent(agentId)}`;
  }
  if (agentId) {
    return `/agent/${encodeURIComponent(agentId)}`;
  }
  return "/agents";
}
