import { availableParallelism } from 'node:os';
import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import tailwindcss from '@tailwindcss/vite';

export default defineConfig({
  ...(process.env.WENLAN_VITE_CACHE_DIR ? { cacheDir: process.env.WENLAN_VITE_CACHE_DIR } : {}),
  plugins: [react(), tailwindcss()],
  define: {
    __WENLAN_REVIEW__: "false",
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    include: ['src/**/*.test.{ts,tsx}', 'scripts/**/*.test.ts', 'preview/**/*.test.ts'],
    passWithNoTests: true,
    // The suite includes several jsdom-heavy CodeMirror and graph files.
    // Vitest otherwise uses every available core in run mode, which can
    // starve their async DOM work on developer laptops and Windows runners.
    // 4 is a cap, not a target. A 3-vCPU macos-14 CI runner gets Vitest's own
    // default of cores - 1 = 2. Four workers on 3 vCPUs oversubscribe the CPU:
    // in a local simulation that roughly doubled per-test time with no
    // wall-clock gain, which pushed CPU-heavy tests past the 5 s timeout.
    maxWorkers: Math.max(1, Math.min(4, availableParallelism() - 1)),
    coverage: {
      provider: 'v8',
      reporter: ['text', 'html', 'lcov'],
      include: [
        // Only enforce coverage on modules that have tests.
        // Expand this list as component tests are added.
        'src/lib/tauri.ts',
        'src/lib/graph/**',
        'src/hooks/useSearch.ts',
      ],
      exclude: [
        'src/test/**',
      ],
      thresholds: {
        lines: 90,
        functions: 90,
        branches: 85,
        statements: 90,
      },
    },
  },
});
