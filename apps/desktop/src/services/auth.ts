import { invokeCommand } from "./tauri";

import {
  getAuthStateWithInvoker,
  loginWithInvoker,
  logoutWithInvoker,
  type AuthState,
  type LoginRequest,
} from "./auth-core";

export type {
  AuthState,
  LoginRequest,
  UserInformation,
} from "./auth-core";

export function login(request: LoginRequest): Promise<AuthState> {
  return loginWithInvoker(invokeCommand, request);
}

export function getAuthState(): Promise<AuthState> {
  return getAuthStateWithInvoker(invokeCommand);
}

export function logout(): Promise<AuthState> {
  return logoutWithInvoker(invokeCommand);
}
