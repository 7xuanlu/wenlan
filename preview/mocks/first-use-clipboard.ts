// SPDX-License-Identifier: AGPL-3.0-only
export async function writeText(text: string): Promise<void> { await navigator.clipboard.writeText(text); }
