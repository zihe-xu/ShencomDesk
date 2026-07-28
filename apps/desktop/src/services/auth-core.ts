export interface LoginRequest {
  username: string;
  password: string;
}

export interface LoginResponse {
  data: LoginData;
  errcode: string;
  errmsg: string;
}

export interface LoginData {
  additionalInformation: AccessToken;
}

export interface AccessToken {
  additionalInformation: UserInformation;
  expiration: number;
  expiresIn: number;
  refreshToken: unknown;
  scope: string[];
  tokenType: string;
  value: string;
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
): Promise<LoginResponse> {
  return invoke<LoginResponse>("login", { request });
}
