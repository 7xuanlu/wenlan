// SPDX-License-Identifier: AGPL-3.0-only
import { File, FilePdf, FileText, Folder, Globe, Image as ImageIcon } from "@phosphor-icons/react";
import obsidianLogo from "../../../assets/obsidian-logo-gradient.svg";
import type { SourceKind } from "./sourcePresentation";

const ICONS = {
  pdf: FilePdf,
  text: FileText,
  image: ImageIcon,
  webpage: Globe,
  file: File,
  folder: Folder,
} satisfies Record<SourceKind | "folder", typeof File>;

export function SourceTypeIcon({
  kind,
  size = 20,
  className,
}: {
  kind: SourceKind | "folder" | "obsidian";
  size?: number;
  className?: string;
}) {
  if (kind === "obsidian") {
    return <img alt="" aria-hidden="true" className={className} data-resource-icon={kind}
      src={obsidianLogo} width={size} height={size} style={{ flexShrink: 0, objectFit: "contain" }} />;
  }
  const Icon = ICONS[kind];
  return <Icon aria-hidden="true" className={className} data-resource-icon={kind} focusable="false" size={size} weight="regular" />;
}
