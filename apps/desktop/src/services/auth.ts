import { invokeCommand } from "./tauri";

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
  expired: boolean;
  expiresIn: number;
  refreshToken: RefreshToken;
  scope: string[];
  tokenType: string;
  value: string;
}

export interface UserInformation {
  tokenid: string;
  sex: number;
  pid: string;
  isBindWx: boolean;
  type: number;
  realname: string;
  uid: string;
  userAuthType: string;
  phone: string;
  id: string;
  scid: string;
  jobNumber: string;
  username: string;
  jti: string;
}

export interface RefreshToken {
  expiration: number;
  value: string;
}

export function login(request: LoginRequest): Promise<LoginResponse> {
  return invokeCommand<LoginResponse>("login", { request });
}
