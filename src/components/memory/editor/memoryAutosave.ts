// SPDX-License-Identifier: AGPL-3.0-only

export interface MemoryAutosaveRecord {
  source_id: string;
  content: string;
}

export interface MemoryAutosaveBaseline {
  id: string;
  content: string;
}

export interface MemoryAutosaveSnapshot {
  baseline: MemoryAutosaveBaseline | null;
  source: string;
  phase: "idle" | "saving" | "error" | "conflict";
  dirty: boolean;
  validation: "empty" | null;
  latest: MemoryAutosaveRecord | null;
}

interface MemoryAutosaveOptions {
  read(id: string): Promise<MemoryAutosaveRecord | null>;
  write(id: string, content: string, expectedContent: string): Promise<void>;
  onChange(snapshot: MemoryAutosaveSnapshot): void;
  onCanonical(record: MemoryAutosaveRecord): void;
  delayMs?: number;
}

/** Serial, read-before-write autosaving for memory source text. */
export class MemoryAutosave {
  private baseline: MemoryAutosaveBaseline | null = null;
  private source = "";
  private phase: MemoryAutosaveSnapshot["phase"] = "idle";
  private validation: "empty" | null = null;
  private latest: MemoryAutosaveRecord | null = null;
  private uncertain: { id: string; content: string } | null = null;
  private composing = false;
  private timer: ReturnType<typeof setTimeout> | null = null;
  private running: Promise<boolean> | null = null;
  private generation = 0;
  private disposed = false;

  constructor(private options: MemoryAutosaveOptions) {}

  setObserver(onChange: MemoryAutosaveOptions["onChange"]): void {
    this.options.onChange = onChange;
  }

  get snapshot(): MemoryAutosaveSnapshot {
    return {
      baseline: this.baseline ? { ...this.baseline } : null,
      source: this.source,
      phase: this.phase,
      dirty: this.baseline !== null && (
        this.source !== this.baseline.content || this.phase !== "idle" || this.uncertain !== null
      ),
      validation: this.validation,
      latest: this.latest ? { ...this.latest } : null,
    };
  }

  reset(record: MemoryAutosaveRecord | null): void {
    this.cancelTimer();
    this.generation += 1;
    this.running = null;
    this.disposed = false;
    this.baseline = record ? { id: record.source_id, content: record.content } : null;
    this.source = record?.content ?? "";
    this.phase = "idle";
    this.validation = null;
    this.latest = null;
    this.uncertain = null;
    this.composing = false;
    this.emit();
  }

  dispose(): void {
    this.cancelTimer();
    this.generation += 1;
    this.running = null;
    this.disposed = true;
  }

  setSource(source: string): void {
    if (this.disposed || !this.baseline || source === this.source) return;
    this.source = source;
    this.validation = null;
    this.emit();
    this.schedule();
  }

  setComposing(composing: boolean): void {
    if (this.disposed) return;
    this.composing = composing;
    if (composing) this.cancelTimer();
    else this.schedule();
  }

  observeCanonical(record: MemoryAutosaveRecord | null): void {
    if (this.disposed || !this.baseline) return;
    if (this.phase === "saving") return; // The readback after the write owns reconciliation.
    if (record === null) {
      if (this.phase === "idle" && this.source === this.baseline.content && this.uncertain === null) {
        this.reset(null);
        return;
      }
      this.markConflict(null);
      return;
    }
    if (record.source_id !== this.baseline.id) return;
    if (this.phase === "conflict") {
      this.latest = { ...record };
      this.emit();
      return;
    }
    if (this.phase === "error" && this.uncertain?.content === record.content) {
      // An ambiguous write may already be visible. Only explicit retry reconciles it.
      return;
    }
    if (record.content === this.baseline.content) return;
    if (this.phase === "idle" && this.source === this.baseline.content) {
      this.baseline = { id: record.source_id, content: record.content };
      this.source = record.content;
      this.latest = null;
      this.validation = null;
      this.emit();
      return;
    }
    this.markConflict(record);
  }

  flush(): Promise<boolean> {
    this.cancelTimer();
    if (this.disposed || this.composing) return Promise.resolve(false);
    if (this.running) return this.running;
    if (this.phase !== "idle") return Promise.resolve(false);
    return this.start((generation) => this.drain(generation));
  }

  retry(): Promise<boolean> {
    this.cancelTimer();
    if (this.disposed || this.composing || this.phase !== "error" || !this.uncertain) return Promise.resolve(false);
    if (this.running) return this.running;
    return this.start((generation) => this.retryUncertain(generation));
  }

  private start(work: (generation: number) => Promise<boolean>): Promise<boolean> {
    const generation = this.generation;
    const run = work(generation).finally(() => {
      if (this.running === run) {
        this.running = null;
        if (generation === this.generation) this.schedule();
      }
    });
    this.running = run;
    return run;
  }

