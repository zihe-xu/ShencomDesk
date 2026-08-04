import { useState } from "react";
import {
  ImageIcon,
  LogOut,
  Monitor,
  Moon,
  Settings,
  ShieldCheck,
  Sun,
} from "lucide-react";
import { Link } from "react-router-dom";

import logoUrl from "../../app-icons/logo-macos.svg";
import { Button, buttonVariants } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import type { ThemePreference } from "@/services/config";
import { cn } from "@/lib/utils";

interface WorkspaceProps {
  displayName: string;
  error?: string;
  isLoggingOut?: boolean;
  isSavingTheme?: boolean;
  onLogout: () => void;
  onThemeChange: (theme: ThemePreference) => void;
  theme: ThemePreference;
}

const THEME_OPTIONS = [
  { value: "system", label: "跟随系统", icon: Monitor },
  { value: "light", label: "亮色", icon: Sun },
  { value: "dark", label: "暗色", icon: Moon },
] satisfies Array<{
  value: ThemePreference;
  label: string;
  icon: typeof Monitor;
}>;

export function Workspace({
  displayName,
  error,
  isLoggingOut = false,
  isSavingTheme = false,
  onLogout,
  onThemeChange,
  theme,
}: WorkspaceProps) {
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  return (
    <main className="min-h-screen bg-background px-5 py-6 text-foreground sm:px-8 sm:py-8">
      <div className="mx-auto max-w-5xl space-y-8">
        <header className="flex flex-col gap-5 border-b border-border pb-6 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex min-w-0 items-center gap-3">
            <img
              alt="ShenDesk"
              className="size-12 flex-shrink-0"
              height="48"
              src={logoUrl}
              width="48"
            />
            <div className="min-w-0">
              <p className="text-sm font-medium tracking-wide text-muted-foreground">
                ShenDesk
              </p>
              <h1
                className="rounded-sm text-xl font-semibold tracking-tight focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                data-route-heading
                tabIndex={-1}
              >
                工作台
              </h1>
            </div>
          </div>

          <div className="flex min-w-0 flex-col gap-3 sm:items-end">
            <p className="truncate text-sm text-muted-foreground">
              当前用户：
              <span className="font-medium text-foreground">{displayName}</span>
            </p>
            <div className="flex flex-wrap gap-2">
              <Button
                aria-controls="workspace-appearance-settings"
                aria-expanded={isSettingsOpen}
                className="h-11 touch-manipulation gap-2"
                disabled={isLoggingOut}
                onClick={() => setIsSettingsOpen((open) => !open)}
                type="button"
                variant="outline"
              >
                <Settings aria-hidden="true" className="size-4" />
                外观设置
              </Button>
              <Button
                className="h-11 touch-manipulation gap-2"
                disabled={isLoggingOut}
                onClick={onLogout}
                type="button"
                variant="outline"
              >
                {isLoggingOut ? (
                  <span
                    aria-hidden="true"
                    className="size-4 rounded-full border-2 border-muted-foreground/40 border-t-foreground motion-safe:animate-spin"
                  />
                ) : (
                  <LogOut aria-hidden="true" className="size-4" />
                )}
                {isLoggingOut ? "正在退出…" : "退出登录"}
              </Button>
            </div>
          </div>
        </header>

        {error && (
          <p
            className="rounded-lg border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900 dark:bg-red-950/50 dark:text-red-300"
            role="alert"
          >
            {error}
          </p>
        )}

        {isSettingsOpen && (
          <Card id="workspace-appearance-settings">
            <CardHeader>
              <CardTitle>外观设置</CardTitle>
              <CardDescription>
                选择应用主题。跟随系统时会自动响应系统外观变化。
              </CardDescription>
            </CardHeader>
            <CardContent>
              <div
                aria-label="主题"
                className="grid gap-3 sm:grid-cols-3"
                role="group"
              >
                {THEME_OPTIONS.map((option) => {
                  const Icon = option.icon;
                  const selected = theme === option.value;
                  return (
                    <Button
                      aria-pressed={selected}
                      className="min-h-11 touch-manipulation gap-2"
                      disabled={isSavingTheme || isLoggingOut}
                      key={option.value}
                      onClick={() => onThemeChange(option.value)}
                      size="lg"
                      variant={selected ? "default" : "outline"}
                    >
                      <Icon aria-hidden="true" className="size-4" />
                      {option.label}
                    </Button>
                  );
                })}
              </div>
              <p
                aria-live="polite"
                className="mt-3 min-h-5 text-sm text-muted-foreground"
              >
                {isSavingTheme ? "正在保存主题设置…" : ""}
              </p>
            </CardContent>
          </Card>
        )}

        <section aria-labelledby="workspace-welcome-title" className="py-2">
          <h2
            className="break-words text-3xl font-semibold tracking-tight text-balance sm:text-4xl"
            id="workspace-welcome-title"
          >
            欢迎回来，{displayName}
          </h2>
          <p className="mt-3 text-base leading-7 text-muted-foreground">
            选择一个工具开始工作。
          </p>
        </section>

        <section aria-labelledby="local-tools-title" className="space-y-4">
          <div>
            <h2 className="text-lg font-semibold" id="local-tools-title">
              本地工具
            </h2>
            <p className="mt-1 text-sm text-muted-foreground">
              文件处理在您的设备上完成。
            </p>
          </div>

          <Card className="max-w-xl transition-colors duration-200 motion-reduce:transition-none hover:border-foreground/25">
            <CardHeader className="gap-4">
              <div className="flex size-12 items-center justify-center rounded-xl bg-primary text-primary-foreground">
                <ImageIcon aria-hidden="true" className="size-6" />
              </div>
              <div className="space-y-2">
                <CardTitle className="text-xl">图片压缩</CardTitle>
                <CardDescription className="text-sm leading-6">
                  批量压缩 PNG 和 JPEG 图片，支持 PNG 无损优化和 JPEG
                  质量调节。
                </CardDescription>
              </div>
            </CardHeader>
            <CardContent className="space-y-5">
              <p className="flex items-center gap-2 text-sm text-muted-foreground">
                <ShieldCheck aria-hidden="true" className="size-4 flex-none" />
                所有图片仅在本机处理
              </p>
              <Link
                className={cn(
                  buttonVariants({ size: "lg" }),
                  "h-11 w-full touch-manipulation gap-2 sm:w-auto",
                )}
                to="/tools/image-compression"
              >
                <ImageIcon aria-hidden="true" className="size-4" />
                打开工具
              </Link>
            </CardContent>
          </Card>
        </section>
      </div>
    </main>
  );
}
