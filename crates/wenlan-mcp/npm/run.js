#!/usr/bin/env node
"use strict";

const { spawn } = require("child_process");
const { join } = require("path");

const binaryName = process.platform === "win32" ? "wenlan-mcp.exe" : "wenlan-mcp";
const bin = join(__dirname, "bin", binaryName);
const child = spawn(bin, process.argv.slice(2), { stdio: "inherit" });

child.on("exit", (code) => process.exit(code ?? 1));
process.on("SIGTERM", () => child.kill("SIGTERM"));
process.on("SIGINT", () => child.kill("SIGINT"));