  private async drain(generation: number): Promise<boolean> {
    while (generation === this.generation && this.baseline) {
      if (this.composing || this.phase !== "idle") return false;
      if (this.source === this.baseline.content) return true;
      const submitted = this.source;
      if (submitted.trim().length === 0) {
        this.validation = "empty";
        this.emit();
        return false;
      }
      const baseline = this.baseline;
      this.phase = "saving";
      this.validation = null;
      this.emit();

      let before: MemoryAutosaveRecord | null;
      try {
        before = await this.options.read(baseline.id);
      } catch {
        return this.fail(generation, baseline.id, submitted);
      }
      if (generation !== this.generation) return false;
      if (!before || before.source_id !== baseline.id) {
        this.markConflict(before && before.source_id === baseline.id ? before : null);
        return false;
      }
      if (before.content !== baseline.content) {
        this.markConflict(before);
        return false;
      }
      if (this.composing) {
        this.phase = "idle";
        this.emit();
        return false;
      }

      try {
        await this.options.write(baseline.id, submitted, baseline.content);
      } catch {
        return this.fail(generation, baseline.id, submitted);
      }
      if (generation !== this.generation) return false;

      let after: MemoryAutosaveRecord | null;
      try {
        after = await this.options.read(baseline.id);
      } catch {
        return this.fail(generation, baseline.id, submitted);
      }
      if (generation !== this.generation) return false;
      if (!after || after.source_id !== baseline.id || after.content !== submitted) {
        this.markConflict(after && after.source_id === baseline.id ? after : null);
        return false;
      }
      this.acceptCanonical(after);
    }
    return generation === this.generation;
  }

  private async retryUncertain(generation: number): Promise<boolean> {
    const pending = this.uncertain;
    const baseline = this.baseline;
    if (!pending || !baseline || pending.id !== baseline.id) return false;

    let canonical: MemoryAutosaveRecord | null;
    try {
      canonical = await this.options.read(pending.id);
    } catch {
      return this.fail(generation, pending.id, pending.content);
    }
    if (generation !== this.generation) return false;
    if (!canonical || canonical.source_id !== pending.id) {
      this.markConflict(null);
      return false;
    }
    if (canonical.content === pending.content) {
      this.acceptCanonical(canonical);
      return this.drain(generation);
    }
    if (canonical.content !== baseline.content) {
      this.markConflict(canonical);
      return false;
    }
    if (this.composing) {
      this.phase = "idle";
      this.uncertain = null;
      this.emit();
      return false;
    }

    this.phase = "idle";
    this.uncertain = null;
    this.emit();
    // drain performs another fresh preflight immediately before the explicit replay.
    return this.drainReplay(generation, pending.content);
  }

  private async drainReplay(generation: number, submitted: string): Promise<boolean> {
    const baseline = this.baseline;
    if (!baseline || generation !== this.generation || this.composing) return false;
    if (submitted.trim().length === 0) {
      this.phase = "idle";
      this.validation = "empty";
      this.emit();
      return false;
    }
    this.phase = "saving";
    this.emit();
    let before: MemoryAutosaveRecord | null;
    try {
      before = await this.options.read(baseline.id);
    } catch {
      return this.fail(generation, baseline.id, submitted);
    }
    if (generation !== this.generation) return false;
    if (!before || before.source_id !== baseline.id) {
      this.markConflict(null);
      return false;
    }
    if (before.content !== baseline.content) {
      this.markConflict(before);
      return false;
    }
    if (this.composing) {
      this.phase = "idle";
      this.emit();
      return false;
    }
    try {
      await this.options.write(baseline.id, submitted, baseline.content);
    } catch {
      return this.fail(generation, baseline.id, submitted);
    }
    if (generation !== this.generation) return false;
    let after: MemoryAutosaveRecord | null;
    try {
      after = await this.options.read(baseline.id);
    } catch {
      return this.fail(generation, baseline.id, submitted);
    }
    if (generation !== this.generation) return false;
    if (!after || after.source_id !== baseline.id || after.content !== submitted) {
      this.markConflict(after && after.source_id === baseline.id ? after : null);
      return false;
    }
    this.acceptCanonical(after);
    return this.drain(generation);
  }

  private acceptCanonical(record: MemoryAutosaveRecord): void {
    this.baseline = { id: record.source_id, content: record.content };
    this.phase = "idle";
    this.uncertain = null;
    this.latest = null;
    this.validation = null;
    this.emit();
    this.options.onCanonical({ ...record });
  }

  private fail(generation: number, id: string, content: string): false {
    if (generation !== this.generation) return false;
    this.phase = "error";
    this.uncertain = { id, content };
    this.emit();
    return false;
  }

  private markConflict(record: MemoryAutosaveRecord | null): void {
    this.cancelTimer();
    this.phase = "conflict";
    this.latest = record ? { ...record } : null;
    this.emit();
  }

  private schedule(): void {
    this.cancelTimer();
    if (
      this.disposed || this.composing || this.running || this.validation ||
      this.phase !== "idle" || !this.snapshot.dirty
    ) return;
    this.timer = setTimeout(() => {
      this.timer = null;
      void this.flush();
    }, this.options.delayMs ?? 600);
  }

  private cancelTimer(): void {
    if (this.timer !== null) clearTimeout(this.timer);
    this.timer = null;
  }

  private emit(): void { this.options.onChange(this.snapshot); }
}
