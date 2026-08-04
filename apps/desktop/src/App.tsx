import { useEffect, useState } from "react";
import {
  Navigate,
  Route,
  Routes,
  useLocation,
  useNavigate,
} from "react-router-dom";

import { Login } from "@/components/Login";
import { Workspace } from "@/components/Workspace";
import { ImageCompression } from "@/components/image-compression/ImageCompression";
import { Toaster } from "@/components/ui/sonner";
import { getAuthState, logout, type AuthState } from "@/services/auth";
import {
  getConfig,
  saveConfig,
  type AppConfig,
  type ThemePreference,
} from "@/services/config";
import { ShenDeskIpcError } from "@/services/tauri";

const DEFAULT_CONFIG: AppConfig = {
  schemaVersion: 1,
  theme: "system",
  language: "zh-CN",
  autoStart: true,
};

function displayNameFrom(state: AuthState): string | null {
  if (!state.authenticated || !state.user) {
    return null;
  }

  return state.user.realname || state.user.username;
}

function App() {
  const location = useLocation();
  const navigate = useNavigate();
  const [displayName, setDisplayName] = useState<string | null>(null);
  const [isRestoring, setIsRestoring] = useState(true);
  const [isLoggingOut, setIsLoggingOut] = useState(false);
  const [isSavingTheme, setIsSavingTheme] = useState(false);
  const [config, setConfig] = useState(DEFAULT_CONFIG);
  const [error, setError] = useState("");

  useEffect(() => {
    const mediaQuery = window.matchMedia("(prefers-color-scheme: dark)");
    const applyTheme = () => {
      const isDark =
        config.theme === "dark" ||
        (config.theme === "system" && mediaQuery.matches);
      document.documentElement.classList.toggle("dark", isDark);
      document.documentElement.style.colorScheme = isDark ? "dark" : "light";
    };

    applyTheme();
    mediaQuery.addEventListener("change", applyTheme);
    return () => mediaQuery.removeEventListener("change", applyTheme);
  }, [config.theme]);

  useEffect(() => {
    let cancelled = false;

    const restoreConfig = getConfig()
      .then((storedConfig) => {
        if (!cancelled) {
          setConfig(storedConfig);
        }
      })
      .catch((requestError: unknown) => {
        if (!cancelled) {
          setError(formatConfigError(requestError));
        }
      });

    const restoreAuth = getAuthState()
      .then((state) => {
        if (!cancelled) {
          setDisplayName(displayNameFrom(state));
        }
      })
      .catch((requestError: unknown) => {
        if (!cancelled) {
          setError(formatAuthError(requestError));
        }
      });

    void Promise.all([restoreConfig, restoreAuth]).finally(() => {
      if (!cancelled) {
        setIsRestoring(false);
      }
    });

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (isRestoring) {
      return;
    }

    const frame = window.requestAnimationFrame(() => {
      document.querySelector<HTMLElement>("[data-route-heading]")?.focus();
    });
    return () => window.cancelAnimationFrame(frame);
  }, [isRestoring, location.pathname]);

  const handleLogout = async () => {
    setError("");
    setIsLoggingOut(true);
    try {
      const state = await logout();
      setDisplayName(displayNameFrom(state));
      navigate("/login", { replace: true });
    } catch (requestError: unknown) {
      setError(formatAuthError(requestError));
    } finally {
      setIsLoggingOut(false);
    }
  };

  const handleThemeChange = async (theme: ThemePreference) => {
    const previousConfig = config;
    const nextConfig = { ...config, theme };
    setError("");
    setConfig(nextConfig);
    setIsSavingTheme(true);

    try {
      setConfig(await saveConfig(nextConfig));
    } catch (requestError: unknown) {
      setConfig(previousConfig);
      setError(formatConfigError(requestError));
    } finally {
      setIsSavingTheme(false);
    }
  };

  const handleLoginSuccess = (name: string) => {
    setDisplayName(name);
    navigate("/workspace", { replace: true });
  };

  if (isRestoring) {
    return (
      <main className="min-h-screen bg-background px-5 py-8 text-foreground sm:px-8">
        <section className="mx-auto flex min-h-[calc(100vh-4rem)] max-w-md items-center">
          <div
            aria-live="polite"
            className="flex w-full min-h-52 flex-col items-center justify-center gap-4 rounded-3xl border border-border bg-card p-7 text-center shadow-2xl shadow-slate-950/10 sm:p-10"
            role="status"
          >
            <span
              aria-hidden="true"
              className="size-6 rounded-full border-2 border-muted-foreground/30 border-t-foreground motion-safe:animate-spin"
            />
            <p className="text-sm text-muted-foreground">正在恢复登录状态…</p>
          </div>
        </section>
      </main>
    );
  }

  return (
    <>
      <Routes>
        <Route
          element={
            <Navigate
              replace
              to={displayName ? "/workspace" : "/login"}
            />
          }
          path="/"
        />
        <Route
          element={
            displayName ? (
              <Navigate replace to="/workspace" />
            ) : (
              <Login onSuccess={handleLoginSuccess} />
            )
          }
          path="/login"
        />
        <Route
          element={
            displayName ? (
              <Workspace
                displayName={displayName}
                error={error}
                isLoggingOut={isLoggingOut}
                isSavingTheme={isSavingTheme}
                onLogout={() => void handleLogout()}
                onThemeChange={(theme) => void handleThemeChange(theme)}
                theme={config.theme}
              />
            ) : (
              <Navigate replace to="/login" />
            )
          }
          path="/workspace"
        />
        <Route
          element={
            displayName ? (
              <ImageCompression
                displayName={displayName}
                error={error}
                isLoggingOut={isLoggingOut}
                isSavingTheme={isSavingTheme}
                onLogout={() => void handleLogout()}
                onThemeChange={(theme) => void handleThemeChange(theme)}
                theme={config.theme}
              />
            ) : (
              <Navigate replace to="/login" />
            )
          }
          path="/tools/image-compression"
        />
        <Route
          element={
            <Navigate
              replace
              to={displayName ? "/workspace" : "/login"}
            />
          }
          path="*"
        />
      </Routes>
      <Toaster />
    </>
  );
}

function formatConfigError(error: unknown): string {
  if (error instanceof ShenDeskIpcError) {
    return error.message;
  }

  return "主题设置操作失败，请重试。";
}

function formatAuthError(error: unknown): string {
  if (error instanceof ShenDeskIpcError) {
    return error.message;
  }

  return "认证操作失败，请重试。";
}

export default App;
