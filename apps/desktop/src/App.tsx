import { useCallback, useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  getConfig,
  resetConfig,
  saveConfig,
  type AppConfig,
} from "@/services/config";
import { getHealthStatus, type HealthStatus } from "@/services/health";
import { ShenDeskIpcError } from "@/services/tauri";

type RequestState = "idle" | "loading" | "ready" | "error";

function App() {
  const [health, setHealth] = useState<HealthStatus | null>(null);
  const [config, setConfig] = useState<AppConfig | null>(null);
  const [requestState, setRequestState] = useState<RequestState>("idle");
  const [message, setMessage] = useState("等待连接 Rust Core");

  const loadDesktopState = useCallback(async () => {
    setRequestState("loading");
    setMessage("正在通过 Tauri IPC 读取状态…");

    try {
      const [nextHealth, nextConfig] = await Promise.all([
        getHealthStatus(),
        getConfig(),
      ]);
      setHealth(nextHealth);
      setConfig(nextConfig);
      setRequestState("ready");
      setMessage("React 与 Rust Core 通信正常");
    } catch (error: unknown) {
      setRequestState("error");
      setMessage(formatIpcError(error));
    }
  }, []);

  useEffect(() => {
    void loadDesktopState();
  }, [loadDesktopState]);

  const toggleTheme = async () => {
    if (!config) {
      return;
    }

    setRequestState("loading");
    try {
      const saved = await saveConfig({
        ...config,
        theme: config.theme === "dark" ? "light" : "dark",
      });
      setConfig(saved);
      setRequestState("ready");
      setMessage(`配置已保存：theme = ${saved.theme}`);
    } catch (error: unknown) {
      setRequestState("error");
      setMessage(formatIpcError(error));
    }
  };

  const restoreDefaults = async () => {
    setRequestState("loading");
    try {
      const defaults = await resetConfig();
      setConfig(defaults);
      setRequestState("ready");
      setMessage("配置已恢复默认值");
    } catch (error: unknown) {
      setRequestState("error");
      setMessage(formatIpcError(error));
    }
  };

  const isBusy = requestState === "loading";

  return (
    <main className="min-h-screen bg-background px-6 py-10 text-foreground">
      <section className="mx-auto flex min-h-[calc(100vh-5rem)] max-w-5xl items-center">
        <div className="grid w-full gap-10 rounded-3xl border border-border bg-card p-8 shadow-2xl shadow-slate-950/10 md:grid-cols-[1.1fr_0.9fr] md:p-12">
          <div className="space-y-6">
            <div className="inline-flex rounded-full border border-border bg-muted px-3 py-1 text-xs font-medium text-muted-foreground">
              Shencom Desktop Platform
            </div>
            <div className="space-y-3">
              <h1 className="text-4xl font-semibold tracking-tight sm:text-5xl">ShenDesk</h1>
              <p className="max-w-xl text-base leading-7 text-muted-foreground sm:text-lg">
                基于 Tauri Command 的类型安全 React ↔ Rust 通信示例。
              </p>
            </div>
            <div className="flex flex-wrap gap-3">
              <Button disabled={isBusy} onClick={() => void loadDesktopState()}>
                刷新状态
              </Button>
              <Button disabled={isBusy || !config} variant="outline" onClick={() => void toggleTheme()}>
                切换主题配置
              </Button>
              <Button disabled={isBusy} variant="ghost" onClick={() => void restoreDefaults()}>
                恢复默认配置
              </Button>
            </div>
            <p
              aria-live="polite"
              className={requestState === "error" ? "text-sm text-red-600" : "text-sm text-muted-foreground"}
            >
              {message}
            </p>
          </div>

          <div className="rounded-2xl border border-border bg-muted/50 p-6">
            <h2 className="text-sm font-semibold uppercase tracking-[0.18em] text-muted-foreground">
              Issue #7 IPC Status
            </h2>
            <dl className="mt-5 grid gap-4 text-sm">
              <StatusRow label="Runtime" value={health?.status ?? "—"} />
              <StatusRow label="Version" value={health?.version ?? "—"} />
              <StatusRow
                label="Uptime"
                value={health ? `${health.uptimeSeconds}s` : "—"}
              />
              <StatusRow label="Theme" value={config?.theme ?? "—"} />
              <StatusRow label="Language" value={config?.language ?? "—"} />
              <StatusRow
                label="Auto Start"
                value={config ? String(config.autoStart) : "—"}
              />
            </dl>
          </div>
        </div>
      </section>
    </main>
  );
}

function StatusRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border pb-3 last:border-0 last:pb-0">
      <dt className="text-muted-foreground">{label}</dt>
      <dd className="font-mono text-xs font-medium">{value}</dd>
    </div>
  );
}

function formatIpcError(error: unknown): string {
  if (error instanceof ShenDeskIpcError) {
    return `IPC 调用失败 [${error.code}]：${error.message}`;
  }

  return error instanceof Error ? error.message : String(error);
}

export default App;
