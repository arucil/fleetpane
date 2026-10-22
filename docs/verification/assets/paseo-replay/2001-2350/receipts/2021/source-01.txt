import { execSync } from "child_process";
import { READ_ONLY_GIT_ENV } from "./checkout-git-utils.js";

export type WorkspaceGitMetadata = {
  projectKind: "git" | "directory";
  projectDisplayName: string;
  workspaceDisplayName: string;
  gitRemote: string | null;
  isWorktree: boolean;
};

export function readGitCommand(cwd: string, command: string): string | null {
  try {
    const output = execSync(command, {
      cwd,
      env: READ_ONLY_GIT_ENV,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
    const trimmed = output.trim();
    return trimmed.length > 0 ? trimmed : null;
  } catch {
    return null;
  }
}

export function parseGitHubRepoFromRemote(remoteUrl: string): string | null {
  let cleaned = remoteUrl.trim();
  if (!cleaned) {
    return null;
  }

  if (cleaned.startsWith("git@github.com:")) {
    cleaned = cleaned.slice("git@github.com:".length);
  } else if (cleaned.startsWith("https://github.com/")) {
    cleaned = cleaned.slice("https://github.com/".length);
  } else if (cleaned.startsWith("http://github.com/")) {
    cleaned = cleaned.slice("http://github.com/".length);
  } else {
    const marker = "github.com/";
    const markerIndex = cleaned.indexOf(marker);
    if (markerIndex === -1) {
      return null;
    }
    cleaned = cleaned.slice(markerIndex + marker.length);
  }

  if (cleaned.endsWith(".git")) {
    cleaned = cleaned.slice(0, -".git".length);
  }

  if (!cleaned.includes("/")) {
    return null;
  }

  return cleaned;
}

export function detectWorkspaceGitMetadata(
  cwd: string,
  directoryName: string,
): WorkspaceGitMetadata {
  const gitDir = readGitCommand(cwd, "git rev-parse --git-dir");
  if (!gitDir) {
    return {
      projectKind: "directory",
      projectDisplayName: directoryName,
      workspaceDisplayName: directoryName,
      gitRemote: null,
      isWorktree: false,
    };
  }

  const gitRemote = readGitCommand(cwd, "git config --get remote.origin.url");
  const githubRepo = gitRemote ? parseGitHubRepoFromRemote(gitRemote) : null;
  const branchName = readGitCommand(cwd, "git symbolic-ref --short HEAD");
  const gitCommonDir = readGitCommand(cwd, "git rev-parse --git-common-dir");
  const isWorktree = gitCommonDir !== null && gitDir !== gitCommonDir;

  return {
    projectKind: "git",
    projectDisplayName: githubRepo ?? directoryName,
    workspaceDisplayName: branchName ?? directoryName,
    gitRemote,
    isWorktree,
  };
}
