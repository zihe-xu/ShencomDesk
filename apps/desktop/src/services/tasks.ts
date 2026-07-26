import { invokeCommand } from "./tauri";

export type TaskState =
  | "pending"
  | "running"
  | "success"
  | "failed"
  | "cancelled";

export interface TaskProgress {
  completed: number;
  total: number;
  percentage: number;
}

export interface TaskSnapshot {
  id: string;
  name: string;
  state: TaskState;
  progress: TaskProgress;
  error: string | null;
}

export interface CreateTaskRequest {
  name: string;
  totalSteps: number;
  stepDelayMs?: number;
}

export function createTask(request: CreateTaskRequest): Promise<TaskSnapshot> {
  return invokeCommand<TaskSnapshot>("create_task", { request });
}

export function getTaskStatus(taskId: string): Promise<TaskSnapshot> {
  return invokeCommand<TaskSnapshot>("get_task_status", { taskId });
}

export function listTasks(): Promise<TaskSnapshot[]> {
  return invokeCommand<TaskSnapshot[]>("list_tasks");
}

export function cancelTask(taskId: string): Promise<TaskSnapshot> {
  return invokeCommand<TaskSnapshot>("cancel_task", { taskId });
}
