import { Channel } from "@tauri-apps/api/core";

import { invokeCommand } from "./tauri";

export interface UpdateInfo {
  currentVersion: string;
  version: string;
  notes: string | null;
  publishedAtUnixSeconds: number | null;
  target: string;
}

export type UpdateProgress =
  | {
      event: "started";
      data: {
        contentLength: number | null;
      };
    }
  | {
      event: "progress";
      data: {
        chunkLength: number;
        downloaded: number;
        contentLength: number | null;
      };
    }
  | {
      event: "finished";
      data: {
        downloaded: number;
      };
    };

export interface UpdateInstallResult {
  installed: boolean;
  restartRequested: boolean;
}

export interface InstallUpdateOptions {
  restart?: boolean;
  onProgress?: (event: UpdateProgress) => void;
}

export function checkForUpdates(): Promise<UpdateInfo | null> {
  return invokeCommand<UpdateInfo | null>("check_for_updates");
}

export function installUpdate(
  options: InstallUpdateOptions = {},
): Promise<UpdateInstallResult> {
  const onProgress = new Channel<UpdateProgress>();
  if (options.onProgress) {
    onProgress.onmessage = options.onProgress;
  }

  return invokeCommand<UpdateInstallResult>("install_update", {
    request: {
      restart: options.restart ?? false,
    },
    onProgress,
  });
}
