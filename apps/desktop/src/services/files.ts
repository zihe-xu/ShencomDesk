import { invokeCommand } from "./tauri";

export type FileChangeKind = "created" | "modified" | "removed" | "other";

export interface FileChange {
  watchId: string;
  path: string;
  kind: FileChangeKind;
}

export interface FileEntry {
  path: string;
  name: string;
  extension: string | null;
  sizeBytes: number;
  modifiedAtUnixMs: number | null;
  isDirectory: boolean;
}

export interface FileReadResult {
  entry: FileEntry;
  content: string;
  fromCache: boolean;
}

export interface FileIndex {
  root: string;
  entries: FileEntry[];
  scannedAtUnixMs: number;
  truncated: boolean;
}

export interface FileWatch {
  id: string;
  root: string;
  recursive: boolean;
}

export interface ReadTextFileRequest {
  path: string;
  maxBytes?: number;
}

export interface IndexFilesRequest {
  root: string;
  maxEntries?: number;
  maxDepth?: number;
}

export interface StartFileWatchRequest {
  path: string;
  recursive?: boolean;
}

export function readTextFile(
  request: ReadTextFileRequest,
): Promise<FileReadResult> {
  return invokeCommand<FileReadResult>("read_text_file", { request });
}

export function indexFiles(request: IndexFilesRequest): Promise<FileIndex> {
  return invokeCommand<FileIndex>("index_files", { request });
}

export function startFileWatch(
  request: StartFileWatchRequest,
): Promise<FileWatch> {
  return invokeCommand<FileWatch>("start_file_watch", { request });
}

export function stopFileWatch(watchId: string): Promise<string> {
  return invokeCommand<string>("stop_file_watch", { watchId });
}

export function clearFileCache(): Promise<void> {
  return invokeCommand<void>("clear_file_cache");
}
