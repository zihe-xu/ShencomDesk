import { Button } from "@/components/ui/button";

const capabilities = [
  "Tauri 2 desktop shell",
  "React 19 + TypeScript",
  "Vite 8 development workflow",
  "Tailwind CSS 4 styling",
  "shadcn/ui component foundation",
];

function App() {
  return (
    <main className="min-h-screen bg-background px-6 py-10 text-foreground">
      <section className="mx-auto flex min-h-[calc(100vh-5rem)] max-w-5xl items-center">
        <div className="grid w-full gap-10 rounded-3xl border border-border bg-card p-8 shadow-2xl shadow-slate-950/10 md:grid-cols-[1.2fr_0.8fr] md:p-12">
          <div className="space-y-6">
            <div className="inline-flex rounded-full border border-border bg-muted px-3 py-1 text-xs font-medium text-muted-foreground">
              Shencom Desktop Platform
            </div>
            <div className="space-y-3">
              <h1 className="text-4xl font-semibold tracking-tight sm:text-5xl">ShenDesk</h1>
              <p className="max-w-xl text-base leading-7 text-muted-foreground sm:text-lg">
                本地优先、跨平台、面向长期演进的桌面应用基础工程。
              </p>
            </div>
            <div className="flex flex-wrap gap-3">
              <Button>开始构建</Button>
              <Button variant="outline">查看架构</Button>
            </div>
          </div>

          <div className="rounded-2xl border border-border bg-muted/50 p-6">
            <h2 className="text-sm font-semibold uppercase tracking-[0.18em] text-muted-foreground">
              Issue #2 Baseline
            </h2>
            <ul className="mt-5 space-y-3">
              {capabilities.map((capability) => (
                <li key={capability} className="flex items-center gap-3 text-sm">
                  <span className="size-2 rounded-full bg-primary" aria-hidden="true" />
                  <span>{capability}</span>
                </li>
              ))}
            </ul>
          </div>
        </div>
      </section>
    </main>
  );
}

export default App;
