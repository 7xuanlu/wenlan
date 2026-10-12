// SPDX-License-Identifier: AGPL-3.0-only
import type { ReactNode } from "react";
import { useUnobstructedContentWidth } from "./useUnobstructedContentWidth";
import "./collectionLayout.css";

/** Browse items using the collection geometry shared by Spaces, Memories and Topics. */
export function CollectionFrame({ children, className }: { readonly children: ReactNode; readonly className?: string }) {
  const frameRef = useUnobstructedContentWidth("--collection-unobstructed-width", true);
  return <div ref={frameRef} className={`collection-frame${className ? ` ${className}` : ""}`} data-collection-layout>
    {children}
  </div>;
}
