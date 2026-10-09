// SPDX-License-Identifier: AGPL-3.0-only
export type NoteTabDrop = {
  readonly sourceGroup: "primary" | "secondary";
  readonly targetGroup: "primary" | "secondary";
  readonly tabKey: string;
  readonly beforeKey: string | null;
};
