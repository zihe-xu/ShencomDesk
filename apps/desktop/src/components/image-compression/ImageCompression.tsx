import { useCallback, useEffect, useState } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { open } from "@tauri-apps/plugin-dialog";
import {
  AlertCircle,
  CheckCircle2,
  CircleDashed,
  FolderOpen,
  ImageIcon,
  MinusCircle,
  Trash2,
  UploadCloud,
} from "lucide-react";
import { toast } from "sonner";

import { Button } from "@/components/ui/button";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Progress } from "@/components/ui/progress";
import { Slider } from "@/components/ui/slider";
import {
  compressImages,
  type CompressImagesResult,
  type CompressionProgress,
  type CompressionStatus,
} from "@/services/image";
import { ShenDeskIpcError } from "@/services/tauri";

type FileStatus = "pending" | CompressionStatus;

interface SelectedImage {
  path: string;
  name: string;
  status: FileStatus;
  originalBytes: number;
  compressedBytes: number;
  error: string | null;
}

interface ImageCompressionProps {
  displayName: string;
  error?: string;
  isLoggingOut?: boolean;
  onLogout?: () => void;
}

const IMAGE_FILTERS = [
  {
    name: "PNG 和 JPEG 图片",
    extensions: ["png", "jpg", "jpeg"],
  },
];

