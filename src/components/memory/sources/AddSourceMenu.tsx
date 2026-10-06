// SPDX-License-Identifier: AGPL-3.0-only
import { useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { toast } from "sonner";
import {
  uploadSourceFile,
  getDaemonVersion,
  daemonMeetsFloor,
  openFile,
} from "../../../lib/tauri";
import AddSourceDialog from "./AddSourceDialog";
import AddWebSourceDialog from "./AddWebSourceDialog";
import SourceDialog from "./SourceDialog";
import { useTranslation } from "react-i18next";

export default function AddSourceMenu({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const qc = useQueryClient();
  const [showFolder, setShowFolder] = useState(false);
  const [showWeb, setShowWeb] = useState(false);

  const { data: version } = useQuery({ queryKey: ["daemonVersion"], queryFn: getDaemonVersion });
  // Optimistic until the version is known, so the menu never flickers a warning.
  const ready = version === undefined ? true : daemonMeetsFloor(version);

  const upload = useMutation({
    mutationFn: async () => {
      const picked = await openDialog({
        directory: false,
        multiple: false,
        filters: [{ name: "Documents", extensions: ["pdf", "md", "txt"] }],
      });
      if (!picked || typeof picked !== "string") return null;
      const name = picked.split("/").pop() ?? "file";
      await uploadSourceFile(picked);
      // Toast idiom: eyebrow (title) / heading + body (description).
      toast(t("sourceAccess.added"), { description: t("sourceAccess.addedBody", { name }) });
      return name;
    },
    onSuccess: (name) => {
      if (!name) return;
      qc.invalidateQueries({ queryKey: ["registeredSources"] });
      // The uploaded file lands in the managed dir; refresh its on-disk listing
      // and the index cross-ref so the new file appears immediately instead of
      // waiting for a window-focus refetch.
      qc.invalidateQueries({ queryKey: ["sourceDir"] });
      qc.invalidateQueries({ queryKey: ["indexedFiles"] });
      onClose();
    },
    onError: (e) => {
      // Without this the button silently reverts to "Add files" and the user
      // never learns the upload failed (daemon down / IO error / bad file).
      toast(t("sourceAccess.addFailed"), { description: String(e) });
    },
  });

  if (showFolder) {
    return <AddSourceDialog onClose={onClose} onSuccess={onClose} />;
  }

  if (showWeb) return <AddWebSourceDialog onClose={onClose} />;
  return <SourceDialog title={t("sourceAccess.addTitle")} onClose={onClose}>
    {!ready && <div role="status"><p>{t("sourceAccess.upgrade")}</p><button className="page-editor-action" type="button" onClick={() => openFile("https://wenlan.app")}>{t("sourceAccess.update")}</button></div>}
    <button type="button" className="source-add-option" disabled={upload.isPending} onClick={() => upload.mutate()}>
      <strong>{t(upload.isPending ? "sourceAccess.adding" : "sourceAccess.files")}</strong><span>{t("sourceAccess.filesHint")}</span>
    </button>
    <button type="button" className="source-add-option" onClick={() => setShowFolder(true)}><strong>{t("sourceAccess.folder")}</strong><span>{t("sourceAccess.folderHint")}</span></button>
    <button type="button" className="source-add-option" onClick={() => setShowWeb(true)}><strong>{t("sourceAccess.web")}</strong><span>{t("sourceAccess.webHint")}</span></button>
  </SourceDialog>;
}
