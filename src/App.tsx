import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";

function App() {
  const [greetMsg, setGreetMsg] = useState("");
  const [name, setName] = useState("");

  async function greet() {
    // Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
    setGreetMsg(await invoke("greet", { name }));
  }

  return (
    <main className="flex min-h-screen items-center justify-center px-6 py-12">
      <div className="w-full max-w-xl space-y-8 rounded-2xl border border-border/70 bg-card/80 p-8 shadow-2xl shadow-cyan-900/30 backdrop-blur">
        <div className="space-y-3">
          <div className="inline-flex items-center gap-2 rounded-full bg-primary/10 px-3 py-1 text-sm font-semibold text-primary-foreground/90 ring-1 ring-primary/30">
            <span className="h-2 w-2 animate-pulse rounded-full bg-primary" aria-hidden />
            Tauri notifier
          </div>
          <h1 className="text-3xl font-semibold text-foreground sm:text-4xl">Welcome to your desktop helper</h1>
          <p className="text-muted-foreground">
            Send a quick greeting from the Rust backend to make sure everything is wired up. Tailwind + shadcn/ui are now
            ready for reuse.
          </p>
        </div>

        <form
          className="space-y-4"
          onSubmit={(e) => {
            e.preventDefault();
            greet();
          }}
        >
          <div className="space-y-2">
            <Label htmlFor="greet-input">Name</Label>
            <Input
              id="greet-input"
              value={name}
              onChange={(e) => setName(e.currentTarget.value)}
              placeholder="Ada Lovelace"
            />
          </div>
          <Button type="submit" className="w-full">
            Greet from Rust
          </Button>
        </form>

        {greetMsg && (
          <div className="rounded-lg border border-border/60 bg-white/5 px-4 py-3 text-sm text-foreground shadow-sm shadow-black/20">
            {greetMsg}
          </div>
        )}
      </div>
    </main>
  );
}

export default App;