export function ImageCompression({
  displayName,
  error,
  isLoggingOut = false,
  onLogout,
}: ImageCompressionProps) {
  const [items, setItems] = useState<SelectedImage[]>([]);
  const [quality, setQuality] = useState(75);
  const [outputDir, setOutputDir] = useState("");
  const [isCompressing, setIsCompressing] = useState(false);
  const [isDragging, setIsDragging] = useState(false);
  const [result, setResult] = useState<CompressImagesResult | null>(null);

  const addFiles = useCallback(
    (paths: string[]) => {
      if (isCompressing) {
        return;
      }

      const accepted = paths.filter(isSupportedImage);
      const rejected = paths.length - accepted.length;
      if (rejected > 0) {
        toast.error(`已忽略 ${rejected} 个不支持的文件，仅支持 PNG 和 JPEG。`);
      }

      setItems((current) => {
        const known = new Set(current.map((item) => item.path));
        const additions = accepted
          .filter((path) => {
            if (known.has(path)) {
              return false;
            }
            known.add(path);
            return true;
          })
          .map(createSelectedImage);
        return [...current.map(resetSelectedImage), ...additions];
      });
      setResult(null);
    },
    [isCompressing],
  );

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    void getCurrentWebview()
      .onDragDropEvent((event) => {
        if (event.payload.type === "drop") {
          setIsDragging(false);
          addFiles(event.payload.paths);
        } else if (
          event.payload.type === "enter" ||
          event.payload.type === "over"
        ) {
          setIsDragging(true);
        } else {
          setIsDragging(false);
        }
      })
      .then((stopListening) => {
        if (disposed) {
          stopListening();
        } else {
          unlisten = stopListening;
        }
      })
      .catch(() => {
        if (!disposed) {
          toast.error("无法启用图片拖拽，请使用文件选择器。");
        }
      });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [addFiles]);

  const terminalCount = items.filter(
    (item) =>
      item.status === "completed" ||
      item.status === "skipped" ||
      item.status === "failed",
  ).length;
  const progressValue =
    items.length === 0 ? 0 : Math.round((terminalCount / items.length) * 100);

  const chooseImages = async () => {
    try {
      const selected = await open({
        directory: false,
        filters: IMAGE_FILTERS,
        multiple: true,
        title: "选择要压缩的图片",
      });
      if (Array.isArray(selected)) {
        addFiles(selected);
      } else if (selected) {
        addFiles([selected]);
      }
    } catch {
      toast.error("无法打开图片选择器，请重试。");
    }
  };

  const chooseOutputDirectory = async () => {
    try {
      const selected = await open({
        directory: true,
        multiple: false,
        title: "选择压缩图片输出目录",
      });
      if (typeof selected === "string") {
        setOutputDir(selected);
        setItems((current) => current.map(resetSelectedImage));
        setResult(null);
      }
    } catch {
      toast.error("无法打开目录选择器，请重试。");
    }
  };

  const removeItem = (path: string) => {
    setItems((current) =>
      current
        .filter((item) => item.path !== path)
        .map(resetSelectedImage),
    );
    setResult(null);
  };

  const startCompression = async () => {
    if (items.length === 0 || !outputDir || isCompressing) {
      return;
    }

    setItems((current) => current.map(resetSelectedImage));
    setResult(null);
    setIsCompressing(true);

    try {
      const summary = await compressImages(
        {
          items: items.map((item) => item.path),
          outputDir,
          quality,
        },
        updateItemProgress,
      );
      setResult(summary);
      if (summary.failed > 0) {
        toast.error(
          `处理完成：${summary.failed} 张失败，请查看文件列表中的原因。`,
        );
      } else {
        toast.success("全部图片处理完成。");
      }
    } catch (error: unknown) {
      toast.error(formatCompressionError(error));
    } finally {
      setIsCompressing(false);
    }
  };

  const updateItemProgress = (progress: CompressionProgress) => {
    setItems((current) =>
      current.map((item, index) =>
        index === progress.index - 1
          ? {
              ...item,
              name: progress.fileName,
              status: progress.status,
              originalBytes: progress.originalBytes,
              compressedBytes: progress.compressedBytes,
              error: progress.error,
            }
          : item,
      ),
    );
  };

  return (
    <main className="min-h-screen bg-background px-5 py-8 text-foreground sm:px-8">
      <div className="mx-auto max-w-5xl space-y-6">
        <header className="flex flex-col gap-2 sm:flex-row sm:items-end sm:justify-between">
          <div>
            <p className="text-sm font-medium tracking-wide text-muted-foreground">
              ShenDesk · 本地工具
            </p>
            <h1 className="mt-1 text-3xl font-semibold tracking-tight">
              图片压缩
            </h1>
            <p className="mt-2 text-base text-muted-foreground">
              PNG 无损优化，JPEG 可调质量。所有图片仅在本机处理。
            </p>
          </div>
          <div className="flex flex-col items-start gap-2 sm:items-end">
            <p className="text-sm text-muted-foreground">
              欢迎回来，
              <span className="font-medium text-foreground">{displayName}</span>
            </p>
            {onLogout && (
              <Button
                disabled={isLoggingOut || isCompressing}
                onClick={onLogout}
                size="sm"
                type="button"
                variant="outline"
              >
                {isLoggingOut ? "正在退出…" : "退出登录"}
              </Button>
            )}
            {error && (
              <p className="text-sm text-red-600" role="alert">
                {error}
              </p>
            )}
          </div>
        </header>

        <Card>
          <CardContent className="p-6">
            <button
              className={`flex min-h-48 w-full flex-col items-center justify-center rounded-xl border-2 border-dashed px-6 text-center outline-none transition-colors duration-200 focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-60 ${
                isDragging
                  ? "border-primary bg-primary/5"
                  : "border-border bg-muted/35 hover:border-primary/60 hover:bg-muted/60"
              }`}
              disabled={isCompressing}
              onClick={() => void chooseImages()}
              type="button"
            >
              <span className="mb-4 flex size-12 items-center justify-center rounded-full bg-primary text-primary-foreground">
                <UploadCloud aria-hidden="true" className="size-6" />
              </span>
              <span className="text-base font-medium">
                拖入图片，或点击选择文件
              </span>
              <span className="mt-2 text-sm text-muted-foreground">
                支持批量选择 PNG、JPG 和 JPEG
              </span>
            </button>
          </CardContent>
        </Card>

        <div className="grid gap-6 lg:grid-cols-[minmax(0,1fr)_20rem]">
          <Card>
            <CardHeader className="flex-row items-start justify-between space-y-0">
              <div>
                <CardTitle>待处理图片</CardTitle>
                <CardDescription className="mt-1.5">
                  已选择 {items.length} 张图片
                </CardDescription>
              </div>
              {items.length > 0 && (
                <Button
                  disabled={isCompressing}
                  onClick={() => {
                    setItems([]);
                    setResult(null);
                  }}
                  size="sm"
                  variant="ghost"
                >
                  清空
                </Button>
              )}
            </CardHeader>
            <CardContent>
              {items.length === 0 ? (
                <div className="flex min-h-40 flex-col items-center justify-center rounded-lg border border-dashed border-border text-center">
                  <ImageIcon
                    aria-hidden="true"
                    className="mb-3 size-7 text-muted-foreground"
                  />
                  <p className="text-sm text-muted-foreground">
                    选择图片后将在这里显示处理状态
                  </p>
                </div>
              ) : (
                <ul className="space-y-2" aria-live="polite">
                  {items.map((item) => (
                    <li
                      className="flex min-w-0 items-center gap-3 rounded-lg border border-border px-3 py-3"
                      key={item.path}
                    >
                      <StatusIcon status={item.status} />
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-sm font-medium">{item.name}</p>
                        <p className="mt-1 text-xs text-muted-foreground">
                          {statusText(item)}
                        </p>
                        {item.error && (
                          <p className="mt-1 text-xs text-red-600">{item.error}</p>
                        )}
                      </div>
                      <Button
                        aria-label={`移除 ${item.name}`}
                        className="size-11 p-0"
                        disabled={isCompressing}
                        onClick={() => removeItem(item.path)}
                        size="sm"
                        variant="ghost"
                      >
                        <Trash2 aria-hidden="true" className="size-4" />
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
            </CardContent>
          </Card>

          <div className="space-y-6">
            <Card>
              <CardHeader>
                <CardTitle>压缩设置</CardTitle>
                <CardDescription>
                  质量只影响 JPEG，PNG 始终执行无损优化。
                </CardDescription>
              </CardHeader>
              <CardContent className="space-y-6">
                <div className="space-y-3">
                  <div className="flex items-center justify-between">
                    <label className="text-sm font-medium" htmlFor="quality">
                      JPEG 质量
                    </label>
                    <span className="text-sm font-semibold tabular-nums">
                      {quality}
                    </span>
                  </div>
                  <Slider
                    aria-label="JPEG 压缩质量"
                    disabled={isCompressing}
                    id="quality"
                    max={100}
                    min={1}
                    onValueChange={([value]) => {
                      setQuality(value);
                      setItems((current) => current.map(resetSelectedImage));
                      setResult(null);
                    }}
                    step={1}
                    value={[quality]}
                  />
                </div>

                <div className="space-y-2">
                  <span className="text-sm font-medium">输出目录</span>
                  <Button
                    className="h-11 w-full justify-start gap-2"
                    disabled={isCompressing}
                    onClick={() => void chooseOutputDirectory()}
                    variant="outline"
                  >
                    <FolderOpen aria-hidden="true" className="size-4 shrink-0" />
                    <span className="truncate">
                      {outputDir || "选择输出目录"}
                    </span>
                  </Button>
                  <p className="text-xs leading-5 text-muted-foreground">
                    不会覆盖任何已有文件；同名文件将标记为失败。
                  </p>
                </div>

                <Button
                  className="h-11 w-full gap-2"
                  disabled={
                    isCompressing || items.length === 0 || outputDir.length === 0
                  }
                  onClick={() => void startCompression()}
                  size="lg"
                >
                  {isCompressing && (
                    <span
                      aria-hidden="true"
                      className="size-4 rounded-full border-2 border-primary-foreground/40 border-t-primary-foreground motion-safe:animate-spin"
                    />
                  )}
                  {isCompressing ? "正在压缩…" : "开始压缩"}
                </Button>
              </CardContent>
            </Card>

            {(isCompressing || terminalCount > 0) && (
              <Card aria-live="polite">
                <CardHeader>
                  <CardTitle>处理进度</CardTitle>
                  <CardDescription>
                    已处理 {terminalCount} / {items.length}
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-2">
                  <Progress
                    aria-label="图片处理进度"
                    value={progressValue}
                  />
                  <p className="text-right text-xs font-medium tabular-nums text-muted-foreground">
                    {progressValue}%
                  </p>
                </CardContent>
              </Card>
            )}

            {result && (
              <Card aria-live="polite">
                <CardHeader>
                  <CardTitle>处理汇总</CardTitle>
                  <CardDescription>
                    产物已写入所选输出目录
                  </CardDescription>
                </CardHeader>
                <CardContent className="grid grid-cols-3 gap-3 text-center">
                  <SummaryValue label="已压缩" value={result.succeeded} />
                  <SummaryValue label="已跳过" value={result.skipped} />
                  <SummaryValue label="失败" value={result.failed} />
                  <div className="col-span-3 rounded-lg bg-muted px-3 py-3 text-sm">
                    共节省{" "}
                    <span className="font-semibold">
                      {formatBytes(
                        Math.max(
                          0,
                          result.totalOriginalBytes -
                            result.totalCompressedBytes,
                        ),
                      )}
                    </span>
                  </div>
                </CardContent>
              </Card>
            )}
          </div>
        </div>
      </div>
    </main>
  );
}

function createSelectedImage(path: string): SelectedImage {
  return {
    path,
    name: path.split(/[\\/]/).pop() || path,
    status: "pending",
    originalBytes: 0,
    compressedBytes: 0,
    error: null,
  };
}

function resetSelectedImage(item: SelectedImage): SelectedImage {
  return {
    ...item,
    status: "pending",
    originalBytes: 0,
    compressedBytes: 0,
    error: null,
  };
}

function isSupportedImage(path: string): boolean {
  return /\.(png|jpe?g)$/i.test(path);
}

function statusText(item: SelectedImage): string {
  switch (item.status) {
    case "processing":
      return "正在处理";
    case "completed":
      return `${formatBytes(item.originalBytes)} → ${formatBytes(item.compressedBytes)}`;
    case "skipped":
      return `未获得更小结果，已复制原图（${formatBytes(item.originalBytes)}）`;
    case "failed":
      return "处理失败";
    default:
      return "等待处理";
  }
}

function StatusIcon({ status }: { status: FileStatus }) {
  const className = "size-5 shrink-0";
  switch (status) {
    case "processing":
      return (
        <CircleDashed
          aria-label="正在处理"
          className={`${className} motion-safe:animate-spin text-blue-600`}
        />
      );
    case "completed":
      return (
        <CheckCircle2
          aria-label="压缩完成"
          className={`${className} text-emerald-600`}
        />
      );
    case "skipped":
      return (
        <MinusCircle
          aria-label="已跳过"
          className={`${className} text-amber-600`}
        />
      );
    case "failed":
      return (
        <AlertCircle
          aria-label="处理失败"
          className={`${className} text-red-600`}
        />
      );
    default:
      return (
        <ImageIcon
          aria-label="等待处理"
          className={`${className} text-muted-foreground`}
        />
      );
  }
}

function SummaryValue({ label, value }: { label: string; value: number }) {
  return (
    <div className="rounded-lg bg-muted px-2 py-3">
      <p className="text-lg font-semibold tabular-nums">{value}</p>
      <p className="mt-1 text-xs text-muted-foreground">{label}</p>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) {
    return `${bytes} B`;
  }
  const units = ["KB", "MB", "GB"];
  let value = bytes / 1024;
  let unit = units[0];
  for (let index = 1; index < units.length && value >= 1024; index += 1) {
    value /= 1024;
    unit = units[index];
  }
  return `${value.toFixed(value >= 10 ? 1 : 2)} ${unit}`;
}

function formatCompressionError(error: unknown): string {
  if (error instanceof ShenDeskIpcError) {
    return error.message;
  }
  return "图片处理失败，请重试。";
}
