import { useEffect, useState } from "react";

import { Login } from "@/components/Login";
import { ImageCompression } from "@/components/image-compression/ImageCompression";
import { Toaster } from "@/components/ui/sonner";
import { getAuthState, logout, type AuthState } from "@/services/auth";
import { ShenDeskIpcError } from "@/services/tauri";

function displayNameFrom(state: AuthState): string | null {
  if (!state.authenticated || !state.user) {
    return null;
  }

  return state.user.realname || state.user.username;
}

function App() {
  const [displayName, setDisplayName] = useState<string | null>(null);
  const [isRestoring, setIsRestoring] = useState(true);
  const [isLoggingOut, setIsLoggingOut] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    let cancelled = false;

    void getAuthState()
      .then((state) => {
        if (!cancelled) {
          setDisplayName(displayNameFrom(state));
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

  const handleLogout = async () => {
    setError("");
    setIsLoggingOut(true);
    try {
      const state = await logout();
      setDisplayName(displayNameFrom(state));
    } catch (requestError: unknown) {
      setError(formatAuthError(requestError));
    } finally {
      setIsLoggingOut(false);
    }
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
      {displayName ? (
        <ImageCompression
          displayName={displayName}
          error={error}
          isLoggingOut={isLoggingOut}
          onLogout={() => void handleLogout()}
        />
      ) : (
        <Login onSuccess={setDisplayName} />
      )}
      <Toaster />
    </>
  );
}

function formatAuthError(error: unknown): string {
  if (error instanceof ShenDeskIpcError) {
    return error.message;
  }

  return "认证操作失败，请重试。";
}

export default App;
