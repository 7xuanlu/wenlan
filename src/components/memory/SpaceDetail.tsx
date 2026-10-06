// SPDX-License-Identifier: AGPL-3.0-only
import { useEffect, useRef } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  confirmSpace,
  deleteSpace,
  getSpace,
  listPages,
  updateSpace,
  type Space,
} from "../../lib/tauri";
import { SpaceProjectContent } from "./space-detail/SpaceProjectContent";
import { SpaceDossierHeader } from "./space-detail/SpaceDossierHeader";
import { SPACE_DETAIL_KEY_COPY, type SpaceDetailCopy } from "./space-detail/copy";
import {
  PAGE_FETCH_LIMIT,
} from "./space-detail/model";
import "./space-detail/space-detail-header.css";
import "./space-detail/space-detail.css";
import "./pages/pageActions.css";

export type SpaceDetailProps = {
  readonly copy?: SpaceDetailCopy;
  readonly onBack: () => void;
  readonly onSpaceDeleted?: (spaceId: string) => void;
  readonly onSpaceLoaded?: (space: Space) => void;
  readonly onSpaceRenamed?: (space: Pick<Space, "id" | "name">) => void;
  readonly onReviewAll?: () => void;
  readonly onCreatePage?: (space: string) => void;
  readonly onSelectPage: (pageId: string) => void;
  readonly spaceName: string;
};

export default function SpaceDetail({
  copy = SPACE_DETAIL_KEY_COPY,
  onBack,
  onSpaceDeleted,
  onSpaceLoaded,
  onSpaceRenamed,
  onReviewAll,
  onCreatePage,
  onSelectPage,
  spaceName,
}: SpaceDetailProps) {
  const queryClient = useQueryClient();
  const spaceQuery = useQuery({
    queryKey: ["space", spaceName],
    queryFn: () => getSpace(spaceName),
    refetchInterval: 5_000,
  });
  const pagesQuery = useQuery({
    queryKey: ["space-pages", spaceName],
    queryFn: () => listPages("active", spaceName, PAGE_FETCH_LIMIT),
    refetchInterval: 10_000,
  });
  const reportedSpaceIdRef = useRef<string | null>(null);

  useEffect(() => {
    const space = spaceQuery.data;
    if (!space || reportedSpaceIdRef.current === space.id) return;
    reportedSpaceIdRef.current = space.id;
    onSpaceLoaded?.(space);
  }, [onSpaceLoaded, spaceQuery.data]);

  const invalidateSpaceQueries = () => {
    queryClient.invalidateQueries({ queryKey: ["space", spaceName] });
    queryClient.invalidateQueries({ queryKey: ["spaces"] });
    queryClient.invalidateQueries({ queryKey: ["space-memories", spaceName] });
    queryClient.invalidateQueries({ queryKey: ["space-entities", spaceName] });
    queryClient.invalidateQueries({ queryKey: ["space-pages", spaceName] });
    queryClient.invalidateQueries({ queryKey: ["memories"] });
  };

  const renameMutation = useMutation({
    mutationFn: ({ newName, description }: { readonly newName: string; readonly description?: string }) =>
      updateSpace(spaceName, newName, description),
    onSuccess: (updatedSpace, variables) => {
      invalidateSpaceQueries();
      if (variables.newName !== spaceName) {
        onSpaceRenamed?.({ id: updatedSpace.id, name: updatedSpace.name });
        onBack();
      }
    },
  });
  const deleteMutation = useMutation({
    mutationFn: () => deleteSpace(spaceName),
    onSuccess: () => {
      invalidateSpaceQueries();
      if (spaceQuery.data) onSpaceDeleted?.(spaceQuery.data.id);
      onBack();
    },
  });
  const confirmMutation = useMutation({
    mutationFn: () => confirmSpace(spaceName),
    onSuccess: invalidateSpaceQueries,
  });

  if (spaceQuery.isPending) {
    return <p className="space-dossier-state" role="status">{copy.loading}</p>;
  }
  if (spaceQuery.isError) {
    return <p className="space-dossier-state" role="alert">{copy.loadError}</p>;
  }
  if (!spaceQuery.data) {
    return <p className="space-dossier-state">{copy.notFound}</p>;
  }

  const space = spaceQuery.data;
  const pages = pagesQuery.data ?? [];
  const mutationError = renameMutation.isError || deleteMutation.isError || confirmMutation.isError;

  return (
    <article className="space-dossier">
      <SpaceDossierHeader
        key={space.id}
        actions={{
          onBack,
          onDelete: () => deleteMutation.mutate(),
          onKeep: () => confirmMutation.mutate(),
          onReviewAll,
          onCreatePage: () => onCreatePage?.(space.name),
          onSaveIdentity: ({ name, description }) => renameMutation.mutate({
            newName: name,
            description,
          }),
        }}
        copy={copy}
        mutationError={mutationError}
        space={space}
      />

      <SpaceProjectContent
        key={`${space.id}:${space.name}`}
        copy={copy}
        spaceId={space.id}
        spaceName={space.name}
        pages={pages}
        pagesPending={pagesQuery.isPending}
        pagesError={pagesQuery.isError}
        onRetryPages={() => void pagesQuery.refetch()}
        onSelectPage={onSelectPage}
      />
    </article>
  );
}
