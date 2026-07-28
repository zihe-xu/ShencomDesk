import { FormEvent, useState } from "react";

import { Button } from "@/components/ui/button";
import { login } from "@/services/auth";
import { ShenDeskIpcError } from "@/services/tauri";

function App() {
  const [phone, setPhone] = useState("");
  const [password, setPassword] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState("");
  const [welcome, setWelcome] = useState("");

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    setError("");
    setWelcome("");

    if (!phone.trim() || !password) {
      setError("请输入手机号和密码。");
      return;
    }

    setIsSubmitting(true);
    try {
      const response = await login({ username: phone.trim(), password });
      setWelcome(`欢迎回来，${response.data.additionalInformation.additionalInformation.realname || phone}`);
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
            <p className="text-sm font-medium tracking-wide text-muted-foreground">Shencom Desktop Platform</p>
            <h1 className="text-3xl font-semibold tracking-tight">登录 ShenDesk</h1>
            <p className="text-base leading-7 text-muted-foreground">使用您的手机号和密码继续。</p>
          </div>

          <form className="space-y-5" onSubmit={(event) => void handleSubmit(event)}>
            <div className="space-y-2">
              <label className="text-sm font-medium" htmlFor="phone">手机号</label>
              <input
                autoComplete="tel"
                className="h-11 w-full rounded-md border border-input bg-background px-3 text-base outline-none transition-colors placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"
                id="phone"
                inputMode="tel"
                onChange={(event) => setPhone(event.target.value)}
                placeholder="请输入手机号"
                type="tel"
                value={phone}
              />
            </div>

            <div className="space-y-2">
              <label className="text-sm font-medium" htmlFor="password">密码</label>
              <input
                autoComplete="current-password"
                className="h-11 w-full rounded-md border border-input bg-background px-3 text-base outline-none transition-colors placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring"
                id="password"
                onChange={(event) => setPassword(event.target.value)}
                placeholder="请输入密码"
                type="password"
                value={password}
              />
            </div>

            {error && <p aria-live="polite" className="text-sm text-red-600">{error}</p>}
            {welcome && <p aria-live="polite" className="text-sm text-emerald-700">{welcome}</p>}

            <Button className="h-11 w-full" disabled={isSubmitting} size="lg" type="submit">
              {isSubmitting ? "正在登录…" : "登录"}
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

export default App;
