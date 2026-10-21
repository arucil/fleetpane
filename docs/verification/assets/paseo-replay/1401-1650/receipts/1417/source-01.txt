import {
  buildHostWorkspaceTabRoute,
  parseHostWorkspaceRouteFromPathname,
} from "@/utils/host-routes";

function stripSearchAndHash(pathname: string): string {
  const hashIndex = pathname.indexOf("#");
  const queryIndex = pathname.indexOf("?");
  const end = [hashIndex, queryIndex]
    .filter((index) => index >= 0)
    .reduce((min, index) => Math.min(min, index), pathname.length);
  return pathname.slice(0, end);
}

type ResolveWorkspaceTabCanonicalPathInput = {
  activeTabId: string | null;
  pathname: string;
  serverId: string;
  workspaceId: string;
};

export function resolveWorkspaceTabCanonicalPath(
  input: ResolveWorkspaceTabCanonicalPathInput
): string | null {
  const { activeTabId, pathname, serverId, workspaceId } = input;
  if (!activeTabId || !serverId || !workspaceId) {
    return null;
  }

  const canonicalPath = buildHostWorkspaceTabRoute(serverId, workspaceId, activeTabId);
  if (canonicalPath === "/") {
    return null;
  }

  const activeWorkspaceRoute = parseHostWorkspaceRouteFromPathname(pathname);
  if (
    !activeWorkspaceRoute ||
    activeWorkspaceRoute.serverId !== serverId ||
    activeWorkspaceRoute.workspaceId !== workspaceId
  ) {
    return null;
  }

  const pathOnly = stripSearchAndHash(pathname);
  if (pathOnly !== canonicalPath) {
    return canonicalPath;
  }

  // Keep workspace tab URLs clean: no query/hash source-of-truth for tab routing.
  if (pathname !== canonicalPath) {
    return canonicalPath;
  }

  return null;
}
