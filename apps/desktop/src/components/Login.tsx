import { type FormEvent, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { login } from "@/services/auth";
import { ShenDeskIpcError } from "@/services/tauri";

interface LoginProps {
  onSuccess: (displayName: string) => void;
}

export function Login({ onSuccess }: LoginProps) {
  const [phone, setPhone] = useState("");
  const [password, setPassword] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState("");
  const phoneInputRef = useRef<HTMLInputElement>(null);
  const passwordInputRef = useRef<HTMLInputElement>(null);

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
      const response = await login({
        username: phone.trim(),
        password,
      });
      onSuccess(
        response.data.additionalInformation.additionalInformation.realname ||
          phone.trim(),
      );
    } catch (requestError: unknown) {
      setError(formatLoginError(requestError));
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <main className="min-h-screen bg-background px-5 py-8 text-foreground sm:px-8">
      <section className="mx-auto flex min-h-[calc(100vh-4rem)] max-w-md items-center">
        <div className="w-full rounded-3xl border border-border bg-card p-7 shadow-2xl shadow-slate-950/10 sm:p-10">
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
                autoComplete="tel"
                autoFocus
                className="h-11 w-full rounded-md border border-input bg-background px-3 text-base outline-none transition-colors placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-60"
                disabled={isSubmitting}
                id="phone"
                inputMode="tel"
                onChange={(event) => {
                  setPhone(event.target.value);
                  setError("");
                }}
                placeholder="请输入手机号"
                ref={phoneInputRef}
                required
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
              <p className="text-sm text-red-600" id="login-error" role="alert">
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
        </div>
      </section>
    </main>
  );
}

function formatLoginError(error: unknown): string {
  if (error instanceof ShenDeskIpcError) {
    return error.message;
  }

  return "登录失败，请重试。";
}
