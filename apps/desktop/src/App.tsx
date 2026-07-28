import { type FormEvent, useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  getAuthState,
  login,
  logout,
  type AuthState,
} from "@/services/auth";
import { ShenDeskIpcError } from "@/services/tauri";

function App() {
  const [authState, setAuthState] = useState<AuthState | null>(null);
  const [phone, setPhone] = useState("");
  const [password, setPassword] = useState("");
  const [isRestoring, setIsRestoring] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [isLoggingOut, setIsLoggingOut] = useState(false);
  const [error, setError] = useState("");
  const phoneInputRef = useRef<HTMLInputElement>(null);
  const passwordInputRef = useRef<HTMLInputElement>(null);
  const authenticatedHeadingRef = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    let cancelled = false;

    void getAuthState()
      .then((state) => {
        if (!cancelled) {
          setAuthState(state);
        }
      })
      .catch((requestError: unknown) => {
        if (!cancelled) {
          setError(formatAuthError(requestError));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setIsRestoring(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError("");

    if (!phone.trim() || !password) {
      setError("请输入手机号和密码。");
      if (!phone.trim()) {
        phoneInputRef.current?.focus();
      } else {
        passwordInputRef.current?.focus();
      }
      return;
    }

    setIsSubmitting(true);
    try {
      const state = await login({ username: phone.trim(), password });
      setAuthState(state);
      setPassword("");
    } catch (requestError: unknown) {
      setError(formatAuthError(requestError));
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleLogout = async () => {
    setError("");
    setIsLoggingOut(true);
    try {
      const state = await logout();
      setAuthState(state);
      setPhone("");
      setPassword("");
    } catch (requestError: unknown) {
      setError(formatAuthError(requestError));
    } finally {
      setIsLoggingOut(false);
    }
  };

  const authenticatedUser =
    authState?.authenticated && authState.user ? authState.user : null;

  useEffect(() => {
    if (!isRestoring && authenticatedUser) {
      authenticatedHeadingRef.current?.focus();
    }
  }, [authenticatedUser, isRestoring]);

  return (
    <main className="min-h-screen bg-background px-5 py-8 text-foreground sm:px-8">
      <section className="mx-auto flex min-h-[calc(100vh-4rem)] max-w-md items-center">
        <div className="w-full rounded-3xl border border-border bg-card p-7 shadow-2xl shadow-slate-950/10 sm:p-10">
          {isRestoring ? (
            <div
              aria-live="polite"
              className="flex min-h-52 flex-col items-center justify-center gap-4 text-center"
              role="status"
            >
              <span
                aria-hidden="true"
                className="size-6 rounded-full border-2 border-muted-foreground/30 border-t-foreground motion-safe:animate-spin"
              />
              <p className="text-sm text-muted-foreground">正在恢复登录状态…</p>
            </div>
          ) : authenticatedUser ? (
            <div aria-busy={isLoggingOut} className="space-y-7">
              <p aria-live="polite" className="sr-only" role="status">
                登录成功，当前用户为
                {authenticatedUser.realname || authenticatedUser.username}。
              </p>
              <div className="space-y-3">
                <p className="text-sm font-medium tracking-wide text-muted-foreground">
                  Shencom Desktop Platform
                </p>
                <h1
                  className="text-3xl font-semibold tracking-tight outline-none"
                  ref={authenticatedHeadingRef}
                  tabIndex={-1}
                >
                  已登录 ShenDesk
                </h1>
                <p className="text-base leading-7 text-muted-foreground">
                  您的认证凭据由系统安全存储保护。
                </p>
              </div>

              <div className="min-w-0 rounded-2xl border border-border bg-background p-5">
                <p className="text-sm text-muted-foreground">当前用户</p>
                <p className="mt-2 break-words text-lg font-semibold">
                  {authenticatedUser.realname || authenticatedUser.username}
                </p>
                <p className="mt-1 break-words text-sm text-muted-foreground">
                  {authenticatedUser.phone}
                </p>
              </div>

              {error && (
                <p className="text-sm text-red-600" role="alert">
                  {error}
                </p>
              )}

              <Button
                className="h-11 w-full gap-2"
                disabled={isLoggingOut}
                onClick={() => void handleLogout()}
                size="lg"
                type="button"
                variant="outline"
              >
                {isLoggingOut && (
                  <span
                    aria-hidden="true"
                    className="size-4 rounded-full border-2 border-muted-foreground/40 border-t-foreground motion-safe:animate-spin"
                  />
                )}
                <span>{isLoggingOut ? "正在退出…" : "退出登录"}</span>
              </Button>
            </div>
          ) : (
            <>
              <div className="mb-9 space-y-3">
                <p className="text-sm font-medium tracking-wide text-muted-foreground">
                  Shencom Desktop Platform
                </p>
                <h1 className="text-3xl font-semibold tracking-tight">
                  登录 ShenDesk
                </h1>
                <p className="text-base leading-7 text-muted-foreground">
                  使用您的手机号和密码继续。
                </p>
              </div>

              <form
                aria-busy={isSubmitting}
                className="space-y-5"
                noValidate
                onSubmit={(event) => void handleSubmit(event)}
              >
                <div className="space-y-2">
                  <label className="text-sm font-medium" htmlFor="phone">
                    手机号
                  </label>
                  <input
                    aria-describedby={error ? "login-error" : undefined}
                    aria-invalid={Boolean(error)}
                    autoComplete="username"
                    autoFocus
                    className="h-11 w-full rounded-md border border-input bg-background px-3 text-base outline-none transition-colors placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-60"
                    disabled={isSubmitting}
                    id="phone"
                    inputMode="tel"
                    name="username"
                    onChange={(event) => {
                      setPhone(event.target.value);
                      setError("");
                    }}
                    placeholder="请输入手机号"
                    ref={phoneInputRef}
                    required
                    spellCheck={false}
                    type="tel"
                    value={phone}
                  />
                </div>

                <div className="space-y-2">
                  <label className="text-sm font-medium" htmlFor="password">
                    密码
                  </label>
                  <input
                    aria-describedby={error ? "login-error" : undefined}
                    aria-invalid={Boolean(error)}
                    autoComplete="current-password"
                    className="h-11 w-full rounded-md border border-input bg-background px-3 text-base outline-none transition-colors placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-60"
                    disabled={isSubmitting}
                    id="password"
                    name="password"
                    onChange={(event) => {
                      setPassword(event.target.value);
                      setError("");
                    }}
                    placeholder="请输入密码"
                    ref={passwordInputRef}
                    required
                    type="password"
                    value={password}
                  />
                </div>

                {error && (
                  <p
                    className="text-sm text-red-600"
                    id="login-error"
                    role="alert"
                  >
                    {error}
                  </p>
                )}

                <Button
                  className="h-11 w-full gap-2"
                  disabled={isSubmitting}
                  size="lg"
                  type="submit"
                >
                  {isSubmitting && (
                    <span
                      aria-hidden="true"
                      className="size-4 rounded-full border-2 border-primary-foreground/40 border-t-primary-foreground motion-safe:animate-spin"
                    />
                  )}
                  <span>{isSubmitting ? "正在登录…" : "登录"}</span>
                </Button>
              </form>
            </>
          )}
        </div>
      </section>
    </main>
  );
}

function formatAuthError(error: unknown): string {
  if (error instanceof ShenDeskIpcError) {
    return error.message;
  }

  return "认证操作失败，请重试。";
}

export default App;
