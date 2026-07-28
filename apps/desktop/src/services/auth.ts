import { invokeCommand } from "./tauri";

import {
  loginWithInvoker,
  type LoginRequest,
  type LoginResponse,
} from "./auth-core";

export type {
  AccessToken,
  LoginData,
  LoginRequest,
  LoginResponse,
  UserInformation,
} from "./auth-core";

export function login(request: LoginRequest): Promise<LoginResponse> {
  return loginWithInvoker(invokeCommand, request);
}
