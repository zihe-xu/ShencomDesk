export interface LoginRequest {
  username: string;
  password: string;
}

export interface AuthState {
  authenticated: boolean;
  user: UserInformation | null;
  expiresAt: number | null;
}

export interface UserInformation {
  realname: string;
  phone: string;
  username: string;
  uid: string;
}

export type CommandInvoker = <T>(
  command: string,
  args?: Record<string, unknown>,
) => Promise<T>;

export function loginWithInvoker(
  invoke: CommandInvoker,
  request: LoginRequest,
): Promise<AuthState> {
  return invoke<AuthState>("login", { request });
}

export function getAuthStateWithInvoker(
  invoke: CommandInvoker,
): Promise<AuthState> {
  return invoke<AuthState>("get_auth_state");
}

export function logoutWithInvoker(
  invoke: CommandInvoker,
): Promise<AuthState> {
  return invoke<AuthState>("logout");
}
