#!/usr/bin/env node
"use strict";

const { spawn } = require("child_process");
const { join } = require("path");

const bin = join(__dirname, "bin", "origin-mcp");
const child = spawn(bin, process.argv.slice(2), { stdio: "inherit" });

child.on("exit", (code) => process.exit(code ?? 1));
process.on("SIGTERM", () => child.kill("SIGTERM"));
process.on("SIGINT", () => child.kill("SIGINT"));
